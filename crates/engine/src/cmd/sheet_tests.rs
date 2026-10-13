use std::f64::consts::{FRAC_PI_2, PI};

use serde_json::{Value, json};

use crate::Session;

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    match s.execute(id, &p) {
        Ok(v) => v,
        Err(e) => panic!("{id} {p}: {e}"),
    }
}

fn volume(s: &mut Session) -> f64 {
    run(s, "inspect.measure", json!({}))["total"]["volume_mm3"].as_f64().unwrap_or(f64::NAN)
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs().max(1e-12)
}

fn flat(s: &mut Session) -> Value {
    run(s, "sheet.flat_pattern", json!({}))
}

/// A w x h base flange on XY.
fn base(s: &mut Session, w: f64, h: f64) {
    run(s, "sketch.create", json!({"plane": "XY", "name": "Base"}));
    run(s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [w, h]}));
    run(s, "sketch.finish", json!({}));
    run(s, "sheet.flange", json!({"sketch": "Base"}));
}

fn rectangle_tool(s: &mut Session, name: &str, p0: [f64; 2], p1: [f64; 2]) {
    run(s, "sketch.create", json!({"plane":"XY","name":name}));
    run(s, "sketch.rectangle.two_point", json!({"p0":p0,"p1":p1}));
    run(s, "sketch.finish", json!({}));
}

#[test]
fn shallow_sheet_pocket_preserves_material_depth_export_undo_and_reopen() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    rectangle_tool(&mut s, "Pocket", [20.0, 20.0], [30.0, 30.0]);
    run(&mut s, "solid.extrude", json!({"sketch":"Pocket","distance":1,"operation":"cut"}));
    assert!((volume(&mut s) - 14900.0).abs() < 1e-6);
    let f = flat(&mut s);
    assert_eq!(f["cutouts"], json!([]));
    assert_eq!(f["pockets"][0]["side"], "bottom");
    assert_eq!(f["pockets"][0]["depth_mm"], 1.0);
    let dxf = run(&mut s, "sheet.export_dxf", json!({}))["dxf"].as_str().unwrap().to_string();
    assert!(dxf.contains("POCKET_BOTTOM_Z0.000000_TO_1.000000_MM"));
    assert_eq!(solidcraft_io::read_dxf(&dxf).unwrap().len(), 8);
    run(&mut s, "edit.undo", json!({}));
    assert!((volume(&mut s) - 15000.0).abs() < 1e-6);
    run(&mut s, "edit.redo", json!({}));
    for command in ["sheet.unfold", "sheet.refold"] {
        run(&mut s, command, json!({}));
        assert!((volume(&mut s) - 14900.0).abs() < 1e-6);
        let state = s.model.state();
        let body = &state.bodies[0].body;
        assert!(!body.contains(solidcraft_geom::Vec3::new(25.0, 25.0, 0.5)));
        assert!(body.contains(solidcraft_geom::Vec3::new(25.0, 25.0, 1.5)));
    }
    let mut reopened = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
    assert!((volume(&mut reopened) - 14900.0).abs() < 1e-6);
    assert_eq!(flat(&mut reopened)["pockets"], f["pockets"]);
}

#[test]
fn planar_sheet_intersection_retains_exact_outline_and_islands() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    rectangle_tool(&mut s, "Retain", [20.0, 20.0], [30.0, 30.0]);
    run(&mut s, "solid.extrude", json!({"sketch":"Retain","through_all":true,"direction":"symmetric","operation":"intersect"}));
    for command in [None, Some("sheet.unfold"), Some("sheet.refold")] {
        if let Some(command) = command {
            run(&mut s, command, json!({}));
        }
        assert!((volume(&mut s) - 250.0).abs() < 1e-6);
        assert_eq!(flat(&mut s)["flat_size_mm"], json!([10.0, 10.0, 2.5]));
    }
    let dxf = run(&mut s, "sheet.export_dxf", json!({}))["dxf"].as_str().unwrap().to_string();
    assert_eq!(solidcraft_io::read_dxf(&dxf).unwrap().len(), 4);
    let mut imported = Session::default();
    run(&mut imported, "sketch.insert_dxf", json!({"name":"Retained","text":dxf}));
    run(&mut imported, "sketch.finish", json!({}));
    run(&mut imported, "solid.extrude", json!({"sketch":"Retained","distance":2.5}));
    assert!((volume(&mut imported) - 250.0).abs() < 1e-6);
    // An annular retained region keeps its central hole, with no first-solid shortcut.
    let mut annulus = Session::default();
    base(&mut annulus, 100.0, 60.0);
    run(&mut annulus, "sketch.create", json!({"plane":"XY","name":"Ring"}));
    run(&mut annulus, "sketch.circle.center", json!({"center":[50,30],"radius":10}));
    run(&mut annulus, "sketch.circle.center", json!({"center":[50,30],"radius":5}));
    run(&mut annulus, "sketch.finish", json!({}));
    run(
        &mut annulus,
        "solid.extrude",
        json!({"sketch":"Ring","profiles":[{"point":[57,30]}],"through_all":true,"direction":"symmetric","operation":"intersect"}),
    );
    run(&mut annulus, "sheet.unfold", json!({}));
    let area: f64 = annulus.model.state().sheets[0].flat_regions().unwrap().iter().map(|r| r.area()).sum();
    assert!((area - PI * 75.0).abs() < 1e-8, "exact retained area: {area}");
    // inspect.measure integrates a tessellation; exact arc area is checked above.
    let measured = volume(&mut annulus);
    assert!((measured - PI * 75.0 * 2.5).abs() < 0.1, "measured={measured}");
    assert_eq!(flat(&mut annulus)["cutouts"].as_array().unwrap().len(), 1);
}

#[test]
fn overlapping_opposite_side_pockets_become_true_through_material() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    rectangle_tool(&mut s, "Bottom", [20.0, 20.0], [30.0, 30.0]);
    run(&mut s, "solid.extrude", json!({"sketch":"Bottom","distance":1.5,"operation":"cut"}));
    rectangle_tool(&mut s, "Top", [25.0, 20.0], [35.0, 30.0]);
    run(&mut s, "solid.extrude", json!({"sketch":"Top","start_offset":2.5,"direction":"negative","distance":1.5,"operation":"cut"}));
    let expected = 15000.0 - 100.0 * 1.5 * 2.0 + 50.0 * 0.5;
    for command in [None, Some("sheet.unfold"), Some("sheet.refold")] {
        if let Some(command) = command {
            run(&mut s, command, json!({}));
        }
        assert!((volume(&mut s) - expected).abs() < 1e-6);
        assert_eq!(flat(&mut s)["cutouts"].as_array().unwrap().len(), 1);
        assert_eq!(flat(&mut s)["pockets"].as_array().unwrap().len(), 2);
    }
}

#[test]
fn pockets_survive_forming_away_from_bends_and_refuse_bend_overlap_atomically() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    rectangle_tool(&mut s, "Pocket", [20.0, 20.0], [30.0, 30.0]);
    run(&mut s, "solid.extrude", json!({"sketch":"Pocket","distance":1,"operation":"cut"}));
    run(&mut s, "sheet.flange", json!({"edges":[[50,0,2.5]],"height":20}));
    assert!((volume(&mut s) - (18972.62196081845 - 100.0)).abs() < 0.2);
    run(&mut s, "sheet.unfold", json!({}));
    let unfolded = volume(&mut s);
    run(&mut s, "sheet.refold", json!({}));
    assert!((volume(&mut s) - (18972.62196081845 - 100.0)).abs() < 0.2);
    assert!(unfolded > 14900.0);
    let mut cross = Session::default();
    base(&mut cross, 100.0, 60.0);
    rectangle_tool(&mut cross, "Pocket", [45.0, 20.0], [55.0, 30.0]);
    run(&mut cross, "solid.extrude", json!({"sketch":"Pocket","distance":1,"operation":"cut"}));
    let before = cross.doc.to_json();
    let undo = cross.undo.len();
    let error = cross.execute("sheet.fold", &json!({"points":[[50,0,2.5],[50,60,2.5]]})).unwrap_err().to_string();
    assert!(error.contains("pocket") && error.contains("bend"), "{error}");
    assert_eq!(cross.doc.to_json(), before);
    assert_eq!(cross.undo.len(), undo);
    assert!((volume(&mut cross) - 14900.0).abs() < 1e-6);
}

#[test]
fn unsupported_sheet_machining_is_atomic_but_true_no_change_is_allowed() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    rectangle_tool(&mut s, "Taper", [20.0, 20.0], [30.0, 30.0]);
    let before = s.doc.to_json();
    let undo = s.undo.len();
    let error = s.execute("solid.extrude", &json!({"sketch":"Taper","distance":1,"operation":"cut","taper":"5 deg"})).unwrap_err().to_string();
    assert!(error.contains("sheet") && error.contains("preserv"), "{error}");
    assert_eq!(s.doc.to_json(), before);
    assert_eq!(s.undo.len(), undo);
    rectangle_tool(&mut s, "Outside", [120.0, 20.0], [130.0, 30.0]);
    run(&mut s, "solid.extrude", json!({"sketch":"Outside","distance":1,"operation":"cut","taper":"5 deg"}));
    assert!((volume(&mut s) - 15000.0).abs() < 1e-6);
    rectangle_tool(&mut s, "All", [-20.0, -20.0], [120.0, 80.0]);
    run(&mut s, "solid.extrude", json!({"sketch":"All","through_all":true,"direction":"symmetric","operation":"intersect"}));
    run(&mut s, "sheet.unfold", json!({}));
    assert!((volume(&mut s) - 15000.0).abs() < 1e-6);
}

#[test]
fn empty_sheet_intersection_removes_geometry_and_metadata_together() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    rectangle_tool(&mut s, "Outside", [120.0, 20.0], [130.0, 30.0]);
    run(&mut s, "solid.extrude", json!({"sketch":"Outside","through_all":true,"direction":"symmetric","operation":"intersect"}));
    assert!(s.model.state().bodies.is_empty());
    assert!(s.model.state().sheets.is_empty());
    assert!(s.execute("sheet.unfold", &json!({})).is_err());
    run(&mut s, "edit.undo", json!({}));
    assert!((volume(&mut s) - 15000.0).abs() < 1e-6);
}

#[test]
fn intersect_preserves_both_material_regions_of_a_logical_ring_sheet() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    run(&mut s, "sketch.create", json!({"plane":"XY","name":"Ring"}));
    for radius in [10.0, 5.0] {
        run(&mut s, "sketch.circle.center", json!({"center":[50,30],"radius":radius}));
    }
    run(&mut s, "sketch.finish", json!({}));
    run(
        &mut s,
        "solid.extrude",
        json!({"sketch":"Ring","profiles":[{"point":[57,30]}],"through_all":true,"direction":"symmetric","operation":"cut"}),
    );
    rectangle_tool(&mut s, "Crop", [35.0, 15.0], [65.0, 45.0]);
    run(&mut s, "solid.extrude", json!({"sketch":"Crop","through_all":true,"direction":"symmetric","operation":"intersect"}));
    for command in [None, Some("sheet.unfold"), Some("sheet.refold")] {
        if let Some(command) = command {
            run(&mut s, command, json!({}));
        }
        let state = s.model.state();
        let regions = state.sheets[0].flat_regions().unwrap();
        assert_eq!(regions.len(), 2);
        let area: f64 = regions.iter().map(|r| r.area()).sum();
        assert!((area - (900.0 - PI * 75.0)).abs() < 1e-8, "area={area}");
        let body = &state.bodies[0].body;
        assert!(body.contains(solidcraft_geom::Vec3::new(50.0, 30.0, 1.0)));
        assert!(!body.contains(solidcraft_geom::Vec3::new(57.0, 30.0, 1.0)));
        assert!(body.contains(solidcraft_geom::Vec3::new(64.0, 30.0, 1.0)));
        assert!(!body.contains(solidcraft_geom::Vec3::new(70.0, 30.0, 1.0)));
        assert!((volume(&mut s) - (900.0 - PI * 75.0) * 2.5).abs() < 0.2);
    }
    let dxf = run(&mut s, "sheet.export_dxf", json!({}))["dxf"].as_str().unwrap().to_string();
    assert_eq!(solidcraft_io::read_dxf(&dxf).unwrap().len(), 6);
}

#[test]
fn machining_a_folded_parallel_panel_preserves_its_pocket_on_unfold() {
    use solidcraft_doc::{apply_point, apply_vector};
    use solidcraft_geom::Vec3;
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    run(&mut s, "sheet.flange", json!({"edges":[[50,0,2.5]],"height":20}));
    let sh = s.model.state().sheets[0].clone();
    let points = sh.panel(&sh.flanges[0]).outer.polyline(1e-3);
    let c = points.iter().copied().fold(solidcraft_geom::Vec2::ZERO, |sum, p| sum + p) / (points.len() as f64);
    let w = sh.world(Some(0));
    let origin = apply_point(&w, Vec3::ZERO);
    let x = apply_vector(&w, Vec3::X);
    let y = apply_vector(&w, Vec3::Y);
    run(
        &mut s,
        "sketch.create",
        json!({"name":"PanelPocket","plane":{"origin":[origin.x,origin.y,origin.z],"x_dir":[x.x,x.y,x.z],"y_dir":[y.x,y.y,y.z]}}),
    );
    run(&mut s, "sketch.rectangle.two_point", json!({"p0":[c.x-2.0,c.y-2.0],"p1":[c.x+2.0,c.y+2.0]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "solid.extrude", json!({"sketch":"PanelPocket","distance":1,"operation":"cut"}));
    assert!((volume(&mut s) - (18972.62196081845 - 16.0)).abs() < 0.2);
    run(&mut s, "sheet.unfold", json!({}));
    let state = s.model.state();
    let body = &state.bodies[0].body;
    assert!(!body.contains(Vec3::new(c.x, c.y, 0.5)));
    assert!(body.contains(Vec3::new(c.x, c.y, 1.5)));
    assert_eq!(flat(&mut s)["pockets"][0]["depth_mm"], 1.0);
    run(&mut s, "sheet.refold", json!({}));
    assert!((volume(&mut s) - (18972.62196081845 - 16.0)).abs() < 0.2);
}

#[test]
fn flipped_sheet_pocket_keeps_depth_in_the_local_thickness_direction() {
    let mut s = Session::default();
    rectangle_tool(&mut s, "Base", [0.0, 0.0], [100.0, 60.0]);
    run(&mut s, "sheet.flange", json!({"sketch":"Base","flip":true}));
    rectangle_tool(&mut s, "Pocket", [20.0, 20.0], [30.0, 30.0]);
    run(&mut s, "solid.extrude", json!({"sketch":"Pocket","distance":1,"direction":"negative","operation":"cut"}));
    for command in [None, Some("sheet.unfold"), Some("sheet.refold")] {
        if let Some(command) = command {
            run(&mut s, command, json!({}));
        }
        assert!((volume(&mut s) - 14900.0).abs() < 1e-6);
        let state = s.model.state();
        assert!(!state.bodies[0].body.contains(solidcraft_geom::Vec3::new(25.0, 25.0, -0.5)));
        assert!(state.bodies[0].body.contains(solidcraft_geom::Vec3::new(25.0, 25.0, -1.5)));
    }
}

#[test]
fn multiple_profile_intersect_keeps_tool_union_for_solids_and_sheet_groups() {
    for sheet in [false, true] {
        let mut s = Session::default();
        if sheet {
            base(&mut s, 100.0, 60.0);
        } else {
            run(&mut s, "solid.box", json!({"length":100,"width":60,"height":2.5,"body_name":"Blank"}));
        }
        let target = s.model.state().bodies[0].name.clone();
        run(&mut s, "solid.box", json!({"corner":[200,0,0],"length":10,"width":10,"height":10,"body_name":"Control"}));
        run(&mut s, "sketch.create", json!({"plane":"XY","name":"TwoIslands"}));
        for x in [20.0, 60.0] {
            run(&mut s, "sketch.rectangle.two_point", json!({"p0":[x,20],"p1":[x+10.0,30]}));
        }
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "solid.extrude", json!({"sketch":"TwoIslands","operation":"intersect","distance":3,"targets":[target]}));
        assert!((volume(&mut s) - 1500.0).abs() < 1e-6);
        if sheet {
            for command in ["sheet.unfold", "sheet.refold"] {
                run(&mut s, command, json!({"body":target}));
                assert!((volume(&mut s) - 1500.0).abs() < 1e-6);
                assert_eq!(flat(&mut s)["outline"].as_array().unwrap().len(), 2);
            }
        }
        let state = s.model.state();
        for x in [25.0, 65.0] {
            assert!(state.bodies.iter().any(|b| b.body.contains(solidcraft_geom::Vec3::new(x, 25.0, 1.0))));
        }
        assert!(!state.bodies.iter().any(|b| b.body.contains(solidcraft_geom::Vec3::new(50.0, 25.0, 1.0))));
        assert!(state.bodies.iter().any(|b| b.name == "Control" && b.body.contains(solidcraft_geom::Vec3::new(205.0, 5.0, 5.0))));
    }
}

#[test]
fn deleting_sheet_material_prunes_cosmetic_threads_and_undo_restores_them() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane":"XY","name":"ThreadPlate"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0":[0,0],"p1":[100,60]}));
    run(&mut s, "sketch.circle.center", json!({"center":[50,30],"radius":5}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "sheet.flange", json!({"sketch":"ThreadPlate","profiles":[{"point":[10,10]}]}));
    run(&mut s, "solid.thread", json!({"face":[55,30,1],"designation":"M10"}));
    assert_eq!(s.model.state().threads.len(), 1);
    rectangle_tool(&mut s, "Outside", [120.0, 20.0], [130.0, 30.0]);
    run(&mut s, "solid.extrude", json!({"sketch":"Outside","through_all":true,"direction":"symmetric","operation":"intersect"}));
    assert!(s.model.state().bodies.is_empty());
    assert!(s.model.state().sheets.is_empty());
    assert!(s.model.state().threads.is_empty());
    assert!(solidcraft_doc::world_state(&s.doc, &s.model.state()).threads.is_empty());
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.model.state().threads.len(), 1);
}

#[test]
fn sheet_pocket_transfer_keeps_local_depth_world_pose_and_history() {
    use solidcraft_geom::Vec3;
    fn check(s: &Session, name: &str, owner: u64, local_origin: Vec3, local_y: Vec3, transferred: bool) {
        let state = s.model.state();
        let mb = state.body(name).unwrap();
        assert_eq!(mb.component, owner);
        assert_eq!(mb.transferred, transferred);
        let sh = state.sheets.iter().find(|sh| sh.body == name).unwrap();
        assert!(sh.frame.origin.dist(local_origin) < 1e-8, "{:?}", sh.frame);
        assert!(sh.frame.x.dist(Vec3::X) < 1e-8);
        assert!(sh.frame.y.dist(local_y) < 1e-8);
        assert_eq!(sh.pockets.len(), 1);
        assert_eq!((sh.pockets[0].z0, sh.pockets[0].z1), (0.0, 1.0));
        let cut = sh.frame.to_world(solidcraft_geom::Vec2::new(25.0, 25.0)) + sh.frame.normal() * 0.5;
        let floor = sh.frame.to_world(solidcraft_geom::Vec2::new(25.0, 25.0)) + sh.frame.normal() * 1.5;
        assert!(!mb.body.contains(cut));
        assert!(mb.body.contains(floor));
        let world = s.world_state();
        let body = &world.body(name).unwrap().body;
        let bounds = body.tessellate(1e-3).unwrap().bounds();
        assert!(bounds.min.dist(Vec3::ZERO) < 1e-6, "{bounds:?}");
        assert!(bounds.max.dist(Vec3::new(100.0, 60.0, 2.5)) < 1e-6, "{bounds:?}");
        assert!(!body.contains(Vec3::new(25.0, 25.0, 0.5)));
        assert!(body.contains(Vec3::new(25.0, 25.0, 1.5)));
        assert!((solidcraft_kernel::measure(body).unwrap().volume - 14900.0).abs() < 1e-6);
    }
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    rectangle_tool(&mut s, "Pocket", [20.0, 20.0], [30.0, 30.0]);
    run(&mut s, "solid.extrude", json!({"sketch":"Pocket","distance":1,"operation":"cut"}));
    let name = s.model.state().sheets[0].body.clone();
    check(&s, &name, 0, Vec3::ZERO, Vec3::Y, false);
    let destination = run(&mut s, "component.create", json!({"name":"Machined","parent":0}));
    let component = destination["component"].as_u64().unwrap();
    let occurrence = destination["occurrence"].as_u64().unwrap();
    run(&mut s, "occurrence.move", json!({"occurrence":occurrence,"translate":[30,20,10],"axis":[1,0,0],"angle":"90 deg","capture":true}));
    run(&mut s, "component.move_bodies", json!({"component":component,"bodies":[name]}));
    assert!(!s.doc.body_offsets.contains_key(&name), "recorded transfer must not leave a legacy placement offset");
    let origin = Vec3::new(-30.0, -10.0, 20.0);
    let y = Vec3::new(0.0, 0.0, -1.0);
    check(&s, &name, component, origin, y, true);
    run(&mut s, "edit.undo", json!({}));
    check(&s, &name, 0, Vec3::ZERO, Vec3::Y, false);
    run(&mut s, "edit.redo", json!({}));
    check(&s, &name, component, origin, y, true);
    run(&mut s, "component.activate", json!({"component":component}));
    for command in ["sheet.unfold", "sheet.refold"] {
        run(&mut s, command, json!({"body":name}));
        check(&s, &name, component, origin, y, true);
        let f = run(&mut s, "sheet.flat_pattern", json!({"body":name}));
        assert_eq!(f["pockets"][0]["side"], "bottom");
        assert_eq!(f["pockets"][0]["depth_mm"], 1.0);
    }
    let mut reopened = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
    check(&reopened, &name, component, origin, y, true);
    run(&mut reopened, "component.activate", json!({"component":component}));
    run(&mut reopened, "sheet.unfold", json!({"body":name}));
    check(&reopened, &name, component, origin, y, true);
}

#[test]
fn base_and_edge_flange_follow_the_k_factor() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    assert!(rel(volume(&mut s), 15000.0) < 1e-9);
    run(&mut s, "sheet.flange", json!({"edges": [[50, 0, 2.5]], "height": 20}));
    // Oracle 68: flat = 60 − 5 + (π/2)(2.5 + 0.44·2.5) + 20 − 5.
    let want = 60.0 - 5.0 + FRAC_PI_2 * (2.5 + 0.44 * 2.5) + 15.0;
    let f = flat(&mut s);
    assert!((f["flat_size_mm"][1].as_f64().unwrap_or(0.0) - want).abs() < 1e-6, "{f}");
    assert_eq!(f["bends"][0]["direction"], "up");
    assert!(rel(volume(&mut s), 18972.622) < 1e-4, "{}", volume(&mut s));
    let m = run(&mut s, "inspect.measure", json!({}));
    assert_eq!(m["total"]["faces"], 14, "{m}");
    // The angle is a parameter: 45° (oracle 70 uses height 15).
    let names = s.doc.features.last().map(|f| f.param_names.clone()).unwrap_or_default();
    run(&mut s, "parameters.change", json!({"name": names[0], "expression": "15"}));
    run(&mut s, "parameters.change", json!({"name": names[1], "expression": "45 deg"}));
    assert!(rel(volume(&mut s), 18450.777) < 1e-4, "{}", volume(&mut s));
    assert!((flat(&mut s)["flat_size_mm"][1].as_f64().unwrap_or(0.0) - 73.685298).abs() < 1e-5);
    // DXF: outline and an up bend line.
    let d = run(&mut s, "sheet.export_dxf", json!({}));
    let text = d["dxf"].as_str().unwrap_or_default();
    assert!(text.contains("BEND_UP") && text.contains("OUTLINE"), "{text}");
    assert!(s.execute("sheet.flange", &json!({"edges": [[50, 30, 40]], "height": 20})).is_err(), "not on an edge");
}

#[test]
fn rules_are_parameters() {
    let mut s = Session::default();
    run(&mut s, "parameters.change", json!({"name": "Thick", "expression": "1.5 mm"}));
    run(&mut s, "sheet.manage_rules", json!({"name": "Custom", "thickness": "Thick", "k_factor": 0.33, "bend_radius": "3 mm"}));
    base(&mut s, 80.0, 50.0);
    run(&mut s, "sheet.flange", json!({"edges": [[40, 50, 1.5], [40, 0, 1.5]], "height": 20}));
    // Oracle 71: 50 − 9 + 2·(π/2)(3 + 0.495) + 2·15.5 = 82.980.
    let want = 50.0 - 2.0 * 4.5 + 2.0 * FRAC_PI_2 * (3.0 + 0.33 * 1.5) + 2.0 * 15.5;
    assert!((flat(&mut s)["flat_size_mm"][1].as_f64().unwrap_or(0.0) - want).abs() < 1e-6);
    assert!(rel(volume(&mut s), 10053.717) < 1e-4, "{}", volume(&mut s));
    // Thicker: everything follows.
    run(&mut s, "parameters.change", json!({"name": "Thick", "expression": "2 mm"}));
    let want = 50.0 - 2.0 * 5.0 + 2.0 * FRAC_PI_2 * (3.0 + 0.33 * 2.0) + 2.0 * 15.0;
    assert!((flat(&mut s)["flat_size_mm"][1].as_f64().unwrap_or(0.0) - want).abs() < 1e-6);
    let l = run(&mut s, "sheet.rules", json!({}));
    assert_eq!(l["active"], "Custom", "{l}");
    assert!(s.execute("sheet.manage_rules", &json!({"name": "Bad", "k_factor": 3})).is_err());
}

#[test]
fn contour_flange_hem_and_tray() {
    // Oracle 72.
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": {"origin": [0, 0, 0], "x_dir": [1, 0, 0], "y_dir": [0, 0, -1]}, "name": "Contour"}));
    run(&mut s, "sketch.line", json!({"points": [[0, 0], [40, 0], [40, -30], [70, -30]], "ids": ["l1", "l2", "l3"]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "sheet.flange", json!({"sketch": "Contour", "curves": ["l1", "l2", "l3"], "distance": 50}));
    assert!(rel(volume(&mut s), 12097.622) < 1e-4, "{}", volume(&mut s));
    assert!((flat(&mut s)["flat_size_mm"][0].as_f64().unwrap_or(0.0) - 96.309734).abs() < 1e-5);
    // Oracle 73: flange, then a flat hem of 8.
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    run(&mut s, "sheet.flange", json!({"edges": [[50, 0, 2.5]], "height": 20}));
    run(&mut s, "sheet.hem", json!({"edges": [[50, 0, 20]], "length": 8}));
    assert!(rel(volume(&mut s), 21334.723) < 1e-4, "{}", volume(&mut s));
    let b = flat(&mut s)["bends"].clone();
    assert!(b.as_array().is_some_and(|a| a.iter().any(|x| (x["angle_deg"].as_f64().unwrap_or(0.0) - 180.0).abs() < 1e-9)), "{b}");
    // A tray: four flanges at once stop at each other's bends.
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    run(&mut s, "sheet.flange", json!({"edges": [[50, 60, 2.5], [100, 30, 2.5], [50, 0, 2.5], [0, 30, 2.5]], "height": 25}));
    let f = flat(&mut s);
    // Oracle 69: 100 − 10 + 2·5.655 + 2·20 = 141.310 across.
    assert!((f["flat_size_mm"][0].as_f64().unwrap_or(0.0) - (90.0 + 2.0 * FRAC_PI_2 * 3.6 + 40.0)).abs() < 1e-6, "{f}");
    let v = volume(&mut s);
    // Each flange runs on past its bend toward the corner until the inner corners are the
    // rule's gap apart (diagonally): 5 − (2.5 + 2.5/√2) = 0.732 at each of 8 ends.
    let section = PI / 4.0 * (25.0 - 6.25) + 2.5 * 20.0;
    let ext = 5.0 - (2.5 + 2.5 / std::f64::consts::SQRT_2);
    let want = 90.0 * 50.0 * 2.5 + section * (2.0 * 90.0 + 2.0 * 50.0) + 8.0 * ext * 2.5 * 20.0;
    assert!(rel(v, want) < 1e-4 && rel(v, 29666.234) < 1e-4, "{v} {want}");
}

#[test]
fn unfold_cut_refold_and_convert() {
    // Oracle 74: two flanges, unfold, a slot across the bend, refold.
    let mut s = Session::default();
    base(&mut s, 80.0, 50.0);
    run(&mut s, "sheet.flange", json!({"edges": [[40, 50, 2.5], [40, 0, 2.5]], "height": 20}));
    let folded = volume(&mut s);
    run(&mut s, "sheet.unfold", json!({}));
    let m = run(&mut s, "inspect.measure", json!({}));
    assert!((m["bodies"][0]["bbox"]["max"][2].as_f64().unwrap_or(0.0) - 2.5).abs() < 1e-6, "{m}");
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Slot"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [35, -12], "p1": [45, 10]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "solid.extrude", json!({"sketch": "Slot", "through_all": true, "operation": "cut", "direction": "symmetric"}));
    run(&mut s, "sheet.refold", json!({}));
    let v = volume(&mut s);
    assert!(v < folded - 300.0, "{v} {folded}");
    assert!(rel(v, 15800.304) < 1e-4, "{v}");
    assert_eq!(flat(&mut s)["cutouts"].as_array().map(Vec::len), Some(1));
    // Convert a plate.
    let mut s = Session::default();
    run(&mut s, "solid.box", json!({"length": 100, "width": 60, "height": 2.5, "body_name": "Plate"}));
    run(&mut s, "sheet.convert", json!({"body": "Plate", "face": [50, 30, 2.5]}));
    run(&mut s, "sheet.flange", json!({"edges": [[50, 0, 2.5]], "height": 20}));
    assert!(rel(volume(&mut s), 18972.622) < 1e-4, "{}", volume(&mut s));
    let mut t = Session::default();
    run(&mut t, "solid.cylinder", json!({"radius": 10, "height": 30, "body_name": "Rod"}));
    assert!(t.execute("sheet.convert", &json!({"body": "Rod", "face": [0, 0, 30]})).is_err());
}

/// A sketch line across a 100 x 60 plate's top face at x = 40.
fn fold_line(s: &mut Session) {
    run(s, "sketch.create", json!({"plane": "XY", "offset": 2.5, "name": "FoldLine"}));
    run(s, "sketch.line", json!({"points": [[40, 0], [40, 60]], "ids": ["fl"]}));
    run(s, "sketch.finish", json!({}));
}

fn bbox(s: &mut Session) -> Value {
    run(s, "inspect.measure", json!({}))["bodies"][0]["bbox"].clone()
}

#[test]
fn fold_takes_the_bend_out_of_the_flat_sheet() {
    let (t, r, k) = (2.5, 2.5, 0.44);
    let ba = FRAC_PI_2 * (r + k * t);
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    fold_line(&mut s);
    run(&mut s, "sheet.fold", json!({"sketch": "FoldLine", "curve": "fl"}));
    // The flat pattern keeps its size; the bend's centre line is the sketch line.
    let f = flat(&mut s);
    assert!((f["flat_size_mm"][0].as_f64().unwrap_or(0.0) - 100.0).abs() < 1e-6, "{f}");
    assert!((f["flat_size_mm"][1].as_f64().unwrap_or(0.0) - 60.0).abs() < 1e-6, "{f}");
    assert!((f["bends"][0]["allowance"].as_f64().unwrap_or(0.0) - ba).abs() < 1e-9, "{f}");
    // Folded: the zone's flat volume BA·T·w becomes the bend's θ·T·(R + T/2)·w.
    let want = 15000.0 - ba * t * 60.0 + FRAC_PI_2 * t * (r + t / 2.0) * 60.0;
    assert!(rel(volume(&mut s), want) < 1e-4, "{} {want}", volume(&mut s));
    // The smaller side (x < 40) stands up: outer face at the zone start less R + T, leg 40 − BA/2
    // tall above the bend.
    let b = bbox(&mut s);
    assert!((b["min"][0].as_f64().unwrap_or(0.0) - (40.0 + ba / 2.0 - r - t)).abs() < 1e-6, "{b}");
    assert!((b["max"][2].as_f64().unwrap_or(0.0) - (t + r + 40.0 - ba / 2.0)).abs() < 1e-6, "{b}");
    assert!((b["max"][0].as_f64().unwrap_or(0.0) - 100.0).abs() < 1e-6, "{b}");
}

#[test]
fn fold_positions_fixed_side_and_flip() {
    let (t, r, k) = (2.5, 2.5, 0.44);
    let ba = FRAC_PI_2 * (r + k * t);
    let case = |p: Value| {
        let mut s = Session::default();
        base(&mut s, 100.0, 60.0);
        fold_line(&mut s);
        let mut q = json!({"sketch": "FoldLine", "curve": "fl"});
        for (k, v) in p.as_object().into_iter().flatten() {
            q[k] = v.clone();
        }
        run(&mut s, "sheet.fold", q);
        bbox(&mut s)
    };
    let x0 = |b: &Value| b["min"][0].as_f64().unwrap_or(f64::NAN);
    // Start: the bend starts at the line (on the fixed side), so the folded outer face is R + T
    // short of it. End: the bend ends there. Mould: the outer face lands on the line.
    assert!((x0(&case(json!({"position": "start"}))) - (40.0 - r - t)).abs() < 1e-6);
    assert!((x0(&case(json!({"position": "end"}))) - (40.0 + ba - r - t)).abs() < 1e-6);
    assert!((x0(&case(json!({"position": "mould"}))) - 40.0).abs() < 1e-6);
    // Keep the small side: the large one (x > 40) stands up instead.
    let b = case(json!({"fixed": [10, 30, 2.5]}));
    assert!((b["max"][0].as_f64().unwrap_or(0.0) - (40.0 - ba / 2.0 + r + t)).abs() < 1e-6, "{b}");
    assert!((b["max"][2].as_f64().unwrap_or(0.0) - (t + r + 60.0 - ba / 2.0)).abs() < 1e-6, "{b}");
    // Flipped: folds down, below the sheet.
    let b = case(json!({"flip": true}));
    assert!((b["min"][2].as_f64().unwrap_or(0.0) + (r + 40.0 - ba / 2.0)).abs() < 1e-6, "{b}");
    // A 45° fold: the leg leans.
    let b = case(json!({"angle": "45 deg"}));
    assert!(b["max"][2].as_f64().unwrap_or(0.0) < t + r + 40.0 - ba / 2.0);
}

#[test]
fn fold_carries_flanges_on_the_moving_side() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    // A flange on the left edge, then a fold between it and the rest.
    run(&mut s, "sheet.flange", json!({"edges": [[0, 30, 2.5]], "height": 20}));
    let v0 = volume(&mut s);
    fold_line(&mut s);
    run(&mut s, "sheet.fold", json!({"sketch": "FoldLine", "curve": "fl", "fixed": [90, 30, 2.5]}));
    // The flange rode along: it is above the plate now, nothing left at z < 0.
    let b = bbox(&mut s);
    assert!(b["min"][2].as_f64().unwrap_or(-1.0) > -1e-6, "{b}");
    let ba = FRAC_PI_2 * (2.5 + 0.44 * 2.5);
    let want = v0 - ba * 2.5 * 60.0 + FRAC_PI_2 * 2.5 * (2.5 + 1.25) * 60.0;
    assert!(rel(volume(&mut s), want) < 1e-4, "{} {want}", volume(&mut s));
    assert!(flat(&mut s)["bends"].as_array().map(Vec::len) == Some(2));
    // Errors: off the sheet, through the flange's bend.
    let mut e = Session::default();
    base(&mut e, 100.0, 60.0);
    assert!(e.execute("sheet.fold", &json!({"points": [[40, 0, 30], [40, 60, 30]]})).is_err());
    assert!(e.execute("sheet.fold", &json!({"points": [[40, 0, 2.5], [40, 60, 2.5]], "position": "sideways"})).is_err());
    assert!(e.execute("sheet.fold", &json!({"points": [[0.5, 0, 2.5], [0.5, 60, 2.5]]})).is_err(), "no material past the bend");
}

#[test]
fn fold_on_a_flange_panel() {
    let (t, r, k) = (2.5, 2.5, 0.44);
    let ba = FRAC_PI_2 * (r + k * t);
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    run(&mut s, "sheet.flange", json!({"edges": [[50, 0, 2.5]], "height": 30}));
    let flat0 = flat(&mut s)["flat_size_mm"].clone();
    let v0 = volume(&mut s);
    // The flange stands up at the plate's y = 0 edge: its outer face is the body's min y.
    let y = bbox(&mut s)["min"][1].as_f64().unwrap_or(f64::NAN);
    run(&mut s, "sheet.fold", json!({"points": [[10, y, 18], [90, y, 18]]}));
    let f = flat(&mut s);
    for i in 0..2 {
        assert!(
            (f["flat_size_mm"][i].as_f64().unwrap_or(0.0) - flat0[i].as_f64().unwrap_or(1.0)).abs() < 1e-6,
            "the flat pattern keeps its size: {f}"
        );
    }
    assert_eq!(f["bends"].as_array().map(Vec::len), Some(2));
    let len = f["bends"][1]["length"].as_f64().unwrap_or(0.0);
    assert!(len > 90.0, "{f}");
    let want = v0 - ba * t * len + FRAC_PI_2 * t * (r + t / 2.0) * len;
    assert!(rel(volume(&mut s), want) < 1e-4, "{} {want}", volume(&mut s));
    // The top part now leans over the plate (folded up, toward its top face).
    let b = bbox(&mut s);
    assert!(b["max"][2].as_f64().unwrap_or(99.0) < 25.0, "{b}");
    // Folding across the flange's own bend is refused.
    assert!(s.execute("sheet.fold", &json!({"points": [[50, y, 2], [50, y, 25]]})).is_err());
}

#[test]
fn fold_through_cut_outs() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    // A slot across the fold line (edges along and across the bend).
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Slot"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [30, 20], "p1": [50, 40]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "solid.extrude", json!({"sketch": "Slot", "through_all": true, "operation": "cut", "direction": "symmetric"}));
    let mut plain = Session::default();
    base(&mut plain, 100.0, 60.0);
    fold_line(&mut plain);
    run(&mut plain, "sheet.fold", json!({"sketch": "FoldLine", "curve": "fl"}));
    fold_line(&mut s);
    run(&mut s, "sheet.fold", json!({"sketch": "FoldLine", "curve": "fl"}));
    let f = flat(&mut s);
    assert_eq!(f["cutouts"].as_array().map(Vec::len), Some(1), "{f}");
    let (v, vp) = (volume(&mut s), volume(&mut plain));
    // The slot takes 20 x 20 of the flat sheet; across the bend it removes the bend's share.
    assert!(v < vp - 900.0 && v > vp - 1100.0, "{v} {vp}");
    run(&mut s, "sheet.unfold", json!({}));
    assert!((volume(&mut s) - (15000.0 - 1000.0)).abs() < 1e-3, "{}", volume(&mut s));
    // A round hole across the bend folds too: its edge runs over the bend as a curve on the
    // bend's cylinder, and unfolds to the same round hole.
    let mut c = Session::default();
    base(&mut c, 100.0, 60.0);
    run(&mut c, "sketch.create", json!({"plane": "XY", "name": "Hole"}));
    run(&mut c, "sketch.circle.center", json!({"center": [40, 30], "radius": 5}));
    run(&mut c, "sketch.finish", json!({}));
    run(&mut c, "solid.extrude", json!({"sketch": "Hole", "through_all": true, "operation": "cut", "direction": "symmetric"}));
    fold_line(&mut c);
    run(&mut c, "sheet.fold", json!({"sketch": "FoldLine", "curve": "fl"}));
    let f = flat(&mut c);
    assert_eq!(f["cutouts"].as_array().map(Vec::len), Some(1), "{f}");
    let folded = volume(&mut c);
    assert!(folded < 15000.0 - 150.0, "{folded}");
    run(&mut c, "sheet.unfold", json!({}));
    let want = 15000.0 - std::f64::consts::PI * 25.0 * 2.5;
    assert!((volume(&mut c) - want).abs() < 0.05, "{} vs {want}", volume(&mut c));
}

#[test]
fn flange_edges_are_named_and_follow_the_base_sketch() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Base"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [100, 60]}));
    let d = run(&mut s, "sketch.dimension", json!({"entities": ["l1"], "value": 100}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "sheet.flange", json!({"sketch": "Base"}));
    run(&mut s, "sheet.flange", json!({"edges": [[100, 30, 2.5]], "height": 20, "name": "Side"}));
    let side = s.doc.find_feature("Side").cloned().unwrap();
    assert_eq!(side.edge_names, vec!["base:l2:top".to_string()]);
    let v0 = volume(&mut s);
    // Narrower base: the flange stays on the right edge (now at x = 40).
    let name = d["param"].as_str().or_else(|| d["name"].as_str()).map(str::to_string).unwrap_or_else(|| "d1".into());
    run(&mut s, "parameters.change", json!({"name": name, "expression": "40"}));
    let b = run(&mut s, "inspect.measure", json!({}))["bodies"][0]["bbox"].clone();
    let w = b["max"][0].as_f64().unwrap_or(0.0) - b["min"][0].as_f64().unwrap_or(0.0);
    assert!((w - 40.0).abs() < 1e-3 && b["max"][2].as_f64().unwrap_or(0.0) > 19.0, "{b}");
    assert!(volume(&mut s) < v0);
    let id = side.id;
    assert_eq!(s.model.result(id).and_then(|r| r.warning.clone()), None);
    // A hem on the flange's far edge is named after the flange.
    let top = b["max"][2].as_f64().unwrap_or(0.0);
    let x = b["max"][0].as_f64().unwrap_or(0.0);
    run(&mut s, "sheet.hem", json!({"edges": [[x, 30, top]], "length": 6, "name": "H"}));
    let h = s.doc.find_feature("H").cloned().unwrap();
    assert!(h.edge_names[0].starts_with(&format!("F{id}.0:far")), "{:?}", h.edge_names);
}

/// With no fixed side, the larger side stays whichever side of the line it is on (QA s22).
#[test]
fn fold_keeps_the_larger_side_either_way() {
    for (w, line, moving_right) in [(60.0, 40.0, true), (100.0, 40.0, false)] {
        let mut s = Session::default();
        base(&mut s, w, 40.0);
        run(&mut s, "sheet.fold", json!({"points": [[line, 0, 2.5], [line, 40, 2.5]]}));
        let b = bbox(&mut s);
        let (x0, x1) = (b["min"][0].as_f64().unwrap_or(0.0), b["max"][0].as_f64().unwrap_or(0.0));
        if moving_right {
            assert!(x0.abs() < 1e-6 && x1 < line + 5.0, "the left (larger) part stays: {b}");
        } else {
            assert!((x1 - w).abs() < 1e-6 && x0 > line - 5.0, "the right (larger) part stays: {b}");
        }
    }
}

/// A 100 x 60 plate with a 90° flange up from its y = 0 edge (steel: 2.5 thick, R = T, K 0.44).
/// The flange's inner face is the plane y = 2.5 and its outer face y = 0; it stands from z = 5.
fn plate_with_flange(s: &mut Session) {
    base(s, 100.0, 60.0);
    run(s, "sheet.flange", json!({"edges": [[50, 0, 2.5]], "height": 20}));
}

/// A circle sketch on `plane`, finished.
fn circle_sketch(s: &mut Session, name: &str, plane: Value, center: [f64; 2], diameter: f64) -> Value {
    let made = run(s, "sketch.create", json!({"plane": plane, "name": name}));
    run(s, "sketch.circle.center", json!({"center": center, "diameter": diameter}));
    run(s, "sketch.finish", json!({}));
    made
}

fn cut_through(s: &mut Session, sketch: &str) {
    run(s, "solid.extrude", json!({"sketch": sketch, "through_all": true, "direction": "negative", "operation": "cut"}));
}

fn contains(s: &Session, p: [f64; 3]) -> bool {
    s.model.state().bodies.first().is_some_and(|b| b.body.contains(solidcraft_geom::Vec3::new(p[0], p[1], p[2])))
}

#[test]
fn a_sketch_on_a_flange_face_attaches_to_its_panel() {
    // Sketch coordinates on a sheet face are the flat pattern's: the flange panel starts at
    // y = 5 − BA and runs on toward −y, so a hole 7 mm up the face is at y = 5 − BA − 7.
    let ba = FRAC_PI_2 * (2.5 + 0.44 * 2.5);
    let hole = PI * 1.8 * 1.8 * 2.5;
    let mut s = Session::default();
    plate_with_flange(&mut s);
    let v0 = volume(&mut s);
    let made = circle_sketch(&mut s, "Holes", json!({"face": [50, 2.5, 12]}), [30.0, 5.0 - ba - 7.0], 3.6);
    assert_eq!(made["attachment"], "sheet_face");
    // The sketch keeps the face's name: the body, the panel (its flange) and the side of the sheet.
    let sketch = s.doc.find_feature("Holes").cloned().unwrap();
    let solidcraft_doc::FeatureKind::Sketch { plane, .. } = &sketch.kind else { panic!("{:?}", sketch.kind) };
    let name = solidcraft_doc::sheet::plane_sheet_face(plane).unwrap_or_default();
    assert!(name.starts_with("sheet:Body1:F") && name.ends_with(":top"), "{name}");
    // A cut through the flange from that sketch is a round hole 7 mm up the face, at x = 30.
    run(&mut s, "solid.extrude", json!({"sketch": "Holes", "distance": 2.5, "direction": "negative", "operation": "cut"}));
    assert!((v0 - volume(&mut s) - hole).abs() < 0.1, "{}", v0 - volume(&mut s));
    assert!(!contains(&s, [30.0, 1.25, 12.0]) && contains(&s, [30.0, 1.25, 14.5]) && contains(&s, [33.0, 1.25, 12.0]));
    let f = flat(&mut s);
    let cut = f["cutouts"][0].as_array().cloned().unwrap_or_default();
    assert!(!cut.is_empty(), "{f}");
    for p in &cut {
        let (x, y) = (p[0].as_f64().unwrap_or(f64::NAN), p[1].as_f64().unwrap_or(f64::NAN));
        assert!(((x - 30.0).hypot(y - (5.0 - ba - 7.0)) - 1.8).abs() < 1e-3, "({x}, {y})");
    }
    // `fixed: true` still freezes the picked plane instead of attaching to the panel.
    let frozen = circle_sketch(&mut s, "Frozen", json!({"face": [50, 2.5, 12], "fixed": true}), [0.0, 0.0], 1.0);
    assert_eq!(frozen["attachment"], "fixed");
}

/// The thickness of a sheet comes from a parameter: the flange's face moves with it, and a
/// sketch frozen on the old plane cuts a blind pocket in the thicker sheet. The attached
/// sketch cuts the same hole as one drawn on the thicker sheet in the first place.
#[test]
fn a_sketch_on_a_sheet_face_follows_the_sheet_when_its_thickness_changes() {
    let build = |t: f64, then: Option<f64>| {
        let mut s = Session::default();
        run(&mut s, "parameters.add", json!({"name": "T", "expression": format!("{t} mm")}));
        run(&mut s, "sheet.manage_rules", json!({"name": "AL", "thickness": "T", "k_factor": "0.5", "bend_radius": "1 mm", "active": true}));
        base(&mut s, 100.0, 60.0);
        run(&mut s, "sheet.flange", json!({"edges": [[50, 0, t]], "height": 20}));
        // The hole is drawn 10 mm up the 2 mm flange, in the flat pattern: its bend ends at
        // y = T + R = 3 and the bend allowance is (R + T/2)·π/2 = π. The coordinates stay as they are.
        let flat_y = 3.0 - PI - 10.0;
        circle_sketch(&mut s, "Holes", json!({"face": [50, t, 12]}), [30.0, flat_y], 6.0);
        run(&mut s, "sketch.create", json!({"plane": {"face": [10, 30, t]}, "offset": "5 mm", "name": "Above"}));
        run(&mut s, "sketch.finish", json!({}));
        let v0 = volume(&mut s);
        cut_through(&mut s, "Holes");
        if let Some(t2) = then {
            let r = run(&mut s, "parameters.change", json!({"name": "T", "expression": format!("{t2} mm")}));
            assert_eq!(r["errors"], json!([]), "{r}");
        }
        (s, v0)
    };
    let (mut grown, _) = build(2.0, Some(3.0));
    let (mut direct, v_direct) = build(3.0, None);
    let (vg, vd) = (volume(&mut grown), volume(&mut direct));
    // The hole is the cylinder Ø6 × 3 through the thicker sheet, as if drawn there at first.
    assert!((v_direct - vd - PI * 9.0 * 3.0).abs() < 0.1, "{}", v_direct - vd);
    assert!(rel(vg, vd) < 1e-9, "{vg} {vd}");
    let (fg, fd) = (flat(&mut grown), flat(&mut direct));
    assert_eq!(fg["pockets"], json!([]), "a blind pocket instead of the hole: {fg}");
    assert_eq!(fg["cutouts"], fd["cutouts"]);
    // The thicker flange's panel starts at flat y = 4 − 3.927 and z = T + R = 4, so the hole is
    // 10.215 mm up it: through the whole 3 mm.
    assert!(!contains(&grown, [30.0, 1.5, 14.2]) && !contains(&grown, [30.0, 0.2, 14.2]) && contains(&grown, [30.0, 1.5, 19.0]));
    // The sketch offset from the base's top face moved up with it: 3 + 5.
    let plane = run(&mut grown, "sketch.inspect", json!({"sketch": "Above"}))["plane"]["origin"][2].as_f64().unwrap_or(f64::NAN);
    assert!((plane - 8.0).abs() < 1e-9, "{plane}");
}

#[test]
fn sketches_on_the_top_and_bottom_faces_of_a_sheet_survive_unfold_refold_and_reopen() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    // The top face's coordinates are the base sketch's; the bottom face's mirror y, as seen from below.
    circle_sketch(&mut s, "Top", json!({"face": [30, 20, 2.5]}), [30.0, 20.0], 4.0);
    circle_sketch(&mut s, "Bottom", json!({"face": [60, 20, 0]}), [60.0, -20.0], 4.0);
    cut_through(&mut s, "Top");
    cut_through(&mut s, "Bottom");
    let want = 15000.0 - 2.0 * PI * 4.0 * 2.5;
    assert!((volume(&mut s) - want).abs() < 0.1, "{}", volume(&mut s));
    let holes = |f: &Value| {
        let mut centres: Vec<(f64, f64)> = f["cutouts"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|c| {
                let pts: Vec<(f64, f64)> = c
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|p| (p[0].as_f64().unwrap_or(f64::NAN), p[1].as_f64().unwrap_or(f64::NAN)))
                    .collect();
                let n = pts.len().max(1) as f64;
                (pts.iter().map(|p| p.0).sum::<f64>() / n, pts.iter().map(|p| p.1).sum::<f64>() / n)
            })
            .collect();
        centres.sort_by(|a, b| a.0.total_cmp(&b.0));
        centres
    };
    for command in [None, Some("sheet.unfold"), Some("sheet.refold")] {
        if let Some(command) = command {
            run(&mut s, command, json!({}));
        }
        let centres = holes(&flat(&mut s));
        assert_eq!(centres.len(), 2, "{centres:?}");
        assert!((centres[0].0 - 30.0).abs() < 0.01 && (centres[0].1 - 20.0).abs() < 0.01, "{centres:?}");
        assert!((centres[1].0 - 60.0).abs() < 0.01 && (centres[1].1 - 20.0).abs() < 0.01, "{centres:?}");
        assert!((volume(&mut s) - want).abs() < 0.1);
    }
    let mut reopened = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
    assert!((volume(&mut reopened) - want).abs() < 0.1);
    assert_eq!(holes(&flat(&mut reopened)), holes(&flat(&mut s)));
}

#[test]
fn a_sketch_made_on_the_unfolded_sheet_lands_on_the_flange_after_refolding() {
    let ba = FRAC_PI_2 * (2.5 + 0.44 * 2.5);
    let mut s = Session::default();
    plate_with_flange(&mut s);
    let v0 = volume(&mut s);
    run(&mut s, "sheet.unfold", json!({}));
    // Unfolded, the flange lies in the sheet's plane: the one flat face covers it too.
    let y = 5.0 - ba - 7.0;
    circle_sketch(&mut s, "Holes", json!({"face": [30, y, 2.5]}), [30.0, y], 3.6);
    cut_through(&mut s, "Holes");
    run(&mut s, "sheet.refold", json!({}));
    assert!((v0 - volume(&mut s) - PI * 1.8 * 1.8 * 2.5).abs() < 0.1, "{}", v0 - volume(&mut s));
    assert!(!contains(&s, [30.0, 1.25, 12.0]) && contains(&s, [30.0, 1.25, 14.5]));
}

#[test]
fn a_sketch_on_a_sheet_face_reports_a_lost_attachment_instead_of_moving_to_another_face() {
    let mut s = Session::default();
    plate_with_flange(&mut s);
    circle_sketch(&mut s, "Holes", json!({"face": [50, 2.5, 12]}), [30.0, -8.0], 3.6);
    let id = s.doc.find_feature("Holes").map(|f| f.id).unwrap_or(0);
    let status = |s: &Session| s.placed_document().sketch_face_attachment(id, &s.model);
    assert!(matches!(status(&s), solidcraft_doc::FaceAttachmentStatus::Sheet { plane: Some(_) }));
    // With the flange suppressed there is no such face; the base's top face is not a substitute.
    run(&mut s, "timeline.suppress", json!({"feature": "EdgeFlange1", "suppressed": true}));
    let error = run(&mut s, "timeline.inspect", json!({"feature": "Holes"}))["error"].as_str().unwrap_or_default().to_string();
    assert!(error.contains("lost face attachment") && error.contains("reattach"), "{error}");
    let solidcraft_doc::FaceAttachmentStatus::Lost { message, repair_plane } = status(&s) else { panic!("{:?}", status(&s)) };
    assert!(message.contains("sheet face") && repair_plane.is_some(), "{message}");
    // Redefine Sketch Plane repairs it on another face, here the base's top (its sketch keeps its geometry).
    let repaired = run(&mut s, "sketch.redefine", json!({"sketch": "Holes", "plane": {"face": [10, 30, 2.5]}}));
    assert_eq!(repaired["attachment"], "sheet_face");
    assert!(run(&mut s, "timeline.inspect", json!({"feature": "Holes"}))["error"].is_null());
    assert!(matches!(status(&s), solidcraft_doc::FaceAttachmentStatus::Sheet { .. }));
    // The flange is back, and a sketch attached to it again is healthy.
    run(&mut s, "timeline.suppress", json!({"feature": "EdgeFlange1", "suppressed": false}));
    let again = run(&mut s, "sketch.redefine", json!({"sketch": "Holes", "plane": {"face": [50, 2.5, 12]}}));
    assert_eq!(again["attachment"], "sheet_face");
    assert!(run(&mut s, "timeline.inspect", json!({"feature": "Holes"}))["error"].is_null());
    assert!(matches!(status(&s), solidcraft_doc::FaceAttachmentStatus::Sheet { .. }));
}

#[test]
fn renaming_a_sheet_body_keeps_its_sketches_on_their_faces() {
    let mut s = Session::default();
    plate_with_flange(&mut s);
    let v0 = volume(&mut s);
    circle_sketch(&mut s, "Holes", json!({"face": [50, 2.5, 12]}), [30.0, -8.0], 3.6);
    run(&mut s, "sketch.create", json!({"plane": {"face": [10, 30, 2.5]}, "offset": "5 mm", "name": "Above"}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "body.rename", json!({"body": "Body1", "name": "Bracket"}));
    for (sketch, face) in [("Holes", "sheet:Bracket:F"), ("Above", "sheet:Bracket:base:top")] {
        let feature = s.doc.find_feature(sketch).cloned().unwrap();
        let solidcraft_doc::FeatureKind::Sketch { plane, .. } = &feature.kind else { panic!("{:?}", feature.kind) };
        assert!(solidcraft_doc::sheet::plane_sheet_face(plane).is_some_and(|name| name.starts_with(face)), "{sketch}: {:?}", feature.kind);
        assert!(run(&mut s, "timeline.inspect", json!({"feature": sketch}))["error"].is_null(), "{sketch}");
    }
    cut_through(&mut s, "Holes");
    assert!((v0 - volume(&mut s) - PI * 1.8 * 1.8 * 2.5).abs() < 0.1, "{}", v0 - volume(&mut s));
}

/// A tray shrunk by a parameter change unfolds like the same tray built at that size. The
/// sketch solved again leaves its corners ~1e-13 mm off, and the flat pattern's planar
/// arrangement used to drop a bend zone through a gap that small: "planar region arrangement
/// failed", for W alone or both sizes, at 72° or 90°, while growing the tray worked.
#[test]
fn tray_shrunk_by_a_parameter_unfolds_like_one_built_at_that_size() {
    for angle in [72.0, 90.0] {
        let tray = |s: &mut Session, w: f64, h: f64| {
            run(s, "parameters.add", json!({"name": "W", "expression": format!("{w} mm")}));
            run(s, "parameters.add", json!({"name": "H", "expression": format!("{h} mm")}));
            run(s, "sheet.manage_rules", json!({"name": "AL2", "thickness": "2 mm", "k_factor": "0.5", "bend_radius": "1 mm", "active": true}));
            run(s, "sketch.create", json!({"plane": "XY", "name": "Tray"}));
            run(s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [w, h]}));
            run(s, "sketch.constraint.fix", json!({"entities": ["p1"]}));
            run(s, "sketch.dimension", json!({"entities": ["l1"], "value": "W"}));
            run(s, "sketch.dimension", json!({"entities": ["l2"], "value": "H"}));
            run(s, "sketch.finish", json!({}));
            run(s, "sheet.flange", json!({"type": "base", "sketch": "Tray"}));
            let edges = json!([[w / 2.0, 0, 2], [w, h / 2.0, 2], [w / 2.0, h, 2], [0, h / 2.0, 2]]);
            run(s, "sheet.flange", json!({"type": "edge", "edges": edges, "height": 38.6, "angle": angle}));
            run(s, "sheet.unfold", json!({}));
        };
        let mut direct = Session::default();
        tray(&mut direct, 400.0, 300.0);
        let want = flat(&mut direct)["flat_size_mm"].clone();
        let mut shrunk = Session::default();
        tray(&mut shrunk, 2000.0, 2500.0);
        for (name, expr) in [("W", "400 mm"), ("H", "300 mm")] {
            let r = run(&mut shrunk, "parameters.change", json!({"name": name, "expression": expr}));
            assert_eq!(r["errors"], json!([]), "{angle}° {name} = {expr}: {r}");
        }
        let got = flat(&mut shrunk)["flat_size_mm"].clone();
        for k in 0..3 {
            assert!((got[k].as_f64().unwrap() - want[k].as_f64().unwrap()).abs() < 1e-9, "{angle}°: {got} {want}");
        }
        assert!(rel(volume(&mut shrunk), volume(&mut direct)) < 1e-9, "{angle}°");
    }
}

#[test]
fn bend_table_exports_shop_dimensions_and_tracks_parameter_changes() {
    let mut s = Session::default();
    run(&mut s, "sheet.manage_rules", json!({"name": "Shop, \"steel\"", "thickness": 2, "bend_radius": 3, "k_factor": 0.4}));
    base(&mut s, 100.0, 60.0);
    run(&mut s, "sheet.flange", json!({"edges": [[50, 0, 2]], "height": 20}));
    let undo = s.undo.len();
    let csv = run(&mut s, "sheet.export_bend_table", json!({}));
    let text = csv["text"].as_str().unwrap();
    assert!(text.starts_with("body,rule,thickness_mm,k_factor,bend,angle_deg,direction,"));
    assert!(text.contains("\"Shop, \"\"steel\"\"\""), "{text}");
    assert_eq!(text.lines().count(), 2);
    assert_eq!(csv["bends"], 1);
    let exported = run(&mut s, "sheet.export_bend_table", json!({"format": "JSON"}));
    let table: Value = serde_json::from_str(exported["text"].as_str().unwrap()).unwrap();
    assert_eq!(table["thickness"], 2.0);
    assert_eq!(table["bends"][0]["direction"], "up");
    assert!((table["bends"][0]["allowance"].as_f64().unwrap() - FRAC_PI_2 * 3.8).abs() < 1e-9);
    assert_eq!(s.undo.len(), undo, "export does not create an undo step");
    run(&mut s, "sheet.manage_rules", json!({"name": "Shop, \"steel\"", "k_factor": 0.5}));
    let updated = run(&mut s, "sheet.export_bend_table", json!({"format": "json"}));
    let table: Value = serde_json::from_str(updated["text"].as_str().unwrap()).unwrap();
    assert!((table["bends"][0]["allowance"].as_f64().unwrap() - FRAC_PI_2 * 4.0).abs() < 1e-9);
    assert!(s.execute("sheet.export_bend_table", &json!({"format": "xml"})).is_err());
    assert!(s.execute("sheet.export_bend_table", &json!({"body": "Missing"})).is_err());
    let mut empty = Session::default();
    assert!(empty.execute("sheet.export_bend_table", &json!({})).is_err());
}

#[test]
fn malformed_fold_points_are_rejected_atomically() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    let before = serde_json::to_value(&s.doc).unwrap();
    for points in [
        json!([[40, 0, 2.5], null, [40, 60, 2.5]]),
        json!([[40, 0, 2.5], [40, 60, 2.5], "junk"]),
        json!([[40, 0, 2.5], [40, 60]]),
        json!([[40, 0, 2.5]]),
        json!("not points"),
    ] {
        assert!(s.execute("sheet.fold", &json!({"points": points})).is_err());
        assert_eq!(serde_json::to_value(&s.doc).unwrap(), before);
    }
}

#[test]
fn flat_dxf_preserves_exact_cutout_entities() {
    use solidcraft_geom::{Loop2, Region2, Seg2, Vec2};
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    let mut sh = s.model.state().sheets[0].clone();
    sh.holes = vec![Region2 {
        outer: Loop2 { segs: vec![Seg2::Arc { center: Vec2::new(40.0, 30.0), radius: 5.0, start: -PI, sweep: -2.0 * PI }] },
        holes: Vec::new(),
    }];
    let dxf = super::flat_dxf(&sh).unwrap();
    assert_eq!(dxf.matches("0\nCIRCLE\n").count(), 1, "{dxf}");
    assert!(!dxf.contains("0\nARC\n"), "a full circle must not become a zero-length ARC");
    let read = solidcraft_io::read_dxf(&dxf).unwrap();
    assert_eq!(read.len(), 5);
    assert!(read.iter().any(|g| matches!(g, solidcraft_io::Geom2::Circle(c, r) if *c == Vec2::new(40.0, 30.0) && (*r - 5.0).abs() < 1e-9)));
    sh.holes[0].outer = Loop2 {
        segs: vec![
            Seg2::Line { a: Vec2::new(10.0, 10.0), b: Vec2::new(30.0, 10.0) },
            Seg2::Arc { center: Vec2::new(20.0, 10.0), radius: 10.0, start: 0.0, sweep: PI },
        ],
    };
    let dxf = super::flat_dxf(&sh).unwrap();
    assert_eq!(dxf.matches("0\nLINE\n").count(), 5, "four outline lines and one exact cutout line");
    assert_eq!(dxf.matches("0\nARC\n").count(), 1);
    assert!(dxf.contains("50\n0.000000\n51\n180.000000\n"));
    let read = solidcraft_io::read_dxf(&dxf).unwrap();
    assert_eq!(read.len(), 6);
    assert!(read.iter().any(|g| matches!(g, solidcraft_io::Geom2::Arc { c, a, b } if *c == Vec2::new(20.0, 10.0) && a.dist(Vec2::new(30.0, 10.0)) < 1e-9 && b.dist(Vec2::new(10.0, 10.0)) < 1e-9)));
}

// A profile chosen by a point outside its inner loop must keep that inner loop through
// sheet construction, flattening and manufacturing export.
#[test]
fn inherited_base_profile_holes_survive_every_sheet_state() {
    for flip in [false, true] {
        let mut s = Session::default();
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Perforated"}));
        run(&mut s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [100, 60]}));
        run(&mut s, "sketch.rectangle.two_point", json!({"p0": [40, 20], "p1": [60, 40]}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "sheet.flange", json!({"sketch": "Perforated", "profiles": [{"point": [10, 10]}], "flip": flip}));
        assert!((volume(&mut s) - 14000.0).abs() < 1e-6);
        let f = flat(&mut s);
        assert_eq!(f["cutouts"].as_array().map(Vec::len), Some(1));
        let dxf = run(&mut s, "sheet.export_dxf", json!({}))["dxf"].as_str().unwrap().to_string();
        assert_eq!(solidcraft_io::read_dxf(&dxf).unwrap().len(), 8);
        let mut reopened = Session::default();
        run(&mut reopened, "sketch.insert_dxf", json!({"text": dxf, "name": "Blank"}));
        run(&mut reopened, "sketch.finish", json!({}));
        let pick = if flip { [10, -10] } else { [10, 10] };
        run(&mut reopened, "solid.extrude", json!({"sketch": "Blank", "profiles": [{"point": pick}], "distance": 2.5}));
        assert!((volume(&mut reopened) - 14000.0).abs() < 1e-5);
        run(&mut s, "sheet.unfold", json!({}));
        assert!((volume(&mut s) - 14000.0).abs() < 1e-6);
        run(&mut s, "sheet.refold", json!({}));
        assert!((volume(&mut s) - 14000.0).abs() < 1e-6);
    }
}

#[test]
fn edge_notch_uses_the_same_material_boundary_in_solid_flat_and_dxf() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Notch"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [-5, 20], "p1": [10, 40]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "solid.extrude", json!({"sketch": "Notch", "through_all": true, "operation": "cut", "direction": "symmetric"}));
    assert!((volume(&mut s) - 14500.0).abs() < 1e-6);
    let f = flat(&mut s);
    assert_eq!(f["outline"].as_array().map(Vec::len), Some(1));
    assert_eq!(f["outline"][0].as_array().map(Vec::len), Some(8));
    assert_eq!(f["cutouts"].as_array().map(Vec::len), Some(0));
    let dxf = run(&mut s, "sheet.export_dxf", json!({}))["dxf"].as_str().unwrap().to_string();
    assert_eq!(solidcraft_io::read_dxf(&dxf).unwrap().len(), 8);
    let mut reopened = Session::default();
    run(&mut reopened, "sketch.insert_dxf", json!({"text": dxf, "name": "Blank"}));
    run(&mut reopened, "sketch.finish", json!({}));
    run(&mut reopened, "solid.extrude", json!({"sketch": "Blank", "distance": 2.5}));
    assert!((volume(&mut reopened) - 14500.0).abs() < 1e-5);
    run(&mut s, "sheet.unfold", json!({}));
    assert!((volume(&mut s) - 14500.0).abs() < 1e-6);
    run(&mut s, "sheet.refold", json!({}));
    assert!((volume(&mut s) - 14500.0).abs() < 1e-6);
}

#[test]
fn inherited_round_hole_survives_flange_and_unfold_in_flipped_frames() {
    for flip in [false, true] {
        let mut s = Session::default();
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Perforated"}));
        run(&mut s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [100, 60]}));
        run(&mut s, "sketch.circle.center", json!({"center": [50, 30], "radius": 5}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "sheet.flange", json!({"sketch": "Perforated", "profiles": [{"point": [10, 10]}], "flip": flip}));
        run(&mut s, "sheet.flange", json!({"edges": [[50, 0, if flip { -2.5 } else { 2.5 }]], "height": 20}));
        let folded = volume(&mut s);
        assert!((folded - (18972.62196081845 - PI * 25.0 * 2.5)).abs() < 0.2, "{folded}");
        let f = flat(&mut s);
        assert_eq!(f["cutouts"].as_array().map(Vec::len), Some(1));
        let sh = s.model.state().sheets[0].clone();
        let regions = sh.flat_regions().unwrap();
        assert_eq!(regions[0].holes[0].segs.len(), 2);
        assert!(regions[0].holes[0].segs.iter().all(|s| matches!(s, solidcraft_geom::Seg2::Arc { radius, .. } if (*radius - 5.0).abs() < 1e-9)));
        let expected = regions.iter().map(solidcraft_geom::Region2::area).sum::<f64>() * 2.5;
        run(&mut s, "sheet.unfold", json!({}));
        assert!((volume(&mut s) - expected).abs() < 0.1);
        run(&mut s, "sheet.refold", json!({}));
        assert!((volume(&mut s) - folded).abs() < 0.1);
    }
}

#[test]
fn crossing_only_disconnecting_cut_preserves_all_material_states() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Split"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [45, -5], "p1": [55, 65]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "solid.extrude", json!({"sketch": "Split", "through_all": true, "operation": "cut", "direction": "symmetric"}));
    assert!((volume(&mut s) - 13500.0).abs() < 1e-6);
    assert_eq!(flat(&mut s)["outline"].as_array().map(Vec::len), Some(2));
    let dxf = run(&mut s, "sheet.export_dxf", json!({}))["dxf"].as_str().unwrap().to_string();
    let mut reopened = Session::default();
    run(&mut reopened, "sketch.insert_dxf", json!({"text": dxf, "name": "SplitBlank"}));
    run(&mut reopened, "sketch.finish", json!({}));
    run(&mut reopened, "solid.extrude", json!({"sketch": "SplitBlank", "distance": 2.5}));
    assert!((volume(&mut reopened) - 13500.0).abs() < 1e-6);
    run(&mut s, "sheet.unfold", json!({}));
    assert!((volume(&mut s) - 13500.0).abs() < 1e-6);
    run(&mut s, "sheet.refold", json!({}));
    assert!((volume(&mut s) - 13500.0).abs() < 1e-6);
}

#[test]
fn splitting_an_unfolded_flanged_sheet_preserves_both_bent_pieces() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    run(&mut s, "sheet.flange", json!({"edges": [[50,0,2.5]], "height": 20}));
    let folded = volume(&mut s);
    run(&mut s, "sheet.unfold", json!({}));
    let unfolded = volume(&mut s);
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "BlankSplit"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [45,-100], "p1": [55,100]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "solid.extrude", json!({"sketch": "BlankSplit", "through_all": true, "operation": "cut", "direction": "symmetric"}));
    assert!((volume(&mut s) - unfolded * 0.9).abs() < 0.1);
    assert_eq!(flat(&mut s)["outline"].as_array().map(Vec::len), Some(2));
    run(&mut s, "sheet.refold", json!({}));
    assert!((volume(&mut s) - folded * 0.9).abs() < 0.2);
    run(&mut s, "sheet.unfold", json!({}));
    assert!((volume(&mut s) - unfolded * 0.9).abs() < 0.1);
}

#[test]
fn repeated_and_nested_sheet_cuts_are_redundant() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    for (i, radius) in [3.0, 5.0, 5.0].into_iter().enumerate() {
        let name = format!("Hole{i}");
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": name}));
        run(&mut s, "sketch.circle.center", json!({"center": [20, 30], "radius": radius}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "solid.extrude", json!({"sketch": name, "through_all": true, "operation": "cut", "direction": "symmetric"}));
    }
    assert!((volume(&mut s) - (6000.0 - PI * 25.0) * 2.5).abs() < 0.1);
    assert_eq!(flat(&mut s)["cutouts"].as_array().map(Vec::len), Some(1));
}

#[test]
fn overlapping_round_cuts_keep_exact_union_in_export_and_roundtrip() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    for (i, x) in [20.0, 22.0].into_iter().enumerate() {
        let name = format!("Overlap{i}");
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": name}));
        run(&mut s, "sketch.circle.center", json!({"center": [x, 30], "radius": 5}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "solid.extrude", json!({"sketch": name, "through_all": true, "operation": "cut", "direction": "symmetric"}));
    }
    let intersection = 50.0 * (0.2f64).acos() - (96.0f64).sqrt();
    let expected = (6000.0 - (50.0 * PI - intersection)) * 2.5;
    assert!((volume(&mut s) - expected).abs() < 0.1);
    assert_eq!(flat(&mut s)["cutouts"].as_array().map(Vec::len), Some(1));
    let dxf = run(&mut s, "sheet.export_dxf", json!({}))["dxf"].as_str().unwrap().to_string();
    assert!(dxf.contains("0\nARC\n"));
    let mut reopened = Session::default();
    run(&mut reopened, "sketch.insert_dxf", json!({"text": dxf, "name": "Blank"}));
    run(&mut reopened, "sketch.finish", json!({}));
    run(&mut reopened, "solid.extrude", json!({"sketch": "Blank", "profiles": [{"point": [10, 10]}], "distance": 2.5}));
    assert!((volume(&mut reopened) - expected).abs() < 0.1);
    run(&mut s, "sheet.unfold", json!({}));
    assert!((volume(&mut s) - expected).abs() < 0.1);
    run(&mut s, "sheet.refold", json!({}));
    assert!((volume(&mut s) - expected).abs() < 0.1);
}

#[test]
fn cut_across_a_concave_notch_uses_crossings_not_just_contained_corners() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "U"}));
    run(&mut s, "sketch.line", json!({"points": [[0,0],[100,0],[100,60],[70,60],[70,20],[30,20],[30,60],[0,60],[0,0]]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "sheet.flange", json!({"sketch": "U"}));
    assert!((volume(&mut s) - 11000.0).abs() < 1e-6);
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "ArmNotches"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [20,30], "p1": [80,40]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "solid.extrude", json!({"sketch": "ArmNotches", "through_all": true, "operation": "cut", "direction": "symmetric"}));
    assert!((volume(&mut s) - 10500.0).abs() < 1e-6);
    let f = flat(&mut s);
    assert_eq!(f["cutouts"].as_array().map(Vec::len), Some(0));
    run(&mut s, "sheet.unfold", json!({}));
    assert!((volume(&mut s) - 10500.0).abs() < 1e-6);
}

#[test]
fn tiny_cut_inside_exact_circle_survives_unfold_refold_and_dxf() {
    for initially_flat in [false, true] {
        let mut s = Session::default();
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Disc"}));
        run(&mut s, "sketch.circle.center", json!({"center": [0,0], "radius": 10}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "sheet.flange", json!({"sketch": "Disc"}));
        let circle = s.model.state().sheets[0].base.outer.clone();
        let displayed = circle.polyline(1e-3);
        let at = (displayed[0] + displayed[1]).normalized().unwrap() * 9.9998;
        assert!(!circle.contains(at));
        if initially_flat {
            run(&mut s, "sheet.unfold", json!({}));
        }
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": "TinyCut"}));
        run(&mut s, "sketch.rectangle.two_point", json!({"p0": [at.x-0.000025,at.y-0.000025], "p1": [at.x+0.000025,at.y+0.000025]}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "solid.extrude", json!({"sketch": "TinyCut", "through_all": true, "operation": "cut", "direction": "symmetric"}));
        for stage in ["sheet.unfold", "sheet.refold"] {
            let regions = s.model.state().sheets[0].flat_regions().unwrap();
            assert_eq!(regions[0].holes.len(), 1);
            assert!((regions[0].holes[0].signed_area().abs() - 2.5e-9).abs() < 1e-13);
            assert_eq!(s.model.state().bodies.len(), 1);
            let dxf = run(&mut s, "sheet.export_dxf", json!({}))["dxf"].as_str().unwrap().to_string();
            assert_eq!(solidcraft_io::read_dxf(&dxf).unwrap().len(), 5);
            if stage == "sheet.unfold" && initially_flat {
                run(&mut s, "sheet.refold", json!({}));
            }
            run(&mut s, stage, json!({}));
        }
        assert_eq!(s.model.state().sheets[0].flat_regions().unwrap()[0].holes.len(), 1);
    }
}

#[test]
fn unrelated_box_cut_preserves_logical_ring_sheet_compound() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    let sheet_name = s.model.state().sheets[0].body.clone();
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Ring"}));
    for radius in [10, 5] {
        run(&mut s, "sketch.circle.center", json!({"center": [50,30], "radius": radius}));
    }
    run(&mut s, "sketch.finish", json!({}));
    run(
        &mut s,
        "solid.extrude",
        json!({"sketch": "Ring", "profiles": [{"point": [57,30]}], "through_all": true, "operation": "cut", "direction": "symmetric"}),
    );
    let material = s.model.state().sheets[0].flat_regions().unwrap();
    run(&mut s, "solid.box", json!({"length": 20, "width": 20, "height": 10, "corner": [200,0,0], "body_name": "SeparateBox"}));
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "BoxCut"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [205,5], "p1": [210,10]}));
    run(&mut s, "sketch.finish", json!({}));
    run(
        &mut s,
        "solid.extrude",
        json!({"sketch": "BoxCut", "participants": ["SeparateBox"], "through_all": true, "operation": "cut", "direction": "symmetric"}),
    );
    assert_eq!(s.model.state().bodies.len(), 2);
    assert_eq!(s.model.state().sheets[0].flat_regions().unwrap(), material);
    for stage in ["sheet.unfold", "sheet.refold"] {
        run(&mut s, stage, json!({"body": sheet_name}));
        assert_eq!(s.model.state().bodies.len(), 2);
        let state = s.model.state();
        let body = &state.bodies.iter().find(|b| b.name == sheet_name).unwrap().body;
        assert!(body.contains(solidcraft_geom::Vec3::new(50.0, 30.0, 1.25)));
        assert!(!body.contains(solidcraft_geom::Vec3::new(57.0, 30.0, 1.25)));
    }
}

#[test]
fn specialized_sheet_cuts_respect_component_and_explicit_participants() {
    for explicit in [false, true] {
        let mut s = Session::default();
        base(&mut s, 100.0, 60.0);
        let name = s.model.state().sheets[0].body.clone();
        run(&mut s, "component.create", json!({"name": "Other"}));
        run(&mut s, "solid.box", json!({"length": 100, "width": 60, "height": 10, "body_name": "OtherBox"}));
        if explicit {
            run(&mut s, "component.activate", json!({"component": "root"}));
        }
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Cut"}));
        run(&mut s, "sketch.rectangle.two_point", json!({"p0": [20,20], "p1": [30,30]}));
        run(&mut s, "sketch.finish", json!({}));
        let mut params = json!({"sketch": "Cut", "through_all": true, "operation": "cut", "direction": "symmetric"});
        if explicit {
            params["participants"] = json!(["OtherBox"]);
        }
        run(&mut s, "solid.extrude", params);
        let m = run(&mut s, "inspect.measure", json!({"bodies": [name]}));
        assert!((m["total"]["volume_mm3"].as_f64().unwrap() - 15000.0).abs() < 1e-6);
        assert!(s.model.state().sheets[0].holes.is_empty());
        let m = run(&mut s, "inspect.measure", json!({"bodies": ["OtherBox"]}));
        assert!((m["total"]["volume_mm3"].as_f64().unwrap() - 59000.0).abs() < 1e-6);
    }
}

#[test]
fn ring_cut_preserves_retained_island_through_unfold_and_refold() {
    fn material_probes(s: &Session) {
        let body = &s.model.state().bodies[0].body;
        assert!(body.contains(solidcraft_geom::Vec3::new(50.0, 30.0, 1.25)));
        assert!(!body.contains(solidcraft_geom::Vec3::new(57.0, 30.0, 1.25)));
        assert!(body.contains(solidcraft_geom::Vec3::new(65.0, 30.0, 1.25)));
    }
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Ring"}));
    run(&mut s, "sketch.circle.center", json!({"center": [50,30], "radius": 10}));
    run(&mut s, "sketch.circle.center", json!({"center": [50,30], "radius": 5}));
    run(&mut s, "sketch.finish", json!({}));
    run(
        &mut s,
        "solid.extrude",
        json!({"sketch": "Ring", "profiles": [{"point": [57,30]}], "through_all": true, "operation": "cut", "direction": "symmetric"}),
    );
    let expected = (6000.0 - PI * 75.0) * 2.5;
    assert!((volume(&mut s) - expected).abs() < 0.2);
    material_probes(&s);
    assert_eq!(flat(&mut s)["outline"].as_array().map(Vec::len), Some(2));
    let regions = s.model.state().sheets[0].flat_regions().unwrap();
    assert!(regions.iter().any(|r| r.contains(solidcraft_geom::Vec2::new(50.0, 30.0))));
    assert!(!regions.iter().any(|r| r.contains(solidcraft_geom::Vec2::new(57.0, 30.0))));
    assert!(regions.iter().any(|r| r.contains(solidcraft_geom::Vec2::new(65.0, 30.0))));
    let dxf = run(&mut s, "sheet.export_dxf", json!({}))["dxf"].as_str().unwrap().to_string();
    let mut reopened = Session::default();
    run(&mut reopened, "sketch.insert_dxf", json!({"text": dxf, "name": "RingBlank"}));
    run(&mut reopened, "sketch.finish", json!({}));
    run(&mut reopened, "solid.extrude", json!({"sketch": "RingBlank", "profiles": [{"point": [10,10]}, {"point": [50,30]}], "distance": 2.5}));
    assert!((volume(&mut reopened) - expected).abs() < 0.2);
    run(&mut s, "sheet.unfold", json!({}));
    assert!((volume(&mut s) - expected).abs() < 0.2);
    material_probes(&s);
    run(&mut s, "sheet.refold", json!({}));
    assert!((volume(&mut s) - expected).abs() < 0.2);
    material_probes(&s);
}

fn retained_depth_check(s: &mut Session, want: f64, z0: f64, z1: f64) {
    use solidcraft_geom::Vec3;
    assert!((volume(s) - want).abs() < 1e-6, "{} vs {want}", volume(s));
    let state = s.model.state();
    assert_eq!(state.sheets.len(), 1);
    assert_eq!(state.sheets[0].t, 2.5, "rule stock gauge is preserved");
    assert!(state.bodies.iter().any(|b| b.body.contains(Vec3::new(25.0, 25.0, (z0 + z1) * 0.5))));
    assert!(!state.bodies.iter().any(|b| b.body.contains(Vec3::new(25.0, 25.0, z0 - 0.1))));
    assert!(!state.bodies.iter().any(|b| b.body.contains(Vec3::new(25.0, 25.0, z1 + 0.1))));
    assert!(!state.bodies.iter().any(|b| b.body.contains(Vec3::new(10.0, 25.0, (z0 + z1) * 0.5))));
    let bounds = state.bodies[0].mesh().bounds();
    assert!((bounds.min.z - z0).abs() < 1e-8 && (bounds.max.z - z1).abs() < 1e-8, "{bounds:?}");
}

fn depth_step_reopen(s: &mut Session, suffix: &str, want: f64, inside: solidcraft_geom::Vec3, outside: solidcraft_geom::Vec3) {
    let path = std::env::temp_dir().join(format!("solidcraft-depth-intersect-{suffix}-{}.step", std::process::id()));
    run(s, "file.export", json!({"path":path.to_string_lossy()}));
    let mut imported = Session::default();
    run(&mut imported, "doc.open", json!({"path":path.to_string_lossy()}));
    assert!((volume(&mut imported) - want).abs() < 1e-6);
    assert!(imported.model.state().bodies.iter().any(|b| b.body.contains(inside)));
    assert!(!imported.model.state().bodies.iter().any(|b| b.body.contains(outside)));
    std::fs::remove_file(path).unwrap();
}

/// The generic sketch importer intentionally drops DXF layers/depth. Reimport only the
/// material outline at a separately known depth, never pretend all entities reconstruct3D.
fn dxf_outline_only(text: &str) -> String {
    let lines: Vec<_> = text.lines().collect();
    let pairs: Vec<_> = lines.as_chunks::<2>().0.iter().filter_map(|p| Some((p.first()?.trim(), *p.get(1)?))).collect();
    let mut out = String::from("0\nSECTION\n2\nENTITIES\n");
    let mut start = 0;
    for end in 1..=pairs.len() {
        if end == pairs.len() || pairs.get(end).is_some_and(|p| p.0 == "0") {
            let entity = &pairs[start..end];
            if entity.first().is_some_and(|p| matches!(p.1, "LINE" | "ARC" | "CIRCLE")) && entity.iter().any(|p| p.0 == "8" && p.1 == "OUTLINE") {
                for (code, value) in entity {
                    out.push_str(&format!("{code}\n{value}\n"));
                }
            }
            start = end;
        }
    }
    out.push_str("0\nENDSEC\n0\nEOF\n");
    out
}

#[test]
fn partial_depth_intersect_retains_bottom_top_and_middle_material_through_history_and_exports() {
    use solidcraft_geom::Vec3;
    for (offset, z0, z1, layers) in [
        (0.0, 0.0, 1.0, vec![("top", 1.0, 2.5)]),
        (1.5, 1.5, 2.5, vec![("bottom", 0.0, 1.5)]),
        (0.5, 0.5, 1.5, vec![("bottom", 0.0, 0.5), ("top", 1.5, 2.5)]),
    ] {
        let mut s = Session::default();
        base(&mut s, 100.0, 60.0);
        rectangle_tool(&mut s, "RetainDepth", [20.0, 20.0], [30.0, 30.0]);
        run(&mut s, "solid.extrude", json!({"sketch":"RetainDepth","distance":1,"start_offset":offset,"operation":"intersect","name":"RetainSlab"}));
        retained_depth_check(&mut s, 100.0, z0, z1);
        let exported = flat(&mut s);
        assert_eq!(exported["thickness"], 2.5);
        assert_eq!(exported["pockets"].as_array().unwrap().len(), layers.len());
        let dxf = run(&mut s, "sheet.export_dxf", json!({}))["dxf"].as_str().unwrap().to_string();
        for (side, a, b) in &layers {
            assert!(
                exported["pockets"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|p| p["side"] == *side && p["z0_mm"] == *a && p["z1_mm"] == *b && p["depth_mm"] == b - a)
            );
            assert!(dxf.contains(&format!("POCKET_{}_Z{a:.6}_TO_{b:.6}_MM", side.to_uppercase())));
        }
        let outline = dxf_outline_only(&dxf);
        assert_eq!(solidcraft_io::read_dxf(&outline).unwrap().len(), 4);
        let mut outline_import = Session::default();
        run(&mut outline_import, "sketch.insert_dxf", json!({"text":outline,"name":"Outline"}));
        run(&mut outline_import, "sketch.finish", json!({}));
        run(&mut outline_import, "solid.extrude", json!({"distance":1}));
        assert!((volume(&mut outline_import) - 100.0).abs() < 1e-6);
        run(&mut s, "edit.undo", json!({}));
        assert!((volume(&mut s) - 15000.0).abs() < 1e-6);
        run(&mut s, "edit.redo", json!({}));
        for command in ["sheet.unfold", "sheet.refold"] {
            run(&mut s, command, json!({}));
            retained_depth_check(&mut s, 100.0, z0, z1);
            assert_eq!(flat(&mut s)["pockets"], exported["pockets"]);
        }
        let mut reopened = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
        retained_depth_check(&mut reopened, 100.0, z0, z1);
        depth_step_reopen(&mut s, &format!("offset{offset}"), 100.0, Vec3::new(25.0, 25.0, (z0 + z1) * 0.5), Vec3::new(25.0, 25.0, z1 + 0.1));
    }
}

#[test]
fn partial_depth_intersect_preserves_prior_pockets_and_actual_interval_intersections() {
    use solidcraft_geom::Vec3;
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    rectangle_tool(&mut s, "OldPocket", [20.0, 20.0], [25.0, 30.0]);
    run(&mut s, "solid.extrude", json!({"sketch":"OldPocket","distance":1,"operation":"cut"}));
    rectangle_tool(&mut s, "RetainDepth", [20.0, 20.0], [30.0, 30.0]);
    run(&mut s, "solid.extrude", json!({"sketch":"RetainDepth","distance":1,"start_offset":0.5,"operation":"intersect"}));
    for command in [None, Some("sheet.unfold"), Some("sheet.refold")] {
        if let Some(command) = command {
            run(&mut s, command, json!({}));
        }
        assert!((volume(&mut s) - 75.0).abs() < 1e-6);
        let state = s.model.state();
        let body = &state.bodies[0].body;
        assert!(!body.contains(Vec3::new(22.0, 25.0, 0.75)));
        assert!(body.contains(Vec3::new(22.0, 25.0, 1.25)));
        assert!(body.contains(Vec3::new(27.0, 25.0, 0.75)));
        assert!(!body.contains(Vec3::new(27.0, 25.0, 0.25)));
    }
    let records = s.model.state().sheets[0].pockets.clone();
    run(&mut s, "solid.extrude", json!({"sketch":"RetainDepth","distance":1.5,"start_offset":0.25,"operation":"intersect"}));
    assert!((volume(&mut s) - 75.0).abs() < 1e-6);
    assert_eq!(s.model.state().sheets[0].pockets, records, "no-change leaves replay work unchanged");
    run(&mut s, "solid.extrude", json!({"sketch":"RetainDepth","distance":0.25,"start_offset":1.0,"operation":"intersect"}));
    assert!((volume(&mut s) - 25.0).abs() < 1e-6);
    run(&mut s, "sheet.unfold", json!({}));
    assert!((volume(&mut s) - 25.0).abs() < 1e-6);
    let mut cold = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
    assert!((volume(&mut cold) - 25.0).abs() < 1e-6);
    depth_step_reopen(&mut s, "prior", 25.0, Vec3::new(22.0, 25.0, 1.125), Vec3::new(22.0, 25.0, 0.75));
}

#[test]
fn partial_depth_intersect_keeps_disconnected_tool_islands_and_annular_curves() {
    use solidcraft_geom::Vec3;
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    rectangle_tool(&mut s, "Islands", [20.0, 20.0], [30.0, 30.0]);
    run(&mut s, "sketch.edit", json!({"sketch":"Islands"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0":[60,20],"p1":[70,30]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "solid.extrude", json!({"sketch":"Islands","distance":1,"operation":"intersect"}));
    for command in [None, Some("sheet.unfold"), Some("sheet.refold")] {
        if let Some(command) = command {
            run(&mut s, command, json!({}));
        }
        assert!((volume(&mut s) - 200.0).abs() < 1e-6);
        assert_eq!(s.model.state().sheets[0].flat_regions().unwrap().len(), 2);
        for x in [25.0, 65.0] {
            assert!(s.model.state().bodies[0].body.contains(Vec3::new(x, 25.0, 0.5)));
        }
        assert!(!s.model.state().bodies[0].body.contains(Vec3::new(45.0, 25.0, 0.5)));
    }
    depth_step_reopen(&mut s, "islands", 200.0, Vec3::new(65.0, 25.0, 0.5), Vec3::new(45.0, 25.0, 0.5));
    let mut ring = Session::default();
    base(&mut ring, 100.0, 60.0);
    run(&mut ring, "sketch.create", json!({"plane":"XY","name":"Ring"}));
    for radius in [10.0, 5.0] {
        run(&mut ring, "sketch.circle.center", json!({"center":[50,30],"radius":radius}));
    }
    run(&mut ring, "sketch.finish", json!({}));
    run(&mut ring, "solid.extrude", json!({"sketch":"Ring","profiles":[{"point":[57,30]}],"distance":1,"operation":"intersect"}));
    for command in [None, Some("sheet.unfold"), Some("sheet.refold")] {
        if let Some(command) = command {
            run(&mut ring, command, json!({}));
        }
        let state = ring.model.state();
        let area: f64 = state.sheets[0].flat_regions().unwrap().iter().map(|r| r.area()).sum();
        assert!((area - 75.0 * PI).abs() < 1e-8);
        assert!((volume(&mut ring) - 75.0 * PI).abs() < 0.1);
        assert!(state.bodies[0].body.contains(Vec3::new(57.0, 30.0, 0.5)));
        assert!(!state.bodies[0].body.contains(Vec3::new(50.0, 30.0, 0.5)));
        assert!(!state.bodies[0].body.contains(Vec3::new(57.0, 30.0, 1.5)));
    }
    let dxf = run(&mut ring, "sheet.export_dxf", json!({}))["dxf"].as_str().unwrap().to_string();
    assert!(dxf.contains("POCKET_TOP_Z1.000000_TO_2.500000_MM"));
    assert_eq!(solidcraft_io::read_dxf(&dxf_outline_only(&dxf)).unwrap().len(), 2);
}

#[test]
fn partial_depth_intersect_deletes_material_already_removed_by_a_pocket_atomically() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    rectangle_tool(&mut s, "Removed", [20.0, 20.0], [30.0, 30.0]);
    run(&mut s, "solid.extrude", json!({"sketch":"Removed","distance":1,"operation":"cut"}));
    run(&mut s, "solid.extrude", json!({"sketch":"Removed","distance":1,"operation":"intersect"}));
    assert!(s.model.state().bodies.is_empty());
    assert!(s.model.state().sheets.is_empty());
    run(&mut s, "edit.undo", json!({}));
    assert!((volume(&mut s) - 14900.0).abs() < 1e-6);
    run(&mut s, "edit.redo", json!({}));
    assert!(s.model.state().bodies.is_empty());
}

#[test]
fn partial_depth_intersect_flipped_tool_and_transferred_sheet_keep_depth_side_and_world_pose() {
    use solidcraft_geom::Vec3;
    let mut flipped = Session::default();
    base(&mut flipped, 100.0, 60.0);
    run(&mut flipped, "sketch.create", json!({"name":"FlippedTool","plane":{"origin":[0,0,2.5],"x_dir":[1,0,0],"y_dir":[0,-1,0]}}));
    run(&mut flipped, "sketch.rectangle.two_point", json!({"p0":[20,-30],"p1":[30,-20]}));
    run(&mut flipped, "sketch.finish", json!({}));
    run(&mut flipped, "solid.extrude", json!({"sketch":"FlippedTool","distance":1,"operation":"intersect"}));
    retained_depth_check(&mut flipped, 100.0, 1.5, 2.5);
    assert_eq!(flat(&mut flipped)["pockets"][0]["side"], "bottom");
    assert_eq!(flat(&mut flipped)["pockets"][0]["depth_mm"], 1.5);
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    rectangle_tool(&mut s, "RetainDepth", [20.0, 20.0], [30.0, 30.0]);
    run(&mut s, "solid.extrude", json!({"sketch":"RetainDepth","distance":1,"start_offset":0.5,"operation":"intersect","name":"RetainSlab"}));
    let before = s.world_state().bodies[0].mesh().bounds();
    let name = s.model.state().sheets[0].body.clone();
    let destination = run(&mut s, "component.create", json!({"name":"Destination","parent":0}));
    let owner = destination["component"].as_u64().unwrap();
    let occurrence = destination["occurrence"].as_u64().unwrap();
    run(&mut s, "occurrence.move", json!({"occurrence":occurrence,"translate":[30,20,10],"axis":[1,0,0],"angle":"90 deg","capture":true}));
    run(&mut s, "component.move_bodies", json!({"component":owner,"bodies":[name]}));
    for command in [None, Some("sheet.unfold"), Some("sheet.refold")] {
        if let Some(command) = command {
            run(&mut s, command, json!({}));
        }
        let state = s.model.state();
        let sh = &state.sheets[0];
        assert_eq!(sh.t, 2.5);
        assert!(sh.frame.origin.dist(Vec3::new(-30.0, -10.0, 20.0)) < 1e-8);
        assert!(sh.frame.normal().dist(Vec3::Y) < 1e-8);
        assert_eq!(sh.pockets.len(), 2);
        assert_eq!(state.body(&name).unwrap().component, owner);
        assert!(state.body(&name).unwrap().transferred);
        let world = s.world_state();
        let body = &world.body(&name).unwrap().body;
        assert!(body.contains(Vec3::new(25.0, 25.0, 1.0)));
        assert!(!body.contains(Vec3::new(25.0, 25.0, 0.25)));
        assert!(!body.contains(Vec3::new(25.0, 25.0, 2.0)));
        let bounds = world.body(&name).unwrap().mesh().bounds();
        assert!(before.min.dist(bounds.min) < 1e-8 && before.max.dist(bounds.max) < 1e-8);
        assert!((solidcraft_kernel::measure(body).unwrap().volume - 100.0).abs() < 1e-6);
    }
    let mut reopened = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
    let reopened_world = reopened.world_state();
    let body = &reopened_world.body(&name).unwrap().body;
    assert!(body.contains(Vec3::new(25.0, 25.0, 1.0)));
    assert!((solidcraft_kernel::measure(body).unwrap().volume - 100.0).abs() < 1e-6);
    run(&mut reopened, "timeline.edit", json!({"feature":"RetainSlab","set":{"extent":{"distance":"1.5"}}}));
    let world = reopened.world_state();
    let body = &world.body(&name).unwrap().body;
    assert!((solidcraft_kernel::measure(body).unwrap().volume - 150.0).abs() < 1e-6);
    assert!(body.contains(Vec3::new(25.0, 25.0, 1.75)));
    assert!(!body.contains(Vec3::new(25.0, 25.0, 2.25)));
}

#[test]
fn partial_depth_intersect_full_gauge_in_rotated_frames_does_not_invent_pockets() {
    use solidcraft_geom::Vec3;
    for (angle, translate) in [(37.0, [30.0, 20.0, 10.0]), (121.0, [12345.678, 98765.432, 4321.123])] {
        let mut s = Session::default();
        base(&mut s, 100.0, 60.0);
        let name = s.model.state().sheets[0].body.clone();
        let destination = run(&mut s, "component.create", json!({"name":"Destination","parent":0}));
        let owner = destination["component"].as_u64().unwrap();
        run(
            &mut s,
            "occurrence.move",
            json!({"occurrence":destination["occurrence"],"translate":translate,"axis":[1,2,3],"angle":format!("{angle} deg"),"capture":true}),
        );
        run(&mut s, "component.move_bodies", json!({"component":owner,"bodies":[name]}));
        run(&mut s, "component.activate", json!({"component":owner}));
        run(&mut s, "sketch.create", json!({"name":"WorldTool","plane":{"origin":[0,0,0],"x_dir":[1,0,0],"y_dir":[0,1,0]}}));
        run(&mut s, "sketch.rectangle.two_point", json!({"p0":[20,20],"p1":[30,30]}));
        run(&mut s, "sketch.finish", json!({}));
        let mut tool_session = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
        // The witness is built in the sketch's own component, so its caps lie in the sketch's frame.
        run(&mut tool_session, "component.activate", json!({"component":owner}));
        run(&mut tool_session, "solid.extrude", json!({"sketch":"WorldTool","distance":2.5,"body_name":"CapWitness"}));
        let witness_state = tool_session.model.state();
        let witness = &witness_state.body("CapWitness").unwrap().body;
        let tool_plane = witness_state.sketches.last().unwrap().plane;
        let sources = witness.face_sources().unwrap();
        assert_eq!(sources.len(), witness.face_count());
        let mut cap_count = 0;
        for (index, source) in sources.iter().enumerate() {
            if let Some(solidcraft_kernel::FaceSource::CertifiedExtrudeCap { high, plane, .. }) = source {
                cap_count += 1;
                let current = witness.planar_face(index).unwrap();
                assert!(current.normal().cross(tool_plane.normal()).len() < 1e-10);
                assert!((current.origin - plane.origin).dot(tool_plane.normal()).abs() < 1e-8);
                assert!(((plane.origin - tool_plane.origin).dot(tool_plane.normal()) - if *high { 2.5 } else { 0.0 }).abs() < 1e-8);
            }
        }
        assert_eq!(cap_count, 2);
        run(&mut s, "solid.extrude", json!({"sketch":"WorldTool","distance":2.5,"operation":"intersect","name":"PostTransferRetain"}));
        for command in [None, Some("sheet.unfold"), Some("sheet.refold")] {
            if let Some(command) = command {
                run(&mut s, command, json!({}));
            }
            assert!(s.model.state().sheets[0].pockets.is_empty(), "full gauge must not acquire roundoff-depth pockets");
            let world = s.world_state();
            let body = &world.body(&name).unwrap().body;
            assert!((solidcraft_kernel::measure(body).unwrap().volume - 250.0).abs() < 1e-5);
            assert!(body.contains(Vec3::new(25.0, 25.0, 0.1)));
            assert!(body.contains(Vec3::new(25.0, 25.0, 2.4)));
            assert!(!body.contains(Vec3::new(10.0, 25.0, 1.0)));
        }
        let mut cold = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
        assert!(cold.model.state().sheets[0].pockets.is_empty());
        assert!(flat(&mut cold)["pockets"].as_array().unwrap().is_empty());
        run(&mut s, "timeline.edit", json!({"feature":"PostTransferRetain","set":{"extent":{"distance":"1","start_offset":"0.5"}}}));
        for command in [None, Some("sheet.unfold"), Some("sheet.refold")] {
            if let Some(command) = command {
                run(&mut s, command, json!({}));
            }
            assert_eq!(s.model.state().sheets[0].pockets.len(), 2);
            let world = s.world_state();
            let body = &world.body(&name).unwrap().body;
            let measured = solidcraft_kernel::measure(body).unwrap().volume;
            assert!((measured - 100.0).abs() < 1e-5, "retained volume {measured}");
            assert!(body.contains(Vec3::new(25.0, 25.0, 1.0)));
            assert!(!body.contains(Vec3::new(25.0, 25.0, 0.1)));
            assert!(!body.contains(Vec3::new(25.0, 25.0, 2.4)));
        }
        let cold = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
        let world = cold.world_state();
        let body = &world.body(&name).unwrap().body;
        assert!((solidcraft_kernel::measure(body).unwrap().volume - 100.0).abs() < 1e-5);
        assert!(body.contains(Vec3::new(25.0, 25.0, 1.0)));
        assert!(!body.contains(Vec3::new(25.0, 25.0, 0.1)));
    }
}

#[test]
fn partial_depth_intersect_planar_to_object_retains_current_target_depth() {
    use solidcraft_geom::Vec3;
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    let name = s.model.state().sheets[0].body.clone();
    run(&mut s, "solid.box", json!({"corner":[200,0,1.5],"length":10,"width":10,"height":1,"body_name":"Roof"}));
    rectangle_tool(&mut s, "Tool", [20.0, 20.0], [30.0, 30.0]);
    run(&mut s, "solid.extrude", json!({"sketch":"Tool","to":[205,5,1.5],"start_offset":0.5,"operation":"intersect","participants":[name]}));
    for command in [None, Some("sheet.unfold"), Some("sheet.refold")] {
        if let Some(command) = command {
            run(&mut s, command, json!({"body":name}));
        }
        let state = s.model.state();
        let body = &state.body(&name).unwrap().body;
        assert!((solidcraft_kernel::measure(body).unwrap().volume - 100.0).abs() < 1e-6);
        assert!(body.contains(Vec3::new(25.0, 25.0, 1.0)));
        assert!(!body.contains(Vec3::new(25.0, 25.0, 0.1)));
        assert!(!body.contains(Vec3::new(25.0, 25.0, 2.0)));
        assert_eq!(state.sheets[0].pockets.len(), 2);
        assert!((solidcraft_kernel::measure(&state.body("Roof").unwrap().body).unwrap().volume - 100.0).abs() < 1e-6);
    }
    let cold = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
    assert!((solidcraft_kernel::measure(&cold.model.state().body(&name).unwrap().body).unwrap().volume - 100.0).abs() < 1e-6);
}

#[test]
fn partial_depth_intersect_curved_to_object_refuses_unrepresentable_depth_atomically() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    let name = s.model.state().sheets[0].body.clone();
    run(&mut s, "solid.cylinder", json!({"base":[25,20,12],"axis":[0,1,0],"radius":11,"height":10,"body_name":"CurvedRoof"}));
    rectangle_tool(&mut s, "Tool", [24.0, 23.0], [26.0, 27.0]);
    let before = s.doc.to_json();
    let error =
        s.execute("solid.extrude", &json!({"sketch":"Tool","to":[36,25,12],"operation":"intersect","participants":[name]})).unwrap_err().to_string();
    assert!(error.contains("sheet machining"), "{error}");
    assert_eq!(s.doc.to_json(), before);
    for command in [None, Some("sheet.unfold"), Some("sheet.refold")] {
        if let Some(command) = command {
            run(&mut s, command, json!({"body":name}));
        }
        let state = s.model.state();
        assert!((solidcraft_kernel::measure(&state.body(&name).unwrap().body).unwrap().volume - 15000.0).abs() < 1e-6);
        assert!(state.sheets[0].pockets.is_empty());
    }
}

#[test]
fn partial_depth_intersect_curved_to_object_preserves_current_trimmed_cap_footprint() {
    use solidcraft_geom::Vec3;
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    let name = s.model.state().sheets[0].body.clone();
    run(&mut s, "solid.cylinder", json!({"base":[25,25,0],"axis":[0,0,1],"radius":2,"height":20,"body_name":"CurvedRoof"}));
    run(&mut s, "solid.box", json!({"corner":[19,19,1],"length":12,"width":12,"height":1,"body_name":"StopPlate"}));
    run(&mut s, "solid.combine", json!({"target":"CurvedRoof","tools":["StopPlate"],"operation":"join"}));
    rectangle_tool(&mut s, "Tool", [20.0, 20.0], [30.0, 30.0]);
    let mut tool_session = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
    run(&mut tool_session, "solid.extrude", json!({"sketch":"Tool","to":[27,25,10],"body_name":"TrimWitness"}));
    let ts = tool_session.model.state();
    let witness = &ts.body("TrimWitness").unwrap().body;
    assert!((solidcraft_kernel::measure(witness).unwrap().volume - (100.0 - 4.0 * std::f64::consts::PI)).abs() < 0.01);
    assert!(witness.contains(Vec3::new(21.0, 21.0, 0.5)));
    assert!(!witness.contains(Vec3::new(25.0, 25.0, 0.5)));
    run(&mut s, "solid.extrude", json!({"sketch":"Tool","to":[27,25,10],"operation":"intersect","participants":[name]}));
    for command in [None, Some("sheet.unfold"), Some("sheet.refold")] {
        if let Some(command) = command {
            run(&mut s, command, json!({"body":name}));
        }
        let state = s.model.state();
        let body = &state.body(&name).unwrap().body;
        let measured = solidcraft_kernel::measure(body).unwrap().volume;
        assert!((measured - (100.0 - 4.0 * std::f64::consts::PI)).abs() < 0.01);
        let area: f64 = state.sheets[0].flat_regions().unwrap().iter().map(|r| r.area()).sum();
        assert!((area - (100.0 - 4.0 * std::f64::consts::PI)).abs() < 1e-8);
        assert!(body.contains(Vec3::new(21.0, 21.0, 0.5)));
        assert!(!body.contains(Vec3::new(25.0, 25.0, 0.5)));
        assert!(!body.contains(Vec3::new(21.0, 21.0, 1.5)));
    }
    let cold = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
    assert!(!cold.model.state().body(&name).unwrap().body.contains(Vec3::new(25.0, 25.0, 0.5)));
    let dxf = run(&mut s, "sheet.export_dxf", json!({"body":name}))["dxf"].as_str().unwrap().to_string();
    assert!(dxf.contains("POCKET_TOP_Z1.000000_TO_2.500000_MM"));
    let outline = dxf_outline_only(&dxf);
    assert_eq!(solidcraft_io::read_dxf(&outline).unwrap().len(), 5);
    let mut outline_import = Session::default();
    run(&mut outline_import, "sketch.insert_dxf", json!({"text":outline,"name":"Outline"}));
    run(&mut outline_import, "sketch.finish", json!({}));
    run(&mut outline_import, "solid.extrude", json!({"distance":1,"profiles":[{"point":[21,21]}]}));
    assert!((volume(&mut outline_import) - (100.0 - 4.0 * std::f64::consts::PI)).abs() < 0.01);
    assert!(!outline_import.model.state().bodies[0].body.contains(Vec3::new(25.0, 25.0, 0.5)));
    let path = std::env::temp_dir().join(format!("solidcraft-current-trim-cap-{}.step", std::process::id()));
    run(&mut s, "file.export", json!({"path":path.to_string_lossy(),"bodies":[name]}));
    let mut imported = Session::default();
    run(&mut imported, "doc.open", json!({"path":path.to_string_lossy()}));
    assert!((volume(&mut imported) - (100.0 - 4.0 * std::f64::consts::PI)).abs() < 0.01);
    assert!(!imported.model.state().bodies[0].body.contains(Vec3::new(25.0, 25.0, 0.5)));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn partial_depth_intersect_sub_extrusion_resolution_layers_refuse_atomically() {
    for (distance, offset) in [(2.5 - 1e-8, 1e-8), (2.5 - 1e-8, 0.0), (1e-8, 0.0)] {
        let mut s = Session::default();
        base(&mut s, 100.0, 60.0);
        let name = s.model.state().sheets[0].body.clone();
        let destination = run(&mut s, "component.create", json!({"name":"Far","parent":0}));
        let owner = destination["component"].as_u64().unwrap();
        run(
            &mut s,
            "occurrence.move",
            json!({"occurrence":destination["occurrence"],"translate":[12345.678,98765.432,4321.123],"axis":[1,2,3],"angle":"121 deg","capture":true}),
        );
        run(&mut s, "component.move_bodies", json!({"component":owner,"bodies":[name]}));
        run(&mut s, "component.activate", json!({"component":owner}));
        run(&mut s, "sketch.create", json!({"name":"Tool","plane":{"origin":[0,0,0],"x_dir":[1,0,0],"y_dir":[0,1,0]}}));
        run(&mut s, "sketch.rectangle.two_point", json!({"p0":[20,20],"p1":[30,30]}));
        run(&mut s, "sketch.finish", json!({}));
        let before = s.doc.to_json();
        let error = s
            .execute("solid.extrude", &json!({"sketch":"Tool","distance":distance,"start_offset":offset,"operation":"intersect"}))
            .unwrap_err()
            .to_string();
        assert!(error.contains("resolution") || error.contains("distance is zero") || error.contains("sheet machining"), "{error}");
        assert_eq!(s.doc.to_json(), before);
        assert!(s.model.state().sheets[0].pockets.is_empty());
        assert!((solidcraft_kernel::measure(&s.world_state().body(&name).unwrap().body).unwrap().volume - 15000.0).abs() < 1e-5);
    }
}

/// A 0.8 mm steel rule and a base flange from a closed outline on XY (the sheet spans z 0..0.8).
fn outline_blank(s: &mut Session, outline: Value) {
    run(s, "sheet.manage_rules", json!({"name": "Steel 0.8", "thickness": "0.8", "active": true}));
    run(s, "sketch.create", json!({"plane": "XY", "name": "Blank"}));
    run(s, "sketch.line", json!({"points": outline, "closed": true}));
    run(s, "sketch.finish", json!({}));
    run(s, "sheet.flange", json!({"sketch": "Blank"}));
}

/// A 100 x 40 blank with two 15 mm tabs on its right edge (y 5..12 and y 28..35), on one bend line.
fn two_tab_blank(s: &mut Session) {
    outline_blank(
        s,
        json!([[0, 0], [100, 0], [100, 5], [115, 5], [115, 12], [100, 12], [100, 28], [115, 28], [115, 35], [100, 35], [100, 40], [0, 40]]),
    );
}

/// Folding one tab must not be refused because another tab lies on the same line (#264).
#[test]
fn fold_one_of_two_collinear_tabs() {
    let mut s = Session::default();
    two_tab_blank(&mut s);
    let flat = volume(&mut s);
    run(&mut s, "sheet.fold", json!({"points": [[107, 5, 0.8], [107, 12, 0.8]], "fixed": [50, 20, 0.8]}));
    assert!(rel(volume(&mut s), flat) < 1e-3, "a fold keeps the material");
}

/// One fold line across both tabs bends both (#264).
#[test]
fn fold_across_two_collinear_tabs() {
    let mut s = Session::default();
    two_tab_blank(&mut s);
    let flat = volume(&mut s);
    run(&mut s, "sheet.fold", json!({"points": [[107, 5, 0.8], [107, 35, 0.8]], "fixed": [50, 20, 0.8]}));
    assert!(rel(volume(&mut s), flat) < 1e-3);
    let bb = &run(&mut s, "inspect.measure", json!({}))["bodies"][0]["bbox"];
    assert!(bb["max"][2].as_f64().unwrap() > 7.0, "both tabs stand up: {bb}");
}

/// Edge flanges on both tab ends in one feature (two separate features already work) (#264).
#[test]
fn edge_flanges_on_two_collinear_tab_ends_in_one_feature() {
    let mut s = Session::default();
    two_tab_blank(&mut s);
    run(&mut s, "sheet.flange", json!({"edges": [[115, 8, 0.8], [115, 31, 0.8]], "height": "10"}));
    let mut separate = Session::default();
    two_tab_blank(&mut separate);
    run(&mut separate, "sheet.flange", json!({"edges": [[115, 8, 0.8]], "height": "10"}));
    run(&mut separate, "sheet.flange", json!({"edges": [[115, 31, 0.8]], "height": "10"}));
    assert!(rel(volume(&mut s), volume(&mut separate)) < 1e-6);
}

/// `fixed` inside a cut-out touches no material: the fold must refuse it or say which side it kept (#263).
#[test]
fn fold_fixed_point_off_the_sheet_is_not_silently_ignored() {
    let mut s = Session::default();
    outline_blank(&mut s, json!([[0, 0], [100, 0], [100, 60], [0, 60]]));
    run(&mut s, "sketch.create", json!({"plane": "XY", "offset": "2", "name": "Window"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [10, 15], "p1": [40, 45]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "solid.extrude", json!({"sketch": "Window", "distance": "4", "direction": "negative", "operation": "cut"}));
    match s.execute("sheet.fold", &json!({"points": [[80, 0, 0.8], [80, 60, 0.8]], "fixed": [25, 30, 0.8]})) {
        Err(e) => assert!(e.to_string().contains("fixed"), "{e}"),
        Ok(r) => {
            let warned =
                ["warning", "warnings"].iter().any(|k| r.get(*k).is_some_and(|w| !w.is_null() && w != &json!([]) && w.to_string().contains("fixed")));
            assert!(warned, "fixed [25, 30] is inside the window, yet the fold succeeded silently: {r}");
        }
    }
}

/// A tab whose fold line sits exactly at its root: the bend zone reaches the blank outline beside
/// the tab. Every edge is straight, so the error must not blame a slanted or curved edge (#262).
#[test]
fn fold_at_a_tab_root_names_the_real_cause() {
    let mut s = Session::default();
    outline_blank(&mut s, json!([[0, 0], [100, 0], [100, 50], [60, 50], [60, 70], [40, 70], [40, 50], [0, 50]]));
    if let Err(e) = s.execute("sheet.fold", &json!({"points": [[40, 50, 0.8], [60, 50, 0.8]], "fixed": [50, 25, 0.8]})) {
        let e = e.to_string();
        assert!(!e.contains("slanted or curved"), "all edges are straight: {e}");
        assert!(e.contains("bend zone"), "name the bend zone reaching the outline: {e}");
    }
    // 1 mm into the tab the same fold works.
    let mut ok = Session::default();
    outline_blank(&mut ok, json!([[0, 0], [100, 0], [100, 50], [60, 50], [60, 70], [40, 70], [40, 50], [0, 50]]));
    run(&mut ok, "sheet.fold", json!({"points": [[40, 51, 0.8], [60, 51, 0.8]], "fixed": [50, 25, 0.8]}));
}

/// #296: two bends meeting at an open corner relief touch only along an edge, so the folded
/// material the machining guard checks must not be sewn into one solid.
#[test]
fn hole_beside_two_adjacent_edge_flanges_cuts_and_bends_stay_guarded() {
    let build = |s: &mut Session| {
        base(s, 80.0, 40.0);
        run(s, "sheet.flange", json!({"edges":[[40.0, 0.0, 2.5]],"height":30,"angle":90}));
        run(s, "sheet.flange", json!({"edges":[[80.0, 22.5, 2.5]],"height":15,"angle":90}));
    };
    let hole = |s: &mut Session, name: &str, centre: [f64; 2]| {
        run(s, "sketch.create", json!({"plane":"XY","name":name}));
        run(s, "sketch.circle.center", json!({"center":centre,"radius":3}));
        run(s, "sketch.finish", json!({}));
        s.execute("solid.extrude", &json!({"sketch":name,"through_all":true,"operation":"cut"}))
    };
    let mut s = Session::default();
    build(&mut s);
    let before = volume(&mut s);
    assert!((before - 14057.2).abs() < 0.1, "{before}");
    if let Err(e) = hole(&mut s, "Hole", [25.0, 20.0]) {
        panic!("hole beside two flanges: {e}");
    }
    let after = volume(&mut s);
    assert!((before - after - 9.0 * PI * 2.5).abs() < 0.05, "{before} -> {after}");
    let mut bend = Session::default();
    build(&mut bend);
    let error = hole(&mut bend, "Bend", [78.5, 20.0]).unwrap_err().to_string();
    assert!(error.contains("bend"), "{error}");
}

/// The guard of #296 sews every bend on its own. A bend that carries a folded round cut-out
/// (curved edges on both of its cylinders, a ruled wall between them) sews like any other: a hole
/// beside it cuts, and a hole in the bend is still refused.
#[test]
fn hole_beside_a_bend_with_a_round_cut_out_cuts_and_the_bend_stays_guarded() {
    let build = |s: &mut Session| {
        base(s, 80.0, 40.0);
        run(s, "sheet.flange", json!({"edges":[[40.0, 0.0, 2.5]],"height":30,"angle":90}));
        run(s, "sheet.unfold", json!({}));
        run(s, "sketch.create", json!({"plane":"XY","name":"Across"}));
        run(s, "sketch.circle.center", json!({"center":[60.0, 0.0],"radius":4}));
        run(s, "sketch.finish", json!({}));
        run(s, "solid.extrude", json!({"sketch":"Across","through_all":true,"operation":"cut","direction":"symmetric"}));
        run(s, "sheet.refold", json!({}));
    };
    let hole = |s: &mut Session, name: &str, centre: [f64; 2]| {
        run(s, "sketch.create", json!({"plane":"XY","name":name}));
        run(s, "sketch.circle.center", json!({"center":centre,"radius":3}));
        run(s, "sketch.finish", json!({}));
        s.execute("solid.extrude", &json!({"sketch":name,"through_all":true,"operation":"cut"}))
    };
    let mut s = Session::default();
    build(&mut s);
    let before = volume(&mut s);
    if let Err(e) = hole(&mut s, "Hole", [25.0, 20.0]) {
        panic!("hole beside a bend with a round cut-out: {e}");
    }
    let after = volume(&mut s);
    assert!((before - after - 9.0 * PI * 2.5).abs() < 0.05, "{before} -> {after}");
    let mut bend = Session::default();
    build(&mut bend);
    let error = hole(&mut bend, "Bend", [25.0, 1.5]).unwrap_err().to_string();
    assert!(error.contains("bend"), "{error}");
}

/// A fold whose stored `fixed` point ends up off the sheet after a parameter change keeps working:
/// the point still says which side stays (#370). Only a new pick off the sheet is refused (#263).
#[test]
#[ignore = "fails until #370 is fixed"]
fn fold_keeps_working_when_a_parameter_moves_the_sheet_off_its_fixed_point() {
    let mut s = Session::default();
    run(&mut s, "parameters.change", json!({"name": "w", "expression": "100 mm"}));
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Blank"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [100, 60]}));
    run(&mut s, "sketch.dimension", json!({"entities": ["l1"], "value": "w"}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "sheet.flange", json!({"sketch": "Blank"}));
    run(&mut s, "sheet.fold", json!({"points": [[40, 0, 2.5], [40, 60, 2.5]], "fixed": [90, 30, 2.5], "name": "Fold1"}));
    run(&mut s, "parameters.change", json!({"name": "w", "expression": "80 mm"}));
    let fold = run(&mut s, "timeline.inspect", json!({"feature": "Fold1"}));
    assert!(fold["error"].is_null(), "the fold fails after the width change: {}", fold["error"]);
    // The +x side (where the point is) stays flat out to the new width; the -x side stands up.
    let bb = &run(&mut s, "inspect.measure", json!({}))["bodies"][0]["bbox"];
    assert!((bb["max"][0].as_f64().unwrap_or(0.0) - 80.0).abs() < 1e-6, "{bb}");
    assert!(bb["max"][2].as_f64().unwrap_or(0.0) > 30.0, "{bb}");
}

/// `fixed` 20 mm above a 2.5 mm sheet touches no material: refuse it or say so, like a point in a
/// cut-out (#371).
#[test]
#[ignore = "fails until #371 is fixed"]
fn fold_fixed_point_above_the_sheet_is_not_silently_accepted() {
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    match s.execute("sheet.fold", &json!({"points": [[40, 0, 2.5], [40, 60, 2.5]], "fixed": [90, 30, 20]})) {
        Err(e) => assert!(e.to_string().contains("fixed"), "{e}"),
        Ok(r) => {
            let warned = ["warning", "warnings"].iter().any(|k| r.get(*k).is_some_and(|w| w.to_string().contains("fixed")));
            assert!(warned, "fixed [90, 30, 20] is 17.5 mm above the sheet, yet the fold succeeded silently: {r}");
        }
    }
}

/// A 120 x 90 tray in 2 mm aluminium (R 1, K 0.5): four 38.6 mm flanges at `deg` in one feature.
fn alu_tray(s: &mut Session, deg: f64, rule: Value) {
    let mut r = json!({"name": "AL2", "thickness": "2 mm", "k_factor": 0.5, "bend_radius": "1 mm", "gap": "0.5 mm", "active": true});
    if let (Some(r), Some(extra)) = (r.as_object_mut(), rule.as_object()) {
        r.extend(extra.clone());
    }
    run(s, "sheet.manage_rules", r);
    base(s, 120.0, 90.0);
    run(s, "sheet.flange", json!({"edges": [[60, 0, 2], [120, 45, 2], [60, 90, 2], [0, 45, 2]], "height": 38.6, "angle": format!("{deg} deg")}));
}

#[test]
fn closed_corners_reach_the_flat_pattern_and_the_dxf() {
    use std::f64::consts::SQRT_2;
    // 60° and 105° are angles whose flat pattern was lost to a rounding slip in the planar arrangement.
    for deg in [60.0f64, 72.0, 90.0, 105.0] {
        let mut s = Session::default();
        alu_tray(&mut s, deg, json!({}));
        assert!(s.model.results.iter().all(|r| r.error.is_none() && r.warning.is_none()), "{deg}: {:?}", s.model.results);
        // The end of the flange on y = 0 at the corner at the origin, by hand (see the doc tests):
        // x(w) = g/√2 + T·sinθ − w·cosθ at flat y = s0 − BA − (w − s0), for w = s0 and w = 38.6.
        let th = deg.to_radians();
        let (s0, ba) = (3.0 * (th / 2.0).tan(), th * 2.0);
        let x_end = |w: f64| 0.5 / SQRT_2 + 2.0 * th.sin() - w * th.cos();
        let f = flat(&mut s);
        let outline: Vec<(f64, f64)> = f["outline"][0]
            .as_array()
            .map(|l| l.iter().map(|p| (p[0].as_f64().unwrap_or(f64::NAN), p[1].as_f64().unwrap_or(f64::NAN))).collect())
            .unwrap_or_default();
        assert_eq!(f["outline"].as_array().map(Vec::len), Some(1), "{deg}: {f}");
        for (x, y) in [(x_end(s0), s0 - ba), (x_end(38.6), s0 - ba - (38.6 - s0))] {
            assert!(outline.iter().any(|p| (p.0 - x).abs() < 1e-6 && (p.1 - y).abs() < 1e-6), "{deg}: ({x}, {y}) not in {outline:?}");
        }
        // The folded part holds the flat pattern's material (K = 0.5 keeps the volume).
        let n = outline.len();
        let area = (0..n).map(|i| outline[i].0 * outline[(i + 1) % n].1 - outline[(i + 1) % n].0 * outline[i].1).sum::<f64>().abs() / 2.0;
        let v = volume(&mut s);
        assert!((v - area * 2.0).abs() < 1e-3 * v, "{deg}: {v} vs {}", area * 2.0);
        // The DXF: the closed outline in straight lines, four bend lines.
        let dxf = run(&mut s, "sheet.export_dxf", json!({}))["dxf"].as_str().unwrap_or_default().to_string();
        assert_eq!(dxf.matches("0\nLINE\n").count(), n + 4, "{deg}: {n} outline points");
        // Unfold and refold keep it.
        run(&mut s, "sheet.unfold", json!({}));
        run(&mut s, "sheet.refold", json!({}));
        assert!((volume(&mut s) - v).abs() < 1e-6 * v, "{deg}");
        // A cut made folded still goes through with the corners closed: a Ø20 hole in the base.
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Hole"}));
        run(&mut s, "sketch.circle.center", json!({"center": [60, 45], "diameter": 20}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "solid.extrude", json!({"sketch": "Hole", "distance": 10, "operation": "cut", "through_all": true}));
        let want = PI * 100.0 * 2.0;
        assert!((v - volume(&mut s) - want).abs() < 1e-3 * want, "{deg}: {v} - {} vs {want}", volume(&mut s));
        assert_eq!(flat(&mut s)["cutouts"].as_array().map(Vec::len), Some(1), "{deg}");
    }
}

#[test]
fn corner_reliefs_of_the_rule_reach_the_part_the_flat_pattern_and_the_dxf() {
    use solidcraft_geom::Vec2;
    // The tray at 72°: its bends meet at the corners of the base, s0 = 3·tan 36° in from the sheet's
    // edges, and are BA = 2θ wide in the flat. A relief 8 mm across reaches r = 4 mm along both edges
    // and takes, at each corner, r²·π/4 (round) or r² (rectangular) of the base and r of two bends.
    let th = 72f64.to_radians();
    let (s0, ba) = (3.0 * (th / 2.0).tan(), th * 2.0);
    let corners = [(s0, s0), (120.0 - s0, s0), (120.0 - s0, 90.0 - s0), (s0, 90.0 - s0)];
    let export = |s: &mut Session| run(s, "sheet.export_dxf", json!({}))["dxf"].as_str().unwrap_or_default().to_string();
    let mut notch = Session::default();
    alu_tray(&mut notch, 72.0, json!({"corner_relief": "8 mm"}));
    let v0 = volume(&mut notch);
    assert_eq!(export(&mut notch).matches("0\nARC\n").count(), 0);
    run(&mut notch, "sheet.unfold", json!({}));
    let flat0 = volume(&mut notch);
    for (shape, of_the_base) in [("round", 4.0 * PI), ("rectangular", 16.0)] {
        let mut s = Session::default();
        alu_tray(&mut s, 72.0, json!({"corner_relief_shape": shape, "corner_relief": "8 mm"}));
        assert!(s.model.results.iter().all(|r| r.error.is_none() && r.warning.is_none()), "{shape}: {:?}", s.model.results);
        let listed = run(&mut s, "sheet.rules", json!({}));
        assert_eq!(listed["rules"][0]["rule"]["corner_relief_shape"], shape);
        assert_eq!(listed["rules"][0]["values"]["corner_relief_shape"], shape);
        // The folded part loses what the flat pattern loses (K = 0.5 keeps the volume of a bend).
        let (v, want) = (volume(&mut s), 4.0 * (of_the_base + 8.0 * ba) * 2.0);
        assert!((v0 - v - want).abs() < 0.1, "{shape}: {v0} - {v} vs {want}");
        // The bend table: every bend is 8 mm shorter.
        let f = flat(&mut s);
        let lengths: Vec<f64> =
            f["bends"].as_array().map(|b| b.iter().map(|b| b["length"].as_f64().unwrap_or(f64::NAN)).collect()).unwrap_or_default();
        for (got, edge) in lengths.iter().zip([120.0, 90.0, 120.0, 90.0]) {
            assert!(lengths.len() == 4 && (got - (edge - 2.0 * s0 - 8.0)).abs() < 1e-6, "{shape}: {lengths:?}");
        }
        assert_eq!(f["outline"].as_array().map(Vec::len), Some(1), "{shape}");
        // The DXF: the round reliefs are arcs of radius 4 about the four corners, the square ones lines.
        let dxf = export(&mut s);
        let arcs: Vec<(Vec2, f64)> = solidcraft_io::read_dxf(&dxf)
            .unwrap_or_default()
            .iter()
            .filter_map(|g| if let solidcraft_io::Geom2::Arc { c, a, .. } = g { Some((*c, a.dist(*c))) } else { None })
            .collect();
        if shape == "round" {
            assert_eq!(arcs.len(), 4, "{arcs:?}");
            for (x, y) in corners {
                assert!(arcs.iter().any(|(c, r)| c.dist(Vec2::new(x, y)) < 1e-6 && (r - 4.0).abs() < 1e-6), "({x}, {y}): {arcs:?}");
            }
        } else {
            assert!(arcs.is_empty() && dxf.contains(&format!("{:.6}\n20\n{:.6}\n", s0 + 4.0, s0 + 4.0)), "{arcs:?}");
        }
        // Unfold and refold keep it, and a cut made folded still goes through: a Ø20 hole in the base.
        run(&mut s, "sheet.unfold", json!({}));
        assert!((flat0 - volume(&mut s) - want).abs() < 0.1, "{shape}: unfolded {flat0} - {} vs {want}", volume(&mut s));
        run(&mut s, "sheet.refold", json!({}));
        assert!((volume(&mut s) - v).abs() < 1e-6 * v, "{shape}");
        run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Hole"}));
        run(&mut s, "sketch.circle.center", json!({"center": [60, 45], "diameter": 20}));
        run(&mut s, "sketch.finish", json!({}));
        run(&mut s, "solid.extrude", json!({"sketch": "Hole", "distance": 10, "operation": "cut", "through_all": true}));
        assert!((v - volume(&mut s) - PI * 100.0 * 2.0).abs() < 0.5, "{shape}: {v} - {}", volume(&mut s));
        let mut reopened = Session::new(solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap());
        assert!((volume(&mut reopened) - volume(&mut s)).abs() < 1e-6, "{shape}");
    }
}

#[test]
fn the_corner_relief_shape_is_a_rule_setting_checked_and_undone() {
    let mut s = Session::default();
    alu_tray(&mut s, 72.0, json!({}));
    let shape = |s: &mut Session| run(s, "sheet.rules", json!({}))["rules"][0]["rule"]["corner_relief_shape"].clone();
    let v0 = volume(&mut s);
    assert_eq!(shape(&mut s), "notch");
    // Only the three names are shapes; a refused one changes nothing.
    for wrong in [json!("star"), json!("square"), json!(""), json!(2)] {
        let e = s.execute("sheet.manage_rules", &json!({"name": "AL2", "corner_relief_shape": wrong})).map(|_| ()).unwrap_err().to_string();
        assert!(e.contains("`corner_relief_shape` must be notch, round or rectangular"), "{e}");
    }
    assert!(shape(&mut s) == "notch" && volume(&mut s) == v0);
    // Changing the rule changes the parts made with it; other settings leave the shape alone.
    let r = run(&mut s, "sheet.manage_rules", json!({"name": "AL2", "corner_relief_shape": "Round"}));
    assert_eq!((&r["rule"]["corner_relief_shape"], &r["values"]["corner_relief_shape"]), (&json!("round"), &json!("round")));
    let round = volume(&mut s);
    assert!(round < v0 - 200.0, "{v0} -> {round}");
    run(&mut s, "sheet.manage_rules", json!({"name": "AL2", "hem_gap": "0.02 mm"}));
    assert!(shape(&mut s) == "round" && volume(&mut s) == round);
    run(&mut s, "sheet.manage_rules", json!({"name": "AL2", "corner_relief_shape": null, "corner_relief": "6 mm"}));
    assert!(shape(&mut s) == "round" && volume(&mut s) > round);
    for _ in 0..3 {
        run(&mut s, "edit.undo", json!({}));
    }
    assert!(shape(&mut s) == "notch" && volume(&mut s) == v0);
    // A relief longer than the bends is left out and reported on the flange; the part is the notched one.
    run(&mut s, "sheet.manage_rules", json!({"name": "AL2", "corner_relief_shape": "rectangular", "corner_relief": "200 mm"}));
    let w = s.model.results.last().and_then(|r| r.warning.clone()).unwrap_or_default();
    assert_eq!(w.matches("no corner relief at (").count(), 4, "{w}");
    assert!(w.contains("would take a whole bend away") && volume(&mut s) == v0, "{w}");
}

#[test]
fn corners_left_open_are_reported_on_the_flange() {
    let mut s = Session::default();
    alu_tray(&mut s, 72.0, json!({"gap": "0 mm"}));
    let w = s.model.results.last().and_then(|r| r.warning.clone()).unwrap_or_default();
    assert_eq!(w.matches("left open").count(), 4, "{w}");
    // Open corners still fold, unfold and take cuts.
    run(&mut s, "sheet.unfold", json!({}));
    run(&mut s, "sheet.refold", json!({}));
    assert!(volume(&mut s) > 0.0);
}

/// 2 mm aluminium (R 1, K 0.5, gap 0.5) and a 120 x 90 base flange.
fn alu_base(s: &mut Session) {
    run(
        s,
        "sheet.manage_rules",
        json!({"name": "AL2", "thickness": "2 mm", "k_factor": 0.5, "bend_radius": "1 mm", "gap": "0.5 mm", "active": true}),
    );
    base(s, 120.0, 90.0);
}

/// The base's four edges (bottom, right, top, left) in one feature: 75° and 38.6 high from the
/// feature, then 90°, 105° at 30 high and 120° at 25 high from the edges themselves.
fn four_angles_in_one_feature() -> Session {
    let mut s = Session::default();
    alu_base(&mut s);
    let edges = json!([
        [60, 0, 2],
        {"point": [120, 45, 2], "angle": "90 deg"},
        {"point": [60, 90, 2], "angle": 105, "height": "30 mm"},
        {"point": {"x": 0, "y": 45, "z": 2}, "angle": "120 deg", "height": 25}
    ]);
    run(&mut s, "sheet.flange", json!({"edges": edges, "height": 38.6, "angle": "75 deg"}));
    s
}

/// The flat outline's points.
fn outline_points(f: &Value) -> Vec<(f64, f64)> {
    let loops = f["outline"].as_array().cloned().unwrap_or_default();
    loops
        .iter()
        .flat_map(|l| l.as_array().cloned().unwrap_or_default())
        .map(|q| (q[0].as_f64().unwrap_or(f64::NAN), q[1].as_f64().unwrap_or(f64::NAN)))
        .collect()
}

/// Two point lists are the same set of points, to 1e-6.
fn same_points(a: &[(f64, f64)], b: &[(f64, f64)]) -> bool {
    a.len() == b.len() && a.iter().all(|p| b.iter().any(|q| (p.0 - q.0).abs() < 1e-6 && (p.1 - q.1).abs() < 1e-6))
}

fn bend_angles(s: &mut Session) -> Vec<f64> {
    flat(s)["bends"].as_array().cloned().unwrap_or_default().iter().map(|b| b["angle_deg"].as_f64().unwrap_or(f64::NAN).round()).collect()
}

fn no_errors_or_warnings(s: &Session) -> bool {
    s.model.results.iter().all(|r| r.error.is_none() && r.warning.is_none())
}

/// The flanges of [`four_angles_in_one_feature`] (bottom, right, top, left): angles in degrees
/// and heights.
const FOUR_ANGLES: [f64; 4] = [75.0, 90.0, 105.0, 120.0];
const FOUR_HEIGHTS: [f64; 4] = [38.6, 38.6, 30.0, 25.0];

/// That tray's panels in the flat pattern by hand (T 2, R 1, K 0.5, gap g 0.5; the derivation
/// is with the doc tests). Folded, a flange at θ whose neighbour at a corner is at φ ends
/// x(w) = ((g/2)·√(2 − 2·cosθ·cosφ) + T·(1 − cosθ·cosφ) − w·sinθ·cosφ) / sinφ along its edge from
/// that corner, w up its outer face from the virtual sharp. In the flat that point lies
/// s0 − BA − (w − s0) into the base from the edge, with s0 = (R + T)·tan(θ/2) the setback and
/// BA = θ·(R + K·T) the bend allowance; the panel spans w = s0 … the flange's height.
fn hand_panels() -> Vec<Vec<(f64, f64)>> {
    // Each edge's start corner, its direction, the direction into the base, and its length.
    let frames: [((f64, f64), (f64, f64), (f64, f64), f64); 4] = [
        ((0.0, 0.0), (1.0, 0.0), (0.0, 1.0), 120.0),
        ((120.0, 0.0), (0.0, 1.0), (-1.0, 0.0), 90.0),
        ((120.0, 90.0), (-1.0, 0.0), (0.0, -1.0), 120.0),
        ((0.0, 90.0), (0.0, -1.0), (1.0, 0.0), 90.0),
    ];
    (0..4)
        .map(|i| {
            let (o, d, v, len) = frames[i];
            let (th, h) = (FOUR_ANGLES[i].to_radians(), FOUR_HEIGHTS[i]);
            let (s0, ba) = (3.0 * (th / 2.0).tan(), th * 2.0);
            let x_end = |phi: f64, w: f64| {
                let c = th.cos() * phi.cos();
                (0.25 * (2.0 - 2.0 * c).sqrt() + 2.0 * (1.0 - c) - w * th.sin() * phi.cos()) / phi.sin()
            };
            let (prev, next) = (FOUR_ANGLES[(i + 3) % 4].to_radians(), FOUR_ANGLES[(i + 1) % 4].to_radians());
            let at = |u: f64, w: f64| {
                let into = s0 - ba - (w - s0);
                (o.0 + d.0 * u + v.0 * into, o.1 + d.1 * u + v.1 * into)
            };
            vec![at(x_end(prev, s0), s0), at(x_end(prev, h), h), at(len - x_end(next, h), h), at(len - x_end(next, s0), s0)]
        })
        .collect()
}

/// A polygon's area.
fn polygon_area(p: &[(f64, f64)]) -> f64 {
    let n = p.len();
    (0..n).map(|i| p[i].0 * p[(i + 1) % n].1 - p[(i + 1) % n].0 * p[i].1).sum::<f64>().abs() / 2.0
}

#[test]
fn edge_flanges_take_their_own_angle_and_height_per_edge() {
    let mut one = four_angles_in_one_feature();
    assert!(no_errors_or_warnings(&one), "{:?}", one.model.results);
    let f1 = flat(&mut one);
    // The bends: each edge's own angle, as long as the base's edge between its neighbours' bend
    // start lines (each s0 in from the original edge).
    assert_eq!(bend_angles(&mut one), FOUR_ANGLES);
    let s0 = FOUR_ANGLES.map(|a| 3.0 * (a.to_radians() / 2.0).tan());
    let ba = FOUR_ANGLES.map(|a| a.to_radians() * 2.0);
    let (w, h) = (120.0 - s0[3] - s0[1], 90.0 - s0[0] - s0[2]);
    let lengths: Vec<f64> = f1["bends"].as_array().cloned().unwrap_or_default().iter().map(|b| b["length"].as_f64().unwrap_or(f64::NAN)).collect();
    assert_eq!(lengths.len(), 4, "{f1}");
    for (got, want) in lengths.iter().zip([w, h, w, h]) {
        assert!((got - want).abs() < 1e-6, "{lengths:?} vs {w} and {h}");
    }
    // The flat pattern is one outline through every hand-derived panel corner.
    let (outline, panels) = (outline_points(&f1), hand_panels());
    assert_eq!(f1["outline"].as_array().map(Vec::len), Some(1), "{f1}");
    for (i, panel) in panels.iter().enumerate() {
        for &(x, y) in panel {
            assert!(outline.iter().any(|p| (p.0 - x).abs() < 1e-6 && (p.1 - y).abs() < 1e-6), "flange {i}: ({x}, {y}) not in {outline:?}");
        }
    }
    // It holds the base, the four bend zones and the four panels, nothing more or less, and its
    // size is the panels' extent.
    let area = w * h + (ba[0] + ba[2]) * w + (ba[1] + ba[3]) * h + panels.iter().map(|p| polygon_area(p)).sum::<f64>();
    assert!(rel(polygon_area(&outline), area) < 1e-9, "{} vs {area}", polygon_area(&outline));
    let all = panels.concat();
    let span = |k: fn(&(f64, f64)) -> f64| all.iter().map(k).fold(f64::MIN, f64::max) - all.iter().map(k).fold(f64::MAX, f64::min);
    for (k, want) in [span(|p| p.0), span(|p| p.1), 2.0].into_iter().enumerate() {
        assert!((f1["flat_size_mm"][k].as_f64().unwrap_or(f64::NAN) - want).abs() < 1e-6, "{}: {want}", f1["flat_size_mm"]);
    }
    // Folded, the part holds that material (K = 0.5 keeps the volume).
    let v1 = volume(&mut one);
    assert!(rel(v1, area * 2.0) < 1e-3, "{v1} vs {}", area * 2.0);
    // The DXF: the outline in straight lines, and four bend lines.
    let dxf = run(&mut one, "sheet.export_dxf", json!({}))["dxf"].as_str().unwrap_or_default().to_string();
    assert_eq!(dxf.matches("0\nLINE\n").count(), outline.len() + 4, "{} outline points", outline.len());
    // Unfold and refold keep it.
    run(&mut one, "sheet.unfold", json!({}));
    assert!(same_points(&outline_points(&flat(&mut one)), &outline_points(&f1)));
    run(&mut one, "sheet.refold", json!({}));
    assert!(rel(volume(&mut one), v1) < 1e-9);
    // Saved, an edge with its own values is an object and one without is the point it always was.
    let text = one.doc.to_json();
    let saved: Value = serde_json::from_str(&text).unwrap();
    let flange = saved["features"]
        .as_array()
        .and_then(|f| f.iter().find(|f| f["type"] == "sheet_flange" && f["edges"].as_array().is_some_and(|e| e.len() == 4)));
    assert_eq!(
        flange.map(|f| f["edges"].clone()),
        Some(json!([
            [60.0, 0.0, 2.0],
            {"point": [120.0, 45.0, 2.0], "angle": "90 deg"},
            {"point": [60.0, 90.0, 2.0], "angle": "105", "height": "30 mm"},
            {"point": [0.0, 45.0, 2.0], "angle": "120 deg", "height": "25"}
        ])),
        "{saved}"
    );
    // Reopened, it is the same part.
    let mut again = Session::new(solidcraft_doc::Document::from_json(&text).unwrap());
    assert!(no_errors_or_warnings(&again), "{:?}", again.model.results);
    assert!(rel(volume(&mut again), v1) < 1e-9);
}

#[test]
fn per_edge_values_are_expressions_and_feature_inputs() {
    let build = |angle: &str, height: &str| {
        let mut s = Session::default();
        alu_base(&mut s);
        run(&mut s, "parameters.add", json!({"name": "wall_angle", "expression": "105 deg"}));
        run(&mut s, "parameters.add", json!({"name": "wall_height", "expression": "30 mm"}));
        let edges = json!([[60, 0, 2], {"point": [120, 45, 2], "angle": "90 deg"}, {"point": [60, 90, 2], "angle": angle, "height": height}]);
        run(&mut s, "sheet.flange", json!({"edges": edges, "height": 38.6, "angle": "75 deg"}));
        assert!(no_errors_or_warnings(&s), "{:?}", s.model.results);
        s
    };
    let part = |s: &mut Session| (volume(s), outline_points(&flat(s)));
    let mut s = build("wall_angle", "wall_height");
    // The feature's inputs: its own, then each edge's, in the order of the edges.
    let labels: Vec<String> =
        s.doc.features.last().map(|f| f.kind.inputs().into_iter().map(|(l, e, _)| format!("{l}={e}")).collect()).unwrap_or_default();
    assert_eq!(labels, ["Height=38.6", "Angle=75 deg", "Edge angle=90 deg", "Edge angle=wall_angle", "Edge height=wall_height"]);
    // Changing the user parameters moves that flange.
    run(&mut s, "parameters.change", json!({"name": "wall_angle", "expression": "110 deg"}));
    run(&mut s, "parameters.change", json!({"name": "wall_height", "expression": "34 mm"}));
    let (v, outline) = part(&mut build("110 deg", "34 mm"));
    assert!(rel(volume(&mut s), v) < 1e-9, "{} vs {v}", volume(&mut s));
    assert!(same_points(&outline_points(&flat(&mut s)), &outline));
    // So does the feature's own input for an edge, by its parameter name (the last two).
    let names = s.doc.features.last().map(|f| f.param_names.clone()).unwrap_or_default();
    assert_eq!(names.len(), 5, "{names:?}");
    run(&mut s, "parameters.change", json!({"name": names[3], "expression": "100 deg"}));
    run(&mut s, "parameters.change", json!({"name": names[4], "expression": "36 mm"}));
    let (v, outline) = part(&mut build("100 deg", "36 mm"));
    assert!(rel(volume(&mut s), v) < 1e-9, "{} vs {v}", volume(&mut s));
    assert!(same_points(&outline_points(&flat(&mut s)), &outline));
    assert!(no_errors_or_warnings(&s), "{:?}", s.model.results);
}

#[test]
fn bad_per_edge_input_is_refused_and_plain_edges_stay_plain() {
    let mut s = Session::default();
    alu_base(&mut s);
    let err = |s: &mut Session, edges: Value| {
        s.execute("sheet.flange", &json!({"edges": edges, "height": 38.6, "angle": "90 deg"})).err().map(|e| e.to_string()).unwrap_or_default()
    };
    // The wrong kind of value, or no expression at all: the edge and the key are named.
    let e = err(&mut s, json!([[60, 0, 2], {"point": [120, 45, 2], "angle": "10 mm"}]));
    assert!(e.contains("edges[1].angle"), "{e}");
    let e = err(&mut s, json!([{"point": [60, 0, 2], "height": "30 deg"}]));
    assert!(e.contains("edges[0].height"), "{e}");
    let e = err(&mut s, json!([{"point": [60, 0, 2], "angle": "nope"}]));
    assert!(e.contains("edges[0].angle"), "{e}");
    let e = err(&mut s, json!([{"point": [60, 0, 2], "angle": [1, 2]}]));
    assert!(e.contains("edges[0]") && e.contains("`angle` must be a number or an expression"), "{e}");
    // An unknown key, a point that is none, an angle without `point`, something that is no edge.
    let e = err(&mut s, json!([{"point": [60, 0, 2], "twist": 3}]));
    assert!(e.contains("edges[0]") && e.contains("unknown key `twist`"), "{e}");
    let e = err(&mut s, json!([{"point": [60, 0], "angle": "60 deg"}]));
    assert!(e.contains("edges[0]") && e.contains("`point` must be [x, y, z]"), "{e}");
    let e = err(&mut s, json!([{"x": 60, "y": 0, "z": 2, "angle": "60 deg"}]));
    assert!(e.contains("edges[0]") && e.contains("unknown key"), "{e}");
    let e = err(&mut s, json!([{"angle": "60 deg"}]));
    assert!(e.contains("edges[0]") && e.contains("`point` must be [x, y, z]"), "{e}");
    let e = err(&mut s, json!(["edge"]));
    assert!(e.contains("edges[0]"), "{e}");
    let e = err(&mut s, json!([]));
    assert!(e.contains("1…1000"), "{e}");
    // Evaluated: an angle outside 0…180° and a height under the bend's, each naming its edge.
    let e = err(&mut s, json!([[60, 0, 2], {"point": [120, 45, 2], "angle": "200 deg"}]));
    assert!(e.contains("edge 2") && e.contains("angle must be between 0 and 180"), "{e}");
    let e = err(&mut s, json!([{"point": [60, 0, 2], "height": "2 mm"}, [120, 45, 2]]));
    assert!(e.contains("edge 1") && e.contains("height must be at least"), "{e}");
    // None of it added a feature.
    assert_eq!(s.doc.features.len(), 2, "{:?}", s.doc.features.iter().map(|f| f.name.clone()).collect::<Vec<_>>());
    // Plain points, `{x, y, z}` points and `{point}` without values are saved as the points they were.
    run(&mut s, "sheet.flange", json!({"edges": [[60, 0, 2], {"x": 120, "y": 45, "z": 2}, {"point": [60, 90, 2]}], "height": 20, "angle": "90 deg"}));
    let edges = serde_json::to_value(s.doc.features.last().map(|f| f.kind.clone())).unwrap()["edges"].clone();
    assert_eq!(edges, json!([[60.0, 0.0, 2.0], [120.0, 45.0, 2.0], [60.0, 90.0, 2.0]]));
}

#[test]
fn editing_a_flange_keeps_each_edges_own_values_with_its_edge() {
    let mut s = four_angles_in_one_feature();
    let id = s.doc.features.last().map(|f| f.id).unwrap_or(0);
    // Give the first edge 80° and take the 90° off the second.
    let edges = json!([
        {"point": [60, 0, 2], "angle": "80 deg"},
        [120, 45, 2],
        {"point": [60, 90, 2], "angle": "105 deg", "height": "30 mm"},
        {"point": [0, 45, 2], "angle": "120 deg", "height": "25 mm"}
    ]);
    run(&mut s, "timeline.edit", json!({"feature": id, "set": {"edges": edges}}));
    assert!(no_errors_or_warnings(&s), "{:?}", s.model.results);
    assert_eq!(bend_angles(&mut s), [80.0, 75.0, 105.0, 120.0]);
    let kind = |s: &Session| serde_json::to_value(s.doc.features.last().map(|f| f.kind.clone())).unwrap();
    assert_eq!(kind(&s)["edges"][1], json!([120.0, 45.0, 2.0]), "no value of its own: a plain point again");
    let (v, outline) = (volume(&mut s), outline_points(&flat(&mut s)));
    // The same edges in another order are the same part: each value follows its edge.
    let edges = json!([
        {"point": [0, 45, 2], "angle": "120 deg", "height": "25 mm"},
        {"point": [60, 90, 2], "angle": "105 deg", "height": "30 mm"},
        {"point": [60, 0, 2], "angle": "80 deg"},
        [120, 45, 2]
    ]);
    run(&mut s, "timeline.edit", json!({"feature": id, "set": {"edges": edges}}));
    assert!(no_errors_or_warnings(&s), "{:?}", s.model.results);
    assert_eq!(bend_angles(&mut s), [120.0, 105.0, 80.0, 75.0]);
    assert!(rel(volume(&mut s), v) < 1e-9, "{} vs {v}", volume(&mut s));
    assert!(same_points(&outline_points(&flat(&mut s)), &outline));
    // A bad edit is refused and leaves the feature as it was.
    let before = kind(&s);
    assert!(s.execute("timeline.edit", &json!({"feature": id, "set": {"edges": [{"point": [60, 0, 2], "angle": "80 deg", "colour": 1}]}})).is_err());
    assert!(s.execute("timeline.edit", &json!({"feature": id, "set": {"edges": [{"point": [60, 0, 2], "angle": "250 deg"}]}})).is_err());
    assert_eq!(kind(&s), before);
    assert_eq!(bend_angles(&mut s), [120.0, 105.0, 80.0, 75.0]);
}

#[test]
fn designs_without_per_edge_values_are_read_and_written_as_before() {
    let mut s = Session::default();
    alu_base(&mut s);
    run(&mut s, "sheet.flange", json!({"edges": [[60, 0, 2], [120, 45, 2], [60, 90, 2], [0, 45, 2]], "height": 38.6, "angle": "72 deg"}));
    let v = volume(&mut s);
    let text = s.doc.to_json();
    let mut design: Value = serde_json::from_str(&text).unwrap();
    let flange = design["features"].as_array().and_then(|f| f.iter().position(|f| f["type"] == "sheet_flange")).unwrap();
    assert_eq!(design["features"][flange]["edges"], json!([[60.0, 0.0, 2.0], [120.0, 45.0, 2.0], [60.0, 90.0, 2.0], [0.0, 45.0, 2.0]]));
    let mut same = Session::new(solidcraft_doc::Document::from_json(&text).unwrap());
    assert!(rel(volume(&mut same), v) < 1e-12);
    // A design file whose last edge carries its own angle is the part the command makes.
    design["features"][flange]["edges"][3] = json!({"point": [0.0, 45.0, 2.0], "angle": "100 deg"});
    let mut opened = Session::new(solidcraft_doc::Document::from_json(&design.to_string()).unwrap());
    assert!(no_errors_or_warnings(&opened), "{:?}", opened.model.results);
    let mut made = Session::default();
    alu_base(&mut made);
    let edges = json!([[60, 0, 2], [120, 45, 2], [60, 90, 2], {"point": [0, 45, 2], "angle": "100 deg"}]);
    run(&mut made, "sheet.flange", json!({"edges": edges, "height": 38.6, "angle": "72 deg"}));
    let (vo, vm) = (volume(&mut opened), volume(&mut made));
    assert!(rel(vo, vm) < 1e-9 && rel(vo, v) > 1e-4, "{vo} vs {vm} vs {v}");
    // A key this build doesn't know is refused, not dropped.
    design["features"][flange]["edges"][3] = json!({"point": [0.0, 45.0, 2.0], "twist": "3 deg"});
    assert!(solidcraft_doc::Document::from_json(&design.to_string()).is_err());
}

/// Names of the hole's side faces (curved, smaller than the bend faces) in `Body1`.
fn hole_face_names(s: &mut Session) -> Vec<String> {
    let faces = run(s, "model.faces", json!({"body": "Body1"}));
    let faces = faces["faces"].as_array().cloned().unwrap_or_default();
    faces
        .iter()
        .filter(|f| f["plane_normal"].is_null() && f["area"].as_f64().is_some_and(|a| a < 100.0))
        .map(|f| f["name"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[test]
#[ignore = "fails until #535 is fixed"]
fn a_hole_on_a_folded_panel_keeps_the_name_of_its_sketch_circle() {
    // A Ø10 hole at x = 20 on a 100 x 60 plate; the fold at x = 40 stands that (smaller) side up.
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Hole"}));
    let circle = run(&mut s, "sketch.circle.center", json!({"center": [20, 30], "radius": 5}));
    let curve = circle["curves"][0].as_str().unwrap_or("c1").to_string();
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "solid.extrude", json!({"sketch": "Hole", "through_all": true, "operation": "cut", "direction": "symmetric"}));
    let side = format!(":side:{curve}#");
    let flat = hole_face_names(&mut s);
    assert_eq!(flat.len(), 2, "{flat:?}");
    assert!(flat.iter().all(|n| n.contains(&side)), "before the fold: {flat:?}");
    fold_line(&mut s);
    run(&mut s, "sheet.fold", json!({"sketch": "FoldLine", "curve": "fl"}));
    assert!(bbox(&mut s)["max"][2].as_f64().unwrap_or(0.0) > 20.0, "the hole's panel stands up");
    let folded = hole_face_names(&mut s);
    assert_eq!(folded.len(), 2, "{folded:?}");
    assert!(folded.iter().all(|n| n.contains(&side)), "after the fold the hole is still circle {curve}: {folded:?} (before: {flat:?})");
}

#[test]
#[ignore = "fails until #541 is fixed"]
fn a_box_cut_straight_through_a_flat_sheet_is_a_cutout_like_the_extrude_cut() {
    // A 10 x 10 box perpendicular to the sheet and through its whole thickness removes exactly
    // what a sketch rectangle extruded through all removes, which is accepted today.
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    run(&mut s, "solid.box", json!({"corner": [20, 20, -1], "length": 10, "width": 10, "height": 5, "operation": "cut"}));
    let mut extruded = Session::default();
    base(&mut extruded, 100.0, 60.0);
    rectangle_tool(&mut extruded, "Hole", [20.0, 20.0], [30.0, 30.0]);
    run(&mut extruded, "solid.extrude", json!({"sketch": "Hole", "through_all": true, "direction": "symmetric", "operation": "cut"}));
    for session in [&mut s, &mut extruded] {
        assert!((volume(session) - 14750.0).abs() < 1e-6);
        assert_eq!(flat(session)["cutouts"].as_array().map(Vec::len), Some(1));
    }
    run(&mut s, "sheet.unfold", json!({}));
    run(&mut s, "sheet.refold", json!({}));
    assert!((volume(&mut s) - 14750.0).abs() < 1e-6);
}

#[test]
#[ignore = "fails until #542 is fixed"]
fn flat_pattern_reports_a_round_hole_as_a_circle() {
    // The DXF export already writes this hole as a CIRCLE; the JSON flat pattern should say
    // "circle, centre (50, 30), radius 1.6" too instead of a sampled polyline. The exact key
    // names are open: any object in the cutout that has a `center` and a `radius` counts.
    fn circles(v: &Value, out: &mut Vec<([f64; 2], f64)>) {
        match v {
            Value::Object(m) => {
                let center = m.get("center").and_then(Value::as_array).and_then(|c| Some([c.first()?.as_f64()?, c.get(1)?.as_f64()?]));
                if let (Some(center), Some(radius)) = (center, m.get("radius").and_then(Value::as_f64)) {
                    out.push((center, radius));
                }
                m.values().for_each(|v| circles(v, out));
            }
            Value::Array(a) => a.iter().for_each(|v| circles(v, out)),
            _ => {}
        }
    }
    let mut s = Session::default();
    base(&mut s, 100.0, 60.0);
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Hole"}));
    run(&mut s, "sketch.circle.center", json!({"center": [50, 30], "radius": 1.6}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "solid.extrude", json!({"sketch": "Hole", "through_all": true, "direction": "symmetric", "operation": "cut"}));
    let dxf = run(&mut s, "sheet.export_dxf", json!({}))["dxf"].as_str().unwrap_or_default().to_string();
    assert!(dxf.contains("CIRCLE"), "the DXF keeps the circle");
    let f = flat(&mut s);
    let cutouts = f["cutouts"].as_array().cloned().unwrap_or_default();
    assert_eq!(cutouts.len(), 1);
    let mut found = Vec::new();
    circles(&cutouts[0], &mut found);
    let shown: String = cutouts[0].to_string().chars().take(120).collect();
    assert!(
        found.iter().any(|(c, r)| (c[0] - 50.0).abs() < 1e-9 && (c[1] - 30.0).abs() < 1e-9 && (r - 1.6).abs() < 1e-9),
        "the round hole is not reported as a circle: {shown} ..."
    );
}

#[test]
#[ignore = "fails until #570 is fixed"]
fn a_flange_asked_for_0_8_mm_is_0_8_mm_or_refused_never_silently_2_5_mm() {
    // `thickness` on sheet.flange may become an inline override of the rule, or be refused as
    // an unknown parameter; either is fine. Building the default 2.5 mm sheet silently is not.
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Base"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [100, 60]}));
    run(&mut s, "sketch.finish", json!({}));
    match s.execute("sheet.flange", &json!({"sketch": "Base", "thickness": 0.8})) {
        Ok(_) => {
            assert!((flat(&mut s)["thickness"].as_f64().unwrap_or(f64::NAN) - 0.8).abs() < 1e-9);
            assert!((volume(&mut s) - 100.0 * 60.0 * 0.8).abs() < 1e-6, "built at {} mm3", volume(&mut s));
        }
        Err(e) => assert!(e.to_string().contains("thickness"), "{e}"),
    }
}
