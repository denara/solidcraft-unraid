//! Read-only previews and short-lived, cookie-bound handoffs to independent browser CAD.
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, ReadBuf};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

use crate::sessions::State as AppState;

const DESIGN_LIMIT: u64 = 256 << 20;
const PNG_LIMIT: u64 = 8 << 20;
const PIXELS: usize = 4096 * 4096;

// Keep the memory allowance until transmission finishes or the client leaves,
// rather than releasing it while a slow response still owns a large buffer.
struct ImportBody {
    bytes: Cursor<Vec<u8>>,
    _permit: OwnedSemaphorePermit,
}

impl AsyncRead for ImportBody {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buffer: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.bytes).poll_read(cx, buffer)
    }
}

struct Ticket {
    owner: String,
    name: String,
    revision: String,
    expires: Instant,
}

pub(crate) struct Media {
    tickets: Mutex<HashMap<String, Ticket>>,
    reads: Arc<Semaphore>,
}

impl Default for Media {
    fn default() -> Self {
        Self { tickets: Mutex::new(HashMap::new()), reads: Arc::new(Semaphore::new(2)) }
    }
}

impl Media {
    #[cfg(test)]
    pub(crate) fn available_reads(&self) -> usize {
        self.reads.available_permits()
    }

    #[cfg(test)]
    pub(crate) async fn expire_all(&self) {
        for ticket in self.tickets.lock().await.values_mut() {
            ticket.expires = Instant::now();
        }
    }
}

fn failure(status: StatusCode) -> Response {
    crate::workspace::protect((status, "Datei nicht verfügbar oder Übergabe abgelaufen.").into_response())
}

fn native(name: &str) -> bool {
    crate::workspace_files::native_model(name)
}

fn read(root: &std::path::Path, name: &str, limit: u64) -> Result<Vec<u8>, StatusCode> {
    if !crate::workspace_files::visible_name(name) {
        return Err(StatusCode::NOT_FOUND);
    }
    let path = crate::paths::resolve(root, name).map_err(|_| StatusCode::NOT_FOUND)?;
    let meta = std::fs::symlink_metadata(&path).map_err(|_| StatusCode::NOT_FOUND)?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(StatusCode::NOT_FOUND);
    }
    if meta.len() > limit {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path).map_err(|_| StatusCode::NOT_FOUND)?.take(limit + 1).read_to_end(&mut bytes).map_err(|_| StatusCode::NOT_FOUND)?;
    if bytes.len() as u64 > limit {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    Ok(bytes)
}

fn png_thumbnail(bytes: &[u8]) -> Result<Vec<u8>, StatusCode> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_limits(png::Limits { bytes: PIXELS * 4 });
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|_| StatusCode::NOT_FOUND)?;
    let info = reader.info();
    if info.width == 0
        || info.height == 0
        || info.width > 4096
        || info.height > 4096
        || info.animation_control.is_some()
        || info.frame_control.is_some()
    {
        return Err(StatusCode::NOT_FOUND);
    }
    let length = reader.output_buffer_size().filter(|size| *size <= PIXELS * 4).ok_or(StatusCode::NOT_FOUND)?;
    let mut decoded = vec![0; length];
    let frame = reader.next_frame(&mut decoded).map_err(|_| StatusCode::NOT_FOUND)?;
    reader.finish().map_err(|_| StatusCode::NOT_FOUND)?;
    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        _ => return Err(StatusCode::NOT_FOUND),
    };
    let width = frame.width as usize;
    let height = frame.height as usize;
    let scale = (160.0 / width as f64).min(120.0 / height as f64).min(1.0);
    let target_width = ((width as f64 * scale) as usize).max(1);
    let target_height = ((height as f64 * scale) as usize).max(1);
    let mut thumbnail = Vec::with_capacity(target_width * target_height * 4);
    for y in 0..target_height {
        for x in 0..target_width {
            let offset = ((y * height / target_height) * width + x * width / target_width) * channels;
            let pixel = decoded.get(offset..offset + channels).ok_or(StatusCode::NOT_FOUND)?;
            let rgba = match pixel {
                [gray] => [*gray, *gray, *gray, 255],
                [gray, alpha] => [*gray, *gray, *gray, *alpha],
                [r, g, b] => [*r, *g, *b, 255],
                [r, g, b, alpha] => [*r, *g, *b, *alpha],
                _ => return Err(StatusCode::NOT_FOUND),
            };
            thumbnail.extend_from_slice(&rgba);
        }
    }
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, target_width as u32, target_height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|_| StatusCode::NOT_FOUND)?;
        writer.write_image_data(&thumbnail).map_err(|_| StatusCode::NOT_FOUND)?;
        writer.finish().map_err(|_| StatusCode::NOT_FOUND)?;
    }
    Ok(encoded)
}

pub(crate) async fn preview(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(name): Path<String>) -> Response {
    if crate::workspace::browser_session(&state, &headers).await.is_none() {
        return failure(StatusCode::UNAUTHORIZED);
    }
    if !crate::workspace_live::source_allowed(&state, &headers) {
        return failure(StatusCode::FORBIDDEN);
    }
    if !name.to_ascii_lowercase().ends_with(".png") {
        return failure(StatusCode::NOT_FOUND);
    }
    let Ok(Ok(_permit)) = tokio::time::timeout(Duration::from_secs(5), Arc::clone(&state.browser.media.reads).acquire_owned()).await else {
        return failure(StatusCode::SERVICE_UNAVAILABLE);
    };
    let root = state.config.files.clone();
    match tokio::task::spawn_blocking(move || png_thumbnail(&read(&root, &name, PNG_LIMIT)?)).await {
        Ok(Ok(bytes)) => crate::workspace::protect(([(header::CONTENT_TYPE, "image/png")], bytes).into_response()),
        Ok(Err(status)) => failure(status),
        Err(_) => failure(StatusCode::SERVICE_UNAVAILABLE),
    }
}

pub(crate) async fn open(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(name): Path<String>) -> Response {
    let Some(owner) = crate::workspace::browser_session(&state, &headers).await else {
        return crate::workspace::protect((StatusCode::SEE_OTHER, [(header::LOCATION, "/workspace/login")]).into_response());
    };
    if !crate::workspace_live::source_allowed(&state, &headers) {
        return failure(StatusCode::FORBIDDEN);
    }
    if !native(&name) || !crate::workspace_files::visible_name(&name) {
        return failure(StatusCode::NOT_FOUND);
    }
    let root = state.config.files.clone();
    let check_name = name.clone();
    let _gate = state.file_gate.lock().await;
    let revision = match tokio::task::spawn_blocking(move || -> Result<String, StatusCode> {
        let path = crate::paths::resolve(&root, &check_name).map_err(|_| StatusCode::NOT_FOUND)?;
        let meta = std::fs::metadata(path).map_err(|_| StatusCode::NOT_FOUND)?;
        if meta.len() > DESIGN_LIMIT {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
        crate::workspace_files::revision(&root, &check_name).map_err(|_| StatusCode::NOT_FOUND)?.ok_or(StatusCode::NOT_FOUND)
    })
    .await
    {
        Ok(Ok(value)) => value,
        Ok(Err(status)) => return failure(status),
        Err(_) => return failure(StatusCode::SERVICE_UNAVAILABLE),
    };
    let mut tickets = state.browser.media.tickets.lock().await;
    tickets.retain(|_, ticket| ticket.expires > Instant::now());
    if tickets.len() >= 32 || tickets.values().filter(|ticket| ticket.owner == owner).count() >= 4 {
        return failure(StatusCode::TOO_MANY_REQUESTS);
    }
    let id = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    tickets.insert(id.clone(), Ticket { owner, name, revision, expires: Instant::now() + Duration::from_secs(120) });
    let location = format!("/#workspace-import={id}");
    crate::workspace::protect((StatusCode::SEE_OTHER, [(header::LOCATION, location)]).into_response())
}

fn encoded_basename(name: &str) -> String {
    use std::fmt::Write as _;
    let mut encoded = String::new();
    for byte in name.rsplit('/').next().unwrap_or_default().bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

pub(crate) async fn import(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let Some(owner) = crate::workspace::browser_session(&state, &headers).await else { return failure(StatusCode::UNAUTHORIZED) };
    if !crate::workspace_live::source_allowed(&state, &headers) {
        return failure(StatusCode::FORBIDDEN);
    }
    if id.len() != 64 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return failure(StatusCode::NOT_FOUND);
    }
    let ticket = {
        let mut tickets = state.browser.media.tickets.lock().await;
        tickets.retain(|_, ticket| ticket.expires > Instant::now());
        if tickets.get(&id).is_none_or(|ticket| ticket.owner != owner) {
            return failure(StatusCode::NOT_FOUND);
        }
        let Some(ticket) = tickets.remove(&id) else { return failure(StatusCode::NOT_FOUND) };
        ticket
    };
    let Ok(Ok(_permit)) = tokio::time::timeout(Duration::from_secs(5), Arc::clone(&state.browser.media.reads).acquire_owned()).await else {
        return failure(StatusCode::SERVICE_UNAVAILABLE);
    };
    let _gate = state.file_gate.lock().await;
    let root: PathBuf = state.config.files.clone();
    let name = ticket.name.clone();
    let bytes = match tokio::task::spawn_blocking(move || read(&root, &name, DESIGN_LIMIT)).await {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(status)) => return failure(status),
        Err(_) => return failure(StatusCode::SERVICE_UNAVAILABLE),
    };
    if format!("\"{:x}\"", Sha256::digest(&bytes)) != ticket.revision {
        return failure(StatusCode::CONFLICT);
    }
    let length = bytes.len();
    let body = Body::from_stream(tokio_util::io::ReaderStream::new(ImportBody { bytes: Cursor::new(bytes), _permit }));
    let mut response = crate::workspace::protect(
        ([(header::CONTENT_TYPE, "application/octet-stream"), (header::CONTENT_DISPOSITION, "attachment")], body).into_response(),
    );
    if let Ok(filename) = HeaderValue::from_str(&encoded_basename(&ticket.name)) {
        response.headers_mut().insert("x-solidcraft-filename", filename);
    }
    if let Ok(length) = HeaderValue::from_str(&length.to_string()) {
        response.headers_mut().insert(header::CONTENT_LENGTH, length);
    }
    response
}
