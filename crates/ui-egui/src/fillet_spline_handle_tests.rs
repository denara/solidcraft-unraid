use crate::scenario::Harness;
use serde_json::{Value, json};
use solidcraft_engine::doc::{FeatureKind, FilletStyle, kernel};

/// A plate whose top narrows from 20 to 2 mm along 40 mm, its top front edge rounded at 3 mm
/// with Taper to Fit.
fn fixture() -> Harness {
    let mut h = Harness::new();
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    h.app.run("sketch.line", json!({"points": [[0, 0], [40, 0], [40, 2], [0, 20]], "closed": true})).unwrap();
    h.app.run("solid.extrude", json!({"distance": 15, "body_name": "Plate"})).unwrap();
    h.app.run("solid.fillet", json!({"edges": [[20, 0, 15]], "radius": 3, "fit": "taper", "name": "Round"})).unwrap();
    h.frames(4);
    h
}

/// Open the fillet for editing and wait for its preview.
fn open(h: &mut Harness) {
    h.step(&json!({"call": "ui.view", "params": {"view": "iso"}})).unwrap();
    h.step(&json!({"call": "ui.view", "params": {"view": "fit"}})).unwrap();
    h.step(&json!({"call": "ui.editFeature", "params": {"feature": "Round"}})).unwrap();
    let began = std::time::Instant::now();
    while (h.app.preview.busy || h.app.preview.tapered.is_empty()) && began.elapsed() < std::time::Duration::from_secs(60) {
        h.frame();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    h.frames(4);
}

fn volume(h: &Harness) -> f64 {
    kernel::measure(&h.app.session.world_state().body("Plate").unwrap().body).unwrap().volume
}

#[test]
fn dragging_a_taper_handle_makes_the_fillet_a_spline() {
    let mut h = fixture();
    let tapered = volume(&h);
    open(&mut h);
    let (_, taper) = h.app.preview.tapered.first().cloned().expect("the preview's taper");
    let law =
        if taper.start.dist(solidcraft_engine::geom::Vec3::new(0.0, 0.0, 15.0)) < 1e-9 { taper.spline.clone() } else { taper.spline.reversed() };
    // A handle per control radius, each that radius into the body from the edge.
    let into = solidcraft_engine::geom::Vec3::new(0.0, 1.0, -1.0) * std::f64::consts::FRAC_1_SQRT_2;
    let at = |u: f64, r: f64| {
        let p = solidcraft_engine::geom::Vec3::new(40.0 * u, 0.0, 15.0) + into * r;
        json!([p.x, p.y, p.z])
    };
    let g = law.greville();
    assert_eq!(g.len(), law.radii.len());
    for (k, (u, r)) in g.iter().zip(&law.radii).enumerate() {
        let k = if taper.spline == law { k } else { g.len() - 1 - k };
        let shown = h.at(&json!({"handle": format!("fillet_r{k}")})).unwrap();
        let want = h.at(&json!({"world": at(*u, *r)})).unwrap();
        assert!((shown[0] - want[0]).hypot(shown[1] - want[1]) < 1.0, "handle {k}: {shown:?} vs {want:?}");
    }
    // The middle control radius pulled out to 4 mm (free drag): the fillet keeps the taper's
    // spline as its own with that radius changed.
    let mid = g.len() / 2;
    let k = if taper.spline == law { mid } else { g.len() - 1 - mid };
    h.step(&json!({"drag": [{"handle": format!("fillet_r{k}")}, {"world": at(g[mid], 4.0)}], "ctrl": true})).unwrap();
    let d = h.app.dialog.as_ref().unwrap();
    assert_eq!(d.extra.get("type").and_then(Value::as_str), Some("spline"));
    assert!(d.extra.get("fit").is_none());
    h.step(&json!({"widget": "OK"})).unwrap();
    let FeatureKind::Fillet { style: FilletStyle::Spline { knots, radii, .. }, radius, .. } = &h.app.session.doc.find_feature("Round").unwrap().kind
    else {
        panic!("a spline fillet")
    };
    assert_eq!(knots, &taper.spline.knots);
    let values: Vec<f64> =
        std::iter::once(radius).chain(radii).map(|e| h.app.session.doc.eval(e, solidcraft_engine::doc::expr::Kind::Length).unwrap()).collect();
    let moved: Vec<usize> = (0..values.len()).filter(|i| (values[*i] - taper.spline.radii[*i]).abs() > 1e-3).collect();
    assert_eq!(moved, [k], "{values:?} vs {:?}", taper.spline.radii);
    assert!((values[k] - 4.0).abs() < 0.05, "{values:?}");
    assert!(volume(&h) < tapered, "{} vs {tapered}", volume(&h));
}
