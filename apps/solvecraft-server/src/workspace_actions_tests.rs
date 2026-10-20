//! Disposable filesystem fixtures only. These tests never mount user data.
use super::*;
use axum::body::HttpBody;
use axum::http::HeaderMap;

fn headers(cookie: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::COOKIE, cookie.parse().unwrap());
    headers.insert(header::HOST, HOST.parse().unwrap());
    headers
}
async fn get(f: &Fixture, cookie: &str, url: &str) -> Response {
    f.app().oneshot(Request::builder().uri(url).header(header::HOST, HOST).header(header::COOKIE, cookie).body(Body::empty()).unwrap()).await.unwrap()
}
async fn confirmation(f: &Fixture, cookie: &str, path: &str) -> (String, String) {
    let response = get(f, cookie, &format!("/workspace/delete/{path}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let text = body_text(response).await;
    let ticket = text.split("name=\"ticket\" value=\"").nth(1).unwrap().split('"').next().unwrap().to_owned();
    let csrf = crate::workspace::browser_session(&f.state, &headers(cookie)).await.unwrap();
    (ticket, csrf)
}
async fn delete(f: &Fixture, cookie: &str, path: &str, ticket: &str, csrf: &str, origin: &str) -> Response {
    f.app()
        .oneshot(form_request("/workspace/delete", cookie, &format!("csrf={csrf}&ticket={ticket}&path={path}&confirm=delete"), origin, HOST))
        .await
        .unwrap()
}

#[tokio::test]
async fn deletion_requires_cookie_csrf_origin_confirmation_and_unchanged_contents() {
    let f = Fixture::new();
    let cookie = f.login().await;
    std::fs::create_dir_all(f.directory.join("Project/empty")).unwrap();
    std::fs::write(f.directory.join("Project/a.txt"), b"before").unwrap();
    let (ticket, csrf) = confirmation(&f, &cookie, "Project").await;
    assert_eq!(delete(&f, &cookie, "Project", &ticket, "bad", ORIGIN).await.status(), StatusCode::FORBIDDEN);
    assert_eq!(delete(&f, &cookie, "Project", &ticket, &csrf, "https://attacker.example").await.status(), StatusCode::FORBIDDEN);
    assert_eq!(delete(&f, "", "Project", &ticket, &csrf, ORIGIN).await.status(), StatusCode::UNAUTHORIZED);
    std::fs::write(f.directory.join("Project/a.txt"), b"after!").unwrap();
    assert_eq!(delete(&f, &cookie, "Project", &ticket, &csrf, ORIGIN).await.status(), StatusCode::CONFLICT);
    assert!(f.directory.join("Project/a.txt").exists());
    let (ticket, csrf) = confirmation(&f, &cookie, "Project").await;
    assert_eq!(delete(&f, &cookie, "Project", &ticket, &csrf, ORIGIN).await.status(), StatusCode::SEE_OTHER);
    assert!(!f.directory.join("Project").exists());
    assert_eq!(delete(&f, &cookie, "Project", &ticket, &csrf, ORIGIN).await.status(), StatusCode::CONFLICT);
    assert!(f.directory.join("public&example.solidcraft").exists());
}

#[tokio::test]
async fn single_file_delete_does_not_delete_virtual_siblings_and_confirmation_is_cookie_bound() {
    let f = Fixture::new();
    let cookie = f.login().await;
    let other = f.login().await;
    std::fs::write(f.directory.join("same.png"), b"png").unwrap();
    std::fs::write(f.directory.join("same.step"), b"step").unwrap();
    let (ticket, csrf) = confirmation(&f, &cookie, "same.png").await;
    let other_csrf = crate::workspace::browser_session(&f.state, &headers(&other)).await.unwrap();
    assert_eq!(delete(&f, &other, "same.png", &ticket, &other_csrf, ORIGIN).await.status(), StatusCode::CONFLICT);
    assert_eq!(delete(&f, &cookie, "same.png", &ticket, &csrf, ORIGIN).await.status(), StatusCode::SEE_OTHER);
    assert!(f.directory.join("same.step").exists());
    assert!(!f.directory.join("same.png").exists());
    assert_eq!(get(&f, &cookie, "/workspace/archive/same").await.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn active_projects_and_late_sessions_prevent_deletion_without_ending_cad() {
    let mut f = Fixture::new();
    Arc::get_mut(&mut f.state).unwrap().config.cli = "/bin/cat".into();
    let cookie = f.login().await;
    std::fs::create_dir(f.directory.join("Project")).unwrap();
    std::fs::write(f.directory.join("Project/a.txt"), b"public").unwrap();
    let (ticket, csrf) = confirmation(&f, &cookie, "Project").await;
    let (id, worker, _) = f.state.create("2025-06-18".into()).await.unwrap();
    f.state.sessions.lock().await.get(&id).unwrap().model.lock().unwrap().project = Some("Project".into());
    assert_eq!(delete(&f, &cookie, "Project", &ticket, &csrf, ORIGIN).await.status(), StatusCode::CONFLICT);
    assert_eq!(get(&f, &cookie, "/workspace/delete/Project/a.txt").await.status(), StatusCode::CONFLICT);
    assert!(f.state.sessions.lock().await.contains_key(&id));
    assert!(f.directory.join("Project/a.txt").exists());
    f.state.remove(&id, crate::sessions::EndReason::Client).await;
    drop(worker);
}

#[tokio::test]
async fn pending_write_gate_is_shared_and_a_stale_delete_cannot_win_after_a_write() {
    let f = Fixture::new();
    let cookie = f.login().await;
    std::fs::write(f.directory.join("race.txt"), b"base").unwrap();
    let (ticket, csrf) = confirmation(&f, &cookie, "race.txt").await;
    let gate = f.state.file_gate.lock().await;
    let operation = delete(&f, &cookie, "race.txt", &ticket, &csrf, ORIGIN);
    let change = async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        std::fs::write(f.directory.join("race.txt"), b"new!").unwrap();
        drop(gate);
    };
    let (response, ()) = tokio::join!(operation, change);
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(std::fs::read(f.directory.join("race.txt")).unwrap(), b"new!");
}

#[tokio::test]
async fn zip_preserves_unicode_nested_and_empty_entries_and_rejects_bearer_and_foreign_source() {
    let f = Fixture::new();
    let cookie = f.login().await;
    std::fs::create_dir_all(f.directory.join("Project/empty/deeper")).unwrap();
    std::fs::write(f.directory.join("Project/Grüße.txt"), b"public unicode").unwrap();
    std::fs::write(f.directory.join("Project/empty/zero.txt"), b"").unwrap();
    assert_eq!(get(&f, "", "/workspace/archive/Project").await.status(), StatusCode::UNAUTHORIZED);
    let rejected = f
        .app()
        .oneshot(
            Request::builder()
                .uri("/workspace/archive/Project")
                .header(header::HOST, HOST)
                .header(header::COOKIE, &cookie)
                .header(header::ORIGIN, "https://attacker.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::FORBIDDEN);
    let response = get(&f, &cookie, "/workspace/archive/Project").await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    assert_eq!(zip.len(), 5);
    assert!(zip.by_name("Project/empty/deeper/").unwrap().is_dir());
    assert_eq!(zip.by_name("Project/empty/zero.txt").unwrap().size(), 0);
    let mut text = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("Project/Grüße.txt").unwrap(), &mut text).unwrap();
    assert_eq!(text, "public unicode");
}

#[tokio::test]
async fn zip_begins_early_bounds_buffers_blocks_only_its_paths_and_releases_on_abort() {
    let f = Fixture::new();
    let cookie = f.login().await;
    std::fs::create_dir(f.directory.join("Large")).unwrap();
    std::fs::File::create(f.directory.join("Large/big.bin")).unwrap().set_len(128 << 20).unwrap();
    let response = get(&f, &cookie, "/workspace/archive/Large").await;
    let mut body = response.into_body();
    let first = std::future::poll_fn(|cx| std::pin::Pin::new(&mut body).poll_frame(cx)).await.unwrap().unwrap();
    assert!(first.into_data().unwrap().len() <= 32 * 1024);
    assert!(!f.state.browser.actions.transfers.lock().unwrap().is_empty(), "transfer must still be in progress after first bytes");
    assert!(crate::workspace_actions::check_transfers(&f.state.browser.actions.transfers, "Large/other.txt").is_err());
    assert!(crate::workspace_actions::check_transfers(&f.state.browser.actions.transfers, "Other/free.txt").is_ok());
    assert_eq!(get(&f, &cookie, "/workspace/delete/Large").await.status(), StatusCode::CONFLICT);
    drop(body);
    for _ in 0..100 {
        if f.state.browser.actions.transfers.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(f.state.browser.actions.transfers.lock().unwrap().is_empty());
    assert_eq!(f.state.browser.actions.slots.available_permits(), 2);
}

#[cfg(unix)]
#[tokio::test]
async fn manifests_reject_root_traversal_symlinks_reserved_and_special_entries() {
    let f = Fixture::new();
    let cookie = f.login().await;
    std::fs::create_dir(f.directory.join("Project")).unwrap();
    std::os::unix::fs::symlink("/etc/passwd", f.directory.join("Project/escape")).unwrap();
    for content in [false, true] {
        assert!(crate::workspace_actions::manifest::inspect(&f.directory, "Project", content).is_err());
    }
    assert_eq!(get(&f, &cookie, "/workspace/archive/Project").await.status(), StatusCode::CONFLICT);
    std::fs::remove_file(f.directory.join("Project/escape")).unwrap();
    std::fs::write(f.directory.join("Project/.secret"), b"must never be deleted or archived").unwrap();
    for name in ["Project", "", ".", "..", "Project/..", "/etc/passwd", "Project\\escape"] {
        assert!(crate::workspace_actions::manifest::inspect(&f.directory, name, true).is_err(), "{name}");
    }
    assert!(f.directory.join("Project/.secret").exists());
}

#[tokio::test]
async fn interrupted_or_revoked_download_never_reports_clean_completion() {
    let f = Fixture::new();
    let cookie = f.login().await;
    std::fs::create_dir(f.directory.join("Changing")).unwrap();
    let path = f.directory.join("Changing/large.bin");
    std::fs::File::create(&path).unwrap().set_len(4 << 20).unwrap();
    let response = get(&f, &cookie, "/workspace/archive/Changing").await;
    std::fs::write(&path, b"external replacement").unwrap();
    assert!(to_bytes(response.into_body(), 8 << 20).await.is_err());
    for _ in 0..100 {
        if f.state.browser.actions.slots.available_permits() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let response = get(&f, &cookie, "/workspace/archive/Changing").await;
    let csrf = crate::workspace::browser_session(&f.state, &headers(&cookie)).await.unwrap();
    assert_eq!(
        f.app().oneshot(form_request("/workspace/logout", &cookie, &format!("csrf={csrf}"), ORIGIN, HOST)).await.unwrap().status(),
        StatusCode::SEE_OTHER
    );
    assert!(to_bytes(response.into_body(), 8 << 20).await.is_err());
}

#[tokio::test]
async fn expired_confirmation_is_rejected_and_live_inventory_tracks_empty_folder_deletion() {
    let f = Fixture::new();
    let cookie = f.login().await;
    let mut updates = f.state.browser.live.subscribe(&f.state).await;
    std::fs::create_dir(f.directory.join("Empty")).unwrap();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            updates.changed().await.unwrap();
            if let crate::workspace_live::Update::Snapshot(snapshot) = updates.borrow_and_update().clone()
                && snapshot["directories"].as_array().unwrap().iter().any(|name| name == "Empty")
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    let (ticket, csrf) = confirmation(&f, &cookie, "Empty").await;
    f.state.browser.actions.expire_confirmations().await;
    assert_eq!(delete(&f, &cookie, "Empty", &ticket, &csrf, ORIGIN).await.status(), StatusCode::CONFLICT);
    assert!(f.directory.join("Empty").is_dir());
    let (ticket, csrf) = confirmation(&f, &cookie, "Empty").await;
    assert_eq!(delete(&f, &cookie, "Empty", &ticket, &csrf, ORIGIN).await.status(), StatusCode::SEE_OTHER);
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            updates.changed().await.unwrap();
            if let crate::workspace_live::Update::Snapshot(snapshot) = updates.borrow_and_update().clone()
                && !snapshot["directories"].as_array().unwrap().iter().any(|name| name == "Empty")
            {
                break;
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn delete_forms_have_a_small_transport_body_limit() {
    let f = Fixture::new();
    let cookie = f.login().await;
    let response = f.app().oneshot(form_request("/workspace/delete", &cookie, &"x".repeat(4097), ORIGIN, HOST)).await.unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}
