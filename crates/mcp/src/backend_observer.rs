//! Private, asynchronous snapshots of the actual unsaved headless model.
use std::sync::Arc;
use std::thread::JoinHandle;

use serde_json::{Value, json};
use solidcraft_engine::{Session, capture, render::StandardView};

use super::Headless;

pub(super) const WIDTH: u32 = 800;
pub(super) const HEIGHT: u32 = 500;
const MAX_PNG: usize = 2 * 1024 * 1024;
const MAX_FEATURES: usize = 2048;
const MAX_INSTANCES: usize = 512;
const MAX_CURVES: usize = 5000;
const MAX_POINTS: usize = 200_000;
const BUDGET: &str = "observer scene exceeds the preview budget";

#[derive(Default)]
pub(super) struct Capture {
    fingerprint: Option<Vec<u8>>,
    revision: u64,
    job: Option<JoinHandle<Result<Value, String>>>,
}

/// Stop before large selection names or view metadata allocate an unbounded
/// fingerprint. This is neither a saved design nor a browser payload.
#[derive(Default)]
struct Fingerprint(Vec<u8>);

impl std::io::Write for Fingerprint {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > 64 * 1024 {
            return Err(std::io::Error::other(BUDGET));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Headless {
    pub(super) fn observer_snapshot(&mut self, params: &Value) -> Result<Value, String> {
        match params.get("action").and_then(Value::as_str) {
            Some("poll") => {
                if self.observer.job.as_ref().is_some_and(|job| !job.is_finished()) {
                    return Ok(json!({"status":"rendering"}));
                }
                match self.observer.job.take() {
                    Some(job) => job.join().map_err(|_| "observer renderer failed".to_string())?,
                    None => Ok(json!({"status":"idle"})),
                }
            }
            Some("start") => self.observer_start(params, capture::installed()),
            _ => Err("observer action must be start or poll".into()),
        }
    }

    fn observer_start(&mut self, params: &Value, renderer: Option<Arc<capture::Renderer>>) -> Result<Value, String> {
        if self.observer.job.is_some() {
            return Ok(json!({"status":"rendering"}));
        }
        let renderer = renderer.ok_or("observer renderer unavailable")?;
        // Only bounded metadata and a scratch copy are prepared on the stdio
        // thread. World placement, tessellation, camera fitting and rendering all
        // happen under the background panic guard, never inside a CAD exchange.
        snapshot_budget(&self.session)?;
        let status = self.workspace_status();
        let s = &self.session;
        let mut fingerprint = Fingerprint::default();
        serde_json::to_writer(
            &mut fingerprint,
            &(
                status.get("revision"),
                s.camera,
                self.view,
                s.section,
                s.active_sketch,
                &s.selection,
                &s.pending_moves,
                s.active_component,
                &s.active_occurrence_path,
                s.analysis,
                &s.continuity,
            ),
        )
        .map_err(|_| BUDGET)?;
        if self.observer.fingerprint.as_ref() != Some(&fingerprint.0) {
            self.observer.revision = self.observer.revision.saturating_add(1);
            self.observer.fingerprint = Some(fingerprint.0);
        }
        let revision = self.observer.revision;
        if params.get("revision").and_then(Value::as_u64) == Some(revision) {
            return Ok(json!({"status":"unchanged","revision":revision}));
        }
        let snapshot = s.observation_snapshot();
        let view = self.view;
        self.observer.job = Some(
            std::thread::Builder::new()
                .name("solidcraft-observer".into())
                .spawn(move || guarded_render(move || render(&snapshot, view, revision, renderer.as_ref())))
                .map_err(|_| "observer renderer could not start")?,
        );
        Ok(json!({"status":"rendering","revision":revision}))
    }
}

fn guarded_render(work: impl FnOnce() -> Result<Value, String>) -> Result<Value, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).map_err(|_| "observer renderer failed".to_string())?
}

fn snapshot_budget(s: &Session) -> Result<(), String> {
    let v = &s.visibility;
    let model = s.model.state();
    if s.doc.features.len() > MAX_FEATURES
        || s.doc.point_clouds.len() > MAX_FEATURES
        || s.model.results.len() > MAX_FEATURES
        || model.bodies.len() > MAX_INSTANCES
        || model.sketches.len() > MAX_FEATURES
        || s.selection.len() > 4096
        || s.pending_moves.len() > 1024
        || s.doc.occurrences.len() > MAX_INSTANCES
        || s.doc.components.len() > MAX_INSTANCES
        || s.active_occurrence_path.as_ref().is_some_and(|path| path.len() > 64)
        || s.continuity.len() > 1024
        || v.hidden_bodies
            .len()
            .saturating_add(v.hidden_occurrences.len())
            .saturating_add(v.hidden_origin.len())
            .saturating_add(v.hidden_sketches.len())
            .saturating_add(v.shown_sketches.len())
            .saturating_add(v.hidden_joints.len())
            .saturating_add(v.hidden_folders.len())
            > 4096
    {
        return Err(BUDGET.into());
    }
    // Point clouds currently have a dedicated GPU pass, outside the portable Scene.
    // Refuse a misleading empty/partial CPU image before cloning or fitting the scan.
    if solidcraft_engine::view::shown_point_clouds(s).next().is_some() {
        return Err("point-cloud preview is not supported by the headless renderer; hide the reference scan to observe CAD geometry".into());
    }
    let continuity = s.continuity.iter().fold(0usize, |n, seam| n.saturating_add(seam.points.len()));
    if continuity > MAX_POINTS || v.hidden_occurrences.iter().any(|path| path.len() > 64) {
        return Err(BUDGET.into());
    }
    // The observer copies only evaluated/view state, never tool clipboards.
    // Bound variable-size metadata before copying it.
    let mut budget = Fingerprint::default();
    serde_json::to_writer(&mut budget, &(v, &s.model.results, &s.model.param_errors, &s.continuity)).map_err(|_| BUDGET)?;
    let names = model
        .bodies
        .iter()
        .fold(0usize, |bytes, body| bytes.saturating_add(body.name.len()).saturating_add(body.source_name.as_ref().map_or(0, String::len)));
    if names > 64 * 1024 {
        return Err(BUDGET.into());
    }
    Ok(())
}

/// Bound nested assembly expansion before world_state creates transformed bodies.
fn placement_budget(s: &Session) -> Result<usize, String> {
    let mut todo = vec![(0u64, 0usize)];
    let mut count = 0usize;
    while let Some((parent, depth)) = todo.pop() {
        if depth > 32 {
            return Err(BUDGET.into());
        }
        for occurrence in s.doc.occurrences.iter().filter(|occurrence| occurrence.parent == parent) {
            count = count.saturating_add(1);
            if count > MAX_INSTANCES {
                return Err(BUDGET.into());
            }
            todo.push((occurrence.component, depth + 1));
        }
    }
    let model = s.model.state();
    if model.bodies.len().saturating_mul(count.max(1)) > MAX_INSTANCES || model.sketches.len() > MAX_FEATURES {
        return Err(BUDGET.into());
    }
    let mut curves = 0usize;
    let mut points = 0usize;
    for solved in &model.sketches {
        let sketch = &solved.sketch;
        if sketch.constraints.len() > 10_000
            || sketch.links.len() > MAX_CURVES
            || sketch.freeform_offsets.len() > MAX_CURVES
            || sketch.offsets.len() > MAX_CURVES
        {
            return Err(BUDGET.into());
        }
        curves = curves.saturating_add(sketch.curves.len());
        points = points.saturating_add(sketch.points.len());
        for wire in &sketch.wires {
            points = points.saturating_add(wire.pts.len()).saturating_add(wire.fit.len());
        }
        if curves > MAX_CURVES || points > MAX_POINTS {
            return Err(BUDGET.into());
        }
    }
    Ok(count.max(1))
}

fn render(s: &Session, view: StandardView, revision: u64, renderer: &capture::Renderer) -> Result<Value, String> {
    let placements = placement_budget(s)?;
    // Inspect definition meshes before occurrence transforms can copy them.
    // Meshing itself remains entirely inside this background job's panic guard.
    mesh_budget(&s.model.state().bodies, placements)?;
    let world = s.world_state();
    if world.bodies.len() > MAX_INSTANCES {
        return Err(BUDGET.into());
    }
    mesh_budget(&world.bodies, 1)?;
    let camera = s.camera.unwrap_or_else(|| {
        let mut camera = solidcraft_engine::view::home_camera(s);
        camera.set_view(view);
        camera
    });
    let mut sampled = 0usize;
    let mut exceeded = false;
    let scene = solidcraft_engine::view::scene_with_section_and_sketch_sampler(s, &camera, s.section, |sketch, index, plane| {
        if exceeded {
            return Vec::new();
        }
        let parts = solidcraft_engine::view::world_curve_parts(sketch, index, plane);
        for part in &parts {
            sampled = sampled.saturating_add(part.len());
        }
        if sampled > MAX_POINTS {
            exceeded = true;
            Vec::new()
        } else {
            parts
        }
    });
    let points = scene.lines.iter().chain(&scene.grid).fold(0usize, |n, line| n.saturating_add(line.points.len()));
    if exceeded || points > MAX_POINTS {
        return Err(BUDGET.into());
    }
    // CPU warnings are metadata, not a repeated container-log message per frame.
    let picture = renderer(&scene, &camera, WIDTH, HEIGHT)?;
    if picture.rgba.len() != (WIDTH * HEIGHT * 4) as usize {
        return Err("observer renderer returned invalid pixels".into());
    }
    let bytes = solidcraft_engine::render::png(WIDTH as usize, HEIGHT as usize, picture.rgba).ok_or("observer PNG encoding failed")?;
    if bytes.len() > MAX_PNG {
        return Err("observer PNG exceeds the preview budget".into());
    }
    Ok(json!({"status":"ready","revision":revision,"width":WIDTH,"height":HEIGHT,
        "renderer":picture.renderer,"warning":capture::warning(picture.renderer),"png":crate::tools::base64(&bytes)}))
}

fn mesh_budget(bodies: &[solidcraft_engine::doc::ModelBody], placements: usize) -> Result<(), String> {
    let mut triangles = 0usize;
    let mut vertices = 0usize;
    let mut lines = 0usize;
    let mut faces = 0usize;
    for body in bodies {
        faces = faces.saturating_add(body.body.face_count());
        if faces > 4096 {
            return Err(BUDGET.into());
        }
        let mesh = body.mesh();
        triangles = triangles.saturating_add(mesh.triangles.len());
        vertices = vertices.saturating_add(mesh.positions.len());
        for edge in &mesh.edges {
            lines = lines.saturating_add(edge.len());
        }
        if triangles.saturating_mul(placements) > 100_000
            || vertices.saturating_mul(placements) > MAX_POINTS
            || lines.saturating_mul(placements) > MAX_POINTS
            || mesh.positions.iter().any(|point| !point.is_finite())
        {
            return Err(BUDGET.into());
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "backend_observer_tests.rs"]
mod tests;
