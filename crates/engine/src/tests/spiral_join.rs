//! Two interleaved spiral walls, joined (the case a of #844: 22 mm pitch, 0.75 turn).

use std::f64::consts::TAU;

use super::*;

/// The join used to be an error: the walls' flat tops and bottoms are B-spline faces, so nothing
/// saw that they coincide. Now they are prisms, and the join is decided in 2D.
#[test]
fn two_interleaved_spiral_walls_join_into_one_body() {
    let (pitch, turns, points) = (22.0, 0.75, 19usize);
    let (thickness, height, start) = (12.0, 60.0, 60.0);
    let (k, end) = (pitch / TAU, turns * TAU);
    let arm = |sign: f64| -> Vec<[f64; 2]> {
        (0..points)
            .map(|i| {
                let a = end * i as f64 / (points - 1) as f64;
                let r = start + k * a;
                [sign * r * a.cos(), sign * r * a.sin()]
            })
            .collect()
    };
    let mut s = Session::default();
    for (tag, sign) in [("A", 1.0), ("B", -1.0)] {
        let x0 = sign * start;
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": format!("Path{tag}")}));
        run(&mut s, "sketch.spline.fit_point", json!({"points": arm(sign)}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "sketch.create", json!({"plane": "XZ", "name": format!("Prof{tag}")}));
        run(&mut s, "sketch.rectangle.two_point", json!({"p0": [x0 - thickness / 2.0, -height], "p1": [x0 + thickness / 2.0, 0]}));
        run(&mut s, "sketch.finish", json!({}));
        let sweep = json!({
            "sketch": format!("Prof{tag}"), "path_sketch": format!("Path{tag}"), "path": ["s1"],
            "operation": "new", "name": format!("Sweep{tag}"), "body_name": format!("Band{tag}"),
        });
        run(&mut s, "solid.sweep", sweep);
    }
    run(&mut s, "solid.combine", json!({"target": "BandA", "tools": ["BandB"], "operation": "join", "name": "Joined"}));
    assert_eq!(s.model.state().bodies.len(), 1);
    // The two walls' plan areas united, by a plan-view raster (0.6 mm cells, flat ends), times the height.
    let want = 450_878.0;
    let got = volume(&mut s);
    assert!(rel(got, want) < 5e-3, "{got} vs {want}");
}
