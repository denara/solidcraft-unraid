//! Real detail routes share browser-only authentication and never expose raw designs.
use super::*;

fn request(path: &str, cookie: Option<&str>, bearer: bool) -> Request<Body> {
    let mut request = Request::builder().uri(path).header(header::HOST, HOST);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    if bearer {
        request = request.header(header::AUTHORIZATION, format!("Bearer {TOKEN}"));
    }
    request.body(Body::empty()).unwrap()
}

#[tokio::test]
async fn detail_route_requires_browser_cookie_and_rejects_hidden_or_escaping_paths() {
    let fixture = Fixture::new();
    for bearer in [false, true] {
        let response = fixture.app().oneshot(request("/workspace/details/public&example.solidcraft", None, bearer)).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let body = body_text(response).await;
        assert!(!body.contains("public&example"));
        assert!(!body.contains(TOKEN));
    }
    let cookie = fixture.login().await;
    for path in [
        "/workspace/details/../private.solidcraft",
        "/workspace/details/folder/%2E%2E/private.solidcraft",
        "/workspace/details/%2Fetc%2Fpasswd",
        "/workspace/details/.workspace-sessions.json",
        "/workspace/details/.private.solidcraft",
        "/workspace/details/part.solidcraft.bak",
        "/workspace/details/missing.solidcraft",
    ] {
        let response = fixture.app().oneshot(request(path, Some(&cookie), false)).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        let body = body_text(response).await;
        assert!(!body.contains(TOKEN));
        assert!(!body.contains(&fixture.directory.to_string_lossy().to_string()));
    }
}

#[tokio::test]
async fn detail_route_renders_escaped_whitelisted_metadata_and_preserves_saved_file() {
    let fixture = Fixture::new();
    let document = json!({
        "format":"solidcraft/1","name":"Bracket <script>injected</script>","units":"mm",
        "features":[{"kind":{"type":"sketch"}},{"kind":{"type":"extrude"}}],"params":[{}],
        "components":[{}],"occurrences":[{},{}],
        "metadata":{"project":"Test project","author":"Example <author>","description":"A & B","part_number":"P-1","revision":"A","organization":"Example org","license":"CC0","org.private.tool":{"token":"private-metadata-token-marker","path":"/private/design/location"}},
        "links":{"1":{"path":"/private/link/location"}}
    });
    let bytes = serde_json::to_vec(&document).unwrap();
    let path = fixture.directory.join("bracket.solidcraft");
    std::fs::write(&path, &bytes).unwrap();
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let revision = crate::workspace_files::revision(&fixture.directory, "bracket.solidcraft").unwrap();
    let cookie = fixture.login().await;
    let response = fixture.app().oneshot(request("/workspace/details/bracket.solidcraft", Some(&cookie), false)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(response.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert!(response.headers()[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/html"));
    let html = body_text(response).await;
    for text in [
        "id=\"file-details\"",
        "Bracket &lt;script&gt;injected&lt;/script&gt;",
        "Example &lt;author&gt;",
        "A &amp; B",
        "Test project",
        "P-1",
        "solidcraft/1",
        "mm",
        "Features",
        "Skizzen",
        "Parameter",
        "Komponenten",
        "Instanzen",
        "/workspace/files/bracket.solidcraft",
    ] {
        assert!(html.contains(text), "missing {text}");
    }
    for private in [
        "<script>injected</script>",
        "private-metadata-token-marker",
        "/private/design/location",
        "/private/link/location",
        "org.private.tool",
        TOKEN,
        &cookie,
        &fixture.directory.to_string_lossy(),
    ] {
        assert!(!html.contains(private), "private data in detail response");
    }
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), modified);
    assert_eq!(crate::workspace_files::revision(&fixture.directory, "bracket.solidcraft").unwrap(), revision);
}

#[tokio::test]
async fn detail_route_reads_public_native_json_fixture_and_preserves_unknown_file_downloads() {
    let fixture = Fixture::new();
    std::fs::write(fixture.directory.join("pi-case.solidcraft"), include_bytes!("../../../tests/action_matrix/designs/pi-case.sc")).unwrap();
    let cookie = fixture.login().await;
    let response = fixture.app().oneshot(request("/workspace/details/pi-case.solidcraft", Some(&cookie), false)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let html = body_text(response).await;
    assert!(html.contains("pi-case"));
    assert!(html.contains("solidcraft/15"));
    assert!(html.contains("<dd>mm</dd>"));
    assert!(html.contains("<dt>Skizzen</dt>"));
    // Broken designs still have useful filesystem properties and a download action.
    let response = fixture.app().oneshot(request("/workspace/details/public&example.solidcraft", Some(&cookie), false)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let html = body_text(response).await;
    assert!(html.contains("nicht gelesen werden"));
    assert!(html.contains("/workspace/files/public%26example.solidcraft"));
    std::fs::write(fixture.directory.join("bracket.step"), b"STEP export").unwrap();
    let response = fixture.app().oneshot(request("/workspace/details/bracket.step", Some(&cookie), false)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(body_text(response).await.contains("keine SolidCraft-Modelleigenschaften"));
}
