//! `solid.move` moves bodies, so it has nothing to offer the geometry under the cursor while a
//! sketch is being edited. It is disabled there, with a reason, and comes back once the sketch
//! is finished.
use super::*;

fn cube(s: &mut Session) {
    s.execute("solid.box", &json!({"length": 20, "width": 20, "height": 20})).unwrap();
}

fn spec() -> &'static crate::cmd::CommandSpec {
    crate::find_command("solid.move").unwrap()
}

#[test]
fn move_is_disabled_while_a_sketch_is_being_edited_and_says_why() {
    let mut s = Session::default();
    cube(&mut s);
    assert!(spec().info(&s).enabled, "Move is enabled with no sketch open");

    s.execute("sketch.create", &json!({"plane": "XY"})).unwrap();
    let info = spec().info(&s);
    assert!(!info.enabled, "Move must be disabled while a sketch is being edited");
    let why = info.disabled_reason.unwrap_or_default();
    assert!(why.contains("sketch is being edited"), "the reason must name the sketch: {why}");
    assert!(why.contains("bodies"), "the reason must say what Move acts on: {why}");
    assert!(why.contains("Finish Sketch"), "the reason must say the way out: {why}");
}

#[test]
fn running_move_in_a_sketch_is_refused_and_changes_nothing() {
    let mut s = Session::default();
    cube(&mut s);
    s.execute("sketch.create", &json!({"plane": "XY"})).unwrap();
    let features = s.doc.features.len();

    let e = s.execute("solid.move", &json!({"bodies": ["Body1"], "translate": [30, 0, 0]})).unwrap_err();
    assert!(matches!(e, EngineError::Disabled(ref id, _) if id == "solid.move"), "{e:?}");

    assert_eq!(s.doc.features.len(), features, "a refused command adds no feature");
    assert!(s.active_sketch.is_some(), "the sketch is still being edited");
    let bounds = s.model.state().body("Body1").unwrap().mesh().bounds();
    assert!(bounds.min.dist(Vec3::ZERO) < 1e-6, "the body did not move: {bounds:?}");
}

#[test]
fn move_works_again_once_the_sketch_is_finished() {
    let mut s = Session::default();
    cube(&mut s);
    s.execute("sketch.create", &json!({"plane": "XY"})).unwrap();
    s.execute("sketch.finish", &json!({})).unwrap();
    assert!(spec().info(&s).enabled, "Move comes back once the sketch is finished");

    s.execute("solid.move", &json!({"bodies": ["Body1"], "translate": [30, 0, 0]})).unwrap();
    let bounds = s.model.state().body("Body1").unwrap().mesh().bounds();
    assert!(bounds.min.dist(Vec3::new(30.0, 0.0, 0.0)) < 1e-6, "{bounds:?}");
}
