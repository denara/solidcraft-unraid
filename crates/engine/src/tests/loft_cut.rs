//! A hollow elbow: a tube lofted from round sections along an arc, minus a narrower tube lofted the
//! same way (and running 5 mm past both ends, so the cut opens the ends).
//!
//! The boolean gave up with "no result" for some section counts and not for their neighbours: the
//! points of the intersection curves lie on the seam between the two halves of a loft's circle,
//! where truck's search for their parameters (from the previous point's) failed.

use std::f64::consts::PI;

use super::*;

/// `sections` circles of radius `radius` centred on an arc of radius `path` turning `angle`
/// (radians), in planes normal to the arc, lofted into the body `body`; `over` extends the
/// sections past both ends along the tangent.
fn tube(s: &mut Session, body: &str, radius: f64, path: f64, angle: f64, sections: usize, over: f64) {
    let mut stations: Vec<[f64; 4]> = (0..sections)
        .map(|i| {
            let t = angle * i as f64 / (sections - 1) as f64;
            [path * t.cos(), path * t.sin(), -t.sin(), t.cos()]
        })
        .collect();
    if over > 0.0 {
        let (a, b) = (stations[0], stations[sections - 1]);
        stations.insert(0, [a[0] - over * a[2], a[1] - over * a[3], a[2], a[3]]);
        stations.push([b[0] + over * b[2], b[1] + over * b[3], b[2], b[3]]);
    }
    let mut list = Vec::new();
    for (i, [x, y, tx, ty]) in stations.iter().enumerate() {
        let name = format!("{body}{i}");
        run(s, "sketch.create", json!({"plane": {"origin": [x, y, 0], "x_dir": [0, 0, 1], "y_dir": [ty, -tx, 0]}, "name": name}));
        run(s, "sketch.circle.center", json!({"center": [0, 0], "radius": radius}));
        run(s, "sketch.finish", json!({}));
        list.push(json!({"sketch": name}));
    }
    run(s, "solid.loft", json!({"sections": list, "operation": "new", "name": format!("Loft{body}"), "body_name": body}));
}

#[test]
fn a_hollow_elbow_cut_from_two_lofted_tubes() {
    let (path, outer, inner) = (100.0, 30.0, 18.0);
    // Each of these gave up in truck's `parameter boundary` step.
    for (degrees, sections) in [(60.0, 5), (60.0, 7), (90.0, 10), (120.0, 7), (120.0, 13)] {
        let angle = degrees * PI / 180.0;
        let mut s = Session::default();
        tube(&mut s, "Outer", outer, path, angle, sections, 0.0);
        tube(&mut s, "Inner", inner, path, angle, sections, 5.0);
        run(&mut s, "solid.combine", json!({"target": "Outer", "tools": ["Inner"], "operation": "cut", "name": "Hollow"}));
        assert_eq!(s.model.state().bodies.len(), 1, "{degrees} degrees, {sections} sections");
        let want = PI * (outer * outer - inner * inner) * path * angle;
        let got = volume(&mut s);
        assert!(rel(got, want) < 3e-3, "{degrees} degrees, {sections} sections: {got} vs {want}");
    }
}
