//! Large STEP imports (#883): the file's text is held once however many undo states share
//! it, and an import cancelled from another thread stops and keeps nothing.

use std::sync::Arc;

use serde_json::json;
use solidcraft_doc::{FeatureKind, progress::Progress};

use crate::*;

fn step_file(name: &str, count: usize) -> String {
    let bodies: Vec<_> = (0..count)
        .map(|i| {
            let at = solidcraft_geom::Vec3::new((i % 32) as f64 * 12.0, (i / 32) as f64 * 12.0, 0.0);
            solidcraft_kernel::box_solid(at, at + solidcraft_geom::Vec3::new(10.0, 10.0, 10.0)).unwrap()
        })
        .collect();
    let text = solidcraft_kernel::step_export(&bodies.iter().collect::<Vec<_>>(), name).unwrap();
    let path = std::env::temp_dir().join(format!("solidcraft-883-{}-{name}.step", std::process::id()));
    std::fs::write(&path, text).unwrap();
    path.to_string_lossy().to_string()
}

fn step_text(s: &Session) -> Arc<str> {
    s.doc.features.iter().find_map(|f| if let FeatureKind::Import { step, .. } = &f.kind { Some(step.clone()) } else { None }).unwrap()
}

/// Memory budget: 30 edits after a large import keep one copy of the STEP text, not 31.
#[test]
fn edits_after_a_large_import_share_one_copy_of_its_text() {
    let path = step_file("shared", 300);
    let mut s = Session::default();
    s.execute("file.insert_step", &json!({"path": path})).unwrap();
    let text = step_text(&s);
    assert!(text.len() > 500_000, "{} bytes", text.len());
    for i in 0..30 {
        s.execute("parameters.add", &json!({"name": format!("p{i}"), "expression": format!("{i} mm")})).unwrap();
    }
    assert!(s.undo.len() >= 30);
    for snap in &s.undo {
        for f in &snap.doc.features {
            if let FeatureKind::Import { step, .. } = &f.kind {
                assert!(Arc::ptr_eq(step, &text), "an undo state holds its own copy of the STEP text");
            }
        }
    }
    assert!(Arc::ptr_eq(&step_text(&s), &text));
    // Every Document clone (preview, scratch session) shares it too.
    let copy = (*s.doc).clone();
    assert!(copy.features.iter().any(|f| matches!(&f.kind, FeatureKind::Import { step, .. } if Arc::ptr_eq(step, &text))));
    let _ = std::fs::remove_file(path);
}

#[test]
fn cancelled_import_stops_and_leaves_the_design_alone() {
    let path = step_file("cancelled", 300);
    let mut s = Session::default();
    s.execute("solid.box", &json!({"length": 10, "width": 10, "height": 10})).unwrap();
    let before = s.model.state().bodies.len();
    // Cancel from the window thread while the worker reads the file.
    let progress = Arc::new(Progress::default());
    let canceller = {
        let progress = progress.clone();
        std::thread::spawn(move || {
            while progress.work().is_none() {
                std::thread::yield_now();
            }
            progress.cancel();
        })
    };
    let r = solidcraft_doc::progress::watch(progress.clone(), || s.execute("file.insert_step", &json!({"path": path})));
    canceller.join().unwrap();
    let err = r.unwrap_err().to_string();
    assert!(err.contains("cancelled"), "{err}");
    assert_eq!(s.model.state().bodies.len(), before);
    assert!(!s.doc.features.iter().any(|f| matches!(f.kind, FeatureKind::Import { .. })));
    let (_, done, total) = progress.work().unwrap();
    assert!(done < total, "stopped part way ({done} of {total})");
    let _ = std::fs::remove_file(path);
}
