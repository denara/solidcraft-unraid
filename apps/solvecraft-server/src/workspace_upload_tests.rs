//! Original public-safe data and disposable directories only.
use super::*;
use serde_json::json;
const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0L20 0L20 10Z"/></svg>"#;
async fn csrf(f: &Fixture, cookie: &str) -> String {
    let mut h = HeaderMap::new();
    h.insert(header::COOKIE, cookie.parse().unwrap());
    crate::workspace::browser_session(&f.state, &h).await.unwrap()
}
async fn post(f: &Fixture, cookie: &str, token: &str, url: &str, value: Value) -> Response {
    f.app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(url)
                .header(header::HOST, HOST)
                .header(header::ORIGIN, ORIGIN)
                .header(header::COOKIE, cookie)
                .header("x-workspace-csrf", token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(value.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}
async fn setup(f: &Fixture) -> (String, String) {
    let c = f.login().await;
    let t = csrf(f, &c).await;
    let r = post(f, &c, &t, "/workspace/projects", json!({"name":"Project"})).await;
    assert_eq!(r.status(), StatusCode::OK);
    (c, t)
}
async fn prepare(f: &Fixture, c: &str, t: &str, files: Value) -> Value {
    let r = post(f, c, t, "/workspace/uploads/prepare", json!({"project":"Project","files":files})).await;
    let status = r.status();
    let text = body_text(r).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    serde_json::from_str(&text).unwrap()
}
fn request(c: &str, t: &str, batch: &Value, index: usize, overwrite: bool, body: Body) -> Request<Body> {
    let mut req = Request::builder()
        .method("PUT")
        .uri("/workspace/upload")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, ORIGIN)
        .header(header::COOKIE, c)
        .header("x-workspace-csrf", t)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header("x-upload-batch", batch["batch"].as_str().unwrap())
        .header("x-upload-index", index.to_string());
    if let Some(revision) = batch["files"][index]["revision"].as_str() {
        req = req.header("if-match", revision);
        if overwrite {
            req = req.header("x-upload-overwrite", "true");
        }
    } else {
        req = req.header("if-none-match", "*");
    }
    req.body(body).unwrap()
}
#[tokio::test]
async fn projects_use_browser_auth_csrf_same_names_and_show_empty_directories() {
    let f = Fixture::new();
    let c = f.login().await;
    let t = csrf(&f, &c).await;
    assert_eq!(post(&f, "", &t, "/workspace/projects", json!({"name":"No"})).await.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(post(&f, &c, "bad", "/workspace/projects", json!({"name":"No"})).await.status(), StatusCode::FORBIDDEN);
    for name in ["../escape", ".secret", "a/b", "CON", " Project"] {
        assert_eq!(post(&f, &c, &t, "/workspace/projects", json!({"name":name})).await.status(), StatusCode::BAD_REQUEST);
    }
    assert_eq!(post(&f, &c, &t, "/workspace/projects", json!({"name":"Öffentliche Vorlage"})).await.status(), StatusCode::OK);
    assert_eq!(post(&f, &c, &t, "/workspace/projects", json!({"name":"Öffentliche Vorlage"})).await.status(), StatusCode::CONFLICT);
    let data = crate::workspace::snapshot(&f.state).await.unwrap();
    assert!(data["directories"].as_array().unwrap().contains(&json!("Öffentliche Vorlage")));
    let html = crate::workspace_view::page(&data, "", "");
    assert!(html.contains("Für alle Dateien übernehmen"));
    assert!(html.contains("Öffentliche Vorlage"));
    assert!(html.contains("Speicher frei:"));
    assert!(!html.contains(&f.directory.to_string_lossy().to_string()));
}
#[tokio::test]
async fn uploads_require_explicit_owner_bound_overwrite_and_stale_revisions_never_win() {
    let f = Fixture::new();
    let (c, t) = setup(&f).await;
    let files = json!([{"name":"Muster.svg","size":SVG.len()}]);
    let batch = prepare(&f, &c, &t, files.clone()).await;
    assert_eq!(f.app().oneshot(request(&c, &t, &batch, 0, false, Body::from(SVG))).await.unwrap().status(), StatusCode::OK);
    assert_eq!(std::fs::read(f.directory.join("Project/Muster.svg")).unwrap(), SVG);
    let batch = prepare(&f, &c, &t, files.clone()).await;
    assert_eq!(f.app().oneshot(request(&c, &t, &batch, 0, false, Body::from(SVG))).await.unwrap().status(), StatusCode::CONFLICT);
    let other = f.login().await;
    let other_csrf = csrf(&f, &other).await;
    assert_eq!(f.app().oneshot(request(&other, &other_csrf, &batch, 0, true, Body::from(SVG))).await.unwrap().status(), StatusCode::CONFLICT);
    std::fs::write(f.directory.join("Project/Muster.svg"), b"newer external work").unwrap();
    assert_eq!(f.app().oneshot(request(&c, &t, &batch, 0, true, Body::from(SVG))).await.unwrap().status(), StatusCode::CONFLICT);
    assert_eq!(std::fs::read(f.directory.join("Project/Muster.svg")).unwrap(), b"newer external work");
    let fresh = prepare(&f, &c, &t, files).await;
    assert_eq!(f.app().oneshot(request(&c, &t, &fresh, 0, true, Body::from(SVG))).await.unwrap().status(), StatusCode::OK);
    assert_eq!(std::fs::read(f.directory.join("Project/Muster.svg")).unwrap(), SVG);
    assert_eq!(f.app().oneshot(request(&c, &t, &fresh, 0, true, Body::from(SVG))).await.unwrap().status(), StatusCode::CONFLICT);
}
#[tokio::test]
async fn content_policy_size_names_duplicates_and_batch_cancellation_are_enforced() {
    let f = Fixture::new();
    let (c, t) = setup(&f).await;
    for files in [
        json!([]),
        json!([{"name":"../bad.svg","size":4}]),
        json!([{"name":"run.html","size":4}]),
        json!([{"name":"a.svg","size":16777217}]),
        json!([{"name":"a.svg","size":2},{"name":"A.svg","size":2}]),
    ] {
        assert_ne!(post(&f, &c, &t, "/workspace/uploads/prepare", json!({"project":"Project","files":files})).await.status(), StatusCode::OK);
    }
    let batch = prepare(&f, &c, &t, json!([{"name":"fake.png","size":SVG.len()},{"name":"Muster.svg","size":SVG.len()}])).await;
    assert_eq!(f.app().oneshot(request(&c, &t, &batch, 0, false, Body::from(SVG))).await.unwrap().status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(!f.directory.join("Project/fake.png").exists());
    assert_eq!(post(&f, &c, &t, "/workspace/uploads/cancel", json!({"batch":batch["batch"]})).await.status(), StatusCode::OK);
    assert_eq!(f.app().oneshot(request(&c, &t, &batch, 1, false, Body::from(SVG))).await.unwrap().status(), StatusCode::CONFLICT);
    assert_eq!(std::fs::read_dir(&f.state.browser.uploads.staging).unwrap().count(), 1, "only the private ownership marker remains");
}
#[tokio::test]
async fn leases_confine_streams_and_abort_leaves_no_partial_file() {
    use tokio::io::AsyncWriteExt;
    let f = Fixture::new();
    let (c, t) = setup(&f).await;
    let batch = prepare(&f, &c, &t, json!([{"name":"Muster.svg","size":SVG.len()}])).await;
    let (mut writer, reader) = tokio::io::duplex(1024);
    let req = request(&c, &t, &batch, 0, false, Body::from_stream(tokio_util::io::ReaderStream::new(reader)));
    let app = f.app();
    let upload = tokio::spawn(async move { app.oneshot(req).await.unwrap() });
    writer.write_all(&SVG[..10]).await.unwrap();
    for _ in 0..50 {
        if f.state.browser.actions.transfers.lock().unwrap().values().any(|p| p == "Project/Muster.svg") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(crate::workspace_actions::check_transfers(&f.state.browser.actions.transfers, "Project").is_err());
    assert!(crate::workspace_actions::check_transfers(&f.state.browser.actions.transfers, "Other/file.svg").is_ok());
    assert!(!f.directory.join("Project/Muster.svg").exists());
    drop(writer);
    assert_eq!(upload.await.unwrap().status(), StatusCode::BAD_REQUEST);
    assert!(crate::workspace_actions::check_transfers(&f.state.browser.actions.transfers, "Project").is_ok());
    assert_eq!(std::fs::read_dir(&f.state.browser.uploads.staging).unwrap().count(), 1);
}
#[tokio::test]
async fn late_mcp_session_and_logout_stop_publication_without_ending_cad() {
    use tokio::io::AsyncWriteExt;
    let mut f = Fixture::new();
    Arc::get_mut(&mut f.state).unwrap().config.cli = "/bin/cat".into();
    let (c, t) = setup(&f).await;
    let batch = prepare(&f, &c, &t, json!([{"name":"Muster.svg","size":SVG.len()}])).await;
    let (mut writer, reader) = tokio::io::duplex(1024);
    let req = request(&c, &t, &batch, 0, false, Body::from_stream(tokio_util::io::ReaderStream::new(reader)));
    let app = f.app();
    let upload = tokio::spawn(async move { app.oneshot(req).await.unwrap() });
    writer.write_all(&SVG[..10]).await.unwrap();
    for _ in 0..50 {
        if !f.state.browser.actions.transfers.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let (id, worker, _) = f.state.create("2025-06-18".into()).await.unwrap();
    f.state.sessions.lock().await.get(&id).unwrap().model.lock().unwrap().project = Some("Project".into());
    writer.write_all(&SVG[10..]).await.unwrap();
    drop(writer);
    assert_eq!(upload.await.unwrap().status(), StatusCode::CONFLICT);
    assert!(!f.directory.join("Project/Muster.svg").exists());
    assert!(f.state.sessions.lock().await.contains_key(&id));
    f.state.remove(&id, crate::sessions::EndReason::Client).await;
    drop(worker);
    let batch = prepare(&f, &c, &t, json!([{"name":"Muster.svg","size":SVG.len()}])).await;
    let (mut writer, reader) = tokio::io::duplex(1024);
    let app = f.app();
    let req = request(&c, &t, &batch, 0, false, Body::from_stream(tokio_util::io::ReaderStream::new(reader)));
    let upload = tokio::spawn(async move { app.oneshot(req).await.unwrap() });
    writer.write_all(&SVG[..10]).await.unwrap();
    for _ in 0..50 {
        if !f.state.browser.actions.transfers.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let _ = f.app().oneshot(form_request("/workspace/logout", &c, &format!("csrf={t}"), ORIGIN, HOST)).await.unwrap();
    writer.write_all(&SVG[10..]).await.unwrap();
    drop(writer);
    assert_eq!(upload.await.unwrap().status(), StatusCode::CONFLICT);
    assert!(!f.directory.join("Project/Muster.svg").exists());
}
#[cfg(unix)]
#[tokio::test]
async fn symbolic_projects_and_destinations_cannot_escape() {
    let f = Fixture::new();
    let (c, t) = setup(&f).await;
    std::os::unix::fs::symlink(&f.directory, f.directory.join("Link")).unwrap();
    assert_ne!(
        post(&f, &c, &t, "/workspace/uploads/prepare", json!({"project":"Link","files":[{"name":"a.svg","size":SVG.len()}]})).await.status(),
        StatusCode::OK
    );
    std::os::unix::fs::symlink(f.directory.join("public&example.solidcraft"), f.directory.join("Project/link.svg")).unwrap();
    assert_ne!(
        post(&f, &c, &t, "/workspace/uploads/prepare", json!({"project":"Project","files":[{"name":"link.svg","size":SVG.len()}]})).await.status(),
        StatusCode::OK
    );
    assert_eq!(std::fs::read(f.directory.join("public&example.solidcraft")).unwrap(), b"public test file");
}
#[test]
fn data_only_formats_reject_active_content_wrong_signatures_and_external_companions() {
    use crate::workspace_upload::formats::validate;
    assert!(validate("a.svg", SVG).is_ok());
    for name in ["a.sc", "a.SC", "a.solidcraft", "a.solvecraft"] {
        assert!(validate(name, br#"{"format":"solidcraft/41","features":[]}"#).is_ok());
        assert!(validate(name, b"println(1); system(\"exec\")").is_err());
        assert!(validate(name, br#"{"commands":[{"command":"file.new"}]}"#).is_err());
    }
    for (name, bytes) in [
        ("a.svg", b"<svg><script>alert(1)</script></svg>".as_slice()),
        ("a.png", SVG),
        ("a.json", br#"{"commands":[{"command":"file.new"}]}"#),
        ("a.gltf", br#"{"asset":{"version":"2.0"},"buffers":[{"uri":"https://example.invalid/remote.bin"}]}"#),
        ("a.obj", b"mtllib a.mtl\nv 0 0 0\nf 1 1 1"),
        ("a.html", SVG),
    ] {
        assert!(validate(name, bytes).is_err(), "{name}");
    }
    assert!(validate("a.step", b"ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\nENDSEC;\nEND-ISO-10303-21;").is_ok());
    assert!(validate("a.gltf", br#"{"asset":{"version":"2.0"},"buffers":[{"uri":"data:application/octet-stream;base64,AAAA"}]}"#).is_ok());
}

#[tokio::test]
async fn cancelling_an_active_batch_prevents_atomic_publication() {
    use tokio::io::AsyncWriteExt;
    let f = Fixture::new();
    let (c, t) = setup(&f).await;
    let batch = prepare(&f, &c, &t, json!([{"name":"Muster.svg","size":SVG.len()}])).await;
    let (mut writer, reader) = tokio::io::duplex(1024);
    let app = f.app();
    let req = request(&c, &t, &batch, 0, false, Body::from_stream(tokio_util::io::ReaderStream::new(reader)));
    let upload = tokio::spawn(async move { app.oneshot(req).await.unwrap() });
    writer.write_all(&SVG[..10]).await.unwrap();
    for _ in 0..50 {
        if !f.state.browser.actions.transfers.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(post(&f, &c, &t, "/workspace/projects", json!({"name":"Other"})).await.status(), StatusCode::OK);
    assert_eq!(
        post(&f, &c, &t, "/workspace/uploads/prepare", json!({"project":"Other","files":[{"name":"Other.svg","size":SVG.len()}]})).await.status(),
        StatusCode::OK
    );
    assert_eq!(post(&f, &c, &t, "/workspace/uploads/cancel", json!({"batch":batch["batch"]})).await.status(), StatusCode::OK);
    writer.write_all(&SVG[10..]).await.unwrap();
    drop(writer);
    assert_eq!(upload.await.unwrap().status(), StatusCode::CONFLICT);
    assert!(!f.directory.join("Project/Muster.svg").exists());
    assert!(f.state.browser.actions.transfers.lock().unwrap().is_empty());
}

#[tokio::test]
async fn zip_leases_and_two_stream_capacity_cannot_be_bypassed_by_upload_tickets() {
    use tokio::io::AsyncWriteExt;
    let f = Fixture::new();
    let (c, t) = setup(&f).await;
    let files = json!([{"name":"A.svg","size":SVG.len()},{"name":"B.svg","size":SVG.len()},{"name":"C.svg","size":SVG.len()}]);
    f.state.browser.actions.transfers.lock().unwrap().insert("zip-proof".into(), "Project".into());
    assert_eq!(post(&f, &c, &t, "/workspace/uploads/prepare", json!({"project":"Project","files":files})).await.status(), StatusCode::CONFLICT);
    f.state.browser.actions.transfers.lock().unwrap().clear();
    let batch = prepare(&f, &c, &t, files).await;
    let mut streams = Vec::new();
    for index in 0..2 {
        let (mut writer, reader) = tokio::io::duplex(1024);
        let req = request(&c, &t, &batch, index, false, Body::from_stream(tokio_util::io::ReaderStream::new(reader)));
        let app = f.app();
        let upload = tokio::spawn(async move { app.oneshot(req).await.unwrap() });
        writer.write_all(&SVG[..10]).await.unwrap();
        streams.push((writer, upload));
    }
    for _ in 0..50 {
        if f.state.browser.actions.transfers.lock().unwrap().len() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(f.app().oneshot(request(&c, &t, &batch, 2, false, Body::from(SVG))).await.unwrap().status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(crate::workspace_actions::check_transfers(&f.state.browser.actions.transfers, "Project").is_err());
    for (mut writer, upload) in streams {
        writer.write_all(&SVG[10..]).await.unwrap();
        drop(writer);
        assert_eq!(upload.await.unwrap().status(), StatusCode::OK);
    }
    assert_eq!(f.app().oneshot(request(&c, &t, &batch, 2, false, Body::from(SVG))).await.unwrap().status(), StatusCode::OK);
    assert!(f.state.browser.actions.transfers.lock().unwrap().is_empty());
}
