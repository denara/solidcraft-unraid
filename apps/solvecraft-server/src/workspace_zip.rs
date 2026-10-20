//! ZIP64 streaming through two bounded chunks. No complete archive or spool file exists.
use std::future::Future;
use std::io::{self, Read, Write};
use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
};
use std::task::{Context, Poll};
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use tokio::io::{AsyncRead, ReadBuf};
use tokio::sync::{OwnedSemaphorePermit, mpsc};
use tokio_util::io::ReaderStream;
use zip::write::SimpleFileOptions;

use super::{Transfers, manifest};
use crate::sessions::State as AppState;
const CHUNK: usize = 32 * 1024;

struct Lease {
    transfers: Transfers,
    id: String,
    _permit: OwnedSemaphorePermit,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut transfers) = self.transfers.lock() {
            transfers.remove(&self.id);
        }
    }
}

struct Sender {
    channel: mpsc::Sender<Bytes>,
    poisoned: Arc<AtomicBool>,
}
impl Write for Sender {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.poisoned.load(Ordering::Acquire) {
            return Err(io::Error::other("archive cancelled"));
        }
        if bytes.is_empty() {
            return Ok(0);
        }
        let length = bytes.len().min(CHUNK);
        let chunk = Bytes::copy_from_slice(bytes.get(..length).ok_or_else(|| io::Error::other("invalid archive chunk"))?);
        match self.channel.try_send(chunk) {
            Ok(()) => Ok(length),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(io::Error::new(io::ErrorKind::BrokenPipe, "download disconnected")),
            Err(mpsc::error::TrySendError::Full(chunk)) => {
                // This writer runs only in spawn_blocking. Wait for real channel
                // capacity instead of polling/sleeping or buffering more ZIP data.
                let runtime = tokio::runtime::Handle::try_current().map_err(io::Error::other)?;
                match runtime.block_on(async { tokio::time::timeout(Duration::from_secs(15), self.channel.send(chunk)).await }) {
                    Ok(Ok(())) => Ok(length),
                    Ok(Err(_)) => Err(io::Error::new(io::ErrorKind::BrokenPipe, "download disconnected")),
                    Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "download stalled")),
                }
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

type AuthCheck = Pin<Box<dyn Future<Output = bool> + Send>>;
struct Download {
    channel: mpsc::Receiver<Bytes>,
    current: Bytes,
    result: Arc<AtomicU8>, // 0 in progress, 1 complete, 2 failed
    poisoned: Arc<AtomicBool>,
    state: Arc<AppState>,
    headers: HeaderMap,
    authorization: Option<AuthCheck>,
    _lease: Arc<Lease>,
}
impl Drop for Download {
    fn drop(&mut self) {
        self.poisoned.store(true, Ordering::Release);
    }
}
impl AsyncRead for Download {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buffer: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        if buffer.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        if self.authorization.is_none() {
            let state = Arc::clone(&self.state);
            let headers = self.headers.clone();
            self.authorization = Some(Box::pin(async move {
                crate::workspace::browser_session(&state, &headers).await.is_some() && !*state.browser.live.stopping().borrow()
            }));
        }
        if let Some(check) = self.authorization.as_mut() {
            match check.as_mut().poll(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(false) => {
                    self.poisoned.store(true, Ordering::Release);
                    self.channel.close();
                    return Poll::Ready(Err(io::Error::other("browser authorization ended")));
                }
                Poll::Ready(true) => self.authorization = None,
            }
        }
        if self.current.is_empty() {
            match self.channel.poll_recv(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(bytes)) => self.current = bytes,
                Poll::Ready(None) => {
                    return Poll::Ready(if self.result.load(Ordering::Acquire) == 1 {
                        Ok(())
                    } else {
                        Err(io::Error::other("archive interrupted or files changed; retry download"))
                    });
                }
            }
        }
        let length = buffer.remaining().min(self.current.len());
        buffer.put_slice(&self.current.split_to(length));
        Poll::Ready(Ok(()))
    }
}

fn create(root: &std::path::Path, name: &str, manifest: &manifest::Manifest, sender: Sender) -> io::Result<()> {
    let poison = Arc::clone(&sender.poisoned);
    let mut zip = zip::ZipWriter::new_stream(sender);
    let result = (|| -> io::Result<()> {
        let prefix = name.rsplit_once('/').map(|(parent, _)| parent.len() + 1).unwrap_or(0);
        let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored).large_file(true).unix_permissions(0o644);
        let mut buffer = vec![0u8; CHUNK];
        for entry in &manifest.entries {
            let path = crate::paths::resolve(root, &entry.name).map_err(io::Error::other)?;
            let meta = std::fs::symlink_metadata(&path)?;
            if manifest::stamp(&meta) != entry.stamp || meta.file_type().is_symlink() {
                return Err(io::Error::other("archive source changed"));
            }
            let relative = entry.name.get(prefix..).ok_or_else(|| io::Error::other("invalid archive path"))?;
            if entry.directory {
                zip.add_directory(format!("{relative}/"), options.unix_permissions(0o755))?;
                continue;
            }
            let mut file = std::fs::File::open(&path)?;
            if manifest::stamp(&file.metadata()?) != entry.stamp {
                return Err(io::Error::other("archive source replaced"));
            }
            zip.start_file(relative, options)?;
            let mut remaining = entry.bytes;
            while remaining > 0 {
                let count = usize::try_from(remaining.min(CHUNK as u64)).map_err(io::Error::other)?;
                let read = file.read(buffer.get_mut(..count).ok_or_else(|| io::Error::other("invalid archive buffer"))?)?;
                if read == 0 {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "archive source shortened"));
                }
                zip.write_all(buffer.get(..read).ok_or_else(|| io::Error::other("invalid archive buffer"))?)?;
                remaining -= read as u64;
            }
            if manifest::stamp(&file.metadata()?) != entry.stamp {
                return Err(io::Error::other("archive source changed"));
            }
        }
        // Recheck the entire tree before writing the central directory. Gateway writes
        // are excluded by a path lease, external changes cause an HTTP body error.
        if manifest::inspect(root, name, false).map_err(io::Error::other)? != *manifest {
            return Err(io::Error::other("archive source changed"));
        }
        Ok(())
    })();
    if result.is_err() {
        poison.store(true, Ordering::Release);
        return result;
    }
    zip.finish()?;
    Ok(())
}

pub(crate) async fn download(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(name): Path<String>) -> Response {
    if crate::workspace::browser_session(&state, &headers).await.is_none() {
        return super::failure(StatusCode::UNAUTHORIZED, "Bitte erneut anmelden.");
    }
    if !crate::workspace_live::source_allowed(&state, &headers) {
        return super::failure(StatusCode::FORBIDDEN, "Diese Herkunft ist nicht freigegeben.");
    }
    let Ok(permit) = Arc::clone(&state.browser.actions.slots).try_acquire_owned() else {
        return super::failure(StatusCode::TOO_MANY_REQUESTS, "Maximal zwei ZIP-Downloads gleichzeitig. Bitte nach Abschluss erneut versuchen.");
    };
    let gate = state.file_gate.lock().await;
    let root = state.config.files.clone();
    let target = name.clone();
    let manifest = match tokio::task::spawn_blocking(move || manifest::inspect(&root, &target, false)).await {
        Ok(Ok(manifest)) if manifest.entries.first().is_some_and(|entry| entry.directory) => manifest,
        Ok(Err(error)) => return super::failure(StatusCode::CONFLICT, error),
        _ => return super::failure(StatusCode::BAD_REQUEST, "ZIP-Downloads sind nur für echte Ordner verfügbar."),
    };
    let id = uuid::Uuid::new_v4().to_string();
    let transfers = Arc::clone(&state.browser.actions.transfers);
    let Ok(mut active) = transfers.lock() else { return super::failure(StatusCode::SERVICE_UNAVAILABLE, "Dateizugriff nicht verfügbar.") };
    active.insert(id.clone(), name.clone());
    drop(active);
    let lease = Arc::new(Lease { transfers, id, _permit: permit });
    drop(gate);
    let (sender, receiver) = mpsc::channel(2);
    let result = Arc::new(AtomicU8::new(0));
    let poisoned = Arc::new(AtomicBool::new(false));
    let status = Arc::clone(&result);
    let poison = Arc::clone(&poisoned);
    let root = state.config.files.clone();
    let target = name.clone();
    let worker_lease = Arc::clone(&lease);
    tokio::task::spawn_blocking(move || {
        let _lease = worker_lease;
        // Retain one sender until the status is published, so EOF cannot race success.
        let completion = sender.clone();
        let outcome = create(&root, &target, &manifest, Sender { channel: sender, poisoned: Arc::clone(&poison) });
        if let Ok(mut transfers) = _lease.transfers.lock() {
            transfers.remove(&_lease.id);
        }
        status.store(
            if outcome.is_ok() {
                1
            } else {
                poison.store(true, Ordering::Release);
                2
            },
            Ordering::Release,
        );
        drop(completion);
    });
    let reader = Download { channel: receiver, current: Bytes::new(), result, poisoned, state, headers, authorization: None, _lease: lease };
    let mut response = crate::workspace::protect(Body::from_stream(ReaderStream::with_capacity(reader, CHUNK)).into_response());
    response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/zip"));
    response.headers_mut().insert("x-accel-buffering", HeaderValue::from_static("no"));
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store, no-transform"));
    let filename = format!("{}.zip", name.rsplit('/').next().unwrap_or("project"));
    let encoded: String = filename.bytes().map(|byte| format!("%{byte:02X}")).collect();
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"project.zip\"; filename*=UTF-8''{encoded}")) {
        response.headers_mut().insert(header::CONTENT_DISPOSITION, value);
    }
    response
}
