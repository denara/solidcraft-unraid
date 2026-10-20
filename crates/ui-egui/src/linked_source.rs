//! Explicitly choose a linked component's source; all its instances share that source.

use std::sync::Arc;

use serde_json::json;

use crate::SolveApp;

pub(crate) fn start_selected(app: &mut SolveApp) {
    let mut components = app.tree.picked_components.clone();
    for selection in &app.session.selection {
        if let solidcraft_engine::Sel::Occurrence { path } = selection
            && let Ok(placement) = app.session.occurrence_placement(path)
        {
            components.push(placement.component);
        }
    }
    components.sort_unstable();
    components.dedup();
    if let [component] = components.as_slice()
        && app.session.doc.links.contains_key(component)
    {
        start(app, *component);
    } else {
        app.set_status(app.tr("ui.relink_select_one").to_string(), true);
    }
}

pub(crate) fn start(app: &mut SolveApp, component: u64) {
    if app.file_picker_busy() || app.file_load.busy() {
        return;
    }
    if !app.session.doc.links.contains_key(&component) {
        app.set_status(app.tr_label("ui.relink_not_linked").to_string(), true);
        return;
    }
    let document = app.session.doc.clone();
    app.pick_open_for(&["solidcraft"], move |app, path| {
        let Some(path) = path else { return };
        // A host or programmatic caller can replace/edit a design while a native picker is
        // pending. Never apply that old component id to a different design or newer revision.
        if !Arc::ptr_eq(&document, &app.session.doc) {
            app.set_status(app.tr_label("ui.relink_design_changed").to_string(), true);
            return;
        }
        if let Ok(result) = app.run("component.relink", json!({"component": component, "path": path})) {
            let warnings: Vec<&str> = result["warnings"].as_array().into_iter().flatten().filter_map(|w| w.as_str()).collect();
            if warnings.is_empty() {
                app.set_status(app.tr_label("ui.relink_done").to_string(), false);
            } else {
                let text = format!("{} {}", app.tr_label("ui.relink_done"), warnings.join("; "));
                app.session.echo(text.clone());
                app.set_status(text, true);
            }
        }
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::context_menu::{self, Target};
    use solidcraft_engine::Session;
    use std::path::PathBuf;

    fn fixture(name: &str) -> (SolveApp, u64, u64, PathBuf, PathBuf) {
        let folder = std::env::temp_dir().join(format!("solidcraft-relink-ui-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let source = folder.join("source.solidcraft");
        let moved = folder.join("moved.solidcraft");
        let mut part = Session::default();
        part.execute("solid.box", &json!({"length": 10, "width": 20, "height": 30})).unwrap();
        part.execute("file.save", &json!({"path": source})).unwrap();
        let mut app = SolveApp::new(Session::default(), Default::default());
        let inserted = app.run("component.insert_design", json!({"path": source})).unwrap();
        let component = inserted["component"].as_u64().unwrap();
        let occurrence = inserted["occurrence"].as_u64().unwrap();
        std::fs::rename(&source, &moved).unwrap();
        (app, component, occurrence, folder, moved)
    }

    fn menu(app: &SolveApp, target: Target) -> Option<context_menu::Item> {
        context_menu::items(app, &target).into_iter().find(|i| i.id == "ui.relinkSource")
    }

    fn answer(app: &mut SolveApp, path: Option<String>) {
        app.services.pick_open = Some(Box::new(move |extensions| {
            assert_eq!(extensions, &["solidcraft"]);
            let result = path.clone();
            Box::pin(async move { result })
        }));
    }

    #[test]
    fn relink_menu_uses_the_selected_link_from_components_and_occurrences() {
        let (mut app, component, occurrence, folder, moved) = fixture("menu");
        let second = app.run("occurrence.copy", json!({"occurrence": occurrence, "translate": [90,0,0]})).unwrap()["occurrence"].as_u64().unwrap();
        let occurrences = app.session.doc.occurrences.clone();
        let item = menu(&app, Target::Component { id: component }).unwrap();
        assert!(item.label.contains("All Instances"));
        assert_eq!(menu(&app, Target::Occurrence { path: vec![second] }).unwrap().params, Some(json!({"component": component})));
        assert!(menu(&app, Target::Component { id: 0 }).is_none());
        answer(&mut app, Some(moved.to_string_lossy().into_owned()));
        let before = app.session.doc.clone();
        context_menu::run_item(&mut app, &item, egui::Pos2::ZERO);
        assert!(app.file_picker_busy());
        assert!(Arc::ptr_eq(&before, &app.session.doc), "choosing is not an edit");
        crate::file_picker::tick(&mut app, &egui::Context::default());
        assert!(!app.file_picker_busy());
        assert!(app.session.doc.links[&component].path.ends_with("moved.solidcraft"));
        assert_eq!(app.session.doc.occurrences, occurrences);
        assert!(app.status.as_ref().is_some_and(|(s, _, error)| s.contains("all instances") && !error));
        app.run("edit.undo", json!({})).unwrap();
        assert_eq!(serde_json::to_value(&*app.session.doc).unwrap(), serde_json::to_value(&*before).unwrap());
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn relink_picker_cancel_bad_file_and_changed_document_keep_the_link() {
        let (mut app, component, _, folder, moved) = fixture("cancel");
        let before = serde_json::to_value(&*app.session.doc).unwrap();
        let undo = app.session.undo.len();
        answer(&mut app, None);
        start(&mut app, component);
        crate::file_picker::tick(&mut app, &egui::Context::default());
        assert_eq!(serde_json::to_value(&*app.session.doc).unwrap(), before);
        assert_eq!(app.session.undo.len(), undo);
        answer(&mut app, Some(folder.join("missing.solidcraft").to_string_lossy().into_owned()));
        start(&mut app, component);
        crate::file_picker::tick(&mut app, &egui::Context::default());
        assert_eq!(serde_json::to_value(&*app.session.doc).unwrap(), before);
        assert_eq!(app.session.undo.len(), undo);
        assert!(app.status.as_ref().is_some_and(|(_, _, error)| *error));
        answer(&mut app, Some(moved.to_string_lossy().into_owned()));
        start(&mut app, component);
        // The ordinary UI blocks edits while picking. Simulate a host replacing/editing the
        // session directly to exercise the continuation's independent stale-target guard.
        app.session.execute("component.rename", &json!({"component": component, "name": "Changed"})).unwrap();
        let changed = serde_json::to_value(&*app.session.doc).unwrap();
        crate::file_picker::tick(&mut app, &egui::Context::default());
        assert_eq!(serde_json::to_value(&*app.session.doc).unwrap(), changed);
        assert!(app.status.as_ref().is_some_and(|(s, _, error)| s.contains("assembly changed") && *error));
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn relink_start_requires_one_linked_selection_and_opens_its_picker() {
        let (mut app, component, occurrence, folder, moved) = fixture("start");
        answer(&mut app, Some(moved.to_string_lossy().into_owned()));
        app.start("component.relink");
        assert!(!app.file_picker_busy());
        assert!(app.status.as_ref().is_some_and(|(s, _, error)| s.contains("Select one linked") && *error));
        app.session.selection = vec![solidcraft_engine::Sel::Occurrence { path: vec![occurrence] }];
        app.start("component.relink");
        assert!(app.file_picker_busy());
        crate::file_picker::tick(&mut app, &egui::Context::default());
        assert!(app.session.doc.links[&component].path.ends_with("moved.solidcraft"));
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn relink_displays_the_warning_when_changed_geometry_has_joints() {
        let (mut app, component, occurrence, folder, moved) = fixture("warning");
        app.run(
            "joint.create",
            json!({"name": "Seat", "type": "rigid", "a": {"occurrence": 0, "point": [0,0,0]}, "b": {"occurrence": occurrence, "point": [0,0,0]}}),
        )
        .unwrap();
        let mut changed = Session::default();
        changed.execute("solid.box", &json!({"length": 10, "width": 20, "height": 45})).unwrap();
        changed.execute("file.save", &json!({"path": moved})).unwrap();
        answer(&mut app, Some(moved.to_string_lossy().into_owned()));
        start(&mut app, component);
        crate::file_picker::tick(&mut app, &egui::Context::default());
        assert!(app.status.as_ref().is_some_and(|(s, _, error)| s.contains("check this component's joints") && *error));
        assert!(app.session.log.iter().any(|s| s.contains("check this component's joints")));
        let _ = std::fs::remove_dir_all(folder);
    }
}
