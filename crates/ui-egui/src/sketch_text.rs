//! Edit Text: the shared feature dialog (OK/Cancel, expression fields) for the source of a
//! sketch text's outlines; OK runs `sketch.edit_text`.
use serde_json::{Value, json};
use solidcraft_engine::{
    Sel,
    doc::expr::Kind as ValueKind,
    sketch::{LinkKind, LinkSource},
};

use crate::SolveApp;
use crate::dialogs::{Dialog, Kind};
use crate::selection::SelInput;

/// Field rows after the text: position, height (lengths) and angle.
const LABELS: [&str; 4] = ["X", "Y", "Height", "Angle"];
const KINDS: [ValueKind; 4] = [ValueKind::Length, ValueKind::Length, ValueKind::Length, ValueKind::Angle];

/// The Edit Text dialog's state: the text being edited and its four values as expressions.
#[derive(Clone, Debug)]
pub struct Tx {
    sketch: u64,
    pub(crate) link: String,
    pub(crate) text: String,
    initial_text: String,
    /// The font as typed: an installed font's family name or a font file's path; empty is the built-in font.
    pub(crate) font: String,
    initial_font: String,
    values: [String; 4],
    /// The fields as first shown; an untouched field keeps its exact saved value.
    initial: [String; 4],
    /// The saved source: x, y, height (mm) and angle (rad).
    original: [f64; 4],
    pub(crate) unit: &'static str,
    focus: bool,
}

/// Only a complete selection belonging to one text source can seed the editor.
pub(crate) fn selected_entity(app: &SolveApp) -> Option<String> {
    let sk = app.session.doc.sketch(app.session.active_sketch?).ok()?;
    let mut selected = None;
    let mut link = None;
    for selection in &app.session.selection {
        let Sel::SketchCurve { id } = selection else { return None };
        let curve = sk.curves.iter().find(|curve| &curve.id == id)?;
        let source = sk.link(curve.link.as_deref()?)?;
        if source.kind != LinkKind::Text || link.is_some_and(|old| old != source.id.as_str()) {
            return None;
        }
        link = Some(source.id.as_str());
        selected = Some(id.clone());
    }
    selected
}

/// A value shown in a field: rounded so binary noise does not show, with its unit.
fn shown(value: f64, unit: &str) -> String {
    let text = format!("{value:.6}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    let text = if text == "-0" { "0" } else { text };
    format!("{text} {unit}")
}

/// The saved text source behind `link` in the active sketch: its text, its font (empty: the built-in
/// font), and x, y, height and angle.
fn source(app: &SolveApp, sketch: u64, link: &str) -> Option<(String, String, [f64; 4])> {
    if app.session.active_sketch != Some(sketch) {
        return None;
    }
    let sk = app.session.doc.sketch(sketch).ok()?;
    let source = sk.link(link).filter(|source| source.kind == LinkKind::Text)?;
    let LinkSource::Text { text, at, height, angle, font } = &source.source else { return None };
    Some((text.clone(), font.clone().unwrap_or_default(), [at.x, at.y, *height, *angle]))
}

fn fields(original: &[f64; 4], unit: &'static str, scale: f64) -> [String; 4] {
    std::array::from_fn(|i| {
        let value = original.get(i).copied().unwrap_or_default();
        if i == 3 { shown(value.to_degrees(), "deg") } else { shown(value * scale, unit) }
    })
}

/// The dialog's kind for the text that `entity` (an outline curve) belongs to.
fn kind_for(app: &SolveApp, entity: &str) -> Option<Kind> {
    let sketch = app.session.active_sketch?;
    let sk = app.session.doc.sketch(sketch).ok()?;
    let link = sk.curves.iter().find(|curve| curve.id == entity).and_then(|curve| curve.link.clone())?;
    let (text, font, original) = source(app, sketch, &link)?;
    let (scale, unit) = crate::prefs::unit_scale(&app.session.doc.units);
    let values = fields(&original, unit, scale);
    Some(Kind::Text(Tx {
        sketch,
        link,
        initial_text: text.clone(),
        text,
        initial_font: font.clone(),
        font,
        initial: values.clone(),
        values,
        original,
        unit,
        focus: true,
    }))
}

/// `Dialog::for_command`: Edit Text with one text's outlines selected.
pub(crate) fn start(app: &SolveApp, id: &str) -> Option<(Kind, Vec<SelInput>)> {
    if id != "sketch.edit_text" {
        return None;
    }
    Some((kind_for(app, &selected_entity(app)?)?, Vec::new()))
}

/// Open the dialog for the text an outline (picked with the Edit Text tool) belongs to.
pub(crate) fn open(app: &mut SolveApp, entity: &str) {
    match kind_for(app, entity) {
        Some(kind) => {
            app.dialog = Some(Dialog::new(kind, Vec::new()));
            app.tool = None;
        }
        None => app.set_status("Choose an outline belonging to sketch text", true),
    }
}

/// Is the Edit Text dialog open?
pub(crate) fn editing(app: &SolveApp) -> bool {
    matches!(app.dialog.as_ref().map(|d| &d.kind), Some(Kind::Text(_)))
}

/// Keep the dialog in step with the design: false once its text is gone (sketch finished, the
/// text deleted or undone). Other design changes leave it open; when the saved source itself
/// changed (undo, a script), fields the user has not touched follow it.
pub(crate) fn refresh(app: &SolveApp, k: &mut Tx) -> bool {
    let Some((text, font, original)) = source(app, k.sketch, &k.link) else { return false };
    if original != k.original {
        let (scale, unit) = crate::prefs::unit_scale(&app.session.doc.units);
        let fresh = fields(&original, unit, scale);
        for ((value, initial), new) in k.values.iter_mut().zip(k.initial.iter_mut()).zip(fresh) {
            if value == initial {
                *value = new.clone();
            }
            *initial = new;
        }
        k.original = original;
        k.unit = unit;
    }
    if text != k.initial_text {
        if k.text == k.initial_text {
            k.text = text.clone();
        }
        k.initial_text = text;
    }
    if font != k.initial_font {
        if k.font == k.initial_font {
            k.font = font.clone();
        }
        k.initial_font = font;
    }
    true
}

/// The dialog's rows; true when Enter was pressed in a value field.
pub(crate) fn rows(ui: &mut egui::Ui, k: &mut Tx) -> bool {
    crate::dialogs::row_label(ui, "Text");
    let width = ui.spacing().text_edit_width;
    let response = ui.add(egui::TextEdit::multiline(&mut k.text).id_salt("sketch_text_content").desired_rows(3).desired_width(width));
    crate::scenario::publish_handle("dialog:Text", response.rect.center());
    if k.focus {
        response.request_focus();
        k.focus = false;
    }
    ui.end_row();
    crate::dialogs::row_label(ui, "Font");
    // Empty means the built-in font: its name is the hint.
    let hint = solidcraft_engine::sketch::DEFAULT_FONT_FAMILY;
    let response = ui.add(egui::TextEdit::singleline(&mut k.font).id_salt("sketch_text_font").hint_text(hint).desired_width(width));
    crate::scenario::publish_handle("dialog:Font", response.rect.center());
    ui.end_row();
    let mut enter = false;
    for (label, value) in LABELS.iter().zip(k.values.iter_mut()) {
        crate::dialogs::row_label(ui, label);
        enter |= crate::dialogs::field(ui, value);
        ui.end_row();
    }
    enter
}

/// The command OK runs. Fields take expressions with units and parameters; a bare number is
/// in the design's units (lengths) or degrees (angle).
pub(crate) fn commands(app: &SolveApp, k: &Tx) -> Result<Vec<(String, Value)>, String> {
    let doc = &app.session.doc;
    let mut out = [0.0; 4];
    for (i, slot) in out.iter_mut().enumerate() {
        let (Some(value), Some(initial), Some(original), Some(kind), Some(label)) =
            (k.values.get(i), k.initial.get(i), k.original.get(i), KINDS.get(i), LABELS.get(i))
        else {
            return Err("missing value".into());
        };
        *slot = if value == initial {
            *original
        } else {
            let v = doc.eval(&doc.with_design_unit(value.trim(), *kind), *kind).map_err(|e| format!("{label}: {e}"))?;
            if !v.is_finite() {
                return Err(format!("{label}: not a finite number"));
            }
            v
        };
    }
    let [x, y, height, angle] = out;
    let mut params = json!({"link": k.link, "text": k.text, "at": [x, y], "height": height});
    // Omitting an unchanged angle keeps its exact saved radians.
    if k.values.get(3) != k.initial.get(3) {
        params["angle"] = json!(angle.to_degrees());
    }
    // An untouched font stays as saved; an emptied one is the built-in font.
    if k.font.trim() != k.initial_font.trim() {
        params["font"] = json!(k.font.trim());
    }
    Ok(vec![("sketch.edit_text".into(), params)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::Harness;

    fn setup() -> (Harness, String, String) {
        let mut h = Harness::new();
        h.app.home.open = false;
        h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
        let result = h.app.run("sketch.text", json!({"text":"I","at":[0,0],"height":10})).unwrap();
        let link = result["link"].as_str().unwrap().to_owned();
        let curve = result["curves"][0].as_str().unwrap().to_owned();
        h.app.session.selection = vec![Sel::SketchCurve { id: curve.clone() }];
        h.frames(3);
        (h, link, curve)
    }

    fn tx(app: &SolveApp) -> &Tx {
        match app.dialog.as_ref().map(|d| &d.kind) {
            Some(Kind::Text(k)) => k,
            _ => panic!("Edit Text is not open"),
        }
    }

    fn ok(h: &mut Harness) {
        h.step(&json!({"click":{"handle":"dialog:OK"}})).unwrap();
        h.frames(2);
    }

    fn replace(h: &mut Harness, handle: &str, value: &str) {
        h.step(&json!({"click":{"handle":handle}})).unwrap();
        h.step(&json!({"key":"A","cmd":true})).unwrap();
        h.step(&json!({"text":value})).unwrap();
    }

    #[test]
    fn selected_text_menu_edits_source_position_and_size_with_one_undo_and_reopen() {
        let (mut h, link, _) = setup();
        let before = h.app.session.doc.to_json();
        let undo = h.app.session.undo.len();
        let item = crate::context_menu::items(&h.app, &crate::context_menu::Target::Viewport)
            .into_iter()
            .find(|item| item.id == "sketch.edit_text")
            .unwrap();
        crate::context_menu::run_item(&mut h.app, &item, egui::Pos2::ZERO);
        h.frames(3);
        assert!(editing(&h.app));
        for (field, value) in [("dialog:Text", "O"), ("dialog:X", "25"), ("dialog:Y", "12"), ("dialog:Height", "20"), ("dialog:Angle", "90")] {
            replace(&mut h, field, value);
        }
        assert_eq!(h.app.session.doc.to_json(), before);
        ok(&mut h);
        assert!(!editing(&h.app));
        assert_eq!(h.app.session.undo.len(), undo + 1);
        let sid = h.app.session.active_sketch.unwrap();
        let sk = h.app.session.doc.sketch(sid).unwrap();
        let expected = LinkSource::Text {
            text: "O".into(),
            at: solidcraft_engine::geom::Vec2::new(25.0, 12.0),
            height: 20.0,
            angle: std::f64::consts::FRAC_PI_2,
            font: None,
        };
        assert_eq!(sk.link(&link).unwrap().source, expected);
        assert!(!h.app.session.world_state().sketch(sid).unwrap().profiles.is_empty());
        let changed = h.app.session.doc.to_json();
        let reopened = solidcraft_engine::Session::new(solidcraft_engine::doc::Document::from_json(&changed).unwrap());
        assert_eq!(reopened.doc.sketch(sid).unwrap().link(&link).unwrap().source, expected);
        h.app.run("edit.undo", json!({})).unwrap();
        assert_eq!(h.app.session.doc.to_json(), before);
        h.app.run("edit.redo", json!({})).unwrap();
        assert_eq!(h.app.session.doc.to_json(), changed);
    }

    #[test]
    fn text_editor_converts_document_units_and_preserves_untouched_source_values() {
        let (mut h, link, _) = setup();
        h.app.run("document.units", json!({"units":"in"})).unwrap();
        h.app.run("sketch.edit_text", json!({"link":link,"at":[3.123456789,7.987654321],"height":9.123456789,"angle":17.123456789})).unwrap();
        let sid = h.app.session.active_sketch.unwrap();
        let original = h.app.session.doc.sketch(sid).unwrap().link(&link).unwrap().source.clone();
        h.app.start("sketch.edit_text");
        h.frames(3);
        assert_eq!(tx(&h.app).unit, "in");
        replace(&mut h, "dialog:Text", "O");
        ok(&mut h);
        let LinkSource::Text { at, height, angle, .. } = original else { panic!("text source") };
        assert_eq!(
            h.app.session.doc.sketch(sid).unwrap().link(&link).unwrap().source,
            LinkSource::Text { text: "O".into(), at, height, angle, font: None }
        );
        h.app.session.selection = vec![Sel::SketchCurve { id: h.app.session.doc.sketch(sid).unwrap().curves[0].id.clone() }];
        h.app.start("sketch.edit_text");
        h.frames(3);
        let undo = h.app.session.undo.len();
        replace(&mut h, "dialog:X", "1.2345");
        replace(&mut h, "dialog:Height", "0.25");
        ok(&mut h);
        let LinkSource::Text { at: updated, height: size, angle: rotation, .. } = h.app.session.doc.sketch(sid).unwrap().link(&link).unwrap().source
        else {
            panic!("text source")
        };
        assert!((updated.x - 31.3563).abs() < 1e-12);
        assert_eq!(updated.y, at.y);
        assert!((size - 6.35).abs() < 1e-12);
        assert_eq!(rotation, angle);
        assert_eq!(h.app.session.undo.len(), undo + 1);
    }

    #[test]
    fn the_font_row_sets_keeps_and_clears_the_font_of_a_text() {
        // The repository's second font, BIZ UDPGothic: its "I" is 17.6385 mm² at a capital height of 10 mm, the built-in
        // font's 13.5557 mm².
        let font = concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/solidcraft-web/fonts/BIZUDPGothic-Regular.ttf");
        let (mut h, link, _) = setup();
        // The dialog closes with the selection; the text's first outline is selected again to open it.
        let reopen = |h: &mut Harness| {
            let sid = h.app.session.active_sketch.unwrap();
            let id = h.app.session.doc.sketch(sid).unwrap().curves[0].id.clone();
            h.app.session.selection = vec![Sel::SketchCurve { id }];
            h.app.start("sketch.edit_text");
            h.frames(3);
        };
        let area = |h: &Harness| -> f64 {
            let sid = h.app.session.active_sketch.unwrap();
            h.app.session.world_state().sketch(sid).unwrap().profiles.iter().map(|p| p.area).sum()
        };
        assert!((area(&h) - 13.5557).abs() < 2e-3);
        h.app.start("sketch.edit_text");
        h.frames(3);
        assert_eq!(tx(&h.app).font, "");
        // A font that is not there keeps the dialog open with the reason, and the design as it was.
        let before = (h.app.session.doc.to_json(), h.app.session.undo.len());
        replace(&mut h, "dialog:Font", "No Such Family 12345");
        ok(&mut h);
        assert!(editing(&h.app));
        assert!(h.app.dialog.as_ref().unwrap().error.as_ref().is_some_and(|e| e.contains("unknown font")));
        assert_eq!((h.app.session.doc.to_json(), h.app.session.undo.len()), before);
        // A font file: one undo step, other outlines, and the dialog shows the font when it is opened again.
        replace(&mut h, "dialog:Font", font);
        ok(&mut h);
        assert!(!editing(&h.app));
        assert_eq!(h.app.session.undo.len(), before.1 + 1);
        let sid = h.app.session.active_sketch.unwrap();
        let font_of = |h: &Harness| match &h.app.session.doc.sketch(sid).unwrap().link(&link).unwrap().source {
            LinkSource::Text { font, .. } => font.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(font_of(&h).as_deref(), Some(font));
        assert!((area(&h) - 17.6385).abs() < 2e-3);
        reopen(&mut h);
        assert_eq!(tx(&h.app).font, font);
        // Changing only the text keeps the font: two I's of it.
        replace(&mut h, "dialog:Text", "II");
        ok(&mut h);
        assert_eq!(font_of(&h).as_deref(), Some(font));
        assert!((area(&h) - 2.0 * 17.6385).abs() < 2e-3);
        // An emptied field is the built-in font again: two I's of it.
        reopen(&mut h);
        h.step(&json!({"click":{"handle":"dialog:Font"}})).unwrap();
        h.step(&json!({"key":"A","cmd":true})).unwrap();
        h.step(&json!({"key":"Backspace"})).unwrap();
        ok(&mut h);
        assert_eq!(font_of(&h), None);
        assert!((area(&h) - 2.0 * 13.5557).abs() < 2e-3);
    }

    #[test]
    fn invalid_text_values_and_cancel_leave_document_and_history_unchanged() {
        let (mut h, _, _) = setup();
        let before = (h.app.session.doc.to_json(), h.app.session.undo.len());
        h.app.start("sketch.edit_text");
        h.frames(3);
        for value in ["NaN", "inf", "-5", "0"] {
            replace(&mut h, "dialog:Height", value);
            ok(&mut h);
            assert!(h.app.dialog.as_ref().unwrap().error.is_some());
            assert_eq!((h.app.session.doc.to_json(), h.app.session.undo.len()), before);
        }
        h.step(&json!({"widget":"Cancel"})).unwrap();
        assert!(!editing(&h.app));
        assert_eq!((h.app.session.doc.to_json(), h.app.session.undo.len()), before);
    }

    #[test]
    fn text_editor_rejects_mixed_sources_and_leaves_with_document_or_command() {
        let (mut h, _, curve) = setup();
        let other = h.app.run("sketch.text", json!({"text":"O","at":[30,0],"height":10})).unwrap();
        h.app.session.selection.push(Sel::SketchCurve { id: other["curves"][0].as_str().unwrap().into() });
        assert!(selected_entity(&h.app).is_none());
        assert!(!crate::context_menu::items(&h.app, &crate::context_menu::Target::Viewport).iter().any(|item| item.id == "sketch.edit_text"));
        h.app.session.selection = vec![Sel::SketchCurve { id: curve }];
        h.app.start("sketch.edit_text");
        assert!(editing(&h.app));
        h.app.start("sketch.line");
        assert!(!editing(&h.app));
        h.app.start("sketch.edit_text");
        assert!(editing(&h.app));
        crate::documents::open_tab(&mut h.app, solidcraft_engine::Session::default()).unwrap();
        assert!(!editing(&h.app));
    }

    #[test]
    fn edit_text_without_preselection_picks_a_real_outline() {
        let (mut h, link, _) = setup();
        h.app.session.selection.clear();
        h.call("ui.view", json!({"view":"top"}));
        h.app.cam.distance = 40.0;
        h.frames(3);
        let sid = h.app.session.active_sketch.unwrap();
        let polyline = h.app.session.doc.sketch(sid).unwrap().polyline(0);
        let midpoint = (polyline[0] + polyline[1]) * 0.5;
        h.step(&json!({"start":"sketch.edit_text"})).unwrap();
        assert!(h.app.tool.is_some());
        h.step(&json!({"click":{"sketch":[midpoint.x,midpoint.y]}})).unwrap();
        assert_eq!(tx(&h.app).link, link);
        assert!(h.app.tool.is_none());
        h.step(&json!({"widget":"Cancel"})).unwrap();
    }

    #[test]
    fn issue_report_preserves_text_edit_and_owns_keyboard_input() {
        let (mut h, _, _) = setup();
        h.app.start("sketch.edit_text");
        h.frames(3);
        replace(&mut h, "dialog:Text", "Pending text");
        let before = (h.app.session.doc.to_json(), h.app.session.undo.len());
        h.app.help.menu = true;
        h.frames(2);
        h.step(&json!({"click":{"handle":"menu:report"}})).unwrap();
        h.frames(3);
        let token = h.app.shot_token;
        crate::issue_report::receive(&mut h.app, token, std::sync::Arc::new(egui::ColorImage::new([2, 2], vec![egui::Color32::GREEN; 4])));
        h.frames(3);
        let focus = egui::Id::new("issue_report_title");
        h.ctx.memory_mut(|memory| memory.request_focus(focus));
        h.frames(2);
        assert_eq!(h.ctx.memory(|memory| memory.focused()), Some(focus));
        h.step(&json!({"text":"report-only-marker"})).unwrap();
        h.step(&json!({"key":"Escape"})).unwrap();
        assert_eq!(tx(&h.app).text, "Pending text");
        assert_eq!((h.app.session.doc.to_json(), h.app.session.undo.len()), before);
        h.step(&json!({"widget":"Close draft"})).unwrap();
        ok(&mut h);
        assert!(!editing(&h.app));
        assert_eq!(h.app.session.undo.len(), before.1 + 1);
    }

    #[test]
    fn fields_take_expressions_with_units_and_parameters() {
        let (mut h, link, _) = setup();
        h.app.run("parameters.add", json!({"name":"gap","expression":"4 mm","unit":"mm"})).unwrap();
        h.app.run("document.units", json!({"units":"in"})).unwrap();
        h.app.start("sketch.edit_text");
        h.frames(3);
        replace(&mut h, "dialog:X", "10 mm * 2 + 0.5 in");
        replace(&mut h, "dialog:Y", "gap * 3");
        replace(&mut h, "dialog:Height", "0.5");
        replace(&mut h, "dialog:Angle", "45 + 45");
        ok(&mut h);
        assert!(!editing(&h.app), "{:?}", h.app.dialog.as_ref().and_then(|d| d.error.clone()));
        let sid = h.app.session.active_sketch.unwrap();
        let LinkSource::Text { at, height, angle, .. } = h.app.session.doc.sketch(sid).unwrap().link(&link).unwrap().source.clone() else {
            panic!("text source")
        };
        assert!((at.x - 32.7).abs() < 1e-9);
        assert!((at.y - 12.0).abs() < 1e-9);
        assert!((height - 12.7).abs() < 1e-9);
        assert!((angle - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    }

    #[test]
    fn unrelated_design_changes_keep_the_dialog_open() {
        let (mut h, _, _) = setup();
        h.app.start("sketch.edit_text");
        h.frames(3);
        replace(&mut h, "dialog:Text", "Kept");
        h.app.run("sketch.line", json!({"points":[[50,0],[60,0]]})).unwrap();
        h.frames(3);
        assert!(editing(&h.app));
        assert_eq!(tx(&h.app).text, "Kept");
        h.app.run("edit.undo", json!({})).unwrap();
        h.app.run("edit.undo", json!({})).unwrap();
        h.frames(3);
        assert!(!editing(&h.app), "the text it edits is gone");
    }
}
