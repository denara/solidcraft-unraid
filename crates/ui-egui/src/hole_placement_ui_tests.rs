//! Hole placement on the canvas. A Single hole picked on or near a sketch point on the face (a
//! circle's centre) goes to that point, as a Multiple pick of the same point does (#657); a pick
//! away from sketch points, or near one off the face under the cursor, stays where it was clicked.

use serde_json::{Value, json};
use solidcraft_engine::Sel;
use solidcraft_engine::doc::FeatureKind;
use solidcraft_engine::geom::Vec3;
use solidcraft_engine::sketch::CurveKind;

use crate::dialogs::Kind;
use crate::scenario::Harness;
use crate::viewport::Hit;

/// A 40 × 30 × 20 block, seen from the top, with a sketch on its top face: a circle of radius 4
/// about (12, 9). Returns the circle centre in the world.
fn block_with_circle() -> (Harness, Vec3) {
    let (h, centre) = block_and_circle_on(json!({"plane": {"face": [20, 15, 20]}}), [12.0, 9.0]);
    assert!((centre.z - 20.0).abs() < 1e-9, "the circle lies on the top face: {centre:?}");
    (h, centre)
}

/// The block, seen from the top, and a circle of radius 4 about `at` (sketch coordinates) in the
/// sketch `sketch` (`sketch.create`'s parameters) makes. Returns the circle centre in the world.
fn block_and_circle_on(sketch: Value, at: [f64; 2]) -> (Harness, Vec3) {
    let mut h = Harness::new();
    h.app.ui.show_origin = false;
    h.app.run("solid.box", json!({"length": 40, "width": 30, "height": 20})).unwrap();
    h.app.run("sketch.create", sketch).unwrap();
    h.app.run("sketch.circle.center", json!({"center": at, "radius": 4})).unwrap();
    h.app.run("sketch.finish", json!({})).unwrap();
    h.step(&json!({"call": "ui.view", "params": {"view": "top"}})).unwrap();
    h.settle();
    let st = h.app.session.world_state();
    let ss = st.sketches.first().unwrap();
    let centre = ss
        .sketch
        .curves
        .iter()
        .find_map(|c| match c.kind {
            CurveKind::Circle { c, .. } => Some(ss.plane.to_world(ss.sketch.points[c].pos)),
            _ => None,
        })
        .unwrap();
    (h, centre)
}

/// Where a world point is on screen.
fn screen(h: &mut Harness, p: Vec3) -> [f64; 2] {
    h.at(&json!({"world": [p.x, p.y, p.z]})).unwrap()
}

/// Five pixels off where `p` is on screen: within reach of a sketch point pick, not on `p`.
fn near(h: &mut Harness, p: Vec3) -> [f64; 2] {
    let c = screen(h, p);
    [c[0] + 4.0, c[1] + 3.0]
}

/// Everything under a screen point, as a click there sees it.
fn hits_at(h: &Harness, at: [f64; 2]) -> Vec<Hit> {
    let proj = crate::viewport::projection(&h.app, h.app.viewport.rect.unwrap());
    crate::viewport::pick(&h.app, &proj, egui::pos2(at[0] as f32, at[1] as f32))
}

/// The face the cursor is over, where the ray meets it, after hovering `at`.
fn hovered_face_point(h: &mut Harness, at: [f64; 2]) -> Vec3 {
    h.step(&json!({ "move": at })).unwrap();
    match &h.app.viewport.hover {
        Some(Hit::Face { point, .. }) => *point,
        other => panic!("the face lights up, not {other:?}"),
    }
}

/// The last feature's hole placement: its id, its position and whether it is at sketch points.
fn hole(h: &Harness) -> (u64, Vec3, bool) {
    let f = h.app.session.doc.features.last().unwrap();
    match &f.kind {
        FeatureKind::Hole { position, points, .. } => (f.id, *position, points.is_some()),
        other => panic!("the last feature is not a hole: {}", other.type_name()),
    }
}

/// The face places the open dialog's first input holds.
fn face_places(h: &Harness) -> Vec<(String, Vec3)> {
    h.app.dialog.as_ref().unwrap().inputs[0]
        .items
        .iter()
        .map(|x| match x {
            Sel::Face { body, point, .. } => (body.clone(), *point),
            other => panic!("a face place, not {other:?}"),
        })
        .collect()
}

/// Whether the model holds material at `p`.
fn solid_at(h: &Harness, p: Vec3) -> bool {
    h.app.session.model.state().bodies.iter().any(|b| b.body.contains(p))
}

#[test]
fn a_single_hole_picked_near_a_circle_centre_goes_to_the_centre() {
    let (mut h, centre) = block_with_circle();
    let click = near(&mut h, centre);
    h.step(&json!({"start": "solid.hole"})).unwrap();
    let d = h.app.dialog.as_ref().unwrap();
    assert!(matches!(&d.kind, Kind::Hole { opts, .. } if !opts.multiple), "a new hole starts with Single placement");
    h.step(&json!({"move": click})).unwrap();
    assert!(
        matches!(&h.app.viewport.hover, Some(Hit::SketchPoint { .. })),
        "the centre lights up as what the pick snaps to: {:?}",
        h.app.viewport.hover
    );
    h.step(&json!({"click": click})).unwrap();
    let places = face_places(&h);
    assert!(matches!(places.as_slice(), [(_, p)] if p.dist(centre) < 1e-9), "the face is picked at the centre {centre:?}, not {places:?}");
    h.step(&json!({"key": "Enter"})).unwrap();
    assert!(h.app.dialog.is_none(), "OK closed the dialog: {:?}", h.app.dialog.as_ref().map(|d| &d.error));
    let (id, position, at_points) = hole(&h);
    assert!(!at_points, "a Single hole is placed on its face");
    assert!(position.dist(centre) < 1e-9, "the hole is at the circle centre {centre:?}, not at {position:?}");
    assert!(h.app.session.model.result(id).is_some_and(|r| r.error.is_none() && r.warning.is_none()));
    // The bore (5 mm across, 10 mm deep) is round about the centre: empty on its axis and up to
    // 2.4 mm from it, solid 2.6 mm from it in every direction.
    let bore = |h: &Harness| {
        for k in 0..8 {
            let a = std::f64::consts::FRAC_PI_4 * f64::from(k);
            let dir = Vec3::new(a.cos(), a.sin(), 0.0);
            let at = |r: f64| centre + dir * r - Vec3::new(0.0, 0.0, 5.0);
            assert!(!solid_at(h, at(0.0)) && !solid_at(h, at(2.4)), "the bore is empty {a} rad from +x");
            assert!(solid_at(h, at(2.6)), "the wall is solid {a} rad from +x");
        }
    };
    bore(&h);
    // Edit Feature shows the Single placement on the top face at the centre, and OK keeps it.
    h.step(&json!({"call": "ui.editFeature", "params": {"feature": id}})).unwrap();
    h.frames(4);
    let d = h.app.dialog.as_ref().expect("the hole dialog");
    assert!(matches!(&d.kind, Kind::Hole { opts, .. } if !opts.multiple), "{:?}", d.kind);
    let places = face_places(&h);
    assert!(matches!(places.as_slice(), [(body, p)] if body == "Body1" && p.dist(centre) < 1e-6), "{places:?}");
    h.step(&json!({"key": "Enter"})).unwrap();
    assert!(h.app.dialog.is_none(), "OK closed the dialog: {:?}", h.app.dialog.as_ref().map(|d| &d.error));
    let (again, position, at_points) = hole(&h);
    assert_eq!(again, id, "the same feature");
    assert!(!at_points && position.dist(centre) < 1e-9, "{position:?}");
    bore(&h);
}

#[test]
fn a_single_hole_picked_away_from_sketch_points_stays_where_it_was_clicked() {
    let (mut h, centre) = block_with_circle();
    // Inside the circle, well away from its centre (and from every other sketch point).
    let away = centre + Vec3::new(3.0, 2.0, 0.0);
    let click = screen(&mut h, away);
    let c = screen(&mut h, centre);
    assert!(c.iter().zip(click).map(|(a, b)| (a - b).powi(2)).sum::<f64>().sqrt() > 16.0, "twice a point pick's reach from the centre");
    h.step(&json!({"start": "solid.hole"})).unwrap();
    let raw = hovered_face_point(&mut h, click);
    h.step(&json!({"click": click})).unwrap();
    h.step(&json!({"key": "Enter"})).unwrap();
    assert!(h.app.dialog.is_none());
    let (_, position, at_points) = hole(&h);
    assert!(!at_points);
    assert!(position.dist(raw) < 1e-9 && position.dist(away) < 1e-3, "the hole stays where it was clicked {raw:?}, not at {position:?}");
}

#[test]
fn a_single_hole_picked_near_a_sketch_point_off_the_face_under_it_stays_where_it_was_clicked() {
    // The circle 10 mm above the top face (its centre lies on no face), then on the bottom face
    // (hidden behind the top face): either way its centre is under the pick on screen, but not on
    // the face picked.
    // (The bottom face's sketch looks up through the block: its y runs along the world's −y.)
    let cases = [(json!({"plane": "XY", "offset": 30}), [12.0, 9.0], 30.0), (json!({"plane": {"face": [20, 15, 0]}}), [12.0, -9.0], 0.0)];
    for (sketch, at, z) in cases {
        let (mut h, centre) = block_and_circle_on(sketch, at);
        assert!(centre.dist(Vec3::new(12.0, 9.0, z)) < 1e-9, "{centre:?}");
        let click = near(&mut h, centre);
        h.step(&json!({"start": "solid.hole"})).unwrap();
        let hits = hits_at(&h, click);
        assert!(matches!(hits.first(), Some(Hit::SketchPoint { .. })), "the click reaches the centre: {hits:?}");
        let raw = hovered_face_point(&mut h, click);
        h.step(&json!({"click": click})).unwrap();
        h.step(&json!({"key": "Enter"})).unwrap();
        assert!(h.app.dialog.is_none());
        let (_, position, at_points) = hole(&h);
        assert!(!at_points);
        assert!(position.dist(raw) < 1e-9, "the hole stays where it was clicked {raw:?}, not at {position:?}");
        assert!((position.z - 20.0).abs() < 1e-9 && position.dist(Vec3::new(centre.x, centre.y, 20.0)) > 0.1, "{position:?}");
    }
}

#[test]
fn select_other_offers_the_centre_and_the_face_as_picked() {
    let (mut h, centre) = block_with_circle();
    let click = near(&mut h, centre);
    h.step(&json!({"start": "solid.hole"})).unwrap();
    // Where the click's ray meets the top face.
    let ray = hits_at(&h, click);
    let Some(Hit::Face { point: raw, .. }) = ray.iter().find(|x| matches!(x, Hit::Face { .. })) else { panic!("{ray:?}") };
    let raw = *raw;
    assert!(raw.dist(centre) > 0.1);
    for (row, want) in [(0, centre), (1, raw)] {
        let list = h.call("ui.selectOther", json!({"x": click[0], "y": click[1]}))["result"].clone();
        let labels: Vec<&str> = list["rows"].as_array().into_iter().flatten().filter_map(|r| r["label"].as_str()).collect();
        assert!(labels.len() == 2 && labels[0].contains("centre") && labels[1].starts_with("Face"), "the snapped centre, then the face: {labels:?}");
        h.call("ui.selectOther", json!({ "pick": row }));
        let places = face_places(&h);
        assert!(matches!(places.as_slice(), [(_, p)] if p.dist(want) < 1e-9), "row {row} places at {want:?}: {places:?}");
        // Start over for the other row (a second pick on the same face would take it out).
        h.step(&json!({"key": "Escape"})).unwrap();
        h.step(&json!({"start": "solid.hole"})).unwrap();
    }
}

#[test]
fn select_other_keeps_the_face_as_picked_beside_many_snapped_points() {
    // More points under the cursor than Select Other lists of one kind: the snapped places are
    // capped as points, and the face as picked keeps its row.
    let mut h = Harness::new();
    h.app.ui.show_origin = false;
    h.app.run("solid.box", json!({"length": 40, "width": 30, "height": 20})).unwrap();
    h.app.run("sketch.create", json!({"plane": {"face": [20, 15, 20]}})).unwrap();
    let many = crate::select_other::MAX_PER_KIND + 4;
    for i in 0..many {
        let a = i as f64 * 0.3;
        h.app.run("sketch.point", json!({"point": [12.0 + 0.1 * a.cos(), 9.0 + 0.1 * a.sin()]})).unwrap();
    }
    h.app.run("sketch.finish", json!({})).unwrap();
    h.step(&json!({"call": "ui.view", "params": {"view": "top"}})).unwrap();
    h.settle();
    let click = near(&mut h, Vec3::new(12.0, 9.0, 20.0));
    h.step(&json!({"start": "solid.hole"})).unwrap();
    let list = h.call("ui.selectOther", json!({"x": click[0], "y": click[1]}))["result"].clone();
    let rows = list["rows"].as_array().cloned().unwrap_or_default();
    let count = |prefix: &str| rows.iter().filter(|r| r["label"].as_str().is_some_and(|l| l.starts_with(prefix))).count();
    assert_eq!((count("Point"), count("Face")), (crate::select_other::MAX_PER_KIND, 1), "{list}");
    assert_eq!(list["total"], json!(many + 1), "{list}");
}
