use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Path, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};
use tower_http::services::ServeDir;

use crate::audit::Event;
use crate::paths;
use crate::sessions::EndReason;
use crate::sessions::State as AppState;

const VERSIONS: &[&str] = &["2025-06-18", "2025-03-26"];
const SESSION: &str = "mcp-session-id";
const VERSION: &str = "mcp-protocol-version";

pub fn router(state: Arc<AppState>) -> Router {
    let protected = Router::new()
        .route("/mcp", post(mcp).get(no_stream).delete(close))
        .route("/files/{*name}", get(download).put(upload))
        .layer(from_fn_with_state(Arc::clone(&state), authorize));
    Router::new()
        .merge(protected)
        .route("/health", get(|| async { Json(json!({"status":"ok","version":env!("CARGO_PKG_VERSION")})) }))
        .fallback_service(ServeDir::new(&state.config.web).append_index_html_on_directories(true))
        .layer(DefaultBodyLimit::max(16 << 20))
        .with_state(state)
}

fn failure(status: StatusCode, id: Value, message: &str) -> Response {
    (status, Json(json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":message}}))).into_response()
}

fn header_str<'a>(headers: &'a HeaderMap, key: &str) -> Option<&'a str> {
    headers.get(key).and_then(|v| v.to_str().ok())
}

fn token_matches(provided: &[u8], expected: &[u8]) -> bool {
    // Compare all expected bytes even after a mismatch. Tokens have bounded lengths.
    let mut diff = provided.len() ^ expected.len();
    for (i, byte) in expected.iter().enumerate() {
        diff |= usize::from(provided.get(i).copied().unwrap_or(0) ^ byte);
    }
    diff == 0
}

async fn authorize(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    if headers.contains_key(header::ORIGIN) && !header_str(headers, "origin").is_some_and(|o| state.config.origins.iter().any(|allowed| allowed == o))
    {
        state.audit.emit(Event::OriginRejected, json!({"reason":"origin_not_allowed"}));
        return failure(StatusCode::FORBIDDEN, Value::Null, "origin is not allowed");
    }
    let token = header_str(headers, "authorization").and_then(|s| s.strip_prefix("Bearer "));
    if !token.is_some_and(|t| token_matches(t.as_bytes(), state.config.token.as_bytes())) {
        state.audit.emit(Event::AuthRejected, json!({"reason":if token.is_some() { "invalid_token" } else { "missing_bearer" }}));
        let mut response = failure(StatusCode::UNAUTHORIZED, Value::Null, "Bearer token required");
        response.headers_mut().insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        return response;
    }
    // Authentication is stateless. Log session admission, not every tool request.
    if !headers.contains_key(SESSION) && request.method() == axum::http::Method::POST {
        state.audit.emit(Event::AuthAccepted, json!({"endpoint":"mcp"}));
    }
    let mut response = next.run(request).await;
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    response
}

fn validate(message: &Value) -> Result<(), &'static str> {
    if !message.is_object() || message.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err("expected one JSON-RPC 2.0 object (HTTP batches are not supported)");
    }
    if let Some(id) = message.get("id")
        && !(id.is_string() || id.is_i64() || id.is_u64())
    {
        return Err("request ID must be a string or integer");
    }
    if let Some(params) = message.get("params")
        && !params.is_object()
    {
        return Err("params must be an object");
    }
    if message.get("method").is_none() && message.get("id").is_some() && (message.get("result").is_some() ^ message.get("error").is_some()) {
        return Ok(());
    }
    if !message.get("method").is_some_and(Value::is_string) {
        return Err("method must be a string");
    }
    Ok(())
}

async fn mcp(State(state): State<Arc<AppState>>, headers: HeaderMap, bytes: Bytes) -> Response {
    if header_str(&headers, "content-type").is_none_or(|h| h.split(';').next() != Some("application/json")) {
        return failure(StatusCode::UNSUPPORTED_MEDIA_TYPE, Value::Null, "Content-Type must be application/json");
    }
    let accepts = |kind| header_str(&headers, "accept").is_some_and(|h| h.split(',').any(|t| t.trim().split(';').next() == Some(kind)));
    if !accepts("application/json") || !accepts("text/event-stream") {
        return failure(StatusCode::NOT_ACCEPTABLE, Value::Null, "Accept must include application/json and text/event-stream");
    }
    let mut message: Value = match serde_json::from_slice(&bytes) {
        Ok(m) => m,
        Err(_) => return failure(StatusCode::BAD_REQUEST, Value::Null, "invalid JSON"),
    };
    let id = message.get("id").cloned().unwrap_or(Value::Null);
    if let Err(e) = validate(&message) {
        return failure(StatusCode::BAD_REQUEST, id, e);
    }
    let initialize = message.get("method").and_then(Value::as_str) == Some("initialize");
    if initialize && (message.get("id").is_none() || headers.contains_key(SESSION)) {
        return failure(StatusCode::BAD_REQUEST, id, "initialize needs an ID and must not reuse a session ID");
    }
    if let Some(version) = header_str(&headers, VERSION)
        && !VERSIONS.contains(&version)
    {
        return failure(StatusCode::BAD_REQUEST, id, "unsupported MCP protocol version");
    }
    let (session_id, worker, log_id) = if initialize {
        let requested = message.pointer("/params/protocolVersion").and_then(Value::as_str).unwrap_or(VERSIONS[0]);
        if requested == "2024-11-05" {
            return failure(StatusCode::BAD_REQUEST, id, "HTTP requires protocol 2025-03-26 or later; stdio supports the legacy revision");
        }
        let version = if VERSIONS.contains(&requested) { requested } else { VERSIONS[0] };
        match state.create(version.into()).await {
            Ok(s) => s,
            Err(e) => return failure(StatusCode::SERVICE_UNAVAILABLE, id, &e),
        }
    } else {
        state.expire().await;
        let Some(session_id) = header_str(&headers, SESSION) else {
            return failure(StatusCode::BAD_REQUEST, id, "Mcp-Session-Id required; initialize first");
        };
        let mut sessions = state.sessions.lock().await;
        let Some(entry) = sessions.get_mut(session_id) else {
            state.audit.emit(Event::SessionRejected, json!({"reason":"unknown_or_expired"}));
            return failure(StatusCode::NOT_FOUND, id, "session ended or expired; initialize again and reopen a saved design");
        };
        if let Some(version) = header_str(&headers, VERSION)
            && version != entry.version
        {
            return failure(StatusCode::BAD_REQUEST, id, "protocol version differs from this session's negotiated version");
        }
        entry.touched = std::time::Instant::now();
        (session_id.to_owned(), Arc::clone(&entry.worker), entry.log_id.clone())
    };
    let tool = message.pointer("/params/name").and_then(Value::as_str).unwrap_or_default().to_owned();
    if message.get("method").and_then(Value::as_str) == Some("tools/call")
        && let Some(args) = message.get_mut("params").and_then(|p| p.get_mut("arguments"))
        && let Err(e) = paths::prepare_tool(&state.config.files, &tool, args)
    {
        state.audit.emit(Event::ToolFailed, json!({"session":log_id,"reason":"file_access_rejected"}));
        if message.get("id").is_none() {
            return StatusCode::ACCEPTED.into_response();
        }
        return Json(json!({"jsonrpc":"2.0","id":id,"result":{"isError":true,"content":[{"type":"text","text":e}]}})).into_response();
    }
    let result = state.exchange(worker, message, log_id.clone()).await;
    let mut response = match result {
        Ok(Some(mut reply)) => {
            if reply.get("error").is_some() || reply.pointer("/result/isError").and_then(Value::as_bool) == Some(true) {
                state.audit.emit(Event::ToolFailed, json!({"session":log_id,"reason":"cad_request_rejected"}));
            }
            if initialize && let Some(instructions) = reply.pointer_mut("/result/instructions") {
                let text = instructions.as_str().unwrap_or_default();
                *instructions = Value::String(format!(
                    "{text} Deployment: this is a separate headless model, independent of the browser app. File paths are relative to the shared persistent files directory. Save explicitly before session expiry or restart, then open the saved file in a new session. Exported files can be downloaded with the same Bearer authentication from /files/<relative-path>."
                ));
            }
            Json(reply).into_response()
        }
        Ok(None) => StatusCode::ACCEPTED.into_response(),
        Err(e) if e.starts_with("CAD session is busy") => return failure(StatusCode::TOO_MANY_REQUESTS, id, &e),
        Err(e) => {
            state.remove(&session_id, EndReason::Failure).await;
            return failure(StatusCode::BAD_GATEWAY, id, &e);
        }
    };
    if initialize && let Ok(value) = HeaderValue::from_str(&session_id) {
        response.headers_mut().insert(SESSION, value);
    }
    response
}

async fn no_stream(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    state.expire().await;
    if let Some(id) = header_str(&headers, SESSION)
        && !state.sessions.lock().await.contains_key(id)
    {
        return failure(StatusCode::NOT_FOUND, Value::Null, "session ended or expired");
    }
    // This server replies synchronously with application/json; SSE is optional in MCP.
    (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, "POST, DELETE")]).into_response()
}

async fn close(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    state.expire().await;
    let Some(id) = header_str(&headers, SESSION) else {
        return failure(StatusCode::BAD_REQUEST, Value::Null, "Mcp-Session-Id required");
    };
    if state.remove(id, EndReason::Client).await {
        StatusCode::NO_CONTENT
    } else {
        state.audit.emit(Event::SessionRejected, json!({"reason":"unknown_or_expired"}));
        StatusCode::NOT_FOUND
    }
    .into_response()
}

async fn download(State(state): State<Arc<AppState>>, Path(name): Path<String>) -> Response {
    let path = match paths::resolve(&state.config.files, &name) {
        Ok(p) => p,
        Err(e) => return failure(StatusCode::BAD_REQUEST, Value::Null, &e),
    };
    let file = match tokio::fs::File::open(path).await {
        Ok(f) => f,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    match file.metadata().await {
        Ok(m) if m.is_file() && m.len() <= 256 << 20 => {}
        _ => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    }
    use tokio::io::AsyncReadExt;
    let body = Body::from_stream(tokio_util::io::ReaderStream::new(file.take(256 << 20)));
    ([(header::CONTENT_TYPE, "application/octet-stream"), (header::CONTENT_DISPOSITION, "attachment")], body).into_response()
}

async fn upload(State(state): State<Arc<AppState>>, Path(name): Path<String>, bytes: Bytes) -> Response {
    let path = match paths::resolve(&state.config.files, &name) {
        Ok(p) => p,
        Err(e) => return failure(StatusCode::BAD_REQUEST, Value::Null, &e),
    };
    // Atomic replacement prevents a concurrent open from seeing a partially uploaded file.
    let temp = state.config.files.join(format!(".upload-{}", uuid::Uuid::new_v4()));
    if tokio::fs::write(&temp, &bytes).await.is_err() {
        return failure(StatusCode::INTERNAL_SERVER_ERROR, Value::Null, "cannot write upload");
    }
    if tokio::fs::rename(&temp, path).await.is_err() {
        let _ = tokio::fs::remove_file(temp).await;
        return failure(StatusCode::BAD_REQUEST, Value::Null, "cannot store upload (parent directory must already exist)");
    }
    StatusCode::CREATED.into_response()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use axum::body::Body;
    use std::time::Duration;
    use tower::ServiceExt;

    fn state() -> Arc<AppState> {
        Arc::new(AppState::new(crate::config::Config {
            listen: "127.0.0.1:0".into(),
            cli: "not-started-in-authorization-tests".into(),
            web: std::env::temp_dir(),
            files: std::env::temp_dir(),
            token: "test-token-for-http-authorization".into(),
            token_source: crate::config::TokenSource::Environment,
            origins: vec!["https://solvecraft.example".into()],
            max_sessions: 2,
            idle: Duration::from_secs(60),
            timeout: Duration::from_secs(10),
        }))
    }

    fn app() -> Router {
        router(state())
    }

    #[test]
    fn every_start_reports_file_or_environment_location_without_the_token() {
        let mut state = state();
        state.audit.startup(&state.config);
        let first = state.audit.rows.lock().unwrap().join("\n");
        assert!(first.contains("token.location") && first.contains("SOLVECRAFT_MCP_TOKEN"));
        assert!(!first.contains(&state.config.token));
        let config = &mut Arc::get_mut(&mut state).unwrap().config;
        config.token_source = crate::config::TokenSource::File("/data/mcp-token".into());
        for _ in 0..2 {
            state.audit.startup(&state.config);
        }
        let rows = state.audit.rows.lock().unwrap();
        assert_eq!(rows.iter().filter(|row| row.contains("token.location") && row.contains("/data/mcp-token")).count(), 2);
        assert!(rows.iter().all(|row| !row.contains(&state.config.token)));
    }

    #[tokio::test]
    async fn audit_logs_auth_results_without_credentials_paths_or_routine_health_noise() {
        let state = state();
        let app = router(Arc::clone(&state));
        for _ in 0..5 {
            let response = app.clone().oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap()).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
        assert!(state.audit.rows.lock().unwrap().is_empty());
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("authorization", "Bearer private-invalid-token-marker")
                    .body(Body::from("private-body-marker"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/files/private-file-marker")
                    .header("authorization", "Bearer test-token-for-http-authorization")
                    .header("origin", "https://private-origin-marker.invalid")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("authorization", "Bearer test-token-for-http-authorization")
                    .header("content-type", "application/json")
                    .header("accept", "application/json, text/event-stream")
                    .body(Body::from("[]"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let rows = state.audit.rows.lock().unwrap();
        assert_eq!(rows.len(), 3);
        let text = rows.join("\n");
        assert!(text.contains("auth.accepted") && text.contains("auth.rejected") && text.contains("origin.rejected"));
        for private in [
            "test-token-for-http-authorization",
            "private-invalid-token-marker",
            "private-body-marker",
            "private-file-marker",
            "private-origin-marker",
            "\u{1b}",
        ] {
            assert!(!text.contains(private));
        }
        for row in rows.iter() {
            assert!(row.split_whitespace().next().is_some_and(|timestamp| chrono::DateTime::parse_from_rfc3339(timestamp).is_ok()));
        }
    }

    #[tokio::test]
    async fn authorization_origin_and_protocol_are_checked_before_starting_cad() {
        let response = app().oneshot(Request::builder().method("POST").uri("/mcp").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let response = app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("authorization", "Bearer test-token-for-http-authorization")
                    .header("origin", "https://attacker.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let response = app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("authorization", "Bearer test-token-for-http-authorization")
                    .header("content-type", "application/json")
                    .header("accept", "application/json, text/event-stream")
                    .body(Body::from("[]"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let response = app()
            .oneshot(Request::builder().uri("/mcp").header("authorization", "Bearer test-token-for-http-authorization").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[test]
    fn hostile_envelopes_and_tokens_are_rejected() {
        for message in [
            json!(null),
            json!([]),
            json!({"jsonrpc":"2.0","method":1}),
            json!({"jsonrpc":"2.0","method":"ping","id":null}),
            json!({"jsonrpc":"2.0","method":"ping","params":[]}),
        ] {
            assert!(validate(&message).is_err());
        }
        assert!(token_matches(b"abc", b"abc"));
        for value in [b"ab".as_slice(), b"abcd", b"xbc"] {
            assert!(!token_matches(value, b"abc"));
        }
    }
}
