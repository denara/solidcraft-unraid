//! Programmatic control of the running app (agents, tests).
//!
//! Methods (JSON lines over the host's transport, `{"id", "method", "params"}`):
//! - `engine.execute {command, params}`: run any command with JSON parameters (no dialogs)
//! - `engine.script {commands: [...]}`: run a command script
//! - `engine.batch {commands: [...], rollback?}`: the steps in one request, each as
//!   `engine.execute` runs it; stops at the first failure and goes back to the checkpoint
//!   (per-step results, `failed`, `undone`). Refused (`engine.batch unavailable: …`) while the
//!   agent cursor is shown or when a step loads a file
//! - `engine.commands`: every command with tab, panel, params and enablement
//! - `document.inspect {measure?}`: the design (parameters, timeline, bodies, sketches)
//! - `ui.inspect`: UI state, viewport rect, camera, tool and dialog
//! - `ui.drag {x0, y0, x1, y1, button?, shift?, ctrl?, steps?, hold?}` (box selection, navigation;
//!   `hold` keeps the button down that many frames before moving);
//!   `ui.selection` (selection, dialog inputs, hover, drawn sketch dimensions);
//!   `ui.sketchToScreen {points: [[x,y]…]}` (screen points of active-sketch coordinates); `ui.editFeature {feature}` (edit dialog)
//! - `ui.set {...UiState fields}`; `ui.view {view: front|back|top|bottom|left|right|iso|home|fit, animate?: bool}` (snaps unless animate)
//! - `ui.start {command}`: like clicking the toolbar button (starts tools/dialogs)
//! - `ui.click {x, y, button?, shift?}`, `ui.move {x, y}`: real
//!   pointer input in screen points; `ui.scroll {x, y, delta}`: mouse-wheel input in
//!   points (zooms); `ui.key {key, cmd?, shift?}`, `ui.text {text}`
//! - `ui.screenshot {path?}`: PNG of the window; `ui.render {path, width?, height?}`: headless
//!   CPU render of the model with the current camera and the section the viewport shows (no
//!   window needed)
//! - `ui.menu {x?, y?, target?, close?}`: open a context menu at a point (the viewport's marking
//!   menu, or a browser menu with `target: {type: body|sketch|component, …}`); without a point,
//!   the open menu and its items. `ui.menuPick {item}` runs an item by id or label;
//!   `ui.rename {text?, commit?}` finishes the rename box
//! - `ui.window {action: minimize|maximize|restore|toggle|close}`: what the title bar's buttons do
//! - `ui.confirm {accept?}`: the Delete confirmation (without `accept`: what it lists)
//! - `ui.shortcut {command, key?, replace?}`: a command's key, or bind one ("" clears)
//! - `ui.help {item?: about|shortcuts|report}`: the Help menu; returns the version and diagnostics
//! - `ui.prefs {...}`: the preferences (default_units, nav, zoom_reverse, orbit_cursor, msaa,
//!   length_decimals, angle_decimals); returns them all
//! - `ui.capture {action?: capture|revert|cancel}`: the Capture Position question
//! - `ui.agent_cursor {show?, speed?: 1–5 (times) | "instant", follow_camera?}`: the agent cursor
//!   (#35); while it is shown, `engine.execute` first glides to the command's button and picks
//!   and replies once the command has run (`"animate": false` skips that one call)
//! - `ui.resize {width, height}`, `app.quit`

use std::sync::mpsc::Sender;

use serde_json::{Value, json};
use solidcraft_engine::io::vfs;
use solidcraft_engine::render::{StandardView, render_png};

use crate::SolveApp;

pub type ControlResponse = Value;

pub struct ControlRequest {
    pub method: String,
    pub params: Value,
    pub reply: Sender<ControlResponse>,
}

impl ControlRequest {
    pub fn new(method: impl Into<String>, params: Value) -> (Self, std::sync::mpsc::Receiver<ControlResponse>) {
        let (tx, rx) = std::sync::mpsc::channel();
        (Self { method: method.into(), params, reply: tx }, rx)
    }
}

pub enum Outcome {
    Done(Value),
    Screenshot { path: Option<String> },
}

fn ok(v: Value) -> Outcome {
    Outcome::Done(json!({"ok": true, "result": v}))
}
fn err(e: impl std::fmt::Display) -> Outcome {
    Outcome::Done(json!({"ok": false, "error": e.to_string()}))
}
fn wrap(r: Result<Value, String>) -> Outcome {
    match r {
        Ok(v) => ok(v),
        Err(e) => err(e),
    }
}

fn key_from(name: &str) -> Option<egui::Key> {
    egui::Key::from_name(name).or(match name.to_ascii_lowercase().as_str() {
        "enter" | "return" => Some(egui::Key::Enter),
        "esc" | "escape" => Some(egui::Key::Escape),
        "delete" => Some(egui::Key::Delete),
        "space" => Some(egui::Key::Space),
        "tab" => Some(egui::Key::Tab),
        _ => None,
    })
}

/// Run one command as `engine.execute` does: through `app.run`, then the first-body fit and the
/// camera the command asked for.
fn execute(app: &mut SolveApp, id: &str, params: Value) -> Result<Value, String> {
    let r = app.run(id, params);
    if r.is_ok()
        && id != "view.navigate"
        && app.session.active_sketch.is_none()
        && app.session.world_state().bodies.len() == 1
        && app.session.undo.len() <= 2
    {
        app.fit_view();
    }
    // After the first-body fit, so a camera the command asked for (`view.apply`) wins.
    app.follow_view_request(false);
    r
}

/// A batch step's command id.
fn command(step: &Value) -> Option<&str> {
    step.get("command").or_else(|| step.get("id")).and_then(Value::as_str)
}

/// Most steps in one `engine.batch` (as in the MCP `batch` tool).
const MAX_BATCH: usize = 10_000;

/// Commands that load a file: through `engine.execute` they go to the app's file loader
/// (`file_load::control`: off the window thread, with progress and Cancel), which a batch would
/// bypass.
const LOADS_A_FILE: [&str; 3] = ["doc.open", "file.insert_step", "file.insert_mesh"];

/// `engine.batch {commands, rollback?}`: the steps in one request, so in one frame (#743). Each
/// step runs as `engine.execute` runs it; the first failure stops the batch and, with
/// `rollback` (the default), the design goes back to its checkpoint. Step failures are part of
/// the reply; refusals (`engine.batch unavailable: …`) come before anything runs, so the client
/// can send the steps one by one instead.
fn batch(app: &mut SolveApp, p: &Value) -> Outcome {
    let Some(steps) = p.get("commands").and_then(Value::as_array).filter(|s| s.len() <= MAX_BATCH) else {
        return err(format!("engine.batch: commands must be an array of at most {MAX_BATCH} steps"));
    };
    // The agent cursor acts out each `engine.execute` on its own.
    if app.ui.agent_cursor.show {
        return err("engine.batch unavailable: the agent cursor is shown");
    }
    if let Some((i, c)) = steps.iter().enumerate().find_map(|(i, step)| command(step).filter(|c| LOADS_A_FILE.contains(c)).map(|c| (i, c))) {
        return err(format!("engine.batch unavailable: step {i} ({c}) loads a file"));
    }
    let rollback = p.get("rollback").and_then(Value::as_bool).unwrap_or(true);
    let checkpoint = if rollback { app.run("edit.checkpoint", json!({})).ok().and_then(|v| v["checkpoint"].as_u64()) } else { None };
    let mut results = Vec::with_capacity(steps.len());
    for (i, step) in steps.iter().enumerate() {
        let r = match command(step) {
            Some(id) => execute(app, id, step.get("params").cloned().filter(|v| !v.is_null()).unwrap_or(json!({}))),
            None => Err("missing `command`".to_string()),
        };
        match r {
            Ok(v) => results.push(v),
            Err(e) => {
                let restored = checkpoint.is_some_and(|id| app.run("edit.restore_checkpoint", json!({"checkpoint": id})).is_ok());
                let failed = json!({"index": i, "command": command(step), "error": e});
                return ok(json!({"completed": i, "results": results, "failed": failed, "undone": if restored { i } else { 0 }}));
            }
        }
    }
    if let Some(id) = checkpoint {
        let _ = app.run("edit.restore_checkpoint", json!({"checkpoint": id, "forget": true}));
    }
    ok(json!({"completed": results.len(), "results": results, "undone": 0}))
}

pub fn handle(app: &mut SolveApp, ctx: &egui::Context, req: &ControlRequest) -> Outcome {
    // A request from an MCP server carries the folders its agent may use; every file the
    // request reaches must lie inside them (docs/mcp.md).
    let Some(allowed) = req.params.get("allow_paths") else { return dispatch(app, ctx, &req.method, &req.params) };
    let Some(roots) = allowed.as_array().and_then(|a| a.iter().map(|r| r.as_str().map(std::path::PathBuf::from)).collect::<Option<Vec<_>>>()) else {
        return err("`allow_paths` must be a list of folders");
    };
    let mut p = req.params.clone();
    if let Some(o) = p.as_object_mut() {
        o.remove("allow_paths");
    }
    // Plus the app's own recovery folder and, when it is a safe one, the open design's folder.
    let roots = roots.into_iter().chain(solidcraft_engine::recovery::default_dir());
    let scope = vfs::PathScope::new(roots).with_design(app.session.path.as_deref());
    vfs::scoped(Some(scope), || dispatch(app, ctx, &req.method, &p))
}

fn dispatch(app: &mut SolveApp, ctx: &egui::Context, method: &str, p: &Value) -> Outcome {
    let s = |k: &str| p.get(k).and_then(Value::as_str);
    let f = |k: &str| p.get(k).and_then(Value::as_f64).filter(|x| x.is_finite());
    let b = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
    match method {
        "engine.execute" | "command" => {
            let Some(id) = s("command").or(s("id")) else { return err("missing `command`") };
            wrap(execute(app, id, p.get("params").cloned().filter(|v| !v.is_null()).unwrap_or(json!({}))))
        }
        "engine.batch" => batch(app, p),
        "ui.agent_cursor" => {
            let s = &mut app.ui.agent_cursor;
            if let Some(v) = p.get("show").and_then(Value::as_bool) {
                s.show = v;
            }
            if let Some(v) = p.get("follow_camera").and_then(Value::as_bool) {
                s.follow_camera = v;
            }
            if let Some(v) = p.get("speed") {
                match crate::agent_cursor::Speed::from_value(v) {
                    Some(sp) => s.speed = sp,
                    None => return err("speed must be a number from 1 to 5, or \"instant\""),
                }
            }
            ok(json!(app.ui.agent_cursor))
        }
        "engine.script" => {
            let r = app.session.run_script(p).map(|v| json!(v)).map_err(|e| e.to_string());
            app.follow_view_request(false);
            wrap(r)
        }
        "engine.commands" => wrap(app.session.execute("engine.commands", &json!({})).map_err(|e| e.to_string())),
        "document.inspect" => wrap(app.session.execute("document.inspect", p).map_err(|e| e.to_string())),
        "ui.inspect" => ok(json!({
            "ui": serde_json::to_value(&app.ui).unwrap_or_default(),
            "visibility": app.session.visibility,
            "viewport": app.viewport.rect.map(|r| json!([r.left(), r.top(), r.width(), r.height()])),
            "camera": app.cam,
            "tool": app.tool.as_ref().map(|t| t.cmd),
            "dialog": app.dialog.as_ref().map(|d| format!("{d:?}")),
            "status": app.status.as_ref().map(|(text, _, error)| json!({"text": text, "error": error})),
            "hover": app.viewport.hover.as_ref().map(|h| format!("{h:?}")),
            "renderer": if app.viewport.gpu.is_some() { "gpu" } else { "cpu" },
            "frame_ms": app.frame_ms,
            "file_busy": app.file_load.busy(),
            "build_ms": app.viewport.build_ms,
            "preview": {"active": app.preview.active, "busy": app.preview.busy, "error": app.preview.error, "ms": app.preview.ms, "replaced": app.preview.replaced},
            "home": app.home.open,
            "recent_thumbnails_ready": app.home.ready_recents(),
            "documents": app.docs.count(),
            "quit_requested": app.quit_requested,
            "quitting": app.docs.quitting.is_some(),
        })),
        "ui.file" => {
            let Some(path) = s("path") else { return err("path is required") };
            if p.get("insert").and_then(Value::as_bool).unwrap_or(false) {
                app.insert_path_async(path);
            } else {
                app.open_path_async(path);
            }
            ok(json!({"busy": app.file_load.busy()}))
        }
        "ui.documents" => ok(crate::documents::control(app, p)),
        "ui.home" => {
            match p.get("open").and_then(Value::as_bool) {
                Some(true) => {
                    let rev = app.session.revision;
                    app.home.show(rev);
                }
                Some(false) => app.home.open = false,
                None => {}
            }
            if let Some(i) = p.get("sample").and_then(Value::as_u64) {
                crate::home::open_sample(app, i as usize);
            }
            ok(json!({"open": app.home.open, "recent": app.home.recent}))
        }
        "ui.set" => {
            // What is hidden lives in the session (`browser.visibility`); `ui.set` still sets it.
            const VIS: [(&str, &str); 4] = [
                ("hiddenBodies", "hidden_bodies"),
                ("hiddenOrigin", "hidden_origin"),
                ("hiddenSketches", "hidden_sketches"),
                ("shownSketches", "shown_sketches"),
            ];
            let mut cur = serde_json::to_value(&app.ui).unwrap_or(json!({}));
            let mut vis = serde_json::to_value(&app.session.visibility).unwrap_or(json!({}));
            if let (Some(o), Some(vo), Some(src)) = (cur.as_object_mut(), vis.as_object_mut(), p.as_object()) {
                for (k, v) in src {
                    match VIS.iter().find(|(camel, _)| camel == k) {
                        Some((_, snake)) => vo.insert(snake.to_string(), v.clone()),
                        None => o.insert(k.clone(), v.clone()),
                    };
                }
            }
            match (serde_json::from_value::<crate::UiState>(cur), serde_json::from_value::<solidcraft_engine::Visibility>(vis)) {
                (Ok(u), Ok(v)) => {
                    app.ui = u;
                    app.session.visibility = v;
                    ok(serde_json::to_value(&app.ui).unwrap_or_default())
                }
                (Err(e), _) | (_, Err(e)) => err(e),
            }
        }
        "ui.view" => {
            let v = s("view").unwrap_or("home");
            app.viewport.orbit_snap = None;
            app.viewport.orbit_free = None;
            if p.get("animate").and_then(Value::as_bool).unwrap_or(false) {
                if v != "fit" && v != "home" && StandardView::parse(v).is_none() {
                    return err(format!("unknown view `{v}`"));
                }
                app.animate_view(v);
                return ok(json!({ "animating": true, "to": app.cam_anim.map(|a| a.to) }));
            }
            app.cancel_view_animation();
            match v {
                "fit" => app.fit_view(),
                "home" => {
                    app.cam.set_view(StandardView::Iso);
                    app.fit_view();
                }
                v => match StandardView::parse(v) {
                    Some(sv) => app.cam.set_view(sv),
                    None => return err(format!("unknown view `{v}`")),
                },
            }
            ok(json!(app.cam))
        }
        "ui.start" => {
            let Some(id) = s("command") else { return err("missing `command`") };
            app.start(id);
            ok(json!({"tool": app.tool.as_ref().map(|t| t.cmd), "dialog": app.dialog.as_ref().map(|d| format!("{d:?}"))}))
        }
        "ui.move" | "ui.click" => {
            let (Some(x), Some(y)) = (f("x"), f("y")) else { return err("missing x/y") };
            let pos = egui::pos2(x as f32, y as f32);
            app.synthetic.push(egui::Event::PointerMoved(pos));
            if method == "ui.click" {
                let button = match s("button") {
                    Some("right") => egui::PointerButton::Secondary,
                    Some("middle") => egui::PointerButton::Middle,
                    _ => egui::PointerButton::Primary,
                };
                let modifiers = egui::Modifiers { shift: b("shift"), ctrl: b("ctrl"), command: b("ctrl"), alt: b("alt"), ..Default::default() };
                // `double: true` sends two clicks in one frame, which egui reads as a double-click.
                for _ in 0..if b("double") { 2 } else { 1 } {
                    app.synthetic.push(egui::Event::PointerButton { pos, button, pressed: true, modifiers });
                    app.synthetic.push(egui::Event::PointerButton { pos, button, pressed: false, modifiers });
                }
            }
            ok(Value::Null)
        }
        "ui.drag" => {
            let (Some(x0), Some(y0), Some(x1), Some(y1)) = (f("x0"), f("y0"), f("x1"), f("y1")) else { return err("missing x0/y0/x1/y1") };
            let button = match s("button") {
                Some("right") => egui::PointerButton::Secondary,
                Some("middle") => egui::PointerButton::Middle,
                _ => egui::PointerButton::Primary,
            };
            let modifiers = egui::Modifiers { shift: b("shift"), ctrl: b("ctrl"), command: b("ctrl"), alt: b("alt"), ..Default::default() };
            let (a, z) = (egui::pos2(x0 as f32, y0 as f32), egui::pos2(x1 as f32, y1 as f32));
            app.synthetic.push(egui::Event::PointerMoved(a));
            app.synthetic.push(egui::Event::PointerButton { pos: a, button, pressed: true, modifiers });
            // `hold`: frames to keep the button down before moving (press-and-hold).
            for _ in 0..f("hold").unwrap_or(0.0).clamp(0.0, 600.0) as usize {
                app.synthetic.push(egui::Event::PointerMoved(a));
            }
            let steps = f("steps").unwrap_or(10.0).clamp(1.0, 200.0) as usize;
            for k in 1..=steps {
                app.synthetic.push(egui::Event::PointerMoved(a + (z - a) * (k as f32 / steps as f32)));
            }
            // `hold_end`: frames to keep the button down at the end (to look at a drag's snap).
            for _ in 0..f("hold_end").unwrap_or(0.0).clamp(0.0, 600.0) as usize {
                app.synthetic.push(egui::Event::PointerMoved(z));
            }
            app.synthetic.push(egui::Event::PointerButton { pos: z, button, pressed: false, modifiers });
            ok(Value::Null)
        }
        "ui.editFeature" => {
            let key = match p.get("feature") {
                Some(Value::Number(n)) => n.to_string(),
                Some(Value::String(x)) => x.clone(),
                _ => return err("`feature` must be an id or name"),
            };
            let Some(id) = app.session.doc.find_feature(&key).map(|f| f.id) else { return err(format!("no feature `{key}`")) };
            app.edit_feature(id);
            ok(json!({"dialog": app.dialog.as_ref().map(|d| format!("{d:?}")), "marker": app.session.doc.marker}))
        }
        "ui.selection" => ok(json!({
            "selection": app.session.selection,
            "dialog": app.dialog.as_ref().map(|d| d.inputs.iter().map(|i| json!({"label": i.label, "items": i.items})).collect::<Vec<_>>()),
            "hover": app.viewport.hover.as_ref().map(|h| format!("{h:?}")),
            "dimensions": crate::dim_view::drawn().into_iter().map(|(id, p)| json!({"id": id, "x": p.x, "y": p.y})).collect::<Vec<_>>(),
            "dimension_selected": crate::dim_view::selected(app),
            "dimension_editing": crate::dim_view::editing(),
        })),
        "ui.triad" => {
            // Screen points of the 3D move triad's handles (x, y, z arrows and the plane square).
            let Some(rect) = app.viewport.rect else { return err("no viewport") };
            let proj = crate::viewport::projection(app, rect);
            ok(json!(
                crate::sketch3d::handle_points(app, &proj).into_iter().map(|(n, p)| json!({"handle": n, "x": p.x, "y": p.y})).collect::<Vec<_>>()
            ))
        }
        "ui.dialogInput" => {
            // What clicking a selection input's chip does: make it the one picks go to.
            let i = p.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            match app.dialog.as_mut() {
                Some(d) if i < d.inputs.len() => {
                    d.active = i;
                    ok(json!({"label": d.inputs.get(i).map(|x| x.label)}))
                }
                _ => err("no dialog input with that index"),
            }
        }
        "ui.handles" => {
            // The handles drawn last frame (toolbar buttons `toolbar:<id>`, panels, browser rows…).
            let list = crate::scenario::handles(s("prefix").unwrap_or(""));
            ok(json!(list.into_iter().map(|(n, p)| json!({"name": n, "x": p.x, "y": p.y})).collect::<Vec<_>>()))
        }
        "ui.at" => {
            if let Some(path) = p.get("occurrence").and_then(Value::as_array) {
                let ids: Option<Vec<u64>> = path.iter().map(Value::as_u64).collect();
                let Some(ids) = ids else { return err("occurrence path must contain integer ids") };
                let key = ids.iter().map(u64::to_string).collect::<Vec<_>>().join("/");
                let control = s("control").unwrap_or("row");
                return match crate::scenario::handle_at(&format!("{control}:occurrence:{key}")) {
                    Some(q) => ok(json!([q.x, q.y])),
                    None => err("occurrence row or control is not shown"),
                };
            }
            // A scenario point: {world: [x,y,z]} | {sketch: [x,y]} | {plane: "XY"} (the middle of an
            // origin plane's square) → screen point.
            if let Some(h) = p.get("handle").and_then(Value::as_str) {
                return match crate::scenario::handle_at(h) {
                    Some(q) => ok(json!([q.x, q.y])),
                    None => err(format!("handle {h} is not shown")),
                };
            }
            // A browser row by its label: the row itself, or its fold arrow.
            for (k, chevron) in [("browser", false), ("chevron", true)] {
                if let Some(label) = p.get(k).and_then(Value::as_str) {
                    let rows = crate::browser::drawn_rows();
                    let found = rows
                        .iter()
                        .find(|r| r.0 == label)
                        .and_then(|r| if chevron { r.2.map(|f| f.center()) } else { Some(r.1.left_center() + egui::vec2(120.0, 0.0)) });
                    return match found {
                        Some(q) => ok(json!([q.x, q.y])),
                        None => err(format!("no browser row `{label}`{} on screen", if chevron { " with a fold arrow" } else { "" })),
                    };
                }
            }
            if let Some(d) = p.get("dimension").and_then(Value::as_str) {
                return match crate::dim_view::text_at(d) {
                    Some(q) => ok(json!([q.x, q.y])),
                    None => err(format!("dimension {d} is not shown")),
                };
            }
            let Some(rect) = app.viewport.rect else { return err("no viewport") };
            let proj = crate::viewport::projection(app, rect);
            let num = |v: &Value, i: usize| v.get(i).and_then(Value::as_f64).unwrap_or(0.0);
            let world = if let Some(w) = p.get("world") {
                Some(solidcraft_engine::geom::Vec3::new(num(w, 0), num(w, 1), num(w, 2)))
            } else if let Some(s) = p.get("sketch") {
                let st = app.session.world_state();
                app.session
                    .active_sketch
                    .and_then(|id| st.sketch(id))
                    .map(|ss| ss.plane.to_world(solidcraft_engine::geom::Vec2::new(num(s, 0), num(s, 1))))
            } else if let Some(n) = p.get("plane").and_then(Value::as_str) {
                crate::viewport::origin_plane_target(app, &proj, n)
            } else if let Some(n) = p.get("axis").and_then(Value::as_str) {
                // Along an origin axis, on its solid part.
                let size = crate::selection::origin_size(app.cam.half_height());
                crate::viewport::origin_axes(app).into_iter().find(|(name, _)| *name == n).map(|(_, d)| d * (size * 0.8))
            } else if let Some(n) = p.get("gizmo").and_then(Value::as_str) {
                // A handle of the move triad ("x", "xy", "rz", "ball"), moved `by` [x,y,z] mm or
                // turned `turn` degrees (rings).
                let by = p.get("by").map(|b| solidcraft_engine::geom::Vec3::new(num(b, 0), num(b, 1), num(b, 2))).unwrap_or_default();
                crate::gizmo::handle_point(n, by, p.get("turn").and_then(Value::as_f64).unwrap_or(0.0))
            } else {
                None
            };
            match world.and_then(|w| proj.to_screen(w)) {
                Some(q) => ok(json!([q.x, q.y])),
                None => err(format!("not on screen: {}", p)),
            }
        }
        "ui.worldToScreen" => {
            let Some(rect) = app.viewport.rect else { return err("no viewport") };
            let proj = crate::viewport::projection(app, rect);
            let pts: Vec<Value> = p
                .get("points")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .take(1000)
                .map(|v| {
                    let c = |i: usize| v.get(i).and_then(Value::as_f64).unwrap_or(0.0);
                    proj.to_screen(solidcraft_engine::geom::Vec3::new(c(0), c(1), c(2))).map_or(Value::Null, |q| json!([q.x, q.y]))
                })
                .collect();
            ok(json!(pts))
        }
        "ui.sketchToScreen" => {
            // Screen points of active-sketch coordinates (for driving sketch interaction).
            let st = app.session.world_state();
            let (Some(rect), Some(ss)) = (app.viewport.rect, app.session.active_sketch.and_then(|s| st.sketch(s))) else {
                return err("no active sketch in a drawn viewport");
            };
            let proj = crate::viewport::projection(app, rect);
            let pts: Vec<Value> = p
                .get("points")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .take(1000)
                .map(|v| {
                    let c = |i: usize| v.get(i).and_then(Value::as_f64).unwrap_or(0.0);
                    let q = solidcraft_engine::geom::Vec2::new(c(0), c(1));
                    proj.to_screen(ss.plane.to_world(q)).map_or(Value::Null, |p| json!([p.x, p.y]))
                })
                .collect();
            ok(json!(pts))
        }
        "ui.scroll" => {
            let (Some(x), Some(y)) = (f("x"), f("y")) else { return err("missing x/y") };
            app.synthetic.push(egui::Event::PointerMoved(egui::pos2(x as f32, y as f32)));
            // Mouse-wheel semantics (lines): zooms regardless of the trackpad pan
            // preference, so scripts and agents keep working. Trackpads report points.
            let speed = ctx.options(|o| o.input_options.line_scroll_speed).max(1.0);
            let lines = f("delta").unwrap_or(-100.0) / f64::from(speed);
            app.synthetic.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, lines as f32),
                modifiers: Default::default(),
                phase: egui::TouchPhase::Move,
            });
            ok(Value::Null)
        }
        "ui.key" => {
            let Some(key) = s("key").and_then(key_from) else { return err("unknown key") };
            let modifiers = egui::Modifiers { shift: b("shift"), command: b("cmd"), ctrl: b("cmd") || b("ctrl"), ..Default::default() };
            app.synthetic.push(egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
            app.synthetic.push(egui::Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers });
            ok(Value::Null)
        }
        "ui.text" => {
            app.synthetic.push(egui::Event::Text(s("text").unwrap_or("").to_string()));
            ok(Value::Null)
        }
        "ui.menu" => {
            if b("close") {
                crate::context_menu::close(app);
                return ok(json!({"open": false}));
            }
            if let (Some(x), Some(y)) = (f("x"), f("y")) {
                let target = match p.get("target") {
                    Some(v) => match serde_json::from_value::<crate::context_menu::Target>(v.clone()) {
                        Ok(t) => t,
                        Err(e) => return err(e),
                    },
                    None => crate::context_menu::Target::Viewport,
                };
                crate::context_menu::request_open(app, egui::pos2(x as f32, y as f32), target);
                return ok(json!({"pending": true}));
            }
            ok(crate::context_menu::describe(app))
        }
        "ui.confirm" => match p.get("accept").and_then(Value::as_bool) {
            Some(a) => {
                if crate::delete::confirm(app, a) {
                    ok(json!({"accepted": a}))
                } else {
                    err("nothing is waiting for a confirmation")
                }
            }
            None => ok(json!({"pending": app.menu.confirm.as_ref().map(|c| c.report.clone())})),
        },
        "ui.shortcut" => {
            let Some(id) = s("command") else { return err("missing `command`") };
            match s("key") {
                Some(k) => match crate::keymap::bind(app, id, k, b("replace")) {
                    Ok(()) => ok(json!({"command": id, "key": crate::keymap::effective(app, id)})),
                    Err(e) => err(e),
                },
                None => ok(json!({"command": id, "key": crate::keymap::effective(app, id)})),
            }
        }
        "ui.browser" => ok(json!({
            "occurrences": crate::browser::drawn_occurrences().iter().map(|(path, label, r, open)| json!({"path": path, "label": label, "y": r.center().y, "open": open})).collect::<Vec<_>>(),
            "rows": crate::browser::drawn_rows().iter().map(|(label, r, _, open)| json!({"label": label, "y": r.center().y, "open": open})).collect::<Vec<_>>(),
            "selection": app.session.selection,
            "picked_components": app.tree.picked_components,
        })),
        "ui.capture" => match s("action") {
            Some(a) => match crate::browser::answer_capture(app, a) {
                Ok(()) => ok(json!({"pending_moves": app.session.pending_moves.len()})),
                Err(e) => err(e),
            },
            None => ok(json!({"question": app.tree.capture_prompt, "pending_moves": app.session.pending_moves.len()})),
        },
        "ui.prefs" => wrap(crate::prefs::control(app, p)),
        "ui.help" => match s("item") {
            Some(i @ ("about" | "shortcuts" | "report" | "crashes")) => {
                crate::help::run(app, ctx, i);
                ok(
                    json!({"diagnostics": crate::help::diagnostics(app), "version": crate::help::VERSION, "commit": crate::help::COMMIT, "built": crate::help::BUILD_DATE}),
                )
            }
            _ => ok(
                json!({"diagnostics": crate::help::diagnostics(app), "version": crate::help::VERSION, "commit": crate::help::COMMIT, "built": crate::help::BUILD_DATE}),
            ),
        },
        "ui.selectOther" => wrap(crate::select_other::control(app, p)),
        "ui.menuPick" => {
            let Some(key) = s("item") else { return err("missing `item` (id or label)") };
            wrap(crate::context_menu::pick(app, key))
        }
        "ui.rename" => {
            if crate::context_menu::finish_rename(app, s("text"), p.get("commit").and_then(Value::as_bool).unwrap_or(true)) {
                ok(json!({"done": true}))
            } else {
                err("no rename in progress")
            }
        }
        "ui.window" => match crate::titlebar::window_action(app, ctx, s("action").unwrap_or("")) {
            Ok(()) => ok(json!({"custom_titlebar": app.custom_titlebar, "maximized": ctx.input(|i| i.viewport().maximized)})),
            Err(e) => err(e),
        },
        "ui.resize" => {
            let (Some(w), Some(h)) = (f("width"), f("height")) else { return err("missing width/height") };
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w.clamp(320.0, 8192.0) as f32, h.clamp(240.0, 8192.0) as f32)));
            ok(Value::Null)
        }
        "ui.screenshot" => match s("path").map(vfs::check) {
            Some(Err(e)) => err(e),
            _ => Outcome::Screenshot { path: s("path").map(str::to_string) },
        },
        "ui.render" => {
            let w = f("width").unwrap_or(1280.0).clamp(16.0, 8192.0) as usize;
            let h = f("height").unwrap_or(800.0).clamp(16.0, 8192.0) as usize;
            // Cut by the section the viewport shows (an open Section Analysis dialog's preview,
            // a sketch's Slice), not only the committed one.
            let scene = solidcraft_engine::view::scene_with_section(&app.session, &app.cam, crate::viewport::section_plane(app));
            let Some(png) = render_png(&scene, &app.cam, w, h) else { return err("render failed") };
            match s("path") {
                Some(path) => match vfs::check(path).and_then(|()| std::fs::write(path, &png)) {
                    Ok(()) => ok(json!({"path": path, "bytes": png.len()})),
                    Err(e) => err(e),
                },
                None => ok(json!({"bytes": png.len()})),
            }
        }
        "app.quit" => {
            // `force`: quit without asking about unsaved designs.
            if p.get("force").and_then(Value::as_bool).unwrap_or(false) {
                app.quit_requested = true;
            } else {
                crate::documents::request_quit(app);
            }
            if app.quit_requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ok(json!({"quitting": app.quit_requested, "asking": app.docs.closing.is_some()}))
        }
        other => err(format!("unknown method `{other}`")),
    }
}

/// Save a screenshot PNG.
pub fn save_screenshot(image: &egui::ColorImage, path: Option<&str>) -> Value {
    let [w, h] = image.size;
    let mut buf = Vec::with_capacity(w * h * 4);
    for c in &image.pixels {
        buf.extend_from_slice(&c.to_array());
    }
    let Some(img) = image::RgbaImage::from_raw(w as u32, h as u32, buf) else { return json!({"ok": false, "error": "bad image"}) };
    let path = path.map(str::to_string).unwrap_or_else(|| std::env::temp_dir().join("solidcraft-shot.png").to_string_lossy().to_string());
    match img.save(&path) {
        Ok(()) => json!({"ok": true, "result": {"path": path, "width": w, "height": h}}),
        Err(e) => json!({"ok": false, "error": e.to_string()}),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use solidcraft_engine::{Sel, Session};

    use super::*;
    use crate::Services;

    fn render(app: &mut SolveApp, path: &std::path::Path) -> Vec<u8> {
        let (req, _rx) = ControlRequest::new("ui.render", json!({"path": path.to_string_lossy(), "width": 200, "height": 140}));
        match handle(app, &egui::Context::default(), &req) {
            Outcome::Done(v) => assert_eq!(v["ok"], true, "{v}"),
            Outcome::Screenshot { .. } => panic!("ui.render took a screenshot"),
        }
        std::fs::read(path).unwrap()
    }

    /// A model render shows the section the window shows: the open Section Analysis dialog's
    /// preview too, not only a committed `inspect.section`.
    #[test]
    fn model_render_shows_the_section_dialog_preview() {
        let mut s = Session::default();
        s.execute("solid.box", &json!({"length": 40, "width": 30, "height": 20})).unwrap();
        let mut app = SolveApp::new(s, Services::default());
        let dir = std::env::temp_dir().join(format!("solidcraft-render-section-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let whole = render(&mut app, &dir.join("whole.png"));
        app.session.selection = vec![Sel::Plane { name: "XZ".into() }];
        app.start("inspect.section");
        let Some(crate::dialogs::Kind::Section { offset, .. }) = app.dialog.as_mut().map(|d| &mut d.kind) else { panic!("no section dialog") };
        *offset = "15 mm".into();
        assert!(app.session.section.is_none() && crate::viewport::section_plane(&app).is_some());
        let preview = render(&mut app, &dir.join("preview.png"));
        assert!(whole != preview, "the model render ignores the section the window shows");
        // Committed, it renders the same.
        app.run("inspect.section", json!({"plane": "XZ", "offset": 15})).unwrap();
        app.dialog = None;
        assert!(render(&mut app, &dir.join("committed.png")) == preview, "the committed section renders differently");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn batch(h: &mut crate::scenario::Harness, params: serde_json::Value) -> serde_json::Value {
        h.call("engine.batch", params)
    }

    fn inspect(h: &mut crate::scenario::Harness) -> serde_json::Value {
        h.call("document.inspect", json!({}))["result"].clone()
    }

    /// Timing fields (`ms`) differ from run to run.
    fn without_timings(v: &serde_json::Value) -> serde_json::Value {
        match v {
            serde_json::Value::Object(o) => o.iter().filter(|(k, _)| *k != "ms").map(|(k, v)| (k.clone(), without_timings(v))).collect(),
            serde_json::Value::Array(a) => a.iter().map(without_timings).collect(),
            v => v.clone(),
        }
    }

    /// #743: a batch is one request through the app's queue, answered within a frame, however
    /// many steps it has.
    #[test]
    fn engine_batch_runs_in_one_frame() {
        let mut h = crate::scenario::Harness::new();
        let steps: Vec<_> = (0..40)
            .flat_map(|_| [json!({"command": "sketch.create", "params": {"plane": "XY"}}), json!({"command": "sketch.finish", "params": {}})])
            .collect();
        let (v, frames) = h.call_queued("engine.batch", json!({"commands": steps}));
        assert_eq!(v["ok"], true, "{v}");
        assert_eq!(v["result"]["completed"], 80, "{v}");
        assert_eq!(v["result"]["results"].as_array().map(Vec::len), Some(80));
        assert!(v["result"].get("failed").is_none(), "{v}");
        assert!(frames <= 2, "a batch of 80 steps took {frames} frames");
        assert_eq!(inspect(&mut h)["sketches"].as_array().map(Vec::len), Some(40));
    }

    #[test]
    fn engine_batch_rolls_back_at_the_first_failure() {
        let mut h = crate::scenario::Harness::new();
        assert_eq!(h.call("engine.execute", json!({"command": "solid.box", "params": {"length": 10, "width": 10, "height": 10}}))["ok"], true);
        let before = inspect(&mut h);
        let v = batch(
            &mut h,
            json!({"commands": [
                {"command": "solid.sphere", "params": {"radius": 3, "center": [50, 0, 0]}},
                {"command": "solid.extrude", "params": {"distance": "bogus +"}},
                {"command": "solid.sphere", "params": {"radius": 3, "center": [80, 0, 0]}},
            ]}),
        );
        assert_eq!(v["ok"], true, "{v}");
        let r = &v["result"];
        assert_eq!((r["completed"].clone(), r["failed"]["index"].clone(), r["undone"].clone()), (json!(1), json!(1), json!(1)), "{v}");
        assert_eq!(r["failed"]["command"], "solid.extrude");
        let after = inspect(&mut h);
        assert_eq!(after["undo"], before["undo"], "{after}");
        assert_eq!(after["bodies"], before["bodies"]);
        assert_eq!(without_timings(&after["timeline"]), without_timings(&before["timeline"]));
    }

    #[test]
    fn engine_batch_without_rollback_keeps_completed_steps() {
        let mut h = crate::scenario::Harness::new();
        let v = batch(
            &mut h,
            json!({"rollback": false, "commands": [
                {"command": "solid.sphere", "params": {"radius": 3}},
                {"command": "NoSuchCommand"},
            ]}),
        );
        assert_eq!(v["result"]["failed"]["index"], 1, "{v}");
        assert_eq!(v["result"]["undone"], 0, "{v}");
        assert_eq!(inspect(&mut h)["bodies"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn engine_batch_step_without_a_command_fails_that_step() {
        let mut h = crate::scenario::Harness::new();
        let v = batch(&mut h, json!({"commands": [{"params": {}}]}));
        assert_eq!(v["result"]["failed"]["index"], 0, "{v}");
        assert!(v["result"]["failed"]["error"].as_str().is_some_and(|e| e.contains("missing `command`")), "{v}");
    }

    #[test]
    fn engine_batch_is_refused_when_a_step_loads_a_file() {
        let mut h = crate::scenario::Harness::new();
        let v = batch(
            &mut h,
            json!({"commands": [
                {"command": "solid.box", "params": {"length": 10, "width": 10, "height": 10}},
                {"command": "doc.open", "params": {"path": "elsewhere.solidcraft"}},
            ]}),
        );
        assert_eq!(v["ok"], false, "{v}");
        assert_eq!(v["error"], "engine.batch unavailable: step 1 (doc.open) loads a file", "{v}");
        assert_eq!(inspect(&mut h)["bodies"].as_array().map(Vec::len), Some(0), "nothing may run before the refusal");
    }

    #[test]
    fn engine_batch_is_refused_while_the_agent_cursor_is_shown() {
        let mut h = crate::scenario::Harness::new();
        assert_eq!(h.call("ui.agent_cursor", json!({"show": true}))["ok"], true);
        let v = batch(&mut h, json!({"commands": [{"command": "solid.box", "params": {"length": 10, "width": 10, "height": 10}}]}));
        assert_eq!(v["ok"], false, "{v}");
        assert!(v["error"].as_str().is_some_and(|e| e.starts_with("engine.batch unavailable")), "{v}");
        assert_eq!(inspect(&mut h)["bodies"].as_array().map(Vec::len), Some(0));
    }

    /// Every step of a batch from an MCP server stays inside the agent's folders (#205), as an
    /// `engine.execute` would.
    #[test]
    fn engine_batch_stays_inside_allow_paths() {
        let mut h = crate::scenario::Harness::new();
        let base = vfs::temp_dir().join(format!("solidcraft-batch-scope-{}", std::process::id()));
        let (root, out) = (base.join("root"), base.join("out"));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        let export = |d: &std::path::Path| json!({"command": "file.export", "params": {"path": d.join("a.step").to_string_lossy()}});
        let v = batch(
            &mut h,
            json!({"allow_paths": [root.to_string_lossy()], "commands": [
                {"command": "solid.box", "params": {"length": 10, "width": 10, "height": 10}},
                export(&root),
                export(&out),
            ]}),
        );
        assert_eq!(v["result"]["failed"]["index"], 2, "{v}");
        assert!(v["result"]["failed"]["error"].as_str().is_some_and(|e| e.contains("outside the folders this agent may use")), "{v}");
        assert!(root.join("a.step").exists());
        assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0, "nothing written outside");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Each step runs as `engine.execute` runs it: same results, same design.
    #[test]
    fn engine_batch_matches_engine_execute() {
        let steps = [
            json!({"command": "sketch.create", "params": {"plane": "XY"}}),
            json!({"command": "sketch.rectangle.two_point", "params": {"p0": [0, 0], "p1": [40, 30]}}),
            json!({"command": "sketch.finish", "params": {}}),
            json!({"command": "solid.extrude", "params": {"distance": 20}}),
        ];
        let mut a = crate::scenario::Harness::new();
        let batched = batch(&mut a, json!({"commands": steps}));
        let mut b = crate::scenario::Harness::new();
        let one_by_one: Vec<_> = steps.iter().map(|s| b.call("engine.execute", s.clone())["result"].clone()).collect();
        assert_eq!(without_timings(&batched["result"]["results"]), without_timings(&json!(one_by_one)), "{batched}");
        assert_eq!(without_timings(&inspect(&mut a)["timeline"]), without_timings(&inspect(&mut b)["timeline"]));
        assert_eq!(a.app.cam, b.app.cam, "the first-body fit must happen as for engine.execute");
    }

    /// A request carrying `allow_paths` (from an MCP server, #205) reaches files only inside
    /// those folders; the same request without it, like the user's own dialogs, is unconfined.
    #[test]
    fn requests_with_allow_paths_stay_inside_them() {
        let mut s = Session::default();
        s.execute("solid.box", &json!({"length": 10, "width": 10, "height": 10})).unwrap();
        let mut app = SolveApp::new(s, Services::default());
        let base = vfs::temp_dir().join(format!("solidcraft-control-scope-{}", std::process::id()));
        let (root, out) = (base.join("root"), base.join("out"));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        let at = |d: &std::path::Path, f: &str| d.join(f).to_string_lossy().to_string();
        let mut call = |method: &str, mut params: Value, scoped: bool| {
            if scoped {
                params["allow_paths"] = json!([root.to_string_lossy()]);
            }
            let (req, _rx) = ControlRequest::new(method, params);
            match handle(&mut app, &egui::Context::default(), &req) {
                Outcome::Done(v) => v,
                Outcome::Screenshot { path } => json!({"ok": true, "screenshot": path}),
            }
        };
        let export = |p: String| json!({"command": "file.export", "params": {"path": p}});
        assert_eq!(call("engine.execute", export(at(&root, "a.step")), true)["ok"], true);
        assert_eq!(call("ui.render", json!({"path": at(&root, "a.png"), "width": 32, "height": 32}), true)["ok"], true);
        assert_eq!(call("ui.screenshot", json!({"path": at(&root, "b.png")}), true)["ok"], true);
        for (method, params) in [
            ("engine.execute", export(at(&out, "a.step"))),
            ("engine.execute", export(at(&root, "../out/a.step"))),
            ("engine.script", json!({"commands": [{"command": "file.export", "params": {"path": at(&out, "a.step")}}]})),
            ("ui.render", json!({"path": at(&out, "a.png"), "width": 32, "height": 32})),
            ("ui.screenshot", json!({"path": at(&out, "b.png")})),
        ] {
            let v = call(method, params, true);
            assert!(v["ok"] == false && v.to_string().contains("outside the folders this agent may use"), "{method}: {v}");
        }
        assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0, "nothing written outside");
        assert_eq!(call("engine.execute", export(at(&out, "a.step")), false)["ok"], true, "without allow_paths: unconfined");
        let bad = call("engine.execute", json!({"command": "file.new", "allow_paths": "/"}), false);
        assert!(bad["error"].as_str().is_some_and(|e| e.contains("allow_paths")), "{bad}");
        let _ = std::fs::remove_dir_all(&base);
    }
}
