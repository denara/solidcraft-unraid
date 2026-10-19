//! Bounded cookie-only observation of the live, unsaved headless model.
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::task::{Context, Poll};
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncWriteExt, DuplexStream, ReadBuf};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot, watch};
use tokio_util::io::ReaderStream;

use crate::sessions::{State as AppState, Worker};

const MAX_PNG: usize = 2 << 20;
const CACHE_BYTES: usize = 32 << 20;
const INTERVAL: Duration = Duration::from_secs(2);

pub struct Hub {
    render: Arc<Semaphore>,
    memory: Arc<Semaphore>,
    images: Arc<Semaphore>,
}

impl Default for Hub {
    fn default() -> Self {
        Self { render: Arc::new(Semaphore::new(1)), memory: Arc::new(Semaphore::new(CACHE_BYTES)), images: Arc::new(Semaphore::new(4)) }
    }
}

fn reserve_frame(hub: &Hub, observation: &Observation) -> Option<OwnedSemaphorePermit> {
    if let Ok(permit) = Arc::clone(&hub.memory).try_acquire_many_owned(MAX_PNG as u32) {
        return Some(permit);
    }
    // At capacity a session must be able to replace its own old frame. Existing
    // HTTP responses retain their Arc and memory lease until their body drops;
    // they are never copied or exempted from the global budget. The browser can
    // keep showing its already decoded previous frame while this one is built.
    if let Ok(mut frame) = observation.frame.lock() {
        *frame = None;
    }
    Arc::clone(&hub.memory).try_acquire_many_owned(MAX_PNG as u32).ok()
}

struct Frame {
    bytes: Vec<u8>,
    revision: u64,
    _memory: OwnedSemaphorePermit,
}

pub struct Observation {
    pub handle: String,
    pub cad_requested: AtomicBool,
    pub exchanging: AtomicBool,
    ended: AtomicBool,
    viewers: AtomicUsize,
    running: Mutex<bool>,
    frame: Mutex<Option<Arc<Frame>>>,
    updates: watch::Sender<Value>,
}

impl Default for Observation {
    fn default() -> Self {
        Self {
            handle: uuid::Uuid::new_v4().to_string(),
            cad_requested: AtomicBool::new(false),
            exchanging: AtomicBool::new(false),
            ended: AtomicBool::new(false),
            viewers: AtomicUsize::new(0),
            running: Mutex::new(false),
            frame: Mutex::new(None),
            updates: watch::channel(json!({"status":"waiting","revision":0})).0,
        }
    }
}

impl Observation {
    pub fn end(&self) {
        self.ended.store(true, Ordering::Release);
        if let Ok(mut frame) = self.frame.lock() {
            *frame = None;
        }
        self.updates.send_replace(json!({"status":"ended","revision":0}));
    }

    fn status(&self, status: &str) {
        let Ok(_frame) = self.frame.lock() else { return };
        if self.ended.load(Ordering::Acquire) {
            return;
        }
        self.updates.send_if_modified(|value| {
            if self.ended.load(Ordering::Acquire) {
                return false;
            }
            if value.get("status").and_then(Value::as_str) == Some(status) {
                return false;
            }
            value["status"] = json!(status);
            true
        });
    }
}

/// Distinguish real concurrent MCP requests from the short observer protocol.
pub struct CadActivity {
    observer: Arc<Observation>,
}
impl CadActivity {
    pub fn acquire(observer: Arc<Observation>) -> Option<Self> {
        observer.cad_requested.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).ok()?;
        Some(Self { observer })
    }
}
impl Drop for CadActivity {
    fn drop(&mut self) {
        self.observer.cad_requested.store(false, Ordering::Release);
    }
}

struct Viewer(Arc<Observation>);
impl Drop for Viewer {
    fn drop(&mut self) {
        if self.0.viewers.fetch_sub(1, Ordering::AcqRel) == 1
            && let Ok(mut frame) = self.0.frame.lock()
            && self.0.viewers.load(Ordering::Acquire) == 0
        {
            *frame = None;
            self.0.updates.send_if_modified(|value| {
                if self.0.ended.load(Ordering::Acquire) {
                    return false;
                }
                *value = json!({"status":"waiting","revision":0});
                true
            });
        }
    }
}

async fn resolve(state: &AppState, handle: &str) -> Option<(Arc<Observation>, Arc<tokio::sync::Mutex<Worker>>)> {
    if uuid::Uuid::parse_str(handle).is_err() {
        return None;
    }
    state.expire().await;
    state
        .sessions
        .lock()
        .await
        .values()
        .find(|entry| entry.observer.handle == handle)
        .map(|entry| (Arc::clone(&entry.observer), Arc::clone(&entry.worker)))
}

pub async fn events(State(state): State<Arc<AppState>>, Path(handle): Path<String>, headers: HeaderMap) -> Response {
    let Some(browser) = crate::workspace::browser_session(&state, &headers).await else {
        return crate::workspace::protect(StatusCode::NO_CONTENT.into_response());
    };
    if !crate::workspace_live::source_allowed(&state, &headers) {
        return crate::workspace::protect(StatusCode::FORBIDDEN.into_response());
    }
    let Some((observation, worker)) = resolve(&state, &handle).await else {
        return crate::workspace::protect(StatusCode::NOT_FOUND.into_response());
    };
    let lease = match state.browser.live.reserve(browser) {
        Ok(lease) => lease,
        Err(status) => return crate::workspace::protect(status.into_response()),
    };
    let receiver = observation.updates.subscribe();
    observation.viewers.fetch_add(1, Ordering::AcqRel);
    let viewer = Viewer(Arc::clone(&observation));
    if let Ok(mut running) = observation.running.lock()
        && !*running
    {
        *running = true;
        tokio::spawn(monitor(Arc::clone(&state), Arc::clone(&observation), Arc::downgrade(&worker)));
    }
    let (writer, reader) = tokio::io::duplex(8192);
    let (disconnect, disconnected) = oneshot::channel();
    let reader = Reader { inner: reader, disconnected: Some(disconnect) };
    tokio::spawn(async move {
        let _lease = lease;
        let _viewer = viewer;
        let mut stopping = state.browser.live.stopping();
        if *stopping.borrow() {
            return;
        }
        tokio::select! {
            _ = stopping.changed() => {},
            _ = disconnected => {},
            _ = finish_on_revocation(
                stream(&state, &headers, receiver, writer),
                revoked(&state, &headers),
                Duration::from_secs(2),
            ) => {}
        }
    });
    let mut response = crate::workspace::protect(Body::from_stream(ReaderStream::new(reader)).into_response());
    response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("text/event-stream; charset=utf-8"));
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store, no-transform"));
    response.headers_mut().insert("x-accel-buffering", HeaderValue::from_static("no"));
    response
}

struct Reader {
    inner: DuplexStream,
    disconnected: Option<oneshot::Sender<()>>,
}
impl AsyncRead for Reader {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buffer: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buffer)
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        if let Some(disconnected) = self.disconnected.take() {
            let _ = disconnected.send(());
        }
    }
}

async fn send(writer: &mut DuplexStream, frame: &str) -> bool {
    matches!(tokio::time::timeout(Duration::from_secs(5), writer.write_all(frame.as_bytes())).await, Ok(Ok(())))
}

async fn revoked(state: &AppState, headers: &HeaderMap) {
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tick.tick().await;
        if crate::workspace::browser_session(state, headers).await.is_none() {
            return;
        }
    }
}

/// Let the regular writer announce expiry before the watchdog cancels it. The
/// short grace only applies to revoked authentication: disconnect and shutdown
/// still cancel this entire future immediately. A stalled browser cannot retain
/// its subscription indefinitely by preventing the final event from being sent.
async fn finish_on_revocation(delivery: impl Future<Output = ()>, revocation: impl Future<Output = ()>, grace: Duration) {
    tokio::pin!(delivery);
    tokio::select! {
        _ = &mut delivery => {},
        _ = revocation => {
            let _ = tokio::time::timeout(grace, delivery).await;
        }
    }
}

async fn stream(state: &AppState, headers: &HeaderMap, mut receiver: watch::Receiver<Value>, mut writer: DuplexStream) {
    if !send(&mut writer, "retry: 3000\n\n").await {
        return;
    }
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    let mut count = 0u8;
    let mut changed = true;
    loop {
        if crate::workspace::browser_session(state, headers).await.is_none() {
            let _ = send(&mut writer, "event: expired\ndata: {}\n\n").await;
            break;
        }
        if changed {
            changed = false;
            let value = receiver.borrow_and_update().clone();
            if !send(&mut writer, &format!("event: observer\ndata: {value}\n\n")).await {
                break;
            }
            if value.get("status").and_then(Value::as_str) == Some("ended") {
                break;
            }
        }
        tokio::select! {
            result = receiver.changed() => { if result.is_err() { break; } changed = true; },
            _ = tick.tick() => {
                count = count.saturating_add(1);
                if count >= 15 { count = 0; if !send(&mut writer, ": keepalive\n\n").await { break; } }
            }
        }
    }
}

// The response owns both leases until transmission ends, including slow clients.
type AuthorizationCheck = Pin<Box<dyn Future<Output = bool> + Send>>;
struct ImageAuthorization {
    state: Arc<AppState>,
    headers: HeaderMap,
    pending: Option<AuthorizationCheck>,
}
struct ImageBody {
    frame: Arc<Frame>,
    observation: Arc<Observation>,
    authorization: Option<ImageAuthorization>,
    offset: usize,
    _read: OwnedSemaphorePermit,
}
impl AsyncRead for ImageBody {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buffer: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        if self.observation.ended.load(Ordering::Acquire) {
            return Poll::Ready(Ok(()));
        }
        if let Some(authorization) = self.authorization.as_mut() {
            if authorization.pending.is_none() {
                let state = Arc::clone(&authorization.state);
                let headers = authorization.headers.clone();
                authorization.pending = Some(Box::pin(async move { crate::workspace::browser_session(&state, &headers).await.is_some() }));
            }
            let Some(check) = authorization.pending.as_mut() else { return Poll::Ready(Ok(())) };
            match check.as_mut().poll(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(false) => return Poll::Ready(Ok(())),
                Poll::Ready(true) => authorization.pending = None,
            }
        }
        let bytes = self.frame.bytes.get(self.offset..).unwrap_or_default();
        let n = bytes.len().min(buffer.remaining());
        if let Some(bytes) = bytes.get(..n) {
            buffer.put_slice(bytes);
        }
        self.offset = self.offset.saturating_add(n);
        Poll::Ready(Ok(()))
    }
}

pub async fn image(State(state): State<Arc<AppState>>, Path(handle): Path<String>, headers: HeaderMap, uri: Uri) -> Response {
    if crate::workspace::browser_session(&state, &headers).await.is_none() {
        return crate::workspace::protect(StatusCode::UNAUTHORIZED.into_response());
    }
    if !crate::workspace_live::source_allowed(&state, &headers) {
        return crate::workspace::protect(StatusCode::FORBIDDEN.into_response());
    }
    let revision = crate::workspace::fields(uri.query().unwrap_or_default()).and_then(|fields| fields.get("revision")?.parse::<u64>().ok());
    let Some((observation, _)) = resolve(&state, &handle).await else { return crate::workspace::protect(StatusCode::NOT_FOUND.into_response()) };
    let frame = observation.frame.lock().ok().and_then(|frame| frame.as_ref().cloned()).filter(|frame| Some(frame.revision) == revision);
    let Some(frame) = frame.filter(|_| !observation.ended.load(Ordering::Acquire)) else {
        return crate::workspace::protect(StatusCode::NOT_FOUND.into_response());
    };
    let Ok(permit) = Arc::clone(&state.observer.images).try_acquire_owned() else {
        return crate::workspace::protect(StatusCode::TOO_MANY_REQUESTS.into_response());
    };
    let body = ImageBody { frame, observation, authorization: Some(ImageAuthorization { state, headers, pending: None }), offset: 0, _read: permit };
    let mut response = crate::workspace::protect(Body::from_stream(ReaderStream::new(body)).into_response());
    response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    response
}

async fn internal(
    worker: &Arc<tokio::sync::Mutex<Worker>>,
    observation: &Observation,
    action: &str,
    revision: Option<u64>,
) -> Result<Option<Value>, ()> {
    if observation.cad_requested.load(Ordering::Acquire) {
        return Ok(None);
    }
    if action == "start" && (observation.ended.load(Ordering::Acquire) || observation.viewers.load(Ordering::Acquire) == 0) {
        return Ok(None);
    }
    let Ok(mut worker) = worker.try_lock() else { return Ok(None) };
    observation.exchanging.store(true, Ordering::Release);
    if observation.cad_requested.load(Ordering::Acquire) {
        drop(worker);
        observation.exchanging.store(false, Ordering::Release);
        return Ok(None);
    }
    let request = json!({"jsonrpc":"2.0","id":format!("observer-{}",uuid::Uuid::new_v4()),"method":"solidcraft/workspaceSnapshot","params":{"action":action,"revision":revision}});
    let result = worker.exchange(&request).await.map_err(|_| ()).and_then(|value| {
        let value = value.ok_or(())?;
        if value.get("error").is_some() {
            return Ok(json!({"status":"unavailable"}));
        }
        value.get("result").cloned().ok_or(())
    });
    drop(worker);
    observation.exchanging.store(false, Ordering::Release);
    result.map(Some)
}

async fn monitor(state: Arc<AppState>, observation: Arc<Observation>, worker: Weak<tokio::sync::Mutex<Worker>>) {
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut render = None;
    let mut last = tokio::time::Instant::now() - INTERVAL;
    let mut revision = None;
    let mut pending = false;
    let mut disabled = false;
    let mut retry_after = tokio::time::Instant::now();
    loop {
        tick.tick().await;
        if *state.browser.live.stopping().borrow() {
            break;
        }
        if observation.ended.load(Ordering::Acquire) && render.is_none() {
            break;
        }
        let Some(worker) = worker.upgrade() else { break };
        if observation.viewers.load(Ordering::Acquire) == 0 && render.is_none() {
            let Ok(mut running) = observation.running.lock() else { break };
            if observation.viewers.load(Ordering::Acquire) == 0 {
                *running = false;
                return;
            }
        }
        if observation.cad_requested.load(Ordering::Acquire) {
            observation.status("busy");
            continue;
        }
        if disabled {
            observation.status("unavailable");
            continue;
        }
        if render.is_none() {
            if tokio::time::Instant::now() < retry_after {
                continue;
            }
            if last.elapsed() < INTERVAL {
                continue;
            }
            let Ok(permit) = Arc::clone(&state.observer.render).try_acquire_owned() else { continue };
            render = Some(permit);
            last = tokio::time::Instant::now();
        }
        if !pending && observation.frame.lock().ok().is_some_and(|frame| frame.is_none()) {
            revision = None;
        }
        let action = if pending { "poll" } else { "start" };
        let result = internal(&worker, &observation, action, revision).await;
        match result {
            Ok(None) => {
                if action == "start" {
                    render = None;
                }
            }
            // A lost protocol exchange cannot prove that the worker's renderer has
            // stopped. Keep the global CPU lease until that worker actually ends.
            Err(()) => {
                disabled = true;
                observation.status("unavailable");
            }
            Ok(Some(value)) => match value.get("status").and_then(Value::as_str) {
                Some("rendering") => {
                    pending = true;
                    observation.status("rendering");
                }
                Some("unchanged" | "idle") => {
                    pending = false;
                    render = None;
                    if observation.frame.lock().ok().is_some_and(|frame| frame.is_some()) {
                        observation.status("ready");
                    }
                }
                Some("ready") => {
                    render = None;
                    pending = false;
                    if observation.viewers.load(Ordering::Acquire) == 0 || observation.ended.load(Ordering::Acquire) {
                        revision = None;
                        continue;
                    }
                    let Some(memory) = reserve_frame(&state.observer, &observation) else {
                        observation.status("unavailable");
                        continue;
                    };
                    let Some(next) = value.get("revision").and_then(Value::as_u64) else {
                        observation.status("unavailable");
                        continue;
                    };
                    let Some(bytes) = value.get("png").and_then(Value::as_str).and_then(decode) else {
                        observation.status("unavailable");
                        continue;
                    };
                    let update = json!({"status":"ready","revision":next,"image_url":format!("/workspace/observe/{}/image?revision={next}",observation.handle),
                        "captured_at":chrono::Utc::now().to_rfc3339(),"renderer":value.get("renderer"),"warning":value.get("warning")});
                    if let Ok(mut frame) = observation.frame.lock() {
                        if observation.ended.load(Ordering::Acquire) || observation.viewers.load(Ordering::Acquire) == 0 {
                            continue;
                        }
                        *frame = Some(Arc::new(Frame { bytes, revision: next, _memory: memory }));
                        revision = Some(next);
                        observation.updates.send_replace(update);
                    }
                }
                Some("unavailable") => {
                    render = None;
                    pending = false;
                    retry_after = tokio::time::Instant::now() + Duration::from_secs(10);
                    observation.status("unavailable");
                }
                _ => {
                    disabled = true;
                    observation.status("unavailable");
                }
            },
        }
    }
    if let Ok(mut running) = observation.running.lock() {
        *running = false;
    }
}

fn decode(text: &str) -> Option<Vec<u8>> {
    if text.len() > MAX_PNG.div_ceil(3) * 4 || !text.len().is_multiple_of(4) {
        return None;
    }
    let mut bytes = Vec::with_capacity(text.len() / 4 * 3);
    let mut bits = 0u32;
    let mut count = 0u8;
    let mut padding = false;
    for b in text.bytes() {
        if b == b'=' {
            padding = true;
            continue;
        }
        if padding {
            return None;
        }
        let n = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        bits = (bits << 6) | u32::from(n);
        count += 6;
        if count >= 8 {
            count -= 8;
            bytes.push((bits >> count) as u8);
        }
    }
    let dimensions = bytes.get(16..24).and_then(|dimensions| {
        Some((u32::from_be_bytes(dimensions.get(..4)?.try_into().ok()?), u32::from_be_bytes(dimensions.get(4..)?.try_into().ok()?)))
    });
    (bytes.len() <= MAX_PNG && bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.get(12..16) == Some(b"IHDR") && dimensions == Some((800, 500)))
        .then_some(bytes)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn logout_watchdog_allows_the_expired_event_to_finish() {
        use tokio::io::AsyncReadExt as _;
        let (mut writer, mut reader) = tokio::io::duplex(1024);
        let delivery = async move {
            // Auth checks need not wake in the same order. The watchdog must
            // allow the ordinary writer to run even if it detects expiry first.
            tokio::time::sleep(Duration::from_millis(5)).await;
            assert!(send(&mut writer, "event: expired\ndata: {}\n\n").await);
        };
        finish_on_revocation(delivery, std::future::ready(()), Duration::from_secs(1)).await;
        let mut received = String::new();
        reader.read_to_string(&mut received).await.unwrap();
        assert_eq!(received, "event: expired\ndata: {}\n\n");
    }

    #[tokio::test]
    async fn logout_watchdog_bounds_an_expired_event_blocked_by_backpressure() {
        let (mut writer, _unread) = tokio::io::duplex(1);
        let delivery = async move {
            let _ = send(&mut writer, "event: expired\ndata: {}\n\n").await;
        };
        // send() normally waits five seconds. The revocation watchdog must end
        // the subscription sooner even when its terminal event cannot drain.
        assert!(
            tokio::time::timeout(Duration::from_secs(1), finish_on_revocation(delivery, std::future::ready(()), Duration::from_millis(10)),)
                .await
                .is_ok()
        );
    }

    #[test]
    fn observation_handles_are_not_session_credentials_and_end_revokes_cache() {
        let observation = Arc::new(Observation::default());
        let first = CadActivity::acquire(Arc::clone(&observation)).unwrap();
        assert!(CadActivity::acquire(Arc::clone(&observation)).is_none());
        drop(first);
        assert!(CadActivity::acquire(Arc::clone(&observation)).is_some());
        observation.end();
        assert_eq!(observation.updates.borrow()["status"], "ended");
        observation.status("ready");
        assert_eq!(observation.updates.borrow()["status"], "ended");
        assert!(observation.frame.lock().unwrap().is_none());
    }
    #[test]
    fn decode_rejects_non_png_and_oversized_frames() {
        assert!(decode("aGVsbG8=").is_none());
        assert!(decode("iVBORw0KGgo=").is_none());
        assert!(decode("iVBORw0KGgo=AAAA").is_none());
        assert!(decode(&"A".repeat(MAX_PNG * 2)).is_none());
    }
    #[tokio::test]
    async fn image_body_holds_memory_and_read_permits_until_drop() {
        let hub = Hub::default();
        let frame =
            Arc::new(Frame { bytes: vec![0; 16], revision: 1, _memory: Arc::clone(&hub.memory).acquire_many_owned(MAX_PNG as u32).await.unwrap() });
        let body = ImageBody {
            frame: Arc::clone(&frame),
            observation: Arc::new(Observation::default()),
            authorization: None,
            offset: 0,
            _read: Arc::clone(&hub.images).acquire_owned().await.unwrap(),
        };
        drop(frame);
        assert_eq!(hub.memory.available_permits(), CACHE_BYTES - MAX_PNG);
        assert_eq!(hub.images.available_permits(), 3);
        drop(body);
        assert_eq!(hub.memory.available_permits(), CACHE_BYTES);
        assert_eq!(hub.images.available_permits(), 4);
    }
    #[test]
    fn a_late_viewer_drop_cannot_resurrect_an_ended_observer() {
        let observation = Arc::new(Observation::default());
        observation.viewers.store(1, Ordering::Release);
        let viewer = Viewer(Arc::clone(&observation));
        observation.end();
        drop(viewer);
        assert_eq!(observation.updates.borrow()["status"], "ended");
        assert_eq!(observation.viewers.load(Ordering::Acquire), 0);
    }
    #[tokio::test]
    async fn dropping_the_last_viewer_discards_the_shared_image() {
        let hub = Hub::default();
        let observation = Arc::new(Observation::default());
        observation.viewers.store(2, Ordering::Release);
        let memory = Arc::clone(&hub.memory).acquire_many_owned(MAX_PNG as u32).await.unwrap();
        *observation.frame.lock().unwrap() = Some(Arc::new(Frame { bytes: vec![0; 16], revision: 7, _memory: memory }));
        drop(Viewer(Arc::clone(&observation)));
        assert!(observation.frame.lock().unwrap().is_some());
        drop(Viewer(Arc::clone(&observation)));
        assert!(observation.frame.lock().unwrap().is_none());
        assert_eq!(hub.memory.available_permits(), CACHE_BYTES);
        assert_eq!(observation.updates.borrow()["status"], "waiting");
    }
    #[tokio::test]
    async fn a_full_cache_can_replace_its_own_frame_without_exceeding_the_budget() {
        let hub = Hub::default();
        let observation = Observation::default();
        let memory = Arc::clone(&hub.memory).acquire_many_owned(MAX_PNG as u32).await.unwrap();
        *observation.frame.lock().unwrap() = Some(Arc::new(Frame { bytes: vec![0; 16], revision: 1, _memory: memory }));
        let others = Arc::clone(&hub.memory).acquire_many_owned((CACHE_BYTES - MAX_PNG) as u32).await.unwrap();
        assert_eq!(hub.memory.available_permits(), 0);
        let replacement = reserve_frame(&hub, &observation).unwrap();
        assert!(observation.frame.lock().unwrap().is_none());
        assert_eq!(hub.memory.available_permits(), 0);
        *observation.frame.lock().unwrap() = Some(Arc::new(Frame { bytes: vec![1; 16], revision: 2, _memory: replacement }));
        drop(others);
        assert_eq!(hub.memory.available_permits(), CACHE_BYTES - MAX_PNG);
        assert_eq!(observation.frame.lock().unwrap().as_ref().unwrap().revision, 2);
    }
}
