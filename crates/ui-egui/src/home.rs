//! The start page: New Design, Open, the recent designs and the built-in samples, each with a
//! picture of the design rendered from it. Shown on launch without a file and from the house
//! button or the first document tab; selecting a design returns to its view.

use crate::fonts::RichTextExt as _;
use std::collections::HashMap;

use egui::{Color32, CornerRadius, RichText, Sense, Stroke, vec2};
use solidcraft_engine::Session;
use solidcraft_engine::render::{Camera, StandardView};

use crate::SolveApp;
use crate::theme::Tokens;

/// Recent designs kept.
pub const MAX_RECENT: usize = 12;
const THUMB: [usize; 2] = solidcraft_engine::io::THUMBNAIL_SIZE;
const SAMPLE_PICTURES: [&[u8]; 6] = [
    include_bytes!("../../../assets/samples/exports/0.png"),
    include_bytes!("../../../assets/samples/exports/1.png"),
    include_bytes!("../../../assets/samples/exports/2.png"),
    include_bytes!("../../../assets/samples/exports/3.png"),
    include_bytes!("../../../assets/samples/exports/4.png"),
    include_bytes!("../../../assets/samples/exports/5.png"),
];

/// What a background job makes: a key ("sample:N" or a path), the picture, and for samples the
/// built design (opened at once when picked).
type Made = (String, Option<egui::ColorImage>, Option<Session>);

#[derive(Default)]
pub struct HomeState {
    pub open: bool,
    /// Recent design paths, newest first (kept in the preferences).
    pub recent: Vec<String>,
    pub opened_at: std::collections::BTreeMap<String, u64>,
    thumbs: HashMap<String, Option<egui::TextureHandle>>,
    samples: HashMap<usize, Session>,
    /// Keys asked for and not made yet.
    queued: Vec<String>,
    #[cfg(not(target_arch = "wasm32"))]
    rx: Option<std::sync::mpsc::Receiver<Made>>,
    /// Building a selected sample must not wait for a cold recent file.
    #[cfg(not(target_arch = "wasm32"))]
    sample_rx: Option<std::sync::mpsc::Receiver<Made>>,
    #[cfg(not(target_arch = "wasm32"))]
    remembering: Option<std::sync::mpsc::Receiver<Made>>,
    /// A sample picked before its design was ready.
    wanted: Option<usize>,
}

impl HomeState {
    pub fn ready_recents(&self) -> usize {
        self.recent.iter().filter(|p| self.thumbs.get(*p).is_some_and(Option::is_some)).count()
    }

    pub fn ready_samples(&self) -> usize {
        (0..solidcraft_engine::sample::samples().len()).filter(|i| self.thumbs.get(&format!("sample:{i}")).is_some_and(Option::is_some)).count()
    }

    pub fn show(&mut self, _rev: u64) {
        self.open = true;
        // Returning from another document cancels an earlier, unfinished sample selection.
        self.wanted = None;
    }

    /// Put a design at the top of the recent list.
    pub fn add_recent(&mut self, path: &str) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(MAX_RECENT);
        self.opened_at.insert(path.to_string(), web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0));
        self.opened_at.retain(|p, _| self.recent.contains(p));
        self.thumbs.remove(path);
    }
}

/// A picture of a design: iso view, fitted.
pub fn thumbnail(s: &Session) -> Option<egui::ColorImage> {
    let mut cam = Camera::default();
    cam.set_view(StandardView::Iso);
    let b = solidcraft_engine::view::bounds(s);
    if b.is_empty() {
        return None;
    }
    cam.fit(&b);
    // Fill the card: the fit leaves room for a whole window's margins.
    cam.distance *= 0.7;
    let scene = solidcraft_engine::view::scene(s, &cam);
    // Drawn on the GPU by the renderer the app installs; none (a browser): no picture.
    let picture = solidcraft_engine::capture::rgba(&scene, &cam, THUMB[0] as u32, THUMB[1] as u32).ok()?;
    Some(egui::ColorImage::from_rgba_unmultiplied(THUMB, &picture.rgba))
}

/// File identity, not just its path: saving or replacing a design invalidates its picture.
#[cfg(not(target_arch = "wasm32"))]
fn cache_path(key: &str) -> Option<std::path::PathBuf> {
    use std::hash::{Hash, Hasher};
    let meta = std::fs::metadata(key).ok()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut h);
    meta.len().hash(&mut h);
    meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.hash(&mut h);
    let root = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_CACHE_HOME"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    Some(root.join("SolidCraft").join("thumbnails").join(format!("{:016x}.png", h.finish())))
}

#[cfg(not(target_arch = "wasm32"))]
fn cached(key: &str) -> Option<egui::ColorImage> {
    let path = cache_path(key)?;
    if std::fs::metadata(&path).ok()?.len() > 512 * 1024 {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    decode_thumbnail(bytes)
}

fn decode_thumbnail(bytes: Vec<u8>) -> Option<egui::ColorImage> {
    if bytes.len() > 512 * 1024 {
        return None;
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(THUMB[0] as u32);
    limits.max_image_height = Some(THUMB[1] as u32);
    limits.max_alloc = Some((THUMB[0] * THUMB[1] * 8) as u64);
    reader.limits(limits);
    let image = reader.decode().ok()?.to_rgba8();
    (image.width() as usize == THUMB[0] && image.height() as usize == THUMB[1])
        .then(|| egui::ColorImage::from_rgba_unmultiplied(THUMB, image.as_raw()))
}

/// Imported STEP/mesh files open as unsaved designs, with no session path. Cache their
/// pictures under the source file identity too, so Home need not reimport them on each launch.
pub(crate) fn remember_file(s: &Session, key: &str) -> Option<egui::ColorImage> {
    #[cfg(not(target_arch = "wasm32"))]
    let identity = cache_path(key);
    remember_at(
        s,
        key,
        #[cfg(not(target_arch = "wasm32"))]
        identity,
    )
}

fn remember_at(s: &Session, key: &str, #[cfg(not(target_arch = "wasm32"))] identity: Option<std::path::PathBuf>) -> Option<egui::ColorImage> {
    #[cfg(target_arch = "wasm32")]
    let _ = key;
    let img = thumbnail(s)?;
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = identity.filter(|p| cache_path(key).as_ref() == Some(p)) {
        let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
        if let Some(bytes) = solidcraft_engine::render::png(THUMB[0], THUMB[1], rgba)
            && let Some(dir) = path.parent()
            && std::fs::create_dir_all(dir).is_ok()
        {
            let _ = solidcraft_engine::io::write_atomic(&path, &bytes, false);
        }
    }
    Some(img)
}

/// A Home picture is being drawn off the UI thread.
#[cfg(test)]
pub(crate) fn remembering(h: &HomeState) -> bool {
    h.remembering.is_some()
}

pub(crate) fn remember_session(h: &mut HomeState, s: &Session) {
    if let Some(path) = s.path.as_deref() {
        remember_source(h, s, path);
    }
}

/// Draw an opened file's picture after adoption, including imports with no save path.
pub(crate) fn remember_source(h: &mut HomeState, s: &Session, key: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let identity = cache_path(key);
        let snapshot = s.scratch();
        let source = key.to_owned();
        let (tx, rx) = std::sync::mpsc::channel();
        if std::thread::Builder::new()
            .name("saved-thumbnail".into())
            .spawn(move || {
                let img = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| remember_at(&snapshot, &source, identity))).ok().flatten();
                let _ = tx.send((source, img, None));
            })
            .is_ok()
        {
            h.remembering = Some(rx);
            h.queued.retain(|k| k != key);
            h.thumbs.insert(key.to_owned(), None);
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (h, s, key);
    }
}

pub(crate) fn install(h: &mut HomeState, ctx: &egui::Context, key: &str, img: Option<egui::ColorImage>) {
    let tex = img.map(|i| ctx.load_texture(format!("thumb:{key}"), i, egui::TextureOptions::LINEAR));
    h.thumbs.insert(key.to_string(), tex);
}

/// Read a disposable picture without evaluating, importing or meshing a model.
fn picture(key: &str) -> Option<egui::ColorImage> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(img) = cached(key) {
        return Some(img);
    }
    solidcraft_engine::io::read_design_thumbnail_file(key).and_then(decode_thumbnail)
}

/// A sample's shipped picture. Its model is built only when the sample is selected.
fn sample_picture(i: usize) -> Option<egui::ColorImage> {
    decode_thumbnail(SAMPLE_PICTURES.get(i)?.to_vec())
}

/// Build what a key stands for (runs off the UI thread where threads exist).
fn make(key: &str) -> Made {
    let sample = key.strip_prefix("sample:").and_then(|i| i.parse::<usize>().ok());
    if sample.is_none()
        && let Some(img) = picture(key)
    {
        return (key.to_string(), Some(img), None);
    }
    let mut s = Session::default();
    let built = match sample {
        Some(i) => solidcraft_engine::sample::samples().get(i).is_some_and(|smp| {
            let ok = s.run_script(&(smp.script)()).is_ok();
            s.doc_mut().name = smp.name.to_string();
            s.undo.clear();
            s.mark_saved();
            ok
        }),
        None => s.execute("doc.open", &serde_json::json!({ "path": key })).is_ok(),
    };
    if !built {
        return (key.to_string(), None, None);
    }
    if sample.is_some() {
        // Keep the first viewport frame warm even though the preview needs no new render.
        s.world_state().warm_meshes(|| true);
    }
    let img = match sample {
        Some(i) => sample_picture(i),
        None => remember_file(&s, key),
    };
    (key.to_string(), img, sample.map(|_| s))
}

/// Send every readable picture before starting any cold model work. A portable picture in a
/// later file must not wait for an earlier STEP import or legacy design rebuild.
#[cfg(not(target_arch = "wasm32"))]
fn read_pictures(keys: Vec<String>, tx: &std::sync::mpsc::Sender<Made>) -> Option<Vec<String>> {
    let mut cold = Vec::new();
    for key in keys {
        match std::panic::catch_unwind(|| picture(&key)).ok().flatten() {
            Some(img) => tx.send((key, Some(img), None)).ok()?,
            None => cold.push(key),
        }
    }
    Some(cold)
}

#[cfg(not(target_arch = "wasm32"))]
fn start_jobs(h: &mut HomeState) {
    if h.rx.is_some() || h.queued.is_empty() {
        return;
    }
    let mut keys = std::mem::take(&mut h.queued);
    let (tx, rx) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new().name("thumbnails".into()).spawn(move || {
        // Prefer local PNGs; even the metadata checks belong off the window thread.
        keys.sort_by_key(|k| !cache_path(k).is_some_and(|p| p.exists()));
        let Some(cold) = read_pictures(keys, &tx) else { return };
        for k in cold {
            let made = std::panic::catch_unwind(|| make(&k)).unwrap_or((k, None, None));
            if tx.send(made).is_err() {
                return;
            }
        }
    });
    if spawned.is_ok() {
        h.rx = Some(rx);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn start_sample(h: &mut HomeState) {
    let Some(i) = h.wanted.filter(|i| !h.samples.contains_key(i)) else { return };
    if h.sample_rx.is_some() {
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    if std::thread::Builder::new()
        .name("selected-sample".into())
        .spawn(move || {
            let key = format!("sample:{i}");
            let made = std::panic::catch_unwind(|| make(&key)).unwrap_or((key, None, None));
            let _ = tx.send(made);
        })
        .is_ok()
    {
        h.sample_rx = Some(rx);
    }
}

/// Take finished pictures and designs (one job per frame on the web, where there are no
/// threads).
fn poll(app: &mut SolveApp, ctx: &egui::Context) {
    let mut done: Vec<Made> = Vec::new();
    #[cfg(not(target_arch = "wasm32"))]
    let mut remembered = None;
    #[cfg(not(target_arch = "wasm32"))]
    {
        start_sample(&mut app.home);
        if let Some(rx) = &app.home.sample_rx {
            match rx.try_recv() {
                Ok(m) => {
                    done.push(m);
                    app.home.sample_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => app.home.sample_rx = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(16)),
            }
        }
        if let Some(rx) = &app.home.remembering {
            match rx.try_recv() {
                Ok(m) => {
                    remembered = Some(m);
                    app.home.remembering = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => app.home.remembering = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(16));
                }
            }
        }
        start_jobs(&mut app.home);
        if let Some(rx) = &app.home.rx {
            loop {
                match rx.try_recv() {
                    Ok(m) => {
                        // A cold job started before a save/open cannot overwrite its fresh picture.
                        if m.0.starts_with("sample:") || !app.home.thumbs.get(&m.0).is_some_and(Option::is_some) {
                            done.push(m);
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        app.home.rx = None;
                        break;
                    }
                }
            }
        }
        if let Some(m) = remembered {
            done.push(m);
        }
        if app.home.rx.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
    #[cfg(target_arch = "wasm32")]
    if let Some(i) = app.home.wanted {
        done.push(make(&format!("sample:{i}")));
        ctx.request_repaint();
    } else if !app.home.queued.is_empty() {
        let key = app.home.queued.remove(0);
        done.push(make(&key));
        ctx.request_repaint();
    }
    for (key, img, session) in done {
        if session.is_none() && key.starts_with("sample:") && app.home.wanted.is_some_and(|w| key == format!("sample:{w}")) {
            app.home.wanted = None;
            app.set_status(format!("the sample could not be built ({key})"), true);
        }
        // A failed sample build does not discard its shipped preview.
        if img.is_some() || !app.home.thumbs.contains_key(&key) {
            install(&mut app.home, ctx, &key, img);
        }
        if let (Some(s), Some(i)) = (session, key.strip_prefix("sample:").and_then(|i| i.parse::<usize>().ok())) {
            app.home.samples.insert(i, s);
        }
    }
    if let Some(i) = app.home.wanted
        && app.home.samples.contains_key(&i)
    {
        app.home.wanted = None;
        open_sample(app, i);
    } else if app.home.wanted.is_some() {
        // A selection can change while another sample finishes; schedule the next build.
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

/// Ask for a key's picture (once: an empty entry marks it as asked for).
fn want(h: &mut HomeState, key: &str) {
    if !h.thumbs.contains_key(key) {
        h.queued.push(key.to_string());
        h.thumbs.insert(key.to_string(), None);
    }
}

/// Open a built-in sample (a copy of the design made for its picture).
pub fn open_sample(app: &mut SolveApp, i: usize) {
    match app.home.samples.get(&i) {
        Some(s) => {
            let mut copy = s.scratch();
            copy.mark_saved();
            crate::documents::adopt(app, copy);
            app.home.open = false;
        }
        None if i < solidcraft_engine::sample::samples().len() => {
            app.home.open = true;
            app.home.wanted = Some(i);
        }
        None => {}
    }
}

/// Is this recent design there to open: in memory, or something the host can bring back?
pub fn available(app: &SolveApp, path: &str) -> bool {
    solidcraft_engine::io::vfs::exists(path) || app.services.can_restore.as_ref().is_some_and(|can| can(path))
}

pub fn open_recent(app: &mut SolveApp, path: &str) {
    if !solidcraft_engine::io::vfs::exists(path) {
        if let Some(restore) = app.services.restore_file.as_ref().filter(|_| available(app, path)) {
            restore(path);
            return;
        }
        app.set_status(format!("{path} is no longer there"), true);
        return;
    }
    app.home.open = false;
    app.open_path_async(path);
}

/// The page itself (in place of the toolbar, browser and viewport).
pub fn show(app: &mut SolveApp, ui: &mut egui::Ui) {
    poll(app, ui.ctx());
    let t = Tokens::get();
    let samples = solidcraft_engine::sample::samples();
    // Embedded previews are available on the first frame, including on the web. No model
    // scripts, filesystem reads or GPU readbacks are needed to show the sample cards.
    for i in 0..samples.len() {
        let key = format!("sample:{i}");
        if !app.home.thumbs.contains_key(&key) {
            install(&mut app.home, ui.ctx(), &key, sample_picture(i));
        }
    }
    let recent = app.home.recent.clone();
    for p in recent.iter().filter(|p| solidcraft_engine::io::vfs::exists(p)) {
        want(&mut app.home, p);
    }
    let mut action: Option<Box<dyn FnOnce(&mut SolveApp)>> = None;
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::symmetric(36, 24))).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("SolidCraft").size(crate::theme::Tokens::get().font_display).semibold().color(t.text));
                ui.add_space(t.space_12);
                ui.label(RichText::new(crate::i18n::tr_label("Start")).size(crate::theme::Tokens::get().font_intro).color(t.text_dim));
            });
            ui.add_space(t.space_16);
            ui.horizontal(|ui| {
                let big = |ui: &mut egui::Ui, icon: &str, label: &str, primary: bool| -> bool {
                    // The label sets the width: a translation longer than the English one widens
                    // the button instead of running past its edge.
                    let ink_text = if primary { t.on_accent } else { t.text };
                    let galley = ui.painter().layout_no_wrap(
                        label.to_owned(),
                        egui::FontId::proportional(crate::theme::Tokens::get().font_subtitle),
                        ink_text,
                    );
                    let width = (56.0 + galley.size().x + 20.0).max(170.0);
                    let (r, resp) = ui.allocate_exact_size(vec2(width, 64.0), Sense::click());
                    let fill = if primary {
                        t.accent
                    } else if resp.hovered() {
                        t.hover
                    } else {
                        t.field
                    };
                    ui.painter().rect(r, t.radius_md, fill, Stroke::new(1.0, if primary { t.accent } else { t.border }), egui::StrokeKind::Inside);
                    let ink = if primary { t.on_accent } else { t.icon };
                    crate::icons::paint(
                        ui.painter(),
                        egui::Rect::from_center_size(r.left_center() + vec2(30.0, 0.0), vec2(30.0, 30.0)),
                        icon,
                        ink,
                        t.icon_fill,
                        ink,
                    );
                    ui.painter().galley(r.left_center() + vec2(56.0, -galley.size().y / 2.0), galley, ink_text);
                    resp.clicked()
                };
                if big(ui, "new", app.tr("ui.new_design"), true) {
                    action = Some(Box::new(|app: &mut SolveApp| crate::documents::new_design(app)));
                }
                ui.add_space(t.space_10);
                if big(ui, "open", app.tr("home.open"), false) {
                    action = Some(Box::new(|app: &mut SolveApp| {
                        app.pick_open(|app, path| {
                            if let Some(path) = path {
                                app.home.open = false;
                                app.open_path_async(&path);
                            }
                        });
                    }));
                }
            });
            ui.add_space(t.space_24);
            ui.horizontal_wrapped(|ui| {
                ui.hyperlink_to(app.tr("home.documentation"), crate::help::DOCS_URL);
                ui.hyperlink_to(app.tr("home.report_issue"), "https://github.com/tangency-labs/solidcraft/issues/new");
                ui.hyperlink_to(app.tr("home.whats_new"), "https://github.com/bherbruck/solidcraft/releases/latest");
                if ui.link(app.tr("ui.keyboard_shortcuts")).clicked() {
                    app.keymap.open = true;
                    app.keymap.read_only = true;
                }
                if ui.link(app.tr("ui.preferences")).clicked() {
                    crate::prefs::open_at(app, 0);
                }
            });
            ui.add_space(t.space_16);
            section(ui, app.tr("home.recent"));
            if recent.is_empty() {
                ui.label(RichText::new(crate::i18n::tr_label("Designs you open or save appear here.")).color(t.text_dim));
            } else {
                grid(ui, recent.len(), |ui, i| {
                    let Some(p) = recent.get(i) else { return };
                    let path = std::path::Path::new(p);
                    let name = path.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| p.clone());
                    let missing = !available(app, p);
                    let mut sub = if missing {
                        app.tr("home.missing").to_string()
                    } else if p.starts_with("/disk/") {
                        // The web build remembers files it saved on the computer.
                        crate::i18n::tr_label("On your computer").to_string()
                    } else if p.starts_with("/opfs/") {
                        // The web build keeps designs in the browser's own storage.
                        app.tr("home.in_this_browser").to_string()
                    } else {
                        p.clone()
                    };
                    if let Some(time) = app.home.opened_at.get(p) {
                        let now = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(*time);
                        let seconds = now.saturating_sub(*time);
                        let opened = if seconds < 60 {
                            app.tr("home.opened_just_now").to_string()
                        } else if seconds < 3600 {
                            crate::i18n::format("home.opened_minutes_ago", &[("count", &(seconds / 60).to_string())])
                        } else if seconds < 86400 {
                            crate::i18n::format("home.opened_hours_ago", &[("count", &(seconds / 3600).to_string())])
                        } else {
                            crate::i18n::format("home.opened_days_ago", &[("count", &(seconds / 86400).to_string())])
                        };
                        sub.push('\n');
                        sub.push_str(&opened);
                    }
                    let response = card(ui, Pic::Ready(app.home.thumbs.get(p.as_str()).and_then(Option::as_ref)), &name, &sub, missing)
                        .on_hover_text(p.as_str());
                    crate::scenario::publish_handle(&format!("home:recent:{i}"), response.rect.center());
                    response.context_menu(|ui| {
                        if crate::context_menu::button(ui, crate::i18n::tr_label("Remove from list"), "delete").clicked() {
                            app.home.recent.retain(|x| x != p);
                            app.home.opened_at.remove(p);
                            ui.close();
                        }
                        if !cfg!(target_arch = "wasm32")
                            && crate::context_menu::button(ui, crate::i18n::tr_label("Show in folder"), "folder").clicked()
                        {
                            if let Some(show) = &app.services.show_in_folder {
                                show(p);
                            }
                            ui.close();
                        }
                    });
                    if response.clicked() {
                        let p = p.clone();
                        action = Some(Box::new(move |app: &mut SolveApp| open_recent(app, &p)));
                    }
                });
            }
            ui.add_space(t.space_24);
            section(ui, app.tr("home.samples"));
            grid(ui, samples.len(), |ui, i| {
                let Some(s) = samples.get(i) else { return };
                let key = format!("sample:{i}");
                let busy = app.home.wanted == Some(i);
                // The engine's sample table holds English text; the opened design keeps it as its name.
                let about = app.tr_label(s.about);
                let sub = if busy { app.tr("home.opening") } else { about };
                let tex = match app.home.thumbs.get(&key) {
                    Some(t) => Pic::Ready(t.as_ref()),
                    None => Pic::Icon("box"),
                };
                if card(ui, tex, app.tr_label(s.name), sub, false).on_hover_text(about).clicked() {
                    action = Some(Box::new(move |app: &mut SolveApp| open_sample(app, i)));
                }
            });
        });
    });
    if let Some(f) = action {
        f(app);
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    let t = Tokens::get();
    ui.label(RichText::new(title).size(crate::theme::Tokens::get().font_subtitle).semibold().color(t.text));
    ui.add_space(t.space_8);
}

/// Cards in rows that fill the width.
fn grid(ui: &mut egui::Ui, n: usize, mut each: impl FnMut(&mut egui::Ui, usize)) {
    let t = Tokens::get();
    let w = THUMB[0] as f32 + 16.0;
    let per_row = ((ui.available_width() + t.space_14) / (w + t.space_14)).floor().max(1.0) as usize;
    for row in 0..n.div_ceil(per_row) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = t.space_14;
            for i in row * per_row..((row + 1) * per_row).min(n) {
                each(ui, i);
            }
        });
        ui.add_space(t.space_14);
    }
}

/// A card's picture: rendered (`None` while it renders), or an icon until it is asked for.
enum Pic<'a> {
    Ready(Option<&'a egui::TextureHandle>),
    Icon(&'static str),
}

/// A design card: its picture (a spinner while it renders), name and a line under it.
fn card(ui: &mut egui::Ui, pic: Pic, name: &str, sub: &str, dim: bool) -> egui::Response {
    let t = Tokens::get();
    let size = vec2(THUMB[0] as f32 + 16.0, THUMB[1] as f32 + 80.0);
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    let hover = resp.hovered();
    ui.painter().rect(
        r,
        t.radius_md,
        if hover { t.hover } else { t.field },
        Stroke::new(1.0, if hover { t.accent } else { t.border }),
        egui::StrokeKind::Inside,
    );
    let img = egui::Rect::from_min_size(r.min + vec2(8.0, 8.0), vec2(THUMB[0] as f32, THUMB[1] as f32));
    match pic {
        Pic::Icon(icon) => {
            ui.painter().rect_filled(img, CornerRadius::same(t.radius_sm), t.panel_header);
            crate::icons::paint(ui.painter(), egui::Rect::from_center_size(img.center(), vec2(56.0, 56.0)), icon, t.icon, t.icon_fill, t.icon);
        }
        Pic::Ready(Some(tx)) => {
            let tint = if dim { Color32::from_white_alpha(90) } else { Color32::WHITE };
            ui.painter().image(tx.id(), img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), tint);
        }
        Pic::Ready(None) => {
            ui.painter().rect_filled(img, CornerRadius::same(t.radius_sm), t.panel_header);
            if !dim {
                egui::Spinner::new().size(18.0).paint_at(ui, egui::Rect::from_center_size(img.center(), vec2(18.0, 18.0)));
            }
        }
    }
    let ty = img.bottom() + 8.0;
    let clip = egui::Rect::from_min_max(egui::pos2(r.left() + 8.0, ty), egui::pos2(r.right() - 8.0, r.bottom()));
    let p = ui.painter().with_clip_rect(clip);
    p.text(
        egui::pos2(r.left() + 10.0, ty),
        egui::Align2::LEFT_TOP,
        name,
        egui::FontId::proportional(t.font_heading),
        if dim { t.text_dim } else { t.text },
    );
    p.text(egui::pos2(r.left() + 10.0, ty + 20.0), egui::Align2::LEFT_TOP, sub, egui::FontId::proportional(t.font_caption), t.text_dim);
    if hover {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_sample_pictures_are_bounded_and_match_the_current_scripts() {
        use sha2::{Digest, Sha256};
        let hashes: Vec<String> = serde_json::from_str(include_str!("../../../assets/samples/exports/scripts.json")).unwrap();
        let samples = solidcraft_engine::sample::samples();
        assert_eq!(samples.len(), SAMPLE_PICTURES.len());
        assert_eq!(hashes.len(), samples.len());
        for (i, sample) in samples.iter().enumerate() {
            let script = serde_json::to_vec(&(sample.script)()).unwrap();
            assert_eq!(hashes.get(i).unwrap(), &format!("{:x}", Sha256::digest(script)), "regenerate the preview for {}", sample.name);
            assert_eq!(sample_picture(i).unwrap().size, THUMB);
        }
        assert!(sample_picture(samples.len()).is_none());
    }

    /// A recent design the host can bring back (the browser build's remembered files) is not
    /// "missing": choosing it asks the host to restore it, and one the host doesn't know still is.
    #[test]
    fn a_recent_design_the_host_can_restore_is_asked_for_not_reported_missing() {
        let mut h = crate::scenario::Harness::new();
        let asked = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let sink = asked.clone();
        h.app.services.can_restore = Some(Box::new(|p| p.starts_with("/disk/known/")));
        h.app.services.restore_file = Some(Box::new(move |p| sink.borrow_mut().push(p.to_string())));
        assert!(available(&h.app, "/disk/known/Bracket.solidcraft"));
        assert!(!available(&h.app, "/disk/other/Bracket.solidcraft"));
        open_recent(&mut h.app, "/disk/known/Bracket.solidcraft");
        assert_eq!(*asked.borrow(), ["/disk/known/Bracket.solidcraft"]);
        open_recent(&mut h.app, "/disk/other/Bracket.solidcraft");
        assert_eq!(asked.borrow().len(), 1, "a file the host doesn't remember is not asked for");
        assert!(h.app.status.as_ref().is_some_and(|(text, _, error)| *error && text.contains("no longer there")));
    }

    /// Rebuild the shipped pictures from the authoritative sample command scripts.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore = "asset authoring: writes assets/samples/exports"]
    fn regenerate_sample_thumbnails() {
        use sha2::{Digest, Sha256};
        assert!(crate::gpu::test_picture_renderer());
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/samples/exports");
        std::fs::create_dir_all(&dir).unwrap();
        let mut hashes = Vec::new();
        for (i, sample) in solidcraft_engine::sample::samples().into_iter().enumerate() {
            let began = std::time::Instant::now();
            let script = (sample.script)();
            let mut session = Session::default();
            session.run_script(&script).unwrap();
            let image = thumbnail(&session).unwrap();
            let rgba = image.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
            let png = solidcraft_engine::render::png(THUMB[0], THUMB[1], rgba).unwrap();
            std::fs::write(dir.join(format!("{i}.png")), png).unwrap();
            hashes.push(format!("{:x}", Sha256::digest(serde_json::to_vec(&script).unwrap())));
            eprintln!("{}: {:.3} s", sample.name, began.elapsed().as_secs_f64());
        }
        std::fs::write(dir.join("scripts.json"), serde_json::to_vec_pretty(&hashes).unwrap()).unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn cached_recent_uses_a_separate_png_without_reading_or_changing_design_json() {
        if !crate::gpu::test_picture_renderer() {
            return;
        }
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("solidcraft-thumb-{}-{nonce}.solidcraft", std::process::id()));
        // Deliberately invalid design: a cached recent must not parse/evaluate it.
        let bytes = b"{\"thumbnail_test\":true}";
        std::fs::write(&path, bytes).unwrap();
        let key = path.to_string_lossy().to_string();
        let mut s = Session::default();
        s.execute("solid.box", &serde_json::json!({"length":10,"width":10,"height":10})).unwrap();
        s.path = Some(key.clone());
        let image = remember_file(&s, &key).unwrap();
        let png = cache_path(&key).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let (_, cached_image, session) = make(&key);
        assert!(session.is_none());
        assert_eq!(cached_image.unwrap().pixels, image.pixels);
        std::fs::write(&path, b"changed file identity").unwrap();
        assert!(cached(&key).is_none());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(png).unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn portable_thumbnail_loads_without_a_local_picture_or_model_rebuild() {
        if !crate::gpu::test_picture_renderer() {
            return;
        }
        let mut session = Session::default();
        session.execute("solid.box", &serde_json::json!({"length":10,"width":20,"height":30})).unwrap();
        for body in &session.world_state().bodies {
            body.mesh();
        }
        let bytes = session.design_container(solidcraft_engine::io::DesignCompression::Fast, false, true).unwrap();
        let png = solidcraft_engine::io::read_design_thumbnail(&bytes).unwrap();
        let image = decode_thumbnail(png).unwrap();
        assert_eq!(image.size, THUMB);
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("solidcraft-portable-thumb-{}-{nonce}.solidcraft", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        let key = path.to_string_lossy().to_string();
        assert!(cached(&key).is_none());
        let (_, picture, rebuilt) = make(&key);
        assert!(rebuilt.is_none());
        assert_eq!(picture.unwrap().pixels, image.pixels);
        assert!(!cache_path(&key).unwrap().exists(), "fallback rebuilding would create a local picture");
        std::fs::remove_file(path).unwrap();
        assert!(decode_thumbnail(vec![0; 600_000]).is_none());
        assert!(decode_thumbnail(vec![0; 24]).is_none());
    }

    /// STEP opens deliberately have no save path, but their source still gets a cache entry.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn imported_step_picture_is_cached_under_its_source_without_a_save_path() {
        if !crate::gpu::test_picture_renderer() {
            return;
        }
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("solidcraft-import-thumb-{}-{nonce}.step", std::process::id()));
        let key = path.to_string_lossy().into_owned();
        let mut part = Session::default();
        part.execute("solid.box", &serde_json::json!({"length":10,"width":20,"height":30})).unwrap();
        part.execute("file.export", &serde_json::json!({"path":key})).unwrap();
        let original = std::fs::read(&path).unwrap();
        let mut opened = Session::default();
        opened.execute("doc.open", &serde_json::json!({"path":key})).unwrap();
        assert!(opened.path.is_none(), "an imported file is an unsaved design");
        let mut home = HomeState::default();
        remember_source(&mut home, &opened, &key);
        let (made_key, image, rebuilt) = home.remembering.take().unwrap().recv_timeout(std::time::Duration::from_secs(30)).unwrap();
        assert_eq!(made_key, key);
        assert!(rebuilt.is_none());
        let image = image.unwrap();
        assert!(opened.path.is_none(), "caching must not turn Save into an overwrite of the STEP source");
        let png = cache_path(&key).unwrap();
        assert_eq!(cached(&key).unwrap().pixels, image.pixels);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let (_, picture, rebuilt) = make(&key);
        assert!(rebuilt.is_none());
        assert_eq!(picture.unwrap().pixels, image.pixels);
        std::fs::write(&path, b"file replaced").unwrap();
        assert!(cached(&key).is_none());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(png).unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn portable_pictures_are_sent_before_any_cold_recent_is_built() {
        if !crate::gpu::test_picture_renderer() {
            return;
        }
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("solidcraft-fast-thumb-{}-{nonce}.solidcraft", std::process::id()));
        let key = path.to_string_lossy().into_owned();
        let mut s = Session::default();
        s.execute("solid.box", &serde_json::json!({"length":10,"width":20,"height":30})).unwrap();
        for b in &s.world_state().bodies {
            b.mesh();
        }
        let bytes = s.design_container(solidcraft_engine::io::DesignCompression::Fast, false, true).unwrap();
        std::fs::write(&path, bytes).unwrap();
        let cold = format!("missing-cold-file-{nonce}.step");
        let (tx, rx) = std::sync::mpsc::channel();
        let misses = read_pictures(vec![cold.clone(), key.clone()], &tx).unwrap();
        assert_eq!(misses, vec![cold]);
        let (ready, picture, rebuilt) = rx.try_recv().unwrap();
        assert_eq!(ready, key);
        assert!(picture.is_some() && rebuilt.is_none());
        assert!(cached(&key).is_none(), "the portable picture did not rebuild the design");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn first_home_frame_shows_all_samples_without_building_any_models() {
        let mut h = crate::scenario::Harness::new();
        h.app.home.show(0);
        h.frames(1);
        assert_eq!(h.app.home.ready_samples(), solidcraft_engine::sample::samples().len());
        assert!(h.app.home.samples.is_empty());
        assert!(h.app.home.queued.is_empty());
        #[cfg(not(target_arch = "wasm32"))]
        assert!(h.app.home.sample_rx.is_none() && h.app.home.rx.is_none());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn selecting_a_sample_does_not_wait_for_a_running_cold_recent() {
        let mut app = SolveApp::new(Session::default(), Default::default());
        let (_blocked_worker, rx) = std::sync::mpsc::channel();
        app.home.rx = Some(rx);
        app.home.open = false;
        let ctx = egui::Context::default();
        open_sample(&mut app, 1);
        assert!(app.home.open, "a programmatic selection from a document must start the sample job too");
        let began = std::time::Instant::now();
        while app.home.open && began.elapsed() < std::time::Duration::from_secs(30) {
            poll(&mut app, &ctx);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!app.home.open, "the recent worker is still blocked, but the sample must open");
        assert!(app.home.rx.is_some());
        assert_eq!(app.session.doc.name, "Sheet Metal Bracket");
        assert!(!app.session.is_dirty());
    }

    #[test]
    fn recent_list_is_newest_first_and_bounded() {
        let mut h = HomeState::default();
        for i in 0..20 {
            h.add_recent(&format!("/d/{i}.solidcraft"));
        }
        h.add_recent("/d/5.solidcraft");
        assert_eq!(h.recent.len(), MAX_RECENT);
        assert_eq!(h.recent.first().map(String::as_str), Some("/d/5.solidcraft"));
        assert_eq!(h.recent.iter().filter(|p| p.as_str() == "/d/5.solidcraft").count(), 1);
    }

    /// Samples build in the background and make a picture; picking one opens its design.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sample_card_makes_a_picture_and_opens() {
        if !crate::gpu::test_picture_renderer() {
            return;
        }
        let (key, img, session) = make("sample:0");
        assert_eq!(key, "sample:0");
        let img = img.unwrap();
        assert_eq!(img.size, THUMB);
        let s = session.unwrap();
        assert_eq!(s.doc.name, "Sample Plate");
        assert!(!s.is_dirty() && s.undo.is_empty());
        let mut app = SolveApp::new(Session::default(), crate::Services::default());
        app.home.samples.insert(0, s);
        app.home.show(app.session.revision);
        open_sample(&mut app, 0);
        assert!(!app.home.open);
        assert_eq!(app.session.doc.name, "Sample Plate");
        assert!(!app.session.is_dirty());
    }

    /// The gear, Ctrl+, and Home's own links open Preferences over Home, not after leaving it.
    #[test]
    fn preferences_and_shortcuts_open_over_home() {
        let mut h = crate::scenario::Harness::new();
        let rev = h.app.session.revision;
        h.app.home.show(rev);
        h.frames(2);
        assert!(h.app.home.open);
        let gear = crate::scenario::handle_at("titlebar:preferences").unwrap();
        h.call("ui.click", serde_json::json!({"x": gear.x, "y": gear.y}));
        assert!(h.app.prefs_window.open && h.app.home.open);
        let shown = |h: &crate::scenario::Harness| {
            h.ctx.memory(|m| m.areas().visible_last_frame(&egui::LayerId::new(egui::Order::Middle, egui::Id::new("sc_prefs"))))
        };
        assert!(shown(&h), "the Preferences window must be drawn on Home");
        h.call("ui.prefs", serde_json::json!({"button": "cancel"}));
        assert!(!shown(&h));

        // Home › Keyboard Shortcuts opens the same window at its page.
        h.app.keymap.open = true;
        h.frames(2);
        assert!(shown(&h) && h.app.prefs_window.section == crate::prefs::PAGE_SHORTCUTS && h.app.home.open);
    }
}
