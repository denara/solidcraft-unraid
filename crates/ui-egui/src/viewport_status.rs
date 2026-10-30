//! Thin presentation adapter: persistent effects and activation order belong to the session.
use crate::{
    SolveApp,
    ui_kit::{self, StatusAction, StatusItem, StatusKind},
};
use solidcraft_engine::status_items::{StatusId, StatusItem as Effect, StatusSetting};

const PRESENTED: &[StatusId] = &[StatusId::SurfaceAnalysis, StatusId::Continuity];

fn identity(effect: &Effect) -> String {
    format!("{:?}:{}", effect.id, effect.command)
}

fn direction(value: [f64; 3]) -> String {
    for (axis, vector) in [("X", [1.0, 0.0, 0.0]), ("Y", [0.0, 1.0, 0.0]), ("Z", [0.0, 0.0, 1.0])] {
        if value.iter().zip(vector).all(|(a, b)| (*a - b).abs() < 1e-8) {
            return axis.into();
        }
        if value.iter().zip(vector).all(|(a, b)| (*a + b).abs() < 1e-8) {
            return format!("−{axis}");
        }
    }
    let [x, y, z] = value;
    format!("({x:.2}, {y:.2}, {z:.2})")
}

pub(crate) fn presentation(app: &SolveApp, effect: &Effect) -> Option<StatusItem> {
    if !PRESENTED.contains(&effect.id) {
        return None;
    }
    let command = solidcraft_engine::find_command(&effect.command)?;
    let setting = match effect.setting {
        StatusSetting::Zebra { stripes } => crate::i18n::format("ui.viewport_status_stripes", &[("count", &stripes.to_string())]),
        StatusSetting::Draft { pull, angle_deg } => format!("{} · {angle_deg}°", direction(pull)),
        StatusSetting::Environment => String::new(),
        StatusSetting::Accessibility { direction: from } => crate::i18n::format("ui.viewport_status_from", &[("direction", &direction(from))]),
        StatusSetting::Curvature { radius_mm } => format!("R {}", crate::prefs::show_mm(app, radius_mm, 1)),
        StatusSetting::Continuity { edge_count } => crate::i18n::format("ui.viewport_status_edges", &[("count", &edge_count.to_string())]),
        StatusSetting::Section { .. } => return None,
    };
    Some(StatusItem {
        id: identity(effect),
        kind: StatusKind::Mode,
        name: crate::toolbar::persistent_button_name(app, command),
        setting,
        has_key: effect.has_key,
        editable: false,
    })
}

fn clear(app: &mut SolveApp, effect: &Effect) {
    let _ = app.run(&effect.clear.command, serde_json::json!({"clear": effect.clear.params.clear}));
}

pub(crate) fn has_effect(app: &SolveApp) -> bool {
    app.session.newest_status_item(PRESENTED).is_some()
}

/// Called only after existing Escape owners have yielded. A repeat cannot cascade clears.
pub(crate) fn escape(app: &mut SolveApp, repeat: bool) -> bool {
    if repeat {
        return app.viewport_status.open_key.is_some() || has_effect(app);
    }
    if let Some(open) = app.viewport_status.open_key.take()
        && app.session.status_items().iter().any(|effect| effect.has_key && identity(effect) == open)
    {
        return true;
    }
    if let Some(effect) = app.session.newest_status_item(PRESENTED) {
        clear(app, &effect);
        return true;
    }
    false
}

pub(crate) fn show(app: &mut SolveApp, ctx: &egui::Context, viewport: egui::Rect) {
    if !app.preferences.viewport_analysis_status {
        app.viewport_status.open_key = None;
        return;
    }
    let effects = app.session.status_items();
    let items: Vec<_> = effects.iter().filter_map(|effect| presentation(app, effect)).collect();
    if items.is_empty() {
        app.viewport_status = Default::default();
        return;
    }
    let content = crate::shell::content_rect(app, viewport);
    if !content.is_finite() || !content.is_positive() {
        return;
    }
    let mut free = crate::shell::workspace_content(app, content);
    for card in &app.shell.cards {
        free.min.y = free.top().max(card.bottom()).min(free.bottom());
    }
    if let Some(layout) = &app.viewport.layout {
        free.max.x = free.right().min(layout.axis_view.left()).max(free.left());
    }
    if !free.is_finite() || !free.is_positive() {
        return;
    }
    let legend = crate::viewport::analysis_legend_data(app);
    let actions = ui_kit::viewport_status_row(ctx, free, &items, &mut app.viewport_status, |ui, _| {
        if let Some((title, along, rows)) = &legend {
            ui_kit::analysis_key(ui, title, along.as_deref(), rows);
        }
    });
    for action in actions {
        if let StatusAction::Clear(id) = action
            && let Some(effect) = effects.iter().find(|effect| identity(effect) == id)
        {
            clear(app, effect);
            app.viewport_status.open_key = None;
        }
    }
}
