//! Browser authentication stays separate from stateless MCP authentication.
#![allow(clippy::unwrap_used)]

#[path = "workspace_remember_tests.rs"]
mod remember_tests;

#[path = "workspace_live_tests.rs"]
mod live_tests;

#[path = "workspace_details_tests.rs"]
mod details_tests;

#[path = "workspace_media_tests.rs"]
mod media_tests;

use super::*;
use axum::body::{Body, to_bytes};
use axum::http::Request;
use tower::ServiceExt;

const HOST: &str = "solidcraft.example";
const ORIGIN: &str = "https://solidcraft.example";
const TOKEN: &str = "workspace-test-token-private-marker-0123456789";

struct Fixture {
    state: Arc<AppState>,
    directory: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!("solidcraft-workspace-auth-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("public&example.solidcraft"), b"public test file").unwrap();
        let state = Arc::new(AppState::new(crate::config::Config {
            listen: "127.0.0.1:0".into(),
            cli: "not-started-in-workspace-auth-tests".into(),
            web: directory.clone(),
            files: directory.clone(),
            browser_auth: directory.join(".workspace-sessions.json"),
            token: TOKEN.into(),
            token_source: crate::config::TokenSource::Environment,
            log_color: false,
            origins: vec![ORIGIN.into()],
            max_sessions: 2,
            idle: Duration::from_secs(60),
            timeout: Duration::from_secs(10),
        }));
        Self { state, directory }
    }

    fn app(&self) -> Router {
        crate::http::router(Arc::clone(&self.state))
    }

    async fn challenge(&self) -> (String, String) {
        let response =
            self.app().oneshot(Request::builder().uri("/workspace/login").header(header::HOST, HOST).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.headers().get(header::REFERRER_POLICY).unwrap(), "same-origin");
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .find_map(|value| value.strip_prefix(&format!("{CHALLENGE}=")))
            .unwrap();
        let csrf = cookie.split(';').next().unwrap().to_owned();
        assert_eq!(csrf.len(), 64);
        let cookie = format!("{CHALLENGE}={csrf}");
        let body = body_text(response).await;
        assert!(body.contains(&csrf));
        assert!(!body.contains(TOKEN));
        (csrf, cookie)
    }

    async fn login(&self) -> String {
        let (csrf, cookie) = self.challenge().await;
        let response =
            self.app().oneshot(form_request("/workspace/login", &cookie, &format!("csrf={csrf}&token={TOKEN}"), ORIGIN, HOST)).await.unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers()[header::LOCATION], "/workspace");
        let cookie = response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .find(|value| value.starts_with(&format!("{COOKIE}=")))
            .unwrap();
        for attribute in ["HttpOnly", "SameSite=Strict", "Secure", "Path=/workspace", "Max-Age=3600"] {
            assert!(cookie.contains(attribute), "missing {attribute}");
        }
        assert!(!cookie.contains(TOKEN));
        cookie.split(';').next().unwrap().to_owned()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn form_request(path: &str, cookie: &str, body: &str, origin: &str, host: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header(header::HOST, host)
        .header(header::ORIGIN, origin)
        .header(header::COOKIE, cookie)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

async fn body_text(response: Response) -> String {
    String::from_utf8(to_bytes(response.into_body(), 1024 * 1024).await.unwrap().to_vec()).unwrap()
}

#[tokio::test]
async fn unauthenticated_workspace_routes_deny_access_and_bearer_is_not_a_browser_login() {
    let fixture = Fixture::new();
    for bearer in [false, true] {
        for (path, expected) in [
            ("/workspace", StatusCode::SEE_OTHER),
            ("/workspace/api", StatusCode::UNAUTHORIZED),
            ("/workspace/files/public&example.solidcraft", StatusCode::UNAUTHORIZED),
        ] {
            let mut request = Request::builder().uri(path).header(header::HOST, HOST);
            if bearer {
                request = request.header(header::AUTHORIZATION, format!("Bearer {TOKEN}"));
            }
            let response = fixture.app().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
            assert_eq!(response.status(), expected);
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            if expected == StatusCode::SEE_OTHER {
                assert_eq!(response.headers()[header::LOCATION], "/workspace/login");
            }
            assert!(!body_text(response).await.contains("public&example"));
        }
    }
}

#[tokio::test]
async fn successful_browser_cookie_has_secure_flags_and_cannot_authorize_mcp_or_direct_files() {
    let fixture = Fixture::new();
    let cookie = fixture.login().await;
    let response = fixture
        .app()
        .oneshot(Request::builder().uri("/workspace/api").header(header::HOST, HOST).header(header::COOKIE, &cookie).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(response.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    let body = body_text(response).await;
    assert!(body.contains("public&example.solidcraft"));
    for private in [TOKEN, &fixture.directory.to_string_lossy(), &fixture.state.config.cli, &cookie] {
        assert!(!body.contains(private));
    }
    for (path, method) in [("/mcp", "POST"), ("/files/public&example.solidcraft", "GET")] {
        let response = fixture
            .app()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header(header::HOST, HOST)
                    .header(header::COOKIE, &cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::ACCEPT, "application/json, text/event-stream")
                    .body(Body::from("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let response = fixture
        .app()
        .oneshot(
            Request::builder()
                .uri("/workspace/files/public&example.solidcraft")
                .header(header::HOST, HOST)
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_DISPOSITION], "attachment");
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(body_text(response).await, "public test file");
}

#[tokio::test]
async fn login_requires_same_origin_host_bound_cookie_challenge_and_one_time_csrf() {
    let fixture = Fixture::new();
    let (csrf, cookie) = fixture.challenge().await;
    let body = format!("csrf={csrf}&token={TOKEN}");
    for (sent_cookie, sent_body, origin, host) in [
        ("", body.as_str(), ORIGIN, HOST),
        (cookie.as_str(), "csrf=wrong", ORIGIN, HOST),
        (cookie.as_str(), body.as_str(), "https://foreign.example", HOST),
        (cookie.as_str(), body.as_str(), ORIGIN, "foreign.example"),
    ] {
        let response = fixture.app().oneshot(form_request("/workspace/login", sent_cookie, sent_body, origin, host)).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    let response = fixture
        .app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workspace/login")
                .header(header::HOST, HOST)
                .header(header::COOKIE, &cookie)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = fixture.app().oneshot(form_request("/workspace/login", &cookie, &body, ORIGIN, HOST)).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let response = fixture.app().oneshot(form_request("/workspace/login", &cookie, &body, ORIGIN, HOST)).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(fixture.state.browser.inner.lock().await.sessions.len(), 1);
}

#[tokio::test]
async fn failed_tokens_are_not_reflected_and_authentication_attempts_are_bounded() {
    let fixture = Fixture::new();
    let private = "invalid-token-never-reflect-this-0123456789";
    for _ in 0..12 {
        let (csrf, cookie) = fixture.challenge().await;
        let response =
            fixture.app().oneshot(form_request("/workspace/login", &cookie, &format!("csrf={csrf}&token={private}"), ORIGIN, HOST)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let html = body_text(response).await;
        assert!(!html.contains(private));
        assert!(!html.contains(TOKEN));
    }
    let (csrf, cookie) = fixture.challenge().await;
    let response =
        fixture.app().oneshot(form_request("/workspace/login", &cookie, &format!("csrf={csrf}&token={TOKEN}"), ORIGIN, HOST)).await.unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let store = fixture.state.browser.inner.lock().await;
    assert_eq!(store.failures.len(), 12);
    assert!(store.sessions.is_empty());
    drop(store);
    let logs = fixture.state.audit.rows.lock().unwrap().join("\n");
    assert!(!logs.contains(private));
    assert!(!logs.contains(TOKEN));
}

#[tokio::test]
async fn logout_requires_csrf_and_revokes_the_server_side_session() {
    let fixture = Fixture::new();
    let cookie = fixture.login().await;
    let csrf = fixture.state.browser.inner.lock().await.sessions.values().next().unwrap().csrf.clone();
    for (body, origin) in [("csrf=wrong".to_owned(), ORIGIN), (format!("csrf={csrf}"), "https://foreign.example")] {
        let response = fixture.app().oneshot(form_request("/workspace/logout", &cookie, &body, origin, HOST)).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    let response = fixture.app().oneshot(form_request("/workspace/logout", &cookie, &format!("csrf={csrf}"), ORIGIN, HOST)).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()[header::LOCATION], "/workspace/login");
    assert!(response.headers()[header::SET_COOKIE].to_str().unwrap().contains("Max-Age=0"));
    let response = fixture
        .app()
        .oneshot(Request::builder().uri("/workspace/api").header(header::HOST, HOST).header(header::COOKIE, cookie).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(fixture.state.browser.inner.lock().await.sessions.is_empty());
}

#[tokio::test]
async fn expired_cookie_sessions_cannot_access_the_snapshot() {
    let fixture = Fixture::new();
    let cookie = fixture.login().await;
    for session in fixture.state.browser.inner.lock().await.sessions.values_mut() {
        session.expires = Instant::now() - Duration::from_secs(1);
    }
    let response = fixture
        .app()
        .oneshot(Request::builder().uri("/workspace/api").header(header::HOST, HOST).header(header::COOKIE, cookie).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(fixture.state.browser.inner.lock().await.sessions.is_empty());
}

#[tokio::test]
async fn challenge_eviction_bounds_memory_without_locking_out_new_login_pages() {
    let fixture = Fixture::new();
    for _ in 0..65 {
        let _ = fixture.challenge().await;
    }
    assert_eq!(fixture.state.browser.inner.lock().await.challenges.len(), 64);
    let _ = fixture.login().await;
    assert_eq!(fixture.state.browser.inner.lock().await.sessions.len(), 1);
}

#[tokio::test]
async fn malformed_duplicate_and_oversized_login_forms_cannot_create_sessions() {
    let fixture = Fixture::new();
    let (csrf, cookie) = fixture.challenge().await;
    for body in [format!("csrf={csrf}&csrf={csrf}&token={TOKEN}"), "csrf=%ZZ&token=invalid".into(), "x".repeat(4097)] {
        let response = fixture.app().oneshot(form_request("/workspace/login", &cookie, &body, ORIGIN, HOST)).await.unwrap();
        assert!(matches!(response.status(), StatusCode::BAD_REQUEST | StatusCode::PAYLOAD_TOO_LARGE));
    }
    assert!(fixture.state.browser.inner.lock().await.sessions.is_empty());
}

#[tokio::test]
async fn opening_another_tab_preserves_the_first_tabs_challenge_and_expiry() {
    let fixture = Fixture::new();
    let (csrf, cookie) = fixture.challenge().await;
    let expires = *fixture.state.browser.inner.lock().await.challenges.get(&csrf).unwrap();
    let response = fixture
        .app()
        .oneshot(Request::builder().uri("/workspace/login").header(header::HOST, HOST).header(header::COOKIE, &cookie).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert!(body_text(response).await.contains(&format!("name=\"csrf\" value=\"{csrf}\"")));
    assert_eq!(fixture.state.browser.inner.lock().await.challenges.get(&csrf), Some(&expires));
    let response =
        fixture.app().oneshot(form_request("/workspace/login", &cookie, &format!("csrf={csrf}&token={TOKEN}"), ORIGIN, HOST)).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn pre_token_rejections_explain_and_log_only_fixed_reasons() {
    for (reason, origin, host, send_cookie, wrong_csrf, expired) in [
        ("origin_not_allowed", "https://private-origin-marker.example", HOST, true, false, false),
        ("origin_missing", "", HOST, true, false, false),
        ("host_mismatch", ORIGIN, "private-host-marker.example", true, false, false),
        ("csrf_cookie_missing", ORIGIN, HOST, false, false, false),
        ("csrf_mismatch", ORIGIN, HOST, true, true, false),
        ("csrf_expired", ORIGIN, HOST, true, false, true),
    ] {
        let fixture = Fixture::new();
        let (csrf, cookie) = fixture.challenge().await;
        if expired {
            fixture.state.browser.inner.lock().await.challenges.insert(csrf.clone(), Instant::now() - Duration::from_secs(1));
        }
        let submitted = if wrong_csrf { "private-csrf-marker" } else { &csrf };
        let mut request =
            form_request("/workspace/login", if send_cookie { &cookie } else { "" }, &format!("csrf={submitted}&token={TOKEN}"), origin, host);
        if origin.is_empty() {
            request.headers_mut().remove(header::ORIGIN);
        }
        let response = fixture.app().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{reason}");
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let body = body_text(response).await;
        assert!(body.contains("Anmeldung neu laden"), "{reason}");
        assert!(body.contains("noch nicht geprüft"));
        assert!(fixture.state.browser.inner.lock().await.sessions.is_empty());
        let rows = fixture.state.audit.rows.lock().unwrap();
        assert_eq!(rows.len(), 1, "{reason}");
        assert!(rows[0].contains("auth.rejected"));
        assert!(rows[0].contains(&format!("\"reason\":\"{reason}\"")));
        assert!(rows[0].contains("\"endpoint\":\"workspace\""));
        for private in [TOKEN, &csrf, &cookie, "private-origin-marker", "private-host-marker", "private-csrf-marker"] {
            assert!(!body.contains(private), "HTML reflected {private}");
            assert!(!rows[0].contains(private), "log reflected {private}");
        }
    }
}

#[tokio::test]
async fn an_expired_cookie_gets_a_fresh_challenge_without_extending_the_old_one() {
    let fixture = Fixture::new();
    let (csrf, cookie) = fixture.challenge().await;
    fixture.state.browser.inner.lock().await.challenges.insert(csrf.clone(), Instant::now() - Duration::from_secs(1));
    let response = fixture
        .app()
        .oneshot(Request::builder().uri("/workspace/login").header(header::HOST, HOST).header(header::COOKIE, cookie).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(!body_text(response).await.contains(&csrf));
    let store = fixture.state.browser.inner.lock().await;
    assert!(!store.challenges.contains_key(&csrf));
    assert_eq!(store.challenges.len(), 1);
    assert!(fixture.state.audit.rows.lock().unwrap().is_empty());
}
