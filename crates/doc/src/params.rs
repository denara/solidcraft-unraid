//! Parameters: user parameters, model parameters of sketch dimensions, and feature inputs
//! (extrude distance, fillet radius…) which also appear as model parameters `dN` without being
//! stored twice: the feature keeps its expression and the parameter name refers to it. Every
//! parameter can be referenced by name from any expression; the dependency graph and cycles
//! come from the expression references.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::expr::{self, Kind, ParamDef, Value};
use crate::{DocError, Document, FeatureKind, HoleKind, MAX_PARAMS, PatternKind, PlaneRef, Result};

/// Whether an expression of a feature is one of its inputs, which become model parameters named
/// in walk order (`dN`), or a coordinate of a primitive's placement point, which gets no name and
/// keeps bare numbers in mm whatever the design's units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExprRole {
    Input,
    Placement,
}

/// The offsets and angles of a plane, outermost first, for `&` and `&mut` (`$m` is `mut` or
/// nothing). Nesting is capped like every walk over a plane.
macro_rules! plane_walk {
    ($name:ident $(, $m:tt)?) => {
        fn $name<'a>(mut p: &'a $($m)? PlaneRef, f: &mut impl FnMut(&'static str, &'a $($m)? String, Kind, ExprRole)) {
            for _ in 0..64 {
                match p {
                    PlaneRef::Offset { base, distance } => {
                        f("Offset", distance, Kind::Length, ExprRole::Input);
                        p = base;
                    }
                    PlaneRef::AtAngle { base, angle, .. } => {
                        f("Angle", angle, Kind::Angle, ExprRole::Input);
                        p = base;
                    }
                    PlaneRef::AlongPath { t, .. } => {
                        f("Distance (0 to 1)", t, Kind::Unitless, ExprRole::Input);
                        break;
                    }
                    _ => break,
                }
            }
        }
    };
}

plane_walk!(plane_exprs);
plane_walk!(plane_exprs_mut, mut);

/// Every expression of a feature with its label, kind and role, in a fixed order (the inputs'
/// order names their parameters), written once for `&` and `&mut`: `$plane` is the matching plane
/// walk, `$refs` the matching `refs`/`refs_mut`, `$m` is `mut` or nothing.
macro_rules! feature_walk {
    ($name:ident, $plane:ident, $refs:ident $(, $m:tt)?) => {
        pub(crate) fn $name<'a>(&'a $($m)? self, f: &mut impl FnMut(&'static str, &'a $($m)? String, Kind, ExprRole)) {
            use ExprRole::{Input as I, Placement as P};
            use Kind::{Angle as A, Length as L, Unitless as U};
            const XYZ: [&str; 3] = ["X", "Y", "Z"];
            match self {
                FeatureKind::Sketch { plane, .. }
                | FeatureKind::ConstructionPlane { plane }
                | FeatureKind::Split { plane, .. }
                | FeatureKind::SplitFace { plane, .. }
                | FeatureKind::Mirror { plane, .. } => $plane(plane, f),
                FeatureKind::ConstructionAxis { def } => {
                    for r in def.$refs() {
                        if let crate::construct::GeoRef::Plane { plane } = r {
                            $plane(plane, f);
                        }
                    }
                }
                FeatureKind::ConstructionPoint { def } => match def {
                    crate::construct::PointDef::AlongPath { t, .. } => f("Distance (0 to 1)", t, U, I),
                    other => {
                        for r in other.$refs() {
                            if let crate::construct::GeoRef::Plane { plane } = r {
                                $plane(plane, f);
                            }
                        }
                    }
                },
                FeatureKind::Extrude { extent, thin, .. } => {
                    f("Distance", & $($m)? extent.distance, L, I);
                    if let Some(d) = & $($m)? extent.distance2 {
                        f("Distance 2", d, L, I);
                    }
                    if let Some(d) = & $($m)? extent.start_offset {
                        f("Start offset", d, L, I);
                    }
                    if let Some(d) = & $($m)? extent.taper {
                        f("Taper angle", d, A, I);
                    }
                    if let Some(d) = & $($m)? extent.to_offset {
                        f("To offset", d, L, I);
                    }
                    if let Some(thin) = thin {
                        f("Thickness", & $($m)? thin.thickness, L, I);
                        if let Some(t) = & $($m)? thin.thickness2 {
                            f("Thickness 2", t, L, I);
                        }
                    }
                }
                FeatureKind::SurfaceExtrude { extent, .. } => {
                    f("Distance", & $($m)? extent.distance, L, I);
                    if let Some(d) = & $($m)? extent.distance2 {
                        f("Distance 2", d, L, I);
                    }
                    if let Some(d) = & $($m)? extent.start_offset {
                        f("Start offset", d, L, I);
                    }
                    if let Some(d) = & $($m)? extent.taper {
                        f("Taper angle", d, A, I);
                    }
                    if let Some(d) = & $($m)? extent.to_offset {
                        f("To offset", d, L, I);
                    }
                }
                FeatureKind::Revolve { angle, angle2, .. } | FeatureKind::SurfaceRevolve { angle, angle2, .. } => {
                    f("Angle", angle, A, I);
                    if let Some(a) = angle2 {
                        f("Angle 2", a, A, I);
                    }
                }
                FeatureKind::Fillet { radius, style, .. } => match style {
                    crate::FilletStyle::Constant => f("Radius", radius, L, I),
                    crate::FilletStyle::Curvature => f("Equivalent radius", radius, L, I),
                    crate::FilletStyle::Chord => f("Chord length", radius, L, I),
                    crate::FilletStyle::Variable { radius2, mid, .. } => {
                        f("Start radius", radius, L, I);
                        f("End radius", radius2, L, I);
                        for (_, r) in mid {
                            f("Mid radius", r, L, I);
                        }
                    }
                    crate::FilletStyle::Spline { radii, .. } => {
                        f("Start radius", radius, L, I);
                        let n = radii.len();
                        for (i, r) in radii.into_iter().enumerate() {
                            f(if i + 1 == n { "End radius" } else { "Control radius" }, r, L, I);
                        }
                    }
                },
                FeatureKind::Chamfer { distance, distance2, angle, .. } => {
                    f("Distance", distance, L, I);
                    if let Some(d) = distance2 {
                        f("Distance 2", d, L, I);
                    }
                    if let Some(a) = angle {
                        f("Angle", a, A, I);
                    }
                }
                FeatureKind::Box { length, width, height, corner, .. } => {
                    f("Length", length, L, I);
                    f("Width", width, L, I);
                    f("Height", height, L, I);
                    for (l, c) in XYZ.into_iter().zip(corner) {
                        f(l, c, L, P);
                    }
                }
                FeatureKind::Cylinder { radius, height, base, .. } => {
                    f("Radius", radius, L, I);
                    f("Height", height, L, I);
                    for (l, c) in XYZ.into_iter().zip(base) {
                        f(l, c, L, P);
                    }
                }
                FeatureKind::Sphere { radius, center, .. } => {
                    f("Radius", radius, L, I);
                    for (l, c) in XYZ.into_iter().zip(center) {
                        f(l, c, L, P);
                    }
                }
                FeatureKind::Torus { major, minor, center, .. } => {
                    f("Major radius", major, L, I);
                    f("Minor radius", minor, L, I);
                    for (l, c) in XYZ.into_iter().zip(center) {
                        f(l, c, L, P);
                    }
                }
                FeatureKind::Pattern { pattern, .. } => match pattern {
                    PatternKind::Rectangular { count1, spacing1, count2, spacing2, .. } => {
                        f("Quantity", count1, U, I);
                        f("Spacing", spacing1, L, I);
                        if let Some(c) = count2 {
                            f("Quantity 2", c, U, I);
                        }
                        if let Some(sp) = spacing2 {
                            f("Spacing 2", sp, L, I);
                        }
                    }
                    PatternKind::Path { count, spacing, .. } => {
                        f("Quantity", count, U, I);
                        f("Distance", spacing, L, I);
                    }
                    PatternKind::Circular { count, angle, .. } => {
                        f("Quantity", count, U, I);
                        f("Total angle", angle, A, I);
                    }
                },
                FeatureKind::Shell { thickness, .. } => f("Thickness", thickness, L, I),
                FeatureKind::Draft { angle, neutral, .. } => {
                    f("Angle", angle, A, I);
                    $plane(neutral, f);
                }
                FeatureKind::Hole { diameter, depth, hole, .. } => {
                    f("Diameter", diameter, L, I);
                    if let Some(d) = depth {
                        f("Depth", d, L, I);
                    }
                    match hole {
                        HoleKind::Simple => {}
                        HoleKind::Drilled { tip_angle } => f("Tip angle", tip_angle, A, I),
                        HoleKind::Counterbore { cb_diameter, cb_depth } => {
                            f("Counterbore diameter", cb_diameter, L, I);
                            f("Counterbore depth", cb_depth, L, I);
                        }
                        HoleKind::Countersink { cs_diameter, cs_angle } => {
                            f("Countersink diameter", cs_diameter, L, I);
                            f("Countersink angle", cs_angle, A, I);
                        }
                    }
                }
                FeatureKind::Thread { length, .. } => {
                    if let Some(l) = length {
                        f("Length", l, L, I);
                    }
                }
                FeatureKind::Scale { factor, factors, .. } => {
                    f("Scale factor", factor, U, I);
                    if let Some([x, y, z]) = factors {
                        f("X scale", x, U, I);
                        f("Y scale", y, U, I);
                        f("Z scale", z, U, I);
                    }
                }
                FeatureKind::OffsetFace { distance, .. } => f("Distance", distance, L, I),
                FeatureKind::BoundingSolid { margin, .. } => f("Margin", margin, L, I),
                FeatureKind::Pipe { diameter, wall, .. } => {
                    f("Diameter", diameter, L, I);
                    if let Some(w) = wall {
                        f("Wall", w, L, I);
                    }
                }
                FeatureKind::Move { translate, angle, .. } => {
                    let [x, y, z] = translate;
                    f("X distance", x, L, I);
                    f("Y distance", y, L, I);
                    f("Z distance", z, L, I);
                    if let Some(a) = angle {
                        f("Angle", a, A, I);
                    }
                }
                FeatureKind::Emboss { depth, .. } => f("Depth", depth, L, I),
                FeatureKind::Boss { diameter, height, hole_diameter, hole_depth, draft, fillet, ribs, rib_thickness, rib_length, rib_offset, .. } => {
                    f("Diameter", diameter, L, I);
                    f("Height", height, L, I);
                    for (l, o, k) in [
                        ("Hole diameter", hole_diameter, L),
                        ("Hole depth", hole_depth, L),
                        ("Draft angle", draft, A),
                        ("Root fillet", fillet, L),
                        ("Ribs", ribs, U),
                        ("Rib thickness", rib_thickness, L),
                        ("Rib length", rib_length, L),
                        ("Rib offset", rib_offset, L),
                    ] {
                        if let Some(e) = o {
                            f(l, e, k, I);
                        }
                    }
                }
                FeatureKind::Lip { width, height, gap, .. } => {
                    f("Width", width, L, I);
                    f("Height", height, L, I);
                    if let Some(g) = gap {
                        f("Gap", g, L, I);
                    }
                }
                FeatureKind::Rest { width, length, height, draft, thickness, .. } => {
                    f("Width", width, L, I);
                    f("Height", height, L, I);
                    for (l, o, k) in [("Length", length, L), ("Draft angle", draft, A), ("Wall thickness", thickness, L)] {
                        if let Some(e) = o {
                            f(l, e, k, I);
                        }
                    }
                }
                FeatureKind::SnapFit { length, thickness, width, catch_depth, catch_length, .. } => {
                    f("Length", length, L, I);
                    f("Thickness", thickness, L, I);
                    f("Width", width, L, I);
                    f("Catch depth", catch_depth, L, I);
                    f("Catch length", catch_length, L, I);
                }
                FeatureKind::SheetContour { distance, .. } => f("Distance", distance, L, I),
                FeatureKind::SheetFlange { edges, height, angle, radius, .. } => {
                    f("Height", height, L, I);
                    f("Angle", angle, A, I);
                    if let Some(r) = radius {
                        f("Bend radius", r, L, I);
                    }
                    // The edges' own values come last, so the inputs above keep their names.
                    for e in edges {
                        if let Some(a) = & $($m)? e.angle {
                            f("Edge angle", a, A, I);
                        }
                        if let Some(h) = & $($m)? e.height {
                            f("Edge height", h, L, I);
                        }
                    }
                }
                FeatureKind::SheetFold { angle, radius, .. } => {
                    f("Angle", angle, A, I);
                    if let Some(r) = radius {
                        f("Bend radius", r, L, I);
                    }
                }
                FeatureKind::SheetHem { length, gap, .. } => {
                    f("Length", length, L, I);
                    if let Some(g) = gap {
                        f("Gap", g, L, I);
                    }
                }
                FeatureKind::Coil { diameter, pitch, turns, section_size, start_angle, base, .. } => {
                    f("Diameter", diameter, L, I);
                    f("Pitch", pitch, L, I);
                    f("Revolutions", turns, U, I);
                    f("Section size", section_size, L, I);
                    if let Some(a) = start_angle {
                        f("Start angle", a, A, I);
                    }
                    for (l, c) in XYZ.into_iter().zip(base) {
                        f(l, c, L, P);
                    }
                }
                FeatureKind::Rib { thickness, depth, .. } => {
                    f("Thickness", thickness, L, I);
                    if let Some(d) = depth {
                        f("Depth", d, L, I);
                    }
                }
                FeatureKind::ReplaceFace { target, .. } => $plane(target, f),
                FeatureKind::Stitch { tolerance, .. } => f("Tolerance", tolerance, L, I),
                FeatureKind::MeshConvert { tolerance, .. } => f("Tolerance", tolerance, L, I),
                FeatureKind::Thicken { thickness, .. } => f("Thickness", thickness, L, I),
                FeatureKind::SurfaceTrim { plane, .. } => $plane(plane, f),
                FeatureKind::SurfaceExtend { distance, .. } => f("Distance", distance, L, I),
                FeatureKind::Unstitch { .. } | FeatureKind::ReverseNormal { .. } => {}
                FeatureKind::SurfaceOffset { distance, tolerance, .. } => {
                    f("Distance", distance, L, I);
                    if let Some(t) = tolerance {
                        f("Tolerance", t, L, I);
                    }
                }
                FeatureKind::Patch { .. } => {}
                FeatureKind::Combine { .. }
                | FeatureKind::SheetBase { .. }
                | FeatureKind::SheetUnfold { .. }
                | FeatureKind::SheetConvert { .. }
                | FeatureKind::Align { .. }
                | FeatureKind::Remove { .. }
                | FeatureKind::DeleteFace { .. }
                | FeatureKind::BoundaryFill { .. }
                | FeatureKind::Loft { .. }
                | FeatureKind::Sweep { .. }
                | FeatureKind::SurfaceSweep { .. }
                | FeatureKind::SurfaceLoft { .. }
                | FeatureKind::Import { .. }
                | FeatureKind::MeshImport { .. }
                | FeatureKind::BodyTransfer { .. } => {}
            }
        }
    };
}

impl FeatureKind {
    feature_walk!(walk_exprs, plane_exprs, refs);
    feature_walk!(walk_exprs_mut, plane_exprs_mut, refs_mut, mut);

    /// The feature's numeric inputs: (label, expression, kind), in a fixed order.
    pub fn inputs_mut(&mut self) -> Vec<(&'static str, &mut String, Kind)> {
        let mut v = Vec::new();
        self.walk_exprs_mut(&mut |label, e, kind, role| {
            if role == ExprRole::Input {
                v.push((label, e, kind));
            }
        });
        v
    }

    /// Read-only view of [`FeatureKind::inputs_mut`]: (label, expression, kind).
    pub fn inputs(&self) -> Vec<(&'static str, String, Kind)> {
        let mut v = Vec::new();
        self.walk_exprs(&mut |label, e, kind, role| {
            if role == ExprRole::Input {
                v.push((label, e.clone(), kind));
            }
        });
        v
    }

    /// Every expression the feature uses: its inputs, then a primitive's placement coordinates.
    pub fn expressions(&self) -> Vec<&str> {
        let mut v = Vec::new();
        self.walk_exprs(&mut |_, e, _, _| v.push(e.as_str()));
        v
    }

    /// A primitive's placement point (length expressions). Its coordinates are not inputs:
    /// they get no parameter names and keep bare numbers in mm whatever the design's units, as
    /// every other position does; renames and users still find them.
    pub fn point_expr_mut(&mut self) -> Option<&mut crate::PointExpr> {
        match self {
            FeatureKind::Box { corner: p, .. }
            | FeatureKind::Cylinder { base: p, .. }
            | FeatureKind::Sphere { center: p, .. }
            | FeatureKind::Torus { center: p, .. }
            | FeatureKind::Coil { base: p, .. } => Some(p),
            _ => None,
        }
    }

    /// Read-only view of [`FeatureKind::point_expr_mut`].
    pub fn point_expr(&self) -> Option<&crate::PointExpr> {
        match self {
            FeatureKind::Box { corner: p, .. }
            | FeatureKind::Cylinder { base: p, .. }
            | FeatureKind::Sphere { center: p, .. }
            | FeatureKind::Torus { center: p, .. }
            | FeatureKind::Coil { base: p, .. } => Some(p),
            _ => None,
        }
    }
}

/// One row of the parameters table.
#[derive(Clone, Debug, Serialize)]
pub struct ParamRow {
    pub name: String,
    pub expression: String,
    pub unit: String,
    pub value: Option<f64>,
    pub error: Option<String>,
    pub comment: String,
    /// "user", "sketch" (a dimension) or "feature".
    pub source: &'static str,
    /// For feature inputs: the feature and the input's label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<&'static str>,
    pub favorite: bool,
    /// Names this one's expression uses, and names whose expressions use this one.
    pub uses: Vec<String>,
    pub used_by: Vec<String>,
}

fn unit_of(k: Kind) -> &'static str {
    match k {
        Kind::Length => "mm",
        Kind::Angle => "deg",
        Kind::Unitless => "",
    }
}

impl Document {
    /// Every (name, expression, kind) the expressions can refer to: stored parameters and the
    /// named feature inputs.
    pub fn all_param_exprs(&self) -> Vec<(String, String, Kind)> {
        let mut v: Vec<(String, String, Kind)> = self.params.iter().map(|p| (p.name.clone(), p.expr.clone(), p.kind())).collect();
        for f in &self.features {
            for (name, (_, e, k)) in f.param_names.iter().zip(f.kind.inputs()) {
                if !name.is_empty() {
                    v.push((name.clone(), e, k));
                }
            }
        }
        v
    }

    pub(crate) fn name_taken(&self, n: &str) -> bool {
        self.param(n).is_some() || self.features.iter().any(|f| f.param_names.iter().any(|x| x == n))
    }

    /// The next free `dN` name.
    pub fn next_d_name(&self) -> String {
        let mut n = 1;
        loop {
            let c = format!("d{n}");
            if !self.name_taken(&c) {
                return c;
            }
            n += 1;
        }
    }

    /// Give a feature's inputs parameter names (keeping existing names in place).
    pub fn name_feature_inputs(&mut self, id: u64) {
        let Some(idx) = self.feature_index(id) else { return };
        let n = self.features.get(idx).map(|f| f.kind.inputs().len()).unwrap_or(0);
        for k in 0..n {
            let have = self.features.get(idx).and_then(|f| f.param_names.get(k)).is_some_and(|x| !x.is_empty());
            if have {
                continue;
            }
            let name = self.next_d_name();
            if let Some(f) = self.features.get_mut(idx) {
                while f.param_names.len() <= k {
                    f.param_names.push(String::new());
                }
                if let Some(slot) = f.param_names.get_mut(k) {
                    *slot = name;
                }
            }
        }
        if let Some(f) = self.features.get_mut(idx) {
            f.param_names.truncate(n);
        }
    }

    /// Repair a loaded timeline in one pass. Reserve even later features' names, just as
    /// `name_feature_inputs` does, without restarting a document scan for every `dN` candidate.
    pub(crate) fn name_all_feature_inputs(&mut self) {
        let mut taken: BTreeMap<String, usize> = BTreeMap::new();
        for name in self.params.iter().map(|p| &p.name).chain(self.features.iter().flat_map(|f| &f.param_names)) {
            *taken.entry(name.clone()).or_default() += 1;
        }
        let mut next = 1usize;
        for f in &mut self.features {
            let n = f.kind.inputs().len();
            for k in 0..n {
                if f.param_names.get(k).is_some_and(|name| !name.is_empty()) {
                    continue;
                }
                let name = loop {
                    let name = format!("d{next}");
                    next += 1;
                    if !taken.contains_key(&name) {
                        break name;
                    }
                };
                if f.param_names.len() <= k {
                    f.param_names.resize(k + 1, String::new());
                }
                // Empty slots cannot collide with a generated name.
                if let Some(slot) = f.param_names.get_mut(k) {
                    *slot = name.clone();
                }
                taken.insert(name, 1);
            }
            // Match the single-feature repair: surplus names become available only after
            // this feature has been repaired. Counts preserve names shared with parameters.
            for name in f.param_names.drain(n..) {
                if let Some(count) = taken.get_mut(&name) {
                    *count -= 1;
                    if *count == 0 {
                        taken.remove(&name);
                        if let Some(i) = name.strip_prefix('d').and_then(|s| s.parse::<usize>().ok())
                            && name == format!("d{i}")
                            && i > 0
                        {
                            next = next.min(i);
                        }
                    }
                }
            }
        }
    }

    /// Change a parameter: a stored one, or a feature input by its name.
    /// A typed length that is a bare number means the design's units: it gets that unit written
    /// in (feature inputs are stored in mm otherwise). Anything else is returned unchanged.
    pub fn with_design_unit(&self, e: &str, kind: Kind) -> String {
        let u = self.units.trim();
        let bare = expr::is_bare_number(e);
        if kind == Kind::Length && bare && !u.is_empty() && u != "mm" && expr::unit_info(u).is_some_and(|(k, _)| k == Kind::Length) {
            format!("{} {u}", e.trim())
        } else {
            e.to_string()
        }
    }

    /// Every length input of every feature as the geometry reads it (mm; a unit-less result is
    /// in the design's units), for checking that an edit moves nothing.
    pub fn length_inputs(&self) -> Vec<Option<f64>> {
        let (vals, _) = self.param_values();
        let scale = expr::unit_info(self.units.trim()).filter(|(k, _)| *k == Kind::Length).map(|(_, s)| s).unwrap_or(1.0);
        let look = |n: &str| vals.get(n).copied().ok_or_else(|| DocError::Expr(format!("unknown parameter `{n}`")));
        self.features
            .iter()
            .flat_map(|f| f.kind.inputs())
            .filter(|(_, _, k)| *k == Kind::Length)
            .map(|(_, e, _)| {
                let d = if expr::is_literal(&e) { expr::Defaults::default() } else { expr::Defaults { len: scale, ..expr::Defaults::default() } };
                expr::eval_with(&e, &look).and_then(|v| v.to_kind_in(Kind::Length, d)).ok()
            })
            .collect()
    }

    pub fn change_param(&mut self, name: &str, expr_s: &str, unit: Option<&str>, comment: Option<&str>) -> Result<()> {
        let hit = self.features.iter().enumerate().find_map(|(i, f)| f.param_names.iter().position(|n| n == name).map(|k| (i, k)));
        let Some((i, k)) = hit else { return self.set_param(name, expr_s, unit, comment) };
        if expr_s.len() > 4096 {
            return Err(DocError::Expr("expression too long".into()));
        }
        let mut next = self.clone();
        let kind = self.features.get(i).and_then(|f| f.kind.inputs().get(k).map(|x| x.2)).unwrap_or(Kind::Unitless);
        let expr_s = self.with_design_unit(expr_s, kind);
        if let Some(f) = next.features.get_mut(i)
            && let Some((_, e, _)) = f.kind.inputs_mut().into_iter().nth(k)
        {
            *e = expr_s.clone();
        }
        if let Some(c) = comment {
            next.param_comments.insert(name.to_string(), c.to_string());
        }
        let (_, errors) = next.param_values();
        if let Some(e) = errors.get(name) {
            return Err(DocError::Expr(e.clone()));
        }
        *self = next;
        Ok(())
    }

    /// Change existing expressions against their final dependency graph, independent of
    /// column order. Unlike `change_param`, this never creates a missing parameter.
    pub fn change_params_atomic(&mut self, changes: &[(String, String)]) -> Result<()> {
        if changes.len() > crate::config::MAX_COLUMNS {
            return Err(DocError::Invalid("too many parameter changes".into()));
        }
        let mut names = BTreeSet::new();
        let mut changed = BTreeSet::new();
        let (_, before_errors) = self.param_values();
        let mut next = self.clone();
        for (name, expression) in changes {
            if !names.insert(name.clone()) {
                return Err(DocError::Invalid(format!("duplicate parameter `{name}`")));
            }
            if expression.len() > 4096 || expression.trim().is_empty() {
                return Err(DocError::Expr("expression is empty or too long".into()));
            }
            if let Some(p) = next.params.iter_mut().find(|p| p.name == *name) {
                if p.expr != expression.trim() {
                    changed.insert(name.clone());
                }
                p.expr = expression.trim().to_string();
                continue;
            }
            let hit = next.features.iter().enumerate().find_map(|(i, f)| f.param_names.iter().position(|n| n == name).map(|k| (i, k)));
            let Some((i, k)) = hit else { return Err(DocError::Unknown(format!("parameter `{name}`"))) };
            let kind = next.features.get(i).and_then(|f| f.kind.inputs().get(k).map(|x| x.2)).unwrap_or(Kind::Unitless);
            let normalized = next.with_design_unit(expression, kind);
            let Some(feature) = next.features.get_mut(i) else { return Err(DocError::Unknown(format!("parameter `{name}`"))) };
            let Some((_, target, _)) = feature.kind.inputs_mut().into_iter().nth(k) else {
                return Err(DocError::Unknown(format!("parameter `{name}`")));
            };
            if *target != normalized {
                changed.insert(name.clone());
            }
            *target = normalized;
        }
        let (_, errors) = next.param_values();
        for (name, error) in errors {
            if changed.contains(&name) || !before_errors.contains_key(&name) {
                return Err(DocError::Expr(format!("parameter `{name}`: {error}")));
            }
        }
        *self = next;
        Ok(())
    }

    /// The parameters table (user, sketch dimensions, feature inputs).
    pub fn param_rows(&self) -> Vec<ParamRow> {
        let (vals, errs) = self.param_values();
        let all = self.all_param_exprs();
        let refs: BTreeMap<String, Vec<String>> = all.iter().map(|(n, e, _)| (n.clone(), expr::references(e))).collect();
        let used_by = |n: &str| -> Vec<String> { refs.iter().filter(|(_, r)| r.iter().any(|x| x == n)).map(|(k, _)| k.clone()).collect() };
        let value = |n: &str, k: Kind| vals.get(n).and_then(|v| v.to_kind(k).ok()).map(|x| if k == Kind::Angle { x.to_degrees() } else { x });
        let mut rows = Vec::new();
        for p in &self.params {
            rows.push(ParamRow {
                name: p.name.clone(),
                expression: p.expr.clone(),
                unit: p.unit.clone(),
                value: self.param_display_value(&vals, &p.name),
                error: errs.get(&p.name).cloned(),
                comment: p.comment.clone(),
                source: if p.model { "sketch" } else { "user" },
                feature: None,
                input: None,
                favorite: self.favorites.contains(&p.name),
                uses: refs.get(&p.name).cloned().unwrap_or_default(),
                used_by: used_by(&p.name),
            });
        }
        for f in &self.features {
            for (name, (label, e, k)) in f.param_names.iter().zip(f.kind.inputs()) {
                if name.is_empty() {
                    continue;
                }
                rows.push(ParamRow {
                    name: name.clone(),
                    expression: e,
                    unit: unit_of(k).into(),
                    value: value(name, k),
                    error: errs.get(name).cloned(),
                    comment: self.param_comments.get(name).cloned().unwrap_or_default(),
                    source: "feature",
                    feature: Some(f.name.clone()),
                    input: Some(label),
                    favorite: self.favorites.contains(name),
                    uses: refs.get(name).cloned().unwrap_or_default(),
                    used_by: used_by(name),
                });
            }
        }
        rows
    }

    pub fn set_favorite(&mut self, name: &str, on: bool) -> Result<()> {
        if !self.name_taken(name) {
            return Err(DocError::Unknown(format!("parameter `{name}`")));
        }
        if on {
            self.favorites.insert(name.to_string());
        } else {
            self.favorites.remove(name);
        }
        Ok(())
    }

    /// User parameters as CSV (`name,unit,expression,value,comment`).
    pub fn params_csv(&self) -> String {
        let q = |s: &str| if s.contains([',', '"', '\n']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() };
        let mut out = String::from("name,unit,expression,value,comment,favorite\n");
        for r in self.param_rows().into_iter().filter(|r| r.source == "user") {
            out += &format!(
                "{},{},{},{},{},{}\n",
                q(&r.name),
                q(&r.unit),
                q(&r.expression),
                r.value.map(|v| format!("{v}")).unwrap_or_default(),
                q(&r.comment),
                r.favorite
            );
        }
        out
    }

    /// Import user parameters (CSV with a header, or JSON `[{name, expression, unit?, comment?}]`):
    /// existing ones are updated, new ones added, all checked before anything changes.
    pub fn import_params(&mut self, text: &str) -> Result<Vec<String>> {
        let rows: Vec<(String, String, Option<String>, Option<String>)> = if text.trim_start().starts_with('[') {
            let v: serde_json::Value = serde_json::from_str(text).map_err(|e| DocError::Invalid(format!("parameters JSON: {e}")))?;
            v.as_array()
                .ok_or_else(|| DocError::Invalid("parameters JSON must be a list".into()))?
                .iter()
                .map(|r| {
                    let s = |k: &str| r.get(k).and_then(|x| x.as_str().map(str::to_string).or_else(|| x.as_f64().map(|n| n.to_string())));
                    Ok((
                        s("name").ok_or_else(|| DocError::Invalid("a parameter without `name`".into()))?,
                        s("expression").or_else(|| s("value")).unwrap_or_default(),
                        s("unit"),
                        s("comment"),
                    ))
                })
                .collect::<Result<_>>()?
        } else {
            let mut lines = text.lines();
            let header: Vec<String> = parse_csv_line(lines.next().unwrap_or_default()).into_iter().map(|h| h.trim().to_ascii_lowercase()).collect();
            let col = |n: &str| header.iter().position(|h| h == n);
            let (Some(ni), Some(ei)) = (col("name"), col("expression").or(col("value"))) else {
                return Err(DocError::Invalid("the CSV needs `name` and `expression` columns".into()));
            };
            let (ui, ci) = (col("unit"), col("comment"));
            lines
                .filter(|l| !l.trim().is_empty())
                .map(|l| {
                    let c = parse_csv_line(l);
                    let get = |i: Option<usize>| i.and_then(|i| c.get(i)).map(|s| s.trim().to_string());
                    (get(Some(ni)).unwrap_or_default(), get(Some(ei)).unwrap_or_default(), get(ui), get(ci))
                })
                .collect()
        };
        if rows.len() > 10_000 {
            return Err(DocError::Invalid("too many parameters".into()));
        }
        let mut next = self.clone();
        let mut names = Vec::new();
        for (n, e, u, c) in rows {
            next.change_param(&n, &e, u.as_deref(), c.as_deref())?;
            names.push(n);
        }
        *self = next;
        Ok(names)
    }
}

/// Something that uses a parameter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParamUser {
    /// Another parameter's expression (a stored one or a feature input).
    Parameter { name: String },
    /// A feature input (`input` is its label, e.g. `Distance`).
    Feature { feature: u64, name: String, input: String },
    /// A sketch dimension (`constraint` is its id).
    Dimension { feature: u64, name: String, constraint: String },
    /// A configured column or an expression saved in a configuration row.
    Configuration { row: Option<String>, column: String },
}

impl ParamUser {
    pub fn describe(&self) -> String {
        match self {
            ParamUser::Parameter { name } => format!("parameter {name}"),
            ParamUser::Feature { name, input, .. } => format!("{name} ({input})"),
            ParamUser::Dimension { name, constraint, .. } => format!("{name} dimension {constraint}"),
            ParamUser::Configuration { row, column } => match row {
                Some(row) => format!("configuration `{row}` ({column})"),
                None => format!("configuration column {column}"),
            },
        }
    }
}

/// Length unit of the document in mm (bare numbers of unit-less parameters next to lengths).
fn doc_len(units: &str) -> f64 {
    match expr::unit_info(units) {
        Some((Kind::Length, s)) => s,
        _ => 1.0,
    }
}

impl Document {
    /// Every parameter with its unit, for evaluation: stored parameters and named feature
    /// inputs (mm / deg / none by their kind).
    pub fn param_defs(&self) -> Vec<ParamDef> {
        let mut v: Vec<ParamDef> =
            self.params.iter().map(|p| ParamDef { name: p.name.clone(), expr: p.expr.clone(), unit: p.unit.clone() }).collect();
        for f in &self.features {
            for (name, (_, e, k)) in f.param_names.iter().zip(f.kind.inputs()) {
                if !name.is_empty() {
                    // A bare number stored in an input is mm (the design's units are written in
                    // when it is typed); anything else that comes out unit-less (a unit-less
                    // parameter used as a length) is in the design's units.
                    let unit = match k {
                        Kind::Length if !expr::is_literal(&e) && expr::unit_info(self.units.trim()).is_some_and(|(k, _)| k == Kind::Length) => {
                            self.units.trim().to_string()
                        }
                        _ => unit_of(k).to_string(),
                    };
                    v.push(ParamDef { name: name.clone(), expr: e, unit });
                }
            }
        }
        v
    }

    /// Evaluate all parameters: values (mm / rad / unit-less) and errors by name. Errors name
    /// the problem (`circular reference: a → b → a`, `cannot add a length and an angle`).
    pub fn param_values(&self) -> (BTreeMap<String, Value>, BTreeMap<String, String>) {
        expr::eval_params(&self.param_defs(), doc_len(&self.units))
    }

    /// A parameter's value in its own unit (e.g. 2 for a 50.8 mm parameter in inches).
    pub fn param_display_value(&self, vals: &BTreeMap<String, Value>, name: &str) -> Option<f64> {
        let v = vals.get(name)?;
        let unit = match self.param(name) {
            Some(p) => p.unit.clone(),
            None => v.kind().map(|k| unit_of(k).to_string()).unwrap_or_default(),
        };
        Some(expr::value_in_unit(*v, &unit))
    }

    /// The unit for a new parameter with this expression: that of its value; a bare number is
    /// a length (mm).
    pub(crate) fn infer_unit(&self, e: &str) -> String {
        let (vals, _) = self.param_values();
        let v = expr::eval_with(e, &|n| vals.get(n).copied().ok_or_else(|| DocError::Expr(format!("unknown parameter `{n}`"))));
        match v.ok().and_then(|v| v.kind()) {
            Some(Kind::Unitless) if !expr::references(e).is_empty() => String::new(),
            Some(Kind::Angle) => "deg".into(),
            _ => "mm".into(),
        }
    }

    /// Everything that uses a parameter: other parameters (and feature inputs) whose
    /// expressions refer to it, and the sketch dimensions it drives.
    pub fn param_users(&self, name: &str) -> Vec<ParamUser> {
        let mut out = Vec::new();
        for rule in &self.configs.rules {
            if rule.parameter_references().iter().any(|reference| reference == name) {
                out.push(ParamUser::Configuration { row: None, column: format!("rule `{}`", rule.name) });
            }
        }
        for (index, column) in self.configs.columns.iter().enumerate() {
            if let crate::config::Column::Param(target) = column {
                let key = self.config_key(column);
                if target == name {
                    out.push(ParamUser::Configuration { row: None, column: key.clone() });
                }
                for row in &self.configs.rows {
                    if row.cells.get(index).is_some_and(|value| expr::references(value).iter().any(|reference| reference == name)) {
                        out.push(ParamUser::Configuration { row: Some(row.name.clone()), column: key.clone() });
                    }
                }
            }
        }
        for p in &self.params {
            if p.name != name && expr::references(&p.expr).iter().any(|r| r == name) {
                out.push(ParamUser::Parameter { name: p.name.clone() });
            }
        }
        for f in &self.features {
            for (k, (label, e, _)) in f.kind.inputs().into_iter().enumerate() {
                if expr::references(&e).iter().any(|r| r == name) {
                    match f.param_names.get(k).filter(|n| !n.is_empty()) {
                        Some(n) => out.push(ParamUser::Parameter { name: n.clone() }),
                        None => out.push(ParamUser::Feature { feature: f.id, name: f.name.clone(), input: label.to_string() }),
                    }
                }
            }
            if f.kind.point_expr().is_some_and(|p| p.iter().any(|e| expr::references(e).iter().any(|r| r == name))) {
                out.push(ParamUser::Feature { feature: f.id, name: f.name.clone(), input: "Position".into() });
            }
            if let FeatureKind::Sketch { sketch, .. } = &f.kind {
                for c in &sketch.constraints {
                    if c.param.as_deref() == Some(name) {
                        out.push(ParamUser::Dimension { feature: f.id, name: f.name.clone(), constraint: c.id.clone() });
                    }
                }
            }
        }
        out
    }

    /// Remove a stored parameter; refused while something uses it (the error lists the users).
    pub fn remove_param(&mut self, name: &str) -> Result<()> {
        if self.param(name).is_none() {
            if self.features.iter().any(|f| f.param_names.iter().any(|n| n == name)) {
                return Err(DocError::Invalid(format!("`{name}` is a feature's input: delete or edit the feature instead")));
            }
            return Err(DocError::Unknown(format!("parameter `{name}`")));
        }
        let users = self.param_users(name);
        if !users.is_empty() {
            let list: Vec<String> = users.iter().map(ParamUser::describe).collect();
            return Err(DocError::Invalid(format!("`{name}` is used by {}", list.join(", "))));
        }
        self.params.retain(|p| p.name != name);
        self.favorites.remove(name);
        Ok(())
    }

    /// Rename a parameter (stored or a feature input); every expression, feature input,
    /// dimension, favourite and comment that refers to it follows. Returns how many
    /// references were updated.
    pub fn rename_param(&mut self, old: &str, new: &str) -> Result<usize> {
        let new = new.trim();
        if !self.name_taken(old) {
            return Err(DocError::Unknown(format!("parameter `{old}`")));
        }
        if old == new {
            return Ok(0);
        }
        if let Some(p) = Self::param_name_problem(new) {
            return Err(DocError::Invalid(p));
        }
        if self.name_taken(new) {
            return Err(DocError::Invalid(format!("there is already a parameter `{new}`")));
        }
        let mut n = 0;
        for p in &mut self.params {
            if p.name == old {
                p.name = new.to_string();
            }
            let r = expr::rename_reference(&p.expr, old, new);
            if r != p.expr {
                p.expr = r;
                n += 1;
            }
        }
        for f in &mut self.features {
            for x in &mut f.param_names {
                if x == old {
                    *x = new.to_string();
                }
            }
            for e in f.kind.point_expr_mut().into_iter().flatten() {
                let r = expr::rename_reference(e, old, new);
                if r != *e {
                    *e = r;
                    n += 1;
                }
            }
            for (_, e, _) in f.kind.inputs_mut() {
                let r = expr::rename_reference(e, old, new);
                if r != *e {
                    *e = r;
                    n += 1;
                }
            }
            if let FeatureKind::Sketch { sketch, .. } = &mut f.kind {
                for c in &mut sketch.constraints {
                    if c.param.as_deref() == Some(old) {
                        c.param = Some(new.to_string());
                        n += 1;
                    }
                }
            }
        }
        if self.favorites.remove(old) {
            self.favorites.insert(new.to_string());
        }
        for c in &mut self.configs.columns {
            if *c == crate::config::Column::Param(old.to_string()) {
                *c = crate::config::Column::Param(new.to_string());
            }
        }
        // Configuration cells refer to parameters too.
        for rule in &mut self.configs.rules {
            rule.rename_parameter(old, new);
        }
        for r in &mut self.configs.rows {
            for (column, cell) in self.configs.columns.iter().zip(&mut r.cells) {
                if matches!(column, crate::config::Column::Param(_)) {
                    *cell = expr::rename_reference(cell, old, new);
                }
            }
        }
        if let Some(c) = self.param_comments.remove(old) {
            self.param_comments.insert(new.to_string(), c);
        }
        Ok(n)
    }

    /// Set a parameter's comment (stored parameters keep it, feature inputs in `param_comments`).
    pub fn set_param_comment(&mut self, name: &str, comment: &str) -> Result<()> {
        if comment.len() > 4096 {
            return Err(DocError::Invalid("comment too long".into()));
        }
        if let Some(p) = self.params.iter_mut().find(|p| p.name == name) {
            p.comment = comment.to_string();
            return Ok(());
        }
        if !self.name_taken(name) {
            return Err(DocError::Unknown(format!("parameter `{name}`")));
        }
        if comment.is_empty() {
            self.param_comments.remove(name);
        } else {
            self.param_comments.insert(name.to_string(), comment.to_string());
        }
        Ok(())
    }

    /// Dependency edges: (user, parameter used). Users are parameter names, or
    /// `Feature:<name>` for a feature whose inputs or dimensions use the parameter.
    pub fn param_graph(&self) -> Vec<(String, String)> {
        let mut out = expr::param_edges(&self.param_defs());
        for f in &self.features {
            let mut names: BTreeSet<String> = f.param_names.iter().filter(|n| !n.is_empty()).cloned().collect();
            if let FeatureKind::Sketch { sketch, .. } = &f.kind {
                names.extend(sketch.constraints.iter().filter_map(|c| c.param.clone()));
            }
            out.extend(names.into_iter().map(|n| (format!("Feature:{}", f.name), n)));
        }
        out
    }

    /// Features whose result depends on a parameter (directly or through other parameters).
    pub fn features_using_param(&self, name: &str) -> Vec<u64> {
        let defs = self.param_defs();
        let mut affected: BTreeSet<String> = BTreeSet::from([name.to_string()]);
        for _ in 0..defs.len().min(MAX_PARAMS * 2) {
            let before = affected.len();
            for d in &defs {
                if expr::references(&d.expr).iter().any(|r| affected.contains(r)) {
                    affected.insert(d.name.clone());
                }
            }
            if affected.len() == before {
                break;
            }
        }
        let mut out = Vec::new();
        for f in &self.features {
            let mut hit = f.param_names.iter().any(|n| affected.contains(n));
            hit |= f.kind.expressions().iter().any(|e| expr::references(e).iter().any(|r| affected.contains(r)));
            if let FeatureKind::Sketch { sketch, .. } = &f.kind {
                hit |= sketch.constraints.iter().any(|c| c.param.as_ref().is_some_and(|p| affected.contains(p)));
            }
            if hit {
                out.push(f.id);
            }
        }
        out
    }

    /// Add a user parameter (refused when the name exists).
    pub fn add_user_param(&mut self, name: &str, expr_s: &str, unit: Option<&str>, comment: Option<&str>) -> Result<()> {
        if self.name_taken(name.trim()) {
            return Err(DocError::Invalid(format!("there is already a parameter `{}`", name.trim())));
        }
        if self.params.len() >= MAX_PARAMS {
            return Err(DocError::Invalid("too many parameters".into()));
        }
        self.set_param(name.trim(), expr_s, unit, comment)
    }
}

/// One CSV line with quoted fields.
fn parse_csv_line(l: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = l.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => out.push(std::mem::take(&mut cur)),
            (c, _) => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Parameters whose definitions depend on each other in a loop.
pub fn cycles(list: &[(String, String, Kind)]) -> BTreeSet<String> {
    let refs: BTreeMap<&str, Vec<String>> = list.iter().map(|(n, e, _)| (n.as_str(), expr::references(e))).collect();
    let mut bad = BTreeSet::new();
    for (start, _, _) in list {
        // Depth-first from `start`; reaching it again is a cycle.
        let mut stack: Vec<(String, usize)> = refs.get(start.as_str()).map(|r| r.iter().map(|x| (x.clone(), 1)).collect()).unwrap_or_default();
        let mut seen = BTreeSet::new();
        while let Some((n, d)) = stack.pop() {
            if &n == start {
                bad.insert(start.clone());
                break;
            }
            if d > 10_000 || !seen.insert(n.clone()) {
                continue;
            }
            if let Some(r) = refs.get(n.as_str()) {
                stack.extend(r.iter().map(|x| (x.clone(), d + 1)));
            }
        }
    }
    bad
}
