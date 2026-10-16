//! More sketch creation tools: midpoint line, tangent arc, tangent circles, center-point and
//! arc slots.

use serde_json::{Value, json};
use solidcraft_geom::Vec2;
use solidcraft_sketch::{ConstraintKind, CurveKind, LinkKind, LinkSource, Sketch, TextAlign, TextExprs, TextStyle};

/// The style parameters of `sketch.text` and `sketch.edit_text`.
macro_rules! text_style_doc {
    () => {
        "bold?, italic?: bool (an installed family's bold or italic face; italic without one is slanted), align?: left|center|right (the lines against `at`, or along the path), letter_spacing?: extra mm between letters, line_spacing?: times the font's line distance (default 1), path?: a curve id of the sketch the baseline follows (\"\": none; `at` and `angle` then do not place the text; the curve becomes construction, a guide the letters stand on), reverse?: bool (the other way along the path)"
    };
}

use super::sketch::{add_c, circumcircle, edit, ids_of, mark_construction, req_parg, result, slot};
use super::{CommandSpec, in_sketch};
use crate::params::{bad, num, req_vec2, str_, string_list};
use crate::{Result, Session};

#[cfg(test)]
#[path = "sketch_text_move_tests.rs"]
mod text_move_tests;

#[cfg(test)]
#[path = "sketch_text_expr_tests.rs"]
mod text_expr_tests;

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("sketch.line.midpoint", "Midpoint Line", midpoint_line)
        .at("SKETCH", "CREATE")
        .icon("line_mid")
        .enabled(in_sketch)
        .params(concat!("mid: [x,y] or point ref, end: [x,y]; construction?", created_doc!())),
    CommandSpec::new("sketch.arc.tangent", "Tangent Arc", arc_tangent).at("SKETCH", "CREATE").icon("arc_tangent").enabled(in_sketch).params(concat!(
        "start: end point of a line or arc (\"l1.end\"), end: [x,y]; curve?: the curve to be tangent to (default: the one ending at start)",
        created_doc!()
    )),
    CommandSpec::new("sketch.circle.two_tangent", "2-Tangent Circle", circle_tan_tan)
        .at("SKETCH", "CREATE")
        .icon("circle_tt")
        .enabled(in_sketch)
        .params(concat!("curves: [two lines/circles/arcs], radius | diameter: expr, near: [x,y] (which of the solutions)", created_doc!())),
    CommandSpec::new("sketch.circle.three_tangent", "3-Tangent Circle", circle_three_tan)
        .at("SKETCH", "CREATE")
        .icon("circle_ttt")
        .enabled(in_sketch)
        .params(concat!("curves: [three lines/circles/arcs], near: [x,y] (which of the solutions)", created_doc!())),
    CommandSpec::new("sketch.slot.center_point", "Center Point Slot", slot_center_point)
        .at("SKETCH", "CREATE")
        .icon("slot")
        .enabled(in_sketch)
        .params(concat!("center: slot centre, end: one arc centre, width", created_doc!())),
    CommandSpec::new("sketch.slot.arc_three_point", "Three Point Arc Slot", arc_slot_three)
        .at("SKETCH", "CREATE")
        .icon("arc_slot")
        .enabled(in_sketch)
        .params(concat!("start, through, end: points on the centre arc, width", created_doc!())),
    CommandSpec::new("sketch.slot.arc_center", "Center Point Arc Slot", arc_slot_center)
        .at("SKETCH", "CREATE")
        .icon("arc_slot")
        .enabled(in_sketch)
        .params(concat!("center, start: centre arc start, end: direction of the centre arc end (counter-clockwise), width", created_doc!())),
    CommandSpec::new("sketch.ellipse", "Ellipse", ellipse).at("SKETCH", "CREATE").icon("ellipse").enabled(in_sketch).params(concat!(
        "center, major: end of the major axis [x,y], minor: a point the ellipse passes through on the minor side [x,y] | minor_radius",
        created_doc!()
    )),
    CommandSpec::new("sketch.spline.fit_point", "Fit Point Spline", spline_fit)
        .at("SKETCH", "CREATE")
        .icon("spline")
        .enabled(in_sketch)
        .params(concat!("points: [[x,y] or point refs…] (2…500): the spline passes through them, periodic?: boolean (default false); periodic cubic fit closes smoothly through at least 3 distinct samples", created_doc!())),
    CommandSpec::new("sketch.spline.control_point", "Control Point Spline", spline_cv3)
        .at("SKETCH", "CREATE")
        .icon("spline_cv")
        .enabled(in_sketch)
        .params(concat!("points: control points (2…500), cubic", created_doc!())),
    CommandSpec::new("sketch.spline.control_point_5", "Control Point Spline (Degree 5)", spline_cv5)
        .at("SKETCH", "CREATE")
        .icon("spline_cv")
        .enabled(in_sketch)
        .params(concat!("points: control points (2…500), degree 5", created_doc!())),
    CommandSpec::new("sketch.conic", "Conic Curve", conic)
        .at("SKETCH", "CREATE")
        .icon("conic")
        .enabled(in_sketch)
        .params(concat!("start, end, apex: [x,y] or point refs; rho?: 0…1 (default 0.5: parabola)", created_doc!())),
    CommandSpec::new("sketch.blend_curve", "Blend Curve", blend)
        .at("SKETCH", "MODIFY")
        .icon("blend")
        .enabled(in_sketch)
        .params(concat!("a, b: end points of two curves (\"l1.end\", \"a2.start\"): a smooth (tangent) spline joining them", created_doc!())),
    CommandSpec::new("sketch.centerline", "Centerline", centerline)
        .at("SKETCH", "CREATE")
        .icon("centerline")
        .enabled(in_sketch)
        .params("curves: [line ids], value?: bool (default toggles): centerline line type (not in profiles; an axis)"),
    CommandSpec::new("sketch.infinite", "Infinite Line", infinite_line)
        .enabled(in_sketch)
        .params("curves: [line ids], value?: bool (default toggles): infinite line type (#1359): construction geometry drawn across the whole view through the line's two points; off, a normal line"),
    CommandSpec::new("sketch.text", "Text", text).at("SKETCH", "CREATE").icon("text").enabled(in_sketch).params(concat!(
        "text: string (\\n breaks lines), at: baseline start [x,y], height?: capital height mm (default 5), angle?: deg; at's x and y, height and angle each take a number or an expression over the design's parameters as a string (\"plate_width / 2\", \"label_size\"): the text keeps the expression, follows the parameters, and returns it in `exprs`; an expression naming no parameter is the number it comes to, a bare number in one is in the design's units, \
         font?: an installed font's family or full name, or a .ttf/.otf file path (~ and %VAR% expand; default: the built-in Liberation Sans), ",
        text_style_doc!(),
        created_doc!()
    )),
    CommandSpec::new("sketch.edit_text", "Edit Text", edit_text)
        .enabled(in_sketch)
        .live(edit_text_live)
        .params(concat!(
            "link: the text's link id (or entity: one of its curves), text?, at?, height?, angle?: deg (numbers or expressions as in sketch.text; a number replaces an expression, and so does moving the text), font?: family or file path (empty: the built-in font), ",
            text_style_doc!(),
            " (each kept when left out); live?: bool: one frame of an interactive change (the sketch is solved, the features after it stay as they were until `sketch.drag_end`)",
            created_doc!()
        )),
];

/// The style a command's parameters give, over `old`.
fn text_style(p: &Value, cmd: &str, old: TextStyle) -> Result<TextStyle> {
    let mut style = old;
    for (key, slot) in [("bold", &mut style.bold), ("italic", &mut style.italic), ("reverse", &mut style.reverse)] {
        match p.get(key) {
            None | Some(Value::Null) => {}
            Some(Value::Bool(b)) => *slot = *b,
            Some(_) => return Err(bad(cmd, format!("`{key}` must be true or false"))),
        }
    }
    match p.get("align") {
        None | Some(Value::Null) => {}
        Some(Value::String(a)) => {
            style.align = match a.trim().to_ascii_lowercase().as_str() {
                "left" => TextAlign::Left,
                "center" | "centre" => TextAlign::Center,
                "right" => TextAlign::Right,
                _ => return Err(bad(cmd, "`align` must be left, center or right")),
            }
        }
        Some(_) => return Err(bad(cmd, "`align` must be left, center or right")),
    }
    if p.get("letter_spacing").is_some_and(|v| !v.is_null()) {
        style.letter_spacing =
            num(p, "letter_spacing").filter(|v| v.is_finite()).ok_or_else(|| bad(cmd, "`letter_spacing` must be a length in mm"))?;
    }
    if p.get("line_spacing").is_some_and(|v| !v.is_null()) {
        style.line_spacing =
            num(p, "line_spacing").filter(|v| v.is_finite() && *v > 0.0).ok_or_else(|| bad(cmd, "`line_spacing` must be a positive factor"))?;
    }
    match p.get("path") {
        None | Some(Value::Null) => {}
        Some(Value::String(c)) if c.trim().is_empty() => style.path = None,
        Some(Value::String(c)) => style.path = Some(c.trim().to_string()),
        Some(_) => return Err(bad(cmd, "`path` must be a curve id")),
    }
    Ok(style)
}

/// What a text's `at` must be.
const AT_MUST: &str = "`at` must be the baseline start [x, y], each a number or an expression";

/// One number of a text, from a command's parameter `key`. A number is that number (mm, or
/// degrees for the angle). A string is an expression over the design's parameters: a bare number
/// in it is in the design's units (degrees for the angle), and it is kept as an expression when
/// it names a parameter, so that the text follows it; one that names none is the number it comes
/// to. Returns the value (mm or radians) and the expression kept. Anything else is refused with
/// `must`.
fn text_number(doc: &solidcraft_doc::Document, v: &Value, cmd: &str, key: &str, must: &str, angle: bool) -> Result<(f64, Option<String>)> {
    use solidcraft_doc::expr::{self, Kind};
    let kind = if angle { Kind::Angle } else { Kind::Length };
    match v {
        Value::Number(n) => {
            let x = n.as_f64().and_then(crate::params::finite).ok_or_else(|| bad(cmd, must))?;
            Ok((if angle { x.to_radians() } else { x }, None))
        }
        Value::String(e) if !e.trim().is_empty() && e.len() <= 4096 => {
            let e = doc.with_design_unit(e.trim(), kind);
            match doc.eval(&e, kind) {
                Ok(x) => {
                    let x = crate::params::finite(x).ok_or_else(|| bad(cmd, must))?;
                    Ok((x, (!expr::references(&e).is_empty()).then_some(e)))
                }
                // Say what is wrong with it (an unknown parameter, a length where an angle goes).
                Err(error) => Err(bad(cmd, format!("{key}: {error}"))),
            }
        }
        _ => Err(bad(cmd, must)),
    }
}

/// A text's values as `text_source` works on them: text, place, height, angle (radians), font,
/// style and the expressions behind the numbers.
type TextValues = (String, Vec2, f64, f64, Option<String>, TextStyle, TextExprs);

/// A text's source from a command's parameters: what is given replaces what `old` has (the
/// defaults, for a new text); what is left out stays, an expression with its number. A number
/// replaces an expression. An input that is not understood is refused, not ignored.
fn text_source(doc: &solidcraft_doc::Document, p: &Value, cmd: &str, old: Option<TextValues>) -> Result<LinkSource> {
    let (t0, mut at, mut height, mut angle, f0, s0, mut exprs) =
        old.unwrap_or((String::new(), Vec2::ZERO, 5.0, 0.0, None, TextStyle::default(), TextExprs::default()));
    let text = match p.get("text") {
        None | Some(Value::Null) => t0,
        Some(Value::String(t)) => t.clone(),
        Some(_) => return Err(bad(cmd, "`text` must be a string")),
    };
    if text.trim().is_empty() {
        return Err(bad(cmd, "`text` must not be empty"));
    }
    if text.chars().count() > solidcraft_sketch::MAX_TEXT_CHARS {
        return Err(bad(cmd, "text too long"));
    }
    match p.get("at") {
        None | Some(Value::Null) => {}
        Some(Value::Array(a)) => {
            let [x, y] = a.as_slice() else { return Err(bad(cmd, AT_MUST)) };
            (at.x, exprs.x) = text_number(doc, x, cmd, "`at` x", AT_MUST, false)?;
            (at.y, exprs.y) = text_number(doc, y, cmd, "`at` y", AT_MUST, false)?;
        }
        Some(v) => {
            at = crate::params::vec2(v).ok_or_else(|| bad(cmd, AT_MUST))?;
            (exprs.x, exprs.y) = (None, None);
        }
    }
    if let Some(v) = p.get("height").filter(|v| !v.is_null()) {
        (height, exprs.height) = text_number(doc, v, cmd, "`height`", "`height` must be a length in mm, or an expression", false)?;
    }
    if !(height > 1e-6 && height < 1e5) {
        return Err(bad(cmd, "`height` must be positive"));
    }
    if let Some(v) = p.get("angle").filter(|v| !v.is_null()) {
        (angle, exprs.angle) = text_number(doc, v, cmd, "`angle`", "`angle` must be an angle in degrees, or an expression", true)?;
    }
    let font = match p.get("font") {
        None | Some(Value::Null) => f0,
        Some(Value::String(f)) if f.trim().is_empty() => None,
        Some(Value::String(f)) => Some(f.trim().to_string()),
        Some(_) => return Err(bad(cmd, "`font` must be a font family name or a font file path")),
    };
    let style = text_style(p, cmd, s0)?;
    // An unknown font, an unreadable file or a style the font lacks is an error now, not a lost
    // link later; so is a font file outside an agent's folders (the font reader asks `vfs`).
    solidcraft_sketch::font_data_styled(font.as_deref(), style.bold, style.italic).map_err(|e| bad(cmd, e.to_string()))?;
    Ok(LinkSource::Text { text, at, height, angle, font, style, exprs })
}

/// The expressions behind a text's numbers, for a command's reply (`{"height": "plate_h / 10"}`).
fn text_exprs_json(src: &LinkSource) -> Option<Value> {
    let LinkSource::Text { exprs, .. } = src else { return None };
    let mut out = serde_json::Map::new();
    for (key, e) in [("x", &exprs.x), ("y", &exprs.y), ("height", &exprs.height), ("angle", &exprs.angle)] {
        if let Some(e) = e {
            out.insert(key.into(), json!(e));
        }
    }
    (!out.is_empty()).then_some(Value::Object(out))
}

fn text(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sketch.text";
    if p.get("at").is_none_or(Value::is_null) {
        return Err(bad(cmd, AT_MUST));
    }
    let src = text_source(&s.doc, p, cmd, None)?;
    let id = s.active_sketch.ok_or_else(|| bad(cmd, "no sketch is being edited"))?;
    let mut doc = (*s.doc).clone();
    let mut sk = doc.sketch(id)?.clone();
    // A text needs nothing outside its sketch (a path is one of its curves).
    let geom = sk.text_source_geometry(&src).map_err(|e| bad(cmd, e.to_string()))?;
    if geom.is_empty() {
        return Err(bad(cmd, "the text makes no outline"));
    }
    let guide = guide(&mut sk, &src);
    let exprs = text_exprs_json(&src);
    let l = sk.add_link(LinkKind::Text, src, &geom)?;
    let curves = ids_of(&sk, &sk.link_curves(&l));
    *doc.sketch_mut(id)? = sk;
    *s.doc_mut() = doc;
    let mut out = json!({"link": l, "curves": curves, "guide": guide});
    if let Some(exprs) = exprs {
        out["exprs"] = exprs;
    }
    Ok(out)
}

/// A text's path is its guide: the curve it follows becomes construction. Left as it was, it
/// would cut the letters standing on it (and the ones hanging below it) into pieces, each
/// profile a piece of a letter and of the curve, and picking the text would pick those pieces.
/// Taking the text off the curve leaves the curve as it is. The curve made construction, if any.
fn guide(sk: &mut Sketch, src: &LinkSource) -> Option<String> {
    let LinkSource::Text { style, .. } = src else { return None };
    let id = style.path.as_ref()?;
    let curve = sk.curve_index(id).and_then(|c| sk.curves.get_mut(c))?;
    if curve.construction {
        return None;
    }
    curve.construction = true;
    curve.centerline = false;
    Some(id.clone())
}

/// The text source owning an outline or one of its linked points.
pub(super) fn text_link<'a>(sk: &'a Sketch, entity: &str) -> Option<&'a str> {
    let link = sk
        .curve_index(entity)
        .and_then(|i| sk.curves.get(i))
        .and_then(|c| c.link.as_deref())
        .or_else(|| sk.resolve_point(entity).and_then(|i| sk.points.get(i)).and_then(|p| p.link.as_deref()))?;
    sk.link(link).filter(|source| source.kind == LinkKind::Text).map(|source| source.id.as_str())
}

/// Transform a text source as a unit; generated outlines remain linked and editable.
pub(super) fn move_text(sk: &mut Sketch, link: &str, translate: Vec2, rotation: f64, center: Vec2, copy: bool) -> Result<Vec<usize>> {
    let Some(LinkSource::Text { text, at, height, angle, font, style, mut exprs }) = sk.link(link).map(|l| l.source.clone()) else {
        return Err(bad("sketch.move", "the selected source is not text"));
    };
    if style.path.is_some() {
        return Err(bad("sketch.move", "a text on a path follows its curve: move the curve, or take the text off it"));
    }
    let q = at - center;
    let (sin, cos) = rotation.sin_cos();
    let moved = center + Vec2::new(q.x * cos - q.y * sin, q.x * sin + q.y * cos) + translate;
    // What the move changes is a number now, not the expression it was; the rest stays.
    if (moved.x - at.x).abs() > 1e-12 {
        exprs.x = None;
    }
    if (moved.y - at.y).abs() > 1e-12 {
        exprs.y = None;
    }
    if rotation.abs() > 1e-12 {
        exprs.angle = None;
    }
    let (at, angle) = (moved, angle + rotation);
    let geom = solidcraft_sketch::text_geometry_styled(font.as_deref(), &text, at, height, angle, &style, None)?;
    let source = LinkSource::Text { text, at, height, angle, font, style, exprs };
    let link = if copy {
        sk.add_link(LinkKind::Text, source, &geom)?
    } else {
        sk.set_link_source(link, source);
        if sk.update_link(link, &geom)? {
            return Err(bad("sketch.move", "moving text must preserve its outline topology; edit the text source first"));
        }
        link.to_owned()
    };
    Ok(sk.link_curves(&link))
}

fn edit_text(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sketch.edit_text";
    let id = s.active_sketch.ok_or_else(|| bad(cmd, "no sketch is being edited"))?;
    let mut doc = (*s.doc).clone();
    let mut sk = doc.sketch(id)?.clone();
    // The numbers its expressions give now (a parameter may have changed since the sketch was
    // last stored); one that cannot be evaluated is left as it was, to be replaced by this edit.
    let _ = doc.apply_text_values(&doc.param_values().0, &mut sk);
    let out = apply_text_edit(&mut sk, &doc, p)?;
    *doc.sketch_mut(id)? = sk;
    *s.doc_mut() = doc;
    Ok(out)
}

/// One frame of an interactive resize or rotation: the sketch is solved and shown, the
/// features after it keep their last result until `sketch.drag_end` evaluates the model.
fn edit_text_live(s: &mut Session, p: &Value) -> Result<(Value, Option<crate::LiveEdit>)> {
    let cmd = "sketch.edit_text";
    let id = s.active_sketch.ok_or_else(|| bad(cmd, "no sketch is being edited"))?;
    let (out, report) = super::sketch::edit_solved(s, id, false, |sk, doc| Ok((apply_text_edit(sk, doc, p)?, super::sketch::Left::Unsolved)))?;
    Ok((out, report.map(|report| crate::LiveEdit { sketch: id, report })))
}

/// `sketch.edit_text`'s change to its sketch: the text source's new values and its outlines
/// regenerated from them.
fn apply_text_edit(sk: &mut Sketch, doc: &solidcraft_doc::Document, p: &Value) -> Result<Value> {
    let cmd = "sketch.edit_text";
    let l = match (str_(p, "link"), str_(p, "entity")) {
        (Some(l), _) => l.to_string(),
        (None, Some(e)) => sk.curves.iter().find(|c| c.id == e).and_then(|c| c.link.clone()).ok_or_else(|| bad(cmd, format!("`{e}` is not text")))?,
        _ => return Err(bad(cmd, "give `link` or `entity`")),
    };
    let old = match sk.link(&l).map(|x| (x.kind, x.source.clone())) {
        Some((LinkKind::Text, LinkSource::Text { text, at, height, angle, font, style, exprs })) => (text, at, height, angle, font, style, exprs),
        _ => return Err(bad(cmd, format!("`{l}` is not text"))),
    };
    let src = text_source(doc, p, cmd, Some(old))?;
    let geom = sk.text_source_geometry(&src).map_err(|e| bad(cmd, e.to_string()))?;
    if geom.is_empty() {
        return Err(bad(cmd, "the text makes no outline"));
    }
    let guide = guide(sk, &src);
    let exprs = text_exprs_json(&src);
    sk.set_link_source(&l, src.clone());
    sk.update_link(&l, &geom)?;
    let curves = ids_of(sk, &sk.link_curves(&l));
    let mut out = json!({"link": l, "curves": curves, "guide": guide});
    if let Some(exprs) = exprs {
        out["exprs"] = exprs;
    }
    Ok(out)
}

/// The curve ending at point `pi` and the direction leaving it there.
fn end_of(sk: &Sketch, pi: usize) -> Option<(usize, Vec2)> {
    sk.curves.iter().enumerate().filter(|(_, c)| !c.construction).find_map(|(i, _)| leaving_dir(sk, i, pi).map(|d| (i, d)))
}

fn blend(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sketch.blend_curve";
    let (out, info) = edit(s, p, cmd, false, |sk, _| {
        let pa = req_parg(sk, p, "a", cmd)?.idx().ok_or_else(|| bad(cmd, "`a` must be the end point of a curve"))?;
        let pb = req_parg(sk, p, "b", cmd)?.idx().ok_or_else(|| bad(cmd, "`b` must be the end point of a curve"))?;
        let (ca, ta) = end_of(sk, pa).ok_or_else(|| bad(cmd, "no curve ends at `a`"))?;
        let (cb, tb) = end_of(sk, pb).ok_or_else(|| bad(cmd, "no curve ends at `b`"))?;
        let (qa, qb) = (sk.point(pa).unwrap_or_default(), sk.point(pb).unwrap_or_default());
        let l = qa.dist(qb);
        if l < 1e-9 || ca == cb && pa == pb {
            return Err(bad(cmd, "the ends must differ"));
        }
        let first_new = sk.points.len();
        let c1 = sk.add_point(qa + ta * (l / 3.0), None)?;
        let c2 = sk.add_point(qb + tb * (l / 3.0), None)?;
        let sp = sk.add_curve(CurveKind::Spline { pts: vec![pa, c1, c2, pb], control: true, degree: 3, periodic: false }, None)?;
        let cons = vec![add_c(sk, ConstraintKind::Tangent { a: ca, b: sp })?, add_c(sk, ConstraintKind::Tangent { a: cb, b: sp })?];
        solve_new_only(sk, first_new);
        Ok((ids_of(sk, &[sp]), cons))
    })?;
    Ok(result(out, info))
}

fn centerline(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sketch.centerline";
    let ids = string_list(p, "curves");
    if ids.is_empty() {
        return Err(bad(cmd, "`curves` must list lines"));
    }
    let want = crate::params::bool_(p, "value");
    let (n, info) = edit(s, p, cmd, false, |sk, _| {
        for id in &ids {
            let c = sk.curve_index(id).ok_or_else(|| bad(cmd, format!("unknown curve `{id}`")))?;
            let cu = sk.curves.get_mut(c).ok_or_else(|| bad(cmd, "curve"))?;
            if !matches!(cu.kind, CurveKind::Line { .. }) {
                return Err(bad(cmd, format!("`{id}` is not a line")));
            }
            cu.centerline = want.unwrap_or(!cu.centerline);
            if cu.centerline {
                cu.construction = false;
                cu.infinite = false;
            }
        }
        Ok(ids.len())
    })?;
    Ok(json!({"changed": n, "sketch": info}))
}

fn infinite_line(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sketch.infinite";
    let ids = string_list(p, "curves");
    if ids.is_empty() {
        return Err(bad(cmd, "`curves` must list lines"));
    }
    let want = crate::params::bool_(p, "value");
    let (n, info) = edit(s, p, cmd, false, |sk, _| {
        for id in &ids {
            let c = sk.curve_index(id).ok_or_else(|| bad(cmd, format!("unknown curve `{id}`")))?;
            let cu = sk.curves.get_mut(c).ok_or_else(|| bad(cmd, "curve"))?;
            if !matches!(cu.kind, CurveKind::Line { .. }) {
                return Err(bad(cmd, format!("`{id}` is not a line")));
            }
            // Infinite lines are construction geometry (never in a profile); off, a normal line.
            cu.infinite = want.unwrap_or(!cu.infinite);
            cu.construction = cu.infinite;
            cu.centerline = false;
        }
        Ok(ids.len())
    })?;
    Ok(json!({"changed": n, "sketch": info}))
}

fn ellipse(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sketch.ellipse";
    let (c, m) = (req_vec2(cmd, p, "center")?, req_vec2(cmd, p, "major")?);
    let u = (m - c).normalized().ok_or_else(|| bad(cmd, "the major axis end must differ from the centre"))?;
    let a = c.dist(m);
    let r = match (num(p, "minor_radius"), p.get("minor").and_then(crate::params::vec2)) {
        (Some(r), _) => r,
        (None, Some(q)) => {
            // Minor radius so the ellipse passes through q.
            let d = q - c;
            let (x, y) = (d.dot(u), d.cross(u).abs());
            let k = 1.0 - (x / a).powi(2);
            if k <= 1e-12 {
                return Err(bad(cmd, "the minor point must lie within the major axis span"));
            }
            y / k.sqrt()
        }
        _ => return Err(bad(cmd, "needs `minor` [x,y] or `minor_radius`")),
    };
    if !(r > 1e-9 && r < 1e8) {
        return Err(bad(cmd, "the minor radius must be positive"));
    }
    let (out, info) = edit(s, p, cmd, false, |sk, _| {
        let ci = sk.add_point(c, None)?;
        let mi = sk.add_point(m, None)?;
        let e = sk.add_curve(CurveKind::Ellipse { c: ci, m: mi, r }, None)?;
        mark_construction(sk, &[e], p);
        Ok((ids_of(sk, &[e]), Vec::new()))
    })?;
    Ok(result(out, info))
}

fn point_list(sk: &mut Sketch, p: &Value, cmd: &str) -> Result<Vec<usize>> {
    let list = p.get("points").and_then(Value::as_array).ok_or_else(|| bad(cmd, "`points` must be a list"))?;
    if list.len() < 2 || list.len() > solidcraft_sketch::MAX_SPLINE_POINTS {
        return Err(bad(cmd, "a spline needs 2 to 500 points"));
    }
    let mut out = Vec::new();
    for v in list {
        let a = super::sketch::parg(sk, v, cmd)?;
        let i = match a.idx() {
            Some(i) => i,
            None => sk.add_point(a.pos(), None)?,
        };
        if out.last() == Some(&i) || out.last().and_then(|l| sk.point(*l)).is_some_and(|q| q.dist(a.pos()) < 1e-9) {
            return Err(bad(cmd, "consecutive points must differ"));
        }
        out.push(i);
    }
    Ok(out)
}

fn spline(s: &mut Session, p: &Value, cmd: &str, control: bool, degree: u8) -> Result<Value> {
    let periodic = match p.get("periodic") {
        None => false,
        Some(Value::Bool(v)) => *v,
        _ => return Err(bad(cmd, "periodic must be boolean")),
    };
    if periodic && (control || degree != 3) {
        return Err(bad(cmd, "periodic mode requires a cubic fit spline"));
    }
    let p = &super::sketch_project::snap_points(s, p, cmd)?;
    let (out, info) = edit(s, p, cmd, false, |sk, _| {
        let mut pts = point_list(sk, p, cmd)?;
        if periodic
            && sk.point(*pts.first().ok_or_else(|| bad(cmd, "missing first point"))?)
                != sk.point(*pts.last().ok_or_else(|| bad(cmd, "missing last point"))?)
        {
            pts.push(*pts.first().ok_or_else(|| bad(cmd, "missing first point"))?);
        }
        let c = sk.add_curve(CurveKind::Spline { pts, control, degree, periodic }, None)?;
        mark_construction(sk, &[c], p);
        Ok((ids_of(sk, &[c]), Vec::new()))
    })?;
    Ok(result(out, info))
}

fn spline_fit(s: &mut Session, p: &Value) -> Result<Value> {
    spline(s, p, "sketch.spline.fit_point", false, 3)
}
fn spline_cv3(s: &mut Session, p: &Value) -> Result<Value> {
    spline(s, p, "sketch.spline.control_point", true, 3)
}
fn spline_cv5(s: &mut Session, p: &Value) -> Result<Value> {
    spline(s, p, "sketch.spline.control_point_5", true, 5)
}

fn conic(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sketch.conic";
    let rho = num(p, "rho").unwrap_or(0.5);
    if !(rho > 1e-6 && rho < 1.0 - 1e-6) {
        return Err(bad(cmd, "`rho` must be between 0 and 1"));
    }
    let p = &super::sketch_project::snap_points(s, p, cmd)?;
    let (out, info) = edit(s, p, cmd, false, |sk, _| {
        let mut idx = Vec::new();
        for k in ["start", "end", "apex"] {
            let a = req_parg(sk, p, k, cmd)?;
            idx.push(match a.idx() {
                Some(i) => i,
                None => sk.add_point(a.pos(), None)?,
            });
        }
        let (a, b, x) = (idx[0], idx[1], idx[2]);
        if a == b || a == x || b == x {
            return Err(bad(cmd, "start, end and apex must differ"));
        }
        let c = sk.add_curve(CurveKind::Conic { a, b, apex: x, rho }, None)?;
        mark_construction(sk, &[c], p);
        Ok((ids_of(sk, &[c]), Vec::new()))
    })?;
    Ok(result(out, info))
}

fn midpoint_line(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sketch.line.midpoint";
    let end = req_vec2(cmd, p, "end")?;
    let (out, info) = edit(s, p, cmd, false, |sk, _| {
        let m = req_parg(sk, p, "mid", cmd)?;
        let start = m.pos() * 2.0 - end;
        if start.dist(end) < 1e-9 {
            return Err(bad(cmd, "the end must differ from the midpoint"));
        }
        let l = sk.add_line(start, end, None, None, None)?;
        let mp = match m.idx() {
            Some(i) => i,
            None => sk.add_point(m.pos(), None)?,
        };
        let k = add_c(sk, ConstraintKind::Midpoint { p: mp, l })?;
        mark_construction(sk, &[l], p);
        Ok((ids_of(sk, &[l]), vec![k]))
    })?;
    Ok(result(out, info))
}

/// Direction leaving curve `ci` at its end point `pi` (continuing the curve past that end).
fn leaving_dir(sk: &Sketch, ci: usize, pi: usize) -> Option<Vec2> {
    match sk.curves.get(ci)?.kind {
        CurveKind::Line { a, b } => {
            let (pa, pb) = (sk.point(a)?, sk.point(b)?);
            if pi == b {
                (pb - pa).normalized()
            } else if pi == a {
                (pa - pb).normalized()
            } else {
                None
            }
        }
        CurveKind::Arc { c, a, b } => {
            let (pc, q) = (sk.point(c)?, sk.point(pi)?);
            let ccw = (q - pc).perp().normalized()?;
            if pi == b {
                Some(ccw)
            } else if pi == a {
                Some(-ccw)
            } else {
                None
            }
        }
        CurveKind::Spline { ref pts, control, periodic, .. } => {
            let q: Vec<Vec2> = pts.iter().filter_map(|i| sk.point(*i)).collect();
            if pts.last() == Some(&pi) {
                solidcraft_sketch::spline_end_tangent_mode(&q, control, periodic, true)
            } else if pts.first() == Some(&pi) {
                solidcraft_sketch::spline_end_tangent_mode(&q, control, periodic, false).map(|t| -t)
            } else {
                None
            }
        }
        CurveKind::Conic { a, b, apex, .. } => {
            let x = sk.point(apex)?;
            if pi == b {
                (sk.point(b)? - x).normalized()
            } else if pi == a {
                (sk.point(a)? - x).normalized()
            } else {
                None
            }
        }
        _ => None,
    }
}

fn arc_tangent(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sketch.arc.tangent";
    let end = req_vec2(cmd, p, "end")?;
    let (out, info) = edit(s, p, cmd, false, |sk, _| {
        let st = req_parg(sk, p, "start", cmd)?;
        let pi = st.idx().ok_or_else(|| bad(cmd, "`start` must be the end point of a line or arc"))?;
        let ci = match str_(p, "curve") {
            Some(id) => sk.curve_index(id).ok_or_else(|| bad(cmd, format!("unknown curve `{id}`")))?,
            None => sk
                .curves
                .iter()
                .enumerate()
                .find(|(i, c)| !c.construction && leaving_dir(sk, *i, pi).is_some())
                .map(|(i, _)| i)
                .ok_or_else(|| bad(cmd, "no line or arc ends at `start`"))?,
        };
        let t = leaving_dir(sk, ci, pi).ok_or_else(|| bad(cmd, "`start` is not an end of that curve"))?;
        let s0 = st.pos();
        let n = t.perp();
        let d = end - s0;
        let den = 2.0 * n.dot(d);
        if den.abs() < 1e-9 * d.len().max(1e-9) || d.len() < 1e-9 {
            return Err(bad(cmd, "the end is straight ahead: draw a line instead"));
        }
        let center = s0 + n * (d.len2() / den);
        let a = if den > 0.0 {
            sk.add_arc(center, s0, end, [None, Some(pi), None], None)?
        } else {
            let a = sk.add_arc(center, end, s0, [None, None, Some(pi)], None)?;
            if let Some(c) = sk.curves.get_mut(a) {
                c.reversed = true;
            }
            a
        };
        let k = add_c(sk, ConstraintKind::Tangent { a: ci, b: a })?;
        Ok((ids_of(sk, &[a]), vec![k]))
    })?;
    Ok(result(out, info))
}

/// Initial radius for a circle near `at` touching the curves: the mean distance to them.
fn guess_radius(sk: &Sketch, curves: &[usize], at: Vec2) -> f64 {
    let ds: Vec<f64> = curves.iter().filter_map(|c| sk.shape(*c)).map(|sh| sh.project(at).dist(at)).collect();
    let r = ds.iter().sum::<f64>() / ds.len().max(1) as f64;
    if r.is_finite() && r > 1e-6 { r } else { 1.0 }
}

fn tangent_curves(sk: &Sketch, p: &Value, cmd: &str, n: usize) -> Result<Vec<usize>> {
    let ids = string_list(p, "curves");
    if ids.len() != n {
        return Err(bad(cmd, format!("`curves` must list {n} curves")));
    }
    ids.iter().map(|id| sk.curve_index(id).ok_or_else(|| bad(cmd, format!("unknown curve `{id}`")))).collect()
}

/// A circle tangent to `curves`, solved from a start near `near`.
fn tangent_circle(s: &mut Session, p: &Value, cmd: &str, n: usize, radius: Option<String>) -> Result<Value> {
    let near = req_vec2(cmd, p, "near")?;
    let (out, info) = edit(s, p, cmd, true, |sk, doc| {
        let cs = tangent_curves(sk, p, cmd, n)?;
        let r0 = match &radius {
            Some(e) => doc.eval(e, solidcraft_doc::expr::Kind::Length).map_err(|e| bad(cmd, format!("radius: {e}")))?,
            None => guess_radius(sk, &cs, near),
        };
        if !(r0 > 1e-9 && r0 < 1e8) {
            return Err(bad(cmd, "radius must be positive"));
        }
        // Start where the circle already nearly touches: move the centre so the guessed circle
        // sits between the curves.
        let first_new = sk.points.len();
        let c = sk.add_circle(near, r0, None, None)?;
        let mut cons = Vec::new();
        for k in &cs {
            cons.push(add_c(sk, ConstraintKind::Tangent { a: *k, b: c })?);
        }
        if let Some(e) = &radius {
            let pname = doc.new_model_param(e, "mm");
            let k = sk.add_constraint(ConstraintKind::Radius { c, value: r0 }, Some(pname))?;
            cons.push(k);
        }
        solve_new_only(sk, first_new);
        Ok((ids_of(sk, &[c]), cons))
    })?;
    if info["solved"] != json!(true) {
        return Err(bad(cmd, "no tangent circle there"));
    }
    Ok(result(out, info))
}

fn circle_tan_tan(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sketch.circle.two_tangent";
    let r = match (crate::params::expr(p, "radius"), num(p, "diameter")) {
        (Some(r), _) => r.into_string(),
        (None, Some(d)) => format!("{}", d / 2.0),
        _ => return Err(bad(cmd, "needs `radius` or `diameter`")),
    };
    tangent_circle(s, p, cmd, 2, Some(r))
}

fn circle_three_tan(s: &mut Session, p: &Value) -> Result<Value> {
    tangent_circle(s, p, "sketch.circle.three_tangent", 3, None)
}

fn width(p: &Value, cmd: &str) -> Result<f64> {
    num(p, "width").filter(|w| *w > 1e-9).ok_or_else(|| bad(cmd, "`width` must be positive"))
}

fn slot_center_point(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sketch.slot.center_point";
    let (c, e) = (req_vec2(cmd, p, "center")?, req_vec2(cmd, p, "end")?);
    let w = width(p, cmd)?;
    slot(s, p, cmd, c * 2.0 - e, e, w)
}

/// Arc slot around `o` (centre-line radius `r`) from angle `t0` sweeping `sweep` (> 0).
fn arc_slot(s: &mut Session, p: &Value, cmd: &str, o: Vec2, r: f64, t0: f64, sweep: f64, w: f64) -> Result<Value> {
    let h = w / 2.0;
    if h >= r {
        return Err(bad(cmd, "the slot is wider than its centre arc's diameter"));
    }
    if !(sweep > 1e-6 && sweep < std::f64::consts::TAU - 1e-6) {
        return Err(bad(cmd, "the centre arc must have a length"));
    }
    let t1 = t0 + sweep;
    let at = |t: f64, rr: f64| o + Vec2::from_angle(t) * rr;
    let (out, info) = edit(s, p, cmd, false, |sk, _| {
        let oc = sk.add_point(o, None)?;
        let ps = sk.add_point(at(t0, r), None)?;
        let pe = sk.add_point(at(t1, r), None)?;
        let (os, is) = (sk.add_point(at(t0, r + h), None)?, sk.add_point(at(t0, r - h), None)?);
        let (oe, ie) = (sk.add_point(at(t1, r + h), None)?, sk.add_point(at(t1, r - h), None)?);
        let outer = sk.add_arc(o, at(t0, r + h), at(t1, r + h), [Some(oc), Some(os), Some(oe)], None)?;
        let inner = sk.add_arc(o, at(t0, r - h), at(t1, r - h), [Some(oc), Some(is), Some(ie)], None)?;
        let cap_e = sk.add_arc(at(t1, r), at(t1, r + h), at(t1, r - h), [Some(pe), Some(oe), Some(ie)], None)?;
        let cap_s = sk.add_arc(at(t0, r), at(t0, r - h), at(t0, r + h), [Some(ps), Some(is), Some(os)], None)?;
        let centre = sk.add_arc(o, at(t0, r), at(t1, r), [Some(oc), Some(ps), Some(pe)], None)?;
        if let Some(c) = sk.curves.get_mut(centre) {
            c.construction = true;
        }
        let mut cons = Vec::new();
        for (a, b) in [(outer, cap_e), (inner, cap_e), (outer, cap_s), (inner, cap_s)] {
            cons.push(add_c(sk, ConstraintKind::Tangent { a, b })?);
        }
        cons.push(add_c(sk, ConstraintKind::Equal { a: cap_s, b: cap_e })?);
        Ok((ids_of(sk, &[outer, cap_e, inner, cap_s]), cons))
    })?;
    Ok(result(out, info))
}

fn arc_slot_three(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sketch.slot.arc_three_point";
    let (a, t, b) = (req_vec2(cmd, p, "start")?, req_vec2(cmd, p, "through")?, req_vec2(cmd, p, "end")?);
    let w = width(p, cmd)?;
    let (o, r) = circumcircle(a, t, b).ok_or_else(|| bad(cmd, "the points are collinear"))?;
    let ang = |q: Vec2| (q - o).angle();
    let span = |from: f64, to: f64| (to - from).rem_euclid(std::f64::consts::TAU);
    // Counter-clockwise from start must pass the through point; otherwise go from the end.
    let (t0, sweep) = if span(ang(a), ang(t)) < span(ang(a), ang(b)) { (ang(a), span(ang(a), ang(b))) } else { (ang(b), span(ang(b), ang(a))) };
    arc_slot(s, p, cmd, o, r, t0, sweep, w)
}

fn arc_slot_center(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sketch.slot.arc_center";
    let (o, a, b) = (req_vec2(cmd, p, "center")?, req_vec2(cmd, p, "start")?, req_vec2(cmd, p, "end")?);
    let w = width(p, cmd)?;
    let r = o.dist(a);
    if r < 1e-9 || o.dist(b) < 1e-9 {
        return Err(bad(cmd, "the points must differ from the centre"));
    }
    let t0 = (a - o).angle();
    let sweep = ((b - o).angle() - t0).rem_euclid(std::f64::consts::TAU);
    arc_slot(s, p, cmd, o, r, t0, sweep, w)
}

/// Solve moving only the points from `first_new` on (the geometry a tool just made), so the
/// existing drawing stays put when the new entities alone can satisfy the constraints. Falls
/// back to leaving everything to the normal solve.
pub(super) fn solve_new_only(sk: &mut Sketch, first_new: usize) {
    let mut trial = sk.clone();
    for (i, p) in trial.points.iter_mut().enumerate() {
        if i < first_new {
            p.fixed = true;
        }
    }
    if solidcraft_sketch::solve(&mut trial).ok() {
        for (p, t) in sk.points.iter_mut().zip(&trial.points).skip(first_new) {
            p.pos = t.pos;
        }
        for (c, t) in sk.curves.iter_mut().zip(&trial.curves) {
            if let (CurveKind::Circle { r, .. }, CurveKind::Circle { r: tr, .. }) = (&mut c.kind, &t.kind) {
                *r = *tr;
            }
        }
    }
}
