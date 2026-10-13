//! Tool windows for sketch-space modify commands, on the `dialogs_surface` pattern (#19): one
//! `Mod` enum behind one `Kind::SketchModify` variant, grown by one variant and one arm per
//! command rather than by a new module each time. Scale came first, whose toolbar button
//! reached no tool and no dialog at all, so a press ran `sketch.scale` with no parameters and
//! the engine answered "`factor` must be a positive number". Offset joins it here (upstream
//! issue #162 item 1: "Offset does not create a constraint, the size of the offset seems
//! random"). `sketch.fillet`, the three `sketch.chamfer.*` commands, the sketch patterns,
//! `sketch.mirror` and `sketch.move` are the same gap and join this module later, each its own
//! change.

use serde_json::{Map, Value, json};
use solidcraft_engine::Sel;
use solidcraft_engine::doc::expr::Kind as ValueKind;

use crate::SolveApp;
use crate::dialogs::{Kind, check, field, row_label};
use crate::selection::{CURVES, POINTS, SelInput};

/// A sketch modify tool window's values.
#[derive(Clone, Debug, PartialEq)]
pub enum Mod {
    /// `sketch.scale`: the factor the picked entities scale by. The base point is the second
    /// input; with none picked the command scales about the sketch origin, as its own default
    /// already does.
    Scale { factor: String },
    /// `sketch.offset`: the curves to offset, the distance typed and whether the connected
    /// chain follows. A positive distance is left of the chain, as the command defines it;
    /// `flip` reverses whatever sign the field holds.
    Offset { distance: String, chain: bool, flip: bool },
}

impl Mod {
    pub fn title(&self) -> &'static str {
        match self {
            Mod::Scale { .. } => "SKETCH SCALE",
            Mod::Offset { .. } => "OFFSET",
        }
    }

    /// The value on the canvas.
    pub fn primary(&mut self) -> Option<(&'static str, ValueKind, &mut String)> {
        Some(match self {
            Mod::Scale { factor } => ("Factor", ValueKind::Unitless, factor),
            Mod::Offset { distance, .. } => ("Distance", ValueKind::Length, distance),
        })
    }
}

fn inputs(k: &Mod) -> Vec<SelInput> {
    match k {
        Mod::Scale { .. } => vec![SelInput::new("Entities", CURVES | POINTS, true), SelInput::new("Base point", POINTS, false)],
        Mod::Offset { .. } => vec![SelInput::new("Curves", CURVES, true)],
    }
}

/// The dialog for a sketch modify command.
pub fn start(_app: &SolveApp, id: &str) -> Option<(Kind, Vec<SelInput>)> {
    let k = match id {
        "sketch.scale" => Mod::Scale { factor: "2".into() },
        "sketch.offset" => Mod::Offset { distance: "2 mm".into(), chain: true, flip: false },
        _ => return None,
    };
    let i = inputs(&k);
    Some((Kind::SketchModify(k), i))
}

/// The rows after the inputs; true when Enter was pressed in a field.
pub fn rows(ui: &mut egui::Ui, k: &mut Mod) -> bool {
    let mut enter = false;
    match k {
        Mod::Scale { factor } => {
            row_label(ui, "Factor");
            enter |= field(ui, factor);
            ui.end_row();
        }
        Mod::Offset { distance, chain, flip } => {
            row_label(ui, "Distance");
            enter |= field(ui, distance);
            ui.end_row();
            row_label(ui, "Flip");
            check(ui, flip);
            ui.end_row();
            row_label(ui, "Chain");
            check(ui, chain);
            ui.end_row();
        }
    }
    enter
}

/// What input `i` holds.
fn items(inputs: &[SelInput], i: usize) -> &[Sel] {
    inputs.get(i).map(|x| x.items.as_slice()).unwrap_or(&[])
}

/// A picked sketch point as the id its own sketch knows it by. Picking prefixes the sketch
/// ("`<sketch>:<point>`") so points of any visible sketch stay distinct; a sketch command
/// names the point alone.
fn point_id(sel: &Sel) -> Option<&str> {
    match sel {
        Sel::SketchPoint { id } => Some(id.split_once(':').map(|(_, p)| p).unwrap_or(id)),
        _ => None,
    }
}

/// The curves and lone points picked in input `i`, as the engine's `entities` list.
fn entity_ids(inputs: &[SelInput], i: usize) -> Vec<String> {
    items(inputs, i)
        .iter()
        .filter_map(|x| match x {
            Sel::SketchCurve { id } => Some(id.clone()),
            _ => point_id(x).map(str::to_string),
        })
        .collect()
}

/// Sketch curve ids picked in input `i`.
fn curve_ids(inputs: &[SelInput], i: usize) -> Vec<String> {
    items(inputs, i).iter().filter_map(|x| if let Sel::SketchCurve { id } = x { Some(id.clone()) } else { None }).collect()
}

/// A typed scale factor as the command wants it: the field evaluated against the document's
/// parameters, in the range the command accepts. Checking the range here lets the window say
/// so in its own error line instead of the status bar.
fn factor(app: &SolveApp, text: &str) -> Result<f64, String> {
    let v = app.session.doc.eval(text.trim(), ValueKind::Unitless).map_err(|e| format!("Factor: {e}"))?;
    if !(v.is_finite() && v > 1e-9 && v < 1e6) {
        return Err("Factor must be a positive number".into());
    }
    Ok(v)
}

/// A typed distance as the command wants it: the field evaluated against the document's
/// parameters, with the sign reversed when Flip is set. A bare number means the design's units,
/// as a typed sketch value does everywhere else.
fn signed_length(app: &SolveApp, text: &str, flip: bool) -> Result<f64, String> {
    let doc = &app.session.doc;
    let v = doc.eval(&doc.with_design_unit(text.trim(), ValueKind::Length), ValueKind::Length).map_err(|e| format!("Distance: {e}"))?;
    if !v.is_finite() || v.abs() <= 1e-9 {
        return Err("Distance must be a non-zero number".into());
    }
    Ok(if flip { -v } else { v })
}

/// The command a sketch modify dialog makes. `sketch.scale` takes a plain evaluated number for
/// its factor and `sketch.offset` one for its distance, not an expression string: neither has a
/// model parameter to carry one, unlike `sketch.fillet`'s radius.
pub fn commands(app: &SolveApp, k: &Mod, inputs: &[SelInput], extra: &Map<String, Value>) -> Result<Vec<(String, Value)>, String> {
    let (cmd, mut params) = match k {
        Mod::Scale { factor: text } => {
            let entities = entity_ids(inputs, 0);
            if entities.is_empty() {
                return Err("Select the sketch curves or points to scale first".into());
            }
            let f = factor(app, text)?;
            let mut p = json!({"entities": entities, "factor": f});
            if let Some(base) = items(inputs, 1).first().and_then(point_id) {
                p["base"] = json!(base);
            }
            ("sketch.scale", p)
        }
        Mod::Offset { distance, chain, flip } => {
            let curves = curve_ids(inputs, 0);
            if curves.is_empty() {
                return Err("Select the curves to offset first".into());
            }
            let d = signed_length(app, distance, *flip)?;
            ("sketch.offset", json!({"curves": curves, "distance": d, "chain": chain}))
        }
    };
    if let Value::Object(m) = &mut params {
        for (key, value) in extra {
            m.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    Ok(vec![(cmd.to_string(), params)])
}
