//! Regression coverage for durable opaque browser sessions, not retained MCP tokens.
use super::*;

async fn remembered_login(fixture: &Fixture, remember: &str) -> String {
    let (csrf, challenge) = fixture.challenge().await;
    let response = fixture
        .app()
        .oneshot(form_request("/workspace/login", &challenge, &format!("csrf={csrf}&token={TOKEN}&remember={remember}"), ORIGIN, HOST))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let cookie =
        response.headers().get_all(header::SET_COOKIE).iter().filter_map(|v| v.to_str().ok()).find(|v| v.starts_with(&format!("{COOKIE}="))).unwrap();
    for flag in ["HttpOnly", "Secure", "SameSite=Strict", "Path=/workspace"] {
        assert!(cookie.contains(flag));
    }
    cookie.split(';').next().unwrap().to_owned()
}

fn restart(fixture: &mut Fixture) {
    let state = Arc::get_mut(&mut fixture.state).unwrap();
    state.browser = BrowserAuth::new(&state.config);
}

async fn status(fixture: &Fixture, cookie: &str) -> StatusCode {
    fixture
        .app()
        .oneshot(Request::builder().uri("/workspace/api").header(header::HOST, HOST).header(header::COOKIE, cookie).body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn remembered_choices_survive_restart_without_persisting_token_or_cookie() {
    for choice in ["7d", "30d", "forever"] {
        let mut fixture = Fixture::new();
        let cookie = remembered_login(&fixture, choice).await;
        let disk = std::fs::read_to_string(&fixture.state.config.browser_auth).unwrap();
        assert!(!disk.contains(TOKEN));
        assert!(!disk.contains(cookie.split('=').nth(1).unwrap()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&fixture.state.config.browser_auth).unwrap().permissions().mode() & 0o777, 0o600);
        }
        restart(&mut fixture);
        assert_eq!(status(&fixture, &cookie).await, StatusCode::OK);
        let csrf = fixture.state.browser.inner.lock().await.sessions.values().next().unwrap().csrf.clone();
        let response = fixture.app().oneshot(form_request("/workspace/logout", &cookie, &format!("csrf={csrf}"), ORIGIN, HOST)).await.unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        restart(&mut fixture);
        assert_eq!(status(&fixture, &cookie).await, StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn ordinary_login_ends_on_restart_and_token_rotation_revokes_remembered_login() {
    let mut fixture = Fixture::new();
    let cookie = fixture.login().await;
    restart(&mut fixture);
    assert_eq!(status(&fixture, &cookie).await, StatusCode::UNAUTHORIZED);
    let cookie = remembered_login(&fixture, "30d").await;
    Arc::get_mut(&mut fixture.state).unwrap().config.token = "rotated-private-token-01234567890123456789".into();
    restart(&mut fixture);
    assert_eq!(status(&fixture, &cookie).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn expired_or_corrupt_stores_fail_closed_and_invalid_choices_do_not_sign_in() {
    let mut fixture = Fixture::new();
    let cookie = remembered_login(&fixture, "7d").await;
    let mut data: Value = serde_json::from_slice(&std::fs::read(&fixture.state.config.browser_auth).unwrap()).unwrap();
    data["sessions"][0]["expires"] = json!(1);
    std::fs::write(&fixture.state.config.browser_auth, serde_json::to_vec(&data).unwrap()).unwrap();
    restart(&mut fixture);
    assert_eq!(status(&fixture, &cookie).await, StatusCode::UNAUTHORIZED);
    std::fs::write(&fixture.state.config.browser_auth, b"not-json").unwrap();
    restart(&mut fixture);
    assert_eq!(status(&fixture, &cookie).await, StatusCode::UNAUTHORIZED);
    let (csrf, challenge) = fixture.challenge().await;
    let response = fixture
        .app()
        .oneshot(form_request("/workspace/login", &challenge, &format!("csrf={csrf}&token={TOKEN}&remember=999d"), ORIGIN, HOST))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unavailable_private_storage_fails_remembered_login_without_false_success() {
    let fixture = Fixture::new();
    std::fs::create_dir(&fixture.state.config.browser_auth).unwrap();
    let (csrf, challenge) = fixture.challenge().await;
    let response = fixture
        .app()
        .oneshot(form_request("/workspace/login", &challenge, &format!("csrf={csrf}&token={TOKEN}&remember=7d"), ORIGIN, HOST))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(fixture.state.browser.inner.lock().await.sessions.is_empty());
}
