//! Render capability discovery. No rendering, host probing or document mutation occurs here.

use serde_json::{Value, json};

use super::CommandSpec;
use crate::params::bad;
use crate::{Result, Session};

pub static COMMANDS: &[CommandSpec] = &[CommandSpec::new("render.backends", "Render Backends", backends)
    .noundo()
    .doc_only()
    .params("No parameters. Returns {schema_version: 1, default_backend: builtin, backends: [{id, label, availability, executors: [{id, availability, profiles, limits, scratch}]}]}. Metadata only; does not start rendering or probe a GPU. See docs/render-backends.md")];

fn backends(_: &mut Session, params: &Value) -> Result<Value> {
    if !params.as_object().is_some_and(|p| p.is_empty()) {
        return Err(bad("render.backends", "expected an empty parameter object"));
    }
    Ok(json!({"schema_version": 1, "default_backend": "builtin", "backends": [crate::render::builtin_backend()]}))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn assert_unchanged(s: &mut Session, params: Value) -> Result<Value> {
        let doc = s.doc.clone();
        let before = (s.is_dirty(), s.revision, s.evaluations, s.model.is_stale());
        let camera = s.camera;
        let undo = s.undo.clone();
        let redo = s.redo.clone();
        let result = s.execute("render.backends", &params);
        assert!(Arc::ptr_eq(&doc, &s.doc));
        assert_eq!(before, (s.is_dirty(), s.revision, s.evaluations, s.model.is_stale()));
        assert_eq!(camera, s.camera);
        for (before, after) in [(&undo, &s.undo), (&redo, &s.redo)] {
            assert_eq!(before.len(), after.len());
            for (a, b) in before.iter().zip(after.iter()) {
                assert!(Arc::ptr_eq(&a.doc, &b.doc));
                assert_eq!((&a.label, a.active_sketch, &a.pending_moves), (&b.label, b.active_sketch, &b.pending_moves));
            }
        }
        result
    }

    #[test]
    fn discovery_preserves_clean_dirty_and_undo_redo_state() {
        let mut s = Session::default();
        let r = assert_unchanged(&mut s, Value::Null).unwrap();
        assert_eq!(r["schema_version"], 1);
        assert_eq!(r["default_backend"], "builtin");
        assert_eq!(r["backends"].as_array().unwrap().len(), 1);
        let descriptor: crate::render::BackendDescriptor = serde_json::from_value(r["backends"][0].clone()).unwrap();
        assert_eq!(descriptor, crate::render::builtin_backend());
        s.execute("solid.box", &json!({"length": 10, "width": 20, "height": 5})).unwrap();
        s.execute("view.apply", &json!({"view": "top"})).unwrap();
        assert!(s.is_dirty());
        assert_unchanged(&mut s, json!({})).unwrap();
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(!s.redo.is_empty());
        assert_unchanged(&mut s, json!({})).unwrap();
        for p in [json!([]), json!(3), json!({"backend": "missing"}), json!({"width": "bad"})] {
            assert!(assert_unchanged(&mut s, p).is_err());
        }
    }

    #[test]
    fn discovery_does_not_settle_a_live_drag_or_rebuild_geometry() {
        let mut s = Session::default();
        for (id, p) in [
            ("sketch.create", json!({"plane": "XY"})),
            ("sketch.rectangle.two_point", json!({"p0": [5, 5], "p1": [25, 20]})),
            ("sketch.dimension", json!({"entities": ["l1"], "value": 20})),
            ("sketch.dimension", json!({"entities": ["l2"], "value": 15})),
            ("sketch.finish", json!({})),
            ("solid.extrude", json!({"distance": 5})),
            ("sketch.edit", json!({"sketch": "Sketch1"})),
            ("sketch.drag", json!({"entity": "l1.start", "from": [5, 5], "to": [4, 5], "live": true})),
        ] {
            s.execute(id, &p).unwrap();
        }
        assert!(s.model.is_stale());
        s.evaluations = 0;
        let model = s.model.state();
        assert_unchanged(&mut s, json!({})).unwrap();
        assert_unchanged(&mut s, json!({"unexpected": true})).unwrap_err();
        assert!(Arc::ptr_eq(&model, &s.model.state()));
        assert_eq!(s.evaluations, 0);
    }
}
