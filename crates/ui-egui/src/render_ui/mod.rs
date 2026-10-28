//! Render shell composition; all hosting and chrome belong to the shared kit.
use crate::{
    SolveApp,
    theme::Tokens,
    ui_kit::{self, Container, Edge},
};
use egui::{Rect, vec2};

pub(crate) fn reset_shell(app: &mut SolveApp) {
    app.shell.workspace_panels.retain(|id, _| !id.starts_with("render:"));
    app.shell.browser = None;
}

pub(crate) fn focused(app: &SolveApp) -> bool {
    crate::render_workspace::active(app) && app.shell.render_focus
}

pub(crate) fn compact(app: &SolveApp, width: f32) -> bool {
    width < 1100.0 || app.ui.layout == crate::shell::LayoutMode::Docked
}

pub(crate) fn focus_shortcut(app: &mut SolveApp, ui: &mut egui::Ui) {
    if crate::render_workspace::active(app) && !ui.ctx().text_edit_focused() && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab))
    {
        ui.ctx().memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
        app.start("view.render_focus");
    }
}

pub(crate) fn canvas(app: &SolveApp, viewport: Rect) -> Rect {
    crate::shell::content_rect(app, viewport)
}

/// The toolbar's shared floating Chrome card hosts both rows; there is no Render ribbon.
pub(crate) fn stage_bar(app: &mut SolveApp, ui: &mut egui::Ui, rect: Rect) {
    use crate::render_workspace::Stage;
    crate::scenario::publish_rect("render:stage_bar", rect);
    ui.ctx().data_mut(|d| d.remove::<Rect>(egui::Id::new("sc_ribbon_search")));
    let labels: Vec<_> = Stage::ALL.iter().map(|stage| app.tr(stage.label()).to_string()).collect();
    let selected = Stage::ALL.iter().position(|s| *s == app.render_workspace.view.stage).unwrap_or(0);
    let ready = [crate::render_workspace::job_ready(app, "render.start"), crate::render_workspace::job_ready(app, "render.cancel")];
    let actions = [
        ui_kit::StageAction {
            icon: Some("play"),
            label: app.tr_label("Render"),
            enabled: ready[0].is_ok(),
            primary: true,
            reason: ready[0].as_ref().err().map(String::as_str),
        },
        ui_kit::StageAction {
            icon: None,
            label: app.tr_label("Cancel"),
            enabled: ready[1].is_ok(),
            primary: false,
            reason: ready[1].as_ref().err().map(String::as_str),
        },
    ];
    let choice = ui_kit::StageBar::row(ui, rect, false, |ui| {
        ui_kit::StageBar::header(ui, selected, &labels.iter().map(String::as_str).collect::<Vec<_>>(), &actions)
    });
    if let Some(choice) = choice {
        for (stage, chip) in Stage::ALL.iter().zip(choice.chips).filter(|(_, r)| r.is_positive()) {
            crate::scenario::publish_target(&format!("render:stage:{}", stage.key()), chip);
        }
        for (id, action) in ["render.start", "render.cancel"].iter().zip(choice.actions).filter(|(_, r)| r.is_positive()) {
            crate::scenario::publish_target(&format!("render:stage:{id}"), action);
        }
        if let Some(stage) = choice.stage.and_then(|index| Stage::ALL.get(index)) {
            let _ = app.run("view.render_stage", serde_json::json!({"stage":stage.key()}));
        }
        if let Some(id) = choice.action.and_then(|index| ["render.start", "render.cancel"].get(index)) {
            app.start(id);
        }
    }
    let split = (rect.top() + ui_kit::StageBar::HEADER_HEIGHT).min(rect.bottom());
    crate::scenario::publish_rect("render:stage_header", Rect::from_min_max(rect.min, egui::pos2(rect.right(), split)));
    crate::scenario::publish_rect("render:stage_divider", Rect::from_min_size(egui::pos2(rect.left(), split), vec2(rect.width(), 1.0)));
    crate::scenario::publish_rect("render:stage_tools", Rect::from_min_max(egui::pos2(rect.left(), split), rect.max));
    ui_kit::StageBar::row(ui, rect, true, |ui| stage_tools(app, ui));
}

fn stage_tools(app: &mut SolveApp, ui: &mut egui::Ui) {
    use crate::render_workspace::Stage;
    match app.render_workspace.view.stage {
        Stage::Look => {
            ui_kit::StageBar::caption(ui, app.tr("render.pick_label"));
            let modes = ["body", "face", "component"];
            let mut selected = modes.iter().position(|mode| *mode == current_pick_mode(app)).unwrap_or(0);
            let pick = ui.add_enabled_ui(!crate::render_workspace::result_active(app), |ui| {
                if ui_kit::segmented(
                    ui,
                    &mut selected,
                    &[app.tr("render.pick_body"), app.tr("render.pick_face"), app.tr("render.pick_component")],
                    ui_kit::Segments {
                        targets: &["render:pick:body", "render:pick:face", "render:pick:component"],
                        icons: &["body", "surface", "component"],
                        workspace_accent: true,
                        dense: true,
                        intrinsic: true,
                        ..Default::default()
                    },
                ) && let Some(mode) = modes.get(selected)
                {
                    let _ = app.run("select.filter", serde_json::json!({"mode":mode}));
                }
            });
            crate::scenario::publish_target("render:pick", pick.response.rect);
            pick.response.on_hover_text(app.tr("render.pick_help"));
            ui_kit::StageBar::tool_gap(ui);
            for (id, label, icon) in
                [("appearance.paint", "render.paint_label", "appearance"), ("appearance.pick_up", "render.pick_up_label", "select")]
            {
                if let Some(command) = solidcraft_engine::find_command(id) {
                    let info = crate::render_workspace::interactive_command_info(app, command);
                    let response = ui_kit::compact_icon_label_button(ui, app.tr(label), icon, info.enabled);
                    if response.clicked() {
                        app.start(id);
                    }
                    if let Some(reason) = info.disabled_reason {
                        response.on_disabled_hover_text(reason);
                    }
                }
            }
            ui_kit::StageBar::separator(ui);
            let note = ui_kit::StageBar::note(ui, app.tr("render.stage_preview_note")).on_hover_text(app.tr("render.preview_lighting_help"));
            crate::scenario::publish_rect("render:preview_note", note.rect);
            if ui_kit::StageBar::icon_action(ui, "settings", app.tr("render.preview_image_settings_help")).clicked() {
                app.start("render.settings.get");
            }
        }
        Stage::Light => {
            ui_kit::text(ui, app.tr("render.lights_fixed_to"), ui_kit::Text::Inline);
            ui.add_enabled_ui(false, |ui| {
                let mut selected = usize::MAX;
                ui_kit::segmented(
                    ui,
                    &mut selected,
                    &[app.tr("render.fixed_camera"), app.tr("render.fixed_world")],
                    ui_kit::Segments { intrinsic: true, ..Default::default() },
                );
            })
            .response
            .on_disabled_hover_text(app.tr("render.light_anchor_pending"));
            ui_kit::text(ui, app.tr("render.light_anchor_pending"), ui_kit::Text::Inline);
        }
        Stage::Frame => {
            let mut projection = usize::from(app.cam.fov == 0.0);
            if ui_kit::segmented(
                ui,
                &mut projection,
                &[app.tr("render.perspective"), app.tr("render.stage_orthographic")],
                ui_kit::Segments { intrinsic: true, ..Default::default() },
            ) && let Err(error) = crate::render_workspace::stage_projection(app, projection == 0)
            {
                app.set_status(error, true);
            }
        }
        Stage::Output => {
            ui.add_enabled_ui(false, |ui| {
                let (size, format) = crate::render_workspace::output_summary(app);
                ui_kit::button(ui, size, ui_kit::Button::Normal);
                ui_kit::button(ui, format, ui_kit::Button::Normal);
            })
            .response
            .on_disabled_hover_text(app.tr("render.output_tools_inspector"));
            ui_kit::text(ui, app.tr("render.output_tools_inspector"), ui_kit::Text::Inline);
        }
    }
}

pub(crate) fn overlays(app: &mut SolveApp, ui: &mut egui::Ui) {
    crate::shell::register_panel(app, "render:focus:exit", None, None);
    if focused(app) {
        dock_chip(app, ui, "render:focus:exit", "render.exit_focus", "view.render_focus", serde_json::json!({"on":false}));
    }
    let Some(viewport) = app.viewport.rect else { return };
    let free = canvas(app, viewport);
    if !free.is_positive() {
        return;
    }
    crate::navigation_bar_view::show(app, ui, viewport);
    if !focused(app) {
        shots_pill(app, ui, free);
    }
}

/// Shared kit chip at the Browser's remembered side, below the stage bar.
pub(crate) fn dock_chip(app: &mut SolveApp, ui: &mut egui::Ui, id: &'static str, label: &str, command: &str, params: serde_json::Value) {
    let bounds = crate::shell::panel_bounds(app, ui.max_rect());
    let t = Tokens::get();
    if bounds.width() < t.control_height || bounds.height() < t.control_height {
        return;
    }
    let left = app.preferences.render_browser_side == "left";
    let position = if left { bounds.left_top() + vec2(t.space_8, 0.0) } else { bounds.right_top() - vec2(t.space_8, 0.0) };
    let ctx = ui.ctx().clone();
    let chip =
        Container::navigation_card(&ctx, egui::Id::new(id), position, if left { egui::Align2::LEFT_TOP } else { egui::Align2::RIGHT_TOP }, |ui| {
            let response = ui_kit::choice_chip(ui, id, app.tr(label), false);
            crate::scenario::publish_target(id, response.rect);
            if response.clicked() {
                let _ = app.run(command, params);
            }
        });
    crate::shell::register_panel(app, id, Some(chip.response.rect), None);
}

fn current_pick_mode(app: &SolveApp) -> &'static str {
    if app.ui.pick_components {
        "component"
    } else if app.ui.pick_bodies {
        "body"
    } else {
        "face"
    }
}

fn shots_pill(app: &mut SolveApp, ui: &mut egui::Ui, free: Rect) {
    crate::shell::register_panel(app, "render:shots:show", None, None);
    if app.preferences.render_shots_strip {
        return;
    }
    let ctx = ui.ctx().clone();
    let pill = Container::navigation_card(
        &ctx,
        egui::Id::new("sc_render_shots_pill"),
        free.left_bottom() + vec2(Tokens::get().space_8, -Tokens::get().space_8),
        egui::Align2::LEFT_BOTTOM,
        |ui| {
            let response =
                ui_kit::button(ui, crate::i18n::tr_label("Shots"), ui_kit::Button::Normal).on_hover_text(crate::i18n::tr_label("Show Shots strip"));
            crate::scenario::publish_target("render:shots:show", response.rect);
            if response.clicked() {
                let _ = app.run("view.shots_strip", serde_json::json!({"visible":true}));
            }
        },
    );
    crate::shell::register_panel(app, "render:shots:show", Some(pill.response.rect), None);
}

/// Existing saved-shot and job actions inside the shared floating bottom-strip host.
pub(crate) fn shots_strip(app: &mut SolveApp, ui: &mut egui::Ui) {
    crate::shell::register_panel(app, "render:shots_strip", None, None);
    if focused(app) || !app.preferences.render_shots_strip {
        return;
    }
    let available = ui.available_rect_before_wrap();
    let content = ui_kit::Panel::bottom_strip(ui, "sc_render_shots_strip", available, 48.0, |ui| {
        ui.horizontal_centered(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let hide = ui_kit::button(ui, crate::i18n::tr_label("Hide"), ui_kit::Button::Normal)
                    .on_hover_text(crate::i18n::tr_label("Hide Shots strip"));
                crate::scenario::publish_target("render:shots:hide", hide.rect);
                if hide.clicked() {
                    let _ = app.run("view.shots_strip", serde_json::json!({"visible":false}));
                }
                let ready = crate::render_workspace::job_ready(app, "render.start");
                let render = ui_kit::button(ui, app.tr("render.image_start"), ui_kit::Button::Enabled(ready.is_ok()));
                let render = if let Err(reason) = ready { render.on_disabled_hover_text(reason) } else { render };
                crate::scenario::publish_target("render:shots:render", render.rect);
                if render.clicked() {
                    app.start("render.start");
                }
                let names: Vec<_> = app.session.doc.render.setups.keys().cloned().collect();
                egui::ScrollArea::horizontal()
                    .id_salt("render_shots_names")
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui_kit::text(ui, crate::i18n::tr_label("Shots"), ui_kit::Text::Inline);
                            for name in names {
                                let shot = ui_kit::choice_chip(ui, &name, &name, name == app.render_workspace.view.name);
                                crate::scenario::publish_target(&format!("render:shots:setup:{name}"), shot.rect);
                                if shot.clicked() {
                                    crate::render_workspace::switch_shot(app, name);
                                }
                            }
                        });
                    });
            });
        });
    });
    let rect = content.map(|c| c.response.rect);
    crate::shell::register_panel(app, "render:shots_strip", rect, Some(Edge::Bottom));
    if let Some(rect) = rect {
        crate::scenario::publish_rect("render:shots_strip", rect);
    }
}
