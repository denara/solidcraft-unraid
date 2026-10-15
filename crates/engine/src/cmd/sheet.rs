//! Sheet metal: rules, Flange (base, edge, contour), Hem, Unfold/Refold, Convert to Sheet Metal,
//! the flat pattern (outline, bend lines, bend table) and its DXF export.

use serde_json::{Value, json};
use solidcraft_doc::expr::Kind;
use solidcraft_doc::sheet::{self, SheetBody, SheetRule};
use solidcraft_doc::{FeatureKind, FlangeEdge};
use solidcraft_geom::{Vec2, Vec3};

use super::CommandSpec;
use super::features::{add_feature, check_expr, feature_sketch, profiles, sketch_id};
use crate::params::{bad, bool_, expr, req_expr, str_, string_list, vec3};
use crate::{EngineError, Result, Session};

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("sheet.flange", "Flange", flange).at("SHEET METAL", "CREATE").icon("flange").params(
        "base: sketch + profiles? (rule?, flip?) | contour: sketch + curves (open chain of lines) + distance (rule?, flip?, reverse?) | \
         edge: edges: [[x,y,z] on top/bottom edges, or {point: [x,y,z], angle?, height?} for an edge with its own angle and height] + \
         height (to the outer face), angle? (default 90 deg), radius? (default: rule), \
         position?: inside|outside|middle, flip?, body?; type?: base|edge|contour (default from the inputs); name?, body_name? (the new body). \
         Thickness, K-factor and bend radius come from the rule (sheet.manage_rules); passing them here is refused",
    ),
    CommandSpec::new("sheet.perforate", "Perforate", perforate).at("SHEET METAL", "CREATE").icon("hem").params(
        "diameter, pitch: expr; margin?: expr (clear of edges, cut-outs and bends; default: the diameter); pattern?: staggered (60°, default) | square; body? - a perforation field over the whole flat sheet: holes in the flat pattern (perforations), the DXF (layer PERFORATION) and the measured volume, not cut into the B-rep (#732)",
    ),
    CommandSpec::new("sheet.hem", "Hem", hem)
        .at("SHEET METAL", "CREATE")
        .icon("hem")
        .params("edges: [[x,y,z] on sheet edges]; length (to the outer face); gap? (inner radius, default: rule); flip?; body?"),
    CommandSpec::new("sheet.fold", "Fold", fold).at("SHEET METAL", "CREATE").icon("fold").params(
        "line: sketch + curve (a sketch line on the sheet's base face; followed when the sketch changes) or points: [[x,y,z], [x,y,z]]; \
         angle? (default 90 deg); radius? (default: rule); position?: centerline|start|end|mould (where the line sits in the bend, default centerline); \
         fixed?: [x,y,z] on the side that stays (default: the larger side); flip? (fold the other way); body?",
    ),
    CommandSpec::new("sheet.unfold", "Unfold", unfold).at("SHEET METAL", "MODIFY").icon("unfold").params("body? (default: the last sheet body) — all bends"),
    CommandSpec::new("sheet.refold", "Refold", refold).at("SHEET METAL", "MODIFY").icon("refold").params("body? (default: the last unfolded sheet)"),
    CommandSpec::new("sheet.convert", "Convert to Sheet Metal", convert)
        .at("SHEET METAL", "CREATE")
        .icon("convert_sheet")
        .params(
            "body; face: [x,y,z] on its large flat face (a plate of even thickness; recesses in one face up to half the thickness deep, \
             such as an engraved label, become pockets of the sheet), or that face's name from model.faces; rule?; \
             bends?: [[x,y,z] on sharp edges] (a solid or a shelled body: `face` is the fixed face and the flat faces reached from it \
             across these edges become the sheet; a shell keeps its thickness, a solid takes the rule's inward; \
             edges between two of those faces that are no bends are cut open with the rule's gap; \
             a point in the middle of a chamfer between two faces, or on a bend modelled with its radius (a cylinder between \
             two flat faces tangent to it, either skin), makes it one bend of the rule's radius at their virtual sharp; \
             the faces' holes become cut-outs and their engravings, up to half the thickness deep, pockets)",
        ),
    CommandSpec::new("sheet.manage_rules", "Sheet Metal Rules", rules).at("SHEET METAL", "MODIFY").icon("sheet_rules").params(
        "name (new or existing); thickness?, k_factor?, bend_radius?, relief_width?, relief_depth?, corner_relief?, hem_gap?, gap? (expressions; `Thickness` is the rule's; \
         gap: between flanges that meet at a corner, whose ends are closed toward each other at any angle); \
         corner_relief_shape?: notch|round|rectangular|tear (what is cut where two bends meet at a corner of the base: round and rectangular a relief \
         `corner_relief` across; tear none, each bend's end is cut straight from the corner to its flange's end; default notch); \
         active?: bool; delete?: bool",
    ),
    CommandSpec::new("sheet.rules", "List Sheet Metal Rules", list_rules).noundo().doc_only().params("→ rules with their expressions and values, and the active one"),
    CommandSpec::new("sheet.flat_pattern", "Create Flat Pattern", flat_pattern)
        .at("SHEET METAL", "CREATE")
        .icon("flat")
        .noundo()
        .params(
            "body? → flat size, outline loops and cut-outs (flat coordinates, as point lists), cutout_shapes (the same cut-outs exactly, in the same order: {circle: {center, radius}} for a round hole, else {segments: [{line: [a, b]} | {arc: {center, radius, start_deg, sweep_deg}} | {polyline: [...]}]}), pockets (with their own cutout_shapes), bend lines and the bend table",
        ),
    CommandSpec::new("sheet.export_dxf", "Export Flat Pattern as DXF", export_dxf)
        .noundo()
        .params("body?, path?, curves?: exact|arcs (default exact: an ellipse as ELLIPSE, splines, text outlines and other Béziers as SPLINE; arcs: tangent arcs and lines within 0.001 mm, for machines that read only those) → DXF (AutoCAD 2000, mm): OUTLINE, BEND_UP and BEND_DOWN (centre lines) layers, and pockets on POCKET_<BOTTOM|TOP|INTERNAL>_Z<from>_TO_<to>_MM"),
    CommandSpec::new("sheet.export_bend_table", "Export Bend Table", export_bend_table)
        .noundo()
        .params("body?, format?: csv|json (default csv), path? → bend table in mm and degrees, including rule and flat centre-line coordinates; a bend that cut-outs split has a row for each piece"),
];

fn points(p: &Value, k: &str, cmd: &str) -> Result<Vec<Vec3>> {
    let a = p.get(k).and_then(Value::as_array).ok_or_else(|| bad(cmd, format!("`{k}` must list points [x, y, z] on sheet edges")))?;
    if a.is_empty() || a.len() > 1000 {
        return Err(bad(cmd, format!("`{k}` must list 1…1000 points")));
    }
    a.iter()
        .map(|v| vec3(v).or_else(|| v.get("point").and_then(vec3)).ok_or_else(|| bad(cmd, format!("`{k}` must contain [x, y, z] points"))))
        .collect()
}

/// The `edges` of an edge flange: points `[x, y, z]` on sheet edges, or `{point, angle?, height?}`
/// for an edge with its own angle and height (expressions, checked like the feature's).
fn flange_edges(s: &Session, p: &Value, cmd: &str) -> Result<Vec<FlangeEdge>> {
    const KEYS: [&str; 3] = ["point", "angle", "height"];
    let list = "`edges` must list points [x, y, z] on sheet edges, or {point, angle?, height?}";
    let a = p.get("edges").and_then(Value::as_array).ok_or_else(|| bad(cmd, list))?;
    if a.is_empty() || a.len() > 1000 {
        return Err(bad(cmd, "`edges` must list 1…1000 points"));
    }
    let mut edges = Vec::with_capacity(a.len());
    for (i, v) in a.iter().enumerate() {
        // A bare point, as before: [x, y, z] or {x, y, z}.
        let Some(o) = v.as_object().filter(|o| KEYS.iter().any(|k| o.contains_key(*k))) else {
            edges.push(FlangeEdge::at(vec3(v).ok_or_else(|| bad(cmd, format!("edges[{i}]: {list}")))?));
            continue;
        };
        if let Some(k) = o.keys().find(|k| !KEYS.contains(&k.as_str())) {
            return Err(bad(cmd, format!("edges[{i}]: unknown key `{k}` (an edge takes point, angle and height)")));
        }
        let point = o.get("point").and_then(vec3).ok_or_else(|| bad(cmd, format!("edges[{i}]: `point` must be [x, y, z]")))?;
        let own = |k: &str, kind: Kind| -> Result<Option<String>> {
            if o.get(k).is_none_or(Value::is_null) {
                return Ok(None);
            }
            let e = expr(v, k).ok_or_else(|| bad(cmd, format!("edges[{i}]: `{k}` must be a number or an expression")))?;
            check_expr(s, &e, kind, cmd, &format!("edges[{i}].{k}"))?;
            Ok(Some(e))
        };
        edges.push(FlangeEdge { point, angle: own("angle", Kind::Angle)?, height: own("height", Kind::Length)? });
    }
    Ok(edges)
}

fn rule_param(s: &Session, p: &Value, cmd: &str) -> Result<Option<String>> {
    match str_(p, "rule") {
        Some(r) if s.doc.sheet.rules.iter().any(|x| x.name == r) || r == SheetRule::steel().name => Ok(Some(r.to_string())),
        Some(r) => Err(bad(cmd, format!("no sheet metal rule `{r}` (sheet.manage_rules makes one)"))),
        None => Ok(None),
    }
}

/// Sheet values a flange takes from its rule. Passing one to `sheet.flange` is refused rather
/// than ignored, so a script never gets a part of another thickness than it asked for.
const RULE_ONLY: [&str; 8] =
    ["thickness", "k_factor", "bend_radius", "relief_width", "relief_depth", "corner_relief", "corner_relief_shape", "hem_gap"];

fn flange(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sheet.flange";
    let nested = p.get("base").filter(|b| b.is_object());
    if let Some(key) = RULE_ONLY.iter().find(|k| p.get(**k).is_some() || nested.is_some_and(|b| b.get(**k).is_some())) {
        return Err(bad(
            cmd,
            format!(
                "`{key}` is not a sheet.flange parameter: the sheet metal rule sets it. Make a rule with \
                 sheet.manage_rules {{name, {key}, …}} and pass its name as `rule`"
            ),
        ));
    }
    let ty = match str_(p, "type") {
        Some(t) => t.to_ascii_lowercase(),
        None if p.get("edges").is_some() => "edge".into(),
        None if p.get("curves").is_some() => "contour".into(),
        None => "base".into(),
    };
    match ty.as_str() {
        "base" => {
            let sketch = feature_sketch(s, p, cmd)?;
            let rule = rule_param(s, p, cmd)?;
            add_feature(s, p, FeatureKind::SheetBase { sketch, profiles: profiles(p, cmd)?, rule, flip: bool_(p, "flip").unwrap_or(false) })
        }
        "contour" => {
            let sketch = sketch_id(s, p.get("sketch"), cmd, "sketch")?;
            let curves = string_list(p, "curves");
            if curves.is_empty() || curves.len() > 1000 {
                return Err(bad(cmd, "`curves` must list the open chain's lines"));
            }
            let distance = req_expr(cmd, p, "distance")?;
            check_expr(s, &distance, Kind::Length, cmd, "distance")?;
            let rule = rule_param(s, p, cmd)?;
            add_feature(
                s,
                p,
                FeatureKind::SheetContour {
                    sketch,
                    curves,
                    distance,
                    rule,
                    flip: bool_(p, "flip").unwrap_or(false),
                    reverse: bool_(p, "reverse").unwrap_or(false),
                },
            )
        }
        "edge" => {
            let edges = flange_edges(s, p, cmd)?;
            let height = req_expr(cmd, p, "height")?;
            check_expr(s, &height, Kind::Length, cmd, "height")?;
            let angle = expr(p, "angle").unwrap_or_else(|| "90 deg".into());
            check_expr(s, &angle, Kind::Angle, cmd, "angle")?;
            let radius = expr(p, "radius");
            if let Some(r) = &radius {
                check_expr(s, r, Kind::Length, cmd, "radius")?;
            }
            let position = str_(p, "position").unwrap_or("inside").to_ascii_lowercase();
            if !matches!(position.as_str(), "inside" | "outside" | "middle") {
                return Err(bad(cmd, "`position` must be inside, outside or middle"));
            }
            let body = str_(p, "body").map(str::to_string);
            add_feature(s, p, FeatureKind::SheetFlange { edges, height, angle, radius, position, flip: bool_(p, "flip").unwrap_or(false), body })
        }
        other => Err(bad(cmd, format!("unknown flange type `{other}` (base, edge or contour)"))),
    }
}

fn perforate(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sheet.perforate";
    let diameter = req_expr(cmd, p, "diameter")?;
    check_expr(s, &diameter, Kind::Length, cmd, "diameter")?;
    let pitch = req_expr(cmd, p, "pitch")?;
    check_expr(s, &pitch, Kind::Length, cmd, "pitch")?;
    let margin = expr(p, "margin");
    if let Some(m) = &margin {
        check_expr(s, m, Kind::Length, cmd, "margin")?;
    }
    let pattern = str_(p, "pattern").map(str::to_string);
    if pattern.as_deref().is_some_and(|t| t != "staggered" && t != "square") {
        return Err(bad(cmd, "`pattern` is \"staggered\" (60°, default) or \"square\""));
    }
    add_feature(s, p, FeatureKind::SheetPerforate { diameter, pitch, margin, pattern, body: str_(p, "body").map(str::to_string) })
}

fn hem(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sheet.hem";
    let edges = points(p, "edges", cmd)?;
    let length = req_expr(cmd, p, "length")?;
    check_expr(s, &length, Kind::Length, cmd, "length")?;
    let gap = expr(p, "gap");
    if let Some(g) = &gap {
        check_expr(s, g, Kind::Length, cmd, "gap")?;
    }
    add_feature(
        s,
        p,
        FeatureKind::SheetHem { edges, length, gap, flip: bool_(p, "flip").unwrap_or(false), body: str_(p, "body").map(str::to_string) },
    )
}

fn fold(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sheet.fold";
    let (sketch, curve, a, b) = match p.get("points") {
        Some(v) => {
            let pts = v.as_array().filter(|a| a.len() == 2).ok_or_else(|| bad(cmd, "`points` must be two points [x, y, z]"))?;
            let a = pts.first().and_then(vec3).ok_or_else(|| bad(cmd, "`points` must be two finite points [x, y, z]"))?;
            let b = pts.get(1).and_then(vec3).ok_or_else(|| bad(cmd, "`points` must be two finite points [x, y, z]"))?;
            (None, None, a, b)
        }
        None => {
            let sk = sketch_id(s, p.get("sketch"), cmd, "sketch")?;
            let c = str_(p, "curve").ok_or_else(|| bad(cmd, "give `sketch` + `curve` (a sketch line) or `points`"))?.to_string();
            let st = s.model.state();
            let ss = st.sketch(sk).ok_or_else(|| bad(cmd, "no such sketch"))?;
            let ci = ss.sketch.curve_index(&c).ok_or_else(|| bad(cmd, format!("no curve `{c}` in the sketch")))?;
            let (a, b) = match ss.sketch.segs(ci).as_slice() {
                [solidcraft_geom::Seg2::Line { a, b }] => (ss.plane.to_world(*a), ss.plane.to_world(*b)),
                _ => return Err(bad(cmd, "the fold line must be a sketch line")),
            };
            (Some(sk), Some(c), a, b)
        }
    };
    if a.dist(b) < 1e-9 {
        return Err(bad(cmd, "the fold line has no length"));
    }
    let angle = expr(p, "angle").unwrap_or_else(|| "90 deg".into());
    check_expr(s, &angle, Kind::Angle, cmd, "angle")?;
    let radius = expr(p, "radius");
    if let Some(r) = &radius {
        check_expr(s, r, Kind::Length, cmd, "radius")?;
    }
    let position = str_(p, "position").unwrap_or("centerline").to_ascii_lowercase();
    if !matches!(position.as_str(), "centerline" | "start" | "end" | "mould" | "mold") {
        return Err(bad(cmd, "`position` must be centerline, start, end or mould"));
    }
    let fixed = match p.get("fixed") {
        Some(v) => Some(vec3(v).ok_or_else(|| bad(cmd, "`fixed` must be a point [x, y, z]"))?),
        None => None,
    };
    let body = str_(p, "body").map(str::to_string);
    let r = add_feature(
        s,
        p,
        FeatureKind::SheetFold { sketch, curve, a, b, angle, radius, position, flip: bool_(p, "flip").unwrap_or(false), fixed, body },
    )?;
    // A recompute keeps a stored fold whose fixed point no longer touches the sheet (with a
    // warning); a new fold refuses one, since it picks no side (#263).
    if let Some(f) = fixed {
        let id = r.get("feature").and_then(Value::as_u64);
        let off =
            id.and_then(|id| s.model.result(id)).and_then(|res| res.warning.as_deref()).is_some_and(|w| w.contains(sheet::FOLD_FIXED_OFF_SHEET));
        if off {
            return Err(bad(
                cmd,
                format!("the fixed point ({:.3}, {:.3}, {:.3}) is not on the sheet: pick a point on the face of the side that stays", f.x, f.y, f.z),
            ));
        }
    }
    Ok(r)
}

fn unfold(s: &mut Session, p: &Value) -> Result<Value> {
    add_feature(s, p, FeatureKind::SheetUnfold { body: str_(p, "body").map(str::to_string), refold: false })
}

fn refold(s: &mut Session, p: &Value) -> Result<Value> {
    add_feature(s, p, FeatureKind::SheetUnfold { body: str_(p, "body").map(str::to_string), refold: true })
}

fn convert(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sheet.convert";
    let body = str_(p, "body").ok_or_else(|| bad(cmd, "`body` is required"))?.to_string();
    if s.model.state().body(&body).is_none() {
        return Err(bad(cmd, format!("no body `{body}`")));
    }
    let face = p.get("face").ok_or_else(|| bad(cmd, "`face` must be a point [x, y, z] on the plate's face, or its name from model.faces"))?;
    let face = super::face::face_ref(s, face, Some(&body), cmd)?;
    // With bend edges, a solid or shelled body; without, a plate.
    let bends = match p.get("bends") {
        None | Some(Value::Null) => Vec::new(),
        Some(_) => points(p, "bends", cmd)?,
    };
    // Such a sheet's values are its rule's (a plate's thickness is the plate's own).
    if let Some(key) = RULE_ONLY.iter().find(|k| !bends.is_empty() && p.get(**k).is_some()) {
        return Err(bad(cmd, format!("`{key}` is not a sheet.convert parameter: the sheet metal rule sets it. Pass a rule's name as `rule`")));
    }
    let rule = rule_param(s, p, cmd)?;
    add_feature(s, p, FeatureKind::SheetConvert { body, face, rule, bends })
}

fn rules(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sheet.manage_rules";
    let name = str_(p, "name").map(str::trim).filter(|n| !n.is_empty() && n.len() <= 128).ok_or_else(|| bad(cmd, "`name` is required"))?.to_string();
    if bool_(p, "delete").unwrap_or(false) {
        let doc = s.doc_mut();
        let before = doc.sheet.rules.len();
        doc.sheet.rules.retain(|r| r.name != name);
        if doc.sheet.rules.len() == before {
            return Err(bad(cmd, format!("no rule `{name}`")));
        }
        if doc.sheet.active.as_deref() == Some(name.as_str()) {
            doc.sheet.active = None;
        }
        return Ok(json!({"deleted": name}));
    }
    let mut rule =
        s.doc.sheet.rules.iter().find(|r| r.name == name).cloned().unwrap_or_else(|| SheetRule { name: name.clone(), ..s.doc.sheet_rule(None) });
    for (k, slot) in [
        ("thickness", &mut rule.thickness),
        ("k_factor", &mut rule.k_factor),
        ("bend_radius", &mut rule.bend_radius),
        ("relief_width", &mut rule.relief_width),
        ("relief_depth", &mut rule.relief_depth),
        ("corner_relief", &mut rule.corner_relief),
        ("hem_gap", &mut rule.hem_gap),
        ("gap", &mut rule.gap),
    ] {
        if let Some(e) = expr(p, k) {
            *slot = e;
        }
    }
    if let Some(shape) = p.get("corner_relief_shape").filter(|v| !v.is_null()) {
        let shape = shape.as_str().and_then(solidcraft_doc::sheet::ReliefShape::parse);
        rule.corner_relief_shape = shape.ok_or_else(|| bad(cmd, "`corner_relief_shape` must be notch, round, rectangular or tear"))?.name().into();
    }
    let (vals, _) = s.doc.param_values();
    let values = s.doc.rule_values(&vals, &rule).map_err(|e| bad(cmd, e.to_string()))?;
    let doc = s.doc_mut();
    match doc.sheet.rules.iter_mut().find(|r| r.name == name) {
        Some(r) => *r = rule.clone(),
        None => {
            if doc.sheet.rules.len() >= 1000 {
                return Err(EngineError::Other("too many rules".into()));
            }
            doc.sheet.rules.push(rule.clone());
        }
    }
    if bool_(p, "active").unwrap_or(doc.sheet.active.is_none()) {
        doc.sheet.active = Some(name.clone());
    }
    s.refresh();
    let errors: Vec<Value> = s.model.results.iter().filter_map(|r| r.error.as_ref().map(|e| json!({"feature": r.name, "error": e}))).collect();
    Ok(json!({"rule": rule, "values": values, "errors": errors}))
}

fn list_rules(s: &mut Session, _p: &Value) -> Result<Value> {
    let (vals, _) = s.doc.param_values();
    let mut all = s.doc.sheet.rules.clone();
    if all.is_empty() {
        all.push(SheetRule::steel());
    }
    let rules: Vec<Value> = all.iter().map(|r| json!({"rule": r, "values": s.doc.rule_values(&vals, r).ok()})).collect();
    let active = s.doc.sheet_rule(None).name;
    Ok(json!({"rules": rules, "active": active}))
}

fn sheet_of(s: &Session, p: &Value, cmd: &str) -> Result<SheetBody> {
    let st = s.model.state();
    match str_(p, "body") {
        Some(b) => st.sheets.iter().find(|x| x.body == b).cloned().ok_or_else(|| bad(cmd, format!("`{b}` is not a sheet metal body"))),
        None => st.sheets.last().cloned().ok_or_else(|| bad(cmd, "there is no sheet metal body")),
    }
}

/// The centre and radius of a loop that is one whole circle (often two semicircles).
fn loop_circle(lp: &solidcraft_geom::Loop2) -> Option<(Vec2, f64)> {
    use solidcraft_geom::Seg2;
    let Some(Seg2::Arc { center, radius, sweep, .. }) = lp.segs.first() else { return None };
    let total: f64 = lp.segs.iter().filter_map(|s| if let Seg2::Arc { sweep, .. } = s { Some(*sweep) } else { None }).sum();
    let circular = lp.segs.iter().all(
        |s| matches!(s, Seg2::Arc { center: c, radius: r, sweep: a, .. } if c.dist(*center) < 1e-9 && (*r - *radius).abs() < 1e-9 && a.signum() == sweep.signum()),
    );
    (circular && (total.abs() - std::f64::consts::TAU).abs() < 1e-9).then_some((*center, *radius))
}

/// A loop's exact shape for the flat pattern JSON: a circle, or its lines and arcs (other
/// curves as point lists at the flat pattern's 0.001 mm tolerance).
fn loop_shape(lp: &solidcraft_geom::Loop2) -> Value {
    use solidcraft_geom::Seg2;
    let pt = |v: Vec2| json!([v.x, v.y]);
    if let Some((center, radius)) = loop_circle(lp) {
        return json!({"circle": {"center": pt(center), "radius": radius}});
    }
    let segments: Vec<Value> = lp
        .segs
        .iter()
        .map(|seg| match *seg {
            Seg2::Line { a, b } => json!({"line": [pt(a), pt(b)]}),
            Seg2::Arc { center, radius, start, sweep } => {
                json!({"arc": {"center": pt(center), "radius": radius, "start_deg": start.to_degrees(), "sweep_deg": sweep.to_degrees()}})
            }
            ref other => json!({"polyline": other.polyline(1e-3).into_iter().map(pt).collect::<Vec<_>>()}),
        })
        .collect();
    json!({"segments": segments})
}

fn flat_json(sh: &SheetBody) -> Result<Value> {
    let regions = sh.flat_regions()?;
    let loops: Vec<Vec<Vec2>> = regions.iter().map(|r| r.outer.polyline(1e-3)).collect();
    let (mut lo, mut hi) = (Vec2::new(f64::MAX, f64::MAX), Vec2::new(f64::MIN, f64::MIN));
    for p in loops.iter().flatten() {
        lo = Vec2::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Vec2::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    let (w, h) = (hi.x - lo.x, hi.y - lo.y);
    let pt = |v: &Vec2| json!([v.x, v.y]);
    let outline: Vec<Value> = loops.iter().map(|l| Value::Array(l.iter().map(pt).collect())).collect();
    let holes: Vec<Value> = regions.iter().flat_map(|r| &r.holes).map(|h| Value::Array(h.polyline(1e-3).iter().map(pt).collect())).collect();
    let hole_shapes: Vec<Value> = regions.iter().flat_map(|r| &r.holes).map(loop_shape).collect();
    let bends: Vec<Value> = sh
        .bends()
        .iter()
        .map(|b| {
            json!({"bend": b.index, "angle_deg": b.angle_deg, "radius": b.radius, "direction": if b.up { "up" } else { "down" },
                "allowance": b.allowance, "length": b.length, "start": pt(&b.start), "end": pt(&b.end)})
        })
        .collect();
    let pockets: Vec<Value> = sh
        .machining()?
        .iter()
        .map(|p| {
            let side = if p.z0 == 0.0 {
                "bottom"
            } else if p.z1 == sh.t {
                "top"
            } else {
                "internal"
            };
            json!({"outline": p.region.outer.polyline(1e-3).iter().map(pt).collect::<Vec<_>>(),
            "cutouts": p.region.holes.iter().map(|h| h.polyline(1e-3).iter().map(pt).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "cutout_shapes": p.region.holes.iter().map(loop_shape).collect::<Vec<_>>(),
            "side": side, "depth_mm": p.z1-p.z0, "z0_mm": p.z0, "z1_mm": p.z1})
        })
        .collect();
    // Holes drilled along a panel into its rim: drilled after cutting, not in the outline.
    let bores: Vec<Value> = sh
        .bores
        .iter()
        .map(|b| json!({"start": [b.base.x, b.base.y, b.base.z], "axis": [b.axis.x, b.axis.y, b.axis.z], "diameter": b.radius * 2.0, "length": b.height.abs()}))
        .collect();
    let mut perforations = Vec::new();
    for p in &sh.perforations {
        let n = sh.perforation_holes(p)?.len();
        perforations.push(json!({"feature": p.feature, "diameter": p.diameter, "pitch": p.pitch, "margin": p.margin,
            "pattern": if p.square { "square" } else { "staggered" }, "count": n,
            "open_area_mm2": n as f64 * std::f64::consts::PI * (p.diameter / 2.0).powi(2)}));
    }
    let bend_zones: Vec<Value> = sh
        .bend_zones()
        .iter()
        .map(|z| {
            let pts = z.outer.polyline(1e-3);
            let f = |g: fn(&Vec2) -> f64, m: fn(f64, f64) -> f64, s: f64| pts.iter().map(g).fold(s, m);
            json!([f(|p| p.x, f64::min, f64::MAX), f(|p| p.x, f64::max, f64::MIN), f(|p| p.y, f64::min, f64::MAX), f(|p| p.y, f64::max, f64::MIN)])
        })
        .collect();
    let mut out = json!({"body": sh.body, "rule": sh.rule, "thickness": sh.t, "k_factor": sh.k, "flat_size_mm": [w, h, sh.t], "outline": outline, "cutouts": holes, "cutout_shapes": hole_shapes, "pockets": pockets, "bends": bends, "unfolded": sh.flat,
        "perforations": perforations, "bend_zones": bend_zones});
    if !bores.is_empty() {
        out["bores"] = Value::Array(bores);
    }
    Ok(out)
}

fn flat_pattern(s: &mut Session, p: &Value) -> Result<Value> {
    let sh = sheet_of(s, p, "sheet.flat_pattern")?;
    flat_json(&sh)
}

/// How the flat DXF writes the curves that are neither lines nor circular arcs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DxfCurves {
    /// Exactly: an ellipse as ELLIPSE, Béziers (spline pieces, text outlines) and the other conics as SPLINE.
    #[default]
    Exact,
    /// As tangent arcs and lines within 0.001 mm, for machines that read only those.
    Arcs,
}

/// A DXF (AutoCAD 2000, mm) of the flat pattern on named layers: OUTLINE, BEND_UP and BEND_DOWN
/// (centre lines), and one layer per pocket depth. Free-form curves as `curves` says.
pub fn flat_dxf(sh: &SheetBody, curves: DxfCurves) -> Result<String> {
    use solidcraft_io::{DxfEntity, DxfLayer, DxfShape};
    let regions = sh.flat_regions()?;
    let pockets = sh.machining()?;
    let pocket_layer = |p: &solidcraft_doc::sheet::SheetPocket| {
        let side = if p.z0 == 0.0 {
            "BOTTOM"
        } else if p.z1 == sh.t {
            "TOP"
        } else {
            "INTERNAL"
        };
        format!("POCKET_{side}_Z{:.6}_TO_{:.6}_MM", p.z0, p.z1)
    };
    let mut layers = std::collections::BTreeSet::from(["OUTLINE".to_string(), "BEND_UP".to_string(), "BEND_DOWN".to_string()]);
    for p in &pockets {
        layers.insert(pocket_layer(p));
    }
    let perforation: Vec<(Vec2, f64)> = sh
        .perforations
        .iter()
        .map(|p| sh.perforation_holes(p).map(|h| h.into_iter().map(|c| (c, p.diameter / 2.0)).collect::<Vec<_>>()))
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect();
    if !perforation.is_empty() {
        layers.insert("PERFORATION".to_string());
    }
    let layers: Vec<DxfLayer> = layers
        .into_iter()
        .map(|name| {
            let color = match name.as_str() {
                "OUTLINE" => 7,
                "BEND_UP" => 1,
                "BEND_DOWN" => 5,
                _ => 3,
            };
            DxfLayer { name, color }
        })
        .collect();
    let mut loops: Vec<_> = regions.iter().flat_map(|r| std::iter::once(&r.outer).chain(&r.holes)).map(|lp| (lp, "OUTLINE".to_string())).collect();
    for p in &pockets {
        for lp in std::iter::once(&p.region.outer).chain(&p.region.holes) {
            loops.push((lp, pocket_layer(p)));
        }
    }
    let mut entities = Vec::new();
    for (h, layer) in loops {
        // A circle is often represented by two semicircles. Keep the whole analytic
        // boundary as one CIRCLE rather than approximating outer boundaries with chords.
        if let Some((center, radius)) = loop_circle(h) {
            entities.push(DxfEntity { layer, shape: DxfShape::Circle { c: center, r: radius } });
            continue;
        }
        let fitted;
        let h = match curves {
            DxfCurves::Arcs if !h.is_line_arc() => {
                fitted = h
                    .fit_arcs(solidcraft_kernel::PLANAR_FIT_TOL)
                    .ok_or_else(|| EngineError::Other("a flat-pattern curve could not be written as arcs".into()))?;
                &fitted
            }
            _ => h,
        };
        // A whole ellipse is one ELLIPSE; every other segment is an entity of its own.
        entities.extend(solidcraft_io::loop_shapes(h).into_iter().map(|shape| DxfEntity { layer: layer.clone(), shape }));
    }
    for b in sh.bends() {
        entities.push(DxfEntity { layer: if b.up { "BEND_UP" } else { "BEND_DOWN" }.into(), shape: DxfShape::Line(b.start, b.end) });
    }
    for (c, r) in perforation {
        entities.push(DxfEntity { layer: "PERFORATION".into(), shape: DxfShape::Circle { c, r } });
    }
    Ok(solidcraft_io::write_dxf_drawing(&layers, &entities))
}

fn export_dxf(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sheet.export_dxf";
    let curves = match str_(p, "curves").map(str::to_ascii_lowercase).as_deref() {
        None | Some("exact") => DxfCurves::Exact,
        Some("arcs") => DxfCurves::Arcs,
        Some(_) => return Err(bad(cmd, "`curves` must be exact or arcs")),
    };
    let sh = sheet_of(s, p, cmd)?;
    let text = flat_dxf(&sh, curves)?;
    if let Some(path) = str_(p, "path").filter(|x| !x.trim().is_empty()) {
        solidcraft_io::vfs::write(path, text.as_bytes()).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
        return Ok(json!({"path": path, "bends": sh.bends().len()}));
    }
    Ok(json!({"dxf": text, "bends": sh.bends().len()}))
}

fn export_bend_table(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sheet.export_bend_table";
    let format = str_(p, "format").unwrap_or("csv").to_ascii_lowercase();
    if !matches!(format.as_str(), "csv" | "json") {
        return Err(bad(cmd, "`format` must be csv or json"));
    }
    let sh = sheet_of(s, p, cmd)?;
    let table = flat_json(&sh)?;
    let text = if format == "json" {
        serde_json::to_string_pretty(&table).map_err(|e| EngineError::Other(e.to_string()))?
    } else {
        // Always quote text fields: body/rule names may include commas, quotes or newlines.
        let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let mut csv =
            "body,rule,thickness_mm,k_factor,bend,angle_deg,direction,radius_mm,allowance_mm,length_mm,start_x_mm,start_y_mm,end_x_mm,end_y_mm\r\n"
                .to_string();
        for b in sh.bends() {
            csv.push_str(&format!(
                "{},{},{:.9},{:.9},{},{:.9},{},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9},{:.9}\r\n",
                quote(&sh.body),
                quote(&sh.rule),
                sh.t,
                sh.k,
                b.index,
                b.angle_deg,
                if b.up { "up" } else { "down" },
                b.radius,
                b.allowance,
                b.length,
                b.start.x,
                b.start.y,
                b.end.x,
                b.end.y
            ));
        }
        csv
    };
    if let Some(path) = str_(p, "path").filter(|x| !x.trim().is_empty()) {
        solidcraft_io::vfs::write(path, text.as_bytes()).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
        return Ok(json!({"path": path, "format": format, "bends": sh.bends().len()}));
    }
    Ok(json!({"format": format, "text": text, "bends": sh.bends().len()}))
}

#[cfg(test)]
#[path = "sheet_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "sheet_convert_tests.rs"]
mod convert_tests;

#[cfg(test)]
#[path = "sheet_dxf_tests.rs"]
mod dxf_tests;
