//! Pre-selection fills every input of a command, each item going to the input that takes its
//! type, whether the command starts from a shortcut, the toolbar or a menu.

use serde_json::json;
use solidcraft_engine::geom::Vec3;
use solidcraft_engine::{Sel, Session};

use crate::dialogs::{Dialog, Kind, apply_commands};
use crate::{Services, SolveApp};

#[test]
fn path_plane_click_retains_source_and_previews_then_edits_in_place() {
    let mut h = crate::scenario::Harness::new();
    let source = h.app.run("sketch.create", json!({"plane":"XY","name":"Bridge"})).unwrap()["sketch"].as_u64().unwrap();
    h.app.run("sketch.options", json!({"sketch_3d":true})).unwrap();
    let wire = h.app.run("sketch.line3d", json!({"points":[[10,20,10],[30,35,40]]})).unwrap()["curves"][0].as_str().unwrap().to_string();
    h.app.run("sketch.finish", json!({})).unwrap();
    h.app.run("sketch.create", json!({"plane":"XY","name":"Other"})).unwrap();
    h.app.run("sketch.options", json!({"sketch_3d":true})).unwrap();
    let duplicate = h.app.run("sketch.line3d", json!({"points":[[-30,0,0],[-20,0,0]]})).unwrap()["curves"][0].as_str().unwrap().to_string();
    assert_eq!(wire, duplicate, "exercise duplicate curve ids in different sketches");
    h.app.run("sketch.finish", json!({})).unwrap();
    h.app.ui.show_origin = false;
    h.settle();
    h.call("ui.view", json!({"view":"home"}));
    h.call("ui.view", json!({"view":"fit"}));
    h.app.start("construct.plane.along_path");
    h.frames(2);
    h.step(&json!({"click":{"world":[20,27.5,25]}})).unwrap();
    let d = h.app.dialog.as_ref().unwrap();
    let commands = apply_commands(&h.app, d).unwrap();
    assert_eq!(commands[0].1["path"]["sketch"], json!(source));
    let history = h.app.session.undo.len();
    let count = h.app.session.doc.features.len();
    let preview = h.app.session.preview_visible_world(&commands).unwrap();
    let built = crate::preview::compute(&h.app.session, &commands, crate::preview::Colors::from_tokens(&crate::theme::Tokens::dark())).unwrap();
    assert!(!built.scene.xray.is_empty(), "construction preview must contain visible geometry");
    let plane = preview.after.construct.last().unwrap();
    let solidcraft_engine::doc::construct::ConstructGeom::Plane(pl) = plane.geom else { panic!("plane") };
    assert!(pl.origin.dist(Vec3::new(20.0, 27.5, 25.0)) < 1e-6);
    assert_eq!(h.app.session.doc.features.len(), count);
    assert_eq!(h.app.session.undo.len(), history);
    h.step(&json!({"key":"Enter"})).unwrap();
    assert!(h.app.dialog.is_none(), "{:?}", h.app.dialog);
    let feature = h.app.session.doc.features.last().unwrap().clone();
    assert_eq!(h.app.session.undo.len(), history + 1);
    h.app.edit_feature(feature.id);
    h.frames(2);
    let d = h.app.dialog.as_mut().unwrap();
    assert!(matches!(d.kind,Kind::PathPlane{sketch:Some(s),..} if s==source));
    if let Kind::PathPlane { t, .. } = &mut d.kind {
        *t = "0.25".into();
    }
    h.step(&json!({"key":"Enter"})).unwrap();
    assert!(h.app.dialog.is_none(), "{:?}", h.app.dialog);
    assert_eq!(h.app.session.doc.features.len(), count + 1);
    let p = solidcraft_engine::view::construction_planes(&h.app.session).into_iter().find(|p| p.0 == feature.id).unwrap().2;
    assert!(p.origin.dist(Vec3::new(15.0, 23.75, 17.5)) < 1e-6, "{p:?}");
    h.app.run("edit.undo", json!({})).unwrap();
    assert_eq!(h.app.session.doc.feature(feature.id).unwrap().kind, feature.kind);
}

#[test]
fn cylinder_emboss_empty_target_face_applies_automatic_wrap_and_deboss() {
    for deboss in [false, true] {
        let mut h = crate::scenario::Harness::new();
        h.app.run("solid.cylinder", json!({"radius":10,"height":20,"body_name":"Sleeve"})).unwrap();
        let initial = h.app.session.model.state().body("Sleeve").unwrap().body.tessellate_with(1e-4, true).unwrap().measure().volume;
        let sketch =
            h.app.run("sketch.create", json!({"plane":{"origin":[0,10,0],"x_dir":[-1,0,0],"y_dir":[0,0,1]},"name":"Badge"})).unwrap()["sketch"]
                .as_u64()
                .unwrap();
        h.app.run("sketch.rectangle.two_point", json!({"p0":[-2,5],"p1":[2,10]})).unwrap();
        h.app.run("sketch.finish", json!({})).unwrap();
        h.app.session.selection = vec![Sel::Profile { sketch, index: 0 }];
        h.app.start("solid.emboss");
        h.frames(2);
        if deboss {
            h.settle();
            h.step(&json!({"click":{"handle":"dialog:Type"}})).unwrap();
            h.step(&json!({"click":{"handle":"dialog:Type:Deboss"}})).unwrap();
        }
        let dialog = h.app.dialog.as_ref().unwrap();
        assert!(dialog.inputs[1].items.is_empty());
        let commands = apply_commands(&h.app, dialog).unwrap();
        assert!(commands[0].1.get("face").is_none());
        let preview = h.app.session.preview_world(&commands).unwrap();
        let expected = if deboss { -19.0 } else { 21.0 };
        let preview_delta = preview.after.body("Sleeve").unwrap().body.tessellate_with(1e-4, true).unwrap().measure().volume - initial;
        assert!(
            (preview_delta - expected).abs() < 0.05,
            "deboss={deboss}, preview delta={preview_delta}, expected={expected}, commands={commands:?}"
        );
        h.step(&json!({"key":"Enter"})).unwrap();
        assert!(h.app.dialog.is_none(), "{:?}", h.app.dialog);
        let feature = h.app.session.doc.features.last().unwrap().clone();
        let solidcraft_engine::doc::FeatureKind::Emboss { face: Some(face), deboss: actual, .. } = &feature.kind else {
            panic!("automatic Apply must save its cylinder anchor: {:?}", feature.kind)
        };
        assert_eq!(*actual, deboss);
        assert_eq!(face.body, "Sleeve");
        assert!(
            (h.app.session.model.state().body("Sleeve").unwrap().body.tessellate_with(1e-4, true).unwrap().measure().volume - initial - expected)
                .abs()
                < 0.05
        );
        h.app.edit_feature(feature.id);
        h.frames(2);
        assert!(matches!(h.app.dialog.as_ref().unwrap().inputs[1].items.first(), Some(Sel::Face { body, .. }) if body == "Sleeve"));
        h.step(&json!({"key":"Escape"})).unwrap();
        assert_eq!(h.app.session.doc.feature(feature.id).unwrap().kind, feature.kind);
    }
}

#[test]
fn cylinder_emboss_target_face_and_edit_keep_the_anchored_operation() {
    let mut h = crate::scenario::Harness::new();
    h.app.run("solid.cylinder", json!({"radius":10,"height":20,"body_name":"Sleeve"})).unwrap();
    let sketch = h.app.run("sketch.create", json!({"plane":{"origin":[0,10,0],"x_dir":[-1,0,0],"y_dir":[0,0,1]},"name":"Badge"})).unwrap()["sketch"]
        .as_u64()
        .unwrap();
    h.app.run("sketch.rectangle.two_point", json!({"p0":[-2,5],"p1":[2,10]})).unwrap();
    h.app.run("sketch.finish", json!({})).unwrap();
    h.app.session.selection = vec![Sel::Profile { sketch, index: 0 }];
    h.app.start("solid.emboss");
    h.frames(2);
    assert_eq!(h.app.dialog.as_ref().unwrap().inputs[0].items.len(), 1);
    let planar = apply_commands(&h.app, h.app.dialog.as_ref().unwrap()).unwrap();
    assert!(planar[0].1.get("face").is_none());
    h.call("ui.dialogInput", json!({"index":1}));
    h.call("ui.view", json!({"view":"back"}));
    h.frames(2);
    h.step(&json!({"click":{"world":[0,10,7.5]}})).unwrap();
    let dialog = h.app.dialog.as_ref().unwrap();
    assert!(matches!(dialog.inputs[1].items.first(), Some(Sel::Face { body, .. }) if body == "Sleeve"));
    let commands = apply_commands(&h.app, dialog).unwrap();
    assert_eq!(commands[0].1["targets"], json!(["Sleeve"]));
    assert!(commands[0].1.get("face").is_some());
    h.step(&json!({"key":"Enter"})).unwrap();
    assert!(h.app.dialog.is_none(), "{:?}", h.app.dialog);
    let feature = h.app.session.doc.features.last().unwrap().id;
    let before = serde_json::to_value(&h.app.session.doc.feature(feature).unwrap().kind).unwrap();
    h.app.edit_feature(feature);
    h.frames(2);
    let edited = apply_commands(&h.app, h.app.dialog.as_ref().unwrap()).unwrap();
    assert_eq!(edited[0].0, "timeline.redefine");
    assert_eq!(edited[0].1["params"]["targets"], json!(["Sleeve"]));
    assert!(edited[0].1["params"].get("face").is_some());
    h.step(&json!({"key":"Enter"})).unwrap();
    assert!(h.app.dialog.is_none(), "{:?}", h.app.dialog);
    assert_eq!(serde_json::to_value(&h.app.session.doc.feature(feature).unwrap().kind).unwrap(), before);
}

#[test]
fn cylinder_emboss_edit_retains_second_source_and_author_paths_and_all_profiles() {
    let mut h = crate::scenario::Harness::new();
    let source = h.app.run("component.create", json!({"name":"Source"})).unwrap()["component"].as_u64().unwrap();
    h.app.run("solid.cylinder", json!({"radius":10,"height":20,"body_name":"Sleeve"})).unwrap();
    h.app.run("occurrence.move", json!({"occurrence":"Source:1","axis":[0,0,1],"angle":"90 deg","translate":[50,0,0]})).unwrap();
    let source_second = h.app.run("occurrence.copy", json!({"component":source,"translate":[80,0,0]})).unwrap()["occurrence"].as_u64().unwrap();
    h.app.run("component.activate", json!({"component":"root"})).unwrap();
    let author = h.app.run("component.create", json!({"name":"Author"})).unwrap()["component"].as_u64().unwrap();
    h.app.run("occurrence.move", json!({"occurrence":"Author:1","axis":[1,0,0],"angle":"90 deg","translate":[-100,40,30]})).unwrap();
    let author_second = h.app.run("occurrence.copy", json!({"component":author,"translate":[0,100,0]})).unwrap()["occurrence"].as_u64().unwrap();
    h.app.run("component.activate", json!({"component":author,"occurrence":author_second})).unwrap();
    h.app.run("sketch.create", json!({"plane":{"origin":[120,0,0],"x_dir":[0,-1,0],"y_dir":[0,0,1]},"name":"Badge"})).unwrap();
    h.app.run("sketch.rectangle.two_point", json!({"p0":[-2,5],"p1":[2,10]})).unwrap();
    h.app.run("sketch.finish", json!({})).unwrap();
    let target = h.app.session.occurrence_bodies(&[source_second], false).unwrap().into_iter().next().unwrap();
    let feature = h.app.run("solid.emboss", json!({"sketch":"Badge","profiles":"all","face":[120,0,7.5],"depth":"1mm","targets":[target]})).unwrap()
        ["feature"]
        .as_u64()
        .unwrap();
    let before = h.app.session.doc.feature(feature).unwrap().clone();
    let solidcraft_engine::doc::FeatureKind::Emboss { face: Some(face), .. } = &before.kind else { panic!("selected face") };
    assert_eq!(face.path, vec![source_second]);
    assert_eq!(face.author_path, vec![author_second]);
    h.app.run("component.activate", json!({"component":"root"})).unwrap();
    h.app.edit_feature(feature);
    h.frames(2);
    let dialog = h.app.dialog.as_ref().unwrap();
    let Sel::Face { body, point, .. } = &dialog.inputs[1].items[0] else { panic!("refilled face") };
    assert_eq!(body, &target);
    assert!(point.dist(Vec3::new(120.0, 0.0, 7.5)) < 1e-8);
    let commands = apply_commands(&h.app, dialog).unwrap();
    assert_eq!(commands[0].1["params"]["profiles"], json!("all"));
    let Sel::Profile { sketch, index } = dialog.inputs[0].items[0] else { panic!("refilled profile") };
    let plane = crate::dialogs::emboss_profile_plane(&h.app, sketch).unwrap().unwrap();
    assert!(plane.origin.dist(Vec3::new(120.0, 0.0, 0.0)) < 1e-8, "{plane:?}");
    let mut guide = crate::gpu::GpuScene::default();
    crate::viewport::profile_fill(&mut guide, &h.app, sketch, index, egui::Color32::BLUE, egui::Color32::BLUE);
    let (vertices, remainder) = guide.trans.as_chunks::<{ crate::gpu::TRI_SIZE }>();
    assert!(remainder.is_empty() && !vertices.is_empty());
    let lift = h.app.cam.back() * (h.app.cam.half_height() * 0.002);
    for vertex in vertices {
        let coordinate = |offset| f32::from_le_bytes(vertex[offset..offset + 4].try_into().unwrap()) as f64;
        let point = Vec3::new(coordinate(0), coordinate(4), coordinate(8)) - lift;
        assert!((point.x - 120.0).abs() < 1e-4 && point.y.abs() < 2.01 && (4.99..=10.01).contains(&point.z), "{point:?}");
    }
    let preview = h.app.session.preview_world(&commands).unwrap();
    let volume = preview.after.body(&target).unwrap().mesh().measure().volume;
    assert!(volume > preview.before.body(&target).unwrap().mesh().measure().volume + 10.0);
    let colors = crate::preview::Colors::from_tokens(&crate::theme::Tokens::get());
    let built = crate::preview::compute(&h.app.session, &commands, colors).unwrap();
    assert!(built.replaced.contains(&target) && !built.scene.tris.is_empty(), "{:?}", built.replaced);
    let source_first = h.app.session.doc.occurrence_of(source).unwrap().id;
    h.app.run("browser.visibility", json!({"paths":[[source_first]],"visible":false})).unwrap();
    let hidden = crate::preview::compute(&h.app.session, &commands, colors).unwrap();
    assert_eq!(hidden.replaced, vec![target.clone()]);
    let visible = h.app.session.preview_visible_world(&commands).unwrap();
    assert!(visible.before.body("Sleeve").is_none() && visible.after.body("Sleeve").is_none());
    assert!(visible.after.body(&target).is_some() && !hidden.scene.tris.is_empty());
    let mut shared = h.app.session.scratch();
    shared.execute("component.activate", &json!({"component":source,"occurrence":source_first})).unwrap();
    let new_body = shared
        .preview_visible_world(&[("solid.box".into(), json!({"body_name":"NewPreview","length":2,"width":2,"height":2,"operation":"new"}))])
        .unwrap();
    let added = new_body.after.bodies.iter().filter(|body| new_body.before.body(&body.name).is_none()).collect::<Vec<_>>();
    assert_eq!(added.len(), 1, "{:?}", added.iter().map(|body| &body.name).collect::<Vec<_>>());
    assert!(added[0].name.contains("Source:2"), "{}", added[0].name);
    let doc = h.app.session.doc.to_json();
    let history = (h.app.session.undo.len(), h.app.session.redo.len());
    let selection = h.app.session.selection.clone();
    let Kind::Emboss { depth, .. } = &mut h.app.dialog.as_mut().unwrap().kind else { panic!("Emboss editor") };
    *depth = "2 mm".into();
    let changed = apply_commands(&h.app, h.app.dialog.as_ref().unwrap()).unwrap();
    let changed_preview = h.app.session.preview_world(&changed).unwrap();
    assert!(changed_preview.after.body(&target).unwrap().mesh().measure().volume > volume + 10.0);
    let mut invalid = changed.clone();
    invalid[0].1["params"]["depth"] = json!("unknown_depth");
    assert!(h.app.session.preview_world(&invalid).is_err());
    assert_eq!(h.app.session.doc.to_json(), doc);
    assert_eq!((h.app.session.undo.len(), h.app.session.redo.len()), history);
    assert_eq!(h.app.session.selection, selection);
    h.step(&json!({"key":"Escape"})).unwrap();
    assert!(h.app.dialog.is_none());
    assert_eq!(h.app.session.doc.feature(feature).unwrap().kind, before.kind);
    h.app.edit_feature(feature);
    h.frames(2);
    h.step(&json!({"key":"Enter"})).unwrap();
    assert!(h.app.dialog.is_none(), "{:?}", h.app.dialog);
    let after = h.app.session.doc.feature(feature).unwrap();
    assert_eq!(after.kind, before.kind);
    assert_eq!(after.face_names, before.face_names);
    assert_eq!(h.app.session.active_component, 0);
}

fn app_with(script: serde_json::Value) -> SolveApp {
    let mut s = Session::default();
    s.run_script(&script).unwrap();
    SolveApp::new(s, Services::default())
}

#[test]
fn cylinder_emboss_profile_guide_uses_target_or_unique_foreign_sketch_frame() {
    for target_owned in [true, false] {
        let mut h = crate::scenario::Harness::new();
        let source = if target_owned { h.app.run("component.create", json!({"name":"Source"})).unwrap()["component"].as_u64().unwrap() } else { 0 };
        h.app.run("solid.cylinder", json!({"radius":10,"height":20,"body_name":"Sleeve"})).unwrap();
        let (profile_owner, expected, target, face) = if target_owned {
            h.app.run("occurrence.move", json!({"occurrence":"Source:1","axis":[0,0,1],"angle":"90 deg","translate":[50,0,0]})).unwrap();
            let second = h.app.run("occurrence.copy", json!({"component":source,"translate":[80,0,0]})).unwrap()["occurrence"].as_u64().unwrap();
            let target = h.app.session.occurrence_bodies(&[second], false).unwrap().remove(0);
            (source, Vec3::new(120.0, 0.0, 0.0), target, json!([120, 0, 7.5]))
        } else {
            let profile = h.app.run("component.create", json!({"name":"Profile"})).unwrap()["component"].as_u64().unwrap();
            h.app.run("occurrence.move", json!({"occurrence":"Profile:1","axis":[0,0,1],"angle":"90 deg","translate":[50,20,0]})).unwrap();
            (profile, Vec3::new(0.0, 10.0, 0.0), "Sleeve".to_string(), json!([0, 10, 7.5]))
        };
        let plane = if target_owned {
            json!({"origin":[40,0,0],"x_dir":[0,-1,0],"y_dir":[0,0,1]})
        } else {
            json!({"origin":[0,10,0],"x_dir":[-1,0,0],"y_dir":[0,0,1]})
        };
        let sketch = h.app.run("sketch.create", json!({"plane":plane,"name":"Badge"})).unwrap()["sketch"].as_u64().unwrap();
        h.app.run("sketch.rectangle.two_point", json!({"p0":[-2,5],"p1":[2,10]})).unwrap();
        h.app.run("sketch.finish", json!({})).unwrap();
        h.app.run("component.activate", json!({"component":"root"})).unwrap();
        let feature =
            h.app.run("solid.emboss", json!({"sketch":sketch,"profiles":"all","face":face,"depth":"1mm","targets":[target]})).unwrap()["feature"]
                .as_u64()
                .unwrap();
        h.app.edit_feature(feature);
        h.frames(2);
        assert!(crate::dialogs::emboss_profile_plane(&h.app, sketch).unwrap().unwrap().origin.dist(expected) < 1e-8);
        if !target_owned {
            h.app.run("occurrence.copy", json!({"component":profile_owner,"translate":[100,0,0]})).unwrap();
            assert!(crate::dialogs::emboss_profile_plane(&h.app, sketch).unwrap_err().contains("ambiguous"));
            let mut guide = crate::gpu::GpuScene::default();
            crate::viewport::profile_fill(&mut guide, &h.app, sketch, 0, egui::Color32::BLUE, egui::Color32::BLUE);
            assert!(guide.trans.is_empty() && guide.lines.is_empty());
        }
    }
}

fn boxed() -> SolveApp {
    app_with(json!([{"command": "solid.box", "params": {"length": 40, "width": 30, "height": 20}}]))
}

/// The face of Body1 whose centre is at `c`.
fn face(app: &SolveApp, c: Vec3) -> Sel {
    let st = app.session.model.state();
    let b = st.body("Body1").unwrap();
    let f = b.body.faces(0.01).unwrap().into_iter().find(|f| f.centroid.dist(c) < 1e-6).unwrap();
    Sel::Face { body: "Body1".into(), index: f.index, point: c }
}

fn edge(app: &SolveApp, p: Vec3) -> Sel {
    let st = app.session.model.state();
    let m = st.body("Body1").unwrap().mesh();
    let i = m.edges.iter().position(|e| e.windows(2).any(|w| p.dist_to_segment(w[0], w[1]) < 1e-9)).unwrap();
    Sel::Edge { body: "Body1".into(), index: i, point: p }
}

fn start(app: &mut SolveApp, sels: Vec<Sel>, id: &str) -> Dialog {
    app.session.selection = sels;
    app.start(id);
    app.dialog.clone().unwrap()
}

#[test]
fn profiles_then_extrude_takes_them_all() {
    let mut app = app_with(json!([
        {"command": "sketch.create", "params": {"plane": "XY"}},
        {"command": "sketch.circle.center", "params": {"center": [0, 0], "radius": 5}},
        {"command": "sketch.circle.center", "params": {"center": [20, 0], "radius": 5}},
        {"command": "sketch.finish", "params": {}},
    ]));
    let sk = app.session.doc.features.first().unwrap().id;
    let d = start(&mut app, vec![Sel::Profile { sketch: sk, index: 0 }, Sel::Profile { sketch: sk, index: 1 }], "solid.extrude");
    assert_eq!(d.inputs[0].items.len(), 2);
    assert!(d.focus, "the value box takes the keyboard");
    assert!(apply_commands(&app, &d).is_ok());
}

#[test]
fn edges_then_fillet_takes_them_with_chains() {
    let mut app = boxed();
    let (a, b) = (edge(&app, Vec3::new(0.0, 0.0, 10.0)), edge(&app, Vec3::new(20.0, 0.0, 20.0)));
    let d = start(&mut app, vec![a, b], "solid.fillet");
    assert_eq!(d.inputs[0].items.len(), 2, "box edges are not tangent to others");
    let cmds = apply_commands(&app, &d).unwrap();
    assert_eq!(cmds[0].1["edges"].as_array().unwrap().len(), 2);
}

#[test]
fn face_then_extrude_press_pulls_it() {
    let mut app = boxed();
    let top = face(&app, Vec3::new(20.0, 15.0, 20.0));
    let d = start(&mut app, vec![top], "solid.extrude");
    let cmds = apply_commands(&app, &d).unwrap();
    assert!(cmds[0].1.get("face").is_some());
}

#[test]
fn faces_then_shell_and_face_then_hole() {
    let mut app = boxed();
    let top = face(&app, Vec3::new(20.0, 15.0, 20.0));
    let front = face(&app, Vec3::new(20.0, 0.0, 10.0));
    let d = start(&mut app, vec![top.clone(), front], "solid.shell");
    assert_eq!(d.inputs[0].items.len(), 2);
    let d = start(&mut app, vec![top], "solid.hole");
    assert_eq!(d.inputs[0].items.len(), 1);
    let cmds = apply_commands(&app, &d).unwrap();
    assert!(cmds[0].1.get("position").is_some());
}

#[test]
fn line_and_profile_then_revolve_routes_each() {
    let mut app = app_with(json!([
        {"command": "sketch.create", "params": {"plane": "XZ"}},
        {"command": "sketch.rectangle.two_point", "params": {"p0": [10, 0], "p1": [20, 10]}},
        {"command": "sketch.line", "params": {"points": [[0, 0], [0, 30]], "ids": ["axis"]}},
    ]));
    let sk = app.session.active_sketch.unwrap();
    let d = start(&mut app, vec![Sel::SketchCurve { id: "axis".into() }, Sel::Profile { sketch: sk, index: 0 }], "solid.revolve");
    assert_eq!(d.inputs[0].items, vec![Sel::Profile { sketch: sk, index: 0 }]);
    assert_eq!(d.inputs[1].items, vec![Sel::SketchCurve { id: "axis".into() }]);
}

#[test]
fn face_then_move_takes_the_body_and_odd_items_are_ignored() {
    let mut app = boxed();
    let top = face(&app, Vec3::new(20.0, 15.0, 20.0));
    let d = start(&mut app, vec![top, Sel::Axis { name: "X".into() }], "solid.move");
    assert_eq!(d.inputs[0].items, vec![Sel::Body { name: "Body1".into() }]);
    assert!(matches!(d.kind, Kind::Move { .. }));
}

#[test]
fn hole_at_sketch_points() {
    let mut app = app_with(json!([
        {"command": "solid.box", "params": {"length": 40, "width": 30, "height": 20}},
        {"command": "sketch.create", "params": {"plane": {"face": [20, 15, 20]}}},
        {"command": "sketch.point", "params": {"point": [10, 10], "id": "h1"}},
        {"command": "sketch.point", "params": {"point": [30, 20], "id": "h2"}},
        {"command": "sketch.finish", "params": {}},
    ]));
    let sk = app.session.doc.features.iter().find(|f| matches!(f.kind, solidcraft_engine::doc::FeatureKind::Sketch { .. })).unwrap().id;
    app.start("solid.hole");
    let mut d = app.dialog.clone().unwrap();
    if let Kind::Hole { opts, .. } = &mut d.kind {
        opts.multiple = true;
    }
    d.inputs = vec![crate::selection::SelInput::new("Points", crate::selection::POINTS, true)];
    d.inputs[0].items = vec![Sel::SketchPoint { id: format!("{sk}:h1") }, Sel::SketchPoint { id: format!("{sk}:h2") }];
    let cmds = apply_commands(&app, &d).unwrap();
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].1["points"], json!(["h1", "h2"]));
    let r = app.session.execute(&cmds[0].0, &cmds[0].1);
    assert!(r.is_ok(), "{r:?}");
}

#[test]
fn a_hole_is_patterned_as_a_feature() {
    let mut app = app_with(json!([
        {"command": "solid.box", "params": {"length": 60, "width": 40, "height": 10}},
        {"command": "solid.hole", "params": {"position": [10, 10, 10], "diameter": 5}}
    ]));
    let hole = app.session.doc.features.last().unwrap().id;
    // Picked in the timeline before the command: the objects become features.
    let mut d = start(&mut app, vec![Sel::Feature { id: hole }], "solid.pattern.rectangular");
    assert_eq!(d.inputs[0].accept, crate::selection::FEATURES);
    assert_eq!(d.inputs[0].items, vec![Sel::Feature { id: hole }]);
    d.inputs[1].items = vec![Sel::Axis { name: "X".into() }];
    if let Kind::PatternRect { count, spacing, .. } = &mut d.kind {
        *count = "4".into();
        *spacing = "12 mm".into();
    }
    let cmds = apply_commands(&app, &d).unwrap();
    assert_eq!(cmds[0].1["features"], json!(["Hole1"]));
    let before = app.session.model.state().bodies[0].mesh().measure().volume;
    for (id, p) in cmds {
        app.session.execute(&id, &p).unwrap();
    }
    let after = app.session.model.state().bodies[0].mesh().measure().volume;
    assert!(after < before - 500.0, "three more holes: {before} -> {after}");
    // On the canvas, a hole's wall stands for the hole; the top face for the box.
    let st = app.session.model.state();
    let m = st.bodies[0].mesh();
    let face_at = |p: Vec3| {
        let t = m.triangles.iter().position(|t| super::dialogs::point_tri_dist(p, m.tri(t).unwrap()) < 0.05).unwrap();
        let [a, b, c] = m.tri(&m.triangles[t]).unwrap();
        crate::viewport::Hit::Face { body: "Body1".into(), index: m.tri_face[t] as usize, point: (a + b + c) * (1.0 / 3.0) }
    };
    let mut d = start(&mut app, vec![], "solid.pattern.circular");
    d.inputs[0].accept = crate::selection::FEATURES;
    let box_id = app.session.doc.features[0].id;
    assert_eq!(d.candidate(&app.session, &face_at(Vec3::new(12.5, 10.0, 5.0))), Some(Sel::Feature { id: hole }));
    assert_eq!(d.candidate(&app.session, &face_at(Vec3::new(30.0, 30.0, 10.0))), Some(Sel::Feature { id: box_id }));
    app.dialog = None;
    // Edit Feature shows the hole as a feature again.
    let pat = app.session.doc.features.last().unwrap().id;
    let e = crate::dialogs::for_feature(&app, pat, None).unwrap();
    assert_eq!(e.inputs[0].accept, crate::selection::FEATURES);
    assert_eq!(e.inputs[0].items, vec![Sel::Feature { id: hole }]);
}

#[test]
fn a_hole_is_mirrored_as_a_feature() {
    let mut app = app_with(json!([
        {"command": "solid.box", "params": {"length": 60, "width": 40, "height": 10}},
        {"command": "solid.hole", "params": {"position": [10, 10, 10], "diameter": 5}}
    ]));
    let hole = app.session.doc.features.last().unwrap().id;
    let mut d = start(&mut app, vec![Sel::Feature { id: hole }], "solid.mirror");
    assert_eq!(d.inputs[0].accept, crate::selection::FEATURES);
    d.inputs[1].items = vec![Sel::Plane { name: "YZ".into() }];
    let cmds = apply_commands(&app, &d).unwrap();
    assert_eq!(cmds[0].1["features"], json!(["Hole1"]));
}

/// The radius arrow points into the body: dragging toward the material grows the round.
#[test]
fn fillet_and_chamfer_arrows_point_into_the_body() {
    for id in ["solid.fillet", "solid.chamfer"] {
        let mut app = boxed();
        let e = edge(&app, Vec3::new(20.0, 0.0, 20.0));
        let d = start(&mut app, vec![e], id);
        let (_, dir) = crate::canvas::anchor(&app, &d).unwrap();
        let dir = dir.unwrap();
        assert!(dir.y > 0.5 && dir.z < -0.5, "{id}: {dir:?}");
    }
}

#[test]
fn chamfer_types_send_their_values() {
    let mut app = boxed();
    let e = edge(&app, Vec3::new(20.0, 0.0, 20.0));
    let mut d = start(&mut app, vec![e], "solid.chamfer");
    let mut with = |t: usize| {
        if let Kind::Fillet { ctype, distance2, angle, flip, .. } = &mut d.kind {
            *ctype = t;
            *distance2 = "3 mm".into();
            *angle = "30 deg".into();
            *flip = true;
        }
        apply_commands(&app, &d).unwrap().remove(0).1
    };
    let p = with(0);
    assert!(p.get("distance2").is_none() && p.get("angle").is_none() && p.get("flip").is_none());
    let p = with(1);
    assert_eq!((p["distance2"].clone(), p["flip"].clone()), (json!("3 mm"), json!(true)));
    let p = with(2);
    assert_eq!(p["angle"], json!("30 deg"));
    assert!(p.get("distance2").is_none());
    // Applied and edited again, the chamfer keeps its type, angle and flip.
    let (id, p) = apply_commands(&app, &d).unwrap().remove(0);
    app.session.execute(&id, &p).unwrap();
    let ch = app.session.doc.features.last().unwrap().id;
    let e = crate::dialogs::for_feature(&app, ch, None).unwrap();
    assert!(matches!(&e.kind, Kind::Fillet { chamfer: true, ctype: 2, angle, flip: true, .. } if angle == "30 deg"), "{:?}", e.kind);
}

/// Radius Fit: the fillet reduces a radius too large for its edges, the preview shows the radius
/// it used, and editing the feature keeps the choice (#293).
#[test]
fn fillet_radius_fit_reduces_and_shows_the_applied_radius() {
    let mut app = app_with(serde_json::json!([
        {"command": "solid.box", "params": {"length": 14, "width": 6, "height": 10, "corner": [14, 0, 0]}}
    ]));
    let edges: Vec<Sel> = [Vec3::new(14.0, 0.0, 5.0), Vec3::new(28.0, 0.0, 5.0), Vec3::new(14.0, 6.0, 5.0), Vec3::new(28.0, 6.0, 5.0)]
        .into_iter()
        .map(|p| edge(&app, p))
        .collect();
    let mut d = start(&mut app, edges, "solid.fillet");
    if let Kind::Fillet { radius, .. } = &mut d.kind {
        *radius = "3.5 mm".into();
    }
    // Exact (the default) sends no fit and the preview refuses the radius.
    let (id, p) = apply_commands(&app, &d).unwrap().remove(0);
    assert!(p.get("fit").is_none(), "{p}");
    assert!(app.session.preview(&[(id.clone(), p)]).is_err());
    // Reduce to Fit: the preview builds it, at the radius that fits.
    d.extra.insert("fit".into(), json!("reduce"));
    let (id, p) = apply_commands(&app, &d).unwrap().remove(0);
    assert_eq!(p["fit"], json!("reduce"));
    let job = crate::preview::compute(&app.session, &[(id.clone(), p.clone())], crate::preview::Colors::from_tokens(&crate::theme::Tokens::dark()))
        .unwrap();
    assert_eq!(job.applied, Some(3.0));
    // Applied and edited again, the fillet keeps its fit.
    app.session.execute(&id, &p).unwrap();
    let fillet = app.session.doc.features.last().unwrap().id;
    let e = crate::dialogs::for_feature(&app, fillet, None).unwrap();
    assert_eq!(e.extra.get("fit"), Some(&json!("reduce")));
    // Its edit preview shows the radius it uses too.
    let cmds = apply_commands(&app, &e).unwrap();
    let job = crate::preview::compute(&app.session, &cmds, crate::preview::Colors::from_tokens(&crate::theme::Tokens::dark())).unwrap();
    assert_eq!(job.applied, Some(3.0));
}

/// Apply a dialog's commands to the session; the new feature's id.
fn apply(app: &mut SolveApp, d: &Dialog) -> u64 {
    for (id, p) in apply_commands(app, d).unwrap() {
        app.session.execute(&id, &p).unwrap();
    }
    app.session.doc.features.last().unwrap().id
}

#[test]
fn shell_direction_and_tangent_chain_round_trip() {
    let mut app = boxed();
    let top = face(&app, Vec3::new(20.0, 15.0, 20.0));
    let mut d = start(&mut app, vec![top], "solid.shell");
    // Tangent Chain is on, as in Fusion; Direction Outside.
    assert_eq!(d.extra.get("tangent_chain"), Some(&json!(true)));
    d.extra.insert("direction".into(), json!("outside"));
    let p = apply_commands(&app, &d).unwrap().remove(0).1;
    assert_eq!((p["direction"].clone(), p["tangent_chain"].clone()), (json!("outside"), json!(true)));
    let id = apply(&mut app, &d);
    let e = crate::dialogs::for_feature(&app, id, None).unwrap();
    assert_eq!(e.extra.get("direction"), Some(&json!("outside")));
    assert_eq!(e.extra.get("tangent_chain"), Some(&json!(true)));
}

#[test]
fn variable_fillet_round_trip() {
    let mut app = boxed();
    let e0 = edge(&app, Vec3::new(40.0, 15.0, 20.0));
    let mut d = start(&mut app, vec![e0], "solid.fillet");
    d.inputs[0].items.truncate(1);
    d.extra.insert("type".into(), json!("variable"));
    d.extra.insert("radius2".into(), json!("4 mm"));
    let p = apply_commands(&app, &d).unwrap().remove(0).1;
    assert_eq!((p["type"].clone(), p["radius2"].clone()), (json!("variable"), json!("4 mm")));
    assert!(p["start"].is_array(), "{p}");
    let id = apply(&mut app, &d);
    let e = crate::dialogs::for_feature(&app, id, None).unwrap();
    assert_eq!(e.extra.get("type"), Some(&json!("variable")));
    assert_eq!(e.extra.get("radius2"), Some(&json!("4 mm")));
}

#[test]
fn variable_fillet_mid_radii_round_trip() {
    let mut app = boxed();
    let e0 = edge(&app, Vec3::new(40.0, 15.0, 20.0));
    let mut d = start(&mut app, vec![e0], "solid.fillet");
    d.inputs[0].items.truncate(1);
    d.extra.insert("type".into(), json!("variable"));
    d.extra.insert("radius2".into(), json!("2 mm"));
    d.extra.insert("mid".into(), json!([[0.5, "4 mm"]]));
    d.extra.insert("smooth".into(), json!(true));
    let p = apply_commands(&app, &d).unwrap().remove(0).1;
    assert_eq!((p["mid"].clone(), p["smooth"].clone()), (json!([[0.5, "4 mm"]]), json!(true)), "{p}");
    let id = apply(&mut app, &d);
    let e = crate::dialogs::for_feature(&app, id, None).unwrap();
    assert_eq!(e.extra.get("mid"), Some(&json!([[0.5, "4 mm"]])));
    assert_eq!(e.extra.get("smooth"), Some(&json!(true)));
    // All middle radii removed: a plain two-radius fillet again.
    let mut d = e;
    d.extra.insert("mid".into(), json!([]));
    let p = apply_commands(&app, &d).unwrap().remove(0).1;
    // (an edit: the feature's new parameters)
    assert_eq!(p["params"]["mid"], json!([]), "{p}");
}

#[test]
fn newer_options_reach_their_commands() {
    let mut app = boxed();
    // Thread: Modeled.
    let side = face(&app, Vec3::new(20.0, 0.0, 10.0));
    let mut d = start(&mut app, vec![side.clone()], "solid.thread");
    d.extra.insert("modeled".into(), json!(true));
    assert_eq!(apply_commands(&app, &d).unwrap()[0].1["modeled"], json!(true));
    // Combine into a new component.
    app.dialog = None;
    let mut d = start(&mut app, vec![], "solid.combine");
    d.extra.insert("new_component".into(), json!(true));
    d.inputs[0].items = vec![Sel::Body { name: "Body1".into() }];
    d.inputs[1].items = vec![Sel::Body { name: "Body2".into() }];
    assert_eq!(apply_commands(&app, &d).unwrap()[0].1["new_component"], json!(true));
    // Hole: To Object sends the place to drill to instead of a depth.
    app.dialog = None;
    let top = face(&app, Vec3::new(20.0, 15.0, 20.0));
    let mut d = start(&mut app, vec![top], "solid.hole");
    d.inputs.push(crate::selection::SelInput::new("To Object", crate::selection::FACES | crate::selection::VERTICES, false));
    d.inputs[1].items = vec![face(&app, Vec3::new(20.0, 15.0, 0.0))];
    let p = apply_commands(&app, &d).unwrap().remove(0).1;
    assert!(p["to"].is_array() && p.get("depth").is_none(), "{p}");
}

#[test]
fn boundary_fill_finds_the_cells_and_fills_the_ticked_ones() {
    let mut app = app_with(json!([
        {"command": "solid.box", "params": {"length": 20, "width": 20, "height": 20}},
        {"command": "solid.box", "params": {"length": 20, "width": 20, "height": 20, "corner": [10, 10, 0], "operation": "new"}}
    ]));
    let mut d = start(&mut app, vec![], "solid.boundary_fill");
    d.inputs[0].items = vec![Sel::Body { name: "Body1".into() }, Sel::Body { name: "Body2".into() }];
    app.dialog = Some(d);
    // The dialog finds the cells on its next frame.
    let ctx = egui::Context::default();
    ctx.run_ui(Default::default(), |ui| crate::dialogs::show(&mut app, ui.ctx())).textures_delta.clear();
    let mut d = app.dialog.take().unwrap();
    let Kind::BoundaryFill { cells, .. } = &mut d.kind else { panic!() };
    // Only A, only B, and both.
    assert_eq!(cells.len(), 3, "{cells:?}");
    assert!(cells.iter().any(|c| c.1 == "Inside Body1, Body2"));
    // Fill just the overlap.
    for c in cells.iter_mut() {
        c.2 = c.1 == "Inside Body1, Body2";
    }
    let id = apply(&mut app, &d);
    assert!(app.session.model.result(id).is_some_and(|r| r.error.is_none()));
    // A new body filling the 10 x 10 x 20 overlap.
    let vols: Vec<f64> = app.session.model.state().bodies.iter().map(|b| b.mesh().measure().volume).collect();
    assert!(vols.len() == 3 && vols.iter().any(|v| (v - 2000.0).abs() < 1.0), "{vols:?}");
}

#[test]
fn align_snaps_to_centres_and_edge_middles() {
    let mut app = boxed();
    let mut d = start(&mut app, vec![], "solid.align");
    d.inputs[0].items = vec![Sel::Body { name: "Body1".into() }];
    d.inputs[1].items = vec![face(&app, Vec3::new(20.0, 15.0, 0.0))];
    d.inputs[2].items = vec![edge(&app, Vec3::new(20.0, 0.0, 20.0))];
    d.extra.insert("from_snap".into(), json!("center"));
    let p = apply_commands(&app, &d).unwrap().remove(0).1;
    assert_eq!(p["from"]["snap"], json!("face_center"));
    assert_eq!(p["to"]["snap"], json!("edge_mid"));
}

#[test]
fn surface_dialogs_make_their_commands() {
    // A planar patch of a sketch's rectangle, then thickened; Edit Feature brings both back.
    let mut app = app_with(json!([
        {"command": "sketch.create", "params": {"plane": "XY"}},
        {"command": "sketch.rectangle.two_point", "params": {"p0": [0, 0], "p1": [30, 20]}},
        {"command": "sketch.finish", "params": {}}
    ]));
    let sk = app.session.doc.features.last().unwrap().id;
    let mut d = start(&mut app, vec![], "surface.patch");
    d.inputs[0].items = vec![Sel::Profile { sketch: sk, index: 0 }];
    let patch = {
        for (id, p) in apply_commands(&app, &d).unwrap() {
            app.session.execute(&id, &p).unwrap();
        }
        app.session.doc.features.last().unwrap().id
    };
    let surf = app.session.model.state().bodies.last().unwrap().name.clone();
    assert!(matches!(crate::dialogs::for_feature(&app, patch, None).unwrap().kind, Kind::Surface(_)));
    app.dialog = None;
    let mut d = start(&mut app, vec![], "surface.thicken");
    d.inputs[0].items = vec![Sel::Body { name: surf }];
    let (id, p) = apply_commands(&app, &d).unwrap().remove(0);
    assert_eq!(id, "surface.thicken");
    app.session.execute(&id, &p).unwrap();
    let v = app.session.model.state().bodies.iter().map(|b| b.mesh().measure().volume).fold(0.0, f64::max);
    assert!((v - 30.0 * 20.0 * 2.0).abs() < 1.0, "{v}");
}

#[test]
fn patch_dialog_fills_a_loop_of_edges_that_is_not_planar() {
    // A box with its top cut away by a sideways cylinder: its rim is a closed loop of four
    // curved edges, not in a plane. Picking them in the Patch dialog fills it with one face.
    let mut app = app_with(json!([
        {"command": "solid.box", "params": {"length": 28, "width": 28, "height": 20, "corner": [-14, -14, 0], "body_name": "Saddle"}},
        {"command": "solid.cylinder", "params": {"radius": 40, "height": 120, "base": [0, -60, 50], "axis": "Y", "operation": "cut", "targets": ["Saddle"]}}
    ]));
    let z = |x: f64| 50.0 - (1600.0 - x * x).sqrt();
    let rim = [Vec3::new(14.0, 0.0, z(14.0)), Vec3::new(0.0, 14.0, z(0.0)), Vec3::new(-14.0, 0.0, z(-14.0)), Vec3::new(0.0, -14.0, z(0.0))];
    let mut d = start(&mut app, vec![], "surface.patch");
    d.inputs[0].items = rim.iter().enumerate().map(|(i, p)| Sel::Edge { body: "Saddle".into(), index: i, point: *p }).collect();
    let cmds = apply_commands(&app, &d).unwrap();
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].0, "surface.patch");
    app.session.execute(&cmds[0].0, &cmds[0].1).unwrap();
    let st = app.session.model.state();
    let fill = st.bodies.iter().find(|b| b.name != "Saddle").unwrap();
    assert!(fill.body.is_surface() && fill.body.face_count() == 1);
    // One smooth face over the rim: its area is close to the cylinder's inside the rim.
    let area = fill.mesh().measure().area;
    let want = 28.0 * 40.0 * 2.0 * (14.0f64 / 40.0).asin();
    assert!((area - want).abs() < want * 1e-2, "{area} vs {want}");
}

#[test]
fn split_body_by_a_curved_face_sends_it_as_a_tool() {
    let mut app = app_with(json!([
        {"command": "solid.box", "params": {"length": 40, "width": 30, "height": 20}},
        {"command": "solid.cylinder", "params": {"diameter": 20, "height": 40, "base": [20, 15, -10], "operation": "new"}}
    ]));
    let st = app.session.model.state();
    let cyl = st.bodies.iter().find(|b| b.name != "Body1").unwrap();
    let m = cyl.mesh();
    // A point on the round side.
    let p = Vec3::new(30.0, 15.0, 5.0);
    let t = m.triangles.iter().position(|t| crate::dialogs::point_tri_dist(p, m.tri(t).unwrap()) < 0.3).unwrap();
    let side = Sel::Face { body: cyl.name.clone(), index: m.tri_face[t] as usize, point: p };
    drop(st);
    let mut d = start(&mut app, vec![], "solid.split_body");
    d.inputs[0].items = vec![Sel::Body { name: "Body1".into() }];
    d.inputs[1].items = vec![side];
    let p = apply_commands(&app, &d).unwrap().remove(0).1;
    assert!(p["tool"]["point"].is_array() && p.get("plane").is_none(), "{p}");
}
