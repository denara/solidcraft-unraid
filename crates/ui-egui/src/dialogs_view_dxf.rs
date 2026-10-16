//! Export the camera at the instant Export is pressed, with a remembered hidden-edge choice.
use crate::{SolveApp, dialogs::Kind, selection::SelInput};
use serde_json::json;

pub fn start(app: &SolveApp, id: &str) -> Option<(Kind, Vec<SelInput>)> {
    (id == "file.export_view_dxf").then(|| (Kind::ViewDxf { dashed: app.preferences.view_dxf_dashed }, Vec::new()))
}

pub fn rows(app: &SolveApp, ui: &mut egui::Ui, dashed: &mut bool) {
    crate::dialogs::row_label(ui, "Hidden edges");
    ui.vertical(|ui| {
        let r = ui.radio_value(dashed, false, app.tr_label("Do not export hidden edges"));
        crate::scenario::publish_handle("dialog:dxf:omit", r.rect.center());
        let r = ui.radio_value(dashed, true, app.tr_label("Export hidden edges as dashed lines"));
        crate::scenario::publish_handle("dialog:dxf:dashed", r.rect.center());
    });
    ui.end_row();
    crate::dialogs::row_label(ui, "Projection");
    ui.label(app.tr_label(if app.cam.fov > 0.0 { "Current perspective view" } else { "Current orthographic view (1:1 mm)" }));
    ui.end_row();
    crate::dialogs::row_label(ui, "");
    ui.label(app.tr_label("Visible bodies only. Curves use line segments (0.01 mm tolerance)."));
    ui.end_row();
}

pub fn run(app: &mut SolveApp, dashed: bool) -> Result<(), String> {
    if crate::viewport::section_plane(app).is_some() {
        return Err(app.tr_label("Turn off section analysis before exporting the view.").into());
    }
    if !app.session.world_state().bodies.iter().any(|b| app.session.body_visible(&b.name)) {
        return Err(app.tr_label("No visible bodies to export.").into());
    }
    app.preferences.view_dxf_dashed = dashed;
    let camera = app.cam;
    app.pick_save(&format!("{}-view.dxf", app.session.doc.name), &["dxf"], move |app, path| {
        if let Some(path) = path {
            let params = json!({"camera":camera,"hidden":if dashed {"dashed"} else {"omit"},"path":path});
            if app.run("file.export_view_dxf", params).is_ok() {
                app.set_status(crate::i18n::format("ui.saved_to", &[("path", &path)]), false);
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use solidcraft_engine::render::StandardView;

    #[test]
    fn view_dxf_file_menu_exports_with_the_current_camera_and_remembers_hidden_choice() {
        let mut h = crate::scenario::Harness::new();
        h.app.run("solid.box", json!({"length":40,"width":30,"height":10})).unwrap();
        h.app.run("solid.cylinder", json!({"radius":3,"height":4,"base":[10,10,0],"operation":"cut"})).unwrap();
        h.app.cam.fov = 0.0;
        h.app.cam.set_view(StandardView::Top);
        h.frames(3);
        h.step(&json!({"click":{"handle":"appbar:folder"}})).unwrap();
        h.step(&json!({"widget":"Export Current View as DXF…"})).unwrap();
        assert!(matches!(h.app.dialog.as_ref().map(|d| &d.kind), Some(Kind::ViewDxf { dashed: false })));
        h.step(&json!({"click":{"handle":"dialog:dxf:dashed"}})).unwrap();
        let dir = h.dir.clone();
        h.app.services.pick_save = Some(Box::new(move |name, extensions| {
            assert!(name.ends_with("-view.dxf"));
            assert_eq!(extensions, &["dxf"]);
            Box::pin(std::future::ready(Some(dir.join("view.dxf").to_string_lossy().into_owned())))
        }));
        h.step(&json!({"click":{"handle":"dialog:Export DXF…"}})).unwrap();
        h.frames(3);
        let text = std::fs::read_to_string(h.dir.join("view.dxf")).unwrap();
        assert!(text.contains("LWPOLYLINE") && text.contains("HIDDEN"));
        assert!(h.app.dialog.is_none() && h.app.preferences.view_dxf_dashed);
        let saved = h.app.prefs();
        let mut restored = SolveApp::new(Default::default(), Default::default());
        restored.load_prefs(&saved);
        assert!(restored.preferences.view_dxf_dashed);
        h.app.start("file.export_view_dxf");
        h.frames(3);
        assert!(matches!(h.app.dialog.as_ref().map(|d| &d.kind), Some(Kind::ViewDxf { dashed: true })));
        h.step(&json!({"click":{"handle":"dialog:dxf:omit"}})).unwrap();
        h.step(&json!({"click":{"handle":"dialog:Export DXF…"}})).unwrap();
        h.frames(3);
        let text = std::fs::read_to_string(h.dir.join("view.dxf")).unwrap();
        assert!(!text.contains("LWPOLYLINE") && !text.contains("HIDDEN"));
        assert!(!h.app.preferences.view_dxf_dashed);
    }
}
