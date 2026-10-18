//! Cookie-authenticated, bounded server events for the read-only workspace.
//!
//! One monitor inventories files while browsers are subscribed. Slow clients retain
//! one snapshot rather than a queue, and never hold a CAD worker or file lock.
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncWriteExt, DuplexStream, ReadBuf};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot, watch};
use tokio_util::io::ReaderStream;

use crate::sessions::State as AppState;

const MAX_STREAMS: usize = 32;
const MAX_BROWSER_STREAMS: usize = 4;
const POLL: Duration = Duration::from_secs(1);
const KEEPALIVE: Duration = Duration::from_secs(15);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub(crate) enum Update {
    Pending,
    Snapshot(Arc<Value>),
    Unavailable,
}

pub struct Live {
    slots: Arc<Semaphore>,
    browsers: Arc<Mutex<HashMap<String, usize>>>,
    channel: tokio::sync::Mutex<Option<watch::Sender<Update>>>,
    stopping: watch::Sender<bool>,
}

impl Default for Live {
    fn default() -> Self {
        Self {
            slots: Arc::new(Semaphore::new(MAX_STREAMS)),
            browsers: Arc::new(Mutex::new(HashMap::new())),
            channel: tokio::sync::Mutex::new(None),
            stopping: watch::channel(false).0,
        }
    }
}

struct Lease {
    _slot: OwnedSemaphorePermit,
    browsers: Arc<Mutex<HashMap<String, usize>>>,
    browser: String,
}

/// Dropping the HTTP body immediately ends its task and releases its stream slot.
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

impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut browsers) = self.browsers.lock()
            && let Some(count) = browsers.get_mut(&self.browser)
        {
            *count = count.saturating_sub(1);
            if *count == 0 {
                browsers.remove(&self.browser);
            }
        }
    }
}

impl Live {
    /// End long-lived responses before axum waits for graceful HTTP shutdown.
    /// Closing the transport preserves authentication and lets browsers reconnect.
    pub(crate) fn stop(&self) {
        self.stopping.send_replace(true);
    }

    #[cfg(test)]
    pub(crate) fn streams_used(&self) -> usize {
        MAX_STREAMS.saturating_sub(self.slots.available_permits())
    }

    fn reserve(&self, browser: String) -> Result<Lease, StatusCode> {
        if *self.stopping.borrow() {
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
        let slot = Arc::clone(&self.slots).try_acquire_owned().map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
        let mut browsers = self.browsers.lock().map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        let count = browsers.entry(browser.clone()).or_default();
        if *count >= MAX_BROWSER_STREAMS {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
        *count += 1;
        Ok(Lease { _slot: slot, browsers: Arc::clone(&self.browsers), browser })
    }

    pub(crate) async fn subscribe(&self, state: &Arc<AppState>) -> watch::Receiver<Update> {
        let mut channel = self.channel.lock().await;
        if let Some(sender) = channel.as_ref().filter(|sender| !sender.is_closed()) {
            return sender.subscribe();
        }
        let (sender, receiver) = watch::channel(Update::Pending);
        *channel = Some(sender.clone());
        tokio::spawn(monitor(Arc::clone(state), sender));
        receiver
    }
}

/// Elapsed display labels must not turn an idle workspace into a change event.
fn meaningful(mut snapshot: Value) -> Value {
    if let Some(sessions) = snapshot.get_mut("sessions").and_then(Value::as_array_mut) {
        for session in sessions {
            if let Some(row) = session.as_object_mut() {
                row.remove("last_request_seconds");
                row.remove("last_model_change_seconds");
            }
        }
    }
    snapshot
}

async fn monitor(state: Arc<AppState>, sender: watch::Sender<Update>) {
    let mut stopping = state.browser.live.stopping.subscribe();
    let mut tick = tokio::time::interval(POLL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut previous = None;
    let mut unavailable = false;
    loop {
        if *stopping.borrow() {
            break;
        }
        tokio::select! {
            _ = stopping.changed() => break,
            _ = sender.closed() => break,
            _ = tick.tick() => {}
        }
        if sender.is_closed() {
            break;
        }
        let snapshot = tokio::select! {
            _ = stopping.changed() => break,
            snapshot = crate::workspace::snapshot(&state) => snapshot,
        };
        match snapshot {
            Ok(snapshot) => {
                let next = meaningful(snapshot.clone());
                let changed = unavailable || previous.as_ref() != Some(&next);
                previous = Some(next);
                unavailable = false;
                // Refresh ages in the latest cache without notifying unchanged data.
                sender.send_if_modified(|value| {
                    *value = Update::Snapshot(Arc::new(snapshot));
                    changed
                });
            }
            Err(_) if !unavailable => {
                unavailable = true;
                let _ = sender.send(Update::Unavailable);
            }
            Err(_) => {}
        }
    }
}

pub(crate) fn source_allowed(state: &AppState, headers: &HeaderMap) -> bool {
    if headers.get("sec-fetch-site").and_then(|value| value.to_str().ok()).is_some_and(|site| !matches!(site, "same-origin" | "none")) {
        return false;
    }
    let Some(host) = headers.get(header::HOST).and_then(|value| value.to_str().ok()) else { return false };
    if !state.config.origins.iter().any(|origin| origin.split_once("://").is_some_and(|(_, authority)| authority == host)) {
        return false;
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        return origin.to_str().ok().is_some_and(|origin| {
            state.config.origins.iter().any(|allowed| allowed == origin) && origin.split_once("://").is_some_and(|(_, authority)| authority == host)
        });
    }
    true
}

pub async fn events(State(state): State<Arc<AppState>>, headers: HeaderMap, uri: Uri) -> Response {
    if *state.browser.live.stopping.borrow() {
        return crate::workspace::protect(StatusCode::SERVICE_UNAVAILABLE.into_response());
    }
    let Some(browser) = crate::workspace::browser_session(&state, &headers).await else {
        // 204 tells EventSource to stop reconnecting after the cookie expires.
        return crate::workspace::protect(StatusCode::NO_CONTENT.into_response());
    };
    if !source_allowed(&state, &headers) {
        return crate::workspace::protect(StatusCode::FORBIDDEN.into_response());
    }
    let form = match crate::workspace::fields(uri.query().unwrap_or_default()) {
        Some(form) => form,
        None => return crate::workspace::protect(StatusCode::BAD_REQUEST.into_response()),
    };
    let query = form.get("q").cloned().unwrap_or_default();
    let kind = form.get("kind").cloned().unwrap_or_default();
    if query.len() > 1024 || !matches!(kind.as_str(), "" | "all" | "model" | "export" | "other") {
        return crate::workspace::protect(StatusCode::BAD_REQUEST.into_response());
    }
    let lease = match state.browser.live.reserve(browser) {
        Ok(lease) => lease,
        Err(status) => {
            let mut response = crate::workspace::protect(status.into_response());
            response.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
            return response;
        }
    };
    let receiver = state.browser.live.subscribe(&state).await;
    let (writer, reader) = tokio::io::duplex(64 * 1024);
    let (disconnect, disconnected) = oneshot::channel();
    let reader = Reader { inner: reader, disconnected: Some(disconnect) };
    tokio::spawn(stream(state, headers, (query, kind), receiver, writer, lease, disconnected));
    let mut response = crate::workspace::protect(Body::from_stream(ReaderStream::new(reader)).into_response());
    response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("text/event-stream; charset=utf-8"));
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store, no-transform"));
    response.headers_mut().insert("x-accel-buffering", HeaderValue::from_static("no"));
    response
}

async fn write(writer: &mut DuplexStream, frame: &str) -> bool {
    matches!(tokio::time::timeout(WRITE_TIMEOUT, writer.write_all(frame.as_bytes())).await, Ok(Ok(())))
}

fn frame(update: &Update, query: &str, kind: &str) -> Option<String> {
    let data = match update {
        Update::Pending => return None,
        Update::Unavailable => return Some("event: unavailable\ndata: {}\n\n".into()),
        Update::Snapshot(snapshot) => {
            let mut snapshot = snapshot.as_ref().clone();
            if let Some(files) = snapshot.get_mut("files").and_then(Value::as_array_mut) {
                let filter = query.to_lowercase();
                files.retain(|file| {
                    file.get("name").and_then(Value::as_str).is_some_and(|name| name.to_lowercase().contains(&filter))
                        && (kind.is_empty() || kind == "all" || file.get("kind").and_then(Value::as_str) == Some(kind))
                });
            }
            json!({ "html": crate::workspace_view::fragment(&snapshot, query, kind) })
        }
    };
    Some(format!("event: workspace\ndata: {data}\n\n"))
}

async fn stream(
    state: Arc<AppState>,
    headers: HeaderMap,
    filters: (String, String),
    receiver: watch::Receiver<Update>,
    writer: DuplexStream,
    _lease: Lease,
    disconnected: oneshot::Receiver<()>,
) {
    let mut stopping = state.browser.live.stopping.subscribe();
    if *stopping.borrow() {
        return;
    }
    // Select around the whole writer, including backpressure, so a stalled
    // browser cannot keep graceful shutdown waiting for the write timeout.
    tokio::select! {
        _ = stopping.changed() => {},
        _ = stream_updates(&state, &headers, filters, receiver, writer, disconnected) => {}
    }
}

async fn stream_updates(
    state: &AppState,
    headers: &HeaderMap,
    filters: (String, String),
    mut receiver: watch::Receiver<Update>,
    mut writer: DuplexStream,
    mut disconnected: oneshot::Receiver<()>,
) {
    let (query, kind) = filters;
    if !write(&mut writer, "retry: 3000\n\n").await {
        return;
    }
    let mut auth = tokio::time::interval(POLL);
    auth.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut keepalive = tokio::time::interval(KEEPALIVE);
    keepalive.tick().await;
    let mut initial = true;
    loop {
        if crate::workspace::browser_session(state, headers).await.is_none() {
            let _ = write(&mut writer, "event: expired\ndata: {}\n\n").await;
            break;
        }
        if initial {
            initial = false;
            let update = receiver.borrow_and_update().clone();
            if let Some(frame) = frame(&update, &query, &kind)
                && !write(&mut writer, &frame).await
            {
                break;
            }
        }
        tokio::select! {
            _ = &mut disconnected => break,
            result = receiver.changed() => {
                if result.is_err() { break; }
                initial = true;
            }
            _ = auth.tick() => {}
            _ = keepalive.tick() => {
                if !write(&mut writer, ": keepalive\n\n").await { break; }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn idle_ages_are_ignored_but_edits_busy_state_and_files_are_meaningful() {
        let first = json!({"files":[],"sessions":[{"display_id":"s-test","model_revision":1,"dirty":true,"request_in_flight":false,"last_request_seconds":1,"last_model_change_seconds":1}]});
        let mut next = first.clone();
        next["sessions"][0]["last_request_seconds"] = json!(90);
        next["sessions"][0]["last_model_change_seconds"] = json!(90);
        assert_eq!(meaningful(first.clone()), meaningful(next.clone()));
        for (key, value) in [("model_revision", json!(2)), ("request_in_flight", json!(true)), ("dirty", json!(false))] {
            let mut changed = next.clone();
            changed["sessions"][0][key] = value;
            assert_ne!(meaningful(first.clone()), meaningful(changed));
        }
        next["files"] = json!([{"name":"created.step"}]);
        assert_ne!(meaningful(first), meaningful(next));
    }

    #[test]
    fn streams_are_bounded_per_browser_and_release_capacity() {
        let live = Live::default();
        let leases = (0..MAX_BROWSER_STREAMS).map(|_| live.reserve("private-cookie-key".into()).unwrap()).collect::<Vec<_>>();
        assert!(matches!(live.reserve("private-cookie-key".into()), Err(StatusCode::TOO_MANY_REQUESTS)));
        drop(leases);
        assert_eq!(live.slots.available_permits(), MAX_STREAMS);
        assert!(live.browsers.lock().unwrap().is_empty());
        assert!(live.reserve("private-cookie-key".into()).is_ok());
    }

    #[test]
    fn total_stream_limit_is_shared_across_browser_sessions() {
        let live = Live::default();
        let leases = (0..MAX_STREAMS).map(|index| live.reserve(format!("browser-{index}")).unwrap()).collect::<Vec<_>>();
        assert!(matches!(live.reserve("one-more-browser".into()), Err(StatusCode::TOO_MANY_REQUESTS)));
        drop(leases);
        assert_eq!(live.slots.available_permits(), MAX_STREAMS);
    }

    #[tokio::test]
    async fn disconnected_reader_releases_writer_without_unbounded_queue() {
        let (mut writer, reader) = tokio::io::duplex(64);
        drop(reader);
        assert!(!write(&mut writer, ": keepalive\n\n").await);
    }
}
