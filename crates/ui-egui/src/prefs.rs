//! Preferences, one dialog in the spirit of Fusion's, opened from the gear at the top right of the
//! window, File › Preferences or Ctrl+, (Cmd+, on macOS): General (theme, UI scale, units of new
//! designs, autosave, display precision, the agent cursor), Navigation (mouse presets, zoom
//! direction, zoom to the cursor, orbit centre, Look At animation), Keyboard Shortcuts (search,
//! rebind, conflicts; `keymap`), Design (default modelling orientation, Y up or Z up), Graphics
//! (visual style, grid, shadow, perspective, anti-aliasing), Sketch and Selection. Changes apply
//! at once; Cancel puts back what was there when the dialog opened, OK keeps them, Apply keeps
//! them and stays open, and Restore Defaults resets the page shown. Everything is kept between
//! runs, except anti-aliasing, which needs a restart.

use egui::{Modifiers, Pos2, RichText};
use serde::{Deserialize, Serialize};
use serde_json::json;
use solidcraft_engine::geom::Vec3;

/// The key the apps keep the preferences under in eframe's storage.
pub const STORAGE_KEY: &str = "solidcraft.prefs";
/// The key earlier releases used. Compatibility shim (docs/compatibility.md): read when the
/// current key is missing, never written.
pub const LEGACY_STORAGE_KEY: &str = "solvecraft.prefs";

/// The stored preferences, from the current key or else the legacy one.
pub fn stored(get: impl Fn(&str) -> Option<String>) -> Option<String> {
    get(STORAGE_KEY).or_else(|| get(LEGACY_STORAGE_KEY))
}

use crate::SolveApp;
use crate::theme::Tokens;
use crate::viewport::NavMode;

pub const UNITS: [&str; 5] = ["mm", "cm", "m", "in", "ft"];
pub const OPENSCAD_NAV: &str = "openscad";
/// A mouse button that can drag the camera.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavButton {
    Left,
    Middle,
    Right,
}

/// Modifier keys a binding needs held (others may be held too).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct NavKeys {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl NavKeys {
    const NONE: NavKeys = NavKeys { shift: false, ctrl: false, alt: false };
    const SHIFT: NavKeys = NavKeys { shift: true, ctrl: false, alt: false };
    const CTRL: NavKeys = NavKeys { shift: false, ctrl: true, alt: false };
    const SHIFT_ALT: NavKeys = NavKeys { shift: true, ctrl: false, alt: true };

    /// All of these keys are held (Ctrl is Cmd on macOS).
    fn held(self, m: Modifiers) -> bool {
        (!self.shift || m.shift) && (!self.ctrl || m.ctrl || m.command) && (!self.alt || m.alt)
    }
}

/// One camera binding: dragging `button` with `keys` held does `mode`.
#[derive(Clone, Copy, Debug)]
pub struct NavBinding {
    pub button: NavButton,
    pub keys: NavKeys,
    pub mode: NavMode,
}

const fn bind(button: NavButton, keys: NavKeys, mode: NavMode) -> NavBinding {
    NavBinding { button, keys, mode }
}

/// A mouse mapping: its bindings, most specific first (the first one whose button is dragged
/// with all its keys held wins).
pub struct NavPreset {
    pub id: &'static str,
    pub label: &'static str,
    /// What the mouse does, for the Preferences page.
    pub help: &'static str,
    /// Choosing the preset turns the wheel the other way.
    pub reverse_zoom: bool,
    pub bindings: &'static [NavBinding],
}

impl NavPreset {
    /// What dragging `button` with `mods` held does, if anything.
    pub fn mode(&self, button: NavButton, mods: Modifiers) -> Option<NavMode> {
        self.bindings.iter().find(|b| b.button == button && b.keys.held(mods)).map(|b| b.mode)
    }
}

use NavButton::{Left, Middle, Right};
use NavMode::{Orbit, Pan, Zoom};

/// The mouse mappings. The first is the default.
pub const NAV_PRESETS: [NavPreset; 6] = [
    NavPreset {
        id: "fusion",
        label: "Fusion",
        help: "Middle drag pans, Shift+middle orbits, right drag orbits",
        reverse_zoom: false,
        bindings: &[bind(Middle, NavKeys::SHIFT, Orbit), bind(Middle, NavKeys::NONE, Pan), bind(Right, NavKeys::NONE, Orbit)],
    },
    NavPreset {
        id: "solidworks",
        label: "SolidWorks",
        help: "Middle drag orbits, Ctrl+middle pans, Shift+middle zooms, right drag orbits",
        reverse_zoom: true,
        bindings: &[
            bind(Middle, NavKeys::CTRL, Pan),
            bind(Middle, NavKeys::SHIFT, Zoom),
            bind(Middle, NavKeys::NONE, Orbit),
            bind(Right, NavKeys::NONE, Orbit),
        ],
    },
    NavPreset {
        id: "inventor",
        label: "Inventor",
        help: "Middle drag pans, Shift+middle orbits, Ctrl+middle zooms, right drag orbits",
        reverse_zoom: false,
        bindings: &[
            bind(Middle, NavKeys::SHIFT, Orbit),
            bind(Middle, NavKeys::CTRL, Zoom),
            bind(Middle, NavKeys::NONE, Pan),
            bind(Right, NavKeys::NONE, Orbit),
        ],
    },
    NavPreset {
        id: "alias",
        label: "Alias",
        help: "Shift+Alt+left drag orbits, Shift+Alt+middle pans, Shift+Alt+right zooms; middle drag also pans and right drag orbits",
        reverse_zoom: false,
        bindings: &[
            bind(Left, NavKeys::SHIFT_ALT, Orbit),
            bind(Middle, NavKeys::SHIFT_ALT, Pan),
            bind(Right, NavKeys::SHIFT_ALT, Zoom),
            bind(Middle, NavKeys::CTRL, Zoom),
            bind(Middle, NavKeys::NONE, Pan),
            bind(Right, NavKeys::NONE, Orbit),
        ],
    },
    NavPreset {
        id: "tinkercad",
        label: "Tinkercad",
        help: "Right drag orbits, middle or Shift+right drag pans",
        reverse_zoom: false,
        bindings: &[bind(Right, NavKeys::SHIFT, Pan), bind(Right, NavKeys::NONE, Orbit), bind(Middle, NavKeys::NONE, Pan)],
    },
    NavPreset {
        id: OPENSCAD_NAV,
        label: "OpenSCAD",
        help: "Left drag on empty space orbits, Shift+left drag box-selects, sketch geometry drags edit; right or middle drag pans, Shift+right or Shift+middle drag zooms",
        reverse_zoom: false,
        bindings: &[
            bind(Left, NavKeys::NONE, Orbit),
            bind(Right, NavKeys::SHIFT, Zoom),
            bind(Middle, NavKeys::SHIFT, Zoom),
            bind(Right, NavKeys::NONE, Pan),
            bind(Middle, NavKeys::NONE, Pan),
        ],
    },
];

/// The preset with this id (the default for an unknown one).
pub fn nav_preset(id: &str) -> &'static NavPreset {
    NAV_PRESETS.iter().find(|p| p.id == id).unwrap_or(&NAV_PRESETS[0])
}
/// The dialog's pages, in order.
pub const PAGES: [&str; 7] = ["General", "Navigation", "Keyboard Shortcuts", "Design", "Graphics", "Sketch", "Selection"];
pub const PAGE_SHORTCUTS: usize = 2;
pub const PAGE_DESIGN: usize = 3;
pub const PAGE_GRAPHICS: usize = 4;
pub const THEMES: [(&str, &str); 3] = [("dark", "Dark"), ("light", "Light"), ("system", "System")];
pub const MSAA: [u32; 4] = [1, 2, 4, 8];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Units of new designs.
    pub default_units: String,
    /// Mouse mapping: `fusion`, `solidworks` or `inventor`.
    pub nav: String,
    /// Show the radial ring and enable hold/flick gestures for viewport context menus.
    pub radial_context_menu: bool,
    /// The wheel zooms out when turned forward.
    pub zoom_reverse: bool,
    /// Orbit around the point under the cursor (else around the view's centre).
    pub orbit_cursor: bool,
    /// Whether a connected 3D mouse moves the view.
    pub space_mouse_enabled: bool,
    /// Speed multiplier for 3D mouse navigation.
    pub space_mouse_speed: f32,
    /// Anti-aliasing samples (applies on restart).
    pub msaa: u32,
    /// Decimals shown for lengths and angles.
    pub length_decimals: u8,
    pub angle_decimals: u8,
    /// `dark`, `light` or `system` (follow the operating system).
    pub theme: String,
    /// Interface language (a registered locale ID).
    pub language: String,
    /// Interface scale (1 = 100 %).
    pub ui_scale: f32,
    /// The wheel zooms toward the point under the cursor (else toward the view's centre).
    pub zoom_to_cursor: bool,
    /// Spread discrete wheel input across frames (off: apply each event immediately).
    pub smooth_wheel_zoom: bool,
    /// Two-finger trackpad scroll pans (off: it zooms like the wheel); pinch always zooms.
    pub trackpad_pan: bool,
    /// View changes (axis view, Look At, Home) animate; `look_at_speed` scales how fast.
    pub look_at_anim: bool,
    pub look_at_speed: f32,
    /// Default modelling orientation: `z` (Z up) or `y` (Y up) (#3).
    pub up_axis: String,
    /// After a STEP or IGES file opens, offer to stand a Y-up part up (`stand_up`).
    pub offer_stand_up: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            default_units: "mm".into(),
            nav: "fusion".into(),
            radial_context_menu: false,
            zoom_reverse: false,
            orbit_cursor: false,
            space_mouse_enabled: true,
            space_mouse_speed: 1.0,
            msaa: 4,
            length_decimals: 2,
            angle_decimals: 1,
            theme: "dark".into(),
            language: "en".into(),
            ui_scale: 1.0,
            zoom_to_cursor: true,
            smooth_wheel_zoom: true,
            trackpad_pan: true,
            look_at_anim: true,
            look_at_speed: 1.0,
            up_axis: "z".into(),
            offer_stand_up: true,
        }
    }
}

/// The preferences window.
#[derive(Default)]
pub struct PrefsWindow {
    pub open: bool,
    /// The page shown (index into [`PAGES`]).
    pub section: usize,
    /// Everything as it was when the dialog opened (or at the last Apply), for Cancel.
    snapshot: Option<Snapshot>,
    /// The up axis preference the view follows now (applied when it changes, and at startup).
    applied_up: Option<String>,
}

/// Open the dialog at a page.
pub fn open_at(app: &mut SolveApp, page: usize) {
    app.prefs_window.open = true;
    app.prefs_window.section = page.min(PAGES.len() - 1);
}

/// Every setting the dialog changes, for Cancel and Restore Defaults.
#[derive(Clone)]
struct Snapshot {
    prefs: Prefs,
    dark: bool,
    grid: bool,
    shadow: bool,
    perspective: bool,
    style: u8,
    pick_bodies: bool,
    agent: crate::agent_cursor::Settings,
    autosave: f64,
    auto_project: bool,
    keys: std::collections::BTreeMap<String, String>,
}

impl Snapshot {
    fn take(app: &SolveApp) -> Snapshot {
        Snapshot {
            prefs: app.preferences.clone(),
            dark: app.ui.dark,
            grid: app.ui.show_grid,
            shadow: app.ui.ground_shadow,
            perspective: app.ui.perspective,
            style: app.ui.visual_style,
            pick_bodies: app.ui.pick_bodies,
            agent: app.ui.agent_cursor,
            autosave: app.autosave_minutes,
            auto_project: app.session.auto_project,
            keys: app.keymap.custom.clone(),
        }
    }
    fn restore(self, app: &mut SolveApp) {
        let msaa = app.preferences.msaa;
        app.preferences = self.prefs;
        app.ui.dark = self.dark;
        app.ui.show_grid = self.grid;
        app.ui.ground_shadow = self.shadow;
        app.ui.perspective = self.perspective;
        app.ui.visual_style = self.style;
        app.ui.pick_bodies = self.pick_bodies;
        app.ui.agent_cursor = self.agent;
        app.autosave_minutes = self.autosave;
        set_autosave(app);
        app.session.auto_project = self.auto_project;
        app.keymap.custom = self.keys;
        if app.preferences.msaa != msaa {
            save_graphics(app);
        }
    }
}

/// Restore Defaults for one page.
fn defaults(app: &mut SolveApp, page: usize) {
    let d = Prefs::default();
    let p = &mut app.preferences;
    match page {
        0 => {
            p.theme = d.theme;
            p.language = d.language;
            p.ui_scale = d.ui_scale;
            p.default_units = d.default_units;
            p.length_decimals = d.length_decimals;
            p.angle_decimals = d.angle_decimals;
            app.ui.dark = true;
            app.ui.agent_cursor = Default::default();
            app.autosave_minutes = 5.0;
            set_autosave(app);
        }
        1 => {
            p.nav = d.nav;
            p.radial_context_menu = d.radial_context_menu;
            p.zoom_reverse = d.zoom_reverse;
            p.zoom_to_cursor = d.zoom_to_cursor;
            p.smooth_wheel_zoom = d.smooth_wheel_zoom;
            p.trackpad_pan = d.trackpad_pan;
            p.orbit_cursor = d.orbit_cursor;
            p.space_mouse_enabled = d.space_mouse_enabled;
            p.space_mouse_speed = d.space_mouse_speed;
            p.look_at_anim = d.look_at_anim;
            p.look_at_speed = d.look_at_speed;
        }
        PAGE_SHORTCUTS => crate::keymap::reset(app, None),
        PAGE_DESIGN => {
            p.up_axis = d.up_axis;
            p.offer_stand_up = d.offer_stand_up;
        }
        PAGE_GRAPHICS => {
            let msaa = p.msaa != d.msaa;
            p.msaa = d.msaa;
            app.ui.visual_style = 0;
            app.ui.show_grid = true;
            app.ui.ground_shadow = true;
            app.ui.perspective = false;
            if msaa {
                save_graphics(app);
            }
        }
        5 => app.session.auto_project = true,
        _ => app.ui.pick_bodies = false,
    }
}

/// Settings that follow the preferences every frame: the theme when it follows the system, and
/// the interface scale.
pub fn apply(app: &mut SolveApp, ctx: &egui::Context) {
    // The view's up axis follows the preference (set in the dialog, by ui.prefs or loaded).
    if app.prefs_window.applied_up.as_deref() != Some(app.preferences.up_axis.as_str()) {
        up_axis_changed(app);
    }
    if app.preferences.theme == "system"
        && let Some(t) = ctx.system_theme()
    {
        app.ui.dark = t == egui::Theme::Dark;
    }
    // egui's end-of-frame shortcuts only change its context; the saved preference would
    // overwrite that change next frame. Consume the desktop shortcuts into the preference.
    if ctx.options(|o| o.zoom_with_keyboard) && !crate::keymap::capturing(app) {
        use egui::gui_zoom::kb_shortcuts::{ZOOM_IN, ZOOM_IN_SECONDARY, ZOOM_OUT, ZOOM_RESET};
        if ctx.input_mut(|i| i.consume_shortcut(&ZOOM_RESET)) {
            app.preferences.ui_scale = 1.0;
        } else {
            let zoom_in = ctx.input_mut(|i| i.consume_shortcut(&ZOOM_IN) || i.consume_shortcut(&ZOOM_IN_SECONDARY));
            let zoom_out = ctx.input_mut(|i| i.consume_shortcut(&ZOOM_OUT));
            if zoom_in != zoom_out {
                app.preferences.ui_scale = (app.preferences.ui_scale + if zoom_in { 0.1 } else { -0.1 }).clamp(0.5, 2.5);
            }
        }
    }
    // Scaling while the pointer is held moves the slider under the pointer, feeding a
    // different value back into it. Commit the layout change after the drag ends.
    let z = app.preferences.ui_scale.clamp(0.5, 2.5);
    if z.is_finite() && !ctx.input(|i| i.pointer.any_down()) && (ctx.zoom_factor() - z).abs() > 1e-3 {
        ctx.set_zoom_factor(z);
    }
}

/// A new, empty design takes the preferred units (not an undo step, not a change to save).
pub fn apply_new_design(app: &mut SolveApp) {
    if app.session.doc.features.is_empty() && app.session.doc.units != app.preferences.default_units {
        let saved = !app.session.is_dirty();
        if app.session.execute("document.units", &json!({ "units": app.preferences.default_units })).is_ok() {
            app.session.undo.pop();
            if saved {
                app.session.mark_saved();
            }
        }
    }
}

/// The autosave interval follows `app.autosave_minutes` (0: off).
pub fn set_autosave(app: &mut SolveApp) {
    let m = app.autosave_minutes;
    if let Some(a) = app.autosave.as_mut() {
        a.interval = if m > 0.0 { std::time::Duration::from_secs_f64(m * 60.0) } else { std::time::Duration::from_secs(u64::MAX / 4) };
    }
}

/// Which navigation dragging `button` with `mods` held does under the preset.
pub fn nav_mode(app: &SolveApp, button: NavButton, mods: Modifiers) -> Option<NavMode> {
    nav_preset(&app.preferences.nav).mode(button, mods)
}

/// +1, or −1 when the wheel zooms the other way.
pub fn zoom_sign(app: &SolveApp) -> f64 {
    if app.preferences.zoom_reverse { -1.0 } else { 1.0 }
}

/// Orbit by pixel deltas: around the view's centre, or, with "orbit around the cursor", around
/// the model point that was under the cursor when the drag began (kept still on screen). A drag
/// that begins over empty space turns around the centre of the visible geometry instead, so the
/// model never swings around a far-off point. The pivot is chosen once, when the drag begins.
pub fn orbit(app: &mut SolveApp, rect: egui::Rect, dx: f64, dy: f64, press: Option<Pos2>) {
    if !app.preferences.orbit_cursor {
        app.cam.orbit(dx, dy);
        return;
    }
    let pivot = match app.viewport.orbit_pivot {
        Some(p) => p,
        None => {
            let proj = crate::viewport::projection(app, rect);
            let at = press.unwrap_or(rect.center());
            let p = pivot_under(app, &proj, at);
            app.viewport.orbit_pivot = Some(p);
            p
        }
    };
    let before = crate::viewport::projection(app, rect).to_screen(pivot);
    app.cam.orbit(dx, dy);
    let after = crate::viewport::projection(app, rect).to_screen(pivot);
    if let (Some(a), Some(b)) = (before, after) {
        app.cam.pan(f64::from(a.x - b.x), f64::from(a.y - b.y), f64::from(rect.height().max(1.0)));
    }
}

/// The model point under a screen position. Over empty space: the centre of the visible bodies
/// and sketches (what Fit frames), or the view's centre when nothing is visible.
fn pivot_under(app: &SolveApp, proj: &crate::viewport::Proj, at: Pos2) -> Vec3 {
    use crate::viewport::Hit;
    for h in crate::viewport::pick(app, proj, at) {
        match h {
            Hit::Face { point, .. } | Hit::Vertex { point, .. } => return point,
            Hit::Edge { mid, .. } => return mid,
            _ => {}
        }
    }
    let b = solidcraft_engine::view::bounds(&app.session);
    if b.is_empty() { app.cam.target } else { b.center() }
}

/// Units per millimetre and the unit's name.
pub fn unit_scale(units: &str) -> (f64, &'static str) {
    match units {
        "cm" => (0.1, "cm"),
        "m" => (0.001, "m"),
        "in" => (1.0 / 25.4, "in"),
        "ft" => (1.0 / 304.8, "ft"),
        _ => (1.0, "mm"),
    }
}

/// Inch labels retain a meaningful fourth digit without promoting floating-point noise.
pub fn length_precision(value: f64, units: &str, decimals: u8) -> usize {
    let base = usize::from(decimals.min(8));
    if units != "in" {
        return base;
    }
    let needs_four = ((value * 10_000.0).round() - (value * 1000.0).round() * 10.0).abs() >= 0.5;
    base.max(if needs_four { 4 } else { 3 })
}

pub fn length_number(mm: f64, units: &str, decimals: u8) -> String {
    let value = mm * unit_scale(units).0;
    format!("{:.*}", length_precision(value, units, decimals), value)
}

/// Edit a millimetre value through a control expressed in the current design units.
/// Keep the stored value untouched unless the user actually changes the field.
pub fn length_drag<'a>(value: &'a mut f64, units: &str, decimals: u8) -> egui::DragValue<'a> {
    let (scale, unit) = unit_scale(units);
    egui::DragValue::new(value)
        .speed(0.5)
        .suffix(format!(" {unit}"))
        .custom_formatter(move |mm, _| length_number(mm, unit, decimals))
        .custom_parser(move |text| parse_length_mm(text, scale))
}

/// A length typed into a field, in millimetres: a bare number is in the design's units (`scale`
/// units per millimetre); an explicit unit ("10 mm", "0.5 in", "2 cm + 3 mm") is honoured.
pub fn parse_length_mm(text: &str, scale: f64) -> Option<f64> {
    use solidcraft_engine::doc::expr;
    if !(scale.is_finite() && scale > 0.0) {
        return None;
    }
    let defaults = expr::Defaults { len: 1.0 / scale, ..expr::Defaults::default() };
    let no_params = |n: &str| Err(solidcraft_engine::doc::DocError::Expr(format!("unknown parameter `{n}`")));
    let value = expr::eval_with_defaults(text.trim(), &no_params, defaults).ok()?.to_kind_in(expr::Kind::Length, defaults).ok()?;
    value.is_finite().then_some(value)
}

/// A length in millimetres in `units`, with `decimals` (for display).
pub fn format_length(mm: f64, units: &str, decimals: u8) -> String {
    let (_, u) = unit_scale(units);
    format!("{} {u}", length_number(mm, units, decimals))
}

/// A length, area (`power` 2) or volume (3) given in millimetres, shown in the design's units
/// with the length precision.
pub fn show_mm(app: &SolveApp, v: f64, power: i32) -> String {
    let (k, u) = unit_scale(&app.session.doc.units);
    let sup = match power {
        2 => "²",
        3 => "³",
        _ => "",
    };
    let value = v * k.powi(power);
    format!("{:.*} {u}{sup}", length_precision(value, &app.session.doc.units, app.preferences.length_decimals), value)
}

/// Numbers in millimetres, in the design's units without the unit name.
pub fn show_mm_bare(app: &SolveApp, v: f64) -> String {
    length_number(v, &app.session.doc.units, app.preferences.length_decimals)
}

/// Write the anti-aliasing setting where the host reads it at start (the window's samples can't
/// change while it is open).
fn save_graphics(app: &SolveApp) {
    if let Some(path) = &app.services.graphics_path {
        let _ = std::fs::write(path, json!({ "msaa": app.preferences.msaa }).to_string());
    }
}

/// At start, with the graphics device known: the saved anti-aliasing (`requested` samples, as
/// the host read it) is more than the device supports. The window would not have opened with it,
/// so the renderer fell back; put the setting back to its default (or the largest supported
/// count below it) and tell the user what changed.
pub fn check_msaa(app: &mut SolveApp, requested: u32) {
    let supported = &app.msaa_supported;
    if requested <= 1 || supported.is_empty() || supported.contains(&requested) {
        return;
    }
    let default = Prefs::default().msaa;
    let reset = supported.iter().copied().filter(|&n| n <= default).max().unwrap_or(1);
    let supported = supported.clone();
    app.preferences.msaa = reset;
    save_graphics(app);
    app.settings_reset.push(SettingReset::Msaa { requested, reset, supported });
}

/// A setting put back at start because it would keep SolidCraft from starting.
#[derive(Clone, Debug, PartialEq)]
pub enum SettingReset {
    /// Anti-aliasing samples the graphics device can't render with.
    Msaa { requested: u32, reset: u32, supported: Vec<u32> },
}

impl SettingReset {
    /// The line the notice shows, in the current interface language.
    pub fn message(&self) -> String {
        match self {
            SettingReset::Msaa { requested, reset, supported } => {
                let setting = format!("{} › {}", crate::i18n::tr("ui.graphics"), crate::i18n::tr("ui.anti_aliasing"));
                let supported = supported.iter().map(|n| format!("{n}")).collect::<Vec<_>>().join("×, ") + "×";
                crate::i18n::format(
                    "settings.msaa_reset",
                    &[("setting", &setting), ("requested", &requested.to_string()), ("reset", &reset.to_string()), ("supported", &supported)],
                )
            }
        }
    }
}

/// Settings put back at start (`SolveApp::settings_reset`), until dismissed.
pub fn show_reset_notice(app: &mut SolveApp, ctx: &egui::Context) {
    if app.settings_reset.is_empty() {
        return;
    }
    let mut ok = false;
    let mut prefs = false;
    crate::frame::window(ctx, app.tr("settings.reset_title"), crate::frame::Width::Normal)
        .id(egui::Id::new("sc_settings_reset"))
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(app.tr("settings.reset_intro"));
            ui.add_space(4.0);
            for reset in &app.settings_reset {
                ui.label(format!("• {}", reset.message()));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ok = ui.button(app.tr("ui.heading_ok")).clicked();
                prefs = ui.button(app.tr("ui.open_preferences")).clicked();
            });
        });
    if ok || prefs {
        app.settings_reset.clear();
    }
    if prefs {
        open_at(app, PAGE_GRAPHICS);
    }
}

pub fn show(app: &mut SolveApp, ctx: &egui::Context) {
    // File › Keyboard Shortcuts and Help › Keyboard Shortcuts open this dialog at that page.
    if app.keymap.open {
        app.keymap.open = false;
        open_at(app, PAGE_SHORTCUTS);
    }
    if !app.prefs_window.open {
        app.prefs_window.snapshot = None;
        return;
    }
    if app.prefs_window.snapshot.is_none() {
        app.prefs_window.snapshot = Some(Snapshot::take(app));
    }
    let t = Tokens::get();
    let mut open = true;
    let mut button: Option<&str> = None;
    let before = app.preferences.clone();
    let shown = crate::frame::window(ctx, app.tr("ui.preferences"), crate::frame::Width::Wide)
        .id(egui::Id::new("sc_prefs"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            // One fixed size for every page (#290): the page is drawn into a clipped child of
            // that size and only that size is allocated, so no page (or translation) can widen
            // the window; descriptions wrap and tall pages scroll inside it.
            ui.set_max_width(PREFS_W);
            let (body, _) = ui.allocate_exact_size(egui::vec2(PREFS_W, PREFS_H), egui::Sense::hover());
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body).layout(egui::Layout::left_to_right(egui::Align::Min)));
            child.shrink_clip_rect(body);
            let ui_body = &mut child;
            ui_body.vertical(|ui| {
                ui.set_width(PREFS_NAV_W);
                for (i, s) in PAGES.iter().enumerate() {
                    if ui.selectable_label(app.prefs_window.section == i, app.tr_label(s)).clicked() {
                        app.prefs_window.section = i;
                        app.keymap.read_only = false;
                    }
                }
            });
            ui_body.separator();
            ui_body.vertical(|ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(app.tr_label(PAGES.get(app.prefs_window.section).copied().unwrap_or_default())).strong().size(14.0));
                ui.add_space(6.0);
                if app.prefs_window.section == PAGE_SHORTCUTS {
                    // The shortcuts table scrolls by itself.
                    page(app, ui, &t);
                } else {
                    egui::ScrollArea::vertical().id_salt("sc_prefs_page").auto_shrink([false, false]).show(ui, |ui| page(app, ui, &t));
                }
            });
            ui.separator();
            // Restore Defaults at the left; OK, Cancel and Apply flush right, as in every dialog.
            ui.horizontal(|ui| {
                ui.set_width(PREFS_W);
                if ui.button(app.tr("ui.restore_defaults")).on_hover_text(app.tr("ui.reset_the_settings_on_this_page")).clicked() {
                    button = Some("defaults");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Right to left: Apply is the rightmost.
                    for (key, id) in [("ui.apply", "apply"), ("ui.cancel", "cancel"), ("ui.heading_ok", "ok")] {
                        let r = ui.button(app.tr(key));
                        crate::scenario::publish_rect(&format!("prefs:{id}"), r.rect);
                        if r.clicked() {
                            button = Some(id);
                        }
                    }
                });
                crate::scenario::publish_rect("prefs:footer", ui.max_rect());
            });
        });
    if let Some(w) = &shown {
        crate::scenario::publish_rect("prefs:window", w.response.rect);
    }
    if app.preferences.msaa != before.msaa {
        save_graphics(app);
    }
    if app.preferences.up_axis != before.up_axis {
        up_axis_changed(app);
    }
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape)) && !crate::keymap::capturing(app);
    match button {
        Some("defaults") => defaults(app, app.prefs_window.section),
        Some("apply") => app.prefs_window.snapshot = Some(Snapshot::take(app)),
        Some("ok") => close(app, false),
        Some("cancel") => close(app, true),
        _ if !open || esc => close(app, !open || esc),
        _ => {}
    }
}

/// Close the dialog; `revert`: put back what was there when it opened (Cancel, Esc, ×).
fn close(app: &mut SolveApp, revert: bool) {
    if let Some(s) = app.prefs_window.snapshot.take()
        && revert
    {
        let up = app.preferences.up_axis.clone();
        s.restore(app);
        if app.preferences.up_axis != up {
            up_axis_changed(app);
        }
    }
    app.prefs_window.open = false;
    app.keymap.read_only = false;
    crate::keymap::stop_capture(app);
}

/// The default orientation changed: the view turns to match.
fn up_axis_changed(app: &mut SolveApp) {
    use solidcraft_engine::render::UpAxis;
    app.prefs_window.applied_up = Some(app.preferences.up_axis.clone());
    let up = if app.preferences.up_axis.eq_ignore_ascii_case("y") { UpAxis::Y } else { UpAxis::Z };
    if app.ui.up_axis != up || app.cam.up != up {
        app.set_up_axis(up);
    }
}

/// The Preferences window's content size: the same on every page.
const PREFS_W: f32 = 744.0;
const PREFS_H: f32 = 400.0;
/// The page list at its left.
const PREFS_NAV_W: f32 = 128.0;

/// One page of the dialog.
fn page(app: &mut SolveApp, ui: &mut egui::Ui, t: &Tokens) {
    let note = |ui: &mut egui::Ui, s: &str| {
        ui.label("");
        // Descriptions wrap within the page instead of widening it.
        ui.add(egui::Label::new(RichText::new(s).size(11.0).color(t.text_dim)).wrap());
        ui.end_row();
    };
    if app.prefs_window.section == PAGE_SHORTCUTS {
        crate::keymap::page(app, ui);
        return;
    }
    egui::Grid::new(("sc_prefs_grid", app.prefs_window.section)).num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| match app.prefs_window.section {
        0 => {
            ui.label(app.tr("ui.language"));
            egui::ComboBox::from_id_salt("pref_language").selected_text(crate::i18n::Locale::for_language(&app.preferences.language).name()).show_ui(
                ui,
                |ui| {
                    for locale in crate::i18n::LOCALES {
                        let (id, label) = (locale.id, locale.name);
                        ui.selectable_value(&mut app.preferences.language, id.to_string(), label);
                    }
                },
            );
            ui.end_row();
            ui.label(app.tr("ui.theme"));
            ui.horizontal(|ui| {
                for (id, label) in THEMES {
                    if ui.radio(app.preferences.theme == id, app.tr_label(label)).clicked() {
                        app.preferences.theme = id.to_string();
                        if id != "system" {
                            app.ui.dark = id == "dark";
                        }
                    }
                }
            });
            ui.end_row();
            ui.label(app.tr("ui.ui_scale"));
            let scale = ui.add(
                egui::Slider::new(&mut app.preferences.ui_scale, 0.5..=2.5).step_by(0.05).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
            );
            crate::scenario::publish_rect("prefs:scale", scale.rect);
            ui.end_row();
            ui.label(app.tr("ui.units_of_new_designs"));
            egui::ComboBox::from_id_salt("pref_units").selected_text(app.preferences.default_units.clone()).show_ui(ui, |ui| {
                for u in UNITS {
                    ui.selectable_value(&mut app.preferences.default_units, u.to_string(), u);
                }
            });
            ui.end_row();
            ui.label(app.tr("ui.autosave_every"));
            if ui.add(egui::DragValue::new(&mut app.autosave_minutes).range(0.0..=120.0).speed(0.5).suffix(" min")).changed() {
                set_autosave(app);
            }
            ui.end_row();
            note(ui, "0 turns it off");
            ui.label(app.tr("ui.length_decimals"));
            ui.add(egui::DragValue::new(&mut app.preferences.length_decimals).range(0..=6));
            ui.end_row();
            ui.label(app.tr("ui.angle_decimals"));
            ui.add(egui::DragValue::new(&mut app.preferences.angle_decimals).range(0..=6));
            ui.end_row();
            ui.label(app.tr("ui.agent_cursor"));
            let speed_label = app.tr("ui.agent_cursor_speed");
            let s = &mut app.ui.agent_cursor;
            ui.checkbox(&mut s.show, "Show the pointer an AI agent moves (#35)");
            ui.end_row();
            ui.label(speed_label);
            ui.add_enabled_ui(s.show, |ui| {
                crate::agent_cursor::speed_slider(ui, &mut s.speed);
            });
            ui.end_row();
            ui.label("");
            ui.add_enabled(s.show, egui::Checkbox::new(&mut s.follow_camera, "Follow the camera (Look At sketches)"));
            ui.end_row();
        }
        1 => {
            ui.label("Pan, zoom, orbit");
            egui::ComboBox::from_id_salt("pref_nav").selected_text(nav_preset(&app.preferences.nav).label).show_ui(ui, |ui| {
                for p in &NAV_PRESETS {
                    if ui.selectable_value(&mut app.preferences.nav, p.id.to_string(), p.label).clicked() {
                        app.preferences.zoom_reverse = p.reverse_zoom;
                    }
                }
            });
            ui.end_row();
            let snap_help = if app.preferences.nav == OPENSCAD_NAV { "" } else { "; hold Shift during orbit to snap" };
            note(ui, &format!("{}; Ctrl+left drag on empty space orbits{snap_help}; the wheel zooms", nav_preset(&app.preferences.nav).help));
            ui.label("Context menu");
            ui.checkbox(&mut app.preferences.radial_context_menu, "Radial context menu");
            ui.end_row();
            ui.label("Reverse zoom direction");
            ui.checkbox(&mut app.preferences.zoom_reverse, "");
            ui.end_row();
            ui.label("Zoom to the cursor");
            ui.checkbox(&mut app.preferences.zoom_to_cursor, "")
                .on_hover_text("The wheel zooms toward the point under the cursor (off: toward the view's centre)");
            ui.end_row();
            ui.label("Smooth wheel zoom");
            ui.checkbox(&mut app.preferences.smooth_wheel_zoom, "")
                .on_hover_text("Turn off to apply each wheel event immediately, without a smoothing tail");
            ui.end_row();
            ui.label("Two-finger scroll pans");
            ui.checkbox(&mut app.preferences.trackpad_pan, "").on_hover_text(
                "Trackpad gestures pan (off: they zoom like the wheel); mouse wheel zooms, thumb wheel pans sideways, pinch always zooms",
            );
            ui.end_row();
            ui.label("Orbit around");
            ui.horizontal(|ui| {
                ui.radio_value(&mut app.preferences.orbit_cursor, false, "View centre");
                ui.radio_value(&mut app.preferences.orbit_cursor, true, "Point under the cursor");
            });
            ui.end_row();
            ui.label("Animate view changes");
            ui.checkbox(&mut app.preferences.look_at_anim, "Axis view, Look At, Home and Fit");
            ui.end_row();
            ui.label("Animation speed");
            ui.add_enabled(
                app.preferences.look_at_anim,
                egui::Slider::new(&mut app.preferences.look_at_speed, 0.5..=3.0).step_by(0.25).custom_formatter(|v, _| format!("{v:.2}×")),
            );
            ui.end_row();
            ui.label("3D mouse (Windows)");
            ui.checkbox(&mut app.preferences.space_mouse_enabled, "Navigate with SpaceNavigator / SpaceMouse");
            ui.end_row();
            ui.label("3D mouse speed");
            ui.add_enabled(
                app.preferences.space_mouse_enabled,
                egui::Slider::new(&mut app.preferences.space_mouse_speed, 0.25..=3.0).step_by(0.05).custom_formatter(|v, _| format!("{v:.2}×")),
            );
            ui.end_row();
            note(ui, "Move and rotate the view together; sketches use pan and zoom only.");
        }
        PAGE_DESIGN => {
            ui.label("Default modelling orientation");
            ui.horizontal(|ui| {
                ui.radio_value(&mut app.preferences.up_axis, "z".to_string(), "Z up");
                ui.radio_value(&mut app.preferences.up_axis, "y".to_string(), "Y up");
            });
            ui.end_row();
            note(ui, "Which axis points up in the views (axis view, Home, Look At). Designs are unchanged.");
            ui.label("Offer to stand up Y-up files");
            ui.checkbox(&mut app.preferences.offer_stand_up, "")
                .on_hover_text("After a STEP or IGES file opens with Z up, offer a quarter turn about X");
            ui.end_row();
        }
        4 => {
            ui.label("Visual style");
            let styles = ["Shaded with edges", "Shaded", "Wireframe", "Shaded with hidden edges"];
            egui::ComboBox::from_id_salt("pref_style").selected_text(styles[usize::from(app.ui.visual_style.min(3))]).show_ui(ui, |ui| {
                for (i, s) in styles.iter().enumerate() {
                    ui.selectable_value(&mut app.ui.visual_style, i as u8, *s);
                }
            });
            ui.end_row();
            ui.label("Grid");
            ui.checkbox(&mut app.ui.show_grid, "");
            ui.end_row();
            ui.label("Ground shadow");
            ui.checkbox(&mut app.ui.ground_shadow, "");
            ui.end_row();
            ui.label("Perspective view");
            ui.checkbox(&mut app.ui.perspective, "");
            ui.end_row();
            ui.label(app.tr("ui.anti_aliasing"));
            egui::ComboBox::from_id_salt("pref_msaa").selected_text(format!("{}×", app.preferences.msaa)).show_ui(ui, |ui| {
                for m in MSAA {
                    // Counts the graphics device can't do would keep the window from opening.
                    let ok = app.msaa_supported.is_empty() || app.msaa_supported.contains(&m);
                    ui.add_enabled_ui(ok, |ui| ui.selectable_value(&mut app.preferences.msaa, m, format!("{m}×")))
                        .inner
                        .on_disabled_hover_text(app.tr("settings.msaa_unsupported"));
                }
            });
            ui.end_row();
            note(ui, app.tr("settings.msaa_applies_on_restart"));
            ui.label("Renderer");
            ui.label(match (&app.viewport.gpu, &app.help.gpu) {
                (Some(_), Some(g)) => format!("GPU: {g}"),
                (Some(_), None) => "GPU".to_string(),
                _ => "CPU (software)".to_string(),
            });
            ui.end_row();
        }
        5 => {
            ui.label("Project face edges into new sketches");
            let mut auto = app.session.auto_project;
            if ui.checkbox(&mut auto, "").changed() {
                let _ = app.run("sketch.auto_project", json!({ "value": auto }));
            }
            ui.end_row();
        }
        _ => {
            ui.label("Click selects whole bodies");
            ui.checkbox(&mut app.ui.pick_bodies, "");
            ui.end_row();
        }
    });
}

/// The control channel's `ui.prefs`: `open` (a page name) opens the dialog there, `button`
/// presses OK, Cancel, Apply or Restore Defaults, `stand_up: true` answers the stand-up offer;
/// any other keys set preferences. Returns them all, the open page and the offer.
pub fn control(app: &mut SolveApp, p: &serde_json::Value) -> Result<serde_json::Value, String> {
    let mut rest = p.as_object().cloned().unwrap_or_default();
    if let Some(page) = rest.remove("open") {
        let name = page.as_str().unwrap_or("General");
        let i = PAGES.iter().position(|x| x.eq_ignore_ascii_case(name)).ok_or_else(|| format!("no page `{name}` ({})", PAGES.join(", ")))?;
        open_at(app, i);
        if app.prefs_window.snapshot.is_none() {
            app.prefs_window.snapshot = Some(Snapshot::take(app));
        }
    }
    if rest.remove("stand_up").and_then(|v| v.as_bool()) == Some(true) {
        crate::stand_up::stand_up(app)?;
    }
    let button = rest.remove("button");
    let mut out = set(app, &serde_json::Value::Object(rest))?;
    match button.as_ref().and_then(|b| b.as_str()) {
        Some("ok") => close(app, false),
        Some("cancel") => close(app, true),
        Some("apply") => app.prefs_window.snapshot = Some(Snapshot::take(app)),
        Some("defaults") => defaults(app, app.prefs_window.section),
        Some(b) => return Err(format!("unknown button `{b}` (ok, cancel, apply, defaults)")),
        None => {}
    }
    if button.is_some() {
        out = serde_json::to_value(&app.preferences).map_err(|e| e.to_string())?;
    }
    if let Some(o) = out.as_object_mut() {
        o.insert("dialog".into(), json!(app.prefs_window.open.then(|| PAGES.get(app.prefs_window.section).copied().unwrap_or_default())));
        o.insert("stand_up_offer".into(), json!(app.stand_up));
        o.insert("dark".into(), json!(app.ui.dark));
    }
    Ok(out)
}

/// Set preferences from JSON (control channel); returns them all.
pub fn set(app: &mut SolveApp, p: &serde_json::Value) -> Result<serde_json::Value, String> {
    let mut cur = serde_json::to_value(&app.preferences).map_err(|e| e.to_string())?;
    if let (Some(o), Some(src)) = (cur.as_object_mut(), p.as_object()) {
        for (k, v) in src {
            if !o.contains_key(k) {
                return Err(format!("unknown preference `{k}`"));
            }
            o.insert(k.clone(), v.clone());
        }
    }
    let new: Prefs = serde_json::from_value(cur).map_err(|e| e.to_string())?;
    if !UNITS.contains(&new.default_units.as_str()) {
        return Err(format!("units must be one of {}", UNITS.join(", ")));
    }
    if !NAV_PRESETS.iter().any(|p| p.id == new.nav) || !MSAA.contains(&new.msaa) {
        return Err("`nav` must be fusion, solidworks, inventor, alias, tinkercad or openscad and `msaa` 1, 2, 4 or 8".into());
    }
    if new.msaa != app.preferences.msaa && !app.msaa_supported.is_empty() && !app.msaa_supported.contains(&new.msaa) {
        return Err(format!("this graphics device doesn't support `msaa` {}; it supports {:?}", new.msaa, app.msaa_supported));
    }
    if !THEMES.iter().any(|(id, _)| *id == new.theme) || !["z", "y"].contains(&new.up_axis.as_str()) {
        return Err("`theme` must be dark, light or system and `up_axis` z or y".into());
    }
    if !crate::i18n::is_supported(&new.language) {
        return Err(format!("`language` must be one of: {}", crate::i18n::LOCALES.iter().map(|locale| locale.id).collect::<Vec<_>>().join(", ")));
    }
    if !(0.5..=2.5).contains(&new.ui_scale) || !(0.25..=4.0).contains(&new.look_at_speed) {
        return Err("`ui_scale` must be 0.5…2.5 and `look_at_speed` 0.25…4".into());
    }
    if new.theme != app.preferences.theme && new.theme != "system" {
        app.ui.dark = new.theme == "dark";
    }
    let up_changed = new.up_axis != app.preferences.up_axis;
    let msaa_changed = new.msaa != app.preferences.msaa;
    app.preferences = new;
    if msaa_changed {
        save_graphics(app);
    }
    if up_changed {
        up_axis_changed(app);
    }
    serde_json::to_value(&app.preferences).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn stored_prefs_fall_back_to_the_legacy_key() {
        let only_old = |k: &str| (k == super::LEGACY_STORAGE_KEY).then(|| "old".to_string());
        assert_eq!(super::stored(only_old).as_deref(), Some("old"));
        let both = |k: &str| Some(if k == super::STORAGE_KEY { "new" } else { "old" }.to_string());
        assert_eq!(super::stored(both).as_deref(), Some("new"));
        assert_eq!(super::stored(|_| None), None);
    }

    use super::*;

    #[test]
    fn unsupported_saved_msaa_is_reset_to_the_default_with_a_notice() {
        let dir = std::env::temp_dir().join(format!("sc_msaa_reset_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("graphics.json");
        let services = crate::Services { graphics_path: Some(path.to_string_lossy().to_string()), ..Default::default() };
        let mut app = SolveApp::new(solidcraft_engine::Session::default(), services);
        // WebGPU's guarantee for the surface format: 8x saved, the device can only do 1x and 4x.
        app.msaa_supported = vec![1, 4];
        app.preferences.msaa = 8;
        check_msaa(&mut app, 8);
        assert_eq!(app.preferences.msaa, 4);
        let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default()).unwrap_or_default();
        assert_eq!(saved["msaa"], 4);
        assert_eq!(app.settings_reset, [SettingReset::Msaa { requested: 8, reset: 4, supported: vec![1, 4] }]);
        assert_eq!(
            app.settings_reset[0].message(),
            "Graphics › Anti-aliasing: 8× isn't supported by this graphics device; it is now 4× (supported: 1×, 4×)."
        );
        {
            let _de = crate::i18n::scope("de");
            assert_eq!(
                app.settings_reset[0].message(),
                "Grafik › Kantenglättung: 8× wird von diesem Grafikgerät nicht unterstützt; jetzt 4× (unterstützt: 1×, 4×)."
            );
        }
        // Below the default when the device can't do that either.
        app.settings_reset.clear();
        app.msaa_supported = vec![1, 2];
        check_msaa(&mut app, 8);
        assert_eq!(app.preferences.msaa, 2);
        // Supported, off, or no GPU: nothing changes (OpenGL's 1x policy isn't a reset either).
        for (supported, requested) in [(vec![1, 2, 4, 8], 8), (vec![1, 4], 1), (vec![], 8), (vec![1, 4], 4)] {
            app.settings_reset.clear();
            app.preferences.msaa = requested;
            app.msaa_supported = supported;
            check_msaa(&mut app, requested);
            assert!(app.settings_reset.is_empty());
            assert_eq!(app.preferences.msaa, requested);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn control_channel_rejects_msaa_the_device_cannot_do_and_bad_saved_msaa_loads_as_default() {
        let mut app = SolveApp::new(solidcraft_engine::Session::default(), crate::Services::default());
        app.msaa_supported = vec![1, 4];
        assert!(set(&mut app, &serde_json::json!({ "msaa": 8 })).is_err());
        assert!(set(&mut app, &serde_json::json!({ "msaa": 1 })).is_ok());
        app.load_prefs(&serde_json::json!({ "preferences": { "msaa": 16 } }).to_string());
        assert_eq!(app.preferences.msaa, Prefs::default().msaa);
    }

    #[test]
    fn length_fields_read_bare_numbers_in_design_units_and_honour_explicit_units() {
        let inch = unit_scale("in").0;
        let close = |a: Option<f64>, b: f64| a.is_some_and(|a| (a - b).abs() < 1e-9);
        assert!(close(parse_length_mm("2", inch), 50.8));
        assert!(close(parse_length_mm(" 10 mm ", inch), 10.0));
        assert!(close(parse_length_mm("0.5 in", 1.0), 12.7));
        assert!(close(parse_length_mm("1 in + 5 mm", unit_scale("cm").0), 30.4));
        assert!(close(parse_length_mm("3", unit_scale("cm").0), 30.0));
        assert!(close(parse_length_mm("3", 1.0), 3.0));
        assert_eq!(parse_length_mm("10 deg", inch), None);
        assert_eq!(parse_length_mm("abc", inch), None);
        assert_eq!(parse_length_mm("", inch), None);
        assert_eq!(parse_length_mm("1e400", 1.0), None);
    }

    #[test]
    fn radial_context_menu_defaults_off_and_follows_preferences_transactions() {
        assert!(!Prefs::default().radial_context_menu);
        let old: Prefs = serde_json::from_value(json!({"nav":"solidworks","zoom_reverse":true})).unwrap();
        assert!(!old.radial_context_menu && old.zoom_reverse && old.nav == "solidworks");
        let mut h = crate::scenario::Harness::new();
        h.call("ui.prefs", json!({"open":"Navigation"}));
        h.step(&json!({"widget":"Radial context menu"})).unwrap();
        assert!(h.app.preferences.radial_context_menu, "the actual checkbox enables radial menus");
        let mut restored = SolveApp::new(solidcraft_engine::Session::default(), Default::default());
        restored.load_prefs(&h.app.prefs());
        assert!(restored.preferences.radial_context_menu, "enabled preference survives save/load");
        h.call("ui.prefs", json!({"button":"cancel"}));
        assert!(!h.app.preferences.radial_context_menu, "Cancel restores the default-off setting");
        h.call("ui.prefs", json!({"open":"Navigation","radial_context_menu":true}));
        h.call("ui.prefs", json!({"button":"apply"}));
        h.call("ui.prefs", json!({"button":"defaults"}));
        assert!(!h.app.preferences.radial_context_menu, "Navigation defaults disable the ring");
        h.call("ui.prefs", json!({"button":"cancel"}));
        assert!(h.app.preferences.radial_context_menu, "Cancel restores the last Apply");
    }

    #[test]
    fn wheel_smoothing_persists_and_follows_dialog_buttons() {
        assert!(serde_json::from_value::<Prefs>(json!({})).unwrap().smooth_wheel_zoom);
        let mut app = SolveApp::new(solidcraft_engine::Session::default(), Default::default());
        control(&mut app, &json!({"open":"Navigation", "smooth_wheel_zoom":false})).unwrap();
        control(&mut app, &json!({"button":"cancel"})).unwrap();
        assert!(app.preferences.smooth_wheel_zoom);
        control(&mut app, &json!({"open":"Navigation", "smooth_wheel_zoom":false})).unwrap();
        control(&mut app, &json!({"button":"apply"})).unwrap();
        control(&mut app, &json!({"smooth_wheel_zoom":true, "button":"cancel"})).unwrap();
        assert!(!app.preferences.smooth_wheel_zoom);
        let mut other = SolveApp::new(solidcraft_engine::Session::default(), Default::default());
        other.load_prefs(&app.prefs());
        assert!(!other.preferences.smooth_wheel_zoom);
        control(&mut app, &json!({"open":"Navigation", "button":"defaults"})).unwrap();
        assert!(app.preferences.smooth_wheel_zoom);
        control(&mut app, &json!({"button":"cancel"})).unwrap();
        assert!(!app.preferences.smooth_wheel_zoom);
    }

    #[test]
    fn openscad_preset_maps_navigation_and_persists() {
        let mut app = SolveApp::new(solidcraft_engine::Session::default(), Default::default());
        set(&mut app, &json!({"nav": OPENSCAD_NAV})).unwrap();
        for button in [NavButton::Middle, NavButton::Right] {
            assert_eq!(nav_mode(&app, button, Modifiers::default()), Some(NavMode::Pan));
            assert_eq!(nav_mode(&app, button, Modifiers { shift: true, ..Default::default() }), Some(NavMode::Zoom));
        }
        assert_eq!(nav_mode(&app, NavButton::Left, Modifiers::default()), Some(NavMode::Orbit));
        let mut other = SolveApp::new(solidcraft_engine::Session::default(), Default::default());
        other.load_prefs(&app.prefs());
        assert_eq!(other.preferences.nav, OPENSCAD_NAV);
    }

    #[test]
    fn trackpad_pan_defaults_on_persists_and_follows_dialog_buttons() {
        assert!(serde_json::from_value::<Prefs>(json!({})).unwrap().trackpad_pan);
        assert!(Prefs::default().trackpad_pan);
        let mut app = SolveApp::new(solidcraft_engine::Session::default(), Default::default());
        control(&mut app, &json!({"open":"Navigation", "trackpad_pan":false})).unwrap();
        control(&mut app, &json!({"button":"cancel"})).unwrap();
        assert!(app.preferences.trackpad_pan);
        control(&mut app, &json!({"open":"Navigation", "trackpad_pan":false})).unwrap();
        control(&mut app, &json!({"button":"apply"})).unwrap();
        assert!(!app.preferences.trackpad_pan);
        let mut other = SolveApp::new(solidcraft_engine::Session::default(), Default::default());
        other.load_prefs(&app.prefs());
        assert!(!other.preferences.trackpad_pan);
        control(&mut app, &json!({"open":"Navigation", "button":"defaults"})).unwrap();
        assert!(app.preferences.trackpad_pan);
        control(&mut app, &json!({"button":"cancel"})).unwrap();
        assert!(!app.preferences.trackpad_pan);
    }

    #[test]
    fn presets_map_the_mouse_and_prefs_round_trip() {
        let mut app = SolveApp::new(solidcraft_engine::Session::default(), Default::default());
        let shift = Modifiers { shift: true, ..Default::default() };
        let ctrl = Modifiers { ctrl: true, ..Default::default() };
        let none = Modifiers::default();
        assert!(matches!(nav_mode(&app, NavButton::Middle, none), Some(NavMode::Pan)));
        assert!(matches!(nav_mode(&app, NavButton::Middle, shift), Some(NavMode::Orbit)));
        set(&mut app, &json!({"nav": "solidworks"})).unwrap();
        assert!(matches!(nav_mode(&app, NavButton::Middle, none), Some(NavMode::Orbit)));
        assert!(matches!(nav_mode(&app, NavButton::Middle, ctrl), Some(NavMode::Pan)));
        set(&mut app, &json!({"nav": "inventor", "zoom_reverse": true})).unwrap();
        assert!(matches!(nav_mode(&app, NavButton::Middle, ctrl), Some(NavMode::Zoom)));
        assert_eq!(zoom_sign(&app), -1.0);
        assert!(matches!(nav_mode(&app, NavButton::Right, none), Some(NavMode::Orbit)), "right drag orbits everywhere");
        assert_eq!(nav_mode(&app, NavButton::Left, none), None, "left drag selects");
        let shift_alt = Modifiers { shift: true, alt: true, ..Default::default() };
        assert_eq!(nav_mode(&app, NavButton::Left, shift_alt), None, "only Alias navigates with the left button");
        set(&mut app, &json!({"nav": "alias"})).unwrap();
        assert_eq!(nav_mode(&app, NavButton::Left, shift_alt), Some(NavMode::Orbit));
        assert_eq!(nav_mode(&app, NavButton::Middle, shift_alt), Some(NavMode::Pan));
        assert_eq!(nav_mode(&app, NavButton::Right, shift_alt), Some(NavMode::Zoom));
        assert_eq!(nav_mode(&app, NavButton::Left, none), None, "a plain left drag still selects");
        assert_eq!(nav_mode(&app, NavButton::Left, shift), None);
        assert_eq!(nav_mode(&app, NavButton::Right, none), Some(NavMode::Orbit));
        assert!(set(&mut app, &json!({"nav": "blender"})).is_err());
        assert!(set(&mut app, &json!({"default_units": "furlong"})).is_err());
        assert!(set(&mut app, &json!({"nope": 1})).is_err());
        // Kept between runs.
        let mut other = SolveApp::new(solidcraft_engine::Session::default(), Default::default());
        other.load_prefs(&app.prefs());
        assert_eq!(other.preferences, app.preferences);
        assert_eq!(format_length(25.4, "in", 3), "1.000 in");
    }

    /// The dialog: changes apply at once, Cancel puts back what was there when it opened, Apply
    /// keeps them, Restore Defaults resets one page, and the shortcuts menus open its page.
    #[test]
    fn preferences_dialog_cancel_apply_and_defaults() {
        let mut app = SolveApp::new(solidcraft_engine::Session::default(), Default::default());
        control(&mut app, &json!({"open": "general"})).unwrap();
        assert!(app.prefs_window.open && app.prefs_window.section == 0);
        control(&mut app, &json!({"theme": "light", "ui_scale": 1.25, "zoom_to_cursor": false})).unwrap();
        assert!(!app.ui.dark, "light applies at once");
        app.keymap.custom.insert("solid.extrude".into(), "Shift+E".into());
        let v = control(&mut app, &json!({"button": "cancel"})).unwrap();
        assert_eq!(v["dialog"], serde_json::Value::Null);
        assert!(app.ui.dark && app.preferences.theme == "dark" && app.preferences.ui_scale == 1.0 && app.preferences.zoom_to_cursor);
        assert!(app.keymap.custom.is_empty(), "a rebinding made while the dialog was open is undone too");

        control(&mut app, &json!({"open": "Navigation", "nav": "alias", "look_at_anim": false})).unwrap();
        control(&mut app, &json!({"button": "apply"})).unwrap();
        control(&mut app, &json!({"nav": "tinkercad"})).unwrap();
        control(&mut app, &json!({"button": "cancel"})).unwrap();
        assert_eq!(app.preferences.nav, "alias", "Cancel goes back to the last Apply");
        let to = {
            let mut c = app.cam;
            c.set_view(solidcraft_engine::render::StandardView::Top);
            c
        };
        app.animate_to(to);
        assert!(app.cam_anim.is_none() && app.cam == to, "no animation: the view jumps");

        control(&mut app, &json!({"open": "Navigation"})).unwrap();
        control(&mut app, &json!({"button": "defaults"})).unwrap();
        assert_eq!(app.preferences.nav, "fusion");
        assert!(app.preferences.look_at_anim);
        control(&mut app, &json!({"button": "ok"})).unwrap();
        assert!(!app.prefs_window.open);

        // File and Help › Keyboard Shortcuts open the same dialog at its page.
        app.keymap.open = true;
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(Default::default(), |ui| show(&mut app, ui.ctx()));
        out.textures_delta.clear();
        assert!(app.prefs_window.open && app.prefs_window.section == PAGE_SHORTCUTS && !app.keymap.open);
        assert!(control(&mut app, &json!({"open": "Plugins"})).is_err());
        assert!(set(&mut app, &json!({"up_axis": "x"})).is_err());
        assert!(set(&mut app, &json!({"theme": "sepia"})).is_err());
        assert!(set(&mut app, &json!({"language": "fr"})).is_err());
    }

    #[test]
    fn language_preference_switches_and_persists() {
        let mut app = SolveApp::new(solidcraft_engine::Session::default(), Default::default());
        assert_eq!(app.preferences.language, "en");
        assert_eq!(app.tr("ui.preferences"), "Preferences");
        assert_eq!(app.tr("ui.extrude"), "Extrude");

        // Switch language to Japanese
        set(&mut app, &json!({"language": "ja"})).unwrap();
        assert_eq!(app.preferences.language, "ja");
        assert_eq!(app.tr("ui.preferences"), "環境設定");
        assert_eq!(app.tr("ui.extrude"), "押し出し");

        // Preferences round-trip
        let mut other = SolveApp::new(solidcraft_engine::Session::default(), Default::default());
        other.load_prefs(&app.prefs());
        assert_eq!(other.preferences.language, "ja");
        assert_eq!(other.tr("ui.preferences"), "環境設定");
    }

    /// A STEP file opened Z-up is offered to be stood up: a quarter turn about X, undoable.
    #[test]
    fn a_y_up_step_file_can_be_stood_up() {
        let dir = std::env::temp_dir().join(format!("solidcraft-standup-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("tall_in_y.step").to_string_lossy().to_string();
        let mut src = SolveApp::new(solidcraft_engine::Session::default(), Default::default());
        src.run("solid.box", json!({"length": 10, "width": 40, "height": 5})).unwrap();
        src.run("file.export", json!({"path": path})).unwrap();

        let mut app = SolveApp::new(solidcraft_engine::Session::default(), Default::default());
        app.open_path(&path);
        let offered = app.stand_up.clone().unwrap_or_default();
        assert_eq!(offered.len(), 1, "the file's body is offered");
        let size = |app: &SolveApp| solidcraft_engine::view::bounds(&app.session);
        assert!((size(&app).max.y - size(&app).min.y - 40.0).abs() < 1e-6);
        control(&mut app, &json!({"stand_up": true})).unwrap();
        let b = size(&app);
        assert!((b.max.z - b.min.z - 40.0).abs() < 1e-6, "Y became Z: {b:?}");
        assert!(app.stand_up.is_none());
        app.run("edit.undo", json!({})).unwrap();
        assert!((size(&app).max.y - size(&app).min.y - 40.0).abs() < 1e-6, "one undo step");

        // Not offered when turned off, or with Y up as the default orientation.
        let mut app = SolveApp::new(solidcraft_engine::Session::default(), Default::default());
        app.preferences.offer_stand_up = false;
        app.open_path(&path);
        assert!(app.stand_up.is_none());
        let mut app = SolveApp::new(solidcraft_engine::Session::default(), Default::default());
        app.preferences.up_axis = "y".into();
        app.open_path(&path);
        assert!(app.stand_up.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn orbiting_around_the_cursor_keeps_that_point_still() {
        let mut app = SolveApp::new(solidcraft_engine::Session::default(), Default::default());
        app.run("solid.box", json!({"length": 40, "width": 40, "height": 40})).unwrap();
        app.preferences.orbit_cursor = true;
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        app.fit_view();
        let at = egui::pos2(500.0, 250.0);
        let pivot = pivot_under(&app, &crate::viewport::projection(&app, rect), at);
        orbit(&mut app, rect, 40.0, 25.0, Some(at));
        let s = crate::viewport::projection(&app, rect).to_screen(pivot).unwrap();
        assert!(s.distance(at) < 2.0, "{s:?}");
    }

    /// A drag that starts over empty space turns around the visible geometry's centre, not a
    /// point far off to the side; with nothing visible it turns around the view's centre.
    #[test]
    fn orbiting_from_empty_space_turns_around_the_visible_geometry() {
        let mut app = SolveApp::new(solidcraft_engine::Session::default(), Default::default());
        app.run("solid.box", json!({"length": 40, "width": 40, "height": 40})).unwrap();
        app.preferences.orbit_cursor = true;
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        app.fit_view();
        let corner = egui::pos2(10.0, 10.0);
        let proj = crate::viewport::projection(&app, rect);
        let hits = crate::viewport::pick(&app, &proj, corner);
        use crate::viewport::Hit;
        assert!(!hits.iter().any(|h| matches!(h, Hit::Face { .. } | Hit::Edge { .. } | Hit::Vertex { .. })), "the corner is empty space");
        let centre = solidcraft_engine::view::bounds(&app.session).center();
        orbit(&mut app, rect, 40.0, 25.0, Some(corner));
        assert_eq!(app.viewport.orbit_pivot, Some(centre));

        let mut app = SolveApp::new(solidcraft_engine::Session::default(), Default::default());
        app.preferences.orbit_cursor = true;
        app.cam.target = solidcraft_engine::geom::Vec3::new(5.0, -3.0, 2.0);
        orbit(&mut app, rect, 40.0, 25.0, Some(corner));
        assert_eq!(app.viewport.orbit_pivot, Some(solidcraft_engine::geom::Vec3::new(5.0, -3.0, 2.0)));
    }

    /// The Y up preference turns the view: the camera's up axis follows it, startup included.
    #[test]
    fn up_axis_preference_turns_the_view() {
        use solidcraft_engine::render::UpAxis;
        let mut app = SolveApp::new(solidcraft_engine::Session::default(), crate::Services::default());
        let ctx = egui::Context::default();
        apply(&mut app, &ctx);
        assert_eq!(app.cam.up, UpAxis::Z);
        app.preferences.up_axis = "y".into();
        apply(&mut app, &ctx);
        assert_eq!((app.ui.up_axis, app.cam.up), (UpAxis::Y, UpAxis::Y));
        // Home looks at the model with Y up: the eye is above the XZ ground.
        assert!(app.cam.back().y > 0.3, "{:?}", app.cam.back());
        app.preferences.up_axis = "z".into();
        apply(&mut app, &ctx);
        assert_eq!(app.cam.up, UpAxis::Z);
    }
}

#[cfg(test)]
mod ui_scale_regression_tests {
    use super::*;
    use egui::{Event, Modifiers, PointerButton, pos2};

    #[test]
    fn desktop_zoom_shortcuts_update_and_persist_the_scale_preference() {
        let mut h = crate::scenario::Harness::new();
        h.call("ui.key", json!({"key":"Minus","cmd":true}));
        assert!((h.app.preferences.ui_scale - 0.9).abs() < 1e-4);
        assert!((h.ctx.zoom_factor() - 0.9).abs() < 1e-4);
        h.frames(12);
        assert!((h.ctx.zoom_factor() - 0.9).abs() < 1e-4, "must not flash back to the old preference");
        h.step(&json!({"restart":"close"})).unwrap();
        assert!((h.app.preferences.ui_scale - 0.9).abs() < 1e-4);
        h.call("ui.key", json!({"key":"Equals","cmd":true}));
        assert!((h.ctx.zoom_factor() - 1.0).abs() < 1e-4);
        h.call("ui.key", json!({"key":"Plus","cmd":true}));
        assert!((h.ctx.zoom_factor() - 1.1).abs() < 1e-4);
        h.call("ui.key", json!({"key":"0","cmd":true}));
        assert!((h.ctx.zoom_factor() - 1.0).abs() < 1e-4);
        h.call("ui.prefs", json!({"ui_scale":0.5}));
        h.call("ui.key", json!({"key":"Minus","cmd":true}));
        assert_eq!(h.app.preferences.ui_scale, 0.5);
        h.call("ui.prefs", json!({"ui_scale":2.5}));
        h.call("ui.key", json!({"key":"Plus","cmd":true}));
        assert_eq!(h.app.preferences.ui_scale, 2.5);
    }

    #[test]
    fn dragging_the_scale_slider_keeps_its_window_still_until_release() {
        let mut h = crate::scenario::Harness::new();
        h.call("ui.prefs", json!({"open":"General"}));
        let window = crate::scenario::rect_of("prefs:window").unwrap();
        let slider = crate::scenario::rect_of("prefs:scale").unwrap();
        let a = pos2(slider.left() + 20.0, slider.center().y);
        let b = pos2(slider.left() + 90.0, slider.center().y);
        h.app.synthetic.push(Event::PointerMoved(a));
        h.app.synthetic.push(Event::PointerButton { pos: a, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
        h.frames(3);
        h.app.synthetic.push(Event::PointerMoved(b));
        h.frames(4);
        assert_ne!(h.app.preferences.ui_scale, 1.0, "the slider changes its value during the drag");
        assert_eq!(h.ctx.zoom_factor(), 1.0, "layout must remain stable until release");
        assert_eq!(crate::scenario::rect_of("prefs:window").unwrap(), window);
        h.app.synthetic.push(Event::PointerButton { pos: b, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
        h.frames(5);
        assert!((h.ctx.zoom_factor() - h.app.preferences.ui_scale).abs() < 1e-4);
        h.call("ui.prefs", json!({"button":"cancel"}));
        assert_eq!(h.app.preferences.ui_scale, 1.0, "Cancel restores the original scale");
        assert_eq!(h.ctx.zoom_factor(), 1.0);
    }
}
