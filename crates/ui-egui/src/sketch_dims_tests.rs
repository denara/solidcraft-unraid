//! Real keyboard input must stay inside the dimensions of the shape being drawn.

use serde_json::json;

use crate::scenario::Harness;

fn drawing(command: &str) -> Harness {
    let mut h = Harness::new();
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    h.step(&json!({"start": command})).unwrap();
    h.step(&json!({"click": {"world": [0, 0, 0]}})).unwrap();
    h.step(&json!({"move": {"world": [20, 10, 0]}})).unwrap();
    assert_focus(&h, 0);
    h
}

fn assert_focus(h: &Harness, index: usize) {
    let expected = egui::Id::new(("sc_dim", index));
    assert_eq!(h.ctx.memory(|memory| memory.focused()), Some(expected), "focus must stay in inline dimension {index}");
}

fn tab(h: &mut Harness, backwards: bool) {
    h.step(&json!({"key": "Tab", "shift": backwards})).unwrap();
}

fn type_value(h: &mut Harness, text: &str) {
    h.step(&json!({"key": "A", "cmd": true})).unwrap();
    h.step(&json!({"text": text})).unwrap();
}

#[test]
fn tabs_cycle_rectangle_dimensions_indefinitely_and_enter_commits_values() {
    for command in ["sketch.rectangle.two_point", "sketch.rectangle.center"] {
        let mut h = drawing(command);
        type_value(&mut h, "30");
        tab(&mut h, false);
        assert_focus(&h, 1);
        type_value(&mut h, "12");
        for i in 0..8 {
            tab(&mut h, false);
            assert_focus(&h, i % 2);
        }
        for i in 0..8 {
            tab(&mut h, true);
            assert_focus(&h, i % 2);
        }
        // Revisited locked dimensions are selected, so another value replaces them directly.
        h.step(&json!({"text": "14"})).unwrap();
        tab(&mut h, true);
        assert_focus(&h, 0);
        h.step(&json!({"text": "32"})).unwrap();
        let dims = &h.app.tool.as_ref().unwrap().dims;
        assert_eq!(dims.iter().map(|d| d.value.as_str()).collect::<Vec<_>>(), ["32", "14"]);
        assert!(dims.iter().all(|d| d.locked));
        h.step(&json!({"key": "Enter"})).unwrap();
        let state = h.app.session.world_state();
        let sketch = state.sketch(h.app.session.active_sketch.unwrap()).unwrap();
        assert_eq!(sketch.sketch.curves.iter().filter(|curve| !curve.construction).count(), 4);
        assert_eq!(sketch.profiles.len(), 1);
        assert!((sketch.profiles[0].area - 448.0).abs() < 1e-6, "{command}: {}", sketch.profiles[0].area);
    }
}

#[test]
fn line_dimensions_wrap_both_directions_and_enter_uses_length_and_angle() {
    let mut h = drawing("sketch.line");
    type_value(&mut h, "10");
    tab(&mut h, false);
    type_value(&mut h, "45 deg");
    for i in 0..6 {
        tab(&mut h, false);
        assert_focus(&h, i % 2);
    }
    tab(&mut h, true);
    assert_focus(&h, 0);
    h.step(&json!({"key": "Enter"})).unwrap();
    let state = h.app.session.world_state();
    let sketch = state.sketch(h.app.session.active_sketch.unwrap()).unwrap();
    assert_eq!(sketch.sketch.curves.len(), 1);
    let points: Vec<_> = sketch.sketch.points.iter().map(|point| point.pos).collect();
    assert!(points.iter().any(|point| (point.x - 10.0 / 2f64.sqrt()).abs() < 1e-6 && (point.y - 10.0 / 2f64.sqrt()).abs() < 1e-6));
}

#[test]
fn single_dimension_keeps_focus_for_tab_and_shift_tab() {
    let mut h = drawing("sketch.circle.center");
    type_value(&mut h, "8");
    for backwards in [false, false, true, true, false] {
        tab(&mut h, backwards);
        assert_focus(&h, 0);
    }
    assert_eq!(h.app.tool.as_ref().unwrap().dims[0].value, "8");
    h.step(&json!({"key": "Enter"})).unwrap();
    let state = h.app.session.world_state();
    let sketch = state.sketch(h.app.session.active_sketch.unwrap()).unwrap();
    assert_eq!(sketch.profiles.len(), 1);
    assert!((sketch.profiles[0].area - std::f64::consts::PI * 16.0).abs() < 1e-6);
}

#[test]
fn forward_tab_completes_expression_then_cycles_and_shift_tab_moves_back() {
    let mut h = drawing("sketch.rectangle.two_point");
    h.app.run("parameters.add", json!({"name": "width", "expression": "24 mm"})).unwrap();
    h.frames(2);
    type_value(&mut h, "wid");
    tab(&mut h, false);
    assert_focus(&h, 0);
    assert_eq!(h.app.tool.as_ref().unwrap().dims[0].value, "width");
    tab(&mut h, false);
    assert_focus(&h, 1);
    type_value(&mut h, "5");
    tab(&mut h, true);
    assert_focus(&h, 0);
    type_value(&mut h, "wid");
    tab(&mut h, true);
    assert_focus(&h, 1);
    assert_eq!(h.app.tool.as_ref().unwrap().dims[0].value, "wid", "Shift+Tab must navigate rather than accepting a completion");
    tab(&mut h, true);
    assert_focus(&h, 0);
    tab(&mut h, false);
    assert_focus(&h, 0);
    assert_eq!(h.app.tool.as_ref().unwrap().dims[0].value, "width");
    h.step(&json!({"key": "Enter"})).unwrap();
    let state = h.app.session.world_state();
    let sketch = state.sketch(h.app.session.active_sketch.unwrap()).unwrap();
    assert_eq!(sketch.profiles.len(), 1);
    assert!((sketch.profiles[0].area - 120.0).abs() < 1e-6);
}

#[test]
fn inch_live_lengths_use_design_units_and_typed_four_digits_commit_in_inches() {
    let mut h = Harness::new();
    h.app.run("document.units", json!({"units":"in"})).unwrap();
    h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
    h.step(&json!({"start":"sketch.rectangle.two_point"})).unwrap();
    h.step(&json!({"click":{"world":[0,0,0]}})).unwrap();
    h.step(&json!({"move":{"world":[25.4,12.7,0]}})).unwrap();
    let dims = &h.app.tool.as_ref().unwrap().dims;
    // Pointer snapping rounds in model space; displayed values must still be inches.
    assert!((dims[0].value.parse::<f64>().unwrap() - 1.0).abs() < 0.01);
    assert!((dims[1].value.parse::<f64>().unwrap() - 0.5).abs() < 0.01);
    type_value(&mut h, "1.2345");
    tab(&mut h, false);
    type_value(&mut h, "0.500");
    h.step(&json!({"key":"Enter"})).unwrap();
    let st = h.app.session.world_state();
    let sk = &st.sketch(h.app.session.active_sketch.unwrap()).unwrap().sketch;
    assert!(sk.constraints.iter().any(|c| c.kind.value().is_some_and(|v| (v - 31.3563).abs() < 1e-6)));
}

/// A conic's fullness is a typed value as well as a drag. The Rho box takes the keyboard once
/// the chord is down, shows the parabola the command would otherwise default to, and the value
/// typed into it reaches the command when the apex is placed.
#[test]
fn a_typed_rho_shapes_the_conic_whose_apex_is_placed_next() {
    let mut h = Harness::new();
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    h.step(&json!({"start": "sketch.conic"})).unwrap();
    h.step(&json!({"click": {"world": [0, 0, 0]}})).unwrap();
    h.step(&json!({"move": {"world": [40, 0, 0]}})).unwrap();
    // One point down is a chord with one end: there is no shape to put a number on yet.
    assert!(h.app.tool.as_ref().unwrap().dims.is_empty());
    h.step(&json!({"click": {"world": [40, 0, 0]}})).unwrap();
    h.step(&json!({"move": {"world": [20, 20, 0]}})).unwrap();
    assert_focus(&h, 0);
    let dims = &h.app.tool.as_ref().unwrap().dims;
    assert_eq!(dims.iter().map(|d| d.label).collect::<Vec<_>>(), ["Rho"]);
    assert_eq!(dims[0].value, "0.5", "the live box reads as a ratio, not as a rounded count");
    assert!(!dims[0].locked);

    type_value(&mut h, "0.8");
    assert!(h.app.tool.as_ref().unwrap().dims[0].locked);
    h.step(&json!({"key": "Enter"})).unwrap();

    let state = h.app.session.world_state();
    let sketch = &state.sketch(h.app.session.active_sketch.unwrap()).unwrap().sketch;
    let (apex, rho) = sketch
        .curves
        .iter()
        .find_map(|curve| match curve.kind {
            solidcraft_engine::sketch::CurveKind::Conic { apex, rho, .. } => Some((apex, rho)),
            _ => None,
        })
        .expect("the typed value placed a conic");
    assert!((rho - 0.8).abs() < 1e-9, "typed rho reached the command: {rho}");
    // The apex lands where the cursor was, within the pixel the cursor was converted through.
    assert!(sketch.point(apex).unwrap().dist(solidcraft_engine::geom::Vec2::new(20.0, 20.0)) < 0.5, "{:?}", sketch.point(apex));
}

/// A rho the command would refuse stays in its box to be corrected, and places nothing.
#[test]
fn a_rho_outside_zero_to_one_is_refused_before_it_reaches_the_command() {
    let mut h = Harness::new();
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    h.step(&json!({"start": "sketch.conic"})).unwrap();
    h.step(&json!({"click": {"world": [0, 0, 0]}})).unwrap();
    h.step(&json!({"click": {"world": [40, 0, 0]}})).unwrap();
    h.step(&json!({"move": {"world": [20, 20, 0]}})).unwrap();
    type_value(&mut h, "5");
    h.step(&json!({"key": "Enter"})).unwrap();

    let state = h.app.session.world_state();
    let sketch = &state.sketch(h.app.session.active_sketch.unwrap()).unwrap().sketch;
    assert!(sketch.curves.is_empty(), "a refused rho must place nothing");
    let tool = h.app.tool.as_ref().expect("the tool stays active for a correction");
    assert_eq!(tool.pts.len(), 2, "the chord stays down");
    assert_eq!(tool.dims.iter().map(|d| d.value.as_str()).collect::<Vec<_>>(), ["5"], "the refused value stays in its box");
    assert!(h.app.status.as_ref().is_some_and(|(_, _, error)| *error), "the refusal is reported");
}

/// Rounded Rectangle (#891): typed width, height and corner radius make one profile whose
/// size is the typed overall size; the radius stays its own parameter.
#[test]
fn rounded_rectangle_takes_typed_width_height_and_radius() {
    let mut h = drawing("sketch.rectangle.rounded");
    assert_eq!(h.app.tool.as_ref().unwrap().dims.iter().map(|d| d.label).collect::<Vec<_>>(), ["Width", "Height", "Radius"]);
    type_value(&mut h, "30");
    tab(&mut h, false);
    type_value(&mut h, "16");
    tab(&mut h, false);
    assert_focus(&h, 2);
    type_value(&mut h, "4");
    let undo = h.app.session.undo.len();
    h.step(&json!({"key": "Enter"})).unwrap();
    let state = h.app.session.world_state();
    let sketch = state.sketch(h.app.session.active_sketch.unwrap()).unwrap();
    let kinds: Vec<_> = sketch.sketch.curves.iter().map(|c| matches!(c.kind, solidcraft_engine::sketch::CurveKind::Arc { .. })).collect();
    assert_eq!(kinds.iter().filter(|arc| **arc).count(), 4, "four corner arcs");
    assert_eq!(kinds.len(), 8);
    assert_eq!(sketch.profiles.len(), 1);
    let want = 30.0 * 16.0 - (4.0 - std::f64::consts::PI) * 16.0;
    assert!((sketch.profiles[0].area - want).abs() < 1e-6, "area {} want {want}", sketch.profiles[0].area);
    assert_eq!(h.app.session.undo.len(), undo + 1, "one undo step");
}

/// Without a typed radius the corners get a fifth of the shorter side.
#[test]
fn rounded_rectangle_clicks_use_a_default_radius() {
    let mut h = Harness::new();
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    h.step(&json!({"start": "sketch.rectangle.rounded"})).unwrap();
    h.step(&json!({"click": {"sketch": [0, 0]}})).unwrap();
    h.step(&json!({"click": {"sketch": [40, 20]}})).unwrap();
    let state = h.app.session.world_state();
    let sketch = state.sketch(h.app.session.active_sketch.unwrap()).unwrap();
    assert_eq!(sketch.profiles.len(), 1, "{:?}", h.app.status);
    let (w, hh) = (40.0, 20.0);
    let r: f64 = 0.2 * 20.0;
    let want = w * hh - (4.0 - std::f64::consts::PI) * r * r;
    assert!((sketch.profiles[0].area - want).abs() < 0.05 * want, "area {} want about {want}", sketch.profiles[0].area);
}
