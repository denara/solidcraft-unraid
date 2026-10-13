//! SolidCraft desktop app.
//!
//! Usage: `solidcraft [--control <port>] [--sample] [--crash-log] [design.solidcraft | part.step]`
//!
//! A `.step`/`.stp` file opens as a new design holding the file's bodies (an Import feature); a
//! `.3mf`/`.stl` file as a new design holding mesh bodies.
//!
//! `--control <port>` (or `SOLIDCRAFT_CONTROL_PORT`) starts a localhost JSON-lines control
//! server: `{"id":1,"method":"engine.execute","params":{"command":"solid.box","params":{…}}}`.
//! See `solidcraft_ui_egui::control` and docs/control-protocol.md.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

#[cfg(not(target_os = "macos"))]
#[path = "../../app_icon.rs"]
mod app_icon;
mod control_server;
#[cfg(any(target_os = "windows", target_os = "linux"))]
mod space_mouse;

use solidcraft_engine::{Session, io};
use solidcraft_ui_egui::{Services, SolveApp};

struct App {
    solve: SolveApp,
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    space_mouse: space_mouse::SpaceMouse,
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // The design on screen is the one whose commands a crash report replays.
        self.solve.session.crash_journal = solidcraft_engine::crash::enabled();
        // The OS closing the window (Alt+F4, Cmd+Q, the window manager): ask about unsaved
        // designs first; the window closes once they are answered (`quit_requested`).
        if ctx.input(|i| i.viewport().close_requested()) && !self.solve.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            solidcraft_ui_egui::documents::request_quit(&mut self.solve);
        }
        self.solve.logic(ctx);
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        self.space_mouse.tick(&mut self.solve, ctx);
        if self.solve.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.solve.raw_input_hook(raw);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.solve.ui(ui);
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(solidcraft_ui_egui::prefs::STORAGE_KEY, self.solve.prefs());
    }
    fn on_exit(&mut self) {
        solidcraft_ui_egui::recovery_ui::close(&mut self.solve);
        solidcraft_engine::crash::end_session();
    }
}

fn services() -> Services {
    Services {
        pick_open: Some(Box::new(|| {
            rfd::FileDialog::new()
                .add_filter(
                    "Designs, STEP, IGES and meshes",
                    &[
                        io::DESIGN_EXT,
                        io::LEGACY_DESIGN_EXT,
                        "json",
                        "step",
                        "stp",
                        "STEP",
                        "STP",
                        "igs",
                        "iges",
                        "IGS",
                        "IGES",
                        "3mf",
                        "3MF",
                        "stl",
                        "STL",
                    ],
                )
                .add_filter("SolidCraft design (container or JSON)", &[io::DESIGN_EXT, io::LEGACY_DESIGN_EXT, "json"])
                .add_filter("STEP", &["step", "stp", "STEP", "STP"])
                .add_filter("IGES", &["igs", "iges", "IGS", "IGES"])
                .add_filter("Mesh (3MF, STL)", &["3mf", "3MF", "stl", "STL"])
                .add_filter("Sketch drawings (DXF, SVG)", &["dxf", "DXF", "svg", "SVG"])
                .add_filter("All files", &["*"])
                .pick_file()
                .map(|p| p.to_string_lossy().to_string())
        })),
        pick_save: Some(Box::new(|name: &str, exts: &[&str]| {
            let dialog = rfd::FileDialog::new().set_file_name(name);
            let dialog = if exts == [io::DESIGN_EXT, "json"] {
                if name.ends_with(".json") {
                    dialog.add_filter("Plain JSON design", &["json"]).add_filter("SolidCraft container", &[io::DESIGN_EXT])
                } else {
                    dialog.add_filter("SolidCraft container", &[io::DESIGN_EXT]).add_filter("Plain JSON design", &["json"])
                }
            } else {
                dialog.add_filter("File", exts)
            };
            dialog.save_file().map(|p| p.to_string_lossy().to_string())
        })),
        graphics_path: graphics_path().map(|p| p.to_string_lossy().to_string()),
        show_in_folder: Some(Box::new(|path| {
            let folder = std::path::Path::new(path).parent().unwrap_or_else(|| std::path::Path::new("."));
            let mut command = std::process::Command::new(if cfg!(target_os = "windows") {
                "explorer"
            } else if cfg!(target_os = "macos") {
                "open"
            } else {
                "xdg-open"
            });
            command.arg(folder);
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }
            let _ = command.spawn();
        })),
    }
}

/// The per-user settings folder: eframe's storage (`app.ron`) and the anti-aliasing preference.
fn settings_dir() -> Option<std::path::PathBuf> {
    eframe::storage_dir("SolidCraft")
}

/// The anti-aliasing preference, kept next to eframe's storage (it is needed before the
/// window opens).
fn graphics_path() -> Option<std::path::PathBuf> {
    settings_dir().map(|d| d.join("graphics.json"))
}

/// Compatibility shim (docs/compatibility.md): on the first launch after the rename, copy the
/// settings of earlier releases into the new folder. They kept `graphics.json` (and, on Linux
/// and macOS, the recovery folder) under the old product name, and eframe's `app.ron` under the
/// window's app id. The old folders are never changed.
fn adopt_legacy_settings() {
    let (Some(current), Some(legacy)) = (settings_dir(), eframe::storage_dir("SolveCraft")) else { return };
    let legacy_eframe = eframe::storage_dir(APP_ID);
    if let Err(e) = adopt_settings(&current, &legacy, legacy_eframe.as_deref()) {
        log::warn!("settings: could not copy the previous release's settings into {}: {e}", current.display());
    }
}

fn adopt_settings(current: &std::path::Path, legacy: &std::path::Path, legacy_eframe: Option<&std::path::Path>) -> std::io::Result<()> {
    let fresh = !current.exists();
    solidcraft_engine::io::adopt_legacy_dir(current, legacy)?;
    if let Some(old) = legacy_eframe.map(|d| d.join("app.ron")).filter(|p| fresh && p.is_file()) {
        std::fs::create_dir_all(current)?;
        let new = current.join("app.ron");
        if !new.exists() {
            std::fs::copy(old, new)?;
        }
    }
    Ok(())
}

/// Anti-aliasing samples from Preferences (4 when unset).
fn msaa() -> u16 {
    graphics_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("msaa").and_then(serde_json::Value::as_u64))
        .and_then(|m| u16::try_from(m).ok())
        .filter(|m| [1, 2, 4, 8].contains(m))
        .unwrap_or(MSAA)
}

/// Windows and Linux: no OS title bar; the application bar is the title bar
/// (`solidcraft_ui_egui::titlebar`). macOS keeps its traffic lights over the integrated bar.
const CUSTOM_TITLEBAR: bool = !cfg!(target_os = "macos");

const DEPTH_BITS: u8 = 24;
/// The section cap marks where it shows in the stencil.
const STENCIL_BITS: u8 = 8;
const MSAA: u16 = 4;

fn app_viewport() -> Result<egui::ViewportBuilder, eframe::Error> {
    // Let macOS use the bundle's layered icon; eframe's default bitmap would replace it.
    #[cfg(target_os = "macos")]
    let icon = egui::IconData::default();
    #[cfg(not(target_os = "macos"))]
    let icon =
        egui::IconData { rgba: app_icon::rgba(64).map_err(|e| eframe::Error::AppCreation(std::io::Error::other(e).into()))?, width: 64, height: 64 };
    Ok(egui::ViewportBuilder::default()
        .with_title("SolidCraft")
        .with_icon(icon)
        .with_inner_size([1600.0, 1000.0])
        .with_min_inner_size([960.0, 600.0])
        .with_drag_and_drop(true)
        .with_decorations(!CUSTOM_TITLEBAR)
        .with_fullsize_content_view(true)
        .with_titlebar_shown(false)
        .with_title_shown(false)
        .with_app_id(APP_ID))
}

/// The window's app id: it names the desktop entry, so it stays the packages' reverse-DNS id,
/// which is kept from before the rename (compatibility shim, docs/compatibility.md).
const APP_ID: &str = "ai.storyteller.solvecraft";

fn main() -> eframe::Result {
    adopt_legacy_settings();
    let samples = msaa();
    let mut control_port: Option<u16> = solidcraft_engine::geom::env::var("CONTROL_PORT").and_then(|p| p.parse().ok());
    let mut files = Vec::new();
    let mut sample = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--control" => control_port = args.next().and_then(|p| p.parse().ok()),
            "--sample" => sample = true,
            "--version" | "-V" => {
                println!("solidcraft {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--crash-log" => {
                match solidcraft_engine::crash::default_dir() {
                    Some(dir) => {
                        println!("crash reports: {}", dir.display());
                        for r in solidcraft_engine::crash::list(&dir).iter().take(5) {
                            println!("  {}  {}  {}", r.id, r.kind, r.message);
                        }
                        println!("more: solidcraft-cli crashes");
                    }
                    None => println!("no crash folder on this system"),
                }
                return Ok(());
            }
            "--help" | "-h" => {
                println!("usage: solidcraft [--control PORT] [--sample] [--crash-log] [design.solidcraft | part.step]");
                return Ok(());
            }
            _ => files.push(a),
        }
    }
    // Crashes leave a report in the crash folder (solidcraft_engine::crash, docs/crash-reports.md).
    let crashed = solidcraft_engine::crash::default_dir()
        .map(|dir| {
            let build = solidcraft_engine::crash::Build {
                version: solidcraft_ui_egui::help::VERSION,
                commit: solidcraft_ui_egui::help::COMMIT,
                built: solidcraft_ui_egui::help::BUILD_DATE,
            };
            solidcraft_engine::crash::install(dir, build)
        })
        .unwrap_or_default();
    let options = eframe::NativeOptions {
        viewport: app_viewport()?,
        depth_buffer: DEPTH_BITS,
        stencil_buffer: STENCIL_BITS,
        multisampling: samples,
        // Kept with the other settings, not in a folder named after the app id.
        persistence_path: settings_dir().map(|d| d.join("app.ron")),
        ..Default::default()
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut options = options;
    #[cfg(all(unix, not(target_os = "macos")))]
    if prefer_x11(
        std::env::var_os("DISPLAY").is_some(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref(),
        solidcraft_engine::geom::env::var("WAYLAND").as_deref(),
    ) {
        options.event_loop_builder = Some(Box::new(|b| {
            use winit::platform::x11::EventLoopBuilderExtX11;
            b.with_x11();
        }));
    }
    eframe::run_native(
        "SolidCraft",
        options,
        Box::new(move |cc| {
            solidcraft_ui_egui::fonts::setup_fonts(&cc.egui_ctx);
            let mut app = SolveApp::new(Session::default(), services());
            app.custom_titlebar = CUSTOM_TITLEBAR;
            app.integrated_titlebar = cfg!(target_os = "macos");
            if let Some(p) = cc.storage.and_then(|s| solidcraft_ui_egui::prefs::stored(|k| s.get_string(k))) {
                app.load_prefs(&p);
            }
            if let Some(rs) = &cc.wgpu_render_state {
                eprintln!("SolidCraft graphics: {:?}, MSAA {}x (requested {}x)", rs.adapter.get_info().backend, rs.msaa_samples, samples);
                app.set_wgpu(rs, DEPTH_BITS, STENCIL_BITS, rs.msaa_samples);
                // A saved anti-aliasing the device can't do: back to the default, with a notice.
                solidcraft_ui_egui::prefs::check_msaa(&mut app, u32::from(samples));
                let info = rs.adapter.get_info();
                solidcraft_engine::crash::set_environment(&format!(
                    "Renderer: {:?} {} ({}), MSAA {}x",
                    info.backend, info.name, info.driver, rs.msaa_samples
                ));
            }
            if let Some(port) = control_port {
                let rx = control_server::start(port, cc.egui_ctx.clone());
                app = app.with_control(rx);
            }
            if sample {
                if let Err(e) = app.session.run_script(&solidcraft_engine::sample::script()) {
                    app.set_status(format!("sample: {e}"), true);
                }
                app.session.doc_mut().name = "Sample Plate".into();
                app.session.mark_saved();
                app.session.undo.clear();
            }
            for f in &files {
                app.open_path_async(f);
            }
            // No design given: the start page (not when driven over the control channel, whose
            // clients expect the design view; `ui.home` shows it there).
            if files.is_empty() && !sample && control_port.is_none() {
                let rev = app.session.revision;
                app.home.show(rev);
            }
            // Autosave, and offer what crashed sessions left behind.
            if let Some(dir) = solidcraft_engine::recovery::default_dir() {
                let every = if app.autosave_minutes > 0.0 { app.autosave_minutes * 60.0 } else { f64::MAX / 4.0 };
                solidcraft_ui_egui::recovery_ui::start(&mut app, &dir, std::time::Duration::from_secs_f64(every.min(1e9)));
            }
            // Tell the user about crashes since the last start (after the recovery offer is set up).
            solidcraft_ui_egui::crash_ui::announce(&mut app, &crashed);
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            let space_mouse = space_mouse::SpaceMouse::start(&cc.egui_ctx);
            Ok(Box::new(App {
                solve: app,
                #[cfg(any(target_os = "windows", target_os = "linux"))]
                space_mouse,
            }))
        }),
    )
}

/// Whether to run through XWayland rather than native Wayland. winit has no file drag-and-drop
/// on Wayland (only on X11), so dropping files from the file manager showed a "no" cursor: use
/// XWayland when it's there. Except on niri, whose XWayland (xwayland-satellite) breaks window
/// management (maximize/restore), so it stays on native Wayland and gives up file drops.
/// `SOLIDCRAFT_WAYLAND=1` keeps native Wayland, `SOLIDCRAFT_WAYLAND=0` forces XWayland.
#[cfg(all(unix, not(target_os = "macos")))]
fn prefer_x11(display: bool, wayland_display: bool, desktop: Option<&str>, wayland_override: Option<&str>) -> bool {
    match wayland_override.map(str::trim) {
        Some("0") => return display,
        Some(_) => return false,
        None => {}
    }
    let niri = desktop.is_some_and(|d| d.split(':').any(|name| name.trim().eq_ignore_ascii_case("niri")));
    display && !(niri && wayland_display)
}

#[cfg(all(test, unix, not(target_os = "macos")))]
mod backend_tests {
    use super::prefer_x11;

    #[test]
    fn xwayland_by_default_native_wayland_on_niri_and_the_override_wins() {
        // XWayland whenever there's an X display, for file drag-and-drop.
        assert!(prefer_x11(true, true, Some("GNOME"), None));
        assert!(prefer_x11(true, false, None, None));
        assert!(!prefer_x11(false, true, Some("KDE"), None));
        // niri (alone or in a desktop list) under Wayland stays native.
        assert!(!prefer_x11(true, true, Some("niri"), None));
        assert!(!prefer_x11(true, true, Some("niri:GNOME"), None));
        // niri without a Wayland socket (nested X session) still uses X11.
        assert!(prefer_x11(true, false, Some("niri"), None));
        // SOLIDCRAFT_WAYLAND: anything but 0 keeps Wayland, 0 forces XWayland (when present).
        assert!(!prefer_x11(true, true, Some("GNOME"), Some("1")));
        assert!(!prefer_x11(true, true, Some("GNOME"), Some("")));
        assert!(prefer_x11(true, true, Some("niri"), Some("0")));
        assert!(!prefer_x11(false, true, Some("niri"), Some("0")));
    }
}

#[cfg(test)]
mod icon_tests {
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_preserves_the_bundle_icon_instead_of_setting_a_runtime_bitmap() {
        let viewport = super::app_viewport().expect("viewport");
        assert_eq!(viewport.icon.as_deref(), Some(&eframe::egui::IconData::default()));
        assert_eq!(viewport.app_id.as_deref(), Some(super::APP_ID));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn desktop_viewport_supplies_the_original_mark_to_the_window() {
        let viewport = super::app_viewport().expect("viewport");
        let icon = viewport.icon.expect("running window must have an icon");
        assert_eq!((icon.width, icon.height), (64, 64));
        assert_eq!(icon.rgba, super::app_icon::rgba(64).expect("original mark"));
        assert_eq!(viewport.app_id.as_deref(), Some(super::APP_ID));
    }
}

#[cfg(test)]
mod msaa_tests {
    #[test]
    fn first_launch_adopts_the_previous_release_settings() {
        let d = std::env::temp_dir().join(format!("solidcraft-adopt-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let (current, legacy, legacy_eframe) = (d.join("solidcraft"), d.join("solvecraft"), d.join("ai.storyteller.solvecraft"));
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::create_dir_all(&legacy_eframe).unwrap();
        std::fs::write(legacy.join("graphics.json"), r#"{"msaa":8}"#).unwrap();
        std::fs::write(legacy_eframe.join("app.ron"), "(prefs)").unwrap();
        super::adopt_settings(&current, &legacy, Some(&legacy_eframe)).unwrap();
        assert_eq!(std::fs::read_to_string(current.join("graphics.json")).unwrap(), r#"{"msaa":8}"#);
        assert_eq!(std::fs::read_to_string(current.join("app.ron")).unwrap(), "(prefs)");
        assert!(legacy.join("graphics.json").is_file() && legacy_eframe.join("app.ron").is_file());
        // Later launches leave the current settings alone.
        std::fs::write(current.join("app.ron"), "(mine)").unwrap();
        std::fs::remove_file(current.join("graphics.json")).unwrap();
        super::adopt_settings(&current, &legacy, Some(&legacy_eframe)).unwrap();
        assert_eq!(std::fs::read_to_string(current.join("app.ron")).unwrap(), "(mine)");
        assert!(!current.join("graphics.json").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn saved_multisampling_uses_the_selected_backend_policy() {
        use eframe::wgpu::Backend;
        for requested in [1, 2, 4, 8] {
            assert_eq!(eframe::egui_wgpu::effective_msaa_samples(Backend::Gl, requested), 1);
            for backend in [Backend::Vulkan, Backend::Dx12, Backend::Metal] {
                assert_eq!(eframe::egui_wgpu::effective_msaa_samples(backend, requested), requested);
            }
        }
    }
}
