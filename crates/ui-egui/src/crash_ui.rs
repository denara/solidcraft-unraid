//! The Crash Reports window (Help > Crash Reports…): the reports `solidcraft_engine::crash`
//! keeps on this computer, each with a reviewed issue draft. It opens by itself after a crash.

use egui::{RichText, vec2};
use solidcraft_engine::crash::{self, Summary};

use crate::SolveApp;
use crate::theme::Tokens;

#[derive(Default)]
pub struct CrashUi {
    pub open: bool,
    /// The crash folder (default: the engine's, `crash::default_dir`).
    pub folder: Option<std::path::PathBuf>,
    reports: Vec<Summary>,
    dir: Option<String>,
    note: String,
}

impl CrashUi {
    /// Read the reports again (when the window opens or changes; never every frame).
    pub fn refresh(&mut self) {
        let dir = self.dir_path();
        self.reports = dir.as_deref().map(crash::list).unwrap_or_default();
        self.dir = dir.map(|d| d.to_string_lossy().into_owned());
    }
}

impl CrashUi {
    fn dir_path(&self) -> Option<std::path::PathBuf> {
        self.folder.clone().or_else(crash::default_dir)
    }
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "command_panic" => "Internal error in a command",
        "kernel_panic_recovered" => "Kernel error (recovered)",
        "unclean_exit" => "Closed unexpectedly",
        _ => "Internal error",
    }
}

/// The host found these new reports at startup: tell the user about real crashes (recovered
/// kernel errors stay quiet; they are listed in the window).
pub fn announce(app: &mut SolveApp, fresh: &[Summary]) {
    let crashes = fresh.iter().filter(|r| r.kind != "kernel_panic_recovered").count();
    if crashes == 0 {
        return;
    }
    app.crashes.open = true;
    app.crashes.refresh();
    app.set_status("SolidCraft closed unexpectedly or hit an internal error last time. A crash report was saved (Help > Crash Reports…).", true);
}

pub fn show(app: &mut SolveApp, ctx: &egui::Context) {
    if !app.crashes.open {
        return;
    }
    let t = Tokens::get();
    let mut open = true;
    let mut report: Option<String> = None;
    let mut delete: Option<Option<String>> = None;
    crate::frame::window(ctx, crate::i18n::tr_label("Crash Reports"), crate::frame::Width::Wide)
        .id(egui::Id::new("sc_crash_reports"))
        .open(&mut open)
        .default_size(vec2(620.0, 420.0))
        .show(ctx, |ui| {
            ui.label(
                RichText::new(
                    "Crash reports stay on this computer and are never sent anywhere. Report… opens a draft you review before anything is shared.",
                )
                .color(t.text_dim),
            );
            if let Some(d) = app.crashes.dir.clone() {
                // A long path is cut short (hover shows it all): it must not widen the window
                // past its title bar.
                ui.horizontal(|ui| {
                    if ui.small_button("Copy folder path").clicked() {
                        ctx.copy_text(d.clone());
                    }
                    ui.add(egui::Label::new(RichText::new(&d).monospace().size(11.0)).truncate()).on_hover_text(&d);
                });
            }
            ui.separator();
            if app.crashes.reports.is_empty() {
                ui.label(crate::i18n::tr_label("No crash reports."));
            }
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for r in &app.crashes.reports {
                    ui.horizontal(|ui| {
                        let times = if r.count > 1 { format!("  ×{}", r.count) } else { String::new() };
                        ui.label(
                            RichText::new(format!("{}  {}{times}", r.time.replace('T', " ").trim_end_matches('Z'), kind_label(&r.kind))).strong(),
                        );
                        if let Some(c) = &r.command {
                            ui.label(RichText::new(c).monospace().size(11.0).color(t.text_dim));
                        }
                    });
                    ui.label(RichText::new(&r.message).size(12.0));
                    ui.horizontal(|ui| {
                        if ui.small_button("Report…").clicked() {
                            report = Some(r.id.clone());
                        }
                        if ui.small_button("Copy path").clicked() {
                            ctx.copy_text(r.path.clone());
                        }
                        if ui.small_button("Delete").clicked() {
                            delete = Some(Some(r.id.clone()));
                        }
                        if r.replayable {
                            ui.label(RichText::new("replayable").size(11.0).color(t.text_dim));
                        }
                    });
                    ui.add_space(6.0);
                }
            });
            if !app.crashes.note.is_empty() {
                ui.label(RichText::new(&app.crashes.note).color(t.text_dim));
            }
            ui.separator();
            if !app.crashes.reports.is_empty() && ui.button(crate::i18n::tr_label("Delete all")).clicked() {
                delete = Some(None);
            }
        });
    app.crashes.open = open;
    if let Some(id) = report
        && let Some(dir) = app.crashes.dir_path()
    {
        match crash::read(&dir, &id) {
            Ok(v) => crate::issue_report::begin_crash(app, ctx, &v),
            Err(e) => app.crashes.note = e,
        }
    }
    if let Some(which) = delete
        && let Some(dir) = app.crashes.dir_path()
    {
        let n = crash::clear(&dir, which.as_deref());
        app.crashes.note = format!("Deleted {n} report(s).");
        app.crashes.refresh();
    }
}

/// Help > Crash Reports…
pub fn open(app: &mut SolveApp) {
    app.crashes.open = true;
    app.crashes.note.clear();
    app.crashes.refresh();
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    fn folder(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("solidcraft-crash-ui-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let id = "20261012-162341-bb60791ed6692f1d";
        std::fs::create_dir_all(d.join(id)).unwrap();
        let report = json!({
            "format": "solidcraft-crash/1", "id": id, "kind": "unclean_exit", "count": 2,
            "first_seen": "2026-10-12T16:23:41Z", "message": "SolidCraft ended without a clean exit while running `solid.fillet`",
            "location": "unclean exit in solid.fillet", "thread": "(unknown)", "backtrace": [],
            "command": {"command": "solid.fillet", "params": {"radius": 2}},
            "recent_commands": [{"command": "solid.box", "ok": true}],
            "app": {"version": "0.3.2", "commit": "abc1234", "built": "2026-10-12"},
            "system": {"os": "linux", "arch": "x86_64", "environment": "Renderer: test"},
        });
        std::fs::write(d.join(id).join("report.json"), report.to_string()).unwrap();
        std::fs::write(d.join(id).join("replay.json"), r#"{"commands":[]}"#).unwrap();
        d
    }

    #[test]
    fn a_crash_report_opens_a_reviewed_issue_draft_and_can_be_deleted() {
        let d = folder("draft");
        let mut h = crate::scenario::Harness::new();
        h.app.crashes.folder = Some(d.clone());
        let fresh = solidcraft_engine::crash::list(&d);
        assert_eq!(fresh.len(), 1);
        super::announce(&mut h.app, &fresh);
        assert!(h.app.crashes.open, "a real crash opens the window");
        assert!(h.app.status.as_ref().is_some_and(|s| s.2), "and says so");
        h.frames(4);
        h.step(&json!({"widget": "Report…"})).unwrap();
        h.frames(2);
        let draft = h.app.issue_report.draft.as_ref().expect("Report… opens the draft");
        assert!(draft.title.starts_with("Crash: SolidCraft ended without a clean exit"), "{}", draft.title);
        for want in [
            "while running `solid.fillet`",
            "\"radius\": 2",
            "solid.box",
            "solidcraft-cli crashes replay 20261012-162341-bb60791ed6692f1d",
            "Renderer: test",
            "Happened 2 time(s)",
        ] {
            assert!(draft.body.contains(want), "missing {want:?} in {}", draft.body);
        }
        assert!(!draft.body.contains(&d.to_string_lossy().into_owned()), "the draft names no local paths");
        h.app.issue_report.draft = None;
        h.frames(2);
        h.step(&json!({"widget": "Delete all"})).unwrap();
        h.frames(2);
        assert!(solidcraft_engine::crash::list(&d).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_long_folder_path_does_not_widen_the_window_past_its_title_bar() {
        let d = folder(&format!("{}-long", "very-long-folder-name-".repeat(6)));
        let mut h = crate::scenario::Harness::new();
        h.app.crashes.folder = Some(d.clone());
        super::open(&mut h.app);
        h.frames(6);
        let window = h.ctx.memory(|m| m.area_rect(egui::Id::new("sc_crash_reports"))).unwrap();
        // The frame's margins come on top of the content's maximum width.
        assert!(window.width() <= crate::frame::WINDOW_WIDE_MAX_W + 40.0, "{} wide", window.width());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn recovered_kernel_errors_do_not_interrupt() {
        let mut h = crate::scenario::Harness::new();
        let quiet = solidcraft_engine::crash::Summary {
            id: "x".into(),
            kind: "kernel_panic_recovered".into(),
            time: String::new(),
            message: String::new(),
            command: None,
            count: 1,
            version: String::new(),
            path: String::new(),
            replayable: false,
        };
        super::announce(&mut h.app, &[quiet]);
        assert!(!h.app.crashes.open);
    }
}
