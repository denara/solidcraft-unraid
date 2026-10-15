//! `snapshot` frames its picture like the MCP `screenshot`: from a camera, a view saved in the
//! design or one the script applies itself, and without the ground grid and the origin's axes
//! when asked (the grid lies on Z 0 and cuts through a part that straddles it).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};
use solidcraft_engine::Session;
use solidcraft_mcp::{Headless, Server};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const SIZE: (usize, usize) = (160, 100);

fn camera() -> Value {
    json!({"target": [20, 0, 0], "yaw": 0.6, "pitch": 0.25, "distance": 100})
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("solidcraft-snapshot-camera-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }

    /// A box that straddles Z 0 and keeps clear of the origin (its axes show beside it), with a view
    /// saved in the design, plus `more` steps.
    fn script(&self, name: &str, more: &[Value]) -> PathBuf {
        let mut steps = vec![
            json!({"command": "solid.box", "params": {"length": 60, "width": 40, "height": 30, "corner": [10, -20, -15]}}),
            json!({"command": "view.save", "params": {"name": "Detail", "camera": camera()}}),
        ];
        steps.extend(more.iter().cloned());
        let path = self.0.join(name);
        std::fs::write(&path, json!({"commands": steps}).to_string()).unwrap();
        path
    }

    fn run(&self, script: &Path, out: &str, extra: &[&str]) -> Output {
        let (w, h) = (SIZE.0.to_string(), SIZE.1.to_string());
        Command::new(env!("CARGO_BIN_EXE_solidcraft-cli"))
            .arg("snapshot")
            .arg(script)
            .arg("--out")
            .arg(self.0.join(out))
            .args(["--width", &w, "--height", &h])
            .args(extra)
            .output()
            .unwrap()
    }

    /// The PNG `snapshot` writes.
    fn snapshot(&self, script: &Path, out: &str, extra: &[&str]) -> Vec<u8> {
        let output = self.run(script, out, extra);
        assert!(output.status.success(), "{extra:?}: {}", String::from_utf8_lossy(&output.stderr));
        let png = std::fs::read(self.0.join(out)).unwrap();
        assert!(png.starts_with(b"\x89PNG"), "{extra:?}");
        png
    }

    /// The PNG the MCP `screenshot` tool writes for the same design.
    fn mcp(&self, script: &Path, out: &str, arguments: Value) -> Vec<u8> {
        let mut session = Session::default();
        session.run_script(&serde_json::from_slice(&std::fs::read(script).unwrap()).unwrap()).unwrap();
        let mut server = Server::new(Box::new(Headless::new(session)));
        let mut arguments = arguments;
        arguments["path"] = json!(self.0.join(out).to_string_lossy());
        arguments["width"] = json!(SIZE.0);
        arguments["height"] = json!(SIZE.1);
        let call = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "screenshot", "arguments": arguments}});
        let reply: Value = serde_json::from_str(&server.handle_line(&call.to_string()).unwrap()).unwrap();
        assert!(reply.get("error").is_none() && reply["result"]["isError"] != json!(true), "{reply}");
        std::fs::read(self.0.join(out)).unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The picture of `--camera` is the picture of the MCP `screenshot` with the same camera, and
/// not the home view the option used to be ignored for.
#[test]
fn a_camera_frames_the_picture_as_the_mcp_screenshot_does() {
    let dir = Scratch::new();
    let script = dir.script("part.json", &[]);
    let home = dir.snapshot(&script, "home.png", &[]);
    let json = camera().to_string();
    let shot = dir.snapshot(&script, "camera.png", &["--camera", &json]);
    assert_ne!(shot, home, "--camera was ignored");
    assert_eq!(shot, dir.mcp(&script, "mcp-camera.png", json!({"camera": camera()})));
    // A perspective camera too, and a different eye gives a different picture.
    let mut near = camera();
    near["fov"] = json!(0.6);
    let perspective = dir.snapshot(&script, "perspective.png", &["--camera", &near.to_string()]);
    assert_ne!(perspective, shot);
    assert_eq!(perspective, dir.mcp(&script, "mcp-perspective.png", json!({"camera": near})));
}

/// `--view` also takes a view saved in the design, which is the camera it was saved with; a
/// standard view still looks as it did.
#[test]
fn a_saved_view_is_its_camera_and_a_standard_view_is_unchanged() {
    let dir = Scratch::new();
    let script = dir.script("part.json", &[]);
    let json = camera().to_string();
    let by_camera = dir.snapshot(&script, "camera.png", &["--camera", &json]);
    let saved = dir.snapshot(&script, "saved.png", &["--view", "Detail"]);
    assert_eq!(saved, by_camera);
    assert_eq!(saved, dir.mcp(&script, "mcp-saved.png", json!({"view": "Detail"})));
    // A standard view frames the model from its direction, as before: the home camera turned.
    let top = dir.snapshot(&script, "top.png", &["--view", "top"]);
    let mut s = Session::default();
    s.run_script(&serde_json::from_slice(&std::fs::read(&script).unwrap()).unwrap()).unwrap();
    let mut cam = solidcraft_engine::view::home_camera(&s);
    cam.set_view(solidcraft_engine::render::StandardView::Top);
    let scene = solidcraft_engine::view::scene(&s, &cam);
    assert_eq!(top, solidcraft_engine::render::render_png(&scene, &cam, SIZE.0, SIZE.1).unwrap());
    assert_eq!(top, dir.mcp(&script, "mcp-top.png", json!({"view": "top"})));
    assert_ne!(top, saved);
    let iso = dir.snapshot(&script, "iso.png", &["--view", "iso"]);
    assert_eq!(iso, dir.snapshot(&script, "home.png", &[]));
    assert_ne!(iso, top);
}

/// A script that applies a view itself (as an MCP `execute` of `view.apply` does) sets the camera
/// of its snapshot; `--view` and `--camera` still decide when given.
#[test]
fn a_script_can_carry_its_camera() {
    let dir = Scratch::new();
    let script = dir.script("carried.json", &[json!({"command": "view.apply", "params": {"camera": camera()}})]);
    let plain = dir.script("plain.json", &[]);
    let carried = dir.snapshot(&script, "carried.png", &[]);
    assert_eq!(carried, dir.snapshot(&plain, "by-camera.png", &["--camera", &camera().to_string()]));
    assert_ne!(carried, dir.snapshot(&plain, "home.png", &[]));
    let top = dir.snapshot(&script, "top.png", &["--view", "top"]);
    assert_eq!(top, dir.snapshot(&plain, "plain-top.png", &["--view", "top"]));
}

/// The grid and the axes each go when asked, and a render without both from any side shows the
/// model alone: an empty design is then the same blank picture from every camera.
#[test]
fn the_grid_and_the_axes_can_be_left_out() {
    let dir = Scratch::new();
    let script = dir.script("part.json", &[]);
    let json = camera().to_string();
    for view in [&["--view", "front"][..], &["--camera", &json][..]] {
        let with = |extra: &[&str], out: &str| dir.snapshot(&script, out, &[view, extra].concat());
        let (all, no_grid, no_axes, bare) = (
            with(&[], "all.png"),
            with(&["--no-grid"], "no-grid.png"),
            with(&["--no-axes"], "no-axes.png"),
            with(&["--no-grid", "--no-axes"], "bare.png"),
        );
        let different = [&all, &no_grid, &no_axes, &bare];
        for (i, a) in different.iter().enumerate() {
            for b in &different[i + 1..] {
                assert_ne!(a, b, "{view:?}: two pictures are the same");
            }
        }
        // The same as the MCP's `grid` and `axes`.
        let mcp = |extra: Value, out: &str| {
            let mut a = match view[0] {
                "--view" => json!({"view": "front"}),
                _ => json!({"camera": camera()}),
            };
            a.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            dir.mcp(&script, out, a)
        };
        assert_eq!(no_grid, mcp(json!({"grid": false}), "mcp-no-grid.png"));
        assert_eq!(no_axes, mcp(json!({"axes": false}), "mcp-no-axes.png"));
        assert_eq!(bare, mcp(json!({"grid": false, "axes": false}), "mcp-bare.png"));
    }
    let empty = dir.0.join("empty.json");
    std::fs::write(&empty, json!({"commands": []}).to_string()).unwrap();
    let blank = |view: &str, out: &str| dir.snapshot(&empty, out, &["--view", view, "--no-grid", "--no-axes"]);
    assert_eq!(blank("top", "blank-top.png"), blank("front", "blank-front.png"));
    assert_ne!(blank("top", "blank-top.png"), dir.snapshot(&empty, "top.png", &["--view", "top"]));
}

#[test]
fn a_view_or_a_camera_that_cannot_be_used_is_an_error_and_writes_nothing() {
    let dir = Scratch::new();
    let script = dir.script("part.json", &[]);
    let cases: [(&[&str], &str); 7] = [
        (&["--view", "Nowhere"], "Nowhere"),
        (&["--view", "iso", "--camera", "{}"], "not both"),
        (&["--camera", "not json"], "--camera"),
        (&["--camera", "{\"target\": [0, 0, 0]}"], "yaw"),
        (&["--camera", "{\"target\": [0, 0, 0], \"yaw\": 0, \"pitch\": 0, \"distance\": 0}"], "distance"),
        (&["--camera", "{\"target\": [0, 0, 0], \"yaw\": 0, \"pitch\": 0, \"distance\": 50, \"fov\": -1}"], "camera"),
        (&["--camera", "[1, 2, 3]"], "camera"),
    ];
    for (extra, expect) in cases {
        let output = dir.run(&script, "never.png", extra);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{extra:?}: {stderr}");
        assert!(stderr.contains(expect), "{extra:?}: {stderr}");
        assert!(output.stdout.is_empty() && !dir.0.join("never.png").exists(), "{extra:?}");
    }
}

/// The usage text says what the new options do.
#[test]
fn the_usage_names_the_camera_and_the_guides() {
    let output = Command::new(env!("CARGO_BIN_EXE_solidcraft-cli")).arg("snapshot").output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    for word in ["--camera", "--view", "view.save", "--no-grid", "--no-axes"] {
        assert!(stderr.contains(word), "usage lacks {word}: {stderr}");
    }
}
