//! Original PNG fixtures and authenticated browser-copy handoff regressions.
use super::*;

const NATIVE: &[u8] = br#"{"format":"solidcraft/1","name":"Public browser copy","features":[],"params":[],"units":"mm"}"#;

fn request(cookie: Option<&str>, uri: &str) -> Request<Body> {
    let mut request = Request::builder().uri(uri).header(header::HOST, HOST);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    request.body(Body::empty()).unwrap()
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&vec![42; width as usize * height as usize * 3]).unwrap();
        writer.finish().unwrap();
    }
    bytes
}

async fn issue(fixture: &Fixture, cookie: &str, name: &str) -> String {
    let reply = fixture.app().oneshot(request(Some(cookie), &format!("/workspace/open/{name}"))).await.unwrap();
    assert_eq!(reply.status(), StatusCode::SEE_OTHER);
    assert_eq!(reply.headers()[header::CACHE_CONTROL], "no-store");
    let target = reply.headers()[header::LOCATION].to_str().unwrap();
    let token = target.strip_prefix("/#workspace-import=").unwrap();
    assert_eq!(token.len(), 64);
    assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert!(!target.contains(TOKEN));
    assert!(!target.contains(name));
    format!("/workspace/import/{token}")
}

#[tokio::test]
async fn png_preview_requires_browser_cookie_and_serves_bounded_pixels_with_safe_headers() {
    let fixture = Fixture::new();
    let bytes = png(2, 2);
    std::fs::write(fixture.directory.join("public-preview.png"), &bytes).unwrap();
    let path = "/workspace/preview/public-preview.png";
    for bearer in [false, true] {
        let mut request = request(None, path);
        if bearer {
            request.headers_mut().insert(header::AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {TOKEN}")).unwrap());
        }
        assert_eq!(fixture.app().oneshot(request).await.unwrap().status(), StatusCode::UNAUTHORIZED);
    }
    let cookie = fixture.login().await;
    let reply = fixture.app().oneshot(request(Some(&cookie), path)).await.unwrap();
    assert_eq!(reply.status(), StatusCode::OK);
    assert_eq!(reply.headers()[header::CONTENT_TYPE], "image/png");
    assert_eq!(reply.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(reply.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    let thumbnail = to_bytes(reply.into_body(), 1024 * 1024).await.unwrap();
    let mut reader = png::Decoder::new(std::io::Cursor::new(&thumbnail)).read_info().unwrap();
    assert!(reader.info().width > 0 && reader.info().width <= 160);
    assert!(reader.info().height > 0 && reader.info().height <= 120);
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut pixels).unwrap();
    assert!(frame.buffer_size() >= 3);
    assert_eq!(&pixels[..3], &[42, 42, 42]);
    assert_eq!(std::fs::read(fixture.directory.join("public-preview.png")).unwrap(), bytes);
}

#[tokio::test]
async fn thumbnail_discards_source_png_ancillary_metadata() {
    let fixture = Fixture::new();
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.add_text_chunk("Comment".into(), "private-png-ancillary-marker".into()).unwrap();
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[42, 42, 42]).unwrap();
        writer.finish().unwrap();
    }
    assert!(bytes.windows(b"private-png-ancillary-marker".len()).any(|part| part == b"private-png-ancillary-marker"));
    std::fs::write(fixture.directory.join("annotated.png"), &bytes).unwrap();
    let cookie = fixture.login().await;
    let reply = fixture.app().oneshot(request(Some(&cookie), "/workspace/preview/annotated.png")).await.unwrap();
    assert_eq!(reply.status(), StatusCode::OK);
    let thumbnail = to_bytes(reply.into_body(), 1024 * 1024).await.unwrap();
    assert!(!thumbnail.windows(b"private-png-ancillary-marker".len()).any(|part| part == b"private-png-ancillary-marker"));
    assert_eq!(std::fs::read(fixture.directory.join("annotated.png")).unwrap(), bytes);
}

#[tokio::test]
async fn previews_reject_spoofed_truncated_oversized_hidden_and_traversal_inputs() {
    let fixture = Fixture::new();
    let cookie = fixture.login().await;
    for (name, bytes) in [
        ("html.png", b"<!doctype html><script>private-marker</script>".to_vec()),
        ("svg.png", b"<svg xmlns='http://www.w3.org/2000/svg'><script>private-marker</script></svg>".to_vec()),
        ("truncated.png", png(1, 1).into_iter().take(24).collect()),
        ("wide.png", png(4097, 1)),
        (".private.png", png(1, 1)),
        ("wrong.txt", png(1, 1)),
    ] {
        std::fs::write(fixture.directory.join(name), bytes).unwrap();
        let reply = fixture.app().oneshot(request(Some(&cookie), &format!("/workspace/preview/{name}"))).await.unwrap();
        assert!(!reply.status().is_success(), "unsafe preview accepted: {name}");
        assert_eq!(reply.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        let text = body_text(reply).await;
        assert!(!text.contains("private-marker") && !text.contains(TOKEN));
    }
    let oversized = std::fs::File::create(fixture.directory.join("oversized.png")).unwrap();
    oversized.set_len(8 * 1024 * 1024 + 1).unwrap();
    for name in ["oversized.png", "%2E%2E/private.png", "nested/%2E%2E/private.png", "missing.png"] {
        let reply = fixture.app().oneshot(request(Some(&cookie), &format!("/workspace/preview/{name}"))).await.unwrap();
        assert!(!reply.status().is_success());
    }
}

#[tokio::test]
async fn edit_handoff_is_cookie_bound_one_use_and_never_changes_saved_design_bytes() {
    let fixture = Fixture::new();
    std::fs::write(fixture.directory.join("public-copy.solidcraft"), NATIVE).unwrap();
    let owner = fixture.login().await;
    let other = fixture.login().await;
    let ticket = issue(&fixture, &owner, "public-copy.solidcraft").await;
    let mut bearer = request(None, &ticket);
    bearer.headers_mut().insert(header::AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {TOKEN}")).unwrap());
    assert!(!fixture.app().oneshot(bearer).await.unwrap().status().is_success());
    assert!(!fixture.app().oneshot(request(Some(&other), &ticket)).await.unwrap().status().is_success());
    let imported = fixture.app().oneshot(request(Some(&owner), &ticket)).await.unwrap();
    assert_eq!(imported.status(), StatusCode::OK);
    assert_eq!(imported.headers()[header::CONTENT_TYPE], "application/octet-stream");
    assert_eq!(imported.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert_eq!(imported.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(imported.headers()[header::CONTENT_LENGTH], NATIVE.len().to_string());
    assert_eq!(imported.headers()[header::CONTENT_DISPOSITION], "attachment");
    assert_eq!(imported.headers()["x-solidcraft-filename"], "public-copy.solidcraft");
    assert_eq!(to_bytes(imported.into_body(), 1024 * 1024).await.unwrap().as_ref(), NATIVE);
    assert!(!fixture.app().oneshot(request(Some(&owner), &ticket)).await.unwrap().status().is_success());
    assert_eq!(std::fs::read(fixture.directory.join("public-copy.solidcraft")).unwrap(), NATIVE);
    for name in ["current.sc", "legacy.solvecraft"] {
        std::fs::write(fixture.directory.join(name), NATIVE).unwrap();
        let ticket = issue(&fixture, &owner, name).await;
        let response = fixture.app().oneshot(request(Some(&owner), &ticket)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["x-solidcraft-filename"], name);
        assert_eq!(to_bytes(response.into_body(), 1024 * 1024).await.unwrap().as_ref(), NATIVE);
        assert!(!fixture.app().oneshot(request(Some(&owner), &ticket)).await.unwrap().status().is_success());
        assert_eq!(std::fs::read(fixture.directory.join(name)).unwrap(), NATIVE);
    }
    assert_eq!(fixture.state.slots_used(), 0);
    assert!(fixture.state.sessions.lock().await.is_empty());
}

#[tokio::test]
async fn edit_ticket_refuses_changed_file_and_logout_revokes_existing_ticket() {
    let fixture = Fixture::new();
    std::fs::write(fixture.directory.join("public-copy.solidcraft"), NATIVE).unwrap();
    let cookie = fixture.login().await;
    let stale = issue(&fixture, &cookie, "public-copy.solidcraft").await;
    let path = fixture.directory.join("public-copy.solidcraft");
    let previous_mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
    let changed = String::from_utf8(NATIVE.to_vec()).unwrap().replace("Public browser copy", "Public browser cops").into_bytes();
    assert_eq!(changed.len(), NATIVE.len());
    std::fs::write(&path, &changed).unwrap();
    std::fs::File::options().write(true).open(&path).unwrap().set_modified(previous_mtime).unwrap();
    assert_eq!(fixture.app().oneshot(request(Some(&cookie), &stale)).await.unwrap().status(), StatusCode::CONFLICT);
    assert_eq!(fixture.app().oneshot(request(Some(&cookie), &stale)).await.unwrap().status(), StatusCode::NOT_FOUND);
    let ticket = issue(&fixture, &cookie, "public-copy.solidcraft").await;
    fixture.state.browser.inner.lock().await.sessions.clear();
    assert!(!fixture.app().oneshot(request(Some(&cookie), &ticket)).await.unwrap().status().is_success());
    assert_eq!(std::fs::read(fixture.directory.join("public-copy.solidcraft")).unwrap(), changed);
}

#[tokio::test]
async fn handoff_ttl_and_capacity_are_bounded_and_expired_tickets_are_unusable() {
    let fixture = Fixture::new();
    std::fs::write(fixture.directory.join("public-copy.solidcraft"), NATIVE).unwrap();
    let cookie = fixture.login().await;
    let mut tickets = Vec::new();
    for _ in 0..4 {
        tickets.push(issue(&fixture, &cookie, "public-copy.solidcraft").await);
    }
    let limited = fixture.app().oneshot(request(Some(&cookie), "/workspace/open/public-copy.solidcraft")).await.unwrap();
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    let other = fixture.login().await;
    let other_ticket = issue(&fixture, &other, "public-copy.solidcraft").await;
    fixture.state.browser.media.expire_all().await;
    for ticket in tickets {
        assert!(!fixture.app().oneshot(request(Some(&cookie), &ticket)).await.unwrap().status().is_success());
    }
    assert!(!fixture.app().oneshot(request(Some(&other), &other_ticket)).await.unwrap().status().is_success());
    let renewed = issue(&fixture, &cookie, "public-copy.solidcraft").await;
    assert_eq!(fixture.app().oneshot(request(Some(&cookie), &renewed)).await.unwrap().status(), StatusCode::OK);
}

#[tokio::test]
async fn total_handoff_ticket_capacity_is_shared_across_browsers() {
    let fixture = Fixture::new();
    std::fs::write(fixture.directory.join("public-copy.solidcraft"), NATIVE).unwrap();
    let mut first = None;
    for _ in 0..8 {
        let cookie = fixture.login().await;
        for _ in 0..4 {
            let ticket = issue(&fixture, &cookie, "public-copy.solidcraft").await;
            if first.is_none() {
                first = Some((cookie.clone(), ticket));
            }
        }
    }
    let fresh = fixture.login().await;
    let limited = fixture.app().oneshot(request(Some(&fresh), "/workspace/open/public-copy.solidcraft")).await.unwrap();
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    let (cookie, ticket) = first.unwrap();
    assert_eq!(fixture.app().oneshot(request(Some(&cookie), &ticket)).await.unwrap().status(), StatusCode::OK);
    let _ = issue(&fixture, &fresh, "public-copy.solidcraft").await;
}

#[tokio::test]
async fn simultaneous_consumers_can_import_a_ticket_only_once() {
    let fixture = Fixture::new();
    std::fs::write(fixture.directory.join("public-copy.solidcraft"), NATIVE).unwrap();
    let cookie = fixture.login().await;
    let ticket = issue(&fixture, &cookie, "public-copy.solidcraft").await;
    let (first, second) =
        tokio::join!(fixture.app().oneshot(request(Some(&cookie), &ticket)), fixture.app().oneshot(request(Some(&cookie), &ticket)));
    let statuses = [first.unwrap().status(), second.unwrap().status()];
    assert_eq!(statuses.iter().filter(|status| **status == StatusCode::OK).count(), 1);
    assert_eq!(statuses.iter().filter(|status| **status == StatusCode::NOT_FOUND).count(), 1);
}

#[tokio::test]
async fn unconsumed_import_bodies_hold_read_capacity_until_dropped() {
    let fixture = Fixture::new();
    std::fs::write(fixture.directory.join("public-copy.solidcraft"), NATIVE).unwrap();
    let cookie = fixture.login().await;
    let first = issue(&fixture, &cookie, "public-copy.solidcraft").await;
    let second = issue(&fixture, &cookie, "public-copy.solidcraft").await;
    let third = issue(&fixture, &cookie, "public-copy.solidcraft").await;
    let first = fixture.app().oneshot(request(Some(&cookie), &first)).await.unwrap();
    let second = fixture.app().oneshot(request(Some(&cookie), &second)).await.unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(fixture.state.browser.media.available_reads(), 0);
    let mut waiting = tokio::spawn(fixture.app().oneshot(request(Some(&cookie), &third)));
    assert!(tokio::time::timeout(Duration::from_millis(100), &mut waiting).await.is_err());
    drop(first);
    let third = tokio::time::timeout(Duration::from_secs(1), waiting).await.unwrap().unwrap().unwrap();
    assert_eq!(third.status(), StatusCode::OK);
    assert_eq!(fixture.state.browser.media.available_reads(), 0);
    drop(second);
    drop(third);
    assert_eq!(fixture.state.browser.media.available_reads(), 2);
}

#[tokio::test]
async fn handoff_filename_is_a_percent_encoded_basename() {
    let fixture = Fixture::new();
    std::fs::create_dir(fixture.directory.join("parts")).unwrap();
    std::fs::write(fixture.directory.join("parts/Öffentlich #1.solidcraft"), NATIVE).unwrap();
    let cookie = fixture.login().await;
    let ticket = issue(&fixture, &cookie, "parts/%C3%96ffentlich%20%231.solidcraft").await;
    let reply = fixture.app().oneshot(request(Some(&cookie), &ticket)).await.unwrap();
    assert_eq!(reply.status(), StatusCode::OK);
    assert_eq!(reply.headers()["x-solidcraft-filename"], "%C3%96ffentlich%20%231.solidcraft");
}

#[cfg(unix)]
#[tokio::test]
async fn previews_and_handoffs_never_follow_links_outside_workspace() {
    let fixture = Fixture::new();
    let outside = Fixture::new();
    std::fs::write(outside.directory.join("outside.png"), png(1, 1)).unwrap();
    std::fs::write(outside.directory.join("outside.solidcraft"), NATIVE).unwrap();
    std::os::unix::fs::symlink(outside.directory.join("outside.png"), fixture.directory.join("linked.png")).unwrap();
    std::os::unix::fs::symlink(outside.directory.join("outside.solidcraft"), fixture.directory.join("linked.solidcraft")).unwrap();
    let cookie = fixture.login().await;
    assert!(!fixture.app().oneshot(request(Some(&cookie), "/workspace/preview/linked.png")).await.unwrap().status().is_success());
    assert!(!fixture.app().oneshot(request(Some(&cookie), "/workspace/open/linked.solidcraft")).await.unwrap().status().is_redirection());
    std::fs::write(fixture.directory.join("public-copy.solidcraft"), NATIVE).unwrap();
    let ticket = issue(&fixture, &cookie, "public-copy.solidcraft").await;
    std::fs::remove_file(fixture.directory.join("public-copy.solidcraft")).unwrap();
    std::os::unix::fs::symlink(outside.directory.join("outside.solidcraft"), fixture.directory.join("public-copy.solidcraft")).unwrap();
    assert!(!fixture.app().oneshot(request(Some(&cookie), &ticket)).await.unwrap().status().is_success());
}

#[tokio::test]
async fn edit_routes_deny_exports_private_names_cross_origin_and_oversized_native_files() {
    let fixture = Fixture::new();
    let cookie = fixture.login().await;
    for name in ["public-copy.step", ".private.solidcraft", "wrong.png"] {
        std::fs::write(fixture.directory.join(name), NATIVE).unwrap();
        assert!(!fixture.app().oneshot(request(Some(&cookie), &format!("/workspace/open/{name}"))).await.unwrap().status().is_redirection());
    }
    std::fs::write(fixture.directory.join("public-copy.solidcraft"), NATIVE).unwrap();
    let mut origin = request(Some(&cookie), "/workspace/open/public-copy.solidcraft");
    origin.headers_mut().insert(header::ORIGIN, HeaderValue::from_static("https://other.example"));
    assert_eq!(fixture.app().oneshot(origin).await.unwrap().status(), StatusCode::FORBIDDEN);
    let ticket = issue(&fixture, &cookie, "public-copy.solidcraft").await;
    let mut origin = request(Some(&cookie), &ticket);
    origin.headers_mut().insert(header::ORIGIN, HeaderValue::from_static("https://other.example"));
    assert_eq!(fixture.app().oneshot(origin).await.unwrap().status(), StatusCode::FORBIDDEN);
    // Cross-origin attempts do not burn the legitimate owner's one-use ticket.
    assert_eq!(fixture.app().oneshot(request(Some(&cookie), &ticket)).await.unwrap().status(), StatusCode::OK);
    let oversized = std::fs::File::create(fixture.directory.join("oversized.solidcraft")).unwrap();
    oversized.set_len(256 * 1024 * 1024 + 1).unwrap();
    assert!(!fixture.app().oneshot(request(Some(&cookie), "/workspace/open/oversized.solidcraft")).await.unwrap().status().is_redirection());
    for name in ["%2E%2E/private.solidcraft", "nested/%2E%2E/private.solidcraft", "missing.solidcraft"] {
        assert!(!fixture.app().oneshot(request(Some(&cookie), &format!("/workspace/open/{name}"))).await.unwrap().status().is_redirection());
    }
}
