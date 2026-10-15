use super::*;

fn point(sk: &solidcraft_sketch::Sketch, id: &str) -> Vec2 {
    sk.point(sk.resolve_point(id).unwrap()).unwrap()
}

fn check_relations(s: &Session) {
    let sk = sketch(s);
    for (i, constraint) in sk.constraints.iter().enumerate() {
        assert!(solidcraft_sketch::constraint_residual(&sk, i).is_some_and(|r| r < 1e-6), "{} {:?}", constraint.id, constraint.kind);
    }
}

#[test]
fn circular_pattern_keeps_seed_copy_and_center_related_during_drags() {
    let mut s = new_sketch();
    run(&mut s, "sketch.line", json!({"points":[[10,0],[20,0]],"ids":["l1"]}));
    let out = run(&mut s, "sketch.pattern.circular", json!({"entities":["l1"],"center":"origin","count":4}));
    assert_eq!(out["sketch"]["dof"], 4);
    assert_eq!(ids(&out["created"]["constraints"]).len(), 6);
    run(&mut s, "sketch.drag", json!({"entity":"l2.start","from":[0,10],"to":[5,13]}));
    check_relations(&s);
    let sk = sketch(&s);
    assert!(point(&sk, "l1.start").dist(Vec2::new(13.0, -5.0)) < 1e-6);
    assert!(point(&sk, "l3.start").dist(Vec2::new(-13.0, 5.0)) < 1e-6);
    run(&mut s, "sketch.drag", json!({"entity":"l1.end","from":[20,0],"to":[24,3]}));
    check_relations(&s);
    assert!(point(&sketch(&s), "l2.end").dist(Vec2::new(-3.0, 24.0)) < 1e-6);
}

#[test]
fn constrained_seed_prevents_independent_copy_motion() {
    for (cmd, params) in [
        ("sketch.pattern.circular", json!({"entities":["l1"],"center":"origin","count":4})),
        ("sketch.pattern.rectangular", json!({"entities":["l1"],"count":4,"spacing":10})),
    ] {
        let mut s = new_sketch();
        run(&mut s, "sketch.line", json!({"points":[[10,0],[20,0]],"ids":["l1"]}));
        run(&mut s, "sketch.constraint.fix", json!({"entities":["l1"]}));
        let out = run(&mut s, cmd, params);
        assert_eq!(out["sketch"]["dof"], 0);
        let before = point(&sketch(&s), "l2.start");
        run(&mut s, "sketch.drag", json!({"entity":"l2.start","from":before,"to":[7,17]}));
        assert!(point(&sketch(&s), "l2.start").dist(before) < 1e-6);
        check_relations(&s);
    }
}

#[test]
fn circular_center_reference_follows_a_moved_circle_and_empty_space_center_is_constrainable() {
    let mut s = new_sketch();
    run(&mut s, "sketch.circle.center", json!({"center":[3,4],"radius":2,"id":"c1"}));
    run(&mut s, "sketch.line", json!({"points":[[13,4],[23,4]],"ids":["l1"]}));
    run(&mut s, "sketch.pattern.circular", json!({"entities":["l1"],"center":"c1.center","count":3,"angle":-180}));
    run(&mut s, "sketch.drag", json!({"entity":"c1.center","from":[3,4],"to":[8,9]}));
    check_relations(&s);
    let center = point(&sketch(&s), "c1.center");
    assert!(center.dist(Vec2::new(8.0, 9.0)) < 1e-6);
    let out = run(&mut s, "sketch.pattern.circular", json!({"entities":["l1"],"center":[8,9],"count":2}));
    let center_id = out["center"].as_str().unwrap();
    run(&mut s, "sketch.constraint.coincident", json!({"a":center_id,"b":"c1.center"}));
    check_relations(&s);
}

#[test]
fn rectangular_pattern_preserves_signed_two_axis_offsets_and_circle_radii() {
    let mut s = new_sketch();
    run(&mut s, "sketch.circle.center", json!({"center":[10,5],"radius":2,"id":"c1"}));
    let out = run(&mut s, "sketch.pattern.rectangular", json!({"entities":["c1"],"dir":[0,1],"count":3,"spacing":-8,"count2":2,"spacing2":6}));
    assert_eq!(out["sketch"]["dof"], 3);
    run(&mut s, "sketch.drag", json!({"entity":"c1.center","from":[10,5],"to":[12,7]}));
    run(&mut s, "sketch.drag", json!({"entity":"c1","from":[14,7],"to":[16,7]}));
    check_relations(&s);
    let sk = sketch(&s);
    for id in ids(&out["curves"]) {
        assert!(close(sk.radius(sk.curve_index(&id).unwrap()).unwrap(), 4.0));
    }
    assert!(point(&sk, "c2.center").dist(Vec2::new(6.0, 7.0)) < 1e-6);
    assert!(point(&sk, "c6.center").dist(Vec2::new(6.0, -9.0)) < 1e-6);
}

#[test]
fn pattern_edits_retain_surviving_ids_and_work_after_undo_and_reopen() {
    for circular in [false, true] {
        let mut s = new_sketch();
        run(&mut s, "sketch.line", json!({"points":[[10,0],[20,0]],"ids":["l1"]}));
        let cmd = if circular { "sketch.pattern.circular" } else { "sketch.pattern.rectangular" };
        let params =
            if circular { json!({"entities":["l1"],"center":"origin","count":4}) } else { json!({"entities":["l1"],"count":4,"spacing":10}) };
        let original = run(&mut s, cmd, params);
        let pattern = original["pattern"].as_str().unwrap();
        let before = s.doc.to_json();
        let shrunk = run(&mut s, cmd, json!({"pattern":pattern,"count":2}));
        assert_eq!(ids(&shrunk["curves"]), vec!["l2"]);
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc.to_json(), before);
        run(&mut s, "edit.redo", json!({}));
        let mut reopened =
            Session { doc: solidcraft_doc::format::read(&s.doc.to_json()).unwrap().into(), active_sketch: s.active_sketch, ..Default::default() };
        reopened.refresh();
        let grown = run(&mut reopened, cmd, json!({"pattern":pattern,"count":5,"spacing":12,"angle":-180}));
        assert_eq!(ids(&grown["curves"])[0], "l2");
        assert_eq!(ids(&grown["curves"]).len(), 4);
        assert!(!ids(&grown["curves"]).contains(&"l3".to_string()));
        check_relations(&reopened);
        let sk = sketch(&reopened);
        let expected = if circular { Vec2::new(10.0 / 2.0_f64.sqrt(), -10.0 / 2.0_f64.sqrt()) } else { Vec2::new(22.0, 0.0) };
        assert!(point(&sk, "l2.start").dist(expected) < 1e-6);
    }
}

#[test]
fn malformed_pattern_edits_roll_back_and_deleted_seed_does_not_panic() {
    let mut s = new_sketch();
    run(&mut s, "sketch.line", json!({"points":[[10,0],[20,0]],"ids":["l1"]}));
    let out = run(&mut s, "sketch.pattern.rectangular", json!({"entities":["l1","l1"],"count":3,"spacing":10}));
    assert_eq!(ids(&out["curves"]).len(), 2);
    let before = s.doc.to_json();
    for params in [json!({"count":2.5}), json!({"count":501}), json!({"count2":500}), json!({"dir":[0,0]}), json!({"spacing":1e308})] {
        let mut params = params;
        params["pattern"] = out["pattern"].clone();
        assert!(s.execute("sketch.pattern.rectangular", &params).is_err());
        assert_eq!(s.doc.to_json(), before);
    }
    run(&mut s, "sketch.delete", json!({"entities":["l1"]}));
    assert!(s.execute("sketch.pattern.rectangular", &json!({"pattern":out["pattern"],"count":4})).is_err());
}

#[test]
fn pattern_seed_move_and_scale_preserve_shape_and_shared_points() {
    let mut s = new_sketch();
    run(&mut s, "sketch.line", json!({"points":[[10,0],[20,0],[20,10]],"ids":["l1","l2"]}));
    let out = run(&mut s, "sketch.pattern.circular", json!({"entities":["l1","l2"],"center":"origin","count":3}));
    let sk = sketch(&s);
    assert_eq!(sk.resolve_point("l3.end"), sk.resolve_point("l4.start"));
    assert_eq!(ids(&out["created"]["constraints"]).len(), 6);
    run(&mut s, "sketch.move", json!({"entities":["l1","l2"],"translate":[5,3]}));
    assert!(point(&sketch(&s), "l1.start").dist(Vec2::new(15.0, 3.0)) < 1e-6);
    check_relations(&s);
    run(&mut s, "sketch.scale", json!({"entities":["l1","l2"],"base":"origin","factor":2}));
    assert!(point(&sketch(&s), "l1.start").dist(Vec2::new(30.0, 6.0)) < 1e-6);
    check_relations(&s);
}

#[test]
fn malformed_saved_pattern_settings_and_relations_are_refused() {
    let mut s = new_sketch();
    run(&mut s, "sketch.line", json!({"points":[[10,0],[20,0]],"ids":["l1"]}));
    run(&mut s, "sketch.pattern.circular", json!({"entities":["l1"],"center":"origin","count":3}));
    let saved: Value = serde_json::from_str(&s.doc.to_json()).unwrap();
    for key in ["count", "index", "angle", "relation", "center"] {
        let mut value = saved.clone();
        let sk = &mut value["features"][0]["sketch"];
        match key {
            "count" => sk["patterns"][0]["kind"]["count"] = json!(usize::MAX),
            "index" => sk["patterns"][0]["instances"][0]["index"] = json!([usize::MAX, 0]),
            "angle" => sk["patterns"][0]["kind"]["angle"] = json!(1e308),
            "center" => sk["patterns"][0]["kind"]["center"] = json!("missing"),
            _ => sk["constraints"][0]["offset"] = json!([1e308, 0]),
        }
        assert!(solidcraft_doc::format::read(&value.to_string()).is_err(), "{key}");
    }
    let mut legacy = saved;
    legacy["format"] = json!("solidcraft/19");
    let sk = legacy["features"][0]["sketch"].as_object_mut().unwrap();
    sk.remove("patterns");
    sk.insert("constraints".into(), json!([]));
    let read = solidcraft_doc::format::read(&legacy.to_string()).unwrap();
    assert!(read.sketch(s.active_sketch.unwrap()).unwrap().patterns.is_empty());
}

#[test]
fn patterned_arc_ellipse_spline_and_conic_follow_seed_geometry_exactly() {
    for (cmd, params) in [
        ("sketch.arc.center_point", json!({"center":[0,0],"start":[10,0],"end":[0,10],"connect":false})),
        ("sketch.ellipse", json!({"center":[0,0],"major":[10,0],"minor_radius":4})),
        ("sketch.spline.fit_point", json!({"points":[[0,0],[6,8],[14,3],[20,0]],"connect":false})),
        ("sketch.conic", json!({"start":[0,0],"end":[20,0],"apex":[10,20],"rho":0.65,"connect":false})),
    ] {
        let mut s = new_sketch();
        let seed = ids(&run(&mut s, cmd, params)["curves"])[0].clone();
        let out = run(&mut s, "sketch.pattern.rectangular", json!({"entities":[seed],"count":3,"spacing":40}));
        run(&mut s, "sketch.scale", json!({"entities":[seed],"base":"origin","factor":1.5}));
        let sk = sketch(&s);
        if matches!(sk.curves[sk.curve_index(&seed).unwrap()].kind, CurveKind::Conic { .. }) {
            let from = sk.exact_segs(sk.curve_index(&seed).unwrap()).unwrap()[0].point_at(0.5);
            run(&mut s, "sketch.drag", json!({"entity":seed,"from":from,"to":[from.x,from.y+2.0]}));
        }
        check_relations(&s);
        let sk = sketch(&s);
        let ci = sk.curve_index(&seed).unwrap();
        let original = sk.exact_segs(ci).unwrap_or_else(|| sk.segs(ci));
        for (i, id) in ids(&out["curves"]).iter().enumerate() {
            let ci = sk.curve_index(id).unwrap();
            let copy = sk.exact_segs(ci).unwrap_or_else(|| sk.segs(ci));
            assert_eq!(copy.len(), original.len());
            for (a, b) in original.iter().zip(copy) {
                for t in [0.0, 0.3, 0.7, 1.0] {
                    assert!(b.point_at(t).dist(a.point_at(t) + Vec2::new(40.0 * (i + 1) as f64, 0.0)) < 1e-6, "{cmd}");
                }
            }
        }
        assert!(solidcraft_doc::format::read(&s.doc.to_json()).is_ok());
    }
}
