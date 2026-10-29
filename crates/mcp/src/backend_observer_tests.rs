use super::*;
use crate::backend::Backend as _;
use solidcraft_engine::geom::Vec3;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

fn renderer() -> Arc<capture::Renderer> {
    Arc::new(|scene, _, width, height| {
        let color = if scene.meshes.is_empty() { 0 } else { 200 };
        Ok(capture::Picture { rgba: vec![color; (width * height * 4) as usize], renderer: "cpu" })
    })
}

fn completed(backend: &mut Headless) -> Result<Value, String> {
    let limit = Instant::now() + Duration::from_secs(5);
    loop {
        let result = backend.observer_snapshot(&json!({"action":"poll"}))?;
        if result["status"] != "rendering" {
            return Ok(result);
        }
        assert!(Instant::now() < limit, "observer did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn unsaved_preview_preserves_document_camera_history_and_dirty_state() {
    let mut backend = Headless::default();
    backend.call("engine.execute", json!({"command":"solid.box","params":{"length":10,"width":20,"height":30}})).unwrap();
    backend.session.section = Some((Vec3::ZERO, Vec3::Z));
    backend.session.analysis = Some(solidcraft_engine::SurfaceAnalysis::Zebra { stripes: 8.0 });
    backend.session.path = Some("not-written-by-observation.solidcraft".into());
    let before = backend.workspace_status();
    let doc = Arc::clone(&backend.session.doc);
    let camera = serde_json::to_value(backend.session.camera).unwrap();
    let undo = backend.session.undo.len();
    let redo = backend.session.redo.len();
    let (send, receive) = std::sync::mpsc::channel();
    let capture: Arc<capture::Renderer> = Arc::new(move |scene, _, width, height| {
        send.send((scene.meshes.len(), scene.clip.is_some(), scene.analysis.is_some())).unwrap();
        Ok(capture::Picture { rgba: vec![200; (width * height * 4) as usize], renderer: "cpu" })
    });
    assert_eq!(backend.observer_start(&json!({}), Some(capture)).unwrap()["status"], "rendering");
    let ready = completed(&mut backend).unwrap();
    assert_eq!(ready["status"], "ready");
    assert_eq!(ready["width"], WIDTH);
    assert!(ready["png"].as_str().unwrap().starts_with("iVBOR"));
    assert_eq!(receive.recv().unwrap(), (1, true, true));
    assert_eq!(backend.workspace_status(), before);
    assert!(Arc::ptr_eq(&doc, &backend.session.doc));
    assert_eq!(serde_json::to_value(backend.session.camera).unwrap(), camera);
    assert_eq!((backend.session.undo.len(), backend.session.redo.len()), (undo, redo));
}

#[test]
fn background_render_does_not_hold_the_stdio_worker_or_queue_extra_jobs() {
    let mut backend = Headless::default();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let render_gate = Arc::clone(&gate);
    let capture: Arc<capture::Renderer> = Arc::new(move |_, _, width, height| {
        let (lock, signal) = &*render_gate;
        let done = lock.lock().unwrap();
        let (done, _) = signal.wait_timeout_while(done, Duration::from_secs(5), |done| !*done).unwrap();
        assert!(*done, "test failed to release renderer");
        Ok(capture::Picture { rgba: vec![0; (width * height * 4) as usize], renderer: "cpu" })
    });
    backend.observer_start(&json!({}), Some(capture)).unwrap();
    assert_eq!(backend.observer_start(&json!({}), Some(renderer())).unwrap()["status"], "rendering");
    assert_eq!(backend.observer_snapshot(&json!({"action":"poll"})).unwrap()["status"], "rendering");
    // A new unsaved body can be created while the renderer retains its old scene.
    backend.call("engine.execute", json!({"command":"solid.box","params":{"length":1,"width":2,"height":3}})).unwrap();
    let (lock, signal) = &*gate;
    *lock.lock().unwrap() = true;
    signal.notify_all();
    assert_eq!(completed(&mut backend).unwrap()["status"], "ready");
    assert!(backend.session.is_dirty());
    assert_eq!(backend.session.doc.features.len(), 1);
}

#[test]
fn visual_revision_includes_camera_and_metadata_without_repeating_unchanged_frames() {
    let mut backend = Headless::default();
    backend.observer_start(&json!({}), Some(renderer())).unwrap();
    let first = completed(&mut backend).unwrap()["revision"].as_u64().unwrap();
    assert_eq!(backend.observer_start(&json!({"revision":first}), Some(renderer())).unwrap()["status"], "unchanged");
    backend.call("ui.view", json!({"view":"top"})).unwrap();
    backend.observer_start(&json!({"revision":first}), Some(renderer())).unwrap();
    assert!(completed(&mut backend).unwrap()["revision"].as_u64().unwrap() > first);
}

#[test]
fn renderer_failure_is_confined_to_preview_and_cad_still_runs() {
    let mut backend = Headless::default();
    let capture: Arc<capture::Renderer> = Arc::new(|_, _, _, _| panic!("test observer failure"));
    backend.observer_start(&json!({}), Some(capture)).unwrap();
    assert!(completed(&mut backend).unwrap_err().contains("observer renderer failed"));
    backend.call("engine.execute", json!({"command":"solid.box","params":{"length":1,"width":2,"height":3}})).unwrap();
    backend.observer_start(&json!({}), Some(renderer())).unwrap();
    assert_eq!(completed(&mut backend).unwrap()["status"], "ready");
}

#[test]
fn hostile_view_metadata_and_assembly_expansion_are_bounded_before_rendering() {
    let mut backend = Headless::default();
    backend.session.pending_moves = (0..1025).map(|id| (id, solidcraft_engine::doc::IDENTITY)).collect();
    assert!(backend.observer_start(&json!({}), Some(renderer())).is_err());
    assert!(backend.observer.job.is_none());
    backend.session.pending_moves.clear();
    backend.session.selection.push(solidcraft_engine::Sel::Body { name: "x".repeat(128 * 1024) });
    assert!(backend.observer_start(&json!({}), Some(renderer())).is_err());
    assert!(backend.observer.job.is_none());
    backend.session.selection.clear();
    let doc = Arc::make_mut(&mut backend.session.doc);
    // One cycle is enough to prove expansion cannot allocate unbounded copies.
    doc.occurrences.push(solidcraft_engine::doc::Occurrence {
        id: 1,
        component: 1,
        parent: 0,
        name: "root".into(),
        transform: solidcraft_engine::doc::IDENTITY,
        grounded: false,
    });
    doc.occurrences.push(solidcraft_engine::doc::Occurrence {
        id: 2,
        component: 1,
        parent: 1,
        name: "cycle".into(),
        transform: solidcraft_engine::doc::IDENTITY,
        grounded: false,
    });
    assert!(placement_budget(&backend.session).is_err());
}

#[test]
fn point_cloud_preview_is_explicitly_unavailable_before_rendering_and_hidden_scans_do_not_block_cad() {
    use solidcraft_engine::doc::point_cloud::{ColorBy, Colors, PointCloud, Points};
    let mut backend = Headless::default();
    Arc::make_mut(&mut backend.session.doc).point_clouds.push(PointCloud {
        id: 1,
        name: "Public scan".into(),
        file: "scan.xyz".into(),
        origin: Vec3::ZERO,
        points: Points::new(vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0].into()),
        colors: Colors::default(),
        source_points: 2,
        units: "mm".into(),
        transform: solidcraft_engine::doc::IDENTITY,
        point_size: 2.0,
        color_by: ColorBy::File,
        color: None,
        height_range: None,
        visible: true,
    });
    let before = backend.workspace_status();
    let error = backend.observer_start(&json!({}), Some(renderer())).unwrap_err();
    assert!(error.contains("point-cloud preview is not supported"));
    assert!(backend.observer.job.is_none());
    assert_eq!(backend.workspace_status(), before);
    Arc::make_mut(&mut backend.session.doc).point_clouds[0].visible = false;
    backend.call("engine.execute", json!({"command":"solid.box","params":{"length":1,"width":2,"height":3}})).unwrap();
    backend.observer_start(&json!({}), Some(renderer())).unwrap();
    assert_eq!(completed(&mut backend).unwrap()["status"], "ready");
}
