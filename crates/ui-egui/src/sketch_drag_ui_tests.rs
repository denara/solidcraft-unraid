//! Dragging sketch geometry in the viewport: live frames re-solve the sketch only, letting go
//! brings the model up to date in the drag's one undo step, and the grab stays on the geometry.

use serde_json::{Value, json};
use solidcraft_engine::geom::Vec2;

use crate::scenario::Harness;

#[test]
fn text_move_drag_is_one_editable_unit_and_escape_restores_the_gesture() {
    for cancel in [false, true] {
        let mut h = Harness::new();
        h.app.home.open = false;
        run(&mut h, "sketch.create", json!({"plane":"XY"}));
        run(&mut h, "sketch.text", json!({"text":"II","at":[0,0],"height":10}));
        run(&mut h, "sketch.line", json!({"points":[[15,0],[15,10]],"connect":false}));
        h.call("ui.view", json!({"view":"top"}));
        h.app.cam.distance = 35.0;
        h.frames(3);
        let before = h.app.session.doc.to_json();
        let undo = h.app.session.undo.len();
        let sk = h.app.session.doc.sketch(1).unwrap();
        let line = sk.curves.last().unwrap().id.clone();
        let neighbor = point(&h, &format!("{line}.start"));
        let polyline = sk.polyline(0);
        let at = (polyline[0] + polyline[1]) * 0.5;
        let a = screen(&mut h, at.x, at.y);
        let b = screen(&mut h, at.x + 3.0, at.y + 2.0);
        let mut path = vec![a];
        path.extend(steps(a, b, 6));
        press_and_move(&mut h, &path);
        assert!(h.app.viewport.text_drag.is_some());
        assert_ne!(h.app.session.doc.to_json(), before);
        if cancel {
            h.step(&json!({"key":"Escape"})).unwrap();
        }
        release(&mut h, b);
        assert!(h.app.viewport.text_drag.is_none());
        assert_eq!(h.app.session.active_sketch, Some(1));
        assert!(point(&h, &format!("{line}.start")).dist(neighbor) < 1e-9);
        if cancel {
            assert_eq!(h.app.session.doc.to_json(), before);
            assert_eq!(h.app.session.undo.len(), undo);
        } else {
            assert_eq!(h.app.session.undo.len(), undo + 1);
            let sk = h.app.session.doc.sketch(1).unwrap();
            let solidcraft_engine::sketch::LinkSource::Text { at, .. } = &sk.links[0].source else { panic!() };
            assert!(at.dist(Vec2::new(3.0, 2.0)) < 0.02, "{at:?}");
            run(&mut h, "edit.undo", json!({}));
            assert_eq!(h.app.session.doc.to_json(), before);
        }
    }
}

fn run(h: &mut Harness, id: &str, p: Value) {
    if let Err(e) = h.app.run(id, p.clone()) {
        panic!("{id} {p}: {e}");
    }
}

fn screen(h: &mut Harness, x: f64, y: f64) -> egui::Pos2 {
    let [sx, sy] = h.at(&json!({"sketch": [x, y]})).expect("on screen");
    egui::pos2(sx as f32, sy as f32)
}

fn point(h: &Harness, id: &str) -> Vec2 {
    let sid = h.app.session.active_sketch.expect("sketch mode");
    let st = h.app.session.world_state();
    let sk = &st.sketch(sid).expect("the sketch").sketch;
    sk.resolve_point(id).and_then(|i| sk.point(i)).unwrap_or_else(|| panic!("no point {id}"))
}

/// Press at `path[0]` and move through the rest, one frame per event.
fn press_and_move(h: &mut Harness, path: &[egui::Pos2]) {
    let Some(&a) = path.first() else { return };
    h.app.synthetic.push(egui::Event::PointerMoved(a));
    h.app.synthetic.push(egui::Event::PointerButton { pos: a, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
    h.app.synthetic.extend(path.iter().skip(1).map(|&p| egui::Event::PointerMoved(p)));
    play(h);
}

fn release(h: &mut Harness, at: egui::Pos2) {
    h.app.synthetic.push(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
    play(h);
}

fn play(h: &mut Harness) {
    while !h.app.synthetic.is_empty() {
        h.frame();
    }
    h.frames(2);
}

/// Points from `a` to `b` in `n` equal moves (without `a`).
fn steps(a: egui::Pos2, b: egui::Pos2, n: usize) -> Vec<egui::Pos2> {
    (1..=n).map(|k| a + (b - a) * (k as f32 / n as f32)).collect()
}

#[test]
fn a_viewport_drag_solves_the_sketch_live_and_rebuilds_the_model_on_release() {
    let mut h = Harness::new();
    run(&mut h, "sketch.create", json!({"plane": "XY"}));
    run(&mut h, "sketch.rectangle.two_point", json!({"p0": [5, 5], "p1": [25, 20]}));
    run(&mut h, "sketch.dimension", json!({"entities": ["l1"], "value": 20}));
    run(&mut h, "sketch.dimension", json!({"entities": ["l2"], "value": 15}));
    run(&mut h, "sketch.finish", json!({}));
    run(&mut h, "solid.extrude", json!({"distance": 5}));
    let sketch = h.app.session.doc.features.first().map(|f| f.id).expect("the sketch");
    h.app.edit_sketch(sketch);
    h.settle();
    let undo = h.app.session.undo.len();
    let a = screen(&mut h, 5.0, 5.0);
    let b = screen(&mut h, -3.0, -6.0);
    let mut path = vec![a];
    path.extend(steps(a, b, 8));
    press_and_move(&mut h, &path);
    assert!(h.app.viewport.point_drag.is_some(), "the drag is under way");
    assert!(h.app.session.model.is_stale(), "the frames updated the sketch only");
    assert!(point(&h, "l1.start").dist(Vec2::new(-3.0, -6.0)) < 0.1, "{:?}", point(&h, "l1.start"));
    release(&mut h, b);
    assert!(h.app.viewport.point_drag.is_none());
    assert!(!h.app.session.model.is_stale(), "letting go evaluated the model");
    assert_eq!(h.app.session.undo.len(), undo + 1, "one undo step for the drag");
    assert!(point(&h, "l1.start").dist(Vec2::new(-3.0, -6.0)) < 0.1);
    run(&mut h, "edit.undo", json!({}));
    assert!(point(&h, "l1.start").dist(Vec2::new(5.0, 5.0)) < 1e-9);
}

#[test]
fn a_dragged_line_that_stops_short_stays_under_the_cursor_when_it_can_follow_again() {
    let mut h = Harness::new();
    run(&mut h, "sketch.create", json!({"plane": "XY"}));
    // Two 10 mm links from the origin hold the start of a horizontal 10 mm line: the line can
    // slide right until its start is 20 mm from the origin (3 mm).
    let (x, y) = (15.0, 75f64.sqrt());
    run(&mut h, "sketch.line", json!({"points": [[0, 0], [10, 0], [x, y], [x + 10.0, y]]}));
    for l in ["l1", "l2", "l3"] {
        run(&mut h, "sketch.dimension", json!({"entities": [l], "value": 10}));
    }
    run(&mut h, "sketch.constraint.horizontal_vertical", json!({"line": "l3"}));
    let grab = screen(&mut h, x + 5.0, y);
    let far = screen(&mut h, x + 11.0, y);
    let back = screen(&mut h, x + 7.0, y);
    let mut path = vec![grab];
    path.extend(steps(grab, far, 8));
    path.extend(steps(far, back, 4));
    press_and_move(&mut h, &path);
    release(&mut h, back);
    // The cursor went 6 mm right, the line only 3; back at 2 mm right it is under it again.
    let start = point(&h, "l3.start");
    assert!(start.dist(Vec2::new(x + 2.0, y)) < 0.1, "{start:?}");
    assert!(!h.app.session.model.is_stale());
}
