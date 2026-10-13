//! The prism fast path: `boolean_whole` asks the exact 2D prism route before it meshes both
//! operands, so a plate and the tool through it are decided without a mesh of either.
use super::*;
use crate::{BoolOp, boolean, box_solid, cylinder, measure};
use std::f64::consts::PI;

/// A cylindrical tool through a plate: the exact prism route answers the pair, and the material
/// it removes is the tool's own.
#[test]
fn a_hole_through_a_plate_removes_the_tool_volume() {
    let plate = box_solid(Vec3::ZERO, Vec3::new(40.0, 30.0, 10.0)).expect("plate");
    let tool = cylinder(Vec3::new(20.0, 15.0, -1.0), Vec3::Z, 5.0, 12.0).expect("tool");
    let cut = boolean(&plate, &tool, BoolOp::Cut).expect("cut").expect("a hole leaves material");
    let m = measure(&cut).expect("measure");
    let want = 40.0 * 30.0 * 10.0 - PI * 25.0 * 10.0;
    assert!((m.volume - want).abs() < want * 5e-3, "{} vs {want}", m.volume);
    assert!(cut.validity().is_empty(), "{:?}", cut.validity());
}

/// Boxes that stay apart: the union keeps both, whichever route decides it.
#[test]
fn bodies_that_stay_apart_join_into_one_body_with_both_lumps() {
    let a = box_solid(Vec3::ZERO, Vec3::new(10.0, 10.0, 10.0)).expect("a");
    let b = box_solid(Vec3::new(20.0, 0.0, 0.0), Vec3::new(30.0, 10.0, 10.0)).expect("b");
    let joined = boolean(&a, &b, BoolOp::Union).expect("union").expect("two boxes");
    assert!((measure(&joined).expect("measure").volume - 2000.0).abs() < 1e-6);
    assert_eq!(joined.lumps().expect("lumps").len(), 2);
}

/// A tool wholly inside its target leaves a closed void, not a through cut.
#[test]
fn a_tool_wholly_inside_leaves_a_closed_void() {
    let block = box_solid(Vec3::ZERO, Vec3::new(40.0, 40.0, 40.0)).expect("block");
    let inner = box_solid(Vec3::new(15.0, 15.0, 15.0), Vec3::new(25.0, 25.0, 25.0)).expect("inner");
    let cut = boolean(&block, &inner, BoolOp::Cut).expect("cut").expect("a void leaves material");
    assert!((measure(&cut).expect("measure").volume - 63000.0).abs() < 1e-6);
    assert!(cut.validity().is_empty(), "{:?}", cut.validity());
}

/// A tool that misses its target, in section or along the extrusion, leaves it as it is: the
/// sampled shortcut answers that by reusing the operand itself, and the 2D route must not
/// rebuild it into a look-alike solid on the way past - an untouched body in a timeline has to
/// stay the very same body.
#[test]
fn a_cut_that_changes_nothing_keeps_the_original_body() {
    let plate = box_solid(Vec3::ZERO, Vec3::new(40.0, 30.0, 10.0)).expect("plate");
    let past_the_end = box_solid(Vec3::new(200.0, 200.0, -5.0), Vec3::new(210.0, 210.0, 20.0)).expect("tool");
    assert!(
        boolean(&plate, &past_the_end, BoolOp::Cut).expect("cut").expect("material stays").same(&plate),
        "a tool clear of the plate must leave the very same body"
    );
    let inside_the_height = box_solid(Vec3::new(200.0, 200.0, 5.0), Vec3::new(210.0, 210.0, 20.0)).expect("tool");
    assert!(
        boolean(&plate, &inside_the_height, BoolOp::Cut).expect("cut").expect("material stays").same(&plate),
        "a tool that only overlaps the plate's height must leave the very same body"
    );
    let covers_it = box_solid(Vec3::new(-10.0, -10.0, -10.0), Vec3::new(50.0, 40.0, 20.0)).expect("tool");
    assert!(boolean(&plate, &covers_it, BoolOp::Cut).expect("cut").is_none(), "a tool over the whole plate removes it");
}
