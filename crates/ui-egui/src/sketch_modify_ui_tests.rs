//! The Sketch Scale tool window: the toolbar button used to reach no tool and no dialog, so a
//! press ran `sketch.scale` with no parameters and the engine refused it.

use serde_json::json;
use solidcraft_engine::Sel;

use crate::dialogs::Kind;
use crate::scenario::Harness;
use crate::selection::SelInput;
use crate::sketch_modify_ui::{Mod, commands};

/// A sketch on XY holding one 20 by 10 rectangle at the origin.
fn rectangle() -> Harness {
    let mut h = Harness::new();
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    h.app.run("sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [20, 10]})).unwrap();
    h.settle();
    h
}

fn curve_ids(h: &Harness) -> Vec<String> {
    let sid = h.app.session.active_sketch.unwrap();
    let st = h.app.session.model.state();
    st.sketch(sid).unwrap().sketch.curves.iter().map(|c| c.id.clone()).collect()
}

#[test]
fn the_toolbar_button_opens_a_window_instead_of_reporting_an_engine_error() {
    let mut h = rectangle();
    let before = curve_ids(&h).len();
    h.step(&json!({"start": "sketch.scale"})).unwrap();
    let d = h.app.dialog.as_ref().expect("sketch.scale opens a tool window");
    assert!(matches!(d.kind, Kind::SketchModify(Mod::Scale { .. })), "{:?}", d.kind);
    assert_eq!(d.inputs.iter().map(|i| i.label).collect::<Vec<_>>(), vec!["Entities", "Base point"]);
    // Opening the window commits nothing, and the old dead button's error is gone.
    assert_eq!(curve_ids(&h).len(), before);
    assert!(!h.app.status.as_ref().is_some_and(|(_, _, error)| *error), "{:?}", h.app.status);
}

#[test]
fn the_typed_factor_reaches_the_command_unchanged() {
    let h = rectangle();
    let ids = curve_ids(&h);
    let mut inputs = vec![SelInput::new("Entities", crate::selection::CURVES, true), SelInput::new("Base point", crate::selection::POINTS, false)];
    inputs[0].items = ids.iter().map(|id| Sel::SketchCurve { id: id.clone() }).collect();
    let k = Mod::Scale { factor: "2.5".into() };
    let out = commands(&h.app, &k, &inputs, &serde_json::Map::new()).unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, "sketch.scale");
    assert_eq!(out[0].1["factor"], json!(2.5));
    assert_eq!(out[0].1["entities"], json!(ids));
    // No base point picked: the command's own default, the sketch origin, stands.
    assert!(out[0].1.get("base").is_none(), "{}", out[0].1);
}

#[test]
fn an_expression_in_the_factor_field_is_evaluated() {
    let mut h = rectangle();
    h.app.run("parameters.add", json!({"name": "growth", "expression": "3", "unit": ""})).unwrap();
    let ids = curve_ids(&h);
    let mut inputs = vec![SelInput::new("Entities", crate::selection::CURVES, true)];
    inputs[0].items = ids.iter().map(|id| Sel::SketchCurve { id: id.clone() }).collect();
    let out = commands(&h.app, &Mod::Scale { factor: "growth * 2".into() }, &inputs, &serde_json::Map::new()).unwrap();
    assert_eq!(out[0].1["factor"], json!(6.0));
}

#[test]
fn a_picked_base_point_loses_the_sketch_prefix_that_picking_adds() {
    let h = rectangle();
    let sid = h.app.session.active_sketch.unwrap();
    let ids = curve_ids(&h);
    let mut inputs = vec![SelInput::new("Entities", crate::selection::CURVES, true), SelInput::new("Base point", crate::selection::POINTS, false)];
    inputs[0].items = vec![Sel::SketchCurve { id: ids[0].clone() }];
    let point = {
        let st = h.app.session.model.state();
        st.sketch(sid).unwrap().sketch.points.first().unwrap().id.clone()
    };
    inputs[1].items = vec![Sel::SketchPoint { id: format!("{sid}:{point}") }];
    let out = commands(&h.app, &Mod::Scale { factor: "2".into() }, &inputs, &serde_json::Map::new()).unwrap();
    assert_eq!(out[0].1["base"], json!(point));
}

#[test]
fn an_empty_selection_or_a_bad_factor_is_an_error_the_window_shows() {
    let h = rectangle();
    let ids = curve_ids(&h);
    let empty = vec![SelInput::new("Entities", crate::selection::CURVES, true)];
    assert!(commands(&h.app, &Mod::Scale { factor: "2".into() }, &empty, &serde_json::Map::new()).is_err());
    let mut filled = empty.clone();
    filled[0].items = vec![Sel::SketchCurve { id: ids[0].clone() }];
    for bad in ["0", "-2", "", "nonsense"] {
        let r = commands(&h.app, &Mod::Scale { factor: bad.into() }, &filled, &serde_json::Map::new());
        assert!(r.is_err(), "factor `{bad}` should not reach the command: {r:?}");
    }
}

#[test]
fn the_window_scales_the_picked_geometry_and_leaves_the_rest_alone() {
    let mut h = rectangle();
    h.app.run("sketch.circle.center", json!({"center": [60, 0], "radius": 5})).unwrap();
    h.settle();
    let sid = h.app.session.active_sketch.unwrap();
    let ids = curve_ids(&h);
    let rect: Vec<String> = ids.iter().take(4).cloned().collect();
    h.step(&json!({"start": "sketch.scale"})).unwrap();
    {
        let d = h.app.dialog.as_mut().unwrap();
        d.inputs[0].items = rect.iter().map(|id| Sel::SketchCurve { id: id.clone() }).collect();
        d.kind = Kind::SketchModify(Mod::Scale { factor: "2".into() });
    }
    // A frame with the picks in place: the canvas value box takes focus on the frame after a
    // pick, and Enter only applies once it has had it.
    h.frames(2);
    h.step(&json!({"key": "Enter"})).unwrap();
    h.settle();
    assert!(h.app.dialog.is_none(), "Enter applies and closes the window");
    let st = h.app.session.model.state();
    let sk = &st.sketch(sid).unwrap().sketch;
    let xs: Vec<f64> = sk.points.iter().map(|p| p.pos.x).collect();
    let ys: Vec<f64> = sk.points.iter().map(|p| p.pos.y).collect();
    // The rectangle doubled about the sketch origin; the circle at x = 60 did not move.
    assert!(xs.iter().any(|x| (x - 40.0).abs() < 1e-6), "{xs:?}");
    assert!(ys.iter().any(|y| (y - 20.0).abs() < 1e-6), "{ys:?}");
    assert!(xs.iter().any(|x| (x - 60.0).abs() < 1e-6), "{xs:?}");
    assert_eq!(sk.curves.len(), 5);
}
