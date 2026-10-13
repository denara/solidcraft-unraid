use serde_json::json;
use solidcraft_engine::doc::{Document, FeatureKind, MeshData};
use solidcraft_engine::geom::Vec3;
use solidcraft_engine::{Sel, Session};
use solidcraft_ui_egui::scenario::Harness;

#[test]
fn convert_mesh_uses_preselection_and_reopens_its_timeline_dialog() {
    let body = solidcraft_engine::kernel::box_solid(Vec3::ZERO, Vec3::new(10., 20., 30.)).unwrap();
    let mesh = body.tessellate(0.01).unwrap();
    let mut doc = Document::default();
    doc.add_feature(
        FeatureKind::MeshImport {
            file: "box.stl".into(),
            meshes: vec![MeshData {
                name: "Mesh".into(),
                positions: mesh.positions.iter().flat_map(|p| [p.x, p.y, p.z]).collect(),
                triangles: mesh.triangles.iter().flatten().copied().collect(),
                color: None,
            }],
        },
        None,
    )
    .unwrap();
    let mut h = Harness::new();
    h.app.session = Session::new(doc);
    h.app.session.selection = vec![Sel::Body { name: "Mesh".into() }];
    h.step(&json!({"start":"mesh.convert"})).unwrap();
    let dialog = h.app.dialog.as_ref().unwrap();
    assert_eq!(dialog.inputs[0].items, vec![Sel::Body { name: "Mesh".into() }]);
    h.step(&json!({"widget":"OK"})).unwrap();
    assert!(h.app.dialog.is_none(), "{:?}", h.app.status);
    assert!(!h.app.session.model.state().body("Mesh").unwrap().body.is_mesh());
    let feature = h.app.session.doc.features.last().unwrap().id;
    let response = h.call("ui.editFeature", json!({"feature":feature}));
    assert_eq!(response["ok"], true, "{response}");
    let dialog = h.app.dialog.as_ref().unwrap();
    assert!(
        matches!(&dialog.kind,solidcraft_ui_egui::dialogs::Kind::Surface(solidcraft_ui_egui::dialogs_surface::Surf::ConvertMesh { tolerance }) if tolerance=="0.05 mm")
    );
    assert!(h.app.session.model.state().body("Mesh").unwrap().body.is_mesh());
    h.step(&json!({"key":"Escape"})).unwrap();
    assert!(!h.app.session.model.state().body("Mesh").unwrap().body.is_mesh());
}
