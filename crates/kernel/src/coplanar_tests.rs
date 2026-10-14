//! Booleans of bodies with faces in a common plane: a face is pushed out of the plane only where
//! that leaves the result as it is. Every expected volume is what the two bodies define.
#![allow(clippy::unwrap_used, clippy::panic)]
use crate::*;
use solidcraft_geom::Vec3;

/// An l x w x h block with two bottom edges chamfered by 1 mm: no prism along any axis, so
/// its booleans are not done on sections.
fn block(l: f64, w: f64, h: f64) -> Body {
    let b = box_solid(Vec3::ZERO, Vec3::new(l, w, h)).unwrap();
    let b = chamfer(&b, &[Vec3::new(l / 2.0, w, 0.0)], 1.0).unwrap();
    chamfer(&b, &[Vec3::new(l, w / 2.0, 0.0)], 1.0).unwrap()
}

/// A 200 x 100 x 20 block with a rib across it: 20 wide over the block's 100, from the top
/// face z = 20 up to z = 30.
fn ribbed() -> Body {
    let rib = box_solid(Vec3::new(90.0, 0.0, 20.0), Vec3::new(110.0, 100.0, 30.0)).unwrap();
    boolean(&block(200.0, 100.0, 20.0), &rib, BoolOp::Union).unwrap().unwrap()
}

/// A channel along that block, longer than its 200: y 30 to 70 and z 10 to 20, so its top
/// lies in the plane of the face the rib stands on.
fn channel() -> Body {
    box_solid(Vec3::new(-20.0, 30.0, 10.0), Vec3::new(220.0, 70.0, 20.0)).unwrap()
}

/// What the boolean of `a` and `b` adds to a's volume, the result's lumps, and how it was made.
fn change(a: &Body, b: &Body, op: BoolOp) -> (f64, usize, BooleanDiagnostics) {
    let (r, how) = capture_boolean_diagnostics(|| boolean(a, b, op));
    let r = r.unwrap().unwrap();
    (measure(&r).unwrap().volume - measure(a).unwrap().volume, r.lumps().unwrap().len(), how)
}

fn used(how: &BooleanDiagnostics, strategy: BooleanStrategy) -> bool {
    how.events.iter().any(|e| e.strategy == strategy)
}

/// A channel cut along a block, up to the face a rib stands on. The channel's top lies in
/// that face's plane and the rib bridges it: pushed up out of the plane (2.29 % of the
/// body's size, 5.6 mm), the top would cut a notch 20 x 40 into the rib.
#[test]
fn a_cut_up_to_a_face_leaves_what_stands_on_the_face() {
    let (added, lumps, how) = change(&ribbed(), &channel(), BoolOp::Cut);
    // It takes 200 x 40 x 10 of the block and nothing of the rib.
    assert!((added + 80_000.0).abs() < 1e-6 && lumps == 1, "{added}, {lumps} lumps, {how:?}");
    assert!(!used(&how, BooleanStrategy::CoplanarSeparation), "{how:?}");
}

/// The same with a round hole through the block beside the channel (r 4 at 30, 15: y 11 to
/// 19, the channel from 30 on). The body no longer has planar faces only, but the hole lies
/// clear of the tool, and the cut stays what the channel is.
#[test]
fn a_round_hole_beside_the_cut_changes_nothing() {
    let hole = cylinder(Vec3::new(30.0, 15.0, -1.0), Vec3::Z, 4.0, 22.0).unwrap();
    let drilled = boolean(&ribbed(), &hole, BoolOp::Cut).unwrap().unwrap();
    let (added, lumps, how) = change(&drilled, &channel(), BoolOp::Cut);
    // 200 x 40 x 10 again (the hole is measured on its mesh, before and after).
    assert!((added + 80_000.0).abs() < 1e-3 && lumps == 1, "{added}, {lumps} lumps, {how:?}");
    assert!(!used(&how, BooleanStrategy::CoplanarSeparation), "{how:?}");
}

/// With nothing in their way the faces are pushed as before: a pocket's top out of the plane
/// of the block's top face, a pad's foot into the block. (This holds without the check too:
/// it keeps the check from refusing what is harmless.)
#[test]
fn faces_with_room_are_pushed_apart() {
    let block = block(200.0, 100.0, 20.0);
    // A pocket 50 x 60 x 5 from the top face: 15 000 less.
    let pocket = box_solid(Vec3::new(75.0, 20.0, 15.0), Vec3::new(125.0, 80.0, 20.0)).unwrap();
    let (added, _, how) = change(&block, &pocket, BoolOp::Cut);
    assert!((added + 15_000.0).abs() < 1e-6 && used(&how, BooleanStrategy::CoplanarSeparation), "{added}, {how:?}");
    // A pad 50 x 60 x 10 on the top face: 30 000 more.
    let pad = box_solid(Vec3::new(75.0, 20.0, 20.0), Vec3::new(125.0, 80.0, 30.0)).unwrap();
    let (added, lumps, how) = change(&block, &pad, BoolOp::Union);
    assert!((added - 30_000.0).abs() < 1e-6 && lumps == 1 && used(&how, BooleanStrategy::CoplanarSeparation), "{added}, {how:?}");
}
