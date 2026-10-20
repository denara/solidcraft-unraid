//! A sketch tool's click that moves on the way (#2448): egui takes a press that moves past its
//! click distance as a drag, which the tools dropped, so a hand that moved while clicking placed
//! nothing.

use crate::scenario::Harness;
use serde_json::json;
use solidcraft_engine::sketch::CurveKind;

fn harness(command: &str) -> Harness {
    let mut h = Harness::new();
    h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
    h.app.cam.distance = 120.0;
    h.settle();
    h.step(&json!({"start":command})).unwrap();
    h
}

/// Press `dx` screen points left of sketch point `at`, move there and let go.
fn moving_click(h: &mut Harness, at: [f64; 2], dx: f64) {
    let p = h.call("ui.at", json!({"sketch":at}))["result"].clone();
    let (x, y) = (p[0].as_f64().unwrap(), p[1].as_f64().unwrap());
    h.call("ui.drag", json!({"x0":x - dx,"y0":y,"x1":x,"y1":y,"steps":4}));
}

fn curves(h: &Harness) -> usize {
    let sid = h.app.session.active_sketch.unwrap();
    h.app.session.model.state().sketch(sid).unwrap().sketch.curves.len()
}

#[test]
fn clicks_that_move_place_their_points() {
    for (command, points, finish) in [
        ("sketch.slot.center_to_center", vec![[-20.0, 0.0], [20.0, 0.0], [0.0, 8.0]], None),
        ("sketch.line", vec![[-20.0, 0.0], [20.0, 5.0]], Some("Escape")),
        ("sketch.arc.three_point", vec![[-20.0, 0.0], [20.0, 0.0], [0.0, 10.0]], None),
        ("sketch.rectangle.three_point", vec![[-20.0, 0.0], [20.0, 0.0], [20.0, 10.0]], None),
        ("sketch.circle.three_point", vec![[-20.0, 0.0], [20.0, 0.0], [0.0, 20.0]], None),
        ("sketch.ellipse", vec![[0.0, 0.0], [25.0, 0.0], [0.0, 10.0]], None),
        ("sketch.spline.fit_point", vec![[-20.0, 0.0], [0.0, 10.0], [20.0, 0.0]], Some("right")),
    ] {
        let mut h = harness(command);
        for at in &points {
            moving_click(&mut h, *at, 12.0);
        }
        match finish {
            Some("right") => {
                let p = h.call("ui.at", json!({"sketch":points[points.len() - 1]}))["result"].clone();
                h.call("ui.click", json!({"x":p[0],"y":p[1],"button":"right"}));
            }
            Some(key) => h.step(&json!({"key":key})).unwrap(),
            None => {}
        }
        h.settle();
        assert!(curves(&h) > 0, "{command}: nothing drawn; {:?} {:?}", h.app.tool, h.app.status);
        // The points went where the button was let go: the first one is at its spot.
        let sid = h.app.session.active_sketch.unwrap();
        let sk = h.app.session.model.state().sketch(sid).unwrap().sketch.clone();
        let first = solidcraft_engine::geom::Vec2::new(points[0][0], points[0][1]);
        // (A three-point circle keeps only its centre: through all three, its radius is 20.)
        let through = sk.curves.iter().any(|c| matches!(c.kind, CurveKind::Circle { r, .. } if (r - 20.0).abs() < 0.5));
        assert!(sk.points.iter().any(|p| (p.pos - first).len() < 0.5) || through, "{command}: no point at {first:?}");
    }
}

#[test]
fn a_short_drag_starts_a_rectangle_circle_or_polygon_by_clicks() {
    for (command, second, count) in [
        ("sketch.rectangle.two_point", [20.0, 10.0], 4),
        ("sketch.rectangle.center", [20.0, 10.0], 6),
        ("sketch.circle.center", [15.0, 0.0], 1),
        ("sketch.polygon.inscribed", [15.0, 0.0], 7),
    ] {
        let mut h = harness(command);
        // A press that moves 12 points (past egui's click distance, short of a drawn shape) is
        // the first point, not a sliver of a shape.
        moving_click(&mut h, [0.0, 0.0], 12.0);
        h.settle();
        assert_eq!(curves(&h), 0, "{command}: a short drag drew a shape");
        assert_eq!(h.app.tool.as_ref().unwrap().pts.len(), 1, "{command}");
        moving_click(&mut h, second, 12.0);
        h.settle();
        assert_eq!(curves(&h), count, "{command}: {:?}", h.app.status);
        let sid = h.app.session.active_sketch.unwrap();
        let sk = h.app.session.model.state().sketch(sid).unwrap().sketch.clone();
        let far = solidcraft_engine::geom::Vec2::new(second[0], second[1]);
        let circle = |c: &solidcraft_engine::sketch::Curve| matches!(c.kind, CurveKind::Circle { r, .. } if (r - far.len()).abs() < 0.5);
        let on_shape = sk.points.iter().any(|p| (p.pos - far).len() < 0.5) || sk.curves.iter().any(circle);
        assert!(on_shape, "{command}: the second click wasn't where it was let go");
    }
}
