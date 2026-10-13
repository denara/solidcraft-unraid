//! Sheet metal: rules (thickness, K-factor, bend radius, reliefs as expressions that may use
//! parameters), and the sheet body model behind every sheet-metal feature.
//!
//! A sheet body is kept as its flat pattern: the base panel, flanges hanging off panel edges
//! (each a bend zone of width BA = θ·(R + K·T) followed by a flat panel), and cut-outs, all in
//! the flat frame (the base panel's bottom face; thickness grows along its normal). Folding maps
//! each flange's flat coordinates through its parents' bends: past a bend line, a point loses
//! the bend zone's width and turns by θ about the bend axis (R from the inner face). The folded
//! solid is built from extruded pieces (panels, and bend zones as annular sectors) joined
//! together; the flat solid is the outline extruded through the thickness.

use serde::{Deserialize, Serialize};
use solidcraft_geom::{Loop2, Plane, Region2, Seg2, Vec2, Vec3};
use solidcraft_kernel::{self as kernel, Body, EdgeSpec, FaceSpec, SurfSpec};

use crate::expr::{self, Kind, Value};
use crate::{DocError, Document, IDENTITY, Mat, Result, apply_point, apply_vector, mat_inverse, mat_mul};

// Bound the aggregate geometry, including sampled curves and intermediate clipping pieces.
// These are work limits, never geometry truncation: a larger input returns a useful error.
// Exact lines and arcs count once each (the planar booleans work on them whole); sampled
// curves count their 0.001 mm chords.
const MAX_FLAT_POINTS: usize = 16_384;
// The 0.001 mm chords of everything, arcs included: what the planar booleans sample (their
// own limit is the same).
const MAX_FLAT_SAMPLES: usize = 65_536;
const MAX_FLAT_PIECES: usize = 4096;
const MAX_FLAT_WORK: usize = 500_000_000;

fn flat_budget_error() -> DocError {
    DocError::Invalid("sheet metal flat geometry exceeds the bounded processing budget".into())
}

pub(crate) struct FlatWork(usize);

impl FlatWork {
    pub(crate) fn new() -> Self {
        Self(MAX_FLAT_WORK)
    }
    pub(crate) fn charge(&mut self, work: usize) -> Result<()> {
        self.0 = self.0.checked_sub(work).ok_or_else(flat_budget_error)?;
        Ok(())
    }
}

fn flat_polyline(lp: &Loop2) -> Result<Vec<Vec2>> {
    if lp.segs.len() > MAX_FLAT_POINTS {
        return Err(flat_budget_error());
    }
    let mut points = Vec::new();
    for seg in &lp.segs {
        let p = seg.polyline(1e-3);
        let add = p.len().saturating_sub(1);
        if points.len().saturating_add(add) > MAX_FLAT_POINTS {
            return Err(flat_budget_error());
        }
        points.extend(p.iter().take(add));
    }
    Ok(points)
}

fn flat_points(polys: &[Vec<Vec2>]) -> Result<usize> {
    let n = polys.iter().try_fold(0usize, |n, p| n.checked_add(p.len()).ok_or_else(flat_budget_error))?;
    if n > MAX_FLAT_POINTS || polys.len() > MAX_FLAT_PIECES {
        return Err(flat_budget_error());
    }
    Ok(n)
}

fn planar_region(region: &Region2) -> Result<Region2> {
    if region.holes.len() > MAX_FLAT_PIECES {
        return Err(flat_budget_error());
    }
    region_complexity(std::slice::from_ref(region))?;
    let convert = |lp: &Loop2| -> Result<Loop2> {
        if lp.segs.iter().all(|s| matches!(s, Seg2::Line { .. } | Seg2::Arc { .. })) {
            Ok(lp.clone())
        } else {
            // Keep free-form profiles through controlled 0.001 mm chord sampling.
            Ok(Loop2::polygon(&flat_polyline(lp)?))
        }
    };
    Ok(Region2 {
        outer: convert(&region.outer)?.ccw(),
        holes: region.holes.iter().map(|h| convert(h).map(|l| l.ccw().reversed())).collect::<Result<_>>()?,
    })
}

fn region_complexity(regions: &[Region2]) -> Result<(usize, usize)> {
    regions_complexity(regions.iter())
}

fn regions_complexity<'a>(regions: impl Iterator<Item = &'a Region2>) -> Result<(usize, usize)> {
    let (mut edges, mut points, mut samples) = (0usize, 0usize, 0usize);
    let mut pieces = 0usize;
    for region in regions {
        pieces += 1;
        if pieces > MAX_FLAT_PIECES || region.holes.len() > MAX_FLAT_PIECES {
            return Err(flat_budget_error());
        }
        for lp in std::iter::once(&region.outer).chain(&region.holes) {
            edges = edges.saturating_add(lp.segs.len());
            if edges > MAX_FLAT_POINTS {
                return Err(flat_budget_error());
            }
            for seg in &lp.segs {
                let chords = seg.polyline(1e-3).len().saturating_sub(1);
                points = points.saturating_add(if matches!(seg, Seg2::Line { .. } | Seg2::Arc { .. }) { 1 } else { chords });
                samples = samples.saturating_add(chords);
                if points > MAX_FLAT_POINTS || samples > MAX_FLAT_SAMPLES {
                    return Err(flat_budget_error());
                }
            }
        }
    }
    Ok((edges, points))
}

pub(crate) fn material_boolean(a: &[Region2], b: &[Region2], op: kernel::BoolOp, work: &mut FlatWork) -> Result<Vec<Region2>> {
    let (ae, ap) = region_complexity(a)?;
    let (be, bp) = region_complexity(b)?;
    let edges = ae.saturating_add(be);
    work.charge(edges.saturating_mul(edges).saturating_add(edges.saturating_mul(ap.saturating_add(bp))).saturating_mul(8))?;
    let regions = kernel::planar_regions_boolean(a, b, op, 1e-7)?;
    region_complexity(&regions)?;
    Ok(regions)
}

// Merge in balanced rounds so hundreds of disjoint holes or adjoining panels do not
// repeatedly arrange a growing accumulated boundary against just one small region.
pub(crate) fn material_union(regions: Vec<Region2>, work: &mut FlatWork) -> Result<Vec<Region2>> {
    region_complexity(&regions)?;
    let mut groups: Vec<Vec<Region2>> = regions.into_iter().map(|r| vec![r]).collect();
    while groups.len() > 1 {
        let mut next = Vec::new();
        let mut iter = groups.into_iter();
        while let Some(a) = iter.next() {
            match iter.next() {
                Some(b) => next.push(material_boolean(&a, &b, kernel::BoolOp::Union, work)?),
                None => next.push(a),
            }
        }
        groups = next;
    }
    Ok(groups.pop().unwrap_or_default())
}
/// A sheet metal rule. Values are expressions; `Thickness` inside them is the rule's thickness.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SheetRule {
    pub name: String,
    pub thickness: String,
    pub k_factor: String,
    pub bend_radius: String,
    #[serde(default = "rule_relief_width")]
    pub relief_width: String,
    #[serde(default = "rule_relief_depth")]
    pub relief_depth: String,
    #[serde(default = "rule_corner_relief")]
    pub corner_relief: String,
    /// What is cut where two bends meet at a corner of the base: `notch` (nothing more: the
    /// bends end square at the corner), `round` or `rectangular` (a relief `corner_relief`
    /// across, centred on the corner the two bends share).
    #[serde(default = "rule_corner_relief_shape")]
    pub corner_relief_shape: String,
    /// Inner radius of flat hems.
    #[serde(default = "rule_hem_gap")]
    pub hem_gap: String,
    /// Gap between flanges that meet at a corner.
    #[serde(default = "rule_gap")]
    pub gap: String,
}

fn rule_gap() -> String {
    "Thickness".into()
}

fn rule_relief_width() -> String {
    "Thickness".into()
}
fn rule_relief_depth() -> String {
    "Thickness * 0.5".into()
}
fn rule_corner_relief() -> String {
    "Thickness * 4.0".into()
}
fn rule_corner_relief_shape() -> String {
    ReliefShape::Notch.name().into()
}
fn rule_hem_gap() -> String {
    "0.01 mm".into()
}

impl SheetRule {
    /// Fusion's default: Steel (mm), 2.5 mm, K 0.44, bend radius = Thickness.
    pub fn steel() -> SheetRule {
        SheetRule {
            name: "Steel (mm)".into(),
            thickness: "2.5 mm".into(),
            k_factor: "0.44".into(),
            bend_radius: "Thickness".into(),
            relief_width: rule_relief_width(),
            relief_depth: rule_relief_depth(),
            corner_relief: rule_corner_relief(),
            corner_relief_shape: rule_corner_relief_shape(),
            hem_gap: rule_hem_gap(),
            gap: rule_gap(),
        }
    }
}

/// The design's sheet metal rules and the one new sheet bodies use.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SheetSettings {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<SheetRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
}

impl SheetSettings {
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty() && self.active.is_none()
    }
}

/// The shape of the relief cut where two bends meet at a corner of the base (a rule's
/// `corner_relief_shape`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReliefShape {
    /// Nothing is cut: the bends end square at the corner.
    #[default]
    Notch,
    /// A round about the corner of the bends, through the angle the base fills there.
    Round,
    /// A square with a corner on the corner of the bends (a kite where the base's corner isn't
    /// square: both cuts leave their edge at a right angle).
    Rectangular,
}

impl ReliefShape {
    pub fn parse(s: &str) -> Option<ReliefShape> {
        match s.trim().to_ascii_lowercase().as_str() {
            "notch" => Some(ReliefShape::Notch),
            "round" => Some(ReliefShape::Round),
            "rectangular" => Some(ReliefShape::Rectangular),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ReliefShape::Notch => "notch",
            ReliefShape::Round => "round",
            ReliefShape::Rectangular => "rectangular",
        }
    }
}

/// A rule's values (mm, unit-less K).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct RuleValues {
    pub thickness: f64,
    pub k_factor: f64,
    pub bend_radius: f64,
    pub relief_width: f64,
    pub relief_depth: f64,
    pub corner_relief: f64,
    pub corner_relief_shape: ReliefShape,
    pub hem_gap: f64,
    pub gap: f64,
}

impl Document {
    /// A rule by name, the active one, or the default steel rule.
    pub fn sheet_rule(&self, name: Option<&str>) -> SheetRule {
        let want = name.or(self.sheet.active.as_deref());
        want.and_then(|n| self.sheet.rules.iter().find(|r| r.name == n))
            .or_else(|| self.sheet.rules.first())
            .cloned()
            .unwrap_or_else(SheetRule::steel)
    }

    /// Evaluate a rule: `Thickness` in its expressions is the rule's thickness.
    pub fn rule_values(&self, vals: &std::collections::BTreeMap<String, Value>, r: &SheetRule) -> Result<RuleValues> {
        let look = |n: &str| vals.get(n).copied().ok_or_else(|| DocError::Expr(format!("unknown parameter `{n}`")));
        let t = expr::eval_with(&r.thickness, &look)?.to_kind(Kind::Length)?;
        if !(t > 0.0 && t < 1e4) {
            return Err(DocError::Invalid("the sheet thickness must be positive".into()));
        }
        let with_t = |e: &str, k: Kind| -> Result<f64> {
            let f = |n: &str| if n == "Thickness" && !vals.contains_key("Thickness") { Ok(Value::length(t)) } else { look(n) };
            expr::eval_with(e, &f)?.to_kind(k)
        };
        let v = RuleValues {
            thickness: t,
            k_factor: with_t(&r.k_factor, Kind::Unitless)?,
            bend_radius: with_t(&r.bend_radius, Kind::Length)?,
            relief_width: with_t(&r.relief_width, Kind::Length)?,
            relief_depth: with_t(&r.relief_depth, Kind::Length)?,
            corner_relief: with_t(&r.corner_relief, Kind::Length)?,
            corner_relief_shape: ReliefShape::parse(&r.corner_relief_shape)
                .ok_or_else(|| DocError::Invalid(format!("unknown corner relief shape `{}` (notch, round or rectangular)", r.corner_relief_shape)))?,
            hem_gap: with_t(&r.hem_gap, Kind::Length)?,
            gap: with_t(&r.gap, Kind::Length)?,
        };
        if !(0.0..=1.0).contains(&v.k_factor) || v.bend_radius < 0.0 {
            return Err(DocError::Invalid("the K-factor must be 0…1 and the bend radius positive".into()));
        }
        Ok(v)
    }
}

/// A flange: a bend along a panel edge and the flat panel past it (flat coordinates).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SheetFlange {
    /// The flange it hangs off (None = the base panel).
    pub parent: Option<usize>,
    /// Bend start line: from `p` along `d` for `len`; `n` points into the flange.
    pub p: Vec2,
    pub d: Vec2,
    pub len: f64,
    pub n: Vec2,
    /// Bend angle (radians; positive bends toward the top face, +z of the flat frame).
    pub angle: f64,
    /// Inner bend radius.
    pub radius: f64,
    /// Flat length of the panel past the bend zone.
    pub leg: f64,
    /// The panel runs on past the bend's ends by these (toward a neighbouring flange), measured
    /// along the bend line; negative values cut the panel back.
    #[serde(default)]
    pub ext: (f64, f64),
    /// How much further each end runs on per unit of panel height: a closed corner between
    /// flanges that aren't at 90° ends on a slanted line in the flat.
    #[serde(default, skip_serializing_if = "is_zero_pair")]
    pub slope: (f64, f64),
    /// A fold's panel: the part of the sheet past the bend (flat coordinates, counter-clockwise)
    /// instead of a rectangle `leg` long.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<Vec<Vec2>>,
    /// Which feature made it: `F<feature>.<k>` (persistent edge names).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tag: String,
}

/// One edge flange to make: a pick on the sheet's edge, and that flange's height (to the outer
/// face, from the virtual sharp) and angle (radians).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeFlange {
    pub pick: Vec3,
    pub height: f64,
    pub angle: f64,
}

/// A sheet metal body (see the module docs).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SheetBody {
    pub body: String,
    pub rule: String,
    pub t: f64,
    pub k: f64,
    /// The flat frame: base panel bottom face; z = thickness direction.
    pub frame: Plane,
    pub base: Region2,
    pub flanges: Vec<SheetFlange>,
    /// Cut-outs (flat coordinates) through the sheet.
    pub holes: Vec<Region2>,
    /// Planar machining in flat coordinates, with depths measured from the flat bottom.
    /// Derived from feature history; these records are not a document schema change.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pockets: Vec<SheetPocket>,
    /// Unfolded.
    pub flat: bool,
    /// Gap between flanges that meet at a corner.
    pub gap: f64,
    /// The relief cut where two bends meet at a corner of the base, and how far across it is
    /// (the rule's `corner_relief_shape` and `corner_relief`).
    pub relief: ReliefShape,
    pub relief_size: f64,
    /// The base sketch's lines (id, ends in flat coordinates), naming the base panel's edges.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub base_curves: Vec<(String, Vec2, Vec2)>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SheetPocket {
    pub region: Region2,
    pub z0: f64,
    pub z1: f64,
}

/// A piece of the sheet: a panel (None = the base) or a flange's bend zone.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Piece {
    Panel(Option<usize>),
    Zone(usize),
}

/// A flat piece of the sheet (counter-clockwise outer loop, clockwise holes).
#[derive(Clone, Debug)]
struct Cell {
    piece: Piece,
    outer: Vec<Seg2>,
    holes: Vec<Vec<Seg2>>,
}

fn is_zero_pair(v: &(f64, f64)) -> bool {
    v.0 == 0.0 && v.1 == 0.0
}

fn rev_edge(e: EdgeSpec) -> EdgeSpec {
    e.reversed()
}

/// Largest distance between a folded bend edge and the cubic chain standing in for it.
const BEND_CHAIN_TOL: f64 = 1e-7;
/// Most halvings of a bend edge's parameter while fitting its chain.
const BEND_CHAIN_DEPTH: usize = 10;

fn cubic_at(p0: Vec3, c1: Vec3, c2: Vec3, p1: Vec3, t: f64) -> Vec3 {
    let u = 1.0 - t;
    p0 * (u * u * u) + c1 * (3.0 * u * u * t) + c2 * (3.0 * u * t * t) + p1 * (t * t * t)
}

fn dedup(p: &[Vec2]) -> Vec<Vec2> {
    let mut v: Vec<Vec2> = Vec::new();
    for q in p {
        if v.last().is_none_or(|l| l.dist(*q) > 1e-9) {
            v.push(*q);
        }
    }
    if v.len() > 1 && v.first().zip(v.last()).is_some_and(|(a, b)| a.dist(*b) < 1e-9) {
        v.pop();
    }
    v
}

/// How a flat edge of a bend zone folds: along the bend to a line, across it to an arc, or
/// any other way (slanted lines and curves) to a curve on the bend's cylinder.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ZoneEdge {
    Along,
    Across,
    Curved,
}

/// How far (mm) a flat edge's ends may stray from a line along or across its bend and still
/// fold as that line or arc: scripts round coordinates, so the test is in millimetres, not
/// relative to the edge's length.
const ZONE_SQUARE_TOL: f64 = 1e-6;

fn zone_edge_kind(f: &SheetFlange, e: &Seg2) -> ZoneEdge {
    let Seg2::Line { a, b } = *e else { return ZoneEdge::Curved };
    let dlt = b - a;
    if dlt.dot(f.n).abs() <= ZONE_SQUARE_TOL {
        ZoneEdge::Along
    } else if dlt.dot(f.d).abs() <= ZONE_SQUARE_TOL {
        ZoneEdge::Across
    } else {
        ZoneEdge::Curved
    }
}

/// A bend for the bend table.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BendInfo {
    pub index: usize,
    pub angle_deg: f64,
    pub radius: f64,
    pub up: bool,
    pub allowance: f64,
    pub length: f64,
    /// Centre line in flat coordinates.
    pub start: Vec2,
    pub end: Vec2,
}

fn rot_about(o: Vec3, k: Vec3, a: f64) -> Mat {
    crate::rigid(Vec3::ZERO, o, k, a)
}

fn translate(t: Vec3) -> Mat {
    let mut m = IDENTITY;
    m[3] = [t.x, t.y, t.z, 1.0];
    m
}

fn v3(v: Vec2) -> Vec3 {
    Vec3::new(v.x, v.y, 0.0)
}

fn plane_of(m: &Mat, o: Vec3, x: Vec3, y: Vec3) -> Result<Plane> {
    Plane::new(apply_point(m, o), apply_vector(m, x), apply_vector(m, y)).ok_or_else(|| DocError::Invalid("sheet: degenerate plane".into()))
}

fn rect(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> Region2 {
    let mut lp = Loop2::polygon(&[a, b, c, d]);
    if lp.signed_area() < 0.0 {
        lp = lp.reversed();
    }
    Region2 { outer: lp, holes: Vec::new() }
}

/// Round to a 1e-9 mm grid (keeps trimmed edges exactly where the kernel expects them).
fn snap(v: Vec2) -> Vec2 {
    let r = |x: f64| (x * 1e9).round() / 1e9;
    Vec2::new(r(v.x), r(v.y))
}

/// Clip a polygon to the half-plane (p − o)·n ≥ 0.
fn clip(poly: &[Vec2], o: Vec2, n: Vec2) -> Vec<Vec2> {
    let mut out = Vec::new();
    let m = poly.len();
    for i in 0..m {
        let (Some(&a), Some(&b)) = (poly.get(i), poly.get((i + 1) % m)) else { continue };
        let (da, db) = ((a - o).dot(n), (b - o).dot(n));
        if da >= 0.0 {
            out.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            let t = da / (da - db);
            out.push(snap(a + (b - a) * t));
        }
    }
    out
}

/// The smallest gap closed corners are given: closer than this the flanges' ends would touch.
const MIN_CORNER_GAP: f64 = 0.01;

/// The smallest corner relief (across) that is cut: a smaller one would leave hairline edges.
const MIN_CORNER_RELIEF: f64 = 0.01;

/// Two bends on the base meeting at a corner (see [`SheetBody::corners`]): flanges `a` and `b`,
/// which end of each (false = the end at `p`), and the corner point (flat coordinates).
#[derive(Clone, Copy, Debug)]
struct Corner {
    a: usize,
    a_end: bool,
    b: usize,
    b_end: bool,
    c: Vec2,
}

/// The corners of a straight-edged region (counter-clockwise).
fn corners_of(r: &Region2) -> Vec<Vec2> {
    dedup(&r.outer.ccw().polyline(1e-3))
}

/// The area two convex counter-clockwise polygons share.
fn convex_overlap(a: &[Vec2], b: &[Vec2]) -> f64 {
    let mut poly = a.to_vec();
    let m = b.len();
    for i in 0..m {
        let (Some(&p), Some(&q)) = (b.get(i), b.get((i + 1) % m)) else { continue };
        let d = q - p;
        poly = clip(&poly, p, Vec2::new(-d.y, d.x));
        if poly.len() < 3 {
            return 0.0;
        }
    }
    signed_area(&poly).abs()
}

impl SheetBody {
    /// Bend allowance of a flange.
    pub fn allowance(&self, f: &SheetFlange) -> f64 {
        f.angle.abs() * (f.radius + self.k * self.t)
    }

    /// Fold transform of flange `i`'s panel (flat 3D coordinates → folded, in the flat frame).
    pub fn fold(&self, i: usize) -> Mat {
        let mut chain = Vec::new();
        let mut cur = Some(i);
        for _ in 0..1000 {
            let Some(c) = cur else { break };
            chain.push(c);
            cur = self.flanges.get(c).and_then(|f| f.parent);
        }
        let mut m = IDENTITY;
        for c in chain.into_iter().rev() {
            if let Some(f) = self.flanges.get(c) {
                m = mat_mul(&m, &self.bend_motion(f));
            }
        }
        m
    }

    /// The fold of one bend: drop the bend zone, then turn about the bend axis.
    fn bend_motion(&self, f: &SheetFlange) -> Mat {
        let ba = self.allowance(f);
        let (axis_o, k, a) = self.bend_axis(f);
        mat_mul(&rot_about(axis_o, k, a), &translate(v3(f.n) * -ba))
    }

    /// Bend axis point, direction and signed turn (flat coordinates).
    fn bend_axis(&self, f: &SheetFlange) -> (Vec3, Vec3, f64) {
        let up = f.angle >= 0.0;
        let z = if up { self.t + f.radius } else { -f.radius };
        let o = v3(f.p) + Vec3::Z * z;
        let k = v3(f.n).cross(Vec3::Z);
        (o, k, if up { f.angle.abs() } else { -f.angle.abs() })
    }

    /// Flat region of a flange's panel.
    pub fn panel(&self, f: &SheetFlange) -> Region2 {
        if let Some(sh) = &f.shape {
            return Region2 { outer: Loop2::polygon(sh).ccw(), holes: Vec::new() };
        }
        let ba = self.allowance(f);
        let a = f.p + f.n * ba - f.d * f.ext.0;
        let b = f.p + f.n * ba + f.d * (f.len + f.ext.1);
        rect(a, b, b + f.n * f.leg + f.d * (f.slope.1 * f.leg), a + f.n * f.leg - f.d * (f.slope.0 * f.leg))
    }

    /// Flat region of a flange's bend zone.
    pub fn zone(&self, f: &SheetFlange) -> Region2 {
        let ba = self.allowance(f);
        let (u0, u1) = self.zone_span(f);
        let (a, b) = (f.p + f.d * u0, f.p + f.d * u1);
        rect(a, b, b + f.n * ba, a + f.n * ba)
    }

    /// The part of a bend along its parent's edge: where a later flange trimmed that edge back
    /// (a corner) or a corner relief cut into it, the bend stops and the flange panel runs on
    /// alone.
    pub fn zone_span(&self, f: &SheetFlange) -> (f64, f64) {
        if f.parent.is_some() {
            return (0.0, f.len);
        }
        let pts = self.base.outer.ccw().polyline(1e-3);
        let m = pts.len();
        let mut span: Option<(f64, f64)> = None;
        for k in 0..m {
            let (Some(&a), Some(&b)) = (pts.get(k), pts.get((k + 1) % m)) else { continue };
            let on = (a - f.p).dot(f.n).abs() < 1e-6 && (b - f.p).dot(f.n).abs() < 1e-6;
            if !on {
                continue;
            }
            let (ua, ub) = ((a - f.p).dot(f.d), (b - f.p).dot(f.d));
            let (lo, hi) = (ua.min(ub).max(0.0), ua.max(ub).min(f.len));
            if hi > lo + 1e-9 {
                span = Some(span.map_or((lo, hi), |(x, y)| (x.min(lo), y.max(hi))));
            }
        }
        span.unwrap_or((0.0, f.len))
    }

    fn frame_mat(&self) -> Mat {
        let fr = &self.frame;
        let n = fr.normal();
        [[fr.x.x, fr.x.y, fr.x.z, 0.0], [fr.y.x, fr.y.y, fr.y.z, 0.0], [n.x, n.y, n.z, 0.0], [fr.origin.x, fr.origin.y, fr.origin.z, 1.0]]
    }

    /// Flat coordinates (x, y, z) → world, through the fold of flange `i` (None = base).
    pub fn world(&self, i: Option<usize>) -> Mat {
        let f = match i {
            Some(i) if !self.flat => self.fold(i),
            _ => IDENTITY,
        };
        mat_mul(&self.frame_mat(), &f)
    }

    /// Where a flat point of a piece lands (folded), at height `z` in the sheet.
    fn map3(&self, piece: Piece, q: Vec2, z: f64) -> Vec3 {
        match piece {
            Piece::Panel(i) => apply_point(&self.world(i), v3(q) + Vec3::Z * z),
            Piece::Zone(i) => {
                let Some(f) = self.flanges.get(i) else { return Vec3::ZERO };
                let s = (q - f.p).dot(f.n);
                let q0 = q - f.n * s;
                let neutral = (f.radius + self.k * self.t).max(1e-12);
                let phi = (s / neutral).clamp(0.0, f.angle.abs());
                let (axis_o, k, turn) = self.bend_axis(f);
                let m = mat_mul(&self.world(f.parent), &rot_about(axis_o, k, turn.signum() * phi));
                apply_point(&m, v3(q0) + Vec3::Z * z)
            }
        }
    }

    /// A flat point of flange `f`'s bend zone folded onto the bend at height `z` (not clamped
    /// to the zone), with its rates of change along the zone's flat `n` and `d` directions.
    fn zone_point(&self, f: &SheetFlange, q: Vec2, z: f64) -> (Vec3, Vec3, Vec3) {
        let s = (q - f.p).dot(f.n);
        let u = (q - f.p).dot(f.d);
        let neutral = (f.radius + self.k * self.t).max(1e-12);
        let phi = s / neutral;
        let (axis_o, k, turn) = self.bend_axis(f);
        let sigma = if turn < 0.0 { -1.0 } else { 1.0 };
        // From the axis to the bend's start line at height z, and that turned a right angle.
        let ez = Vec3::Z * (z - axis_o.z);
        let ek = k.cross(ez) * sigma;
        let (sn, cs) = phi.sin_cos();
        let w = self.world(f.parent);
        let local = axis_o + v3(f.d) * u + ez * cs + ek * sn;
        let ds = (ez * -sn + ek * cs) * (1.0 / neutral);
        (apply_point(&w, local), apply_vector(&w, ds), apply_vector(&w, v3(f.d)))
    }

    /// A flat edge of flange `f`'s bend zone, folded at height `z`, at parameter `t`: the point
    /// and its derivative.
    fn zone_edge_at(&self, f: &SheetFlange, e: &Seg2, z: f64, t: f64) -> (Vec3, Vec3) {
        let (p, ds, du) = self.zone_point(f, e.point_at(t), z);
        let dq = e.derivative(t);
        (p, ds * dq.dot(f.n) + du * dq.dot(f.d))
    }

    /// Where to break a bend zone's flat edge so cubic Hermite pieces follow the folded edge
    /// to within [`BEND_CHAIN_TOL`] on both faces. The breaks depend on the flat edge only, so
    /// the top face, bottom face and wall of a cut-out share one chain per height.
    fn zone_breaks(&self, f: &SheetFlange, e: &Seg2) -> Vec<f64> {
        let mut out = vec![0.0];
        let mut todo = vec![(0.0, 1.0, 0usize)];
        while let Some((t0, t1, depth)) = todo.pop() {
            let fits = depth >= BEND_CHAIN_DEPTH
                || [0.0, self.t].iter().all(|&z| {
                    let h = t1 - t0;
                    let ((p0, d0), (p1, d1)) = (self.zone_edge_at(f, e, z, t0), self.zone_edge_at(f, e, z, t1));
                    let (c1, c2) = (p0 + d0 * (h / 3.0), p1 - d1 * (h / 3.0));
                    [0.25, 0.5, 0.75].iter().all(|&x| cubic_at(p0, c1, c2, p1, x).dist(self.zone_edge_at(f, e, z, t0 + h * x).0) <= BEND_CHAIN_TOL)
                });
            if fits {
                out.push(t1);
            } else {
                let m = (t0 + t1) / 2.0;
                // Depth first, right half last on the stack: breaks come out in order.
                todo.push((m, t1, depth + 1));
                todo.push((t0, m, depth + 1));
            }
        }
        out
    }

    /// A bend zone's flat edge folded at height `z`: a chain of cubic Hermite pieces.
    fn zone_chain(&self, f: &SheetFlange, e: &Seg2, z: f64) -> EdgeSpec {
        let breaks = self.zone_breaks(f, e);
        let mut ctrl = Vec::with_capacity(breaks.len() * 3);
        for w in breaks.windows(2) {
            let (t0, t1) = (w[0], w[1]);
            let h = t1 - t0;
            let ((p0, d0), (p1, d1)) = (self.zone_edge_at(f, e, z, t0), self.zone_edge_at(f, e, z, t1));
            if ctrl.is_empty() {
                ctrl.push(p0);
            }
            ctrl.extend([p0 + d0 * (h / 3.0), p1 - d1 * (h / 3.0), p1]);
        }
        EdgeSpec::Chain { ctrl: ctrl.into() }
    }

    /// A flat edge of a piece as a 3D edge at height `z`.
    fn edge3(&self, piece: Piece, e: &Seg2, z: f64) -> Result<EdgeSpec> {
        let (a, b) = (e.start(), e.end());
        let (pa, pb) = (self.map3(piece, a, z), self.map3(piece, b, z));
        match (piece, e) {
            (Piece::Panel(_), Seg2::Line { .. }) => Ok(EdgeSpec::Line { a: pa, b: pb }),
            (Piece::Panel(_), Seg2::Arc { .. }) => Ok(EdgeSpec::Arc { a: pa, b: pb, mid: self.map3(piece, e.point_at(0.5), z) }),
            (Piece::Zone(i), _) => {
                let f = self.flanges.get(i).ok_or_else(|| DocError::Invalid("bend".into()))?;
                match zone_edge_kind(f, e) {
                    ZoneEdge::Along => Ok(EdgeSpec::Line { a: pa, b: pb }),
                    ZoneEdge::Across => Ok(EdgeSpec::Arc { a: pa, b: pb, mid: self.map3(piece, (a + b) * 0.5, z) }),
                    // Slanted lines fold to helices, curves to curves on the bend's cylinder:
                    // neither has a line, arc or conic form.
                    ZoneEdge::Curved => Ok(self.zone_chain(f, e, z)),
                }
            }
            (_, _) => Err(DocError::Invalid("not supported yet: spline or conic cut-outs in sheet metal".into())),
        }
    }

    /// The pieces of the sheet as cells: base and flange panels and bend zones, each less
    /// the combined cut-out material. Planar clipping retains holes and disconnected islands.
    fn cells_with_work(&self, work: &mut FlatWork) -> Result<Vec<Cell>> {
        if self.flanges.len().saturating_mul(2).saturating_add(1) > MAX_FLAT_PIECES || self.holes.len() > MAX_FLAT_PIECES {
            return Err(flat_budget_error());
        }
        let raw = self
            .flanges
            .iter()
            .fold(self.base.outer.segs.len(), |n, f| n.saturating_add(f.shape.as_ref().map_or(4, Vec::len)))
            .saturating_add(self.holes.iter().fold(0usize, |n, h| n.saturating_add(h.outer.segs.len())));
        if raw > MAX_FLAT_POINTS {
            return Err(flat_budget_error());
        }
        let mut pieces: Vec<(Piece, Region2)> = self.panels().into_iter().map(|(pi, r)| (Piece::Panel(pi), r)).collect();
        for (i, f) in self.flanges.iter().enumerate() {
            pieces.push((Piece::Zone(i), self.zone(f)));
        }
        let mut cuts = Vec::new();
        let mut cut_points = 0usize;
        for hole in &self.holes {
            let region = planar_region(hole)?;
            cut_points = cut_points.saturating_add(region_complexity(std::slice::from_ref(&region))?.1);
            if cut_points > MAX_FLAT_POINTS {
                return Err(flat_budget_error());
            }
            cuts.push(region);
        }
        cuts.extend(self.through_pocket_regions(work)?);
        let cuts = material_union(cuts, work)?;
        let mut out = Vec::new();
        let mut points = 0usize;
        for (piece, region) in pieces {
            let panel = planar_region(&region)?;
            let material = if cuts.is_empty() { vec![panel] } else { material_boolean(&[panel], &cuts, kernel::BoolOp::Cut, work)? };
            for region in material {
                points = points.saturating_add(region_complexity(std::slice::from_ref(&region))?.1);
                if points > MAX_FLAT_POINTS || out.len() >= MAX_FLAT_PIECES {
                    return Err(flat_budget_error());
                }
                out.push(Cell { piece, outer: region.outer.segs, holes: region.holes.into_iter().map(|l| l.segs).collect() });
            }
        }
        Ok(out)
    }
    /// Faces of the solid made by `cells` between heights `z0` and `z1`.
    fn faces(&self, cells: &[Cell], z0: f64, z1: f64, work: &mut FlatWork) -> Result<Vec<FaceSpec>> {
        // Split straight edges where other cells' corners touch them, so neighbours share edges.
        let edges = cells
            .iter()
            .fold(0usize, |n, c| n.saturating_add(c.outer.len()).saturating_add(c.holes.iter().fold(0usize, |m, h| m.saturating_add(h.len()))));
        if edges > MAX_FLAT_POINTS || cells.len() > MAX_FLAT_PIECES {
            return Err(flat_budget_error());
        }
        work.charge(edges.saturating_mul(edges).saturating_mul(4))?;
        let corners: Vec<Vec2> = cells.iter().flat_map(|c| c.outer.iter().chain(c.holes.iter().flatten()).map(Seg2::start)).collect();
        let mut split_edges = 0usize;
        let mut split_loop = |segs: &[Seg2]| -> Result<Vec<Seg2>> {
            let mut v = Vec::new();
            for e in segs {
                let Seg2::Line { a, b } = *e else {
                    split_edges += 1;
                    if split_edges > MAX_FLAT_POINTS {
                        return Err(flat_budget_error());
                    }
                    v.push(*e);
                    continue;
                };
                let d = b - a;
                let l2 = d.len2();
                let mut ts: Vec<f64> = corners
                    .iter()
                    .filter_map(|p| {
                        let t = (*p - a).dot(d) / l2.max(1e-30);
                        ((t > 1e-9 && t < 1.0 - 1e-9) && (a + d * t).dist(*p) < 1e-7).then_some(t)
                    })
                    .collect();
                ts.sort_by(|x, y| x.total_cmp(y));
                ts.dedup_by(|x, y| (*x - *y).abs() < 1e-12);
                let mut prev = a;
                for t in ts {
                    split_edges += 1;
                    if split_edges > MAX_FLAT_POINTS {
                        return Err(flat_budget_error());
                    }
                    let q = a + d * t;
                    v.push(Seg2::Line { a: prev, b: q });
                    prev = q;
                }
                split_edges += 1;
                if split_edges > MAX_FLAT_POINTS {
                    return Err(flat_budget_error());
                }
                v.push(Seg2::Line { a: prev, b });
            }
            Ok(v)
        };
        let mut split_cells = Vec::new();
        for c in cells {
            let outer = split_loop(&c.outer)?;
            let holes = c.holes.iter().map(|h| split_loop(h)).collect::<Result<_>>()?;
            split_cells.push(Cell { piece: c.piece, outer, holes });
        }
        // Reserve the complete adjacency scan before visiting any wall.
        work.charge(split_edges.saturating_mul(split_edges).saturating_mul(2))?;
        let cells = split_cells;
        let near = |p: Vec2, q: Vec2| p.dist(q) < 1e-6;
        // Shared with a neighbour: the same flat edge the other way, landing in the same place
        // (flat neighbours across a corner of two bends do not).
        let shared = |ci: usize, e: &Seg2| {
            let Some(me) = cells.get(ci) else { return false };
            let here = self.map3(me.piece, e.point_at(0.5), z0);
            cells.iter().enumerate().any(|(cj, c)| {
                cj != ci
                    && c.outer.iter().chain(c.holes.iter().flatten()).any(|g| {
                        near(g.start(), e.end())
                            && near(g.end(), e.start())
                            && matches!((g, e), (Seg2::Line { .. }, Seg2::Line { .. }))
                            && self.map3(c.piece, g.point_at(0.5), z0).dist(here) < 1e-6
                    })
            })
        };
        let mut faces = Vec::new();
        for (ci, c) in cells.iter().enumerate() {
            // Top (z1, outward +z) and bottom (z0, outward −z, loops reversed).
            let loops = |z: f64, rev: bool| -> Result<Vec<Vec<EdgeSpec>>> {
                std::iter::once(&c.outer)
                    .chain(c.holes.iter())
                    .map(|l| {
                        let mut v: Vec<EdgeSpec> = l.iter().map(|e| self.edge3(c.piece, e, z)).collect::<Result<_>>()?;
                        if rev {
                            v = v.into_iter().rev().map(rev_edge).collect();
                        }
                        Ok(v)
                    })
                    .collect()
            };
            let centre = c.outer.iter().map(Seg2::start).fold(Vec2::default(), |a, b| a + b) * (1.0 / c.outer.len().max(1) as f64);
            for (z, rev, sign) in [(z1, false, 1.0), (z0, true, -1.0)] {
                let surface = match c.piece {
                    Piece::Panel(_) => SurfSpec::Plane,
                    Piece::Zone(i) => {
                        let f = self.flanges.get(i).ok_or_else(|| DocError::Invalid("bend".into()))?;
                        // The surface must hold the whole boundary: curved cut-out edges may
                        // bulge past the cell's corners, so they are sampled (and the surface
                        // padded a little, never past the zone's own width).
                        let curved = c.outer.iter().any(|e| zone_edge_kind(f, e) == ZoneEdge::Curved);
                        let pts: Vec<Vec2> =
                            if curved { c.outer.iter().flat_map(|e| e.polyline(1e-4)).collect() } else { c.outer.iter().map(Seg2::start).collect() };
                        let ss: Vec<f64> = pts.iter().map(|q| (*q - f.p).dot(f.n)).collect();
                        let (mut s0, mut s1) = (ss.iter().cloned().fold(f64::MAX, f64::min), ss.iter().cloned().fold(f64::MIN, f64::max));
                        let us: Vec<f64> = pts.iter().map(|q| (*q - f.p).dot(f.d)).collect();
                        let (mut u0, mut u1) = (us.iter().cloned().fold(f64::MAX, f64::min), us.iter().cloned().fold(f64::MIN, f64::max));
                        if curved {
                            let pad = 1e-3;
                            (s0, s1) = ((s0 - pad).max(0.0), (s1 + pad).min(self.allowance(f)));
                            (u0, u1) = (u0 - pad, u1 + pad);
                        }
                        let neutral = (f.radius + self.k * self.t).max(1e-12);
                        let (axis_o, k, turn) = self.bend_axis(f);
                        let wp = self.world(f.parent);
                        let q = |u: f64| f.p + f.n * s0 + f.d * u;
                        SurfSpec::Revolved {
                            a: self.map3(c.piece, q(u0), z),
                            b: self.map3(c.piece, q(u1), z),
                            axis_origin: apply_point(&wp, axis_o),
                            axis: apply_vector(&wp, k),
                            angle: turn.signum() * (s1 - s0) / neutral,
                        }
                    }
                };
                let p = self.map3(c.piece, centre, z);
                let outward = (self.map3(c.piece, centre, z + 1.0) - p) * sign;
                faces.push(FaceSpec { loops: loops(z, rev)?, surface, probe: Some((p, outward)) });
            }
            // Walls along edges no other cell shares.
            for e in c.outer.iter().chain(c.holes.iter().flatten()) {
                if shared(ci, e) {
                    continue;
                }
                let (a, b) = (e.start(), e.end());
                let bottom = self.edge3(c.piece, e, z0)?;
                let top_ab = self.edge3(c.piece, e, z1)?;
                let up_b = EdgeSpec::Line { a: self.map3(c.piece, b, z0), b: self.map3(c.piece, b, z1) };
                let down_a = EdgeSpec::Line { a: self.map3(c.piece, a, z1), b: self.map3(c.piece, a, z0) };
                let surface = match (c.piece, e) {
                    (Piece::Panel(_), Seg2::Arc { .. }) => {
                        let (pa, pb, pm) = (self.map3(c.piece, a, z0), self.map3(c.piece, b, z0), self.map3(c.piece, e.point_at(0.5), z0));
                        SurfSpec::Extruded { a: pa, b: pb, mid: pm, dir: self.map3(c.piece, a, z1) - pa }
                    }
                    // A cut-out wall across a bend at an angle: square to the sheet, so its
                    // straight lines run radially from the bottom face's edge to the top's.
                    (Piece::Zone(_), _) if matches!(bottom, EdgeSpec::Chain { .. }) => SurfSpec::Ruled { a: bottom.clone(), b: top_ab.clone() },
                    _ => SurfSpec::Plane,
                };
                let top = rev_edge(top_ab);
                let mid = e.point_at(0.5);
                let tdir = e.point_at(0.51) - e.point_at(0.49);
                let right = Vec2::new(tdir.y, -tdir.x);
                let pm = self.map3(c.piece, mid, (z0 + z1) / 2.0);
                let outward = self.map3(c.piece, mid + right * 1e-3, (z0 + z1) / 2.0) - pm;
                faces.push(FaceSpec { loops: vec![vec![bottom, up_b, top, down_a]], surface, probe: Some((pm, outward)) });
            }
        }
        Ok(faces)
    }

    /// The folded (or, when unfolded, flat) solid, sewn from its faces.
    pub fn solid(&self) -> Result<Body> {
        if self.flat {
            return self.flat_solid();
        }
        let mut work = FlatWork::new();
        let cells = self.cells_with_work(&mut work)?;
        // Validate the bounded material first, then build close to the sheet's own
        // origin. Fitting planes after a distant component transfer loses precision.
        let mut local = self.clone();
        local.frame = Plane::XY;
        let body = kernel::sew(&local.faces(&cells, 0.0, self.t, &mut work)?)?;
        let body = local.machine_pockets(body, &mut work)?;
        self.place_local_solid(body)
    }

    /// The flat pattern as one extrusion (outline with its cut-outs).
    pub fn flat_solid(&self) -> Result<Body> {
        let regions = self.flat_regions()?;
        let mut local = self.clone();
        local.frame = Plane::XY;
        local.flat = true;
        let body = kernel::extrude_planar_regions(&Plane::XY, &regions, 0.0, self.t)?;
        let body = local.machine_pockets(body, &mut FlatWork::new())?;
        self.place_local_solid(body)
    }

    fn place_local_solid(&self, body: Body) -> Result<Body> {
        if self.frame == Plane::XY { Ok(body) } else { Ok(kernel::transform_matrix(&body, self.frame_mat())?) }
    }

    /// Retain bounded derived machining records, without duplicating identical replay work.
    pub(crate) fn record_pocket(&mut self, region: Region2, z0: f64, z1: f64, work: &mut FlatWork) -> Result<()> {
        if !z0.is_finite() || !z1.is_finite() || z0 < 0.0 || z1 > self.t || z0 >= z1 {
            return Err(DocError::Invalid("invalid sheet pocket depth".into()));
        }
        if self.pockets.len() > 1000 {
            return Err(flat_budget_error());
        }
        let pocket = SheetPocket { region, z0, z1 };
        let (old_edges, old_points) = regions_complexity(self.pockets.iter().map(|p| &p.region))?;
        let (new_edges, new_points) = region_complexity(std::slice::from_ref(&pocket.region))?;
        work.charge(
            old_edges.saturating_add(old_points).saturating_add(new_edges.saturating_add(new_points).saturating_mul(self.pockets.len().max(1))),
        )?;
        if self.pockets.contains(&pocket) {
            return Ok(());
        }
        if self.pockets.len() >= 1000 {
            return Err(flat_budget_error());
        }
        if old_edges.saturating_add(new_edges) > MAX_FLAT_POINTS || old_points.saturating_add(new_points) > MAX_FLAT_POINTS {
            return Err(flat_budget_error());
        }
        self.pockets.push(pocket);
        Ok(())
    }

    /// Certify whether a planar panel has material outside a retained thickness interval.
    /// Classification uses exact planar arrangements at every pocket depth boundary.
    pub(crate) fn material_outside_depth(&self, panel: &Region2, z0: f64, z1: f64, work: &mut FlatWork) -> Result<bool> {
        if !z0.is_finite() || !z1.is_finite() || z0 < 0.0 || z1 > self.t || z0 >= z1 {
            return Err(DocError::Invalid("invalid sheet retained depth".into()));
        }
        if self.pockets.len() > 1000 {
            return Err(flat_budget_error());
        }
        regions_complexity(self.pockets.iter().map(|p| &p.region))?;
        let mut levels = vec![0.0, self.t, z0, z1];
        for p in &self.pockets {
            if !p.z0.is_finite() || !p.z1.is_finite() || p.z0 < 0.0 || p.z1 > self.t || p.z0 >= p.z1 {
                return Err(DocError::Invalid("invalid sheet pocket depth".into()));
            }
            levels.extend([p.z0, p.z1]);
        }
        levels.sort_by(f64::total_cmp);
        levels.dedup();
        work.charge(levels.len().saturating_mul(self.pockets.len().max(1)))?;
        for zs in levels.windows(2) {
            if zs[0] >= z0 && zs[1] <= z1 {
                continue;
            }
            let active: Vec<_> = self.pockets.iter().filter(|p| p.z0 <= zs[0] && p.z1 >= zs[1]).map(|p| p.region.clone()).collect();
            let removed = material_union(active, work)?;
            if !material_boolean(std::slice::from_ref(panel), &removed, kernel::BoolOp::Cut, work)?.is_empty() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Replay exact planar machining after forming the full-thickness material.
    fn machine_pockets(&self, mut body: Body, work: &mut FlatWork) -> Result<Body> {
        if self.pockets.len() > 1000 {
            return Err(flat_budget_error());
        }
        regions_complexity(self.pockets.iter().map(|p| &p.region))?;
        let panels = if self.flat {
            vec![(None, self.flat_regions()?)]
        } else {
            self.material_panels(None, work)?.into_iter().map(|(pi, r)| (pi, vec![r])).collect()
        };
        for pocket in &self.pockets {
            if !pocket.z0.is_finite() || !pocket.z1.is_finite() || pocket.z0 < 0.0 || pocket.z1 > self.t || pocket.z0 >= pocket.z1 {
                return Err(DocError::Invalid("invalid sheet pocket depth".into()));
            }
            if !self.flat {
                for flange in &self.flanges {
                    if flat_regions_overlap(&pocket.region, &self.zone(flange), work)? {
                        return Err(DocError::Invalid(
                            "a sheet pocket crosses a bend zone; planar pocket machining cannot yet preserve curved bend material".into(),
                        ));
                    }
                }
            }
            for (pi, panel) in &panels {
                let clipped = material_boolean(panel, std::slice::from_ref(&pocket.region), kernel::BoolOp::Intersect, work)?;
                if clipped.is_empty() {
                    continue;
                }
                let w = if self.flat { self.frame_mat() } else { self.world(*pi) };
                let plane = plane_of(&w, Vec3::ZERO, Vec3::X, Vec3::Y)?;
                // BRep replay is costlier than planar clipping; share the command budget.
                work.charge(100_000)?;
                let tool = kernel::extrude_planar_regions(&plane, &clipped, pocket.z0, pocket.z1)?;
                body = kernel::boolean(&body, &tool, kernel::BoolOp::Cut)?
                    .ok_or_else(|| DocError::Invalid("sheet pocket removes all sheet material".into()))?;
            }
        }
        Ok(body)
    }

    /// Actual curved material, used to certify unsupported folded machining before mutation:
    /// one solid per bend zone or nonparallel panel. Each is sewn on its own, because the
    /// selected pieces need not form a manifold solid together (two bends meeting at an open
    /// corner relief touch only along an edge).
    pub(crate) fn unmodeled_solids(&self, plane: &Plane) -> Result<Vec<Body>> {
        if self.flat || self.flanges.is_empty() {
            return Ok(Vec::new());
        }
        let mut work = FlatWork::new();
        let cells: Vec<_> = self
            .cells_with_work(&mut work)?
            .into_iter()
            .filter(|c| match c.piece {
                Piece::Zone(_) => true,
                Piece::Panel(pi) => apply_vector(&self.world(pi), Vec3::Z).cross(plane.normal()).len() > 1e-6,
            })
            .collect();
        cells.iter().map(|c| Ok(kernel::sew(&self.faces(std::slice::from_ref(c), 0.0, self.t, &mut work)?)?)).collect()
    }

    fn through_pocket_regions(&self, work: &mut FlatWork) -> Result<Vec<Region2>> {
        if self.pockets.is_empty() {
            return Ok(Vec::new());
        }
        if self.pockets.len() > 1000 {
            return Err(flat_budget_error());
        }
        regions_complexity(self.pockets.iter().map(|p| &p.region))?;
        let mut levels = vec![0.0, self.t];
        for p in &self.pockets {
            if !p.z0.is_finite() || !p.z1.is_finite() || p.z0 < 0.0 || p.z1 > self.t || p.z0 >= p.z1 {
                return Err(DocError::Invalid("invalid sheet pocket depth".into()));
            }
            levels.extend([p.z0, p.z1]);
        }
        levels.sort_by(f64::total_cmp);
        levels.dedup();
        work.charge(levels.len().saturating_mul(self.pockets.len()))?;
        // Most pockets leave an untouched thickness layer. Certify that cheap case
        // before arranging hundreds of unrelated profiles at every depth level.
        if levels.windows(2).any(|zs| !self.pockets.iter().any(|p| p.z0 <= zs[0] && p.z1 >= zs[1])) {
            return Ok(Vec::new());
        }
        let mut through: Option<Vec<Region2>> = None;
        for zs in levels.windows(2) {
            let active: Vec<_> = self.pockets.iter().filter(|p| p.z0 <= zs[0] && p.z1 >= zs[1]).map(|p| p.region.clone()).collect();
            if active.is_empty() {
                return Ok(Vec::new());
            }
            let removed = material_union(active, work)?;
            through = Some(match through {
                None => removed,
                Some(previous) => material_boolean(&previous, &removed, kernel::BoolOp::Intersect, work)?,
            });
            if through.as_ref().is_some_and(Vec::is_empty) {
                return Ok(Vec::new());
            }
        }
        Ok(through.unwrap_or_default())
    }

    /// Pocket operations clipped to the surviving flat material, with exact depth intervals.
    pub fn machining(&self) -> Result<Vec<SheetPocket>> {
        let material = self.flat_regions()?;
        let mut work = FlatWork::new();
        let mut out = Vec::new();
        for pocket in &self.pockets {
            for region in material_boolean(&material, std::slice::from_ref(&pocket.region), kernel::BoolOp::Intersect, &mut work)? {
                if out.len() >= 1000 {
                    return Err(flat_budget_error());
                }
                out.push(SheetPocket { region, z0: pocket.z0, z1: pocket.z1 });
            }
        }
        Ok(out)
    }

    /// Actual flat material boundaries after cut-outs. The same panel and cut regions build
    /// the folded solid; planar unions remove panel/bend seams and turn edge-crossing cuts
    /// into notches. Lines and circular arcs remain exact through clipping and merging.
    pub fn flat_regions(&self) -> Result<Vec<Region2>> {
        let regions = self.flat_material()?;
        if regions.is_empty() {
            return Err(DocError::Invalid("empty flat pattern".into()));
        }
        Ok(regions)
    }

    pub(crate) fn flat_material(&self) -> Result<Vec<Region2>> {
        let mut work = FlatWork::new();
        // Merge structural pieces before cutting: clipping each panel first introduces
        // coincident notch endpoints at seams, while a single material cut has no seams.
        if self.flanges.len().saturating_mul(2).saturating_add(1) > MAX_FLAT_PIECES || self.holes.len() > MAX_FLAT_PIECES {
            return Err(flat_budget_error());
        }
        let raw = self.flanges.iter().fold(self.base.outer.segs.len(), |n, f| n.saturating_add(f.shape.as_ref().map_or(4, Vec::len)));
        if raw > MAX_FLAT_POINTS {
            return Err(flat_budget_error());
        }
        let mut regions = Vec::new();
        let mut panel_points = 0usize;
        for (_, r) in self.panels() {
            let r = planar_region(&r)?;
            panel_points = panel_points.saturating_add(region_complexity(std::slice::from_ref(&r))?.1);
            if panel_points > MAX_FLAT_POINTS {
                return Err(flat_budget_error());
            }
            regions.push(r);
        }
        for f in &self.flanges {
            regions.push(planar_region(&self.zone(f))?);
            region_complexity(&regions)?;
        }
        let regions = material_union(regions, &mut work)?;
        let mut cuts = Vec::new();
        let mut cut_points = 0usize;
        for hole in &self.holes {
            let region = planar_region(hole)?;
            cut_points = cut_points.saturating_add(region_complexity(std::slice::from_ref(&region))?.1);
            if cut_points > MAX_FLAT_POINTS {
                return Err(flat_budget_error());
            }
            cuts.push(region);
        }
        cuts.extend(self.through_pocket_regions(&mut work)?);
        let cuts = material_union(cuts, &mut work)?;
        let regions = if cuts.is_empty() { regions } else { material_boolean(&regions, &cuts, kernel::BoolOp::Cut, &mut work)? };
        Ok(regions)
    }
    /// Outline loops of the flat pattern (flat coordinates): the pieces' edges, shared ones left out.
    pub fn outline(&self) -> Result<Vec<Vec<Vec2>>> {
        if self.flanges.len().saturating_mul(2).saturating_add(1) > MAX_FLAT_PIECES {
            return Err(flat_budget_error());
        }
        let mut polys: Vec<Vec<Vec2>> = vec![dedup(&flat_polyline(&self.base.outer.ccw())?)];
        for f in &self.flanges {
            polys.push(dedup(&flat_polyline(&self.zone(f).outer.ccw())?));
            if f.leg > 1e-9 {
                polys.push(dedup(&flat_polyline(&self.panel(f).outer.ccw())?));
            }
            flat_points(&polys)?;
        }
        let loops = merge_loops(&polys, &mut FlatWork::new())?;
        if loops.is_empty() {
            return Err(DocError::Invalid("empty flat pattern".into()));
        }
        Ok(loops)
    }

    /// The bends: angle, radius, direction, allowance and centre line (flat coordinates).
    pub fn bends(&self) -> Vec<BendInfo> {
        self.flanges
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let ba = self.allowance(f);
                let (u0, u1) = self.zone_span(f);
                let a = f.p + f.n * (ba / 2.0) + f.d * u0;
                BendInfo {
                    index: i + 1,
                    angle_deg: f.angle.abs().to_degrees(),
                    radius: f.radius,
                    up: f.angle >= 0.0,
                    allowance: ba,
                    length: u1 - u0,
                    start: a,
                    end: a + f.d * (u1 - u0),
                }
            })
            .collect()
    }

    /// Topology counted as Fusion does: a round hole is one face bounded by two closed circular
    /// edges (one vertex each), where the sewn solid splits it into arcs. Returns the change to
    /// the sewn (faces, edges, vertices).
    pub fn round_hole_correction(&self) -> (i64, i64, i64) {
        let (mut df, mut de, mut dv) = (0i64, 0i64, 0i64);
        let Ok(regions) = self.flat_regions() else { return (0, 0, 0) };
        for h in regions.iter().flat_map(|r| &r.holes) {
            let arcs: Vec<(Vec2, f64)> =
                h.segs.iter().filter_map(|s| if let Seg2::Arc { center, radius, .. } = s { Some((*center, *radius)) } else { None }).collect();
            let n = h.segs.len() as i64;
            let full =
                !arcs.is_empty() && arcs.len() == h.segs.len() && arcs.iter().all(|(c, r)| c.dist(arcs[0].0) < 1e-9 && (r - arcs[0].1).abs() < 1e-9);
            if full && n > 1 {
                df += 1 - n;
                de += 2 - 3 * n;
                dv += 2 - 2 * n;
            }
        }
        (df, de, dv)
    }

    /// Bounding size of the flat pattern (x, y in the flat frame).
    pub fn flat_size(&self) -> Result<(f64, f64)> {
        let loops: Vec<Vec<Vec2>> = self.flat_regions()?.iter().map(|r| r.outer.polyline(1e-3)).collect();
        let (mut lo, mut hi) = (Vec2::new(f64::MAX, f64::MAX), Vec2::new(f64::MIN, f64::MIN));
        for p in loops.iter().flatten() {
            lo = Vec2::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Vec2::new(hi.x.max(p.x), hi.y.max(p.y));
        }
        Ok((hi.x - lo.x, hi.y - lo.y))
    }

    /// Panels with their regions and world placements: (flange index or None, region).
    fn panels(&self) -> Vec<(Option<usize>, Region2)> {
        let mut v = vec![(None, self.base.clone())];
        for (i, f) in self.flanges.iter().enumerate() {
            if f.leg > 1e-9 {
                v.push((Some(i), self.panel(f)));
            }
        }
        v
    }

    /// The panel edge nearest a world point: (panel, edge a, edge b in flat coordinates, on the
    /// top face?, distance).
    pub fn edge_at(&self, p: Vec3) -> Option<(Option<usize>, Vec2, Vec2, bool, f64)> {
        let mut best: Option<(Option<usize>, Vec2, Vec2, bool, f64)> = None;
        for (pi, r) in self.panels() {
            let w = self.world(pi);
            let Some(inv) = mat_inverse(&w) else { continue };
            let q = apply_point(&inv, p);
            let pts = r.outer.ccw().polyline(1e-3);
            let m = pts.len();
            for i in 0..m {
                let (Some(&a), Some(&b)) = (pts.get(i), pts.get((i + 1) % m)) else { continue };
                if a.dist(b) < 1e-9 {
                    continue;
                }
                for (z, top) in [(0.0, false), (self.t, true)] {
                    let q2 = Vec2::new(q.x, q.y);
                    let d = b - a;
                    let t = ((q2 - a).dot(d) / d.len2()).clamp(0.0, 1.0);
                    let dist = ((a + d * t) - q2).len().hypot(q.z - z);
                    if best.as_ref().is_none_or(|x| dist < x.4) {
                        best = Some((pi, a, b, top, dist));
                    }
                }
            }
        }
        best
    }

    /// Trim a panel's edge a→b back by `by` (toward the panel interior).
    fn trim(&mut self, panel: Option<usize>, a: Vec2, b: Vec2, by: f64) -> Result<()> {
        if by.abs() < 1e-12 {
            return Ok(());
        }
        let d = (b - a).normalized().ok_or_else(|| DocError::Invalid("edge".into()))?;
        // Interior side of a counter-clockwise loop is to the left.
        let inward = Vec2::new(-d.y, d.x);
        match panel {
            None => {
                let pts = self.base.outer.ccw().polyline(1e-3);
                let o = a + inward * by;
                let mut cut = clip(&pts, o, inward);
                // Only the material along this edge goes: other parts of the outline past the
                // line (tabs side by side on one line) stay.
                let mut work = FlatWork::new();
                let off = merge_loops(&[clip(&pts, o, inward * -1.0)], &mut work)?;
                if off.len() > 1 {
                    let mid = (a + b) * 0.5;
                    let touches = |p: &Vec<Vec2>| {
                        let m = p.len();
                        (0..m).filter_map(|i| Some((*p.get(i)?, *p.get((i + 1) % m)?))).any(|(p0, p1)| {
                            let e = p1 - p0;
                            let t = if e.len2() > 1e-18 { ((mid - p0).dot(e) / e.len2()).clamp(0.0, 1.0) } else { 0.0 };
                            (p0 + e * t).dist(mid) < 1e-6
                        })
                    };
                    let mut keep = vec![cut];
                    keep.extend(off.into_iter().filter(|p| !touches(p)));
                    cut = match merge_loops(&keep, &mut work)?.as_slice() {
                        [one] => one.clone(),
                        _ => return Err(DocError::Invalid("not supported yet: a flange setback that splits the sheet".into())),
                    };
                }
                if cut.len() < 3 {
                    return Err(DocError::Invalid("the flange's setback removes the whole panel".into()));
                }
                self.base = Region2 { outer: Loop2::polygon(&cut).ccw(), holes: self.base.holes.clone() };
            }
            Some(i) => {
                // A flange panel: its far edge shortens the leg; side edges are not trimmed.
                let Some(f) = self.flanges.get_mut(i) else { return Err(DocError::Invalid("flange".into())) };
                if inward.dot(f.n * -1.0) > 0.99 {
                    f.leg -= by;
                    if f.leg < -1e-9 {
                        return Err(DocError::Invalid("the flange's setback is longer than the panel".into()));
                    }
                } else {
                    return Err(DocError::Invalid("not supported yet: a flange on the side of another flange".into()));
                }
            }
        }
        Ok(())
    }

    /// Add an edge flange on the panel edge nearest `pick` (world): `height` to the outer face
    /// from the virtual sharp, `angle` (0…180°), inside / outside / middle bend position.
    pub fn add_flange(&mut self, pick: Vec3, height: f64, angle: f64, radius: f64, position: &str, flip: bool) -> Result<usize> {
        self.add_flanges(&[pick], height, angle, radius, position, flip)?.into_iter().next().ok_or_else(|| DocError::Invalid("flange".into()))
    }

    /// Edge flanges on several edges at once: every edge is trimmed back first, so flanges on
    /// adjacent edges stop at each other's bends; the corners between them are then closed with
    /// the rule's gap and given the rule's relief (`close_corners`).
    pub fn add_flanges(&mut self, picks: &[Vec3], height: f64, angle: f64, radius: f64, position: &str, flip: bool) -> Result<Vec<usize>> {
        self.add_flanges_noted(picks, height, angle, radius, position, flip).map(|(made, _)| made)
    }

    /// [`SheetBody::add_flanges`], also returning notes on corners that could not be closed
    /// (they are left open, as the bends leave them) or relieved (they get no relief).
    pub fn add_flanges_noted(
        &mut self,
        picks: &[Vec3],
        height: f64,
        angle: f64,
        radius: f64,
        position: &str,
        flip: bool,
    ) -> Result<(Vec<usize>, Vec<String>)> {
        let each: Vec<EdgeFlange> = picks.iter().map(|&pick| EdgeFlange { pick, height, angle }).collect();
        self.add_flanges_each(&each, radius, position, flip)
    }

    /// Edge flanges that each have their own height and angle, in one go: every edge is trimmed
    /// back by its own setback first, then the flanges are added and the corners between them
    /// are closed. The closing works from the two flanges it joins (each one's own fold and
    /// leg), so unequal flanges close like equal ones.
    pub fn add_flanges_each(&mut self, edges: &[EdgeFlange], radius: f64, position: &str, flip: bool) -> Result<(Vec<usize>, Vec<String>)> {
        if edges.is_empty() || edges.len() > 1000 {
            return Err(DocError::Invalid("pick 1…1000 edges".into()));
        }
        // With several edges a message says which one it is about.
        let on = |k: usize| if edges.len() > 1 { format!("edge {}: ", k + 1) } else { String::new() };
        // Each edge's setback and leg (the panel past the bend) follow its own angle and height.
        let mut sizes = Vec::with_capacity(edges.len());
        for (k, e) in edges.iter().enumerate() {
            if !(e.angle > 1e-6 && e.angle <= std::f64::consts::PI - 1e-6) {
                return Err(DocError::Invalid(format!("{}the flange angle must be between 0 and 180°", on(k))));
            }
            let sharp = (radius + self.t) * (e.angle / 2.0).tan();
            let setback = match position {
                "inside" | "" => sharp,
                "outside" => 0.0,
                "middle" | "mid" => sharp / 2.0,
                other => return Err(DocError::Invalid(format!("unknown bend position `{other}` (inside, outside or middle)"))),
            };
            let leg = e.height - sharp;
            if !(leg >= -1e-9) {
                return Err(DocError::Invalid(format!("{}the flange height must be at least {sharp:.3} mm for this bend", on(k))));
            }
            sizes.push((setback, leg));
        }
        // Find every edge before anything moves.
        let mut found = Vec::new();
        for (e, &(setback, leg)) in edges.iter().zip(&sizes) {
            let (panel, a, b, top, dist) = self.edge_at(e.pick).ok_or_else(|| DocError::Invalid("no sheet edge there".into()))?;
            if dist > self.t * 2.0 + 1e-3 {
                return Err(DocError::Invalid("pick an edge of the sheet's top or bottom face".into()));
            }
            let d = (b - a).normalized().ok_or_else(|| DocError::Invalid("edge".into()))?;
            found.push((panel, a, b, d, top, setback, leg, e.angle));
        }
        for (panel, a, b, _, _, setback, _, _) in &found {
            self.trim(*panel, *a, *b, *setback)?;
        }
        let mut made = Vec::new();
        for (panel, a, b, d, top, setback, leg, angle) in found {
            let outward = Vec2::new(d.y, -d.x);
            let line_o = a - outward * setback;
            // The trimmed edge: along d, on the moved line.
            let (start, len) = match panel {
                None => {
                    let pts = self.base.outer.ccw().polyline(1e-3);
                    let m = pts.len();
                    let on_line: Vec<(Vec2, f64)> = (0..m)
                        .filter_map(|k| {
                            let (p0, p1) = (*pts.get(k)?, *pts.get((k + 1) % m)?);
                            let e = p1 - p0;
                            let on = (p0 - line_o).dot(outward).abs() < 1e-6 && (p1 - line_o).dot(outward).abs() < 1e-6;
                            (on && e.dot(d) > 0.0).then_some((p0, e.len()))
                        })
                        .collect();
                    // Of several edges on the line (tabs side by side), the one the pick was on.
                    let (ua, ub) = (a.dot(d), b.dot(d));
                    on_line
                        .iter()
                        .find(|(p0, len)| (p0.dot(d) + len).min(ub) - p0.dot(d).max(ua) > 1e-9)
                        .or_else(|| on_line.first())
                        .copied()
                        .ok_or_else(|| DocError::Invalid("the flange's edge was trimmed away".into()))?
                }
                Some(i) => {
                    let f = self.flanges.get(i).ok_or_else(|| DocError::Invalid("flange".into()))?;
                    let u0 = (line_o - f.p).dot(f.d).clamp(0.0, f.len);
                    (f.p + f.n * (self.allowance(f) + f.leg) + f.d * u0, f.len)
                }
            };
            let up = top != flip;
            self.flanges.push(SheetFlange {
                parent: panel,
                p: snap(start),
                d,
                len,
                n: outward,
                angle: if up { angle } else { -angle },
                radius,
                leg: leg.max(0.0),
                shape: None,
                tag: String::new(),
                ext: (0.0, 0.0),
                slope: (0.0, 0.0),
            });
            made.push(self.flanges.len() - 1);
        }
        let notes = self.close_corners(&made);
        Ok((made, notes))
    }

    /// Corners where two of the new flanges' bends meet on the base: both bend zones end at the
    /// corner point. Hems and fold panels are left out, and so is a corner with a flange of an
    /// earlier feature: that one keeps its length and the new flange ends at its bend, as before.
    fn corners(&self, made: &[usize]) -> Vec<Corner> {
        let limit = std::f64::consts::PI - 1e-6;
        let ends: Vec<(usize, bool, Vec2)> = self
            .flanges
            .iter()
            .enumerate()
            .filter(|(_, f)| f.parent.is_none() && f.shape.is_none() && f.angle.abs() < limit)
            .flat_map(|(i, f)| {
                let (u0, u1) = self.zone_span(f);
                [(i, false, f.p + f.d * u0), (i, true, f.p + f.d * u1)]
            })
            .collect();
        let mut out = Vec::new();
        for (k, &(a, a_end, c)) in ends.iter().enumerate() {
            for &(b, b_end, c2) in ends.iter().skip(k + 1) {
                if a != b && c.dist(c2) < 1e-6 && made.contains(&a) && made.contains(&b) {
                    out.push(Corner { a, a_end, b, b_end, c });
                }
            }
        }
        out
    }

    /// Close the corners the new flanges make, then cut the rule's relief at each. Returns a
    /// note for every corner left open or without its relief.
    fn close_corners(&mut self, made: &[usize]) -> Vec<String> {
        let corners = self.corners(made);
        let mut notes = Vec::new();
        let at = |s: &SheetBody, c: Vec2| {
            let w = apply_point(&s.frame_mat(), v3(c));
            format!("({:.3}, {:.3}, {:.3})", w.x, w.y, w.z)
        };
        for k in &corners {
            if let Err(why) = self.close_corner(k) {
                notes.push(format!("the flanges at {} are left open: {why}", at(self, k.c)));
            }
        }
        // The reliefs come last: they move the bends' ends, which is how the corners were found.
        if self.relief != ReliefShape::Notch {
            for k in &corners {
                if let Err(why) = self.relieve_corner(k, &corners) {
                    notes.push(format!("no corner relief at {}: {why}", at(self, k.c)));
                }
            }
        }
        notes
    }

    /// Run two flanges' panels on toward each other (or cut them back) until their ends meet the
    /// plane halfway between the flanges with the rule's gap between them. The ends stay square
    /// to the sheet and follow lines in the flat, slanted when the flanges aren't at 90°, so that
    /// the edges nearest each other (on the inner faces) are `gap` apart along their length. Any
    /// angles, also unequal ones, at any convex corner of the base.
    fn close_corner(&mut self, k: &Corner) -> std::result::Result<(), String> {
        let (Some(fa), Some(fb)) = (self.flanges.get(k.a).cloned(), self.flanges.get(k.b).cloned()) else {
            return Err("a flange is missing".into());
        };
        let ea = if k.a_end { fa.d * -1.0 } else { fa.d };
        let eb = if k.b_end { fb.d * -1.0 } else { fb.d };
        // At an inside corner of the base the flanges turn away from each other.
        if ea.dot(fb.n) > -1e-9 || eb.dot(fa.n) > -1e-9 {
            return Ok(());
        }
        if fa.leg <= 1e-9 || fb.leg <= 1e-9 {
            // No panel past a bend: the bends meet at the corner, there is nothing to close.
            return Ok(());
        }
        if (fa.angle >= 0.0) != (fb.angle >= 0.0) {
            return Err("the flanges bend opposite ways".into());
        }
        let gap = self.gap;
        if !(gap >= MIN_CORNER_GAP) {
            return Err(format!("the flange gap {gap:.4} mm is below {MIN_CORNER_GAP} mm"));
        }
        let (wa, wb) = (self.fold(k.a), self.fold(k.b));
        // The inner faces (toward the bends' axes), with normals pointing from the outer face in.
        let (z_in, sign) = if fa.angle >= 0.0 { (self.t, 1.0) } else { (0.0, -1.0) };
        let na = apply_vector(&wa, Vec3::Z) * sign;
        let nb = apply_vector(&wb, Vec3::Z) * sign;
        let pa = apply_point(&wa, v3(k.c) + Vec3::Z * z_in);
        let pb = apply_point(&wb, v3(k.c) + Vec3::Z * z_in);
        let m = (nb - na).len();
        if !(m > 1e-6) {
            return Err("the flanges are parallel".into());
        }
        // Signed distance from the plane bisecting the inner faces, positive on flange a's side.
        let bisector = |p: Vec3| (nb.dot(p - pb) - na.dot(p - pa)) / m;
        // A flange's end: its panel keeps u ≥ u_end(s) (u from the corner along its bend, s from
        // the bend's start across it), so that every point of the end face is gap/2 or more from
        // the bisecting plane. The distance is affine in (u, s, z), so the end is a line in the
        // flat: returns u_end at the panel's bend edge and how much further the panel runs on per
        // unit of height.
        let end = |f: &SheetFlange, w: &Mat, e: Vec2, sign: f64| -> std::result::Result<(f64, f64), String> {
            let at = |u: f64, s: f64, z: f64| sign * bisector(apply_point(w, v3(k.c + e * u + f.n * s) + Vec3::Z * z));
            let c0 = at(0.0, 0.0, 0.0);
            let (cu, cs, cz) = (at(1.0, 0.0, 0.0) - c0, at(0.0, 1.0, 0.0) - c0, at(0.0, 0.0, 1.0) - c0);
            if !(cu > 1e-6) || ![c0, cs, cz].iter().all(|x| x.is_finite()) {
                return Err("the flanges don't meet across the corner".into());
            }
            let ba = self.allowance(f);
            // At 90° the end is square to the bend: keep the panel an exact rectangle.
            let slope = if (cs / cu).abs() < 1e-12 { 0.0 } else { cs / cu };
            Ok(((gap / 2.0 - c0 - (cz * self.t).min(0.0) - cs * ba) / cu, slope))
        };
        let mut new = [fa.clone(), fb.clone()];
        for (f, (e, at_end, sign)) in new.iter_mut().zip([(ea, k.a_end, 1.0), (eb, k.b_end, -1.0)]) {
            let w = if sign > 0.0 { &wa } else { &wb };
            let (u_end, slope) = end(f, w, e, sign)?;
            let tc = (k.c - f.p).dot(f.d);
            if at_end {
                f.ext.1 = tc - u_end - f.len;
                f.slope.1 = slope;
            } else {
                f.ext.0 = -(tc + u_end);
                f.slope.0 = slope;
            }
            let bottom = f.len + f.ext.0 + f.ext.1;
            let top = bottom + (f.slope.0 + f.slope.1) * f.leg;
            if !(bottom > 1e-6 && top > 1e-6) {
                return Err("the gap would cut a panel away".into());
            }
        }
        // The flat pattern must stay one sheet without overlaps.
        let [new_a, new_b] = new;
        let (qa, qb) = (corners_of(&self.panel(&new_a)), corners_of(&self.panel(&new_b)));
        if convex_overlap(&qa, &qb) > 1e-9
            || convex_overlap(&qa, &corners_of(&self.zone(&fb))) > 1e-9
            || convex_overlap(&qb, &corners_of(&self.zone(&fa))) > 1e-9
        {
            return Err("closed, the flanges would overlap in the flat pattern".into());
        }
        if let Some(f) = self.flanges.get_mut(k.a) {
            *f = new_a;
        }
        if let Some(f) = self.flanges.get_mut(k.b) {
            *f = new_b;
        }
        Ok(())
    }

    /// Cut the rule's relief where two bends meet at a corner of the base, `relief_size / 2`
    /// from the corner along both edges: a round about the corner through the angle the base
    /// fills there, or the square with that side (a kite where the corner isn't square, its
    /// cuts square to the two edges). The base's edges then start that far from the corner, so
    /// both bends stop there, cut straight across (`zone_span`): their ends stay exact arcs,
    /// the round runs on into them tangentially, and the bend table gets the shorter bends.
    /// The flange panels keep the ends `close_corner` gave them. A relief too small to cut, one
    /// that would leave nothing of an edge it starts on (with the relief at that edge's other
    /// end, where there is one), or one reaching as far from the corner as another edge of the
    /// base is, is refused with the reason.
    fn relieve_corner(&mut self, k: &Corner, corners: &[Corner]) -> std::result::Result<(), String> {
        let size = self.relief_size;
        if !(size >= MIN_CORNER_RELIEF) {
            return Err(format!("the relief size {size:.4} mm is below {MIN_CORNER_RELIEF} mm"));
        }
        let r = size / 2.0;
        let segs = self.base.outer.ccw().segs;
        let n = segs.len();
        // The base's edges at the corner: `i` ends there and the next one starts there.
        let at_corner =
            |i: usize| segs.get(i).is_some_and(|s| s.end().dist(k.c) < 1e-6) && segs.get((i + 1) % n).is_some_and(|s| s.start().dist(k.c) < 1e-6);
        let Some(i) = (0..n).find(|&i| at_corner(i)) else { return Err("the bends don't meet at a corner of the base".into()) };
        let j = (i + 1) % n;
        let (Some(&Seg2::Line { a: pa, b: c }), Some(&Seg2::Line { b: pb, .. })) = (segs.get(i), segs.get(j)) else {
            return Err("the base's edges at the corner are not straight".into());
        };
        let (Some(da), Some(db)) = ((pa - c).normalized(), (pb - c).normalized()) else {
            return Err("an edge of the base at the corner has no length".into());
        };
        // The angle the base fills at the corner.
        let alpha = db.cross(da).atan2(db.dot(da));
        if !(alpha > 1e-6 && alpha < std::f64::consts::PI - 1e-6) {
            // An inside corner of the base: the flanges turn away from each other.
            return Ok(());
        }
        // Both bends must keep some length. An edge whose far end is another of the corners has
        // to hold a relief there too, so the reliefs at its two ends fit or are left out together.
        let takes = |far: Vec2| if corners.iter().any(|q| q.c.dist(far) < 1e-6) { 2.0 * r } else { r };
        if takes(pa) >= pa.dist(c) - 1e-6 || takes(pb) >= pb.dist(c) - 1e-6 {
            return Err(format!("a relief {size:.3} mm across would take a whole bend away"));
        }
        // The relief from the edge that ends at the corner to the one that starts there, and
        // how far from the corner it reaches.
        let (relief, reach) = match self.relief {
            ReliefShape::Notch => return Ok(()),
            ReliefShape::Round => (vec![Seg2::Arc { center: c, radius: r, start: da.angle(), sweep: -alpha }], r),
            ReliefShape::Rectangular => {
                let x = snap(c + (da + db) * (r / (1.0 + da.dot(db))));
                (vec![Seg2::Line { a: snap(c + da * r), b: x }, Seg2::Line { a: x, b: snap(c + db * r) }], x.dist(c))
            }
        };
        let (Some(start), Some(end)) = (relief.first().map(Seg2::start), relief.last().map(Seg2::end)) else { return Ok(()) };
        // The rest of the outline must stay clear of it.
        let clear = segs.iter().enumerate().filter(|(e, _)| *e != i && *e != j).all(|(_, s)| {
            s.polyline(1e-3).windows(2).all(|w| {
                let d = w[1] - w[0];
                let t = if d.len2() > 1e-30 { ((c - w[0]).dot(d) / d.len2()).clamp(0.0, 1.0) } else { 0.0 };
                c.dist(w[0] + d * t) > reach + 1e-6
            })
        });
        if !clear {
            return Err(format!("a relief {size:.3} mm across reaches {reach:.3} mm from the corner, where another edge of the base is nearer"));
        }
        let mut outer = Vec::with_capacity(n + relief.len());
        for (e, s) in segs.iter().enumerate() {
            if e == i {
                outer.push(Seg2::Line { a: pa, b: start });
                outer.extend(relief.iter().copied());
            } else if e == j {
                outer.push(Seg2::Line { a: end, b: pb });
            } else {
                outer.push(*s);
            }
        }
        self.base = Region2 { outer: Loop2 { segs: outer }, holes: self.base.holes.clone() };
        Ok(())
    }

    /// A flat hem: a 180° bend with a tiny radius at the panel edge, `length` to its outer face.
    pub fn add_hem(&mut self, pick: Vec3, length: f64, gap: f64, flip: bool) -> Result<usize> {
        let (panel, a, b, top, dist) = self.edge_at(pick).ok_or_else(|| DocError::Invalid("no sheet edge there".into()))?;
        if dist > self.t * 2.0 + 1e-3 {
            return Err(DocError::Invalid("pick an edge of the sheet".into()));
        }
        let d = (b - a).normalized().ok_or_else(|| DocError::Invalid("edge".into()))?;
        let outward = Vec2::new(d.y, -d.x);
        let leg = length - (gap + self.t);
        if leg < 0.0 {
            return Err(DocError::Invalid("the hem is shorter than its bend".into()));
        }
        // A hem folds back over the face it starts from (toward the bottom unless flipped).
        let up = top != flip;
        let up = !up;
        self.flanges.push(SheetFlange {
            parent: panel,
            p: a,
            d,
            len: a.dist(b),
            n: outward,
            angle: if up { std::f64::consts::PI } else { -std::f64::consts::PI },
            radius: gap,
            leg,
            ext: (0.0, 0.0),
            slope: (0.0, 0.0),
            shape: None,
            tag: String::new(),
        });
        Ok(self.flanges.len() - 1)
    }

    /// Fold a panel (the base or a flange's) along a line on its top or bottom face (world
    /// points `a`, `b`). The bend zone (width BA = θ·(R + K·T)) is taken out of the existing
    /// sheet, so the flat pattern keeps its size; `position` puts the line at the zone's centre
    /// (`centerline`), start or end (`start`/`end`, from the fixed side), or on the mould line
    /// (`mould`: where the outer faces meet, so the fixed side measures the same to it folded as
    /// flat). On the base the side holding `fixed` (default: the larger side) stays put; on a
    /// flange the side toward its own bend does. `flip` folds the other way.
    #[allow(clippy::too_many_arguments)]
    pub fn add_fold(&mut self, a: Vec3, b: Vec3, angle: f64, radius: f64, position: &str, flip: bool, fixed: Option<Vec3>) -> Result<usize> {
        if !(angle > 1e-6 && angle <= std::f64::consts::PI - 1e-6) {
            return Err(DocError::Invalid("the fold angle must be between 0 and 180°".into()));
        }
        if !(radius >= 0.0) {
            return Err(DocError::Invalid("the bend radius must not be negative".into()));
        }
        // The panel the line lies on: both ends on its top or bottom face, the middle inside it.
        let on_face = |q: Vec3| q.z.abs() < 1e-4 || (q.z - self.t).abs() < 1e-4;
        let mut found = None;
        for (pi, r) in self.panels() {
            let Some(inv) = mat_inverse(&self.world(pi)) else { continue };
            let (fa, fb) = (apply_point(&inv, a), apply_point(&inv, b));
            if !(on_face(fa) && on_face(fb) && (fa.z - fb.z).abs() < 1e-4) {
                continue;
            }
            let pts = dedup(&r.outer.ccw().polyline(1e-3));
            let (a2, b2) = (Vec2::new(fa.x, fa.y), Vec2::new(fb.x, fb.y));
            if point_in(&pts, (a2 + b2) * 0.5) || polys_cross(&pts, &[a2, b2, (a2 + b2) * 0.5]) {
                found = Some((pi, inv, a2, b2, pts));
                break;
            }
        }
        let (panel, inv, a2, b2, pts) = found.ok_or_else(|| DocError::Invalid("the fold line must lie on a flat face of the sheet".into()))?;
        let d = (b2 - a2).normalized().ok_or_else(|| DocError::Invalid("the fold line has no length".into()))?;
        let mut n = Vec2::new(-d.y, d.x);
        let l0 = a2.dot(n);
        // Which side moves. A flange keeps the side its own bend is on.
        let on_line = n * l0;
        let side_area = |m: Vec2| signed_area(&clip(&pts, on_line, m)).abs();
        let moves_n = match (panel, fixed) {
            (Some(j), _) => {
                let f = self.flanges.get(j).ok_or_else(|| DocError::Invalid("flange".into()))?;
                let ba = self.allowance(f);
                // The panel's attaching edge must stay whole on the fixed side.
                let (e0, e1) = (f.p + f.n * ba - f.d * f.ext.0, f.p + f.n * ba + f.d * (f.len + f.ext.1));
                let (s0, s1) = (e0.dot(n) - l0, e1.dot(n) - l0);
                if s0 * s1 < -1e-12 {
                    return Err(DocError::Invalid("a fold on a flange must run across it, clear of the flange's own bend".into()));
                }
                (s0 + s1) < 0.0
            }
            (None, Some(p)) => {
                let q = apply_point(&inv, p);
                let q2 = Vec2::new(q.x, q.y);
                // A point in the panel's slab must be on material: inside the outline and
                // outside every cut-out (a side face of either counts as material).
                let near = |poly: &[Vec2]| {
                    let m = poly.len();
                    (0..m).any(|i| match (poly.get(i), poly.get((i + 1) % m)) {
                        (Some(&a), Some(&b)) => {
                            let d = b - a;
                            let t = if d.len2() > 1e-18 { ((q2 - a).dot(d) / d.len2()).clamp(0.0, 1.0) } else { 0.0 };
                            (a + d * t).dist(q2) < 1e-3
                        }
                        _ => false,
                    })
                };
                let in_slab = q.z > -1e-4 && q.z < self.t + 1e-4;
                let in_hole = self.holes.iter().any(|h| h.contains(q2) && !near(&h.outer.ccw().polyline(1e-3)));
                if in_slab && ((!point_in(&pts, q2) && !near(&pts)) || in_hole) {
                    return Err(DocError::Invalid(format!(
                        "the fixed point ({:.3}, {:.3}, {:.3}) is not on the sheet: pick a point on the face of the side that stays",
                        p.x, p.y, p.z
                    )));
                }
                q2.dot(n) < l0
            }
            (None, None) => side_area(n) <= side_area(n * -1.0),
        };
        if !moves_n {
            n = n * -1.0;
        }
        let l = a2.dot(n);
        let probe = SheetFlange {
            parent: panel,
            p: a2,
            d,
            len: 1.0,
            n,
            angle,
            radius,
            leg: 0.0,
            ext: (0.0, 0.0),
            slope: (0.0, 0.0),
            shape: None,
            tag: String::new(),
        };
        let ba = self.allowance(&probe);
        let s0 = match position {
            "centerline" | "center" | "centre" => l - ba / 2.0,
            "start" => l,
            "end" => l - ba,
            "mould" | "mold" => l - (radius + self.t) * (angle / 2.0).tan(),
            o => return Err(DocError::Invalid(format!("unknown fold position `{o}` (centerline, start, end or mould)"))),
        };
        let fixed_part = clip(&pts, n * s0, n * -1.0);
        let moving = clip(&pts, n * (s0 + ba), n);
        let strip = clip(&clip(&pts, n * s0, n), n * (s0 + ba), n * -1.0);
        if signed_area(&fixed_part).abs() < 1e-9 || signed_area(&moving).abs() < 1e-9 {
            return Err(DocError::Invalid("the fold line must cross the sheet with material on both sides of the bend".into()));
        }
        // Separate tabs on one line (the zone falls apart into several pieces): fold the ones
        // the line's points span, each as its own bend.
        let strips = merge_loops(std::slice::from_ref(&strip), &mut FlatWork::new())?;
        if strips.len() > 1 {
            let span = (a2.dot(d).min(b2.dot(d)), a2.dot(d).max(b2.dot(d)));
            let angle = if flip { -angle } else { angle };
            return self.fold_tabs(panel, &pts, (n, d), (s0, ba), span, (angle, radius), &strips);
        }
        // The bend zone must be a rectangle across the panel (a slanted or curved edge would
        // run across the bend as a helix, which the sheet's faces can't carry yet).
        let us: Vec<f64> = strip.iter().map(|q| q.dot(d)).collect();
        let (u0, u1) = (us.iter().cloned().fold(f64::MAX, f64::min), us.iter().cloned().fold(f64::MIN, f64::max));
        if ((u1 - u0) * ba - signed_area(&strip).abs()).abs() > 1e-6 * (u1 - u0).max(1.0) * ba.max(1.0) {
            // A corner of the outline inside the zone (e.g. a fold right at a tab's root): name
            // it and how far the line has to move to clear it.
            let corners: Vec<(Vec2, f64)> = pts.iter().map(|q| (*q, q.dot(n) - s0)).filter(|(_, s)| *s > 1e-6 && *s < ba - 1e-6).collect();
            if let Some(&(c, _)) = corners.first() {
                let on = corners.iter().fold(0.0_f64, |m, (_, s)| m.max(*s));
                let back = corners.iter().fold(0.0_f64, |m, (_, s)| m.max(ba - *s));
                let w = apply_point(&self.world(panel), Vec3::new(c.x, c.y, self.t));
                return Err(DocError::Invalid(format!(
                    "the bend zone ({ba:.3} mm wide) reaches a corner of the sheet's outline at ({:.3}, {:.3}, {:.3}): \
                     move the fold line at least {on:.3} mm toward the side that folds or {back:.3} mm toward the side that stays",
                    w.x, w.y, w.z
                )));
            }
            return Err(DocError::Invalid("not supported yet: folds whose bend zone crosses a slanted or curved edge of the sheet".into()));
        }
        let up = !flip;
        let leg = moving.iter().map(|q| q.dot(n) - s0 - ba).fold(0.0, f64::max);
        let p0 = n * s0 + d * u0;
        let ccw = |mut v: Vec<Vec2>| {
            if signed_area(&v) < 0.0 {
                v.reverse();
            }
            v.into_iter().map(snap).collect::<Vec<_>>()
        };
        match panel {
            None => self.base = Region2 { outer: Loop2::polygon(&fixed_part).ccw(), holes: self.base.holes.clone() },
            Some(j) => {
                if let Some(f) = self.flanges.get_mut(j) {
                    f.shape = Some(ccw(fixed_part.clone()));
                }
            }
        }
        self.flanges.push(SheetFlange {
            parent: panel,
            p: snap(p0),
            d,
            len: u1 - u0,
            n,
            angle: if up { angle } else { -angle },
            radius,
            leg,
            ext: (0.0, 0.0),
            slope: (0.0, 0.0),
            shape: Some(ccw(moving)),
            tag: String::new(),
        });
        let i = self.flanges.len() - 1;
        // Flanges on the moving part's edges now hang off the fold and move with it.
        for k in 0..i {
            let Some(f) = self.flanges.get(k) else { continue };
            if f.parent != panel {
                continue;
            }
            let (e0, e1) = ((f.p - p0).dot(n), (f.p + f.d * f.len - p0).dot(n));
            if e0.min(e1) >= ba - 1e-9 {
                if let Some(f) = self.flanges.get_mut(k) {
                    f.parent = Some(i);
                }
            } else if e0.max(e1) > 1e-9 {
                return Err(DocError::Invalid("not supported yet: a fold through a flange's bend".into()));
            }
        }
        // Cut-outs may cross the bend where their edges run along or across it (the faces
        // check the rest when the sheet is built).
        Ok(i)
    }

    /// A fold whose bend zone falls apart into several pieces (`strips`: tabs on one line). The
    /// pieces the line's points span (`span`, along `d`) bend, each with the material beyond it;
    /// the rest of the panel stays put. `n` points to the moving side, the zone is `s0..s0 + ba`.
    #[allow(clippy::too_many_arguments)]
    fn fold_tabs(
        &mut self,
        panel: Option<usize>,
        pts: &[Vec2],
        (n, d): (Vec2, Vec2),
        (s0, ba): (f64, f64),
        span: (f64, f64),
        (angle, radius): (f64, f64),
        strips: &[Vec<Vec2>],
    ) -> Result<usize> {
        const EPS: f64 = 1e-6;
        let urange = |p: &[Vec2]| p.iter().fold((f64::MAX, f64::MIN), |(lo, hi), q| (lo.min(q.dot(d)), hi.max(q.dot(d))));
        let overlap = |a: (f64, f64), b: (f64, f64)| a.1.min(b.1) - a.0.max(b.0);
        let mut work = FlatWork::new();
        // The material beyond the zone, piece by piece, and the zone piece each one hangs from.
        let beyond = merge_loops(&[clip(pts, n * (s0 + ba), n)], &mut work)?;
        let far_edges = |p: &[Vec2]| -> Vec<(f64, f64)> {
            let m = p.len();
            (0..m)
                .filter_map(|i| Some((*p.get(i)?, *p.get((i + 1) % m)?)))
                .filter(|(a, b)| (a.dot(n) - s0 - ba).abs() < EPS && (b.dot(n) - s0 - ba).abs() < EPS)
                .map(|(a, b)| (a.dot(d).min(b.dot(d)), a.dot(d).max(b.dot(d))))
                .collect()
        };
        let strip_u: Vec<(f64, f64)> = strips.iter().map(|s| urange(s)).collect();
        let mut hangs: Vec<Vec<usize>> = vec![Vec::new(); strips.len()];
        for (k, part) in beyond.iter().enumerate() {
            let edges = far_edges(part);
            let on: Vec<usize> = (0..strips.len()).filter(|&i| strip_u.get(i).is_some_and(|&u| edges.iter().any(|&e| overlap(e, u) > EPS))).collect();
            match on.as_slice() {
                [i] => {
                    if let Some(h) = hangs.get_mut(*i) {
                        h.push(k);
                    }
                }
                _ => return Err(DocError::Invalid("not supported yet: folding tabs that join again beyond the bend".into())),
            }
        }
        let chosen: Vec<usize> = (0..strips.len()).filter(|&i| strip_u.get(i).is_some_and(|&u| overlap(u, span) > EPS)).collect();
        if chosen.is_empty() {
            return Err(DocError::Invalid("the fold line must cross the sheet with material on both sides of the bend".into()));
        }
        // What stays: the panel short of the zone plus the zone pieces (and what hangs from
        // them) that don't fold.
        let mut keep = vec![clip(pts, n * s0, n * -1.0)];
        for i in 0..strips.len() {
            if chosen.contains(&i) {
                continue;
            }
            keep.extend(strips.get(i).cloned());
            keep.extend(hangs.get(i).into_iter().flatten().filter_map(|&k| beyond.get(k).cloned()));
        }
        let fixed = match merge_loops(&keep, &mut work)?.as_slice() {
            [one] if signed_area(one) > 1e-9 => one.clone(),
            _ => return Err(DocError::Invalid("not supported yet: a fold that splits the part that stays".into())),
        };
        let mut made = Vec::new();
        for &i in &chosen {
            let (Some(strip), Some(&(u0, u1))) = (strips.get(i), strip_u.get(i)) else { continue };
            // Each piece of the zone must be a rectangle (see `add_fold`).
            if ((u1 - u0) * ba - signed_area(strip).abs()).abs() > 1e-6 * (u1 - u0).max(1.0) * ba.max(1.0) {
                return Err(DocError::Invalid("not supported yet: folds whose bend zone crosses a slanted or curved edge of the sheet".into()));
            }
            let moving = match hangs.get(i).map(Vec::as_slice) {
                Some([k]) => beyond.get(*k).cloned().ok_or_else(|| DocError::Invalid("fold".into()))?,
                _ => return Err(DocError::Invalid("the fold line must cross the sheet with material on both sides of the bend".into())),
            };
            let leg = moving.iter().map(|q| q.dot(n) - s0 - ba).fold(0.0, f64::max);
            made.push((u0, u1, n * s0 + d * u0, leg, moving));
        }
        // Flanges already on this panel: those beyond a folding tab move with it.
        let mut moves = Vec::new();
        for k in 0..self.flanges.len() {
            let Some(f) = self.flanges.get(k) else { continue };
            if f.parent != panel {
                continue;
            }
            let (e0, e1) = (f.p.dot(n) - s0, (f.p + f.d * f.len).dot(n) - s0);
            let fu = (f.p.dot(d).min((f.p + f.d * f.len).dot(d)), f.p.dot(d).max((f.p + f.d * f.len).dot(d)));
            let crosses = Err(DocError::Invalid("not supported yet: a fold through a flange's bend".into()));
            match made.iter().position(|m| fu.0 >= m.0 - 1e-9 && fu.1 <= m.1 + 1e-9) {
                Some(j) if e0.min(e1) >= ba - 1e-9 => moves.push((k, j)),
                Some(_) if e0.max(e1) > 1e-9 => return crosses,
                None if e0.max(e1) > 1e-9 && made.iter().any(|m| overlap((m.0, m.1), fu) > EPS) => return crosses,
                _ => {}
            }
        }
        let ccw = |mut v: Vec<Vec2>| {
            if signed_area(&v) < 0.0 {
                v.reverse();
            }
            v.into_iter().map(snap).collect::<Vec<_>>()
        };
        match panel {
            None => self.base = Region2 { outer: Loop2::polygon(&fixed).ccw(), holes: self.base.holes.clone() },
            Some(j) => {
                if let Some(f) = self.flanges.get_mut(j) {
                    f.shape = Some(ccw(fixed));
                }
            }
        }
        let first = self.flanges.len();
        for (u0, u1, p0, leg, moving) in made {
            self.flanges.push(SheetFlange {
                parent: panel,
                p: snap(p0),
                d,
                len: u1 - u0,
                n,
                angle,
                radius,
                leg,
                ext: (0.0, 0.0),
                slope: (0.0, 0.0),
                shape: Some(ccw(moving)),
                tag: String::new(),
            });
        }
        for (k, j) in moves {
            if let Some(f) = self.flanges.get_mut(k) {
                f.parent = Some(first + j);
            }
        }
        Ok(first)
    }

    /// Name a panel edge (flat coordinates a→b on `panel`, top or bottom face): base edges by the
    /// base sketch line they lie on, flange edges by the flange (its tag) and side.
    fn edge_name_of(&self, panel: Option<usize>, a: Vec2, b: Vec2, top: bool) -> Option<String> {
        let face = if top { "top" } else { "bottom" };
        let d = (b - a).normalized()?;
        match panel {
            None => {
                let on = |p: Vec2, s: Vec2, e: Vec2| {
                    let t = e - s;
                    t.len() > 1e-9 && ((p - s).cross(t) / t.len()).abs() < 1e-6
                };
                self.base_curves.iter().find(|(_, s, e)| on(a, *s, *e) && on(b, *s, *e)).map(|(id, _, _)| format!("base:{id}:{face}"))
            }
            Some(i) => {
                let f = self.flanges.get(i)?;
                if f.tag.is_empty() {
                    return None;
                }
                let side = if d.cross(f.d).abs() < 1e-6 {
                    // Along the bend: the far edge (or the bend side, never picked).
                    if ((a - f.p).dot(f.n)) > self.allowance(f) + 1e-6 { "far" } else { "bend" }
                } else if ((a + b) * 0.5 - f.p).dot(f.d) < f.len / 2.0 {
                    "side0"
                } else {
                    "side1"
                };
                Some(format!("{}:{side}:{face}", f.tag))
            }
        }
    }

    /// The persistent name of the sheet edge nearest a world point.
    pub fn edge_name(&self, pick: Vec3) -> Option<String> {
        let (panel, a, b, top, _) = self.edge_at(pick)?;
        self.edge_name_of(panel, a, b, top)
    }

    /// A world point on the sheet edge with this name (the one nearest `near` if several).
    pub fn edge_by_name(&self, name: &str, near: Vec3) -> Option<Vec3> {
        let mut best: Option<(f64, Vec3)> = None;
        for (pi, r) in self.panels() {
            let pts = r.outer.ccw().polyline(1e-3);
            let m = pts.len();
            let w = self.world(pi);
            for i in 0..m {
                let (Some(&a), Some(&b)) = (pts.get(i), pts.get((i + 1) % m)) else { continue };
                if a.dist(b) < 1e-9 {
                    continue;
                }
                for (z, top) in [(0.0, false), (self.t, true)] {
                    if self.edge_name_of(pi, a, b, top).as_deref() == Some(name) {
                        let mid = (a + b) * 0.5;
                        let q = apply_point(&w, Vec3::new(mid.x, mid.y, z));
                        if best.is_none_or(|(d, _)| q.dist(near) < d) {
                            best = Some((q.dist(near), q));
                        }
                    }
                }
            }
        }
        best.map(|(_, q)| q)
    }

    /// The persistent name of a panel: `base`, a flange's tag, or `#<index>` for an untagged
    /// flange; `flat` for the whole sheet while it is unfolded.
    fn panel_name(&self, pi: Option<usize>) -> String {
        match pi {
            _ if self.flat => "flat".into(),
            None => "base".into(),
            Some(i) => match self.flanges.get(i) {
                Some(f) if !f.tag.is_empty() => f.tag.clone(),
                _ => format!("#{i}"),
            },
        }
    }

    /// The plane of a flat face of a panel (`top`: the face at the thickness, else the bottom
    /// one), facing out of the sheet. Its coordinates are the flat pattern's (the bottom
    /// face's mirrored in y), so a sketch on it measures as the flat pattern does.
    fn panel_face_plane(&self, world: &Mat, top: bool) -> Option<Plane> {
        let (x, y) = (apply_vector(world, Vec3::X), apply_vector(world, Vec3::Y));
        Plane::new(apply_point(world, Vec3::new(0.0, 0.0, if top { self.t } else { 0.0 })), x, if top { y } else { y * -1.0 })
    }

    /// The flat face of the sheet that a point lies on (sheet coordinates, within `tol`, which
    /// stays below a quarter of the thickness): its persistent name (`<panel>:top` or
    /// `<panel>:bottom`) and its plane (see [`panel_face`](Self::panel_face)).
    pub fn panel_face_at(&self, p: Vec3, tol: f64) -> Option<(String, Plane)> {
        let tol = tol.min(self.t * 0.25);
        let flat = if self.flat { self.flat_regions().ok()? } else { Vec::new() };
        let panels = if self.flat { vec![(None, self.base.clone())] } else { self.panels() };
        for (pi, region) in panels {
            let world = self.world(pi);
            let Some(inverse) = mat_inverse(&world) else { continue };
            let q = apply_point(&inverse, p);
            let top = (q.z - self.t).abs() < q.z.abs();
            if (if top { (q.z - self.t).abs() } else { q.z.abs() }) > tol {
                continue;
            }
            let q2 = Vec2::new(q.x, q.y);
            if if self.flat { flat.iter().any(|r| r.contains(q2)) } else { region.contains(q2) } {
                return Some((format!("{}:{}", self.panel_name(pi), if top { "top" } else { "bottom" }), self.panel_face_plane(&world, top)?));
            }
        }
        None
    }

    /// The plane of the flat face with this persistent name (see [`panel_face_at`](Self::panel_face_at)).
    /// A face named while the sheet was folded follows its panel; one named unfolded (`flat`)
    /// is the flat sheet's own plane.
    pub fn panel_face(&self, name: &str) -> Option<Plane> {
        let (panel, side) = name.rsplit_once(':')?;
        let top = match side {
            "top" => true,
            "bottom" => false,
            _ => return None,
        };
        let pi = match panel {
            "flat" | "base" => None,
            tag => Some(
                self.flanges
                    .iter()
                    .position(|f| f.tag == tag)
                    .or_else(|| tag.strip_prefix('#')?.parse::<usize>().ok().filter(|i| *i < self.flanges.len()))?,
            ),
        };
        self.panel_face_plane(&self.world(pi), top)
    }

    /// Map a region in a world plane to flat coordinates through a panel's placement.
    pub fn region_to_flat(&self, panel: Option<usize>, plane: &Plane, r: &Region2) -> Result<Region2> {
        region_complexity(std::slice::from_ref(r))?;
        let inv = mat_inverse(&self.world(panel)).ok_or_else(|| DocError::Invalid("invalid sheet panel frame".into()))?;
        let map = |p: Vec2| {
            let q = apply_point(&inv, plane.to_world(p));
            Vec2::new(q.x, q.y)
        };
        // A rigid map of the plane: lines and arcs stay lines and arcs (mirrored or not).
        let (o, ex, ey) = (map(Vec2::default()), map(Vec2::new(1.0, 0.0)), map(Vec2::new(0.0, 1.0)));
        let (ax, ay) = (ex - o, ey - o);
        let mirrored = ax.cross(ay) < 0.0;
        let rot = ax.y.atan2(ax.x);
        let seg = |sg: &Seg2| -> Seg2 {
            match *sg {
                Seg2::Line { a, b } => Seg2::Line { a: map(a), b: map(b) },
                Seg2::Arc { center, radius, start, sweep } => {
                    if mirrored {
                        Seg2::Arc { center: map(center), radius, start: rot - start, sweep: -sweep }
                    } else {
                        Seg2::Arc { center: map(center), radius, start: rot + start, sweep }
                    }
                }
                other => Seg2::Line { a: map(other.start()), b: map(other.end()) },
            }
        };
        let map_loop = |lp: &Loop2| -> Result<Loop2> {
            let curved_other = lp.segs.iter().any(|x| !matches!(x, Seg2::Line { .. } | Seg2::Arc { .. }));
            if curved_other {
                Ok(Loop2::polygon(&flat_polyline(lp)?.into_iter().map(map).collect::<Vec<_>>()))
            } else {
                Ok(Loop2 { segs: lp.segs.iter().map(seg).collect() })
            }
        };
        Ok(Region2 {
            outer: map_loop(&r.outer)?.ccw(),
            holes: r.holes.iter().map(|h| map_loop(h).map(|l| l.ccw().reversed())).collect::<Result<_>>()?,
        })
    }

    /// Panels whose plane is parallel to `plane` (world): (panel, region).
    pub fn parallel_panels(&self, plane: &Plane) -> Vec<(Option<usize>, Region2)> {
        self.panels()
            .into_iter()
            .filter(|(pi, _)| {
                let w = self.world(*pi);
                let n = apply_vector(&w, Vec3::Z);
                n.cross(plane.normal()).len() < 1e-6
            })
            .collect()
    }

    pub(crate) fn material_panels(&self, plane: Option<&Plane>, work: &mut FlatWork) -> Result<Vec<(Option<usize>, Region2)>> {
        Ok(self
            .cells_with_work(work)?
            .into_iter()
            .filter_map(|c| {
                let Piece::Panel(pi) = c.piece else {
                    return None;
                };
                if plane.is_some_and(|p| apply_vector(&self.world(pi), Vec3::Z).cross(p.normal()).len() > 1e-6) {
                    return None;
                }
                Some((pi, Region2 { outer: Loop2 { segs: c.outer }, holes: c.holes.into_iter().map(|segs| Loop2 { segs }).collect() }))
            })
            .collect())
    }
}

fn point_in(poly: &[Vec2], p: Vec2) -> bool {
    let mut inside = false;
    let m = poly.len();
    for i in 0..m {
        let (Some(&a), Some(&b)) = (poly.get(i), poly.get((i + m - 1) % m)) else { continue };
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

/// The boundary of a union of counter-clockwise polygons that meet edge to edge: edges are
/// split where corners touch them, edges running both ways cancel, the rest chain into loops
/// (outer loops counter-clockwise, holes clockwise; straight runs merged).
fn merge_loops(polys: &[Vec<Vec2>], work: &mut FlatWork) -> Result<Vec<Vec<Vec2>>> {
    let n = flat_points(polys)?;
    work.charge(n.saturating_mul(n).saturating_mul(2))?;
    let mut segs: Vec<(Vec2, Vec2)> = Vec::new();
    for p in polys {
        let m = p.len();
        for i in 0..m {
            if let (Some(&a), Some(&b)) = (p.get(i), p.get((i + 1) % m)) {
                segs.push((a, b));
            }
        }
    }
    let pts: Vec<Vec2> = segs.iter().flat_map(|(a, b)| [*a, *b]).collect();
    let mut split: Vec<(Vec2, Vec2)> = Vec::new();
    for (a, b) in &segs {
        let d = *b - *a;
        let l2 = d.len2();
        if l2 < 1e-18 {
            continue;
        }
        let mut ts: Vec<f64> = pts
            .iter()
            .filter_map(|p| {
                let t = (*p - *a).dot(d) / l2;
                ((t > 1e-9 && t < 1.0 - 1e-9) && (*a + d * t).dist(*p) < 1e-7).then_some(t)
            })
            .collect();
        ts.push(0.0);
        ts.push(1.0);
        ts.sort_by(|x, y| x.total_cmp(y));
        ts.dedup_by(|x, y| (*x - *y).abs() < 1e-12);
        for w in ts.windows(2) {
            if split.len() >= MAX_FLAT_POINTS {
                return Err(flat_budget_error());
            }
            split.push((*a + d * w[0], *a + d * w[1]));
        }
    }
    work.charge(split.len().saturating_mul(split.len()).saturating_mul(2))?;
    let same = |p: Vec2, q: Vec2| p.dist(q) < 1e-6;
    let keep: Vec<(Vec2, Vec2)> = split
        .iter()
        .enumerate()
        .filter(|(i, (a, b))| !split.iter().enumerate().any(|(j, (c, d))| j != *i && same(*a, *d) && same(*b, *c)))
        .map(|(_, s)| *s)
        .collect();
    let mut loops = Vec::new();
    let mut used = vec![false; keep.len()];
    for s0 in 0..keep.len() {
        if used.get(s0).copied().unwrap_or(true) {
            continue;
        }
        let mut lp = Vec::new();
        let mut cur = s0;
        for _ in 0..keep.len() + 1 {
            if let Some(u) = used.get_mut(cur) {
                *u = true;
            }
            let Some(&(a, b)) = keep.get(cur) else { break };
            lp.push(a);
            match (0..keep.len()).find(|j| !used.get(*j).copied().unwrap_or(true) && keep.get(*j).is_some_and(|(c, _)| same(*c, b))) {
                Some(n) => cur = n,
                None => break,
            }
        }
        let m = lp.len();
        let clean: Vec<Vec2> = (0..m)
            .filter_map(|i| {
                let (p, q, r) = (*lp.get((i + m - 1) % m)?, *lp.get(i)?, *lp.get((i + 1) % m)?);
                ((q - p).cross(r - q).abs() > 1e-9 * (q - p).len().max(1.0) * (r - q).len().max(1.0)).then_some(q)
            })
            .collect();
        if clean.len() >= 3 {
            loops.push(clean);
        }
    }
    Ok(loops)
}

fn signed_area(p: &[Vec2]) -> f64 {
    let m = p.len();
    (0..m).filter_map(|i| Some(p.get(i)?.cross(*p.get((i + 1) % m)?))).sum::<f64>() / 2.0
}

/// Do two polygons' edges cross?
fn polys_cross(a: &[Vec2], b: &[Vec2]) -> bool {
    let seg = |p: &[Vec2], i: usize| -> Option<(Vec2, Vec2)> { Some((*p.get(i)?, *p.get((i + 1) % p.len())?)) };
    (0..a.len()).filter_map(|i| seg(a, i)).any(|(p, q)| {
        (0..b.len()).filter_map(|j| seg(b, j)).any(|(r, s)| {
            let d1 = (q - p).cross(r - p);
            let d2 = (q - p).cross(s - p);
            let d3 = (s - r).cross(p - r);
            let d4 = (s - r).cross(q - r);
            d1 * d2 < -1e-12 && d3 * d4 < -1e-12
        })
    })
}

/// Bounded overlap predicate, including crossing-only cuts whose vertices lie outside a panel.
pub(crate) fn flat_regions_overlap(a: &Region2, b: &Region2, work: &mut FlatWork) -> Result<bool> {
    let (a, b) = (planar_region(a)?, planar_region(b)?);
    Ok(!material_boolean(&[a], &[b], kernel::BoolOp::Intersect, work)?.is_empty())
}

/// A sheet from a closed profile (base flange): the region in `plane`, thickness along its normal.
pub fn base_flange(body: &str, rule: &str, rv: &RuleValues, plane: Plane, region: Region2) -> SheetBody {
    let holes = region.holes.into_iter().map(|h| Region2 { outer: h.ccw(), holes: Vec::new() }).collect();
    SheetBody {
        body: body.into(),
        rule: rule.into(),
        t: rv.thickness,
        k: rv.k_factor,
        frame: plane,
        base: Region2 { outer: region.outer, holes: Vec::new() },
        flanges: Vec::new(),
        holes,
        pockets: Vec::new(),
        flat: false,
        gap: rv.gap,
        relief: rv.corner_relief_shape,
        relief_size: rv.corner_relief,
        base_curves: Vec::new(),
    }
}

/// A sheet from an open chain of lines (contour flange): `pts` in `plane`, the material on the
/// right of the travel (or the left when `flip`), `width` along the plane normal.
pub fn contour_flange(body: &str, rule: &str, rv: &RuleValues, plane: &Plane, pts: &[Vec2], width: f64, flip: bool) -> Result<SheetBody> {
    if pts.len() < 2 {
        return Err(DocError::Invalid("a contour flange needs an open chain of lines".into()));
    }
    let (t, r) = (rv.thickness, rv.bend_radius);
    let seg_dir = |i: usize| -> Result<(Vec2, f64)> {
        let (Some(a), Some(b)) = (pts.get(i), pts.get(i + 1)) else { return Err(DocError::Invalid("chain".into())) };
        let l = a.dist(*b);
        Ok(((*b - *a).normalized().ok_or_else(|| DocError::Invalid("zero-length line in the contour".into()))?, l))
    };
    let side = if flip { -1.0 } else { 1.0 };
    // Material side in the sketch: right of travel.
    let (d0, _) = seg_dir(0)?;
    let m0 = Vec2::new(d0.y, -d0.x) * side;
    let start = pts.first().copied().unwrap_or_default();
    let (x, z) = (plane.dir_to_world(d0), plane.dir_to_world(m0));
    let frame = Plane::new(plane.to_world(start), x, z.cross(x)).ok_or_else(|| DocError::Invalid("contour frame".into()))?;
    // Width along the sketch normal: +y or −y of the frame.
    let ysign = if frame.y.dot(plane.normal()) >= 0.0 { 1.0 } else { -1.0 };
    let n = pts.len() - 1;
    // Turn at each corner: angle and whether it bends toward the material (up).
    let mut corners = Vec::new();
    for i in 0..n.saturating_sub(1) {
        let (da, _) = seg_dir(i)?;
        let (db, _) = seg_dir(i + 1)?;
        let cross = da.cross(db) * side;
        let ang = da.dot(db).clamp(-1.0, 1.0).acos();
        if ang < 1e-9 {
            return Err(DocError::Invalid("straight corners in a contour flange".into()));
        }
        // Right-hand material: turning right (negative cross) bends toward it.
        corners.push((ang, cross < 0.0));
    }
    let setback = |ang: f64, up: bool| if up { (r + t) * (ang / 2.0).tan() } else { r * (ang / 2.0).tan() };
    let mut sheet = SheetBody {
        body: body.into(),
        rule: rule.into(),
        t,
        k: rv.k_factor,
        frame,
        base: Region2 { outer: Loop2 { segs: Vec::new() }, holes: Vec::new() },
        flanges: Vec::new(),
        holes: Vec::new(),
        pockets: Vec::new(),
        flat: false,
        gap: rv.gap,
        relief: rv.corner_relief_shape,
        relief_size: rv.corner_relief,
        base_curves: Vec::new(),
    };
    let (y0, y1) = if ysign > 0.0 { (0.0, width) } else { (-width, 0.0) };
    // First segment: the base panel.
    let (_, l0) = seg_dir(0)?;
    let end0 = l0 - corners.first().map(|(a, u)| setback(*a, *u)).unwrap_or(0.0);
    if end0 <= 0.0 {
        return Err(DocError::Invalid("a contour line is shorter than its bends".into()));
    }
    sheet.base = rect(Vec2::new(0.0, y0), Vec2::new(end0, y0), Vec2::new(end0, y1), Vec2::new(0.0, y1));
    let mut parent: Option<usize> = None;
    let mut at = end0;
    for (i, (ang, up)) in corners.iter().enumerate() {
        let (_, l) = seg_dir(i + 1)?;
        let before = setback(*ang, *up);
        let after = corners.get(i + 1).map(|(a, u)| setback(*a, *u)).unwrap_or(0.0);
        let leg = l - before - after;
        if leg < -1e-9 {
            return Err(DocError::Invalid("a contour line is shorter than its bends".into()));
        }
        // The material flips sides relative to the panel when the bend goes down.
        let f = SheetFlange {
            parent,
            p: Vec2::new(at, y0),
            d: Vec2::new(0.0, 1.0),
            len: y1 - y0,
            n: Vec2::new(1.0, 0.0),
            angle: if *up { *ang } else { -*ang },
            radius: r,
            leg: leg.max(0.0),
            ext: (0.0, 0.0),
            slope: (0.0, 0.0),
            shape: None,
            tag: String::new(),
        };
        let ba = sheet.allowance(&f);
        sheet.flanges.push(f);
        parent = Some(sheet.flanges.len() - 1);
        at += ba + leg.max(0.0);
    }
    Ok(sheet)
}

/// The name a sketch keeps for a flat face of a sheet: `sheet:<body>:<panel>:<top|bottom>`.
pub fn sheet_face_name(body: &str, face: &str) -> String {
    format!("sheet:{body}:{face}")
}

/// Point the sheet face a sketch plane is attached to (also beneath an offset or a rotation)
/// at body `new`, when it is on body `old`.
pub fn rename_plane_sheet_body(plane: &mut crate::PlaneRef, old: &str, new: &str) {
    match plane {
        crate::PlaneRef::Face { name: Some(name), anchor: None, .. } => {
            let renamed = split_sheet_face_name(name).filter(|(body, _)| *body == old).map(|(_, face)| sheet_face_name(new, face));
            if let Some(renamed) = renamed {
                *name = renamed;
            }
        }
        crate::PlaneRef::Offset { base, .. } | crate::PlaneRef::AtAngle { base, .. } => rename_plane_sheet_body(base, old, new),
        _ => {}
    }
}

/// The sheet face a sketch plane is attached to (also beneath an offset or a rotation): its
/// [`sheet_face_name`].
pub fn plane_sheet_face(plane: &crate::PlaneRef) -> Option<&str> {
    match plane {
        crate::PlaneRef::Face { name: Some(name), anchor: None, .. } if split_sheet_face_name(name).is_some() => Some(name),
        crate::PlaneRef::Offset { base, .. } | crate::PlaneRef::AtAngle { base, .. } => plane_sheet_face(base),
        _ => None,
    }
}

/// The body and the `<panel>:<side>` of a [`sheet_face_name`].
pub fn split_sheet_face_name(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix("sheet:")?;
    let (head, side) = rest.rsplit_once(':')?;
    let (body, panel) = head.rsplit_once(':')?;
    (!body.is_empty() && !panel.is_empty() && !side.is_empty()).then_some(())?;
    Some((body, rest.get(body.len() + 1..)?))
}

/// The plane of the sheet face with a [`sheet_face_name`], in the body's own coordinates.
pub fn sheet_face_plane(st: &crate::ModelState, name: &str) -> Option<(String, Plane)> {
    let (body, face) = split_sheet_face_name(name)?;
    let plane = st.sheets.iter().find(|s| s.body == body)?.panel_face(face)?;
    Some((body.to_string(), plane))
}

/// Persistent names of the sheet edges nearest each world point (empty when none).
pub fn sheet_edge_names(st: &crate::ModelState, pts: &[Vec3]) -> Vec<String> {
    pts.iter()
        .map(|p| {
            st.sheets
                .iter()
                .filter_map(|sh| sh.edge_at(*p).map(|e| (e.4, sh)))
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .and_then(|(_, sh)| sh.edge_name(*p))
                .unwrap_or_default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steel() -> RuleValues {
        RuleValues {
            thickness: 2.5,
            k_factor: 0.44,
            bend_radius: 2.5,
            relief_width: 2.5,
            relief_depth: 1.25,
            corner_relief: 10.0,
            corner_relief_shape: ReliefShape::Notch,
            hem_gap: 0.01,
            gap: 2.5,
        }
    }

    fn plate(w: f64, h: f64) -> SheetBody {
        let r = rect(Vec2::new(0.0, 0.0), Vec2::new(w, 0.0), Vec2::new(w, h), Vec2::new(0.0, h));
        base_flange(
            "Body1",
            "Steel (mm)",
            &steel(),
            Plane::new(Vec3::ZERO, Vec3::X, Vec3::Y)
                .unwrap_or_else(|| Plane::named("XY").unwrap_or(Plane { origin: Vec3::ZERO, x: Vec3::X, y: Vec3::Y })),
            r,
        )
    }

    fn vol(b: &Body) -> f64 {
        kernel::measure(b).map(|m| m.volume).unwrap_or(f64::NAN)
    }

    #[test]
    fn edge_flange_matches_fusion() {
        // Oracle 68: 100 x 60 plate, 20 mm flange on the y = 0 top edge.
        let mut s = plate(100.0, 60.0);
        s.add_flange(Vec3::new(50.0, 0.0, 2.5), 20.0, std::f64::consts::FRAC_PI_2, 2.5, "inside", false).unwrap();
        let b = s.solid().unwrap();
        assert!((vol(&b) - 18972.622).abs() < 2.0, "{}", vol(&b));
        let (w, h) = s.flat_size().unwrap();
        assert!((w - 100.0).abs() < 1e-9 && (h - 75.654867).abs() < 1e-5, "{w} {h}");
        let bends = s.bends();
        assert_eq!(bends.len(), 1);
        assert!((bends[0].allowance - std::f64::consts::FRAC_PI_2 * 3.6).abs() < 1e-9);
        // Unfolded: the same volume, flat.
        s.flat = true;
        let f = s.solid().unwrap();
        assert!((vol(&f) - 100.0 * 75.654867 * 2.5).abs() < 2.0);
        // Oracle 70: 45°, height 15.
        let mut s = plate(100.0, 60.0);
        s.add_flange(Vec3::new(50.0, 0.0, 2.5), 15.0, std::f64::consts::FRAC_PI_4, 2.5, "inside", false).unwrap();
        assert!((vol(&s.solid().unwrap()) - 18450.777).abs() < 2.0);
        assert!((s.flat_size().unwrap().1 - 73.685298).abs() < 1e-5);
    }

    #[test]
    fn flat_faces_of_a_sheet_are_found_and_named_by_panel() {
        use std::f64::consts::FRAC_PI_2;
        // Steel, 2.5 thick, R = T: a 90° flange on the y = 0 edge stands between the planes
        // y = 0 (its outer face) and y = 2.5 (its inner face) from z = 5 up.
        let mut s = plate(100.0, 60.0);
        s.add_flange(Vec3::new(50.0, 0.0, 2.5), 20.0, FRAC_PI_2, 2.5, "inside", false).unwrap();
        s.flanges[0].tag = "F7.0".into();
        let face = |s: &SheetBody, p: Vec3| s.panel_face_at(p, 1e-6);
        let (name, plane) = face(&s, Vec3::new(50.0, 30.0, 2.5)).unwrap();
        assert_eq!(name, "base:top");
        assert!(plane.normal().dist(Vec3::Z) < 1e-12 && (plane.origin.z - 2.5).abs() < 1e-12);
        // The top face's coordinates are the flat pattern's; the bottom face's mirror y.
        assert!(plane.to_local(Vec3::new(30.0, 20.0, 2.5)).dist(Vec2::new(30.0, 20.0)) < 1e-12);
        let (name, plane) = face(&s, Vec3::new(50.0, 30.0, 0.0)).unwrap();
        assert_eq!(name, "base:bottom");
        assert!(plane.normal().dist(Vec3::Z * -1.0) < 1e-12 && plane.origin.z.abs() < 1e-12);
        assert!(plane.to_local(Vec3::new(30.0, 20.0, 0.0)).dist(Vec2::new(30.0, -20.0)) < 1e-12);
        // The flange's inner face is the sheet's top side there; 7 mm up it is flat y = 5 − BA − 7.
        let ba = FRAC_PI_2 * (2.5 + 0.44 * 2.5);
        let (inner, plane) = face(&s, Vec3::new(30.0, 2.5, 12.0)).unwrap();
        assert_eq!(inner, "F7.0:top");
        assert!(plane.normal().dist(Vec3::Y) < 1e-12, "{:?}", plane.normal());
        assert!(plane.to_local(Vec3::new(30.0, 2.5, 12.0)).dist(Vec2::new(30.0, 5.0 - ba - 7.0)) < 1e-9);
        let (outer, plane) = face(&s, Vec3::new(30.0, 0.0, 12.0)).unwrap();
        assert_eq!(outer, "F7.0:bottom");
        assert!(plane.normal().dist(Vec3::Y * -1.0) < 1e-12, "{:?}", plane.normal());
        // A name gives the same plane again.
        for p in [Vec3::new(50.0, 30.0, 2.5), Vec3::new(50.0, 30.0, 0.0), Vec3::new(30.0, 2.5, 12.0), Vec3::new(30.0, 0.0, 12.0)] {
            let (name, plane) = face(&s, p).unwrap();
            assert_eq!(s.panel_face(&name), Some(plane), "{name}");
        }
        // Inside the sheet, on the bend, beside the sheet, above the flange: no flat face.
        for p in [Vec3::new(50.0, 30.0, 1.0), Vec3::new(50.0, 1.0, 1.0), Vec3::new(150.0, 30.0, 2.5), Vec3::new(30.0, 2.5, 40.0)] {
            assert!(face(&s, p).is_none(), "{p:?}");
        }
        // Names that are no face, or no flange, give no plane.
        for name in ["base", "base:side", "nobody:top", "#7:top", "F8.0:top", ""] {
            assert!(s.panel_face(name).is_none(), "{name}");
        }
        // A flange without a tag is named by its index.
        s.flanges[0].tag.clear();
        assert_eq!(face(&s, Vec3::new(30.0, 2.5, 12.0)).map(|f| f.0).as_deref(), Some("#0:top"));
        assert!(s.panel_face("#0:bottom").is_some());
        // Unfolded, the whole sheet is one panel.
        s.flat = true;
        assert_eq!(face(&s, Vec3::new(50.0, -10.0, 2.5)).map(|f| f.0).as_deref(), Some("flat:top"));
        assert!(s.panel_face("flat:bottom").is_some());
    }

    #[test]
    fn a_pick_beside_a_thin_sheet_takes_the_nearer_face() {
        // The tolerance of a pick on a large part is far above a thin sheet's thickness, and
        // never reaches a quarter of it here: the nearer side wins, the middle is no face.
        let rv = RuleValues { thickness: 0.5, ..steel() };
        let s = base_flange(
            "Thin",
            "Sheet",
            &rv,
            Plane::XY,
            rect(Vec2::new(0.0, 0.0), Vec2::new(100.0, 0.0), Vec2::new(100.0, 60.0), Vec2::new(0.0, 60.0)),
        );
        let at = |z: f64| s.panel_face_at(Vec3::new(50.0, 30.0, z), 5.0).map(|f| f.0);
        assert_eq!(at(0.4).as_deref(), Some("base:top"));
        assert_eq!(at(0.1).as_deref(), Some("base:bottom"));
        assert_eq!(at(0.25), None);
        assert_eq!(at(3.0), None);
    }

    #[test]
    fn sheet_face_names_split_from_the_right() {
        assert_eq!(split_sheet_face_name(&sheet_face_name("Body1", "F7.0:top")), Some(("Body1", "F7.0:top")));
        // A body name may hold colons; the panel and the side never do.
        assert_eq!(split_sheet_face_name("sheet:Part:2:base:bottom"), Some(("Part:2", "base:bottom")));
        for name in ["", "sheet", "sheet:Body1", "sheet:Body1:top", "sheet::base:top", "sheet:Body1::top", "sheet:Body1:base:", "Body1:base:top"] {
            assert_eq!(split_sheet_face_name(name), None, "{name}");
        }
    }

    #[test]
    fn contour_flange_matches_fusion() {
        // Oracle 72: XZ chain (0,0)→(40,0)→(40,−30)→(70,−30), 50 wide.
        let xz = Plane::new(Vec3::ZERO, Vec3::X, Vec3::new(0.0, 0.0, -1.0)).unwrap();
        let pts = [Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), Vec2::new(40.0, -30.0), Vec2::new(70.0, -30.0)];
        let s = contour_flange("Body1", "Steel (mm)", &steel(), &xz, &pts, 50.0, false).unwrap();
        let b = s.solid().unwrap();
        assert!((vol(&b) - 12097.622).abs() < 1.2, "{}", vol(&b));
        let m = kernel::measure(&b).unwrap();
        assert!((m.bbox.max.z - 32.5).abs() < 1e-6 && (m.bbox.max.y - 50.0).abs() < 1e-6, "{:?}", m.bbox);
        assert!((s.flat_size().unwrap().0 - 96.309734).abs() < 1e-5, "{:?}", s.flat_size());
    }

    #[test]
    fn hem_and_holes() {
        // Oracle 73: flange then a flat hem of 8 on its top edge.
        let mut s = plate(100.0, 60.0);
        s.add_flange(Vec3::new(50.0, 0.0, 2.5), 20.0, std::f64::consts::FRAC_PI_2, 2.5, "inside", false).unwrap();
        s.add_hem(Vec3::new(50.0, 0.0, 20.0), 8.0, 0.01, false).unwrap();
        let b = s.solid().unwrap();
        assert!((vol(&b) - 21334.723).abs() < 2.0, "{}", vol(&b));
        assert!((s.flat_size().unwrap().1 - 84.632035).abs() < 1e-4, "{:?}", s.flat_size());
        // A hole in the base and a slot across the bend.
        let mut s = plate(80.0, 50.0);
        s.add_flange(Vec3::new(40.0, 0.0, 2.5), 20.0, std::f64::consts::FRAC_PI_2, 2.5, "inside", false).unwrap();
        s.holes.push(Region2 { outer: Loop2::circle(Vec2::new(40.0, 30.0), 3.0), holes: vec![] });
        let v0 = vol(&s.solid().unwrap());
        s.holes.push(rect(Vec2::new(35.0, -12.0), Vec2::new(45.0, -12.0), Vec2::new(45.0, 10.0), Vec2::new(35.0, 10.0)));
        let v1 = vol(&s.solid().unwrap());
        let ba = std::f64::consts::FRAC_PI_2 * 3.6;
        let want = 10.0 * 2.5 * (22.0 - ba) + 10.0 * std::f64::consts::FRAC_PI_2 * (2.5 + 1.25) * 2.5;
        assert!((v0 - v1 - want).abs() < 1.0, "{v0} {v1} {want}");
        s.flat = true;
        assert!(s.solid().is_ok());
    }

    #[test]
    fn disconnected_material_keeps_each_hole_with_its_own_region() {
        let mut s = plate(100.0, 60.0);
        s.holes.push(rect(Vec2::new(45.0, -5.0), Vec2::new(55.0, -5.0), Vec2::new(55.0, 65.0), Vec2::new(45.0, 65.0)));
        s.holes.push(Region2 { outer: Loop2::circle(Vec2::new(20.0, 30.0), 5.0), holes: vec![] });
        let regions = s.flat_regions().unwrap();
        assert_eq!(regions.len(), 2);
        let left = regions.iter().find(|r| r.outer.contains(Vec2::new(10.0, 10.0))).unwrap();
        let right = regions.iter().find(|r| r.outer.contains(Vec2::new(90.0, 10.0))).unwrap();
        assert_eq!(left.holes.len(), 1);
        assert!(left.holes[0].segs.iter().all(|s| matches!(s, Seg2::Arc { radius, .. } if (*radius - 5.0).abs() < 1e-9)));
        assert!(right.holes.is_empty());
        let area: f64 = regions.iter().map(Region2::area).sum();
        assert!((area - (5400.0 - std::f64::consts::PI * 25.0)).abs() < 1e-6);
        assert!((vol(&s.flat_solid().unwrap()) - area * 2.5).abs() < 0.1);
    }

    #[test]
    fn ring_cut_retains_exact_central_material_island() {
        let mut s = plate(100.0, 60.0);
        s.holes.push(Region2 { outer: Loop2::circle(Vec2::new(50.0, 30.0), 10.0), holes: vec![Loop2::circle(Vec2::new(50.0, 30.0), 5.0)] });
        let regions = s.flat_regions().unwrap();
        assert_eq!(regions.len(), 2);
        let island = regions.iter().find(|r| r.outer.contains(Vec2::new(50.0, 30.0))).unwrap();
        assert!(island.holes.is_empty());
        assert!(island.outer.segs.iter().all(|s| matches!(s, Seg2::Arc { radius, .. } if (*radius - 5.0).abs() < 1e-9)));
        let area: f64 = regions.iter().map(Region2::area).sum();
        assert!((area - (6000.0 - std::f64::consts::PI * 75.0)).abs() < 1e-6);
        assert!((vol(&s.flat_solid().unwrap()) - area * 2.5).abs() < 0.1);
    }

    #[test]
    fn freeform_profile_uses_bounded_sampling_with_exact_round_holes() {
        let mut s = plate(100.0, 60.0);
        s.base.outer.segs = vec![
            Seg2::Line { a: Vec2::ZERO, b: Vec2::new(100.0, 0.0) },
            Seg2::Line { a: Vec2::new(100.0, 0.0), b: Vec2::new(100.0, 60.0) },
            Seg2::Cubic { p0: Vec2::new(100.0, 60.0), p1: Vec2::new(200.0 / 3.0, 80.0), p2: Vec2::new(100.0 / 3.0, 80.0), p3: Vec2::new(0.0, 60.0) },
            Seg2::Line { a: Vec2::new(0.0, 60.0), b: Vec2::ZERO },
        ];
        s.holes.push(Region2 { outer: Loop2::circle(Vec2::new(50.0, 30.0), 3.0), holes: vec![] });
        let regions = s.flat_regions().unwrap();
        assert!(regions[0].outer.segs.iter().all(|s| matches!(s, Seg2::Line { .. })));
        assert!(regions[0].holes[0].segs.iter().all(|s| matches!(s, Seg2::Arc { .. })));
        // The cubic has x=100(1-t), y=60+60t(1-t), hence an exact blank area of 7000.
        let expected = (7000.0 - std::f64::consts::PI * 9.0) * 2.5;
        assert!((vol(&s.solid().unwrap()) - expected).abs() < 0.5);
        assert!((vol(&s.flat_solid().unwrap()) - expected).abs() < 0.5);
    }

    #[test]
    fn flat_geometry_budgets_reject_aggregate_and_intermediate_work() {
        let mut s = plate(100.0, 60.0);
        // Each arc is individually legal, but their combined sampled points exceed the cap.
        s.base.outer.segs = vec![Seg2::Arc { center: Vec2::ZERO, radius: 1000.0, start: 0.0, sweep: std::f64::consts::TAU }; 32];
        assert!(s.flat_regions().unwrap_err().to_string().contains("budget"));
        let mut s = plate(100.0, 60.0);
        s.holes = vec![Region2 { outer: Loop2::circle(Vec2::new(20.0, 20.0), 1.0), holes: vec![] }; MAX_FLAT_PIECES + 1];
        assert!(s.flat_regions().unwrap_err().to_string().contains("budget"));
        let p = vec![vec![Vec2::ZERO; MAX_FLAT_POINTS]];
        assert!(merge_loops(&p, &mut FlatWork::new()).unwrap_err().to_string().contains("budget"));
        // Exhausting a clipping/merge allowance must report an error before work begins.
        assert!(merge_loops(&[vec![Vec2::ZERO, Vec2::X, Vec2::Y]], &mut FlatWork(0)).is_err());
    }

    #[test]
    fn round_cutouts_count_as_exact_arcs_in_the_flat_budget() {
        // 240 round 4 mm holes sample to over 16 384 chords of 0.001 mm, but are 480 exact arcs.
        let mut s = plate(250.0, 130.0);
        for i in 0..24 {
            for j in 0..10 {
                s.holes.push(Region2 { outer: Loop2::circle(Vec2::new(5.0 + 10.0 * i as f64, 15.0 + 10.0 * j as f64), 2.0), holes: vec![] });
            }
        }
        let regions = s.flat_regions().unwrap();
        assert_eq!(regions[0].holes.len(), 240);
        let area = 250.0 * 130.0 - 240.0 * std::f64::consts::PI * 2.0 * 2.0;
        assert!((regions[0].area() - area).abs() < 1e-6);
    }

    #[test]
    fn overlapping_round_cutouts_union_with_exact_arc_boundaries() {
        let mut s = plate(100.0, 60.0);
        s.holes.push(Region2 { outer: Loop2::circle(Vec2::new(20.0, 30.0), 5.0), holes: vec![] });
        s.holes.push(Region2 { outer: Loop2::circle(Vec2::new(22.0, 30.0), 5.0), holes: vec![] });
        let regions = s.flat_regions().unwrap();
        assert_eq!(regions[0].holes.len(), 1);
        assert!(regions[0].holes[0].segs.iter().all(|s| matches!(s, Seg2::Arc { radius, .. } if (*radius - 5.0).abs() < 1e-9)));
        let intersection = 50.0 * (0.2f64).acos() - (96.0f64).sqrt();
        let area = 6000.0 - (50.0 * std::f64::consts::PI - intersection);
        assert!((regions[0].area() - area).abs() < 1e-6);
        assert!((vol(&s.solid().unwrap()) - area * 2.5).abs() < 0.1);
    }

    #[test]
    fn nested_and_repeated_round_cutouts_keep_only_the_largest_exact_boundary() {
        for reverse in [false, true] {
            let mut s = plate(100.0, 60.0);
            let mut radii = vec![3.0, 5.0, 5.0];
            if reverse {
                radii.reverse();
            }
            for radius in radii {
                s.holes.push(Region2 { outer: Loop2::circle(Vec2::new(20.0, 30.0), radius), holes: vec![] });
            }
            let regions = s.flat_regions().unwrap();
            assert_eq!(regions[0].holes.len(), 1);
            assert!((regions[0].area() - (6000.0 - std::f64::consts::PI * 25.0)).abs() < 1e-6);
            let volume = vol(&s.solid().unwrap());
            // Body measure tessellates curved faces; the exact region area above is the
            // stronger analytic check, while this follows the existing sheet measure budget.
            assert!((volume - (6000.0 - std::f64::consts::PI * 25.0) * 2.5).abs() < 0.1, "{volume}");
            assert_eq!(s.round_hole_correction(), (-1, -4, -2));
        }
    }

    #[test]
    fn sewing_budget_rejects_edge_splitting_growth_before_allocating_faces() {
        let s = plate(100.0, 60.0);
        let cells: Vec<Cell> = (0..250)
            .map(|i| Cell {
                piece: Piece::Panel(None),
                outer: Loop2::polygon(&[Vec2::new(i as f64, 0.0), Vec2::new(250.0, 0.0), Vec2::new(i as f64, 1.0)]).segs,
                holes: vec![],
            })
            .collect();
        assert!(s.faces(&cells, 0.0, 2.5, &mut FlatWork::new()).unwrap_err().to_string().contains("budget"));
        assert!(s.faces(&cells[..1], 0.0, 2.5, &mut FlatWork(0)).unwrap_err().to_string().contains("budget"));
    }

    #[test]
    fn pocket_geometry_and_depth_budgets_are_checked_before_replay() {
        let pocket = SheetPocket {
            region: rect(Vec2::new(20.0, 20.0), Vec2::new(30.0, 20.0), Vec2::new(30.0, 30.0), Vec2::new(20.0, 30.0)),
            z0: 0.0,
            z1: 1.0,
        };
        let mut s = plate(100.0, 60.0);
        s.pockets = vec![pocket.clone(); 1001];
        assert!(s.flat_solid().unwrap_err().to_string().contains("budget"));
        s.pockets = vec![pocket.clone()];
        s.pockets[0].region.outer.segs = vec![Seg2::Line { a: Vec2::ZERO, b: Vec2::X }; MAX_FLAT_POINTS + 1];
        assert!(s.flat_solid().unwrap_err().to_string().contains("budget"));
        for (z0, z1) in [(f64::NAN, 1.0), (-1.0, 1.0), (0.0, 3.0), (1.0, 1.0)] {
            s.pockets = vec![SheetPocket { z0, z1, ..pocket.clone() }];
            assert!(s.flat_solid().unwrap_err().to_string().contains("depth"));
        }
    }
    #[test]
    fn pocket_recording_deduplicates_at_limit_and_rejects_growth_before_mutation() {
        let mut s = plate(100.0, 60.0);
        let pocket =
            SheetPocket { region: rect(Vec2::new(1.0, 1.0), Vec2::new(2.0, 1.0), Vec2::new(2.0, 2.0), Vec2::new(1.0, 2.0)), z0: 0.0, z1: 0.5 };
        s.pockets = vec![pocket.clone(); 1000];
        let mut work = FlatWork::new();
        s.record_pocket(pocket.region.clone(), pocket.z0, pocket.z1, &mut work).unwrap();
        assert_eq!(s.pockets.len(), 1000);
        assert!(s.record_pocket(pocket.region.clone(), 0.0, 0.6, &mut work).is_err());
        assert_eq!(s.pockets.len(), 1000);
        s.pockets.push(pocket.clone());
        assert!(s.record_pocket(pocket.region, 0.0, 0.5, &mut work).is_err());
    }

    /// A sketch solved again after a parameter change leaves its corners off by ~1e-13 mm, so
    /// its edges are not quite square to the axes. Such a tray (four 72° flanges, shrunk to
    /// 400 x 300 from a larger size) failed to unfold in the planar arrangement while the same
    /// tray drawn at that size unfolded.
    #[test]
    fn nearly_square_base_unfolds_like_the_exact_one() {
        let rv = RuleValues { thickness: 2.0, k_factor: 0.5, bend_radius: 1.0, ..steel() };
        let tray = |[a, b, c, d]: [Vec2; 4]| {
            let mut s = base_flange("Body1", "AL2", &rv, Plane::XY, rect(a, b, c, d));
            let picks = [Vec3::new(200.0, 0.0, 2.0), Vec3::new(400.0, 150.0, 2.0), Vec3::new(200.0, 300.0, 2.0), Vec3::new(0.0, 150.0, 2.0)];
            s.add_flanges(&picks, 38.6, 72f64.to_radians(), 1.0, "inside", false).unwrap();
            s
        };
        let flat_area = |s: &SheetBody| s.flat_regions().unwrap().iter().map(|r| r.area().abs()).sum::<f64>();
        let square = [Vec2::new(0.0, 0.0), Vec2::new(400.0, 0.0), Vec2::new(400.0, 300.0), Vec2::new(0.0, 300.0)];
        let exact = flat_area(&tray(square));
        // The corners the solver left after the parameter change, and the rectangle turned by a hair.
        let solved = [
            Vec2::new(0.0, 0.0),
            Vec2::new(400.0000000000001, 4.369543929613394e-13),
            Vec2::new(400.00000000000017, 300.0000000000008),
            Vec2::new(0.0, 300.0000000000009),
        ];
        let (c, s) = (1e-14f64.cos(), 1e-14f64.sin());
        let turned = square.map(|p| Vec2::new(p.x * c - p.y * s, p.x * s + p.y * c));
        for corners in [solved, turned] {
            let mut sheet = tray(corners);
            assert!((flat_area(&sheet) - exact).abs() < 1e-6, "{} {exact}", flat_area(&sheet));
            sheet.flat = true;
            assert!((vol(&sheet.solid().unwrap()) - exact * 2.0).abs() < 1e-3);
        }
    }

    /// A 100 × 80 plate with a 20 mm flange on its y = 0 edge: 2 mm sheet, inner radius 1,
    /// K 0.5. The neutral radius is the mid radius, so the folded sheet holds exactly its flat
    /// area times the thickness.
    fn flanged(angle: f64) -> SheetBody {
        let rule = RuleValues {
            thickness: 2.0,
            k_factor: 0.5,
            bend_radius: 1.0,
            relief_width: 2.0,
            relief_depth: 1.0,
            corner_relief: 8.0,
            corner_relief_shape: ReliefShape::Notch,
            hem_gap: 0.01,
            gap: 2.0,
        };
        let r = rect(Vec2::new(0.0, 0.0), Vec2::new(100.0, 0.0), Vec2::new(100.0, 80.0), Vec2::new(0.0, 80.0));
        let mut s = base_flange("Body1", "AL2", &rule, Plane::XY, r);
        s.add_flanges(&[Vec3::new(50.0, 0.0, 2.0)], 20.0, angle, 1.0, "inside", false).unwrap();
        s
    }

    #[test]
    fn slanted_and_round_cut_outs_across_a_bend_fold() {
        let area = |s: &SheetBody| s.flat_regions().unwrap().iter().map(Region2::area).sum::<f64>();
        for angle in [90f64, 72.0, 108.0] {
            let plain = flanged(angle.to_radians());
            let (a0, v0) = (area(&plain), vol(&plain.solid().unwrap()));
            // A 10 × 20 slot turned 30° about (50, 0) and a Ø8 circle at (30, 0.5): both reach
            // from the base over the whole bend zone into the flange.
            let (c, a) = (Vec2::new(50.0, 0.0), 30f64.to_radians());
            let (ux, uy) = (Vec2::from_angle(a) * 5.0, Vec2::from_angle(a + std::f64::consts::FRAC_PI_2) * 10.0);
            let slot = Region2 { outer: Loop2::polygon(&[c - ux - uy, c + ux - uy, c + ux + uy, c - ux + uy]).ccw(), holes: vec![] };
            let circle = Region2 { outer: Loop2::circle(Vec2::new(30.0, 0.5), 4.0), holes: vec![] };
            for (cut, removed) in [(slot, 200.0), (circle, std::f64::consts::PI * 16.0)] {
                let mut s = plain.clone();
                s.holes.push(cut);
                assert!((a0 - area(&s) - removed).abs() < 1e-6, "{angle}°: flat area {}", a0 - area(&s));
                let body = s.solid().unwrap();
                assert!(body.validity().is_empty(), "{angle}°: {:?}", body.validity());
                // The folded sheet loses the cut-out's flat area times the thickness.
                assert!((v0 - vol(&body) - removed * 2.0).abs() < 0.05, "{angle}°: {} vs {}", v0 - vol(&body), removed * 2.0);
            }
        }
    }

    #[test]
    fn a_slanted_flat_edge_folds_to_its_exact_helix() {
        // A 72° flange, inside: the bend's axis runs along x through y = setback, z = T + R,
        // and a point at height z in the sheet stays T + R − z away from it.
        let angle = 72f64.to_radians();
        let s = flanged(angle);
        let f = s.flanges[0].clone();
        let setback = 3.0 * (angle / 2.0).tan();
        // The unclamped zone map agrees with the fold inside the zone.
        for (sv, u, z) in [(0.3, 10.0, 0.0), (1.2, 40.0, 1.0), (2.4, 90.0, 2.0)] {
            let q = f.p + f.n * sv + f.d * u;
            assert!(s.zone_point(&f, q, z).0.dist(s.map3(Piece::Zone(0), q, z)) < 1e-12);
        }
        // A flat line across the whole zone, 15 mm along the bend: the bend angle and the
        // distance along the axis both grow evenly along it.
        let e = Seg2::Line { a: f.p + f.d * 20.0, b: f.p + f.n * s.allowance(&f) + f.d * 35.0 };
        let breaks = s.zone_breaks(&f, &e);
        for z in [0.0, 2.0] {
            let helix = |t: f64| {
                let (phi, rho) = (angle * t, 3.0 - z);
                Vec3::new(e.point_at(t).x, setback - rho * phi.sin(), 3.0 - rho * phi.cos())
            };
            let EdgeSpec::Chain { ctrl } = s.zone_chain(&f, &e, z) else { panic!("a slanted edge folds to a chain") };
            assert_eq!(ctrl.len(), 3 * (breaks.len() - 1) + 1);
            for (k, w) in breaks.windows(2).enumerate() {
                for x in [0.0, 0.1, 0.37, 0.5, 0.81, 1.0] {
                    let p = cubic_at(ctrl[3 * k], ctrl[3 * k + 1], ctrl[3 * k + 2], ctrl[3 * k + 3], x);
                    let exact = helix(w[0] + (w[1] - w[0]) * x);
                    assert!(p.dist(exact) < BEND_CHAIN_TOL, "height {z}, piece {k}: {}", p.dist(exact));
                }
            }
        }
        // Lines along the bend and square to it keep their exact line and arc.
        let along = Seg2::Line { a: f.p + f.n * 1.0 + f.d * 20.0, b: f.p + f.n * 1.0 + f.d * 35.0 };
        let across = Seg2::Line { a: f.p + f.d * 20.0, b: f.p + f.n * s.allowance(&f) + f.d * 20.0 };
        assert!(matches!(s.edge3(Piece::Zone(0), &along, 2.0), Ok(EdgeSpec::Line { .. })));
        assert!(matches!(s.edge3(Piece::Zone(0), &across, 2.0), Ok(EdgeSpec::Arc { .. })));
    }

    /// 2 mm aluminium, R 1, K 0.5 (folding keeps the volume), flanges 0.5 mm apart at corners.
    fn alu() -> RuleValues {
        RuleValues {
            thickness: 2.0,
            k_factor: 0.5,
            bend_radius: 1.0,
            relief_width: 2.0,
            relief_depth: 1.0,
            corner_relief: 8.0,
            corner_relief_shape: ReliefShape::Notch,
            hem_gap: 0.01,
            gap: 0.5,
        }
    }

    /// A 120 x 90 tray: flanges `height` high at `deg` on all four edges (bottom, right, top, left).
    fn tray(rv: &RuleValues, deg: f64, height: f64) -> (SheetBody, Vec<String>) {
        let r = rect(Vec2::new(0.0, 0.0), Vec2::new(120.0, 0.0), Vec2::new(120.0, 90.0), Vec2::new(0.0, 90.0));
        let mut s = base_flange("Body1", "AL2", rv, Plane::XY, r);
        let t = rv.thickness;
        let picks = [Vec3::new(60.0, 0.0, t), Vec3::new(120.0, 45.0, t), Vec3::new(60.0, 90.0, t), Vec3::new(0.0, 45.0, t)];
        let (_, notes) = s.add_flanges_noted(&picks, height, deg.to_radians(), rv.bend_radius, "inside", false).unwrap();
        (s, notes)
    }

    fn flat_area(s: &SheetBody) -> f64 {
        s.flat_regions().unwrap().iter().map(Region2::area).sum()
    }

    /// Distance between two parallel 3D lines through a0→a1 and b0→b1 (None if not parallel).
    fn parallel_gap(a0: Vec3, a1: Vec3, b0: Vec3, b1: Vec3) -> Option<f64> {
        let (u, v) = ((a1 - a0).normalized()?, (b1 - b0).normalized()?);
        (u.cross(v).len() < 1e-9).then(|| (b0 - a0).cross(u).len())
    }

    #[test]
    fn closed_corners_end_on_the_bisector_with_the_gap() {
        let (t, r, g, h) = (2.0, 1.0, 0.5, 38.6);
        for deg in [90.0f64, 72.0] {
            let (s, notes) = tray(&alu(), deg, h);
            assert!(notes.is_empty(), "{notes:?}");
            let th = deg.to_radians();
            let s0 = (r + t) * (th / 2.0).tan();
            let ba = th * (r + 0.5 * t);
            let leg = h - s0;
            // By hand, for the flange on y = 0 and its mirror image on x = 0: a point of the flange w up
            // its outer face from the virtual sharp (the original edge), h along the outward normal
            // (−T ≤ h ≤ 0) is (x, −w·cosθ − h·sinθ, w·sinθ − h·cosθ). Keeping x − y ≥ g/√2 on the
            // inner face (h = −T) gives the end x(w) = g/√2 + T·sinθ − w·cosθ; the panel spans w = s0 … h.
            let x_end = |w: f64| g / std::f64::consts::SQRT_2 + t * th.sin() - w * th.cos();
            let (y0, y1) = (s0 - ba, s0 - ba - leg);
            let want = [(x_end(s0), y0), (120.0 - x_end(s0), y0), (120.0 - x_end(h), y1), (x_end(h), y1)];
            let got = corners_of(&s.panel(&s.flanges[0]));
            assert_eq!(got.len(), 4);
            for (x, y) in want {
                assert!(got.iter().any(|p| p.dist(Vec2::new(x, y)) < 1e-9), "{deg}°: ({x}, {y}) not in {got:?}");
            }
            if deg == 90.0 {
                // The rule 90° corners always had: the panel runs on s0 − (T + g/√2) past its bend.
                assert!((s.flanges[0].ext.0 - (s0 - t - g / std::f64::consts::SQRT_2)).abs() < 1e-12);
                assert_eq!(s.flanges[0].slope, (0.0, 0.0));
            } else {
                assert!((s.flanges[0].slope.0 - th.cos()).abs() < 1e-12, "{:?}", s.flanges[0].slope);
            }
            // Folded, the inner edges of the two ends (bottom flange 0, left flange 3) run parallel,
            // `gap` apart.
            let (fa, fb) = (s.fold(0), s.fold(3));
            let pa = |x: f64, y: f64| apply_point(&fa, Vec3::new(x, y, t));
            let pb = |x: f64, y: f64| apply_point(&fb, Vec3::new(x, y, t));
            let gap = parallel_gap(pa(x_end(s0), y0), pa(x_end(h), y1), pb(y0, x_end(s0)), pb(y1, x_end(h)));
            assert!(gap.is_some_and(|d| (d - g).abs() < 1e-9), "{deg}°: {gap:?}");
            // The flat pattern is one piece and the folded part holds the same material (K = 0.5).
            let regions = s.flat_regions().unwrap();
            assert_eq!(regions.len(), 1);
            let b = s.solid().unwrap();
            assert!(b.validity().is_empty() && b.shell_count() == 1, "{deg}°: {:?}", b.validity());
            let v = vol(&b);
            assert!((v - flat_area(&s) * t).abs() < 1e-3 * v, "{deg}°: {v} vs {}", flat_area(&s) * t);
        }
    }

    #[test]
    fn a_flange_of_a_later_feature_leaves_the_corner_as_it_was() {
        // The convention of reference case 75-sm-bracket-holes: the earlier flange keeps its length
        // and the later one ends at its bend. Only flanges made together close toward each other.
        let rv = alu();
        let base = rect(Vec2::new(0.0, 0.0), Vec2::new(120.0, 0.0), Vec2::new(120.0, 90.0), Vec2::new(0.0, 90.0));
        let mut s = base_flange("Body1", "AL2", &rv, Plane::XY, base);
        s.add_flanges(&[Vec3::new(60.0, 0.0, 2.0)], 38.6, 72f64.to_radians(), 1.0, "inside", false).unwrap();
        let (made, notes) = s.add_flanges_noted(&[Vec3::new(120.0, 45.0, 2.0)], 25.0, 72f64.to_radians(), 1.0, "inside", false).unwrap();
        assert!(notes.is_empty() && s.corners(&made).is_empty(), "{notes:?}");
        assert!(s.flanges.iter().all(|f| f.ext == (0.0, 0.0) && f.slope == (0.0, 0.0)));
        assert!((s.flanges[0].len - 120.0).abs() < 1e-9);
        assert!(s.solid().is_ok() && s.flat_regions().unwrap().len() == 1);
    }

    #[test]
    fn corners_that_are_not_square_close_with_the_gap() {
        // A trapezoid: two acute and two obtuse corners.
        let pts = [Vec2::new(0.0, 0.0), Vec2::new(120.0, 0.0), Vec2::new(100.0, 90.0), Vec2::new(20.0, 90.0)];
        let base = Region2 { outer: Loop2::polygon(&pts).ccw(), holes: vec![] };
        let mut s = base_flange("Body1", "AL2", &alu(), Plane::XY, base);
        let picks = [Vec3::new(60.0, 0.0, 2.0), Vec3::new(110.0, 45.0, 2.0), Vec3::new(60.0, 90.0, 2.0), Vec3::new(10.0, 45.0, 2.0)];
        let (made, notes) = s.add_flanges_noted(&picks, 38.6, 72f64.to_radians(), 1.0, "inside", false).unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        let corners = s.corners(&made);
        assert_eq!(corners.len(), 4);
        // Each end's inner edge, folded.
        let edge = |i: usize, at_end: bool| {
            let f = &s.flanges[i];
            let ba = s.allowance(f);
            let (q0, q1) = if at_end {
                let b = f.p + f.n * ba + f.d * (f.len + f.ext.1);
                (b, b + f.n * f.leg + f.d * (f.slope.1 * f.leg))
            } else {
                let a = f.p + f.n * ba - f.d * f.ext.0;
                (a, a + f.n * f.leg - f.d * (f.slope.0 * f.leg))
            };
            let w = s.fold(i);
            (apply_point(&w, Vec3::new(q0.x, q0.y, 2.0)), apply_point(&w, Vec3::new(q1.x, q1.y, 2.0)))
        };
        for k in &corners {
            let ((a0, a1), (b0, b1)) = (edge(k.a, k.a_end), edge(k.b, k.b_end));
            let gap = parallel_gap(a0, a1, b0, b1);
            assert!(gap.is_some_and(|d| (d - 0.5).abs() < 1e-9), "{k:?}: {gap:?}");
        }
        let b = s.solid().unwrap();
        assert!(b.validity().is_empty() && b.shell_count() == 1, "{:?}", b.validity());
        assert!((vol(&b) - flat_area(&s) * 2.0).abs() < 1e-3 * vol(&b));
    }

    #[test]
    fn corners_that_cannot_close_stay_open_with_a_note() {
        // A gap of zero would make the ends touch.
        let mut rv = alu();
        rv.gap = 0.0;
        let (s, notes) = tray(&rv, 72.0, 38.6);
        assert_eq!(notes.len(), 4, "{notes:?}");
        assert!(notes.iter().all(|n| n.contains("left open") && n.contains("gap")), "{notes:?}");
        assert!(s.flanges.iter().all(|f| f.ext == (0.0, 0.0) && f.slope == (0.0, 0.0)));
        assert!(s.solid().is_ok());
        // Flanges bending opposite ways (the second edge picked on the bottom face).
        let base = rect(Vec2::new(0.0, 0.0), Vec2::new(120.0, 0.0), Vec2::new(120.0, 90.0), Vec2::new(0.0, 90.0));
        let mut s = base_flange("Body1", "AL2", &alu(), Plane::XY, base);
        let picks = [Vec3::new(60.0, 0.0, 2.0), Vec3::new(120.0, 45.0, 0.0)];
        let (_, notes) = s.add_flanges_noted(&picks, 20.0, 72f64.to_radians(), 1.0, "inside", false).unwrap();
        assert!(notes.len() == 1 && notes[0].contains("opposite"), "{notes:?}");
        assert!(s.solid().is_ok());
    }

    /// Folded, the inner edge of the end of flange `i`'s panel (`at_end`: the end past the far end
    /// of its bend).
    fn inner_end_edge(s: &SheetBody, i: usize, at_end: bool) -> (Vec3, Vec3) {
        let f = &s.flanges[i];
        let ba = s.allowance(f);
        let (q0, q1) = if at_end {
            let b = f.p + f.n * ba + f.d * (f.len + f.ext.1);
            (b, b + f.n * f.leg + f.d * (f.slope.1 * f.leg))
        } else {
            let a = f.p + f.n * ba - f.d * f.ext.0;
            (a, a + f.n * f.leg - f.d * (f.slope.0 * f.leg))
        };
        let (w, z) = (s.fold(i), if f.angle >= 0.0 { s.t } else { 0.0 });
        (apply_point(&w, Vec3::new(q0.x, q0.y, z)), apply_point(&w, Vec3::new(q1.x, q1.y, z)))
    }

    /// At each of `corners` the two panel ends' inner edges run parallel, `gap` apart.
    fn ends_are_gap_apart(s: &SheetBody, corners: &[Corner], gap: f64, what: &str) {
        for k in corners {
            let ((a0, a1), (b0, b1)) = (inner_end_edge(s, k.a, k.a_end), inner_end_edge(s, k.b, k.b_end));
            let got = parallel_gap(a0, a1, b0, b1);
            assert!(got.is_some_and(|d| (d - gap).abs() < 1e-9), "{what}: {k:?}: {got:?}");
        }
    }

    /// The flat pattern is one piece without hairline edges, and the folded part is one valid shell
    /// holding the same material (K = 0.5 keeps the volume).
    fn holds_the_flat_material(s: &SheetBody, what: &str) {
        let regions = s.flat_regions().unwrap_or_else(|e| panic!("{what}: no flat pattern: {e}"));
        assert_eq!(regions.len(), 1, "{what}");
        let shortest = regions[0].outer.segs.iter().map(Seg2::length).fold(f64::MAX, f64::min);
        assert!(regions[0].holes.is_empty() && shortest > 1e-3, "{what}: shortest outline edge {shortest:e}");
        assert_eq!(s.outline().unwrap_or_else(|e| panic!("{what}: no outline: {e}")).len(), 1, "{what}");
        let b = s.solid().unwrap_or_else(|e| panic!("{what}: no solid: {e}"));
        assert!(b.validity().is_empty() && b.shell_count() == 1, "{what}: {:?}", b.validity());
        let v = vol(&b);
        assert!((v - flat_area(s) * s.t).abs() < 1e-3 * v, "{what}: {v} vs {}", flat_area(s) * s.t);
    }

    /// The flat area a corner relief `size` across takes where the base fills the angle `alpha`
    /// and both bends are `ba` wide: of the base a sector of radius size/2 (round) or the kite
    /// with that side, square to both edges (rectangular), and size/2 of each bend's length.
    fn relief_area(shape: ReliefShape, size: f64, alpha: f64, ba: f64) -> f64 {
        let r = size / 2.0;
        match shape {
            ReliefShape::Notch => 0.0,
            ReliefShape::Round => r * r * alpha / 2.0 + 2.0 * r * ba,
            ReliefShape::Rectangular => r * r * (alpha / 2.0).tan() + 2.0 * r * ba,
        }
    }

    #[test]
    fn the_rule_names_its_corner_relief_shape() {
        // A rule saved before the shape existed reads as the notch it always had.
        let old: SheetRule = serde_json::from_str(r#"{"name":"Old","thickness":"2 mm","k_factor":"0.5","bend_radius":"1 mm"}"#).unwrap();
        assert_eq!(old.corner_relief_shape, "notch");
        assert_eq!(SheetRule::steel().corner_relief_shape, "notch");
        let (doc, vals) = (Document::default(), std::collections::BTreeMap::new());
        assert_eq!(doc.rule_values(&vals, &old).unwrap().corner_relief_shape, ReliefShape::Notch);
        for (text, shape) in [("round", ReliefShape::Round), (" Rectangular ", ReliefShape::Rectangular), ("NOTCH", ReliefShape::Notch)] {
            let rule = SheetRule { corner_relief_shape: text.into(), ..old.clone() };
            assert_eq!(doc.rule_values(&vals, &rule).unwrap().corner_relief_shape, shape);
            assert_eq!(ReliefShape::parse(shape.name()), Some(shape));
        }
        // No other name is a shape.
        for text in ["square", "circle", ""] {
            let err = doc.rule_values(&vals, &SheetRule { corner_relief_shape: text.into(), ..old.clone() }).unwrap_err().to_string();
            assert!(err.contains(&format!("unknown corner relief shape `{text}` (notch, round or rectangular)")), "{err}");
        }
    }

    #[test]
    fn corner_reliefs_cut_the_base_corner_and_stop_the_bends() {
        use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};
        // The tray: the bends on x = 0 and y = 0 meet at c = (s0, s0), s0 = (R + T)·tan(θ/2) in from
        // the sheet's corner. A relief 8 mm across reaches 4 mm from c along both edges of the base.
        for deg in [72.0f64, 90.0] {
            let th = deg.to_radians();
            let (s0, ba) = (3.0 * (th / 2.0).tan(), th * 2.0);
            let len = 120.0 - 2.0 * s0;
            let c = Vec2::new(s0, s0);
            let (notch, _) = tray(&alu(), deg, 38.6);
            assert_eq!(notch.base.outer.segs.len(), 4, "{deg}°: the notch cuts nothing");
            for shape in [ReliefShape::Round, ReliefShape::Rectangular] {
                let what = format!("{shape:?} at {deg}°");
                let (s, notes) = tray(&RuleValues { corner_relief_shape: shape, ..alu() }, deg, 38.6);
                assert!(notes.is_empty(), "{what}: {notes:?}");
                let segs = &s.base.outer.segs;
                if shape == ReliefShape::Round {
                    // A quarter round of radius 4 about c, from the edge x = s0 round to the edge y = s0.
                    assert_eq!((segs.len(), segs.iter().filter(|g| matches!(g, Seg2::Arc { .. })).count()), (8, 4), "{what}");
                    let found = segs.iter().any(|g| {
                        matches!(g, Seg2::Arc { center, radius, .. } if center.dist(c) < 1e-8 && (radius - 4.0).abs() < 1e-12)
                            && g.start().dist(c + Vec2::new(0.0, 4.0)) < 1e-9
                            && g.point_at(0.5).dist(c + Vec2::from_angle(FRAC_PI_4) * 4.0) < 1e-9
                            && g.end().dist(c + Vec2::new(4.0, 0.0)) < 1e-9
                    });
                    assert!(found, "{what}: {segs:?}");
                } else {
                    // A 4 mm square: in from the edge x = s0 to (s0 + 4, s0 + 4), out to the edge y = s0.
                    let line =
                        |a: Vec2, b: Vec2| segs.iter().any(|g| matches!(g, Seg2::Line { .. }) && g.start().dist(a) < 1e-9 && g.end().dist(b) < 1e-9);
                    assert_eq!(segs.len(), 12, "{what}");
                    assert!(
                        line(c + Vec2::new(0.0, 4.0), c + Vec2::new(4.0, 4.0)) && line(c + Vec2::new(4.0, 4.0), c + Vec2::new(4.0, 0.0)),
                        "{what}: {segs:?}"
                    );
                }
                // The bend on y = 0 stops 4 mm from the corners at both its ends, and the bend table follows.
                let (u0, u1) = s.zone_span(&s.flanges[0]);
                assert!((u0 - 4.0).abs() < 1e-9 && (u1 - (len - 4.0)).abs() < 1e-9, "{what}: {u0} {u1}");
                let bend = &s.bends()[0];
                let (start, end) = (Vec2::new(s0 + 4.0, s0 - ba / 2.0), Vec2::new(120.0 - s0 - 4.0, s0 - ba / 2.0));
                assert!((bend.length - (len - 8.0)).abs() < 1e-9 && bend.start.dist(start) < 1e-9 && bend.end.dist(end) < 1e-9, "{what}: {bend:?}");
                // Only the base changes: the flanges are the ones the closing left.
                assert_eq!(s.flanges, notch.flanges, "{what}");
                // The flat pattern loses, at each of its four corners, the relief in the base and 4 mm of two bends.
                let (removed, want) = (flat_area(&notch) - flat_area(&s), 4.0 * relief_area(shape, 8.0, FRAC_PI_2, ba));
                assert!((removed - want).abs() < 1e-6, "{what}: {removed} vs {want}");
                holds_the_flat_material(&s, &what);
                let unfolded = vol(&s.flat_solid().unwrap());
                assert!((unfolded - flat_area(&s) * 2.0).abs() < 1e-3 * unfolded, "{what}: {unfolded}");
            }
        }
    }

    /// The trapezoid of the closing tests with four flanges at `deg`, under rule `rv`.
    fn trapezoid(rv: &RuleValues, deg: f64) -> (SheetBody, Vec<String>) {
        let pts = [Vec2::new(0.0, 0.0), Vec2::new(120.0, 0.0), Vec2::new(100.0, 90.0), Vec2::new(20.0, 90.0)];
        let mut s = base_flange("Body1", "AL2", rv, Plane::XY, Region2 { outer: Loop2::polygon(&pts).ccw(), holes: vec![] });
        let picks = [Vec3::new(60.0, 0.0, 2.0), Vec3::new(110.0, 45.0, 2.0), Vec3::new(60.0, 90.0, 2.0), Vec3::new(10.0, 45.0, 2.0)];
        let (_, notes) = s.add_flanges_noted(&picks, 38.6, deg.to_radians(), rv.bend_radius, "inside", false).unwrap();
        (s, notes)
    }

    #[test]
    fn corner_reliefs_follow_corners_that_are_not_square() {
        use std::f64::consts::PI;
        // The base fills α = atan(90/20) = 77.47° at the two lower corners of the trapezoid and 180° − α
        // at the upper ones. With every edge trimmed by s0, the bends on y = 0 and on the left edge meet
        // at c = (s0/tan(α/2), s0), and the left edge leaves c in the direction (cos α, sin α).
        let th = 72f64.to_radians();
        let (s0, ba) = (3.0 * (th / 2.0).tan(), th * 2.0);
        let alpha = 90f64.atan2(20.0);
        let c = Vec2::new(s0 / (alpha / 2.0).tan(), s0);
        let (along, up) = (Vec2::new(1.0, 0.0), Vec2::from_angle(alpha));
        let (notch, _) = trapezoid(&alu(), 72.0);
        for shape in [ReliefShape::Round, ReliefShape::Rectangular] {
            let what = format!("{shape:?}");
            let (s, notes) = trapezoid(&RuleValues { corner_relief_shape: shape, ..alu() }, 72.0);
            assert!(notes.is_empty(), "{what}: {notes:?}");
            let segs = &s.base.outer.segs;
            if shape == ReliefShape::Round {
                // An arc of radius 4 about c through the angle α, tangent to both bends' straight ends.
                let found = segs.iter().any(|g| {
                    matches!(g, Seg2::Arc { center, radius, sweep, .. } if center.dist(c) < 1e-8 && (radius - 4.0).abs() < 1e-12 && (sweep + alpha).abs() < 1e-9)
                        && g.start().dist(c + up * 4.0) < 1e-8
                        && g.end().dist(c + along * 4.0) < 1e-8
                        && g.start_tangent().dot(up).abs() < 1e-9
                        && g.end_tangent().dot(along).abs() < 1e-9
                });
                assert!(found, "{what}: {segs:?}");
            } else {
                // A kite: both cuts square to their edge, meeting 4/cos(α/2) from c on the bisector.
                let x = c + Vec2::from_angle(alpha / 2.0) * (4.0 / (alpha / 2.0).cos());
                let line =
                    |a: Vec2, b: Vec2| segs.iter().any(|g| matches!(g, Seg2::Line { .. }) && g.start().dist(a) < 1e-8 && g.end().dist(b) < 1e-8);
                assert!(line(c + up * 4.0, x) && line(x, c + along * 4.0), "{what}: {segs:?}");
                assert!((x - (c + up * 4.0)).dot(up).abs() < 1e-9 && (x - (c + along * 4.0)).dot(along).abs() < 1e-9);
            }
            // Two corners of α and two of 180° − α, eight bend ends.
            let want = 2.0 * relief_area(shape, 8.0, alpha, ba) + 2.0 * relief_area(shape, 8.0, PI - alpha, ba);
            let removed = flat_area(&notch) - flat_area(&s);
            assert!((removed - want).abs() < 1e-6, "{what}: {removed} vs {want}");
            assert_eq!(s.flanges, notch.flanges, "{what}");
            holds_the_flat_material(&s, &what);
        }
    }

    #[test]
    fn corner_reliefs_hold_at_every_angle_and_size() {
        use std::f64::consts::{FRAC_PI_2, PI};
        // The round relief runs into the bends' straight ends tangentially, a contact the planar
        // booleans have to keep as one point. So every 5° from 45° to 135° is built on the tray and on
        // the trapezoid with both shapes, and on the tray reliefs from 0.5 to 20 mm across.
        let alpha = 90f64.atan2(20.0);
        for shape in [ReliefShape::Round, ReliefShape::Rectangular] {
            for k in 9..=27 {
                let deg = k as f64 * 5.0;
                let ba = deg.to_radians() * 2.0;
                let rv = RuleValues { corner_relief_shape: shape, ..alu() };
                for (name, (s, notes), (notch, _), corners) in [
                    ("tray", tray(&rv, deg, 38.6), tray(&alu(), deg, 38.6), [FRAC_PI_2; 4]),
                    ("trapezoid", trapezoid(&rv, deg), trapezoid(&alu(), deg), [alpha, alpha, PI - alpha, PI - alpha]),
                ] {
                    let what = format!("{shape:?} on the {name} at {deg}°");
                    assert!(notes.is_empty(), "{what}: {notes:?}");
                    let want: f64 = corners.iter().map(|a| relief_area(shape, 8.0, *a, ba)).sum();
                    let removed = flat_area(&notch) - flat_area(&s);
                    assert!((removed - want).abs() < 1e-6, "{what}: {removed} vs {want}");
                    holds_the_flat_material(&s, &what);
                }
            }
            for size in [0.5, 1.0, 3.0, 5.0, 12.0, 20.0] {
                for deg in [72.0f64, 90.0] {
                    let what = format!("{shape:?} {size} mm across at {deg}°");
                    let (s, notes) = tray(&RuleValues { corner_relief_shape: shape, corner_relief: size, ..alu() }, deg, 38.6);
                    assert!(notes.is_empty(), "{what}: {notes:?}");
                    let want = 4.0 * relief_area(shape, size, FRAC_PI_2, deg.to_radians() * 2.0);
                    let removed = flat_area(&tray(&alu(), deg, 38.6).0) - flat_area(&s);
                    assert!((removed - want).abs() < 1e-6, "{what}: {removed} vs {want}");
                    holds_the_flat_material(&s, &what);
                }
            }
        }
    }

    #[test]
    fn a_relief_that_does_not_fit_is_left_out_with_a_note() {
        let closed = tray(&alu(), 72.0, 38.6).0;
        // 200 mm across reaches 100 mm along edges of which the shorter is 90 − 2·s0 = 85.6 mm long.
        // 100 mm across reaches 50 mm: that fits the edge once but not from both its ends, and all four
        // corners are left out together. Nothing, or less than nothing, is too small to cut.
        let sizes = [(200.0, "whole bend"), (100.0, "whole bend"), (0.0, "below 0.01 mm"), (-8.0, "below 0.01 mm"), (f64::NAN, "below 0.01 mm")];
        for (size, why) in sizes {
            let (s, notes) = tray(&RuleValues { corner_relief_shape: ReliefShape::Round, corner_relief: size, ..alu() }, 72.0, 38.6);
            assert_eq!(notes.len(), 4, "{size}: {notes:?}");
            assert!(notes.iter().all(|n| n.starts_with("no corner relief at (") && n.contains(why)), "{size}: {notes:?}");
            // The part is the one the notch gives, corners closed.
            assert_eq!((&s.base, &s.flanges), (&closed.base, &closed.flanges), "{size}");
        }
        // A 120 × 12 strip with flanges on its lower and left edges, a relief 16 mm across. Its 8 mm fit
        // on both edges at the corner (the left one is 12 − s0 = 9.82 long), but the far corner of the
        // square is 8·√2 = 11.3 mm from the corner of the bends, and the strip's upper edge is only 9.82
        // from it: no relief may reach as far from its corner as another edge is. The round one reaches
        // 8 mm and fits.
        let strip = |shape: ReliefShape| {
            let base = rect(Vec2::new(0.0, 0.0), Vec2::new(120.0, 0.0), Vec2::new(120.0, 12.0), Vec2::new(0.0, 12.0));
            let mut s = base_flange("Body1", "AL2", &RuleValues { corner_relief_shape: shape, corner_relief: 16.0, ..alu() }, Plane::XY, base);
            let picks = [Vec3::new(60.0, 0.0, 2.0), Vec3::new(0.0, 6.0, 2.0)];
            let (_, notes) = s.add_flanges_noted(&picks, 20.0, 72f64.to_radians(), 1.0, "inside", false).unwrap();
            (s, notes)
        };
        let (s, notes) = strip(ReliefShape::Rectangular);
        assert!(notes.len() == 1 && notes[0].contains("another edge of the base"), "{notes:?}");
        assert!(s.base.outer.segs.len() == 4 && s.solid().is_ok());
        let (s, notes) = strip(ReliefShape::Round);
        assert!(notes.is_empty() && s.base.outer.segs.iter().filter(|g| matches!(g, Seg2::Arc { .. })).count() == 1, "{notes:?}");
        holds_the_flat_material(&s, "the strip");
        // A right triangle with flanges on its two legs: the base's legs are 100 − 2·s0 = 95.6 long and
        // its long edge passes (100 − 2·s0)/√2 = 67.6 mm from the corner of the bends. A round relief of
        // radius 75 would cut through that edge; one of radius 65 stays inside.
        let triangle = |size: f64| {
            let pts = [Vec2::new(0.0, 0.0), Vec2::new(100.0, 0.0), Vec2::new(0.0, 100.0)];
            let rv = RuleValues { corner_relief_shape: ReliefShape::Round, corner_relief: size, ..alu() };
            let mut s = base_flange("Body1", "AL2", &rv, Plane::XY, Region2 { outer: Loop2::polygon(&pts).ccw(), holes: vec![] });
            let picks = [Vec3::new(50.0, 0.0, 2.0), Vec3::new(0.0, 50.0, 2.0)];
            let (_, notes) = s.add_flanges_noted(&picks, 20.0, 72f64.to_radians(), 1.0, "inside", false).unwrap();
            (s, notes)
        };
        let (s, notes) = triangle(150.0);
        assert!(notes.len() == 1 && notes[0].contains("another edge of the base"), "{notes:?}");
        assert!(s.base.outer.segs.len() == 3 && s.solid().is_ok());
        let (s, notes) = triangle(130.0);
        assert!(notes.is_empty() && s.base.outer.segs.len() == 4, "{notes:?}");
        holds_the_flat_material(&s, "the triangle");
    }

    #[test]
    fn a_corner_with_a_flange_of_an_earlier_feature_gets_no_relief() {
        // Like the closing, the relief belongs to the corners between the flanges of one feature.
        let rv = RuleValues { corner_relief_shape: ReliefShape::Round, ..alu() };
        let base = rect(Vec2::new(0.0, 0.0), Vec2::new(120.0, 0.0), Vec2::new(120.0, 90.0), Vec2::new(0.0, 90.0));
        let mut s = base_flange("Body1", "AL2", &rv, Plane::XY, base);
        s.add_flanges(&[Vec3::new(60.0, 0.0, 2.0)], 38.6, 72f64.to_radians(), 1.0, "inside", false).unwrap();
        let (_, notes) = s.add_flanges_noted(&[Vec3::new(120.0, 45.0, 2.0)], 25.0, 72f64.to_radians(), 1.0, "inside", false).unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        assert!(s.base.outer.segs.iter().all(|g| matches!(g, Seg2::Line { .. })));
        assert!(s.solid().is_ok() && s.flat_regions().unwrap().len() == 1);
    }

    #[test]
    fn closed_corners_keep_their_flat_pattern_at_every_angle() {
        // 45° to 135° in steps of 5° on the tray and on a trapezoid. Which angles lose the flat
        // pattern to a rounding slip in the planar arrangement is a matter of luck (50°, 60°, 65°,
        // 75°, 105° and 110° did on the tray), so every one is built.
        let tray = [Vec2::new(0.0, 0.0), Vec2::new(120.0, 0.0), Vec2::new(120.0, 90.0), Vec2::new(0.0, 90.0)];
        let tray_picks = [Vec3::new(60.0, 0.0, 2.0), Vec3::new(120.0, 45.0, 2.0), Vec3::new(60.0, 90.0, 2.0), Vec3::new(0.0, 45.0, 2.0)];
        let trapezoid = [Vec2::new(0.0, 0.0), Vec2::new(120.0, 0.0), Vec2::new(100.0, 90.0), Vec2::new(20.0, 90.0)];
        let trapezoid_picks = [Vec3::new(60.0, 0.0, 2.0), Vec3::new(110.0, 45.0, 2.0), Vec3::new(60.0, 90.0, 2.0), Vec3::new(10.0, 45.0, 2.0)];
        for (name, pts, picks) in [("tray", tray, tray_picks), ("trapezoid", trapezoid, trapezoid_picks)] {
            for k in 9..=27 {
                let deg = k as f64 * 5.0;
                let what = format!("{name} at {deg}°");
                let base = Region2 { outer: Loop2::polygon(&pts).ccw(), holes: vec![] };
                let mut s = base_flange("Body1", "AL2", &alu(), Plane::XY, base);
                let (made, notes) = s.add_flanges_noted(&picks, 38.6, deg.to_radians(), 1.0, "inside", false).unwrap();
                assert!(notes.is_empty(), "{what}: {notes:?}");
                let corners = s.corners(&made);
                assert_eq!(corners.len(), 4, "{what}");
                ends_are_gap_apart(&s, &corners, 0.5, &what);
                holds_the_flat_material(&s, &what);
            }
        }
    }

    /// Edge flanges on `base`, each with its own height and angle, added in one call. Returns the
    /// sheet and the notes on corners left open.
    fn flanges_each(base: Region2, rv: &RuleValues, each: &[EdgeFlange]) -> (SheetBody, Vec<String>) {
        let mut s = base_flange("Body1", "AL2", rv, Plane::XY, base);
        let notes = s.add_flanges_each(each, rv.bend_radius, "inside", false).unwrap().1;
        (s, notes)
    }

    /// The 120 x 90 tray's four flanges (bottom, right, top, left), each with its own angle
    /// (degrees) and height.
    fn tray_each(rv: &RuleValues, angles: [f64; 4], heights: [f64; 4]) -> (SheetBody, Vec<String>) {
        let base = rect(Vec2::new(0.0, 0.0), Vec2::new(120.0, 0.0), Vec2::new(120.0, 90.0), Vec2::new(0.0, 90.0));
        let t = rv.thickness;
        let picks = [Vec3::new(60.0, 0.0, t), Vec3::new(120.0, 45.0, t), Vec3::new(60.0, 90.0, t), Vec3::new(0.0, 45.0, t)];
        let each: Vec<EdgeFlange> = (0..4).map(|i| EdgeFlange { pick: picks[i], height: heights[i], angle: angles[i].to_radians() }).collect();
        flanges_each(base, rv, &each)
    }

    /// The material of a base flange bent up, for asking whether a folded point lies in it. Worked
    /// out from the fold on its own (not through `map3`): the panel through the inverse of its fold,
    /// the bend from the radius and turn about the bend's axis.
    struct Material {
        back: Mat,
        panel: Vec<Vec2>,
        axis: (Vec3, Vec3),
        p: Vec2,
        d: Vec2,
        span: (f64, f64),
        radius: f64,
        angle: f64,
        t: f64,
    }

    impl Material {
        fn of(s: &SheetBody, i: usize) -> Material {
            let f = &s.flanges[i];
            assert!(f.parent.is_none() && f.angle > 0.0);
            let (o, k, _) = s.bend_axis(f);
            Material {
                back: mat_inverse(&s.fold(i)).unwrap(),
                panel: corners_of(&s.panel(f)),
                axis: (o, k),
                p: f.p,
                d: f.d,
                span: s.zone_span(f),
                radius: f.radius,
                angle: f.angle,
                t: s.t,
            }
        }

        /// How deep (mm) `q` lies inside the flange's panel or bend; None when it is outside both.
        fn depth(&self, q: Vec3) -> Option<f64> {
            let flat = apply_point(&self.back, q);
            let m = self.panel.len();
            let sides = (0..m).map(|j| {
                let (a, b) = (self.panel[j], self.panel[(j + 1) % m]);
                (b - a).cross(Vec2::new(flat.x, flat.y) - a) / (b - a).len()
            });
            let in_panel = sides.fold(flat.z.min(self.t - flat.z), f64::min);
            // The bend turns the sheet under its axis toward the flange: a point's distance from
            // the axis is its radius, its turn from straight below the axis its place across the bend.
            let (o, k) = self.axis;
            let rel = q - o;
            let perp = rel - k * rel.dot(k);
            let down = Vec3::Z * -1.0;
            let (rho, turn) = (perp.len(), perp.dot(k.cross(down)).atan2(perp.dot(down)));
            let u = (Vec2::new(q.x, q.y) - self.p).dot(self.d);
            let in_bend = [rho - self.radius, self.radius + self.t - rho, turn * rho, (self.angle - turn) * rho, u - self.span.0, self.span.1 - u]
                .into_iter()
                .fold(f64::INFINITY, f64::min);
            let depth = in_panel.max(in_bend);
            (depth > 0.0).then_some(depth)
        }
    }

    /// Folded, the two flanges at corner `k` keep out of each other: no point of one's bend or
    /// panel near the corner (sampled every 0.5 mm or finer, at three heights through the sheet)
    /// lies in the other's. Each sample is also found inside its own flange, which checks
    /// [`Material`] against the fold that places the solid's faces.
    fn flanges_keep_clear(s: &SheetBody, k: &Corner, what: &str) {
        let (ma, mb) = (Material::of(s, k.a), Material::of(s, k.b));
        for (i, at_end, own, other) in [(k.a, k.a_end, &ma, &mb), (k.b, k.b_end, &mb, &ma)] {
            let f = &s.flanges[i];
            let ba = s.allowance(f);
            let mut samples = Vec::new();
            // The bend's 12 mm nearest the corner.
            let (u0, u1) = s.zone_span(f);
            for iu in 0..48 {
                let du = 0.02 + 0.25 * iu as f64;
                let u = if at_end { u1 - du } else { u0 + du };
                for is in 0..24 {
                    samples.push((Piece::Zone(i), f.p + f.d * u + f.n * (ba * (is as f64 + 0.5) / 24.0)));
                }
            }
            // The panel's 25 mm nearest its end, over its whole height.
            let (q0, q1, inward) = if at_end {
                let b = f.p + f.n * ba + f.d * (f.len + f.ext.1);
                (b, b + f.n * f.leg + f.d * (f.slope.1 * f.leg), f.d * -1.0)
            } else {
                let a = f.p + f.n * ba - f.d * f.ext.0;
                (a, a + f.n * f.leg - f.d * (f.slope.0 * f.leg), f.d)
            };
            let rows = (f.leg / 0.5).ceil().max(1.0) as usize;
            for ir in 0..rows {
                let on_end = q0 + (q1 - q0) * ((ir as f64 + 0.5) / rows as f64);
                for iu in 0..50 {
                    samples.push((Piece::Panel(Some(i)), on_end + inward * (0.02 + 0.5 * iu as f64)));
                }
            }
            for (piece, flat) in samples {
                for z in [0.1 * s.t, 0.5 * s.t, 0.9 * s.t] {
                    let q = s.map3(piece, flat, z);
                    assert!(own.depth(q).is_some(), "{what}: flange {i}: its own point {flat:?} at {z} ({piece:?}) is not found in it");
                    let d = other.depth(q);
                    assert!(
                        d.is_none_or(|d| d < 1e-6),
                        "{what}: flange {i}'s {piece:?} at {flat:?}, height {z}, lies {d:?} mm inside the other flange"
                    );
                }
            }
        }
    }

    /// The panel of flange `i` of the tray by hand, in the flat pattern. Take a flange at θ on the
    /// edge y = 0 and its neighbour at φ on x = 0, T the thickness. A point of the first one's inner
    /// face, w up its outer face from the virtual sharp, is (x, −w·cosθ + T·sinθ, w·sinθ + T·cosθ);
    /// the inner faces are the planes y·sinθ + z·cosθ = T and x·sinφ + z·cosφ = T, and the plane
    /// halving them passes through the corner with normal (sinφ, −sinθ, cosφ − cosθ), of length
    /// √(2 − 2·cosθ·cosφ). Keeping the inner face gap/2 from it gives the panel's end
    /// x(w) = ((g/2)·√(2 − 2·cosθ·cosφ) + T·(1 − cosθ·cosφ) − w·sinθ·cosφ) / sinφ for w = s0 … h, at
    /// flat y = s0 − BA − (w − s0). Each flange is taken in its own frame: the origin at the start
    /// corner of its edge, u along the edge, v into the base; flange i − 1 is at its start and
    /// flange i + 1 at its end.
    fn hand_panel(rv: &RuleValues, angles: [f64; 4], heights: [f64; 4], i: usize) -> Vec<Vec2> {
        let (t, r, g) = (rv.thickness, rv.bend_radius, rv.gap);
        let (o, d, v, len) = [
            (Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0), 120.0),
            (Vec2::new(120.0, 0.0), Vec2::new(0.0, 1.0), Vec2::new(-1.0, 0.0), 90.0),
            (Vec2::new(120.0, 90.0), Vec2::new(-1.0, 0.0), Vec2::new(0.0, -1.0), 120.0),
            (Vec2::new(0.0, 90.0), Vec2::new(0.0, -1.0), Vec2::new(1.0, 0.0), 90.0),
        ][i];
        let th = angles[i].to_radians();
        let (s0, ba) = ((r + t) * (th / 2.0).tan(), th * (r + rv.k_factor * t));
        let x_end = |phi: f64, w: f64| {
            let c = th.cos() * phi.cos();
            ((g / 2.0) * (2.0 - 2.0 * c).sqrt() + t * (1.0 - c) - w * th.sin() * phi.cos()) / phi.sin()
        };
        let (prev, next) = (angles[(i + 3) % 4].to_radians(), angles[(i + 1) % 4].to_radians());
        let flat = |u: f64, w: f64| o + d * u + v * (s0 - ba - (w - s0));
        let h = heights[i];
        vec![flat(x_end(prev, s0), s0), flat(x_end(prev, h), h), flat(len - x_end(next, h), h), flat(len - x_end(next, s0), s0)]
    }

    /// The tray's flat area by hand: the base inside the four bend start lines, every bend zone
    /// (the base's trimmed edge by the bend allowance) and every panel of [`hand_panel`]. With
    /// s0 = (R + T)·tan(θ/2) a flange's setback and BA = θ·(R + K·T) its bend allowance, the base
    /// is (120 − s0_left − s0_right) by (90 − s0_bottom − s0_top).
    fn hand_flat_area(rv: &RuleValues, angles: [f64; 4], heights: [f64; 4]) -> f64 {
        let s0 = angles.map(|a| (rv.bend_radius + rv.thickness) * (a.to_radians() / 2.0).tan());
        let ba = angles.map(|a| a.to_radians() * (rv.bend_radius + rv.k_factor * rv.thickness));
        let (w, h) = (120.0 - s0[3] - s0[1], 90.0 - s0[0] - s0[2]);
        let panels: f64 = (0..4).map(|i| signed_area(&hand_panel(rv, angles, heights, i)).abs()).sum();
        w * h + (ba[0] + ba[2]) * w + (ba[1] + ba[3]) * h + panels
    }

    /// 60°, 90°, 105° and 120° on the tray's bottom, right, top and left edges.
    const MIXED: [f64; 4] = [60.0, 90.0, 105.0, 120.0];

    #[test]
    fn flanges_with_their_own_angle_and_height_close_at_their_corners() {
        // Four angles on a 120 x 90 tray, then four heights as well, in one call: every panel ends
        // where the hand derivation says, whatever its neighbour's angle and height.
        let rv = alu();
        for (heights, what) in [([38.6; 4], "one height"), ([38.6, 30.0, 45.0, 25.0], "four heights")] {
            let (s, notes) = tray_each(&rv, MIXED, heights);
            assert!(notes.is_empty(), "{what}: {notes:?}");
            for i in 0..4 {
                let (f, th) = (&s.flanges[i], MIXED[i].to_radians());
                assert!((f.angle - th).abs() < 1e-12, "{what}: flange {i}");
                assert!((f.leg - (heights[i] - (rv.bend_radius + rv.thickness) * (th / 2.0).tan())).abs() < 1e-9, "{what}: flange {i}");
                let got = corners_of(&s.panel(f));
                assert_eq!(got.len(), 4, "{what}: flange {i}");
                for q in hand_panel(&rv, MIXED, heights, i) {
                    assert!(got.iter().any(|p| p.dist(q) < 1e-9), "{what}: flange {i}: {q:?} not in {got:?}");
                }
            }
            // The flat pattern is the base, the bend zones and those panels, nothing more or less;
            // folded it holds the same material (`holds_the_flat_material`), so the volume is this
            // area times the thickness.
            let (area, want) = (flat_area(&s), hand_flat_area(&rv, MIXED, heights));
            assert!((area - want).abs() < 1e-6, "{what}: flat area {area} vs {want}");
            let corners = s.corners(&[0, 1, 2, 3]);
            assert_eq!(corners.len(), 4, "{what}");
            ends_are_gap_apart(&s, &corners, rv.gap, what);
            for k in &corners {
                flanges_keep_clear(&s, k, what);
            }
            holds_the_flat_material(&s, what);
        }
    }

    #[test]
    fn a_trapezoid_with_unequal_angles_and_heights_closes_at_its_acute_and_obtuse_corners() {
        // 77.5° and 102.5° base corners; flanges at 60°, 105°, 75° and 120°, each its own height.
        let pts = [Vec2::new(0.0, 0.0), Vec2::new(120.0, 0.0), Vec2::new(100.0, 90.0), Vec2::new(20.0, 90.0)];
        let picks = [Vec3::new(60.0, 0.0, 2.0), Vec3::new(110.0, 45.0, 2.0), Vec3::new(60.0, 90.0, 2.0), Vec3::new(10.0, 45.0, 2.0)];
        let (angles, heights) = ([60.0f64, 105.0, 75.0, 120.0], [38.6, 30.0, 45.0, 25.0]);
        let each: Vec<EdgeFlange> = (0..4).map(|i| EdgeFlange { pick: picks[i], height: heights[i], angle: angles[i].to_radians() }).collect();
        let (s, notes) = flanges_each(Region2 { outer: Loop2::polygon(&pts).ccw(), holes: vec![] }, &alu(), &each);
        assert!(notes.is_empty(), "{notes:?}");
        for i in 0..4 {
            let (f, th) = (&s.flanges[i], angles[i].to_radians());
            assert!((f.angle - th).abs() < 1e-12 && (f.leg - (heights[i] - 3.0 * (th / 2.0).tan())).abs() < 1e-9, "flange {i}: {f:?}");
        }
        let corners = s.corners(&[0, 1, 2, 3]);
        assert_eq!(corners.len(), 4);
        ends_are_gap_apart(&s, &corners, 0.5, "trapezoid");
        for k in &corners {
            flanges_keep_clear(&s, k, "trapezoid");
        }
        holds_the_flat_material(&s, "trapezoid");
    }

    /// The angles of the sweep below.
    const SWEEP: [f64; 7] = [45.0, 60.0, 75.0, 90.0, 105.0, 120.0, 135.0];

    #[test]
    fn every_pair_of_angles_closes_at_any_base_corner() {
        // A parallelogram base (120 and 90 long) with a corner of 60°, 90° or 120° at the origin;
        // flanges 38.6 high on its two edges there, in one call, at every pair of the angles: 147
        // corners, each closed with the gap, the flanges clear of each other, the part one valid
        // shell holding the flat pattern's material.
        let rv = alu();
        let mut cases = 0;
        for alpha in [60.0f64, 90.0, 120.0] {
            let u = Vec2::new(alpha.to_radians().cos(), alpha.to_radians().sin());
            let pts = [Vec2::new(0.0, 0.0), Vec2::new(120.0, 0.0), Vec2::new(120.0, 0.0) + u * 90.0, u * 90.0];
            for a in SWEEP {
                for b in SWEEP {
                    let what = format!("corner {alpha}°, flanges {a}° and {b}°");
                    let each = [
                        EdgeFlange { pick: Vec3::new(60.0, 0.0, 2.0), height: 38.6, angle: a.to_radians() },
                        EdgeFlange { pick: Vec3::new(u.x * 45.0, u.y * 45.0, 2.0), height: 38.6, angle: b.to_radians() },
                    ];
                    let (s, notes) = flanges_each(Region2 { outer: Loop2::polygon(&pts).ccw(), holes: vec![] }, &rv, &each);
                    assert!(notes.is_empty(), "{what}: {notes:?}");
                    assert!((s.flanges[0].angle - a.to_radians()).abs() < 1e-12 && (s.flanges[1].angle - b.to_radians()).abs() < 1e-12, "{what}");
                    let corners = s.corners(&[0, 1]);
                    assert_eq!(corners.len(), 1, "{what}");
                    ends_are_gap_apart(&s, &corners, rv.gap, &what);
                    flanges_keep_clear(&s, &corners[0], &what);
                    holds_the_flat_material(&s, &what);
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, 147);
    }

    #[test]
    fn corner_reliefs_are_cut_between_flanges_of_unequal_angle_and_height() {
        use std::f64::consts::FRAC_PI_2;
        // The relief is cut in the base, so flanges with their own angle and height get the same
        // one: 4 mm into the base at each of its four corners, and every bend 4 mm shorter at both
        // ends, each over its own width BA = θ·(R + K·T).
        let heights = [38.6, 30.0, 45.0, 25.0];
        let (notch, _) = tray_each(&alu(), MIXED, heights);
        for shape in [ReliefShape::Round, ReliefShape::Rectangular] {
            let what = format!("{shape:?}");
            let (s, notes) = tray_each(&RuleValues { corner_relief_shape: shape, ..alu() }, MIXED, heights);
            assert!(notes.is_empty(), "{what}: {notes:?}");
            assert_eq!(s.flanges, notch.flanges, "{what}");
            // The bend on y = 0 runs between the setbacks of its neighbours at 120° and 90°:
            // 120 − 3·tan 60° − 3·tan 45° long, less the two reliefs.
            let (bends, before) = (s.bends(), notch.bends());
            assert_eq!((bends.len(), before.len()), (4, 4), "{what}");
            assert!((bends[0].length - (120.0 - 3.0 * 60f64.to_radians().tan() - 3.0 - 8.0)).abs() < 1e-9, "{what}: {}", bends[0].length);
            for (b, b0) in bends.iter().zip(&before) {
                assert!((b.length - (b0.length - 8.0)).abs() < 1e-9, "{what}: {} vs {}", b.length, b0.length);
            }
            let in_the_bends: f64 = MIXED.iter().map(|deg| 8.0 * deg.to_radians() * 2.0).sum();
            let (removed, want) = (flat_area(&notch) - flat_area(&s), 4.0 * relief_area(shape, 8.0, FRAC_PI_2, 0.0) + in_the_bends);
            assert!((removed - want).abs() < 1e-6, "{what}: {removed} vs {want}");
            holds_the_flat_material(&s, &what);
        }
    }
}
