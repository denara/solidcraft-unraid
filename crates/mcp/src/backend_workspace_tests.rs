//! Internal workspace telemetry follows engine state, including commands inside MCP batches.

use super::{Backend, Headless};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Files(PathBuf);

impl Files {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!("solidcraft-workspace-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn status(backend: &mut Headless) -> Value {
    backend.call("workspace.status", json!({})).unwrap()
}

fn execute(backend: &mut Headless, command: &str, params: Value) -> Result<Value, String> {
    backend.call("engine.execute", json!({"command": command, "params": params}))
}

fn box_params() -> Value {
    json!({"length": 10, "width": 20, "height": 30})
}

#[test]
fn workspace_status_follows_actual_file_and_dirty_state() {
    let files = Files::new();
    let a = files.path("sample-a.solidcraft");
    let b = files.path("sample-b.solidcraft");
    let mut backend = Headless::default();
    assert_eq!(status(&mut backend), json!({"current_file": null, "dirty": false, "has_model": false, "revision": 1}));

    // A sketch is a nonempty design even before any solid exists.
    execute(&mut backend, "sketch.create", json!({"plane": "XY"})).unwrap();
    assert_eq!(status(&mut backend)["has_model"], true);
    execute(&mut backend, "file.new", json!({})).unwrap();
    execute(&mut backend, "solid.box", box_params()).unwrap();
    assert_eq!(status(&mut backend)["current_file"], Value::Null);
    assert_eq!(status(&mut backend)["dirty"], true);
    execute(&mut backend, "file.save_as", json!({"path": a, "format": "json"})).unwrap();
    let saved = status(&mut backend);
    assert_eq!(saved["current_file"], a);
    assert_eq!(saved["dirty"], false);
    assert_eq!(saved["has_model"], true);

    execute(&mut backend, "solid.box", json!({"length": 1, "width": 2, "height": 3, "corner": [40, 0, 0]})).unwrap();
    let changed = status(&mut backend);
    assert_eq!(changed["dirty"], true);
    assert!(changed["revision"].as_u64().unwrap() > saved["revision"].as_u64().unwrap());
    execute(&mut backend, "edit.undo", json!({})).unwrap();
    assert_eq!(status(&mut backend)["dirty"], false);
    execute(&mut backend, "edit.redo", json!({})).unwrap();
    assert_eq!(status(&mut backend)["dirty"], true);

    // Failed file operations cannot manufacture a new current file in the dashboard.
    assert!(execute(&mut backend, "doc.open", json!({"path": files.path("missing.solidcraft")})).is_err());
    assert_eq!(status(&mut backend)["current_file"], a);
    assert_eq!(status(&mut backend)["dirty"], true);
    assert!(execute(&mut backend, "file.save_as", json!({"path": files.path("missing/failed.solidcraft"), "format": "json"})).is_err());
    assert_eq!(status(&mut backend)["current_file"], a);
    assert_eq!(status(&mut backend)["dirty"], true);

    execute(&mut backend, "doc.open", json!({ "path": a })).unwrap();
    assert_eq!(status(&mut backend)["dirty"], false);
    execute(&mut backend, "file.save_as", json!({"path": b, "format": "json"})).unwrap();
    assert_eq!(status(&mut backend)["current_file"], b);
    assert_eq!(status(&mut backend)["dirty"], false);
    execute(&mut backend, "file.new", json!({})).unwrap();
    let fresh = status(&mut backend);
    assert_eq!(fresh["current_file"], Value::Null);
    assert_eq!(fresh["dirty"], false);
    assert_eq!(fresh["has_model"], false);
    assert!(fresh["revision"].as_u64().unwrap() > saved["revision"].as_u64().unwrap());
}

#[test]
fn workspace_status_is_read_only_and_tracks_batch_rollback() {
    let mut backend = Headless::default();
    let failed = crate::call_tool(
        &mut backend,
        "batch",
        &json!({"commands": [
            {"command": "solid.box", "params": box_params()},
            {"command": "unknown.workspace_test", "params": {}}
        ]}),
    );
    assert_eq!(failed["isError"], true);
    assert_eq!(status(&mut backend)["dirty"], false);
    assert_eq!(status(&mut backend)["has_model"], false);

    let passed = crate::call_tool(&mut backend, "batch", &json!({"commands": [{"command": "solid.box", "params": box_params()}]}));
    assert_ne!(passed["isError"], true);
    let before = status(&mut backend);
    let undo = backend.session.undo.len();
    let redo = backend.session.redo.len();
    assert_eq!(before["dirty"], true);
    assert_eq!(before["has_model"], true);
    assert_eq!(status(&mut backend), before);
    assert_eq!(backend.session.undo.len(), undo);
    assert_eq!(backend.session.redo.len(), redo);
}

#[test]
fn internal_workspace_rpc_does_not_add_a_public_mcp_tool() {
    let mut server = crate::Server::new(Box::new(Headless::default()));
    let reply = server.handle(json!({"jsonrpc": "2.0", "id": 27, "method": "solidcraft/workspaceStatus", "params": {}})).unwrap();
    assert_eq!(reply["id"], 27);
    assert_eq!(reply["result"], json!({"current_file": null, "dirty": false, "has_model": false, "revision": 1}));
    assert!(crate::tool_definitions().iter().all(|tool| tool["name"] != "workspace.status" && tool["name"] != "solidcraft/workspaceStatus"));
}

#[test]
fn workspace_revision_counts_observed_content_not_refresh_attempts() {
    let files = Files::new();
    let mut backend = Headless::default();
    let empty = status(&mut backend);
    assert!(execute(&mut backend, "unknown.workspace_test", json!({})).is_err());
    assert!(execute(&mut backend, "file.save_as", json!({"path":files.path("missing/failed.solidcraft"),"format":"json"})).is_err());
    backend.call("document.inspect", json!({"measure":false})).unwrap();
    backend.call("engine.commands", json!({})).unwrap();
    assert_eq!(status(&mut backend), empty);

    execute(&mut backend, "solid.box", box_params()).unwrap();
    let model = status(&mut backend);
    assert_eq!(model["revision"], 2);
    assert!(execute(&mut backend, "file.save_as", json!({"path":files.path("missing/failed.solidcraft"),"format":"json"})).is_err());
    assert_eq!(status(&mut backend), model);
    backend.call("ui.view", json!({"view":"top"})).unwrap();
    assert_eq!(status(&mut backend), model);

    // Visibility is saved design content even though it has no geometry refresh.
    execute(&mut backend, "browser.visibility", json!({"items":["Body1"],"visible":false})).unwrap();
    let hidden = status(&mut backend);
    assert_eq!(hidden["revision"], 3);
    assert_eq!(status(&mut backend), hidden);
    execute(&mut backend, "file.new", json!({})).unwrap();
    assert_eq!(status(&mut backend)["revision"], 4);
}

#[test]
fn point_cloud_only_design_is_an_unsaved_model_and_keeps_its_source_unchanged() {
    let files = Files::new();
    let path = files.path("scan.xyz");
    let bytes = b"0 0 0\n10 0 0\n0 10 0\n";
    std::fs::write(&path, bytes).unwrap();
    let mut backend = Headless::default();
    execute(&mut backend, "doc.open", json!({"path":path})).unwrap();
    assert!(backend.session.doc.features.is_empty());
    let result = status(&mut backend);
    assert_eq!(result["has_model"], true);
    assert_eq!(result["dirty"], true);
    assert_eq!(result["current_file"], Value::Null);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}
