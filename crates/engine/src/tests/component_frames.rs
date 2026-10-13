//! Components as frames (regression cases for user bug #4): picks, sketches and bodies on
//! moved components and pasted instances.

use std::f64::consts::PI;

use serde_json::json;

use super::{rel, run};
use crate::*;

/// Components are frames: world picks on a moved component (whichever is active) land on its
/// geometry, bodies moved into it keep their place, and sketches on its faces follow it.
#[test]
fn moved_component_picks_and_frames() {
    let mut s = Session::default();
    run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10, "body_name": "Base"}));
    let c = run(&mut s, "component.create", json!({"name": "Arm"}));
    let (cid, occ) = (c["component"].as_u64().unwrap(), c["occurrence"].as_u64().unwrap());
    run(&mut s, "solid.box", json!({"corner": [20, 0, 0], "length": 10, "width": 4, "height": 4, "body_name": "ArmBody"}));
    run(&mut s, "component.activate", json!({"component": 0}));
    run(&mut s, "occurrence.move", json!({"occurrence": occ, "translate": [0, 50, 0]}));
    let wbox = |s: &Session, b: &str| s.world_state().body(b).unwrap().mesh().bounds();
    let vol = |s: &mut Session, b: &str| run(s, "inspect.measure", json!({"bodies": [b]}))["total"]["volume_mm3"].as_f64().unwrap();
    // Root active: a fillet picked at the moved edge in the world.
    run(&mut s, "solid.fillet", json!({"edges": [[25, 54, 4]], "radius": 1}));
    assert!(rel(vol(&mut s, "ArmBody"), 160.0 - (1.0 - PI / 4.0) * 10.0) < 1e-4, "{}", vol(&mut s, "ArmBody"));
    // An explicit fixed plane on the modified body's top keeps the world pick;
    // unsupported modifier lineage cannot silently create a weak attachment.
    let sk = run(&mut s, "sketch.create", json!({"plane": {"face": [25, 52, 4],"fixed":true}}));
    run(&mut s, "sketch.circle.center", json!({"center": [0, 0], "radius": 1}));
    run(&mut s, "sketch.finish", json!({}));
    let si = run(&mut s, "sketch.inspect", json!({"sketch": sk["sketch"]}));
    let z: Vec<f64> = si["points"].as_array().unwrap().iter().map(|p| p["world"][2].as_f64().unwrap()).collect();
    assert!(z.iter().all(|z| (z - 4.0).abs() < 1e-6), "{si}");
    // Base moved into the moved component stays where it was.
    let before = wbox(&s, "Base");
    run(&mut s, "component.move_bodies", json!({"bodies": ["Base"], "component": "Arm"}));
    let after = wbox(&s, "Base");
    assert!(before.min.dist(after.min) < 1e-9 && before.max.dist(after.max) < 1e-9, "{before:?} {after:?}");
    // A pasted instance: a pick on it lands on the shared definition.
    run(&mut s, "occurrence.copy", json!({"component": cid, "translate": [0, 20, 0]}));
    let v0 = vol(&mut s, "ArmBody");
    let names: Vec<String> = s.world_state().bodies.iter().map(|b| b.name.clone()).collect();
    let inst = names.iter().find(|n| n.starts_with("ArmBody (")).cloned().unwrap();
    let ib = wbox(&s, &inst);
    // Its bottom edge along x at the far side (y max, z 0).
    run(&mut s, "solid.fillet", json!({"edges": [[25, ib.max.y, 0]], "radius": 1}));
    assert!(vol(&mut s, "ArmBody") < v0 - 1.0, "the instance's pick reached the definition");
}

/// An extrude in a moved component, made from a sketch of another component (here the root), is
/// built where the sketch is shown: the sketch is seen through both placements, as a sketch on
/// that component's face already is. Before, its plane was read as if it were in the feature's
/// own component, so the join landed turned along with the moved component.
#[test]
fn extrude_in_a_moved_component_from_a_root_sketch() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "XY"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [-40, -5], "p1": [0, 5]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "solid.extrude", json!({"distance": 10, "body_name": "Jaw"}));
    let made = run(&mut s, "component.from_bodies", json!({"bodies": ["Jaw"]}));
    let jaw = made["components"][0]["component"].as_u64().unwrap();
    // Turned half a turn about Z: the jaw now lies along +X (x 0..40, y -5..5).
    run(&mut s, "occurrence.move", json!({"occurrence": "Jaw:1", "axis": [0, 0, 1], "angle": "180 deg", "origin": [0, 0, 0]}));
    run(&mut s, "component.activate", json!({"component": 0}));
    // A root sketch: a tab on the jaw's +Y side, x 30..40, y 5..7 in the world.
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Tab"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [30, 5], "p1": [40, 7]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "component.activate", json!({"component": jaw}));
    run(&mut s, "solid.extrude", json!({"sketch": "Tab", "distance": 10, "operation": "join", "targets": ["Jaw"]}));
    let b = s.world_state().body("Jaw").unwrap().mesh().bounds();
    let near = |a: f64, e: f64| (a - e).abs() < 1e-6;
    assert!(near(b.min.x, 0.0) && near(b.max.x, 40.0) && near(b.min.y, -5.0) && near(b.max.y, 7.0), "{b:?}");
    let v = run(&mut s, "inspect.measure", json!({"bodies": ["Jaw"]}))["total"]["volume_mm3"].as_f64().unwrap();
    assert!(rel(v, 4000.0 + 200.0) < 1e-6, "{v}");
}

/// To Object in a moved component: the picked face is a world point, kept in the feature's own
/// component like its other picks, so the extrude ends at that face (here the jaw's top, 10 mm
/// above the sketch) wherever the component is placed.
#[test]
fn extrude_to_a_face_in_a_moved_component() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "XY"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [-40, -5], "p1": [0, 5]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "solid.extrude", json!({"distance": 10, "body_name": "Jaw"}));
    let made = run(&mut s, "component.from_bodies", json!({"bodies": ["Jaw"]}));
    let jaw = made["components"][0]["component"].as_u64().unwrap();
    run(&mut s, "occurrence.move", json!({"occurrence": "Jaw:1", "axis": [0, 0, 1], "angle": "180 deg", "origin": [0, 0, 0]}));
    // Lifted too, so the jaw's own frame and the world differ in height (x 0..40, z 5..15).
    run(&mut s, "occurrence.move", json!({"occurrence": "Jaw:1", "translate": [0, 0, 5]}));
    run(&mut s, "component.activate", json!({"component": 0}));
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Tab"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [30, 5], "p1": [40, 7]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "component.activate", json!({"component": jaw}));
    // Up to the jaw's top face, picked in the world at its middle: the tab rises from z 0 to 15.
    run(&mut s, "solid.extrude", json!({"sketch": "Tab", "to": [20, 0, 15], "operation": "join", "targets": ["Jaw"]}));
    let b = s.world_state().body("Jaw").unwrap().mesh().bounds();
    let near = |a: f64, e: f64| (a - e).abs() < 1e-6;
    assert!(
        near(b.min.x, 0.0) && near(b.max.x, 40.0) && near(b.min.y, -5.0) && near(b.max.y, 7.0) && near(b.min.z, 0.0) && near(b.max.z, 15.0),
        "{b:?}"
    );
    let v = run(&mut s, "inspect.measure", json!({"bodies": ["Jaw"]}))["total"]["volume_mm3"].as_f64().unwrap();
    assert!(rel(v, 4000.0 + 10.0 * 2.0 * 15.0) < 1e-6, "{v}");
}
