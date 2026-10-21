//! Settings → AI: the master switch, the agents (found and added, each with Test, Default and,
//! for added ones, Remove), permissions (global and per agent, and the "always allow" answers),
//! and the panel. Plus the Add agent dialog: an installed ACP agent, a custom command, or an
//! OpenAI-compatible endpoint (whose key goes to the system keychain).

use super::agents::{self, AgentConfig, Kind, Mode};
use super::*;
use crate::ui_kit;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AddKind {
    #[default]
    Installed,
    Custom,
    Api,
}

/// The Add agent dialog's fields.
#[derive(Default)]
pub struct AddState {
    pub kind: AddKind,
    pub name: String,
    pub command: String,
    pub installed: usize,
    pub preset: usize,
    pub base_url: String,
    pub model: String,
    pub key: String,
    pub error: Option<String>,
    test: Option<Test>,
}

/// A connection test running or done.
pub enum Test {
    Running(std::sync::mpsc::Receiver<Result<String, String>>),
    Done(Result<String, String>),
}

impl Test {
    fn poll(&mut self) {
        if let Test::Running(rx) = self
            && let Ok(r) = rx.try_recv()
        {
            *self = Test::Done(r);
        }
    }
}

/// Settings state that isn't preferences: the dialog and the tests.
#[derive(Default)]
pub struct State {
    pub add: Option<AddState>,
    pub tests: std::collections::HashMap<String, Test>,
}

/// Open the Add agent dialog.
pub fn open_add(app: &mut SolveApp) {
    let preset = agents::API_PRESETS.first();
    app.agent_chat.settings.add = Some(AddState {
        base_url: preset.map(|p| p.1.to_string()).unwrap_or_default(),
        name: preset.map(|p| p.0.to_string()).unwrap_or_default(),
        ..Default::default()
    });
}

/// Run a connection test in the background (it starts the agent briefly, or asks the endpoint
/// for its models).
#[cfg(not(target_arch = "wasm32"))]
fn start_test(app: &SolveApp, e: &agents::Entry, key: Option<String>) -> Test {
    let (tx, rx) = std::sync::mpsc::channel();
    match e.kind {
        Kind::Acp => {
            let argv = e.argv.clone();
            std::thread::spawn(move || {
                let _ = tx.send(super::acp_client::test_agent(&argv, &working_dir()));
            });
        }
        Kind::Api => {
            let Some(http) = app.services.agent_http.clone() else { return Test::Done(Err(tr("ui.ai_no_http").to_string())) };
            let (url, model) = (e.base_url.clone(), e.model.clone());
            std::thread::spawn(move || {
                let _ = tx.send(super::api_client::test_endpoint(&http, &url, key.as_deref(), &model));
            });
        }
    }
    Test::Running(rx)
}

fn test_line(ui: &mut egui::Ui, test: &Option<&Test>) {
    match test {
        Some(Test::Running(_)) => {
            ui_kit::text(ui, tr("ui.ai_testing"), ui_kit::Text::Note("testing"));
        }
        Some(Test::Done(Ok(message))) => {
            ui_kit::text(ui, message, ui_kit::Text::Report("test_ok", false));
        }
        Some(Test::Done(Err(error))) => {
            ui_kit::text(ui, error, ui_kit::Text::Report("test_error", true));
        }
        None => {}
    }
}

/// The AI page of Settings.
pub fn page(app: &mut SolveApp, ui: &mut egui::Ui) {
    let r = ui_kit::checkbox(ui, tr("ui.ai_show_features"), &mut app.preferences.ai_features);
    crate::scenario::publish_handle("prefs:ai_features", r.rect.center());
    ui_kit::text(ui, tr("ui.ai_show_features_note"), ui_kit::Text::Note("features"));
    if cfg!(target_arch = "wasm32") {
        ui_kit::text(ui, tr("ui.agent_chat_desktop_only"), ui_kit::Text::Note("desktop"));
        return;
    }
    // Any agent's control requests, the chat's or an outside MCP client's (#1989).
    let r = ui_kit::checkbox(ui, tr("ui.agent_off_thread"), &mut app.preferences.agent_off_thread);
    crate::scenario::publish_handle("prefs:agent_off_thread", r.rect.center());
    ui_kit::text(ui, tr("ui.agent_off_thread_note"), ui_kit::Text::Note("off_thread"));
    if !app.preferences.ai_features {
        return;
    }
    let r = ui_kit::checkbox(ui, tr("ui.ai_toolbar_button"), &mut app.preferences.ai_toolbar_button);
    crate::scenario::publish_handle("prefs:ai_toolbar_button", r.rect.center());
    ui_kit::text(ui, tr("ui.ai_toolbar_button_note"), ui_kit::Text::Note("toolbar"));
    #[cfg(not(target_arch = "wasm32"))]
    {
        if app.agent_chat.detection.is_none() && app.agent_chat.detecting.is_none() {
            refresh(app, &ui.ctx().clone());
        }
        agents_section(app, ui);
    }
    ui_kit::section(ui, "permissions", tr("ui.ai_permissions"), true, |ui| {
        for mode in Mode::ALL {
            let r = ui_kit::list_picker(ui, mode.id(), mode.label(), app.preferences.ai_permissions == mode);
            crate::scenario::publish_handle(&format!("prefs:ai_mode:{}", mode.id()), r.rect.center());
            if r.clicked() {
                app.preferences.ai_permissions = mode;
            }
            ui_kit::text(
                ui,
                tr(match mode {
                    Mode::Ask => "ui.ai_mode_ask_note",
                    Mode::Edits => "ui.ai_mode_edits_note",
                    Mode::All => "ui.ai_mode_all_note",
                }),
                ui_kit::Text::Note(mode.id()),
            );
        }
        if app.preferences.ai_permissions == Mode::All {
            ui_kit::text(ui, tr("ui.ai_mode_all_warning"), ui_kit::Text::Report("permissions_warning", true));
        }
        ui_kit::text(ui, tr("ui.ai_modes_scope"), ui_kit::Text::Note("scope"));
        ui_kit::section(ui, "always_allowed", tr("ui.ai_always_allowed"), true, |ui| {
            let remembered: Vec<_> =
                app.preferences.agent_always_allow.iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| (k.clone(), v.clone())).collect();
            if remembered.is_empty() {
                ui_kit::text(ui, tr("ui.ai_always_allowed_none"), ui_kit::Text::Note("none"));
            }
            let names: std::collections::HashMap<_, _> = agents::entries(app).into_iter().map(|e| (e.id, e.name)).collect();
            for (agent, tools) in remembered {
                ui_kit::label_actions(ui, names.get(&agent).unwrap_or(&agent), |ui| {
                    if ui_kit::button(ui, tr("ui.ai_reset"), ui_kit::Button::Small).clicked() {
                        app.preferences.agent_always_allow.remove(&agent);
                    }
                });
                ui_kit::text(ui, &tools.join(", "), ui_kit::Text::Report(&agent, false));
            }
        });
    });
    ui_kit::section(ui, "panel", tr("ui.ai_panel"), true, |ui| {
        ui_kit::row(ui, tr("ui.agent_chat_side"), |ui| {
            let p = &mut app.preferences;
            let mut selected = if p.agent_chat_popped { 2 } else { usize::from(p.agent_chat_side != "left") };
            let options = [tr("ui.agent_chat_left"), tr("ui.agent_chat_right"), tr("ui.agent_chat_own_window")];
            if ui_kit::segmented(ui, &mut selected, &options, Default::default()) {
                p.agent_chat_popped = selected == 2;
                if selected < 2 {
                    p.agent_chat_side = if selected == 0 { "left" } else { "right" }.into();
                }
            }
        });
        ui_kit::row(ui, tr("ui.agent_chat_turn_limit"), |ui| {
            ui_kit::number_field(ui, &mut app.preferences.agent_chat_turn_limit, 1..=200, 1.0, "");
        });
        ui_kit::text(ui, tr("ui.agent_chat_privacy_note"), ui_kit::Text::Note("privacy"));
    });
}

/// The agents: found and added, with Test, Default, a permission override and Remove.
#[cfg(not(target_arch = "wasm32"))]
fn agents_section(app: &mut SolveApp, ui: &mut egui::Ui) {
    for test in app.agent_chat.settings.tests.values_mut() {
        test.poll();
    }
    let all = agents::entries(app);
    let d = &app.preferences.agent_default;
    let default = if all.iter().any(|e| &e.id == d) { Some(d.clone()) } else { all.first().map(|e| e.id.clone()) };
    let mut remove: Option<String> = None;
    let mut test: Option<agents::Entry> = None;
    ui_kit::section(ui, "agents", tr("ui.ai_agents"), true, |ui| {
        if app.agent_chat.detecting.is_some() {
            ui_kit::text(ui, tr("ui.agent_chat_detecting"), ui_kit::Text::Note("detecting"));
        } else if all.is_empty() {
            ui_kit::text(ui, tr("ui.agent_chat_none_found"), ui_kit::Text::Note("none"));
        }
        for e in &all {
            ui_kit::section(ui, &e.id, &e.name, true, |ui| {
                ui_kit::text(ui, &e.detail(), ui_kit::Text::Report("detail", false));
                super::panel::login_chip(ui, e.login);
                if !e.found {
                    ui_kit::text(ui, tr("ui.ai_added"), ui_kit::Text::Note("added"));
                }
                ui.horizontal_wrapped(|ui| {
                    if ui_kit::list_picker(ui, &e.id, tr("ui.ai_default"), default.as_deref() == Some(e.id.as_str())).clicked() {
                        app.preferences.agent_default = e.id.clone();
                    }
                });
                ui_kit::row(ui, tr("ui.ai_permissions"), |ui| {
                    let cur = app.preferences.agent_permissions.get(&e.id).copied();
                    let shown = cur.map_or_else(|| tr("ui.ai_mode_global").to_string(), |m| m.label().to_string());
                    let mut pick: Option<Option<Mode>> = None;
                    ui_kit::dropdown_menu(ui, ("sc_ai_mode", &e.id), &shown, |ui| {
                        if ui_kit::list_picker(ui, "global", tr("ui.ai_mode_global"), cur.is_none()).clicked() {
                            pick = Some(None);
                        }
                        for m in Mode::ALL {
                            if ui_kit::list_picker(ui, m.id(), m.label(), cur == Some(m)).clicked() {
                                pick = Some(Some(m));
                            }
                        }
                    });
                    match pick {
                        Some(Some(m)) => {
                            app.preferences.agent_permissions.insert(e.id.clone(), m);
                        }
                        Some(None) => {
                            app.preferences.agent_permissions.remove(&e.id);
                        }
                        None => {}
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    let b = ui_kit::button(ui, tr("ui.ai_test"), ui_kit::Button::Normal);
                    crate::scenario::publish_handle(&format!("prefs:ai_test:{}", e.id), b.rect.center());
                    if b.on_hover_text(tr(if e.kind == Kind::Acp { "ui.ai_test_acp_tip" } else { "ui.ai_test_api_tip" })).clicked() {
                        test = Some(e.clone());
                    }
                    if !e.found && ui_kit::button(ui, tr("ui.ai_remove"), ui_kit::Button::Normal).clicked() {
                        remove = Some(e.id.clone());
                    }
                });
                test_line(ui, &app.agent_chat.settings.tests.get(&e.id));
            });
        }
        ui.horizontal_wrapped(|ui| {
            let b = ui_kit::button(ui, tr("ui.ai_add_agent"), ui_kit::Button::Normal);
            crate::scenario::publish_handle("prefs:ai_add", b.rect.center());
            if b.clicked() {
                open_add(app);
            }
            if ui_kit::button(ui, tr("ui.agent_chat_refresh"), ui_kit::Button::Enabled(app.agent_chat.detecting.is_none())).clicked() {
                refresh(app, &ui.ctx().clone());
            }
        });
    });
    if let Some(e) = test {
        let key = if e.key { app.services.secrets.as_ref().and_then(|s| s.get(&e.id).ok().flatten()) } else { None };
        let started = start_test(app, &e, key);
        app.agent_chat.settings.tests.insert(e.id.clone(), started);
    }
    if let Some(id) = remove {
        app.preferences.agents.retain(|a| a.id != id);
        app.preferences.agent_permissions.remove(&id);
        app.preferences.agent_always_allow.remove(&id);
        if let Some(s) = &app.services.secrets {
            let _ = s.delete(&id);
        }
        if app.agent_chat.agent_id.as_deref() == Some(id.as_str()) {
            app.agent_chat.agent_id = None;
        }
    }
    if app.agent_chat.settings.tests.values().any(|t| matches!(t, Test::Running(_))) {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
    }
}

/// The Add agent dialog (drawn every frame while it is open).
pub fn dialog(app: &mut SolveApp, ctx: &egui::Context) {
    if app.agent_chat.settings.add.is_none() {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    let found: Vec<agents::Entry> = agents::entries(app).into_iter().filter(|e| e.found).collect();
    #[cfg(target_arch = "wasm32")]
    let found: Vec<agents::Entry> = Vec::new();
    let has_keychain = app.services.secrets.is_some();
    let mut open = true;
    let mut add = false;
    let mut test = false;
    let (closed, action) = ui_kit::Window::new(egui::Id::new("sc_ai_add"), "settings", tr("ui.ai_add_agent_title"))
        .cards(2.0)
        .fields()
        .actions(tr("ui.ai_add"), "Cancel", None)
        .show(ctx, true, |ui| {
            let Some(s) = app.agent_chat.settings.add.as_mut() else { return };
            if let Some(test) = s.test.as_mut() {
                test.poll();
            }
            let kinds = [(AddKind::Installed, "ui.ai_add_installed"), (AddKind::Custom, "ui.ai_add_custom"), (AddKind::Api, "ui.ai_add_api")];
            ui_kit::row(ui, tr("ui.ai_preset"), |ui| {
                let mut selected = kinds.iter().position(|(k, _)| *k == s.kind).unwrap_or(0);
                let labels = kinds.map(|(_, key)| tr(key));
                if ui_kit::segmented(ui, &mut selected, &labels, ui_kit::Segments { compact: true, ..Default::default() })
                    && let Some((kind, _)) = kinds.get(selected)
                {
                    s.kind = *kind;
                    s.error = None;
                    s.test = None;
                }
                for (_, key) in kinds {
                    if let Some(at) = crate::scenario::handle_at(&format!("dialog:{}:{}", tr("ui.ai_preset"), tr(key))) {
                        crate::scenario::publish_handle(&format!("ai_add:{key}"), at);
                    }
                }
            });
            match s.kind {
                AddKind::Installed => {
                    ui_kit::row(ui, tr("ui.agent_chat_pick"), |ui| {
                        if found.is_empty() {
                            ui_kit::text(ui, tr("ui.agent_chat_none_found_short"), ui_kit::Text::Inline);
                        } else {
                            let sel = found.get(s.installed).map(|e| e.name.clone()).unwrap_or_default();
                            ui_kit::dropdown_menu(ui, "sc_ai_add_installed", &sel, |ui| {
                                for (i, e) in found.iter().enumerate() {
                                    if ui_kit::list_picker(ui, &e.id, &format!("{}  {}", e.name, e.detail()), s.installed == i).clicked() {
                                        s.installed = i;
                                        s.command = e.argv.iter().map(|a| super::panel::quote(a)).collect::<Vec<_>>().join(" ");
                                        s.name = format!("{} ({})", e.name, tr("ui.ai_copy_suffix"));
                                    }
                                }
                            });
                        }
                    });
                    if s.command.is_empty()
                        && let Some(e) = found.get(s.installed)
                    {
                        s.command = e.argv.iter().map(|a| super::panel::quote(a)).collect::<Vec<_>>().join(" ");
                        s.name = format!("{} ({})", e.name, tr("ui.ai_copy_suffix"));
                    }
                    ui_kit::row(ui, tr("ui.ai_name"), |ui| {
                        ui_kit::text_field(ui, &mut s.name, "");
                    });
                    ui_kit::row(ui, tr("ui.agent_chat_command"), |ui| {
                        ui_kit::text_field(ui, &mut s.command, "");
                    });
                    ui_kit::text(ui, tr("ui.ai_add_installed_note"), ui_kit::Text::Note("installed"));
                }
                AddKind::Custom => {
                    ui_kit::row(ui, tr("ui.ai_name"), |ui| {
                        ui_kit::text_field(ui, &mut s.name, tr("ui.ai_name_hint"));
                    });
                    ui_kit::row(ui, tr("ui.agent_chat_command"), |ui| {
                        ui_kit::text_field(ui, &mut s.command, tr("ui.ai_command_hint"));
                    });
                    ui_kit::text(ui, tr("ui.agent_chat_custom_hint"), ui_kit::Text::Note("custom"));
                }
                AddKind::Api => {
                    let presets: Vec<_> = agents::API_PRESETS.iter().map(|p| p.0).chain(std::iter::once(tr("ui.ai_custom_url"))).collect();
                    ui_kit::row(ui, tr("ui.ai_preset"), |ui| {
                        ui_kit::dropdown_menu(ui, "sc_ai_add_preset", presets.get(s.preset).copied().unwrap_or_default(), |ui| {
                            for (index, preset) in presets.iter().enumerate() {
                                if ui_kit::list_picker(ui, preset, preset, s.preset == index).clicked() {
                                    s.preset = index;
                                    if let Some((name, url, _)) = agents::API_PRESETS.get(index) {
                                        s.base_url = url.to_string();
                                        s.name = name.to_string();
                                    } else {
                                        s.base_url.clear();
                                    }
                                }
                            }
                        });
                    });
                    ui_kit::row(ui, tr("ui.ai_name"), |ui| {
                        ui_kit::text_field(ui, &mut s.name, "");
                    });
                    ui_kit::row(ui, tr("ui.ai_base_url"), |ui| {
                        ui_kit::text_field(ui, &mut s.base_url, tr("ui.ai_url_hint"));
                    });
                    ui_kit::row(ui, tr("ui.ai_model"), |ui| {
                        ui_kit::text_field(ui, &mut s.model, tr("ui.ai_model_hint"));
                    });
                    ui_kit::row(ui, tr("ui.ai_key"), |ui| {
                        ui.add_enabled_ui(has_keychain, |ui| {
                            let r = ui_kit::password_field(ui, egui::Id::new("sc_ai_add_key"), &mut s.key, tr("ui.ai_key_hint"));
                            crate::scenario::publish_target("ai_add:key", r.rect);
                        });
                    });
                    ui_kit::text(ui, tr(if has_keychain { "ui.ai_key_note" } else { "ui.ai_no_keychain" }), ui_kit::Text::Note("keychain"));
                    if ui_kit::button(ui, tr("ui.ai_test"), ui_kit::Button::Normal).clicked() {
                        test = true;
                    }
                    test_line(ui, &s.test.as_ref());
                }
            }
            if let Some(e) = &s.error {
                ui_kit::text(ui, e, ui_kit::Text::Report("add_error", true));
            }
        });
    if let Some(rect) = crate::scenario::rect_of("dialog:ok") {
        crate::scenario::publish_handle("ai_add:add", rect.center());
    }
    add |= action == ui_kit::Action::Ok;
    open &= !closed && action != ui_kit::Action::Cancel;
    if test {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(s) = app.agent_chat.settings.add.as_ref() {
            let url = agents::normalize_url(&s.base_url);
            let e = agents::Entry {
                id: String::new(),
                name: s.name.clone(),
                kind: Kind::Api,
                argv: Vec::new(),
                version: None,
                login: "",
                found: false,
                base_url: url.clone().unwrap_or_default(),
                model: s.model.trim().to_string(),
                key: false,
            };
            let key = Some(s.key.trim().to_string()).filter(|k| !k.is_empty());
            let result = match url {
                Ok(_) => start_test(app, &e, key),
                Err(err) => Test::Done(Err(err)),
            };
            if let Some(s) = app.agent_chat.settings.add.as_mut() {
                s.test = Some(result);
            }
        }
    }
    if add {
        match add_agent(app) {
            Ok(()) => open = false,
            Err(e) => {
                if let Some(s) = app.agent_chat.settings.add.as_mut() {
                    s.error = Some(e);
                }
            }
        }
    }
    if !open {
        app.agent_chat.settings.add = None;
    }
    if app.agent_chat.settings.add.as_ref().is_some_and(|s| matches!(s.test, Some(Test::Running(_)))) {
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
}

/// Add what the dialog holds; its key goes to the keychain.
fn add_agent(app: &mut SolveApp) -> Result<(), String> {
    let Some(s) = app.agent_chat.settings.add.as_ref() else { return Ok(()) };
    let name = s.name.trim().chars().take(80).collect::<String>();
    let mut c = AgentConfig { name, ..Default::default() };
    let mut key: Option<String> = None;
    match s.kind {
        AddKind::Installed | AddKind::Custom => {
            let cmd = s.command.trim();
            if acp::split_command(cmd).is_empty() {
                return Err(tr("ui.ai_need_command").to_string());
            }
            c.kind = Kind::Acp;
            c.command = cmd.chars().take(4096).collect();
            c.id = agents::new_id(&app.preferences, "custom");
        }
        AddKind::Api => {
            c.kind = Kind::Api;
            c.base_url = agents::normalize_url(&s.base_url)?;
            c.model = s.model.trim().to_string();
            if c.model.is_empty() {
                return Err(tr("ui.ai_need_model").to_string());
            }
            key = Some(s.key.trim().to_string()).filter(|k| !k.is_empty());
            if key.is_some() && !agents::key_safe_for(&c.base_url) {
                return Err(tr("ui.ai_key_needs_https").to_string());
            }
            c.id = agents::new_id(&app.preferences, "api");
        }
    }
    if c.name.is_empty() {
        c.name = c.id.clone();
    }
    if let Some(k) = key {
        let Some(store) = &app.services.secrets else { return Err(tr("ui.ai_no_keychain").to_string()) };
        store.set(&c.id, &k).map_err(|e| trf("ui.ai_keychain_error", &[("error", &e)]))?;
        c.key = true;
    }
    app.preferences.agents.push(c);
    Ok(())
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod card_tests {
    use super::*;
    use crate::scenario::{self, Harness};
    use serde_json::json;

    #[test]
    fn add_agent_card_retains_kind_handles_draft_and_validation() {
        let mut h = Harness::new();
        h.app.home.open = false;
        h.app.preferences.ai_features = true;
        h.app.agent_chat.detection = Some(Default::default());
        open_add(&mut h.app);
        h.frames(4);
        h.step(&json!({"click":{"handle":"ai_add:ui.ai_add_custom"}})).unwrap();
        assert_eq!(h.app.agent_chat.settings.add.as_ref().unwrap().kind, AddKind::Custom);
        h.step(&json!({"click":{"handle":"ai_add:add"}})).unwrap();
        assert!(h.app.agent_chat.settings.add.as_ref().unwrap().error.is_some());
        let add = h.app.agent_chat.settings.add.as_mut().unwrap();
        add.name = "Local fixture".into();
        add.command = "solidcraft-fixture-agent --acp".into();
        h.frames(3);
        h.step(&json!({"click":{"handle":"ai_add:add"}})).unwrap();
        assert!(h.app.agent_chat.settings.add.is_none());
        assert!(h.app.preferences.agents.iter().any(|e| e.name == "Local fixture"));
        open_add(&mut h.app);
        h.frames(3);
        h.step(&json!({"click":{"handle":"ai_add:ui.ai_add_api"}})).unwrap();
        assert!(scenario::handle_at("ai_add:key").is_some());
        assert!(h.app.services.secrets.is_none(), "this fixture has no keychain");
        h.step(&json!({"key":"Escape"})).unwrap();
        assert!(h.app.agent_chat.settings.add.is_none());
    }
}
