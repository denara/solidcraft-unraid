//! Localization must not change scenario handles or leave the next app in another locale.

use crate::scenario::{self, Harness};
use serde_json::json;

#[test]
fn japanese_dialog_keeps_english_scenario_handles() {
    let mut h = Harness::new();
    h.app.preferences.language = "ja".into();
    h.app.home.open = false;
    h.call("ui.start", json!({"command":"sketch.create"}));
    h.frames(3);
    assert!(h.widget("平面").is_some(), "the input label must be translated");
    assert!(h.widget("キャンセル").is_some(), "the close button must be translated");
    assert!(!scenario::handles("dialog:Plane").is_empty(), "scenario handles must retain source labels");
    assert_eq!(crate::i18n::tr("ui.preferences"), "Preferences", "the render scope must restore English");

    h.app.preferences.language = "en".into();
    h.frames(3);
    assert!(h.widget("Plane").is_some());
    assert!(h.widget("Cancel").is_some());
}

#[test]
fn sketch_palette_profile_and_points_rows_show_their_own_translation() {
    // ui.palette_profile / ui.palette_points are reached by id, not by the usual English-text
    // lookup (see the comment on sketch_palette::TOGGLES, and
    // i18n::tests::id_labels_resolve_to_their_own_translation_not_an_english_fallback for the
    // lookup itself). A broken id falls back to English silently rather than failing loudly, so
    // check the word actually on screen in each locale, not just that something renders.
    for (lang, profile, points) in [("de", "Profil", "Punkte"), ("ja", "プロファイル", "点"), ("ko", "프로파일", "점")] {
        let mut h = Harness::new();
        h.app.preferences.language = lang.into();
        h.app.home.open = false;
        h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
        h.frames(3);
        assert!(h.widget(profile).is_some(), "{lang}: the Profile row must show {profile:?}");
        assert!(h.widget(points).is_some(), "{lang}: the Points row must show {points:?}");
        assert!(h.widget("Profile").is_none(), "{lang}: must not silently fall back to English");
        assert!(h.widget("Points").is_none(), "{lang}: must not silently fall back to English");
    }
}

#[test]
fn language_preference_cancel_restores_previous_locale() {
    let mut h = Harness::new();
    h.app.home.open = false;
    h.call("ui.prefs", json!({"open":"General"}));
    h.call("ui.prefs", json!({"language":"ja"}));
    assert!(h.widget("言語").is_some());
    assert_eq!(h.app.preferences.language, "ja");
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert_eq!(h.app.preferences.language, "en");
    assert_eq!(h.app.tr("ui.preferences"), "Preferences");
}

/// The chrome a user sees before touching anything: the browser heading and the SELECT panel.
/// Both were English literals in the drawing code, and a broken message id falls back to
/// English silently, so check the word actually rendered in each language.
#[test]
fn window_chrome_renders_in_the_active_language() {
    for (lang, browser, filter, faces) in [
        ("zh", "浏览器", "选择筛选", "面"),
        ("ja", "ブラウザ", "選択フィルター", "面"),
        ("de", "BROWSER", "Auswahlfilter", "Flächen"),
        ("ko", "브라우저", "선택 필터", "면"),
        ("ru", "БРАУЗЕР", "Фильтр выделения", "Грани"),
    ] {
        let mut h = Harness::new();
        h.app.preferences.language = lang.into();
        h.app.home.open = false;
        h.frames(2);
        // The browser heading is on screen from the start; the selection filter lives in the
        // SELECT panel, which is not the one shown by default.
        assert!(h.widget(browser).is_some(), "{lang}: the browser heading must show {browser:?}");
        let select = scenario::handle_at("panel:SOLID:SELECT").expect("the SELECT panel is on the SOLID tab");
        h.step(&json!({"click": [select.x, select.y]})).unwrap();
        h.frames(2);
        assert!(h.widget(filter).is_some(), "{lang}: the selection filter must show {filter:?}");
        assert!(h.widget(faces).is_some(), "{lang}: the Faces checkbox must show {faces:?}");
    }
}

/// The command palette's hint embeds a JSON example, so its braces must not be mistaken for a
/// placeholder name by the catalog's parser, and the example must survive interpolation whole.
#[test]
fn command_palette_hint_keeps_its_json_example_intact() {
    let mut h = Harness::new();
    h.app.preferences.language = "zh".into();
    h.app.home.open = false;
    h.frames(2);
    let hint = crate::i18n::format("ui.command_palette_hint", &[("example", "Extrude {\"distance\": 20}")]);
    assert!(hint.contains("Extrude {\"distance\": 20}"), "the example is inserted literally: {hint:?}");
    assert!(!hint.contains("{example}"), "the placeholder is substituted: {hint:?}");
}

#[test]
fn translated_dialog_buttons_do_not_wrap() {
    fn cancel_rows(shape: &egui::Shape) -> Option<usize> {
        match shape {
            egui::Shape::Text(text) if text.galley.job.text == "キャンセル" => Some(text.galley.rows.len()),
            egui::Shape::Vec(shapes) => shapes.iter().find_map(cancel_rows),
            _ => None,
        }
    }
    let mut h = Harness::new();
    crate::fonts::setup_fonts(&h.ctx);
    h.app.preferences.language = "ja".into();
    h.app.home.open = false;
    h.call("ui.start", json!({"command":"solid.extrude"}));
    let mut out = h.ctx.run_ui(
        egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0))), ..Default::default() },
        |ui| h.app.ui(ui),
    );
    assert_eq!(out.shapes.iter().find_map(|shape| cancel_rows(&shape.shape)), Some(1));
    out.textures_delta.clear();
}

#[test]
fn long_panel_label_wraps_centred_and_its_button_is_centred() {
    fn label_rows(shape: &egui::Shape) -> Option<Vec<egui::Rect>> {
        match shape {
            egui::Shape::Text(text) if text.galley.job.text == "SKIZZE BEENDEN" => {
                Some(text.galley.rows.iter().map(|row| row.rect().translate(text.pos.to_vec2())).collect())
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(label_rows),
            _ => None,
        }
    }
    fn hover_rect(shape: &egui::Shape, hover: egui::Color32, at: egui::Pos2) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Rect(r) if r.fill == hover && r.rect.contains(at) => Some(r.rect),
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| hover_rect(s, hover, at)),
            _ => None,
        }
    }
    let mut h = Harness::new();
    h.app.preferences.language = "de".into();
    h.app.home.open = false;
    h.app.run("sketch.create", json!({"plane":"xy"})).unwrap();
    h.frames(2);
    let label = scenario::handles("panel:SKETCH:FINISH SKETCH")[0].1;
    let button = scenario::handles("button:sketch.finish")[0].1;
    assert!((label.x - button.x).abs() < 1.0, "the only button sits centred over its label: {label:?} {button:?}");

    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0));
    let mut out =
        h.ctx.run_ui(egui::RawInput { screen_rect: Some(screen), events: vec![egui::Event::PointerMoved(label)], ..Default::default() }, |ui| {
            h.app.ui(ui)
        });
    out.textures_delta.clear();
    let mut out = h.ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| h.app.ui(ui));
    let rows = out.shapes.iter().find_map(|shape| label_rows(&shape.shape)).expect("the Finish Sketch panel label is drawn");
    assert_eq!(rows.len(), 2, "the label wider than its one-button panel wraps: {rows:?}");
    assert!(rows[0].width() != rows[1].width(), "the lines differ in width: {rows:?}");
    assert!((rows[0].center().x - rows[1].center().x).abs() < 1.0, "each line is centred: {rows:?}");
    let hover = crate::theme::Tokens::get().hover;
    let lr = out.shapes.iter().find_map(|shape| hover_rect(&shape.shape, hover, label)).expect("the hovered label is highlighted");
    assert!(rows.iter().all(|row| lr.contains_rect(*row)), "the highlight holds both lines: {lr:?} {rows:?}");
    out.textures_delta.clear();
}
