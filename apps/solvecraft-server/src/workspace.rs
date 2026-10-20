//! A browser workspace with narrowly authorized, confirmed file deletion. Cookies are never accepted by the MCP routes.
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::sessions::State as AppState;

#[path = "workspace_remember.rs"]
mod remember;

const COOKIE: &str = "solidcraft-workspace";
const CHALLENGE: &str = "solidcraft-workspace-login";
const LIFETIME: Duration = Duration::from_secs(3600);

struct BrowserSession {
    csrf: String,
    expires: Instant,
    remembered: Option<remember::Expiry>,
}

#[derive(Default)]
struct AuthStore {
    sessions: HashMap<String, BrowserSession>,
    challenges: HashMap<String, Instant>,
    failures: VecDeque<Instant>,
}

pub struct BrowserAuth {
    inner: Mutex<AuthStore>,
    pub(crate) live: crate::workspace_live::Live,
    pub(crate) media: crate::workspace_media::Media,
    pub(crate) actions: crate::workspace_actions::Actions,
}

impl BrowserAuth {
    pub fn new(config: &crate::config::Config) -> Self {
        Self { inner: Mutex::new(remember::load(config)), live: Default::default(), media: Default::default(), actions: Default::default() }
    }
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/workspace", get(dashboard))
        .route("/workspace/", get(dashboard))
        .route("/workspace/login", get(login_page).post(login))
        .route("/workspace/logout", post(logout))
        .route("/workspace/api", get(api))
        .route("/workspace/delete/{*name}", get(crate::workspace_actions::preview))
        .route("/workspace/delete", post(crate::workspace_actions::delete).layer(DefaultBodyLimit::max(4096)))
        .route("/workspace/archive/{*name}", get(crate::workspace_actions::download))
        .route("/workspace/details/{*name}", get(details))
        .route("/workspace/boot.js", get(boot))
        .route("/workspace/assets/{*name}", get(client_asset))
        .route("/workspace/events", get(crate::workspace_live::events))
        .route("/workspace/observe/{handle}/events", get(crate::workspace_observer::events))
        .route("/workspace/observe/{handle}/image", get(crate::workspace_observer::image))
        .route("/workspace/preview/{*name}", get(crate::workspace_media::preview))
        .route("/workspace/open/{*name}", get(crate::workspace_media::open))
        .route("/workspace/import/{ticket}", get(crate::workspace_media::import))
        .route("/workspace/files/{*name}", get(download))
}

pub(crate) fn protect(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    // no-referrer also turns a native form POST's Origin into "null" (Fetch),
    // which would correctly fail our origin/CSRF check. Keep same-origin forms
    // identifiable while withholding referrers from every external destination.
    headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("same-origin"));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; img-src 'self' data:; style-src 'unsafe-inline'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'"),
    );
    response
}

fn redirect(path: &'static str) -> Response {
    protect((StatusCode::SEE_OTHER, [(header::LOCATION, path)]).into_response())
}

fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).flat_map(|s| s.split(';')).find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        (key == name && value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())).then_some(value)
    })
}

fn secret() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

fn secure(state: &AppState, headers: &HeaderMap) -> bool {
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or_default();
    let local = host == "localhost"
        || host.starts_with("localhost:")
        || host == "127.0.0.1"
        || host.starts_with("127.0.0.1:")
        || host == "[::1]"
        || host.starts_with("[::1]:");
    !(local && state.config.origins.iter().any(|o| o == &format!("http://{host}")))
}

fn set_cookie(response: &mut Response, name: &str, value: &str, max_age: u64, secure: bool) {
    let suffix = if secure { "; Secure" } else { "" };
    if let Ok(value) = HeaderValue::from_str(&format!("{name}={value}; Path=/workspace; Max-Age={max_age}; HttpOnly; SameSite=Strict{suffix}")) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
}

pub(crate) fn check_origin(state: &AppState, headers: &HeaderMap) -> Result<(), &'static str> {
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok());
    let origin = headers.get(header::ORIGIN).and_then(|h| h.to_str().ok()).ok_or("origin_missing")?;
    if !state.config.origins.iter().any(|allowed| allowed == origin) {
        return Err("origin_not_allowed");
    }
    if origin.split_once("://").is_none_or(|(_, authority)| Some(authority) != host) {
        return Err("host_mismatch");
    }
    Ok(())
}

fn login_rejected(state: &AppState, status: StatusCode, reason: &'static str) -> Response {
    // Only fixed reason codes: never log headers, submitted credentials or cookies.
    state.audit.emit(crate::audit::Event::AuthRejected, json!({"endpoint":"workspace","reason":reason}));
    protect((status, Html(crate::workspace_view::login_rejected(reason))).into_response())
}

fn decode(value: &str) -> Option<String> {
    let mut out = Vec::new();
    let mut bytes = value.as_bytes().iter().copied();
    while let Some(byte) = bytes.next() {
        match byte {
            b'+' => out.push(b' '),
            b'%' => {
                let a = char::from(bytes.next()?).to_digit(16)?;
                let b = char::from(bytes.next()?).to_digit(16)?;
                out.push((a * 16 + b) as u8);
            }
            _ => out.push(byte),
        }
    }
    String::from_utf8(out).ok()
}

pub(crate) fn fields(input: &str) -> Option<HashMap<String, String>> {
    if input.len() > 4096 {
        return None;
    }
    let mut out = HashMap::new();
    for field in input.split('&').filter(|s| !s.is_empty()) {
        let (key, value) = field.split_once('=')?;
        if out.insert(decode(key)?, decode(value)?).is_some() {
            return None;
        }
    }
    Some(out)
}

fn clean(store: &mut AuthStore) {
    let now = Instant::now();
    store.sessions.retain(|_, s| s.expires > now);
    store.challenges.retain(|_, expires| *expires > now);
    while store.failures.front().is_some_and(|time| time.elapsed() >= Duration::from_secs(60)) {
        store.failures.pop_front();
    }
}

pub(crate) async fn browser_session(state: &AppState, headers: &HeaderMap) -> Option<String> {
    let id = cookie_value(headers, COOKIE)?;
    let mut store = state.browser.inner.lock().await;
    clean(&mut store);
    let session = store.sessions.get_mut(&remember::hash(id))?;
    if matches!(session.remembered, Some(remember::Expiry::Forever)) {
        session.expires = Instant::now() + Duration::from_secs(remember::Expiry::Forever.cookie_seconds());
    }
    Some(session.csrf.clone())
}

fn renew_remembered(state: &AppState, headers: &HeaderMap, response: &mut Response, store: &AuthStore) {
    if let Some(id) = cookie_value(headers, COOKIE)
        && store.sessions.get(&remember::hash(id)).is_some_and(|session| matches!(session.remembered, Some(remember::Expiry::Forever)))
    {
        set_cookie(response, COOKIE, id, remember::Expiry::Forever.cookie_seconds(), secure(state, headers));
    }
}

async fn challenge(state: &AppState, headers: &HeaderMap, error: bool) -> Response {
    let mut store = state.browser.inner.lock().await;
    clean(&mut store);
    // Tabs share cookies. Replacing a still-valid challenge on every GET made
    // another open tab's form fail before its token was ever checked.
    let csrf = match cookie_value(headers, CHALLENGE).filter(|id| store.challenges.contains_key(*id)) {
        Some(existing) => existing.to_owned(),
        None => {
            if store.challenges.len() >= 64
                && let Some(oldest) = store.challenges.iter().min_by_key(|(_, expires)| **expires).map(|(id, _)| id.clone())
            {
                store.challenges.remove(&oldest);
            }
            let csrf = secret();
            store.challenges.insert(csrf.clone(), Instant::now() + Duration::from_secs(600));
            csrf
        }
    };
    let mut response = protect(Html(crate::workspace_view::login(error, &csrf)).into_response());
    set_cookie(&mut response, CHALLENGE, &csrf, 600, secure(state, headers));
    response
}

async fn login_page(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if browser_session(&state, &headers).await.is_some() {
        return redirect("/workspace");
    }
    challenge(&state, &headers, false).await
}

async fn login(State(state): State<Arc<AppState>>, headers: HeaderMap, bytes: Bytes) -> Response {
    if let Err(reason) = check_origin(&state, &headers) {
        return login_rejected(&state, StatusCode::FORBIDDEN, reason);
    }
    if bytes.len() > 4096
        || headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_none_or(|v| v.split(';').next() != Some("application/x-www-form-urlencoded"))
    {
        return login_rejected(&state, StatusCode::BAD_REQUEST, "invalid_form");
    }
    let Some(form) = std::str::from_utf8(&bytes).ok().and_then(fields) else {
        return login_rejected(&state, StatusCode::BAD_REQUEST, "invalid_form");
    };
    let Some(cookie) = cookie_value(&headers, CHALLENGE) else {
        return login_rejected(&state, StatusCode::FORBIDDEN, "csrf_cookie_missing");
    };
    let Some(csrf) = form.get("csrf").filter(|csrf| cookie == csrf.as_str()) else {
        return login_rejected(&state, StatusCode::FORBIDDEN, "csrf_mismatch");
    };
    let remembered = match remember::choice(form.get("remember").map(String::as_str)) {
        Ok(value) => value,
        Err(()) => return login_rejected(&state, StatusCode::BAD_REQUEST, "invalid_form"),
    };
    let mut store = state.browser.inner.lock().await;
    clean(&mut store);
    if store.failures.len() >= 12 {
        return login_rejected(&state, StatusCode::TOO_MANY_REQUESTS, "rate_limited");
    }
    if store.sessions.len() >= 64 {
        return login_rejected(&state, StatusCode::TOO_MANY_REQUESTS, "browser_capacity");
    }
    if store.challenges.remove(csrf).is_none() {
        return login_rejected(&state, StatusCode::FORBIDDEN, "csrf_expired");
    }
    let valid = form.get("token").is_some_and(|provided| crate::http::token_matches(provided.as_bytes(), state.config.token.as_bytes()));
    if !valid {
        store.failures.push_back(Instant::now());
        drop(store);
        state.audit.emit(crate::audit::Event::AuthRejected, json!({"endpoint":"workspace","reason":"invalid_token"}));
        return challenge(&state, &headers, true).await;
    }
    let id = secret();
    let key = remember::hash(&id);
    let lifetime = remembered.map_or(LIFETIME.as_secs(), |value| value.cookie_seconds());
    store.sessions.insert(key.clone(), BrowserSession { csrf: secret(), expires: Instant::now() + Duration::from_secs(lifetime), remembered });
    if remembered.is_some() && remember::save(&state.config, &store).is_err() {
        store.sessions.remove(&key);
        return login_rejected(&state, StatusCode::SERVICE_UNAVAILABLE, "browser_storage_unavailable");
    }
    state.audit.emit(crate::audit::Event::AuthAccepted, json!({"endpoint":"workspace"}));
    let mut response = redirect("/workspace");
    set_cookie(&mut response, COOKIE, &id, lifetime, secure(&state, &headers));
    set_cookie(&mut response, CHALLENGE, "", 0, secure(&state, &headers));
    response
}

async fn logout(State(state): State<Arc<AppState>>, headers: HeaderMap, bytes: Bytes) -> Response {
    let Some(csrf) = browser_session(&state, &headers).await else {
        return protect(StatusCode::UNAUTHORIZED.into_response());
    };
    if check_origin(&state, &headers).is_err()
        || std::str::from_utf8(&bytes).ok().and_then(fields).and_then(|f| f.get("csrf").cloned()).as_deref() != Some(&csrf)
    {
        return protect(StatusCode::FORBIDDEN.into_response());
    }
    if let Some(id) = cookie_value(&headers, COOKIE) {
        let mut store = state.browser.inner.lock().await;
        let key = remember::hash(id);
        if let Some(session) = store.sessions.remove(&key)
            && session.remembered.is_some()
            && remember::save(&state.config, &store).is_err()
        {
            store.sessions.insert(key, session);
            return login_rejected(&state, StatusCode::SERVICE_UNAVAILABLE, "browser_storage_unavailable");
        }
    }
    let mut response = redirect("/workspace/login");
    set_cookie(&mut response, COOKIE, "", 0, secure(&state, &headers));
    response
}

pub async fn snapshot(state: &AppState) -> Result<Value, String> {
    let root = state.config.files.clone();
    let listing = tokio::task::spawn_blocking(move || crate::workspace_files::list(&root)).await.map_err(|_| "file listing unavailable")??;
    let entries = state.sessions.lock().await;
    let mut sessions = Vec::new();
    for entry in entries.values() {
        let mut row = entry.model.lock().map_err(|_| "model status unavailable")?.snapshot();
        row["display_id"] = json!(entry.log_id.chars().take(14).collect::<String>());
        row["observer_handle"] = json!(entry.observer.handle);
        row["request_in_flight"] = json!(entry.observer.cad_requested.load(std::sync::atomic::Ordering::Acquire));
        row["last_request_seconds"] = json!(entry.touched.elapsed().as_secs());
        sessions.push(row);
    }
    sessions.sort_by(|a, b| a["display_id"].as_str().cmp(&b["display_id"].as_str()));
    let mut files = listing.files;
    for file in &mut files {
        let matching: Vec<_> = sessions.iter().filter(|session| session["current_file"] == file["name"]).collect();
        file["opened_by"] = json!(matching.iter().map(|s| s["display_id"].clone()).collect::<Vec<_>>());
        file["dirty"] = if matching.iter().any(|s| s["dirty"] == true) {
            json!(true)
        } else if matching.iter().all(|s| s["dirty"] == false) {
            json!(false)
        } else {
            Value::Null
        };
    }
    Ok(
        json!({"directories":listing.directories,"files":files,"sessions":sessions,"slots_total":state.config.max_sessions,"slots_used":state.slots_used(),"truncated":listing.truncated}),
    )
}

async fn dashboard(State(state): State<Arc<AppState>>, headers: HeaderMap, uri: Uri) -> Response {
    let Some(csrf) = browser_session(&state, &headers).await else {
        return redirect("/workspace/login");
    };
    let form = fields(uri.query().unwrap_or_default()).unwrap_or_default();
    let query = form.get("q").map(String::as_str).unwrap_or_default();
    let kind = form.get("kind").map(String::as_str).unwrap_or_default();
    match snapshot(&state).await {
        Ok(mut data) => {
            data["csrf"] = json!(csrf);
            if let Some(files) = data["files"].as_array_mut() {
                let filter = query.to_lowercase();
                files.retain(|file| {
                    file["name"].as_str().is_some_and(|n| n.to_lowercase().contains(&filter))
                        && (kind.is_empty() || kind == "all" || file["kind"] == kind)
                });
            }
            let mut response = protect(Html(crate::workspace_view::page(&data, query, kind)).into_response());
            renew_remembered(&state, &headers, &mut response, &*state.browser.inner.lock().await);
            response
        }
        Err(_) => protect((StatusCode::SERVICE_UNAVAILABLE, "Dateiübersicht vorübergehend nicht verfügbar.").into_response()),
    }
}

async fn api(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if browser_session(&state, &headers).await.is_none() {
        return protect(StatusCode::UNAUTHORIZED.into_response());
    }
    match snapshot(&state).await {
        Ok(data) => protect(Json(data).into_response()),
        Err(_) => protect(StatusCode::SERVICE_UNAVAILABLE.into_response()),
    }
}

async fn download(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(name): Path<String>) -> Response {
    if browser_session(&state, &headers).await.is_none() {
        return protect(StatusCode::UNAUTHORIZED.into_response());
    }
    if !crate::workspace_files::visible_name(&name) {
        return protect(StatusCode::BAD_REQUEST.into_response());
    }
    protect(crate::http::download(State(state), Path(name)).await)
}

async fn details(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(name): Path<String>) -> Response {
    if browser_session(&state, &headers).await.is_none() {
        return protect(StatusCode::UNAUTHORIZED.into_response());
    }
    let root = state.config.files.clone();
    match tokio::task::spawn_blocking(move || crate::workspace_files::details(&root, &name)).await {
        Ok(Ok(data)) => protect(Html(crate::workspace_view::details(&data)).into_response()),
        _ => protect((StatusCode::NOT_FOUND, "Dateidetails nicht verfügbar.").into_response()),
    }
}

async fn boot() -> Response {
    // Only wasm-bindgen's generated module bootstrap is JavaScript; application
    // behavior, EventSource handling and dialogs are implemented in Rust/Wasm.
    protect(
        (
            [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
            "import init from '/workspace/assets/solidcraft-workspace.js';init().catch(()=>{});",
        )
            .into_response(),
    )
}

async fn client_asset(State(state): State<Arc<AppState>>, Path(name): Path<String>) -> Response {
    let content_type = match name.as_str() {
        "solidcraft-workspace.js" => "text/javascript; charset=utf-8",
        "solidcraft-workspace_bg.wasm" => "application/wasm",
        _ => return protect(StatusCode::NOT_FOUND.into_response()),
    };
    let Some(parent) = state.config.web.parent() else { return protect(StatusCode::NOT_FOUND.into_response()) };
    let path = parent.join("workspace").join(name);
    match tokio::fs::read(path).await {
        Ok(bytes) => protect(([(header::CONTENT_TYPE, content_type)], bytes).into_response()),
        Err(_) => protect(StatusCode::NOT_FOUND.into_response()),
    }
}

pub fn tools() -> [Value; 2] {
    [
        json!({"name":"workspace_list_files","description":"List saved server files, independently of unsaved session models. Use path to obtain one file's content revision for expected_revisions on writes.","inputSchema":{"type":"object","properties":{"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":100},"path":{"type":"string"}},"additionalProperties":false},"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}),
        json!({"name":"session_status","description":"Current session's model file, unsaved changes, loaded content revision and request state; no other session's model contents.","inputSchema":{"type":"object","properties":{},"additionalProperties":false},"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}),
    ]
}

pub async fn file_tool(state: &AppState, args: &Value) -> Result<Value, String> {
    if !args.is_object() && !args.is_null() {
        return Err("arguments must be an object".into());
    }
    let root = state.config.files.clone();
    let args = args.clone();
    tokio::task::spawn_blocking(move || {
        let listing = crate::workspace_files::list(&root)?;
        if let Some(path) = args.get("path").and_then(Value::as_str) {
            let name = crate::workspace_model::relative(&root,path).ok_or("invalid workspace path")?;
            let mut file = listing.files.into_iter().find(|f|f["name"] == name).ok_or("file unavailable or outside listing limits")?;
            file["revision"] = json!(crate::workspace_files::revision(&root,&name)?);
            return Ok(json!({"files":[file],"truncated":listing.truncated}));
        }
        let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0).min(512) as usize;
        let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(50).clamp(1,100) as usize;
        let total = listing.files.len();
        Ok(json!({"directories":listing.directories,"files":listing.files.into_iter().skip(offset).take(limit).collect::<Vec<_>>(),"truncated":listing.truncated,"next_offset":(offset+limit < total).then_some(offset+limit)}))
    }).await.map_err(|_|"file listing unavailable".to_string())?
}

#[cfg(test)]
#[path = "workspace_auth_tests.rs"]
mod tests;
