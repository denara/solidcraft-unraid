use super::*;

fn run(s: &mut Session, cmd: &str, params: Value) -> Value {
    s.execute(cmd, &params).unwrap()
}

fn setup() -> (Session, String, String) {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane":"XY"}));
    let result = run(&mut s, "sketch.text", json!({"text":"II","at":[3,4],"height":10,"angle":15}));
    (s, result["link"].as_str().unwrap().into(), result["curves"][0].as_str().unwrap().into())
}

#[test]
fn text_move_updates_the_source_once_preserves_ids_and_roundtrips() {
    let (mut s, link, curve) = setup();
    let before = s.doc.to_json();
    let ids: Vec<_> = s.doc.sketch(1).unwrap().curves.iter().map(|c| c.id.clone()).collect();
    let undo = s.undo.len();
    run(&mut s, "sketch.move", json!({"entities":[curve,ids[1]],"translate":[10,-2],"center":[3,4],"angle":30}));
    let sk = s.doc.sketch(1).unwrap();
    assert_eq!(sk.curves.iter().map(|c| c.id.clone()).collect::<Vec<_>>(), ids);
    let LinkSource::Text { at, angle, text, height, font } = &sk.link(&link).unwrap().source else { panic!() };
    assert!(at.dist(Vec2::new(13.0, 2.0)) < 1e-9);
    assert!((*angle - 45_f64.to_radians()).abs() < 1e-12);
    assert_eq!((text.as_str(), *height, font), ("II", 10.0, &None));
    assert_eq!(s.undo.len(), undo + 1);
    let moved = s.doc.to_json();
    let reopened = solidcraft_doc::Document::from_json(&moved).unwrap();
    assert_eq!(reopened.to_json(), moved);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc.to_json(), before);
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(s.doc.to_json(), moved);
}

#[test]
fn text_copy_is_a_new_editable_source_and_original_is_unchanged() {
    let (mut s, link, curve) = setup();
    let original = s.doc.sketch(1).unwrap().link(&link).unwrap().source.clone();
    let result = run(&mut s, "sketch.move", json!({"entities":[curve],"translate":[20,0],"copy":true}));
    let copied_curve = result["curves"][0].as_str().unwrap();
    let new_link = text_link(s.doc.sketch(1).unwrap(), copied_curve).unwrap().to_owned();
    assert_ne!(new_link, link);
    run(&mut s, "sketch.edit_text", json!({"link":new_link,"text":"O"}));
    assert_eq!(s.doc.sketch(1).unwrap().link(&link).unwrap().source, original);
}

#[test]
fn text_drag_moves_all_letters_and_rebuilds_downstream_on_release() {
    let (mut s, link, curve) = setup();
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "solid.extrude", json!({"distance":3}));
    run(&mut s, "sketch.edit", json!({"sketch":1}));
    let sk = s.doc.sketch(1).unwrap();
    let before = sk.link_points(&link).into_iter().map(|i| (i, sk.point(i).unwrap())).collect::<Vec<_>>();
    for frame in 0..3 {
        let result = run(&mut s, "sketch.drag", json!({"entity":curve,"from":[frame,0],"to":[frame+1,1],"live":true}));
        assert_eq!(result["reached"], 1.0);
        assert!(s.model.is_stale());
    }
    run(&mut s, "sketch.drag_end", json!({}));
    assert!(!s.model.is_stale());
    assert!(s.model.result(2).is_some_and(|r| r.error.is_none()));
    let sk = s.doc.sketch(1).unwrap();
    for (i, old) in before {
        assert!(sk.point(i).unwrap().dist(old + Vec2::new(3.0, 3.0)) < 1e-8);
    }
    assert!(matches!(sk.link(&link).unwrap().source, LinkSource::Text { .. }));
}

#[test]
fn text_move_in_a_rotated_component_preserves_font_and_local_coordinates() {
    let mut s = Session::default();
    let component = run(&mut s, "component.create", json!({"name":"Label"}));
    run(&mut s, "occurrence.move", json!({"occurrence":component["occurrence"],"translate":[30,20,10],"axis":[0,0,1],"angle":90}));
    let created = run(&mut s, "sketch.create", json!({"plane":"XY"}));
    let sid = created["sketch"].as_u64().unwrap();
    let result = run(&mut s, "sketch.text", json!({"text":"I","at":[0,0],"height":10,"font":solidcraft_sketch::DEFAULT_FONT_FAMILY}));
    let link = result["link"].as_str().unwrap();
    let before = s.world_state().sketch(sid).unwrap().plane;
    run(&mut s, "sketch.move", json!({"entities":[result["curves"][0]],"translate":[5,2]}));
    let sk = s.doc.sketch(sid).unwrap();
    let LinkSource::Text { at, font, .. } = &sk.link(link).unwrap().source else { panic!() };
    assert!(at.dist(Vec2::new(5.0, 2.0)) < 1e-9);
    assert_eq!(font.as_deref(), Some(solidcraft_sketch::DEFAULT_FONT_FAMILY));
    let state = s.world_state();
    assert!(state.sketch(sid).unwrap().plane.to_world(*at).dist(before.to_world(Vec2::new(5.0, 2.0))) < 1e-9);
    let reopened = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
    assert_eq!(reopened.doc.sketch(sid).unwrap().link(link).unwrap().source, sk.link(link).unwrap().source);
}

#[test]
fn text_move_rejects_malformed_values_without_changing_history() {
    let (mut s, _, curve) = setup();
    let before = s.doc.to_json();
    let undo = s.undo.len();
    for params in [
        json!({"entities":[curve],"translate":[1]}),
        json!({"entities":[curve],"angle":"invalid"}),
        json!({"entity":curve,"from":[0,0],"to":["invalid",2]}),
    ] {
        let cmd = if params.get("entity").is_some() { "sketch.drag" } else { "sketch.move" };
        assert!(s.execute(cmd, &params).is_err());
        assert_eq!(s.doc.to_json(), before);
        assert_eq!(s.undo.len(), undo);
    }
}
