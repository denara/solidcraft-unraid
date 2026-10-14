use super::*;
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn versions_compare_as_semver() {
    assert!(is_newer("v0.10.0", "0.9.0"));
    assert!(is_newer("v0.10.0", "0.9.12"), "0.10 is after 0.9.12, not before it as text would sort");
    assert!(is_newer("v1.0.0", "0.10.0"));
    assert!(!is_newer("v0.10.0", "0.10.0"));
    assert!(!is_newer("v0.9.0", "0.10.0"));
    // A release candidate is older than its release; the release is offered to it.
    assert!(is_newer("v0.11.0", "0.11.0-rc.2"));
    // A prerelease is never offered (releases/latest never returns one anyway).
    assert!(!is_newer("v0.12.0-rc.1", "0.11.0"));
    assert!(!is_newer("garbage", "0.9.0"));
    assert!(!is_newer("v0.10.0", "not a version"));
}

#[test]
fn nightlies_count_as_the_release_they_follow() {
    // Built from main after 0.10.0: not nagged about 0.10.0 itself.
    assert!(!is_newer("v0.10.0", "0.10.0-nightly.20261015.37.1"));
    assert!(!is_newer("v0.9.0", "0.10.0-nightly.20261015.37.1"));
    // A newer stable release is offered.
    assert!(is_newer("v0.11.0", "0.10.0-nightly.20261015.37.1"));
    assert!(is_newer("v0.10.0", "0.9.0-nightly.20261014.36.1"));
}

fn probe(os: &str, release: bool, env: &[(&str, &str)], exe: &str, files: &[&str]) -> InstallKind {
    let env: BTreeMap<String, String> = env.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let files: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
    let get = |k: &str| env.get(k).cloned();
    let exists = |p: &Path| files.iter().any(|f| f == p);
    detect(&Probe { os, release_build: release, env: &get, exe: Some(PathBuf::from(exe)), exists: &exists })
}

#[test]
fn install_kinds_are_told_apart() {
    let home = "/home/me/Apps/solidcraft";
    assert_eq!(probe("linux", false, &[("APPIMAGE", "/x.AppImage")], home, &[]), InstallKind::Dev, "dev builds never check");
    assert_eq!(probe("linux", true, &[("FLATPAK_ID", APP_ID)], "/app/bin/solidcraft", &[]), InstallKind::Flatpak(APP_ID.into()));
    let snap = |rev: &str| {
        probe(
            "linux",
            true,
            &[("SNAP", "/snap/solidcraft/12"), ("SNAP_NAME", "solidcraft"), ("SNAP_REVISION", rev)],
            "/snap/solidcraft/12/usr/bin/solidcraft",
            &[],
        )
    };
    assert_eq!(snap("12"), InstallKind::Snap { store: true });
    assert_eq!(snap("x3"), InstallKind::Snap { store: false }, "installed from a file with --dangerous");
    assert_eq!(
        probe("linux", true, &[("APPIMAGE", "/home/me/SolidCraft.AppImage")], "/tmp/.mount_SolidXYZ/usr/bin/solidcraft", &[]),
        InstallKind::AppImage("/home/me/SolidCraft.AppImage".into())
    );
    // An empty variable doesn't count.
    assert_eq!(probe("linux", true, &[("APPIMAGE", "")], "/opt/sc/bin/solidcraft", &[]), InstallKind::LinuxTarball);
    assert_eq!(probe("linux", true, &[], "/usr/bin/solidcraft", &["/var/lib/dpkg/info/solidcraft.list", "/var/lib/rpm"]), InstallKind::Deb);
    assert_eq!(probe("linux", true, &[], "/usr/bin/solidcraft", &["/var/lib/rpm"]), InstallKind::Rpm);
    assert_eq!(probe("linux", true, &[], "/usr/bin/solidcraft", &["/usr/lib/sysimage/rpm"]), InstallKind::Rpm);
    assert_eq!(probe("linux", true, &[], "/usr/bin/solidcraft", &[]), InstallKind::Other);
    assert_eq!(probe("linux", true, &[], home, &[]), InstallKind::LinuxTarball);

    let pf = [("ProgramFiles", "C:\\Program Files"), ("ProgramFiles(x86)", "C:\\Program Files (x86)")];
    assert_eq!(probe("windows", true, &pf, "C:\\Program Files\\SolidCraft\\solidcraft.exe", &[]), InstallKind::WindowsMsi);
    assert_eq!(probe("windows", true, &pf, "c:\\program files\\SolidCraft\\solidcraft.exe", &[]), InstallKind::WindowsMsi, "case-insensitive");
    // "C:\Program Files Extra" is not under "C:\Program Files".
    assert_eq!(probe("windows", true, &pf, "C:\\Program Files Extra\\solidcraft.exe", &[]), InstallKind::WindowsExe);
    let portable = "C:\\Users\\me\\solidcraft-0.10.0-windows-x64-portable\\solidcraft.exe";
    let cli = "C:\\Users\\me\\solidcraft-0.10.0-windows-x64-portable\\solidcraft-cli.exe";
    // Path::parent splits on `\` only on Windows; this test describes the layout either way.
    if cfg!(windows) {
        assert_eq!(probe("windows", true, &pf, portable, &[cli]), InstallKind::WindowsPortable);
    }
    assert_eq!(probe("windows", true, &pf, "C:\\Users\\me\\Downloads\\SolidCraft-0.10.0-windows-x64.exe", &[]), InstallKind::WindowsExe);

    assert_eq!(
        probe("macos", true, &[], "/Applications/SolidCraft.app/Contents/MacOS/solidcraft", &[]),
        InstallKind::MacApp("/Applications/SolidCraft.app".into())
    );
    assert_eq!(probe("macos", true, &[], "/usr/local/bin/solidcraft", &[]), InstallKind::Other);
    assert_eq!(probe("macos", false, &[], "/Applications/SolidCraft.app/Contents/MacOS/solidcraft", &[]), InstallKind::Dev);
    assert_eq!(probe("freebsd", true, &[], "/usr/local/bin/solidcraft", &[]), InstallKind::Other);
    assert!(!InstallKind::Dev.checks() && !InstallKind::Web.checks() && InstallKind::Snap { store: true }.checks());
    assert!(!InstallKind::Flatpak(APP_ID.into()).checks(), "the Flatpak has no network permission; Flatpak updates it");
}

#[test]
fn portable_windows_layout_is_recognised() {
    let exe = PathBuf::from("/portable/solidcraft.exe");
    let get = |k: &str| (k == "ProgramFiles").then(|| "C:\\Program Files".to_string());
    let exists = |p: &Path| p == Path::new("/portable/solidcraft-cli.exe");
    assert_eq!(detect(&Probe { os: "windows", release_build: true, env: &get, exe: Some(exe), exists: &exists }), InstallKind::WindowsPortable);
}

/// The assets of v0.10.0, as published.
fn release() -> Release {
    let names = [
        "SHA256SUMS.txt",
        "solidcraft-0.10.0-linux-x86_64.AppImage",
        "solidcraft-0.10.0-linux-x86_64.AppImage.zsync",
        "solidcraft-0.10.0-linux-x86_64.tar.gz",
        "solidcraft-0.10.0-linux-aarch64.tar.gz",
        "SolidCraft-0.10.0-windows-x64.exe",
        "solidcraft-linux-aarch64.AppImage",
        "solidcraft-linux-aarch64.deb",
        "solidcraft-linux-x86_64.AppImage",
        "solidcraft-linux-x86_64.deb",
        "solidcraft-linux-x86_64.flatpak",
        "solidcraft-linux-x86_64.rpm",
        "solidcraft-linux-x86_64.snap",
        "solidcraft-macos-universal.dmg",
        "solidcraft-windows-arm64-portable.zip",
        "solidcraft-windows-arm64.msi",
        "solidcraft-windows-x64-portable.zip",
        "solidcraft-windows-x64.msi",
        "solidcraft-windows-x86-portable.zip",
        "solidcraft-windows-x86.msi",
    ];
    let assets: Vec<Value> = names
        .iter()
        .map(|n| json!({"name": n, "size": 10, "browser_download_url": format!("https://github.com/bherbruck/solidcraft/releases/download/v0.10.0/{n}")}))
        .collect();
    let body = json!({
        "tag_name": "v0.10.0", "name": "SolidCraft v0.10.0", "body": "**What's new**\n\n- Surface tools.",
        "html_url": "https://github.com/bherbruck/solidcraft/releases/tag/v0.10.0", "draft": false, "prerelease": false, "assets": assets,
    });
    Release::parse(body.to_string().as_bytes()).unwrap()
}

#[test]
fn each_installation_gets_its_own_file() {
    let r = release();
    let name = |k: InstallKind, arch: &str| r.asset_for(&k, arch).map(|a| a.name.clone());
    assert_eq!(name(InstallKind::AppImage("/a".into()), "x86_64").as_deref(), Some("solidcraft-linux-x86_64.AppImage"));
    assert_eq!(name(InstallKind::AppImage("/a".into()), "aarch64").as_deref(), Some("solidcraft-linux-aarch64.AppImage"));
    assert_eq!(name(InstallKind::Deb, "x86_64").as_deref(), Some("solidcraft-linux-x86_64.deb"));
    assert_eq!(name(InstallKind::Rpm, "x86_64").as_deref(), Some("solidcraft-linux-x86_64.rpm"));
    assert_eq!(name(InstallKind::LinuxTarball, "aarch64").as_deref(), Some("solidcraft-0.10.0-linux-aarch64.tar.gz"));
    assert_eq!(name(InstallKind::WindowsMsi, "x86_64").as_deref(), Some("solidcraft-windows-x64.msi"));
    assert_eq!(name(InstallKind::WindowsMsi, "aarch64").as_deref(), Some("solidcraft-windows-arm64.msi"));
    assert_eq!(name(InstallKind::WindowsMsi, "x86").as_deref(), Some("solidcraft-windows-x86.msi"));
    assert_eq!(name(InstallKind::WindowsPortable, "x86_64").as_deref(), Some("solidcraft-windows-x64-portable.zip"));
    assert_eq!(name(InstallKind::WindowsExe, "x86_64").as_deref(), Some("SolidCraft-0.10.0-windows-x64.exe"));
    assert_eq!(name(InstallKind::WindowsExe, "aarch64").as_deref(), Some("solidcraft-windows-arm64-portable.zip"));
    assert_eq!(name(InstallKind::MacApp("/A.app".into()), "aarch64").as_deref(), Some("solidcraft-macos-universal.dmg"));
    assert_eq!(name(InstallKind::Other, "x86_64"), None);
    assert_eq!(name(InstallKind::Deb, "riscv64"), None);
    // Download: the file, or the page when there is none or a package manager updates it.
    assert_eq!(
        r.download_url(&InstallKind::WindowsMsi, "x86_64"),
        "https://github.com/bherbruck/solidcraft/releases/download/v0.10.0/solidcraft-windows-x64.msi"
    );
    assert_eq!(r.download_url(&InstallKind::Other, "x86_64"), "https://github.com/bherbruck/solidcraft/releases/tag/v0.10.0");
    assert_eq!(r.download_url(&InstallKind::Flatpak(APP_ID.into()), "x86_64"), r.page);
    assert_eq!(r.download_url(&InstallKind::Snap { store: true }, "x86_64"), r.page);
    assert_eq!(
        r.download_url(&InstallKind::Snap { store: false }, "x86_64"),
        "https://github.com/bherbruck/solidcraft/releases/download/v0.10.0/solidcraft-linux-x86_64.snap"
    );
    assert_eq!(
        InstallKind::Snap { store: false }.update_command(Some("solidcraft-linux-x86_64.snap")).as_deref(),
        Some("sudo snap install --dangerous ./solidcraft-linux-x86_64.snap")
    );
    assert_eq!(InstallKind::Flatpak(APP_ID.into()).update_command(None).as_deref(), Some("flatpak update ai.storyteller.solidcraft"));
    assert_eq!(
        InstallKind::Deb.update_command(Some("solidcraft-linux-x86_64.deb")).as_deref(),
        Some("sudo apt install ./solidcraft-linux-x86_64.deb")
    );
    assert_eq!(InstallKind::WindowsMsi.update_command(None), None);
}

#[test]
fn release_answers_are_checked() {
    let r = release();
    assert_eq!((r.tag.as_str(), r.version(), r.assets.len()), ("v0.10.0", "0.10.0", 20));
    let bad = |v: Value| Release::parse(v.to_string().as_bytes()).is_err();
    assert!(bad(json!({"tag_name": "v0.11.0-rc.1", "prerelease": true})));
    assert!(bad(json!({"tag_name": "v0.11.0", "draft": true})));
    assert!(bad(json!({"tag_name": "nightly"})));
    assert!(Release::parse(b"<html>").is_err());
    // Links elsewhere are dropped: the page falls back to the releases page, foreign assets go.
    let r = Release::parse(
        json!({"tag_name": "v1.0.0", "html_url": "https://evil.example/x", "assets": [
            {"name": "solidcraft-windows-x64.msi", "browser_download_url": "https://evil.example/a.msi"},
            {"name": "solidcraft-windows-x86.msi", "browser_download_url": "https://github.com/bherbruck/solidcraft/releases/download/v1.0.0/solidcraft-windows-x86.msi"},
        ]})
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    assert_eq!(r.page, RELEASES_PAGE);
    assert_eq!(r.assets.len(), 1);
    assert_eq!(r.name, "SolidCraft v1.0.0");
    // Huge notes are cut (on a character boundary).
    let r = Release::parse(json!({"tag_name": "v1.0.0", "body": "é".repeat(MAX_NOTES)}).to_string().as_bytes()).unwrap();
    assert!(r.notes.len() <= MAX_NOTES + 8 && r.notes.ends_with('…'));
}

#[test]
fn skip_and_remind_later() {
    let r = release();
    let mut m = Memo::default();
    let now = 1_800_000_000.0;
    assert!(m.offers(&r, "0.9.0", now));
    assert!(!m.offers(&r, "0.10.0", now));
    m.remind_later(now);
    assert!(!m.offers(&r, "0.9.0", now + 3600.0));
    assert!(m.offers(&r, "0.9.0", now + REMIND_DELAY_S + 1.0));
    m.skip(&r);
    assert!(!m.offers(&r, "0.9.0", now + REMIND_DELAY_S + 1.0), "a skipped version stays hidden");
    let mut next = r.clone();
    next.tag = "v0.11.0".into();
    assert!(m.offers(&next, "0.9.0", now + REMIND_DELAY_S + 1.0), "a later version shows again");
}

#[test]
fn checks_run_daily_and_back_off() {
    let mut m = Memo::default();
    let now = 1_800_000_000.0;
    assert!(m.due(now), "the first start checks");
    m.checked(now, &Ok(release()));
    assert!(!m.due(now + 3600.0));
    assert!(m.due(now + CHECK_INTERVAL_S));
    assert!(m.due(now - 2.0 * CHECK_INTERVAL_S), "a clock set back doesn't block checks for days");
    m.checked(now, &Err(FetchError::Failed("offline".into())));
    assert!(!m.due(now + 60.0));
    assert!(m.due(now + RETRY_AFTER_FAILURE_S));
    m.checked(now, &Err(FetchError::RateLimited { retry_after_s: 1800 }));
    assert!(!m.due(now + 1700.0));
    assert!(m.due(now + 1800.0));
    assert_eq!(m.rate_limited_until, now + 1800.0);
    // Absurd waits are capped at a day.
    m.checked(now, &Err(FetchError::RateLimited { retry_after_s: u64::MAX }));
    assert!(m.due(now + CHECK_INTERVAL_S));
}

#[test]
fn the_memo_survives_with_the_preferences() {
    let mut p = crate::prefs::Prefs::default();
    assert!(p.check_updates);
    p.update.skipped = "0.10.0".into();
    p.update.remind_after = 5.0;
    let back: crate::prefs::Prefs = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(back, p);
    // Older preferences without these fields load with the defaults.
    let old: crate::prefs::Prefs = serde_json::from_str(r#"{"theme":"light"}"#).unwrap();
    assert!(old.check_updates && old.update == Memo::default());
}

#[test]
fn release_notes_read_as_markdown() {
    let md = "## What's new\n\n**Why 0.10:** a minor\nbump.\n\n- **Surface tools.** Revolve and `solidcraft-cli mcp`\n  continued ([#925](https://x/925)).\n  - nested\n* Fix by @a in https://github.com/bherbruck/solidcraft/pull/1064\n\nSee https://example.com/x.";
    let b = parse_markdown(md);
    assert_eq!(b.first(), Some(&Block::Heading(vec![Span::Text("What's new".into())])));
    assert_eq!(b.get(1), Some(&Block::Paragraph(vec![Span::Strong("Why 0.10:".into()), Span::Text(" a minor bump.".into())])));
    assert_eq!(
        b.get(2),
        Some(&Block::Item(
            0,
            vec![
                Span::Strong("Surface tools.".into()),
                Span::Text(" Revolve and ".into()),
                Span::Code("solidcraft-cli mcp".into()),
                Span::Text(" continued (".into()),
                Span::Link("#925".into()),
                Span::Text(").".into()),
            ]
        ))
    );
    assert_eq!(b.get(3), Some(&Block::Item(1, vec![Span::Text("nested".into())])));
    assert_eq!(b.get(4), Some(&Block::Item(0, vec![Span::Text("Fix by @a in ".into()), Span::Link("#1064".into())])));
    assert_eq!(b.get(5), Some(&Block::Paragraph(vec![Span::Text("See https://example.com/x.".into())])));
    // Unclosed markers and odd input show as written, without panicking.
    for s in ["**open", "`tick", "[a](b", "[a]", "#", "- ", "é**", "https://github.com/", "**[x](y)**"] {
        let _ = parse_markdown(s);
    }
    assert_eq!(inline("**[x](y)**"), vec![Span::Strong("x".into())]);
}
