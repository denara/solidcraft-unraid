use crate::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, cmd: &str, params: Value) -> Value {
    s.execute(cmd, &params).unwrap()
}
fn close(actual: &Value, expected: f64) {
    assert!((actual.as_f64().unwrap() - expected).abs() < 1e-6, "{actual} != {expected}");
}
fn box_in(s: &mut Session, name: &str) -> (u64, u64, String) {
    let c = run(s, "component.create", json!({"name": name}));
    run(s, "solid.box", json!({"length": 10, "width": 10, "height": 10}));
    let body = s.model.state().bodies.last().unwrap().name.clone();
    (c["component"].as_u64().unwrap(), c["occurrence"].as_u64().unwrap(), body)
}

#[test]
fn assembly_counts_instances_and_resolves_original_materials() {
    let mut s = Session::default();
    let (component, occurrence, body) = box_in(&mut s, "Block");
    run(&mut s, "material.assign", json!({"bodies": [body], "material": "Steel"}));
    let copy = run(&mut s, "occurrence.copy", json!({"component": component, "translate": [20, 0, 0]}));
    let rho = s.doc.density(&body);
    let root = run(&mut s, "component.properties", json!({"component": "root"}));
    assert_eq!(root["body_count"], 2);
    close(&root["total"]["mass_g"], 2.0 * rho);
    close(&root["total"]["volume_mm3"], 2000.0);
    close(&root["total"]["center_of_mass"][0], 15.0);
    let first = run(&mut s, "component.properties", json!({"component": component}));
    assert_eq!(first["body_count"], 1);
    close(&first["total"]["center_of_mass"][0], 5.0);
    let second = run(&mut s, "occurrence.properties", json!({"occurrence": copy["occurrence"]}));
    close(&second["total"]["center_of_mass"][0], 25.0);
    assert_eq!(second["bodies"][0]["material"], "Steel");
    assert_eq!(first["path"], json!([occurrence]));
}

#[test]
fn mass_centroid_is_density_weighted_and_pending_moves_are_read_only() {
    let mut s = Session::default();
    let (a, oa, steel) = box_in(&mut s, "Steel block");
    run(&mut s, "material.assign", json!({"bodies": [steel], "material": "Steel"}));
    run(&mut s, "component.activate", json!({"component": "root"}));
    let (_, ob, plastic) = box_in(&mut s, "Plastic block");
    run(&mut s, "material.assign", json!({"bodies": [plastic], "material": "ABS Plastic"}));
    run(&mut s, "occurrence.move", json!({"occurrence": ob, "translate": [30, 0, 0], "capture": false}));
    let doc = s.doc.clone();
    let moves = s.pending_moves.clone();
    let r = run(&mut s, "component.properties", json!({"component": 0}));
    let ra = s.doc.density(&steel);
    let rb = s.doc.density(&plastic);
    close(&r["total"]["center_of_mass"][0], (5.0 * ra + 35.0 * rb) / (ra + rb));
    close(&r["total"]["bbox"]["max"][0], 40.0);
    assert!(std::sync::Arc::ptr_eq(&s.doc, &doc));
    assert_eq!(s.pending_moves, moves);
    run(&mut s, "occurrence.move", json!({"occurrence": oa, "translate": [10, 0, 0], "axis": [0, 0, 1], "angle": "90 deg"}));
    let r = run(&mut s, "component.properties", json!({"component": a}));
    close(&r["total"]["center_of_mass"][0], 5.0);
    close(&r["total"]["center_of_mass"][1], 5.0);
}

#[test]
fn nested_instances_require_a_path_and_can_exclude_descendants() {
    let mut s = Session::default();
    let parent = run(&mut s, "component.create", json!({"name": "Assembly"}));
    let (_, child_occurrence, _) = box_in(&mut s, "Child");
    let copy = run(&mut s, "occurrence.copy", json!({"component": parent["component"], "translate": [100, 0, 0]}));
    assert!(s.execute("occurrence.properties", &json!({"occurrence": child_occurrence})).is_err());
    let r = run(&mut s, "occurrence.properties", json!({"occurrence": child_occurrence, "path": [copy["occurrence"], child_occurrence]}));
    close(&r["total"]["center_of_mass"][0], 105.0);
    let r = run(&mut s, "component.properties", json!({"component": parent["component"]}));
    assert_eq!(r["body_count"], 1);
    let r = run(&mut s, "component.properties", json!({"component": parent["component"], "descendants": false}));
    assert_eq!(r["body_count"], 0);
    assert!(r["total"]["center_of_mass"].is_null());
    assert!(r["total"]["bbox"].is_null());
}

#[test]
fn body_reassignment_offsets_preserve_world_properties() {
    let mut s = Session::default();
    run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10}));
    let body = s.model.state().bodies.first().unwrap().name.clone();
    let c = run(&mut s, "component.create", json!({"name": "Moved"}));
    run(&mut s, "occurrence.move", json!({"occurrence": c["occurrence"], "translate": [100, 0, 0]}));
    run(&mut s, "component.move_bodies", json!({"bodies": [body], "component": c["component"]}));
    let r = run(&mut s, "component.properties", json!({"component": c["component"]}));
    close(&r["total"]["center_of_mass"][0], 5.0);
}

#[test]
fn properties_reject_hostile_identifiers_and_options() {
    let mut s = Session::default();
    let (_, occurrence, _) = box_in(&mut s, "Block");
    for p in [json!({"component": []}), json!({"component": "missing"}), json!({"descendants": 1})] {
        assert!(s.execute("component.properties", &p).is_err());
    }
    for path in [json!([]), json!(["bad"]), json!([0]), json!(vec![occurrence; 65])] {
        assert!(s.execute("occurrence.properties", &json!({"occurrence": occurrence, "path": path})).is_err());
    }
}

#[test]
fn compact_exponentially_instanced_assembly_refuses_partial_totals() {
    let mut s = Session::default();
    let mut last_occurrence = 0;
    // Only 17 component definitions and 34 occurrences, but over 100000 placements.
    for _ in 0..17 {
        let parent = s.active_component;
        let component = s.doc_mut().add_component(None, parent).unwrap();
        last_occurrence = s.doc_mut().add_occurrence(component, parent, solidcraft_doc::IDENTITY).unwrap();
        s.active_component = component;
    }
    for command in ["component.properties", "occurrence.properties"] {
        let error = s.execute(command, &json!({"component": 0, "occurrence": last_occurrence})).unwrap_err();
        assert!(error.to_string().contains("traversal limits"), "{error}");
    }
}

#[test]
fn excessively_nested_assembly_refuses_partial_totals() {
    let mut s = Session::default();
    for _ in 0..65 {
        let parent = s.active_component;
        s.active_component = s.doc_mut().add_component(None, parent).unwrap();
    }
    let error = s.execute("component.properties", &json!({"component": 0})).unwrap_err();
    assert!(error.to_string().contains("traversal limits"), "{error}");
}

/// #532: a product BOM needs materials outside the built-in library (walnut, polypropylene,
/// rubber, leather). A user-defined material with its density is assigned like a built-in one
/// and sets the body's mass: a 10 mm walnut cube at 0.65 g/cm³ weighs 0.65 g.
#[test]
#[ignore = "fails until #532 is fixed"]
fn user_defined_material_sets_the_body_mass() {
    let mut s = Session::default();
    run(&mut s, "solid.box", json!({"length": 10, "width": 10, "height": 10, "body_name": "Slat"}));
    run(&mut s, "material.create", json!({"name": "Walnut", "density_g_cm3": 0.65}));
    run(&mut s, "material.assign", json!({"bodies": ["Slat"], "material": "Walnut"}));
    let props = run(&mut s, "component.properties", json!({"component": "root"}));
    assert_eq!(props["bodies"][0]["material"], "Walnut", "{props}");
    close(&props["bodies"][0]["density_g_cm3"], 0.65);
    close(&props["total"]["mass_g"], 0.65);
}
