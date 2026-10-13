use std::f64::consts::FRAC_PI_2;

use serde_json::{Value, json};
use solidcraft_geom::Vec3;

use crate::Session;

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    match s.execute(id, &p) {
        Ok(v) => v,
        Err(e) => panic!("{id} {p}: {e}"),
    }
}

fn occ(s: &Session, component: &str) -> u64 {
    let c = s.doc.find_component(component).unwrap_or(0);
    s.doc.occurrence_of(c).map(|o| o.id).unwrap_or(0)
}

fn place(s: &Session, component: &str, p: Vec3) -> Vec3 {
    solidcraft_doc::apply_point(&solidcraft_doc::joints::occurrence_world(&s.doc, occ(s, component)), p)
}

fn near(a: Vec3, b: Vec3) -> bool {
    a.dist(b) < 1e-6
}

/// A hinge leaf (oracle 63's knuckle geometry): a 40 x 30 x 3 plate and an r4 knuckle along X
/// at y = 33, z = 4.
fn leaf(s: &mut Session, name: &str) {
    run(s, "component.activate", json!({"component": "root"}));
    run(s, "component.create", json!({"name": name}));
    run(s, "solid.box", json!({"length": 40, "width": 30, "height": 3, "corner": [0, 0, 0]}));
    run(s, "solid.cylinder", json!({"base": [0, 33, 4], "axis": [1, 0, 0], "radius": 4, "height": 40, "operation": "new"}));
}

#[test]
fn revolute_hinge_turns_about_the_knuckle() {
    let mut s = Session::default();
    leaf(&mut s, "LeafA");
    leaf(&mut s, "LeafB");
    let o = occ(&s, "LeafA");
    run(&mut s, "occurrence.ground", json!({"occurrence": o, "grounded": true}));
    let r = run(
        &mut s,
        "joint.create",
        json!({
        "type": "revolute", "a": {"occurrence": "LeafA", "circle": [1, 37, 4]}, "b": {"occurrence": "LeafB", "circle": [1, 37, 4]}, "flip": true, "name": "Hinge"}),
    );
    assert_eq!(r["conflicts"].as_array().map(Vec::len), Some(0), "{r}");
    // Same frames: B stays put at angle 0.
    assert!(near(place(&s, "LeafB", Vec3::new(20.0, 0.0, 0.0)), Vec3::new(20.0, 0.0, 0.0)));
    let l = run(&mut s, "joint.list", json!({}));
    let dof = l["dof"].as_array().unwrap().iter().find(|d| d["occurrence"] == occ(&s, "LeafB")).unwrap()["dof"].clone();
    assert_eq!(dof, 1, "{l}");
    // Drive 90°: B turns about the knuckle axis (through (0, 33, 4), along the frame's z = −X).
    run(&mut s, "joint.drive", json!({"joint": "Hinge", "value": 90}));
    let want = solidcraft_doc::rigid(Vec3::ZERO, Vec3::new(0.0, 33.0, 4.0), Vec3::new(-1.0, 0.0, 0.0), FRAC_PI_2);
    let p = Vec3::new(20.0, 0.0, 0.0);
    assert!(near(place(&s, "LeafB", p), solidcraft_doc::apply_point(&want, p)), "{:?}", place(&s, "LeafB", p));
    assert!(near(place(&s, "LeafB", p), Vec3::new(20.0, 29.0, 37.0)));
    // LeafA did not move; the knuckle axis is fixed.
    assert!(near(place(&s, "LeafA", p), p));
    assert!(near(place(&s, "LeafB", Vec3::new(10.0, 33.0, 4.0)), Vec3::new(10.0, 33.0, 4.0)));
    // Expressions drive too, and undo puts it back.
    run(&mut s, "joint.drive", json!({"joint": "Hinge", "value": "pi rad"}));
    assert!(near(place(&s, "LeafB", p), Vec3::new(20.0, 66.0, 8.0)));
    run(&mut s, "edit.undo", json!({}));
    assert!(near(place(&s, "LeafB", p), Vec3::new(20.0, 29.0, 37.0)));
}

/// Two 10 mm cubes in components A (grounded) and B.
fn two_cubes() -> Session {
    let mut s = Session::default();
    for (name, x) in [("A", 0), ("B", 20)] {
        run(&mut s, "component.activate", json!({"component": "root"}));
        run(&mut s, "component.create", json!({"name": name}));
        run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10, "corner": [x, 0, 0]}));
    }
    let o = occ(&s, "A");
    run(&mut s, "occurrence.ground", json!({"occurrence": o, "grounded": true}));
    s
}

#[test]
fn slider_limits_clamp() {
    let mut s = two_cubes();
    // B's bottom face onto A's top face (faces meet), sliding along their common normal.
    let r = run(
        &mut s,
        "joint.create",
        json!({"type": "slider", "a": {"face": [5, 5, 10]}, "b": {"face": [25, 5, 0]}, "limits": [[0, 10]], "name": "Rail"}),
    );
    assert_eq!(r["conflicts"].as_array().map(Vec::len), Some(0), "{r}");
    // B now sits on A.
    assert!(near(place(&s, "B", Vec3::new(25.0, 5.0, 0.0)), Vec3::new(5.0, 5.0, 10.0)), "{:?}", place(&s, "B", Vec3::new(25.0, 5.0, 0.0)));
    let r = run(&mut s, "joint.drive", json!({"joint": "Rail", "value": 25}));
    assert_eq!(r["values"], json!([10.0]), "{r}");
    assert!(near(place(&s, "B", Vec3::new(25.0, 5.0, 0.0)), Vec3::new(5.0, 5.0, 20.0)));
    run(&mut s, "joint.drive", json!({"joint": "Rail", "value": -5}));
    assert!(near(place(&s, "B", Vec3::new(25.0, 5.0, 0.0)), Vec3::new(5.0, 5.0, 10.0)));
    // Changing the limits clamps the current value.
    run(&mut s, "joint.drive", json!({"joint": "Rail", "value": 8}));
    run(&mut s, "joint.limits", json!({"joint": "Rail", "min": 0, "max": 4}));
    assert!(near(place(&s, "B", Vec3::new(25.0, 5.0, 0.0)), Vec3::new(5.0, 5.0, 14.0)));
    // Joints follow edits: A gets taller, B rides on its new top.
    let h = s
        .doc
        .features
        .iter()
        .find(|f| matches!(f.kind, solidcraft_doc::FeatureKind::Box { .. }))
        .and_then(|f| f.param_names.get(2).cloned())
        .unwrap_or_default();
    run(&mut s, "parameters.change", json!({"name": h, "expression": "15"}));
    assert!(near(place(&s, "B", Vec3::new(25.0, 5.0, 0.0)), Vec3::new(5.0, 5.0, 19.0)), "{:?}", place(&s, "B", Vec3::new(25.0, 5.0, 0.0)));
}

#[test]
fn rigid_cycles_solve_and_conflicts_report() {
    let mut s = two_cubes();
    run(&mut s, "component.activate", json!({"component": "root"}));
    run(&mut s, "component.create", json!({"name": "C"}));
    run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10, "corner": [40, 0, 0]}));
    run(&mut s, "component.activate", json!({"component": "root"}));
    // A rigid group and a closing as-built joint: a consistent loop.
    run(&mut s, "joint.rigid_group", json!({"occurrences": ["A", "B", "C"]}));
    let r = run(&mut s, "joint.as_built", json!({"type": "rigid", "a": "B", "b": "C"}));
    assert_eq!(r["conflicts"].as_array().map(Vec::len), Some(0), "{r}");
    assert!(near(place(&s, "C", Vec3::new(40.0, 0.0, 0.0)), Vec3::new(40.0, 0.0, 0.0)));
    let l = run(&mut s, "joint.list", json!({}));
    assert!(l["dof"].as_array().unwrap().iter().all(|d| d["dof"] == 0), "{l}");
    // A joint that wants C elsewhere conflicts and says so.
    let r = run(
        &mut s,
        "joint.create",
        json!({"type": "rigid", "a": {"occurrence": "A", "point": [0, 0, 0]}, "b": {"occurrence": "C", "point": [40, 0, 0]}, "name": "Wrong"}),
    );
    let c = r["conflicts"].as_array().cloned().unwrap_or_default();
    assert_eq!(c.len(), 1, "{r}");
    assert!(c[0]["why"].as_str().unwrap_or("").contains("off by"), "{r}");
    // Suppressing it clears the conflict.
    let r = run(&mut s, "joint.edit", json!({"joint": "Wrong", "suppressed": true}));
    assert_eq!(r["conflicts"].as_array().map(Vec::len), Some(0));
    assert!(
        s.execute(
            "joint.create",
            &json!({"type": "rigid", "a": {"occurrence": "A", "point": [0, 0, 0]}, "b": {"occurrence": "A", "point": [1, 0, 0]}})
        )
        .is_err()
    );
    assert!(s.execute("joint.create", &json!({"type": "hinge"})).is_err());
}

/// A 30 x 30 x 3 plate with a through slot whose x = 20 wall carries one rib (0.3 tall, 0.8
/// wide), and a peg in the slot with 0.15 clearance all round.
fn ribbed_slot_and_peg(s: &mut Session, peg_x1: f64) {
    run(s, "sketch.create", json!({"plane": "XY"}));
    run(s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [30, 30]}));
    run(s, "sketch.finish", json!({}));
    run(s, "solid.extrude", json!({"distance": 3, "body_name": "Plate"}));
    run(s, "sketch.create", json!({"plane": "XY", "offset": -1}));
    let slot = json!([[10, 5], [20, 5], [20, 14.6], [19.7, 14.6], [19.7, 15.4], [20, 15.4], [20, 25], [10, 25]]);
    run(s, "sketch.line", json!({"points": slot, "closed": true}));
    run(s, "sketch.finish", json!({}));
    run(s, "solid.extrude", json!({"distance": 5, "operation": "cut", "targets": ["Plate"]}));
    run(s, "sketch.create", json!({"plane": "XY"}));
    run(s, "sketch.rectangle.two_point", json!({"p0": [10.15, 5.15], "p1": [peg_x1, 24.85]}));
    run(s, "sketch.finish", json!({}));
    run(s, "solid.extrude", json!({"distance": 3, "operation": "new", "body_name": "Peg"}));
}

/// The peg overlaps only the rib: 0.15 x 0.8 x 3 = 0.36 mm³ (#442).
#[test]
fn interference_finds_a_peg_overlapping_a_small_rib() {
    let mut s = Session::default();
    ribbed_slot_and_peg(&mut s, 19.85);
    let i = run(&mut s, "inspect.interference", json!({}));
    assert_eq!(i["count"], 1, "{i}");
    assert!((i["interferences"][0]["volume_mm3"].as_f64().unwrap_or(0.0) - 0.36).abs() < 1e-6, "{i}");
    let (lo, hi) = (&i["interferences"][0]["bbox"]["min"], &i["interferences"][0]["bbox"]["max"]);
    assert!((lo[0].as_f64().unwrap_or(0.0) - 19.7).abs() < 1e-6 && (hi[0].as_f64().unwrap_or(0.0) - 19.85).abs() < 1e-6, "{i}");
}

/// An arc revolved up to a post ends on the post's wall, where the two bodies can't be
/// intersected: the pair is listed as unchecked instead of passing as clear.
#[test]
fn interference_lists_a_pair_it_could_not_check() {
    let mut s = Session::default();
    run(&mut s, "solid.cylinder", json!({"base": [0, 14, -5], "radius": 4, "height": 15, "body_name": "Post"}));
    run(&mut s, "sketch.create", json!({"name": "Profile", "plane": "XZ"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [10, 0], "p1": [15, 5]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "solid.revolve", json!({"sketch": "Profile", "axis": {"origin": [0, 0, 0], "dir": [0, 0, 1]}, "to": [4, 14, 2], "body_name": "Arc"}));
    let i = run(&mut s, "inspect.interference", json!({}));
    assert_eq!(i["count"], 0, "{i}");
    let unchecked = i["unchecked"].as_array().cloned().unwrap_or_default();
    assert_eq!(unchecked.len(), 1, "{i}");
    assert!(unchecked[0]["reason"].as_str().is_some_and(|r| r.starts_with("not verified") && !r.contains("displacement")), "{i}");
    // Bodies that are clear of each other, or overlap plainly, leave the list empty.
    let mut s = two_cubes();
    let i = run(&mut s, "inspect.interference", json!({}));
    assert_eq!(i["unchecked"].as_array().map(Vec::len), Some(0), "{i}");
}

#[test]
fn motion_links_and_interference() {
    let mut s = two_cubes();
    run(&mut s, "joint.create", json!({"type": "cylindrical", "a": {"face": [5, 5, 10]}, "b": {"face": [25, 5, 0]}, "name": "Screw"}));
    run(&mut s, "joint.drive", json!({"joint": "Screw", "values": [0, 0]}));
    // No overlap while B sits on A; then push B 4 mm into A.
    let i = run(&mut s, "inspect.interference", json!({}));
    assert_eq!(i["count"], 0, "{i}");
    run(&mut s, "joint.drive", json!({"joint": "Screw", "values": [0, -4]}));
    let i = run(&mut s, "inspect.interference", json!({}));
    assert_eq!(i["count"], 1, "{i}");
    assert!((i["interferences"][0]["volume_mm3"].as_f64().unwrap_or(0.0) - 400.0).abs() < 1e-3, "{i}");
    // A motion link: a second joint's value follows.
    run(&mut s, "component.activate", json!({"component": "root"}));
    run(&mut s, "component.create", json!({"name": "D"}));
    run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10, "corner": [60, 0, 0]}));
    run(
        &mut s,
        "joint.create",
        json!({"type": "slider", "a": {"occurrence": "A", "face": [5, 5, 10]}, "b": {"occurrence": "D", "face": [65, 5, 0]}, "name": "Follower"}),
    );
    run(&mut s, "joint.motion_link", json!({"a": "Screw", "ia": 1, "b": "Follower", "ratio": 2}));
    run(&mut s, "joint.drive", json!({"joint": "Screw", "values": [0, 3]}));
    assert!(near(place(&s, "D", Vec3::new(65.0, 5.0, 0.0)), Vec3::new(5.0, 5.0, 16.0)), "{:?}", place(&s, "D", Vec3::new(65.0, 5.0, 0.0)));
    // Saved and reopened, joints come back.
    let back = solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap();
    assert_eq!(back.assembly, s.doc.assembly);
    let mut t = Session::new(back);
    let l = run(&mut t, "joint.list", json!({}));
    assert_eq!(l["joints"].as_array().map(Vec::len), Some(2));
}

/// Two faces joined mate: B's bottom face lands on A's top face, B on top of A (not inside
/// it); Flip turns B over instead.
#[test]
fn faces_mate_by_default() {
    let mut s = two_cubes();
    run(&mut s, "joint.create", json!({"type": "rigid", "a": {"face": [5, 5, 10]}, "b": {"face": [25, 5, 0]}, "name": "Stack"}));
    // B's bottom centre (25, 5, 0) at A's top centre, B above it.
    assert!(near(place(&s, "B", Vec3::new(25.0, 5.0, 0.0)), Vec3::new(5.0, 5.0, 10.0)));
    assert!(near(place(&s, "B", Vec3::new(25.0, 5.0, 10.0)), Vec3::new(5.0, 5.0, 20.0)), "{:?}", place(&s, "B", Vec3::new(25.0, 5.0, 10.0)));
    run(&mut s, "joint.edit", json!({"joint": "Stack", "flip": true}));
    assert!(near(place(&s, "B", Vec3::new(25.0, 5.0, 10.0)), Vec3::new(5.0, 5.0, 0.0)), "{:?}", place(&s, "B", Vec3::new(25.0, 5.0, 10.0)));
}

/// #527: an assembly with its fasteners passes 200 bodies quickly; the clash check must still
/// cover the whole design. 201 separate 1 mm cubes 2 mm apart, plus one cube pushed half-way
/// into the first: exactly that pair overlaps, by 0.5 mm³.
#[test]
fn interference_checks_designs_with_more_than_200_bodies() {
    let mut s = Session::default();
    for i in 0..201 {
        run(&mut s, "solid.box", json!({"length": 1, "width": 1, "height": 1, "corner": [2 * i, 0, 0], "operation": "new"}));
    }
    run(&mut s, "solid.box", json!({"length": 1, "width": 1, "height": 1, "corner": [0.5, 0, 0], "operation": "new", "body_name": "Clash"}));
    assert_eq!(s.model.state().bodies.len(), 202);
    let i = run(&mut s, "inspect.interference", json!({}));
    assert_eq!(i["checked"], 202, "{i}");
    assert_eq!(i["count"], 1, "{i}");
    let hit = &i["interferences"][0];
    let mut pair = [hit["a"].as_str().unwrap_or(""), hit["b"].as_str().unwrap_or("")];
    pair.sort_unstable();
    assert_eq!(pair, ["Body1", "Clash"], "{i}");
    assert!((hit["volume_mm3"].as_f64().unwrap_or(0.0) - 0.5).abs() < 1e-6, "{i}");
}

/// The bounding-box sweep behind `inspect.interference` finds exactly the pairs a check of every
/// pair finds (touching boxes count), and reports how many it left out past its limit.
#[test]
fn interference_box_sweep_matches_every_pair_check() {
    use solidcraft_geom::Aabb3;
    let cube = |x: f64, y: f64, z: f64, e: f64| Aabb3 { min: Vec3::new(x, y, z), max: Vec3::new(x + e, y + e, z + e) };
    // A deterministic scatter: overlapping, touching, and apart in only one axis.
    let boxes: Vec<Aabb3> = (0..60)
        .map(f64::from)
        .map(|i| cube((i * 7.3) % 20.0, (i * 3.1) % 9.0, (i * 5.7) % 4.0, 1.0 + (i % 3.0)))
        .chain([cube(30.0, 0.0, 0.0, 1.0), cube(31.0, 0.0, 0.0, 1.0)])
        .collect();
    let mut every = Vec::new();
    for (i, a) in boxes.iter().enumerate() {
        for (j, b) in boxes.iter().enumerate().skip(i + 1) {
            let apart = a.max.x < b.min.x || b.max.x < a.min.x || a.max.y < b.min.y || b.max.y < a.min.y || a.max.z < b.min.z || b.max.z < a.min.z;
            if !apart {
                every.push((i, j));
            }
        }
    }
    assert!(every.contains(&(60, 61)), "touching boxes are a candidate pair");
    let (pairs, skipped) = super::overlapping_pairs(&boxes, usize::MAX);
    assert_eq!((pairs, skipped), (every.clone(), 0));
    let (pairs, skipped) = super::overlapping_pairs(&boxes, 10);
    assert_eq!((pairs.len(), skipped), (10, every.len() - 10));
}
