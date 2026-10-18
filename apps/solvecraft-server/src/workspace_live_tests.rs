//! Integration coverage for the actual streaming HTTP route and shared monitor.
use super::*;

fn event_request(cookie: Option<&str>, uri: &str) -> Request<Body> {
    let mut request = Request::builder().uri(uri).header(header::HOST, HOST);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    request.body(Body::empty()).unwrap()
}

#[tokio::test]
async fn event_route_is_cookie_only_and_rejects_cross_origin_sources() {
    let fixture = Fixture::new();
    let response = fixture.app().oneshot(event_request(None, "/workspace/events")).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let mut request = event_request(None, "/workspace/events");
    request.headers_mut().insert(header::AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {TOKEN}")).unwrap());
    assert_eq!(fixture.app().oneshot(request).await.unwrap().status(), StatusCode::NO_CONTENT);

    let cookie = fixture.login().await;
    for (key, value) in [(header::ORIGIN, "https://other.example"), (header::HeaderName::from_static("sec-fetch-site"), "cross-site")] {
        let mut request = event_request(Some(&cookie), "/workspace/events");
        request.headers_mut().insert(key, HeaderValue::from_static(value));
        assert_eq!(fixture.app().oneshot(request).await.unwrap().status(), StatusCode::FORBIDDEN);
    }
    assert_eq!(
        fixture.app().oneshot(event_request(Some(&cookie), "/workspace/events?kind=invalid")).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(fixture.app().oneshot(event_request(Some(&cookie), "/workspace/events?q=one&q=two")).await.unwrap().status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn logout_closes_authenticated_event_stream_and_never_exposes_credentials() {
    let fixture = Fixture::new();
    let cookie = fixture.login().await;
    let response = fixture.app().oneshot(event_request(Some(&cookie), "/workspace/events")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "text/event-stream; charset=utf-8");
    assert_eq!(response.headers()["x-accel-buffering"], "no");
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store, no-transform");
    let body = tokio::spawn(body_text(response));
    tokio::time::sleep(Duration::from_millis(100)).await;
    fixture.state.browser.inner.lock().await.sessions.clear();
    let text = tokio::time::timeout(Duration::from_secs(3), body).await.unwrap().unwrap();
    assert!(text.contains("event: workspace") && text.contains("event: expired"));
    assert!(text.contains("public&amp;example.solidcraft"));
    assert!(!text.contains(TOKEN));
    assert!(!text.contains(&cookie));
    assert!(!text.contains(&fixture.directory.to_string_lossy().to_string()));
}

#[tokio::test]
async fn expired_cookie_ends_an_idle_stream_and_does_not_reconnect() {
    let fixture = Fixture::new();
    let cookie = fixture.login().await;
    let response = fixture.app().oneshot(event_request(Some(&cookie), "/workspace/events")).await.unwrap();
    let body = tokio::spawn(body_text(response));
    for session in fixture.state.browser.inner.lock().await.sessions.values_mut() {
        session.expires = Instant::now();
    }
    let text = tokio::time::timeout(Duration::from_secs(3), body).await.unwrap().unwrap();
    assert!(text.contains("event: expired"));
    assert_eq!(fixture.app().oneshot(event_request(Some(&cookie), "/workspace/events")).await.unwrap().status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn graceful_stop_closes_stream_without_revoking_authentication_or_leaking_a_slot() {
    let fixture = Fixture::new();
    let cookie = fixture.login().await;
    let response = fixture.app().oneshot(event_request(Some(&cookie), "/workspace/events")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(fixture.state.browser.live.streams_used(), 1);
    let body = tokio::spawn(body_text(response));
    // Exercise an established idle stream, rather than only admission after stop.
    tokio::time::sleep(Duration::from_millis(100)).await;
    fixture.state.browser.live.stop();
    let text = tokio::time::timeout(Duration::from_secs(1), body).await.unwrap().unwrap();
    assert!(!text.contains("event: expired"));
    assert!(!text.contains(TOKEN) && !text.contains(&cookie));
    assert_eq!(fixture.state.browser.live.streams_used(), 0);
    let valid_request = event_request(Some(&cookie), "/workspace/events");
    assert!(browser_session(&fixture.state, valid_request.headers()).await.is_some());
    let rejected = fixture.app().oneshot(event_request(Some(&cookie), "/workspace/events")).await.unwrap();
    assert_eq!(rejected.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(rejected.headers()[header::CACHE_CONTROL], "no-store");
}

#[tokio::test]
async fn shared_monitor_sends_actual_changes_and_coalesces_idle_snapshots() {
    let fixture = Fixture::new();
    let mut receiver = fixture.state.browser.live.subscribe(&fixture.state).await;
    tokio::time::timeout(Duration::from_secs(3), receiver.changed()).await.unwrap().unwrap();
    let first = receiver.borrow_and_update().clone();
    assert!(matches!(first, crate::workspace_live::Update::Snapshot(_)));
    assert!(tokio::time::timeout(Duration::from_millis(1200), receiver.changed()).await.is_err());
    std::fs::write(fixture.directory.join("new.step"), b"saved export").unwrap();
    tokio::time::timeout(Duration::from_secs(3), receiver.changed()).await.unwrap().unwrap();
    let next = receiver.borrow_and_update().clone();
    let crate::workspace_live::Update::Snapshot(next) = next else { panic!("snapshot expected") };
    assert!(next["files"].as_array().unwrap().iter().any(|file| file["name"] == "new.step"));
    let mut second = fixture.state.browser.live.subscribe(&fixture.state).await;
    assert!(matches!(*second.borrow_and_update(), crate::workspace_live::Update::Snapshot(_)));
    assert!(tokio::time::timeout(Duration::from_millis(1100), second.changed()).await.is_err());
    std::fs::remove_file(fixture.directory.join("new.step")).unwrap();
    tokio::time::timeout(Duration::from_secs(3), receiver.changed()).await.unwrap().unwrap();
    let next = receiver.borrow_and_update().clone();
    let crate::workspace_live::Update::Snapshot(next) = next else { panic!("snapshot expected") };
    assert!(!next["files"].as_array().unwrap().iter().any(|file| file["name"] == "new.step"));
}
