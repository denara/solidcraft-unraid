//! A hole placed on a face follows that face along the hole's axis, never sideways: when the face
//! moves the hole moves with it, and when the face no longer reaches the place the hole stays
//! where it was put and says so instead of sliding to the face's nearest point.

use serde_json::{Value, json};
use solidcraft_geom::Vec3;

use crate::Session;

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    match s.execute(id, &p) {
        Ok(v) => v,
        Err(e) => panic!("{id} {p}: {e}"),
    }
}

fn change(s: &mut Session, name: &str, expression: &str) {
    run(s, "parameters.change", json!({"name": name, "expression": expression}));
}

fn volume(s: &mut Session) -> f64 {
    run(s, "inspect.measure", json!({}))["total"]["volume_mm3"].as_f64().unwrap()
}

/// Whether the model holds material at (x, y, z).
fn solid_at(s: &Session, x: f64, y: f64, z: f64) -> bool {
    s.model.state().bodies.iter().any(|b| b.body.contains(Vec3::new(x, y, z)))
}

/// What the hole feature says (its error and its warning); empty when it is silent.
fn said(s: &Session, feature: &str) -> String {
    let id = s.doc.find_feature(feature).unwrap().id;
    let r = s.model.result(id).unwrap();
    format!("{}{}", r.error.clone().unwrap_or_default(), r.warning.clone().unwrap_or_default())
}

#[test]
fn a_hole_the_face_no_longer_reaches_stays_where_it_was_placed_and_says_so() {
    let mut s = Session::default();
    run(&mut s, "parameters.add", json!({"name": "length", "expression": "100 mm"}));
    run(&mut s, "solid.box", json!({"length": "length", "width": 50, "height": 20, "name": "Plate"}));
    run(&mut s, "solid.hole", json!({"position": [80, 25, 20], "diameter": "6 mm", "name": "Hole"}));
    // The bore is at x = 80, 3 mm either way.
    let drilled_at_80 = |s: &Session| !solid_at(s, 80.0, 25.0, 10.0) && solid_at(s, 76.0, 25.0, 10.0) && solid_at(s, 84.0, 25.0, 10.0);
    assert!(drilled_at_80(&s));
    // Longer: the place is still on the top face, and the hole stays at x = 80.
    change(&mut s, "length", "150 mm");
    assert_eq!(said(&s, "Hole"), "");
    assert!(drilled_at_80(&s));
    // Shorter than the place: nothing is drilled at the new end, and the hole says why. A hole
    // moved to the face's nearest point would cut half a hole there (59 717 mm³ left).
    change(&mut s, "length", "60 mm");
    let left = volume(&mut s);
    assert!((left - 60_000.0).abs() < 1.0, "half a hole was cut at the plate's end: {left} mm³ of 60000");
    let msg = said(&s, "Hole");
    assert!(msg.contains("no longer on face") && msg.contains("[80, 25, 20]"), "{msg}");
    // Long again: the hole is back where it was put, with no message.
    change(&mut s, "length", "100 mm");
    assert_eq!(said(&s, "Hole"), "");
    assert!(drilled_at_80(&s));
}

#[test]
fn a_slanted_hole_stays_on_its_axis_when_its_face_moves() {
    let mut s = Session::default();
    run(&mut s, "parameters.add", json!({"name": "height", "expression": "20 mm"}));
    run(&mut s, "solid.box", json!({"length": 60, "width": 30, "height": "height", "name": "Block"}));
    // Entering the top face at x = 20, running on in x as it goes down: 1 across for every 2 down.
    run(&mut s, "solid.hole", json!({"position": [20, 15, 20], "direction": [1, 0, -2], "diameter": "2 mm", "name": "Hole"}));
    // The axis is the line through the place along the direction, whatever the height of the block:
    // x = 20 + (20 - z) / 2. At mid-height the bore is empty on it and solid 3 mm to either side
    // (the bore is about 2.2 mm wide there).
    let on_axis = |s: &Session, h: f64| {
        let (x, z) = (20.0 + (20.0 - h / 2.0) / 2.0, h / 2.0);
        assert!(!solid_at(s, x, 15.0, z), "block height {h}: the bore should pass through x = {x} at z = {z}");
        assert!(solid_at(s, x - 3.0, 15.0, z) && solid_at(s, x + 3.0, 15.0, z), "block height {h}: material 3 mm beside the axis");
        // (The kernel may note a boolean retry for a slanted bore; the hole's place is on its face.)
        assert!(!said(s, "Hole").contains("no longer on face"), "block height {h}: {}", said(s, "Hole"));
    };
    on_axis(&s, 20.0);
    // Taller: the top face moves up. Moved to the face's nearest point the hole would slide
    // sideways onto another axis (5 mm further along x at mid-height); along its axis it enters
    // higher up the same line.
    change(&mut s, "height", "30 mm");
    on_axis(&s, 30.0);
    // Lower: the same line enters further along.
    change(&mut s, "height", "12 mm");
    on_axis(&s, 12.0);
}
