//! Interactive file operations run off the window thread. The old design stays intact until
//! the command and display meshes are ready; failures and cancelled jobs never replace it.

use std::collections::VecDeque;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
    mpsc::{Receiver, TryRecvError},
};

use serde_json::Value;
#[cfg(not(target_arch = "wasm32"))]
use serde_json::json;
use solidcraft_engine::{Session, doc::Document};

use crate::SolveApp;

struct Request {
    path: String,
    insert: bool,
}
struct Made {
    session: Session,
    result: Value,
    image: Option<egui::ColorImage>,
}
type Job = Result<Made, String>;
struct Running {
    request: Request,
    base: Arc<Document>,
    base_revision: u64,
    before: Vec<String>,
    rx: Receiver<Job>,
    phase: Arc<AtomicU8>,
    cancelled: Arc<AtomicBool>,
    preview_rx: Option<Receiver<egui::ColorImage>>,
    preview: Option<egui::TextureHandle>,
}

impl Running {
    fn matches(&self, session: &Session) -> bool {
        Arc::ptr_eq(&session.doc, &self.base) && session.revision == self.base_revision
    }
}

#[derive(Default)]
pub struct FileLoad {
    queued: VecDeque<Request>,
    running: Option<Running>,
}

impl FileLoad {
    pub fn busy(&self) -> bool {
        self.running.as_ref().is_some_and(|r| !r.cancelled.load(Ordering::Relaxed)) || !self.queued.is_empty()
    }
}

pub fn begin(app: &mut SolveApp, path: &str, insert: bool) {
    #[cfg(target_arch = "wasm32")]
    {
        if insert {
            app.insert_path(path);
        } else {
            app.open_path(path);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if app.file_load.queued.len() >= crate::documents::MAX_OPEN {
            app.set_status("too many queued files", true);
            return;
        }
        app.file_load.queued.push_back(Request { path: path.into(), insert });
        start(app);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn start(app: &mut SolveApp) {
    if app.file_load.running.is_some() {
        return;
    }
    let Some(request) = app.file_load.queued.pop_front() else { return };
    if !request.insert && app.docs.count() >= crate::documents::MAX_OPEN && !crate::documents::active_is_blank(app) {
        app.set_status("too many open designs; close a tab before opening another", true);
        return;
    }
    let before = if request.insert { app.session.world_state().bodies.iter().map(|b| b.name.clone()).collect() } else { Vec::new() };
    let mut scratch = if request.insert { app.session.scratch() } else { Session::default() };
    if request.insert {
        scratch.selection = app.session.selection.clone();
    }
    let base = app.session.doc.clone();
    let base_revision = app.session.revision;
    let phase = Arc::new(AtomicU8::new(0));
    let worker_phase = phase.clone();
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = cancelled.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let (preview_tx, preview_rx) = std::sync::mpsc::channel();
    let path = request.path.clone();
    let insert = request.insert;
    let spawned = std::thread::Builder::new().name("file-load".into()).spawn(move || {
        let preview_began = std::time::Instant::now();
        if !insert {
            let cancelled = worker_cancelled.clone();
            scratch.open_preview = Some(Box::new(move |preview| {
                if !cancelled.load(Ordering::Relaxed) {
                    // Render on the file worker, never the window thread.
                    let picture = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| saved_picture(preview))).ok().flatten();
                    if let Some(picture) = picture.filter(|_| !cancelled.load(Ordering::Relaxed)) {
                        let _ = preview_tx.send(picture);
                        if solidcraft_engine::geom::env::var_os("PROFILE_OPEN").is_some() {
                            eprintln!("SolidCraft saved preview: ready_ms={:.3}", preview_began.elapsed().as_secs_f64() * 1000.0);
                        }
                    }
                }
            }));
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| compute(scratch, &path, insert, &worker_phase, &worker_cancelled)))
            .unwrap_or_else(|_| Err("the file operation failed; the current design is unchanged".into()));
        let _ = tx.send(result);
    });
    match spawned {
        Ok(_) => {
            app.file_load.running =
                Some(Running { request, base, base_revision, before, rx, phase, cancelled, preview_rx: Some(preview_rx), preview: None })
        }
        Err(e) => app.set_status(format!("could not start file operation: {e}"), true),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn saved_picture(preview: solidcraft_engine::io::DesignPreview) -> Option<egui::ColorImage> {
    use solidcraft_engine::{
        geom::{Aabb3, Mesh, Vec3},
        render::{Camera, Rgb, Scene, SceneMesh, StandardView},
    };
    let mut scene = Scene::default();
    let mut bounds = Aabb3::EMPTY;
    for cached in preview.meshes {
        let mut mesh = Mesh {
            positions: cached.positions.as_chunks::<3>().0.iter().map(|p| Vec3::new(p[0], p[1], p[2])).collect(),
            triangles: cached.triangles.as_chunks::<3>().0.to_vec(),
            ..Default::default()
        };
        mesh.normals.resize(mesh.positions.len(), Vec3::ZERO);
        for triangle in &mesh.triangles {
            let a = *mesh.positions.get(*triangle.first()? as usize)?;
            let b = *mesh.positions.get(*triangle.get(1)? as usize)?;
            let c = *mesh.positions.get(*triangle.get(2)? as usize)?;
            let normal = (b - a).cross(c - a).normalized().unwrap_or(Vec3::Z);
            for index in triangle {
                if let Some(n) = mesh.normals.get_mut(*index as usize) {
                    *n += normal;
                }
            }
        }
        for normal in &mut mesh.normals {
            *normal = normal.normalized().unwrap_or(Vec3::Z);
        }
        bounds = bounds.union(&mesh.bounds());
        let [r, g, b] = cached.color.unwrap_or([0.69, 0.73, 0.78]).map(|c| (c * 255.0).round() as u8);
        scene.meshes.push(SceneMesh { mesh: Arc::new(mesh), color: Rgb(r, g, b), face_colors: Vec::new() });
    }
    if bounds.is_empty() {
        return None;
    }
    let mut camera = Camera::default();
    camera.set_view(StandardView::Iso);
    camera.fit(&bounds);
    scene.radius = bounds.diagonal();
    if !preview_work_fits(&scene, &camera) {
        return None;
    }
    let canvas = solidcraft_engine::render::render(&scene, &camera, 320, 200);
    Some(egui::ColorImage::from_rgba_unmultiplied([320, 200], &canvas.rgba))
}

/// Bound raster work as well as file size: repeated full-screen triangles must
/// not monopolize the file worker. Match the rasterizer's 2x sampling rectangles.
#[cfg(not(target_arch = "wasm32"))]
fn preview_work_fits(scene: &solidcraft_engine::render::Scene, camera: &solidcraft_engine::render::Camera) -> bool {
    let mut pixels = 0.0;
    let projection = camera.view_proj(640.0 / 400.0, scene.radius.max(1.0));
    for body in &scene.meshes {
        let points: Vec<_> = body
            .mesh
            .positions
            .iter()
            .map(|p| {
                let q = projection.apply(*p);
                (q[3] > 1e-9).then(|| ((q[0] / q[3] + 1.0) * 320.0, (1.0 - q[1] / q[3]) * 200.0, q[2] / q[3]))
            })
            .collect();
        for triangle in &body.mesh.triangles {
            let mut lo = [f64::INFINITY; 2];
            let mut hi = [f64::NEG_INFINITY; 2];
            let mut visible = true;
            for index in triangle {
                if let Some(Some((x, y, _))) = points.get(*index as usize) {
                    lo = [lo[0].min(*x), lo[1].min(*y)];
                    hi = [hi[0].max(*x), hi[1].max(*y)];
                } else {
                    visible = false;
                    break;
                }
            }
            if visible {
                let width = (hi[0].ceil().min(639.0) - lo[0].floor().max(0.0) + 1.0).max(0.0);
                let height = (hi[1].ceil().min(399.0) - lo[1].floor().max(0.0) + 1.0).max(0.0);
                pixels += width * height;
                if !pixels.is_finite() || pixels > 16_000_000.0 {
                    return false;
                }
            }
        }
    }
    true
}

#[cfg(not(target_arch = "wasm32"))]
fn compute(mut session: Session, path: &str, insert: bool, phase: &AtomicU8, cancelled: &AtomicBool) -> Job {
    let profile = solidcraft_engine::geom::env::var_os("PROFILE_OPEN").is_some();
    let began = std::time::Instant::now();
    if cancelled.load(Ordering::Relaxed) {
        return Err("file operation cancelled".into());
    }
    let command = if !insert {
        "doc.open"
    } else if solidcraft_engine::io::is_mesh_path(path) {
        "file.insert_mesh"
    } else {
        "file.insert_step"
    };
    let result = session.execute(command, &json!({"path": path})).map_err(|e| e.to_string())?;
    let geometry_ms = began.elapsed().as_secs_f64() * 1000.0;
    let view_began = std::time::Instant::now();
    phase.store(1, Ordering::Relaxed);
    let world = session.world_state();
    let placement_ms = view_began.elapsed().as_secs_f64() * 1000.0;
    let mesh_began = std::time::Instant::now();
    // Fit, picking and the first viewport frame must not trigger cold tessellation on the UI.
    for b in &world.bodies {
        if cancelled.load(Ordering::Relaxed) {
            return Err("file operation cancelled".into());
        }
        let _ = b.mesh();
    }
    let tessellation_ms = mesh_began.elapsed().as_secs_f64() * 1000.0;
    phase.store(2, Ordering::Relaxed);
    let view_ms = view_began.elapsed().as_secs_f64() * 1000.0;
    let thumbnail_began = std::time::Instant::now();
    let image = if insert { None } else { crate::home::remember(&session) };
    if profile {
        eprintln!(
            "SolidCraft open phases: geometry_ms={geometry_ms:.3} placement_ms={placement_ms:.3} tessellation_ms={tessellation_ms:.3} display_warmup_ms={view_ms:.3} thumbnail_ms={:.3} total_ms={:.3}",
            thumbnail_began.elapsed().as_secs_f64() * 1000.0,
            began.elapsed().as_secs_f64() * 1000.0
        );
    }
    Ok(Made { session, result, image })
}

pub fn tick(app: &mut SolveApp, ctx: &egui::Context) {
    if let Some(running) = &mut app.file_load.running {
        if !running.cancelled.load(Ordering::Relaxed) && running.matches(&app.session) {
            if let Some(image) = running.preview_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
                running.preview = Some(ctx.load_texture("opening-preview", image, egui::TextureOptions::LINEAR));
            }
        } else {
            running.preview = None;
        }
    }
    let result = app.file_load.running.as_ref().and_then(|r| match r.rx.try_recv() {
        Ok(job) => Some(job),
        Err(TryRecvError::Empty) => None,
        Err(TryRecvError::Disconnected) => Some(Err("the file operation stopped; the current design is unchanged".into())),
    });
    if let Some(job) = result
        && let Some(running) = app.file_load.running.take()
    {
        match job {
            _ if running.cancelled.load(Ordering::Relaxed) => {}
            Ok(made) if running.matches(&app.session) => {
                if running.request.insert {
                    app.session.accept_computed(made.session);
                    app.big_operation = true;
                } else {
                    crate::documents::adopt(app, made.session);
                    app.home.add_recent(&running.request.path);
                    crate::home::install(&mut app.home, ctx, &running.request.path, made.image);
                }
                app.home.open = false;
                app.fit_view();
                crate::stand_up::offer(app, &running.request.path, &running.before);
                let warnings = made.result.get("warnings").and_then(Value::as_array);
                app.set_status(
                    if let Some(w) = warnings.filter(|w| !w.is_empty()) {
                        format!("imported with {} warning(s): {}", w.len(), w.first().and_then(Value::as_str).unwrap_or(""))
                    } else {
                        format!("{} done", if running.request.insert { "Insert" } else { "Open" })
                    },
                    false,
                );
            }
            Ok(_) => app.set_status("the active design changed while loading; the file operation was discarded", true),
            Err(e) => app.set_status(e, true),
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    start(app);
    if app.file_load.busy() {
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

pub fn show(app: &mut SolveApp, ctx: &egui::Context) {
    let Some(r) = &app.file_load.running else { return };
    let cancelled = r.cancelled.load(Ordering::Relaxed);
    if cancelled && app.file_load.queued.is_empty() {
        return;
    }
    let name = std::path::Path::new(&r.request.path).file_name().map(|p| p.to_string_lossy()).unwrap_or_default();
    let phase = r.phase.load(Ordering::Relaxed);
    let mut cancel = false;
    egui::Modal::new(egui::Id::new("file_loading")).show(ctx, |ui| {
        ui.heading(if cancelled {
            "Waiting for file operation"
        } else if r.request.insert {
            "Inserting geometry"
        } else {
            "Opening design"
        });
        ui.label(name.as_ref());
        if let Some(preview) = r.preview.as_ref().filter(|_| !cancelled) {
            ui.image((preview.id(), egui::vec2(320.0, 200.0)));
            ui.label("Saved preview. Geometry is being checked before editing.");
        }
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(if cancelled {
                "Waiting for the previous operation to stop..."
            } else {
                match phase {
                    1 => "Preparing the view...",
                    2 => "Preparing the thumbnail...",
                    _ => "Reading and building geometry...",
                }
            });
        });
        cancel = ui.button("Cancel").clicked();
    });
    if cancel {
        // Keep the running job until it exits: cancelling must not allow another large
        // import to overlap it in memory. The user can continue editing the current design.
        if let Some(r) = &app.file_load.running {
            r.cancelled.store(true, Ordering::Relaxed);
        }
        app.file_load.queued.clear();
        app.set_status("file operation cancelled", false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_result_and_preview_cannot_overwrite_a_newer_design() {
        for (insert, change_document) in [(false, true), (true, true), (false, false)] {
            let mut app = SolveApp::new(Session::default(), Default::default());
            let base = app.session.doc.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            let (preview_tx, preview_rx) = std::sync::mpsc::channel();
            preview_tx.send(egui::ColorImage::new([2, 2], vec![egui::Color32::RED; 4])).unwrap();
            app.file_load.running = Some(Running {
                request: Request { path: "old.solidcraft".into(), insert },
                base,
                base_revision: app.session.revision,
                before: Vec::new(),
                rx,
                phase: Arc::new(AtomicU8::new(0)),
                cancelled: Arc::new(AtomicBool::new(false)),
                preview_rx: Some(preview_rx),
                preview: None,
            });
            if change_document {
                app.session.execute("file.new", &serde_json::json!({"name":"Newer design"})).unwrap();
            } else {
                // An edit and undo can return to the original Arc; the generation still changed.
                app.session.revision += 1;
            }
            let newer = app.session.doc.clone();
            tick(&mut app, &egui::Context::default());
            assert!(app.file_load.running.as_ref().unwrap().preview.is_none());
            tx.send(Ok(Made { session: Session::default(), result: Value::Null, image: None })).unwrap();
            tick(&mut app, &egui::Context::default());
            assert!(Arc::ptr_eq(&newer, &app.session.doc));
            assert!(app.status.as_ref().unwrap().0.contains("discarded"));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn provisional_picture_renders_without_adopting_cached_meshes() {
        let preview = solidcraft_engine::io::DesignPreview {
            meshes: vec![solidcraft_engine::doc::MeshData {
                name: "Saved triangle".into(),
                positions: vec![0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 0.0, 10.0, 0.0],
                triangles: vec![0, 1, 2],
                color: Some([0.8, 0.2, 0.1]),
            }],
        };
        let picture = saved_picture(preview).unwrap();
        assert_eq!(picture.size, [320, 200]);
        assert!(picture.pixels.iter().any(|p| u16::from(p.r()) > u16::from(p.g()) * 2));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn repeated_large_preview_triangles_are_a_cache_miss_before_rasterization() {
        let preview = solidcraft_engine::io::DesignPreview {
            meshes: vec![solidcraft_engine::doc::MeshData {
                name: "Hostile overdraw".into(),
                positions: vec![0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 0.0, 10.0, 0.0],
                triangles: (0..10_000).flat_map(|_| [0, 1, 2]).collect(),
                color: None,
            }],
        };
        assert!(saved_picture(preview).is_none());
    }
    #[test]
    fn failed_import_keeps_the_current_document() {
        let mut app = SolveApp::new(Session::default(), Default::default());
        let old = app.session.doc.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Err("bad STEP".into())).unwrap();
        app.file_load.running = Some(Running {
            request: Request { path: "bad.step".into(), insert: true },
            base: old.clone(),
            base_revision: app.session.revision,
            before: Vec::new(),
            rx,
            phase: Arc::new(AtomicU8::new(0)),
            cancelled: Arc::new(AtomicBool::new(false)),
            preview_rx: None,
            preview: None,
        });
        tick(&mut app, &egui::Context::default());
        assert!(Arc::ptr_eq(&app.session.doc, &old));
        assert!(!app.file_load.busy());
        assert!(app.status.as_ref().unwrap().0.contains("bad STEP"));
    }
    #[test]
    fn cancelling_discards_a_finished_result_and_keeps_the_worker_slot_until_completion() {
        let mut app = SolveApp::new(Session::default(), Default::default());
        let old = app.session.doc.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(true));
        app.file_load.running = Some(Running {
            request: Request { path: "cancelled.step".into(), insert: true },
            base: old.clone(),
            base_revision: app.session.revision,
            before: Vec::new(),
            rx,
            phase: Arc::new(AtomicU8::new(0)),
            cancelled,
            preview_rx: None,
            preview: None,
        });
        assert!(!app.file_load.busy());
        assert!(app.file_load.running.is_some());
        let mut computed = Session::default();
        computed.execute("solid.box", &serde_json::json!({"length":10,"width":10,"height":10})).unwrap();
        tx.send(Ok(Made { session: computed, result: Value::Null, image: None })).unwrap();
        tick(&mut app, &egui::Context::default());
        assert!(app.file_load.running.is_none());
        assert!(Arc::ptr_eq(&old, &app.session.doc));
    }
}
