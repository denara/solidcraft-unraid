//! Sewing a solid from face descriptions: each face is its boundary loops (lines, circular
//! arcs and cubic Bézier chains, counter-clockwise about the outward normal; holes clockwise)
//! and its surface (planar, revolved from a line, extruded from an arc, or ruled between two
//! Bézier chains). Vertices and edges at the same places are shared, so the faces close into
//! one shell without booleans — for solids whose faces are known exactly, such as folded
//! sheet metal.

use std::collections::HashMap;
use std::sync::Arc;

use mt::builder;
use solidcraft_geom::Vec3;
use truck_modeling as mt;

use crate::body::{Body, Solid, p3, v3};
use crate::{KernelError, Result, guard};

/// Most control points of one Bézier chain edge.
pub const MAX_CHAIN_POINTS: usize = 3 * 1024 + 1;

/// A boundary edge.
#[derive(Clone, Debug, PartialEq)]
pub enum EdgeSpec {
    Line {
        a: Vec3,
        b: Vec3,
    },
    /// A circular arc from `a` to `b` through `mid`.
    Arc {
        a: Vec3,
        b: Vec3,
        mid: Vec3,
    },
    /// A chain of cubic Bézier pieces: 3n + 1 control points (n ≥ 1); the curve passes
    /// through `ctrl[0]`, `ctrl[3]`, … `ctrl[3n]`. For curves with no exact line, arc or conic
    /// form, such as a cut-out edge traced across a sheet metal bend.
    Chain {
        ctrl: Arc<[Vec3]>,
    },
}

impl EdgeSpec {
    fn ends(&self) -> (Vec3, Vec3) {
        match self {
            EdgeSpec::Line { a, b } | EdgeSpec::Arc { a, b, .. } => (*a, *b),
            EdgeSpec::Chain { ctrl } => (ctrl.first().copied().unwrap_or(Vec3::ZERO), ctrl.last().copied().unwrap_or(Vec3::ZERO)),
        }
    }

    /// The same edge, run the other way.
    pub fn reversed(&self) -> EdgeSpec {
        match self {
            EdgeSpec::Line { a, b } => EdgeSpec::Line { a: *b, b: *a },
            EdgeSpec::Arc { a, b, mid } => EdgeSpec::Arc { a: *b, b: *a, mid: *mid },
            EdgeSpec::Chain { ctrl } => EdgeSpec::Chain { ctrl: ctrl.iter().rev().copied().collect() },
        }
    }

    /// A point telling apart edges with the same ends, whichever way they run.
    fn mid_key(&self) -> Option<Key> {
        match self {
            EdgeSpec::Line { .. } => None,
            EdgeSpec::Arc { mid, .. } => Some(key(*mid)),
            EdgeSpec::Chain { ctrl } => {
                let n = ctrl.len().max(1) as f64;
                Some(key(ctrl.iter().fold(Vec3::ZERO, |s, p| s + *p) * (1.0 / n)))
            }
        }
    }

    fn valid(&self) -> bool {
        match self {
            EdgeSpec::Line { a, b } => a.is_finite() && b.is_finite(),
            EdgeSpec::Arc { a, b, mid } => a.is_finite() && b.is_finite() && mid.is_finite(),
            EdgeSpec::Chain { ctrl } => {
                ctrl.len() >= 4 && ctrl.len() <= MAX_CHAIN_POINTS && (ctrl.len() - 1) % 3 == 0 && ctrl.iter().all(|p| p.is_finite())
            }
        }
    }
}

/// A face's surface.
#[derive(Clone, Debug, PartialEq)]
pub enum SurfSpec {
    /// The plane of the boundary.
    Plane,
    /// The surface the line `a`→`b` sweeps turning `angle` about the axis.
    Revolved { a: Vec3, b: Vec3, axis_origin: Vec3, axis: Vec3, angle: f64 },
    /// The surface an arc sweeps moving along `dir`.
    Extruded { a: Vec3, b: Vec3, mid: Vec3, dir: Vec3 },
    /// The straight lines joining equal parameters of two Bézier chains with the same number
    /// of pieces, `a` and `b` running the same way; both chains lie on it as boundary edges.
    Ruled { a: EdgeSpec, b: EdgeSpec },
}

/// A face: boundary loops, surface, and a point on it with its outward normal (curved faces).
#[derive(Clone, Debug, PartialEq)]
pub struct FaceSpec {
    pub loops: Vec<Vec<EdgeSpec>>,
    pub surface: SurfSpec,
    pub probe: Option<(Vec3, Vec3)>,
}

type Key = (i64, i64, i64);

fn key(p: Vec3) -> Key {
    let q = |x: f64| (x * 1e6).round() as i64;
    (q(p.x), q(p.y), q(p.z))
}

#[derive(Default)]
struct Cache {
    verts: HashMap<Key, mt::Vertex>,
    edges: HashMap<(Key, Key, Option<Key>, usize), mt::Edge>,
}

impl Cache {
    fn vertex(&mut self, p: Vec3) -> mt::Vertex {
        self.verts.entry(key(p)).or_insert_with(|| builder::vertex(p3(p))).clone()
    }
    fn edge(&mut self, e: &EdgeSpec, sheet: usize) -> Result<mt::Edge> {
        let (a, b) = e.ends();
        let mid = e.mid_key();
        let (ka, kb) = (key(a), key(b));
        if let Some(found) = self.edges.get(&(ka, kb, mid, sheet)) {
            return Ok(found.clone());
        }
        if let Some(found) = self.edges.get(&(kb, ka, mid, sheet)) {
            return Ok(found.inverse());
        }
        let (va, vb) = (self.vertex(a), self.vertex(b));
        let made = match e {
            EdgeSpec::Line { .. } => builder::line(&va, &vb),
            EdgeSpec::Arc { mid, .. } => arc_edge(&va, &vb, *mid),
            EdgeSpec::Chain { .. } => {
                let curve = self.chain(e)?;
                mt::Edge::try_new(&va, &vb, mt::Curve::BSplineCurve(curve)).map_err(|e| KernelError::Failed(format!("sew: chain edge: {e}")))?
            }
        };
        self.edges.insert((ka, kb, mid, sheet), made.clone());
        Ok(made)
    }

    /// A Bézier chain as a cubic B-spline (triple inner knots) ending on the shared vertices.
    fn chain(&mut self, e: &EdgeSpec) -> Result<mt::BSplineCurve<mt::Point3>> {
        let EdgeSpec::Chain { ctrl } = e else { return Err(KernelError::Invalid("sew: not a Bézier chain".into())) };
        if !e.valid() {
            return Err(KernelError::Invalid("sew: a Bézier chain needs 3n + 1 finite control points".into()));
        }
        let n = (ctrl.len() - 1) / 3;
        let mut knots = vec![0.0; 4];
        for i in 1..n {
            knots.extend([i as f64; 3]);
        }
        knots.extend([n as f64; 4]);
        let mut pts: Vec<mt::Point3> = ctrl.iter().map(|p| p3(*p)).collect();
        let (a, b) = e.ends();
        if let Some(first) = pts.first_mut() {
            *first = self.vertex(a).point();
        }
        if let Some(last) = pts.last_mut() {
            *last = self.vertex(b).point();
        }
        mt::BSplineCurve::try_new(mt::KnotVec::from(knots), pts).map_err(|e| KernelError::Failed(format!("sew: chain curve: {e}")))
    }
}

/// Sew the faces into a closed solid.
pub fn sew(faces: &[FaceSpec]) -> Result<Body> {
    sew_impl(faces, false, None).map(|(body, _)| body)
}

/// Separate boundary sheets at a shared geometric edge without moving it.
/// Optional transient sources follow only unchanged produced face identities.
/// Also reports the face each face of the body was built from (by its place in `faces`; `None`
/// when that could not be followed).
pub(crate) fn sew_touching_with_sources(faces: &[FaceSpec], sources: &[Option<crate::FaceSource>]) -> Result<(Body, Option<Vec<usize>>)> {
    if sources.len() != faces.len() {
        return Err(KernelError::Invalid("sew source alignment".into()));
    }
    sew_impl(faces, true, Some(sources))
}

type EdgeUse = (usize, usize, usize);

fn touching_sheets(faces: &[FaceSpec]) -> Result<HashMap<EdgeUse, usize>> {
    type Use = (EdgeUse, Vec3, Vec3, Vec3);
    let mut uses: HashMap<(Key, Key), Vec<Use>> = HashMap::new();
    for (fi, face) in faces.iter().enumerate() {
        let Some(outer) = face.loops.first() else { continue };
        let normal = outer
            .iter()
            .fold(Vec3::ZERO, |n, e| {
                let (a, b) = e.ends();
                n + a.cross(b)
            })
            .normalized();
        for (li, lp) in face.loops.iter().enumerate() {
            for (ei, e) in lp.iter().enumerate() {
                if matches!(e, EdgeSpec::Line { .. }) && matches!(face.surface, SurfSpec::Plane) {
                    let (a, b) = e.ends();
                    let (ka, kb) = (key(a), key(b));
                    let k = if ka < kb { (ka, kb) } else { (kb, ka) };
                    if let Some(n) = normal {
                        uses.entry(k).or_default().push(((fi, li, ei), a, b, n));
                    }
                }
            }
        }
    }
    let mut sheets = HashMap::new();
    for group in uses.values().filter(|g| g.len() > 2) {
        if group.len() != 4 {
            return Err(KernelError::Invalid("sew: unsupported edge contact".into()));
        }
        let mut paired = std::collections::HashSet::new();
        for (i, (use_a, a, b, n)) in group.iter().enumerate() {
            if paired.contains(&i) {
                continue;
            }
            let t = (*b - *a).normalized().ok_or_else(|| KernelError::Invalid("sew: a degenerate contact".into()))?;
            // Pair the two faces bounding each material wedge, not the empty
            // wedge between them. Each pair uses the edge in opposite directions.
            let j = group
                .iter()
                .enumerate()
                .find_map(|(j, (_, c, d, m))| {
                    (i != j
                        && !paired.contains(&j)
                        && key(*a) == key(*d)
                        && key(*b) == key(*c)
                        && n.cross(t).dot(*m) < -1e-8
                        && m.cross(-t).dot(*n) < -1e-8)
                        .then_some(j)
                })
                .ok_or_else(|| KernelError::Invalid("sew: ambiguous edge contact".into()))?;
            let use_b = group.get(j).ok_or_else(|| KernelError::Invalid("sew: edge contact".into()))?.0;
            let sheet = paired.len() / 2;
            sheets.insert(*use_a, sheet);
            sheets.insert(use_b, sheet);
            paired.insert(i);
            paired.insert(j);
        }
    }
    Ok(sheets)
}

fn faces_connected(faces: &[mt::Face]) -> bool {
    let mut by_edge: HashMap<mt::EdgeID, Vec<usize>> = HashMap::new();
    for (i, f) in faces.iter().enumerate() {
        for e in f.edge_iter() {
            by_edge.entry(e.id()).or_default().push(i);
        }
    }
    let mut seen = std::collections::HashSet::new();
    let mut todo = vec![0];
    while let Some(i) = todo.pop() {
        if seen.insert(i)
            && let Some(f) = faces.get(i)
        {
            for e in f.edge_iter() {
                if let Some(adjacent) = by_edge.get(&e.id()) {
                    todo.extend(adjacent.iter().copied().filter(|j| !seen.contains(j)));
                }
            }
        }
    }
    seen.len() == faces.len()
}

/// Keep disconnected closed boundaries as distinct shells, preserving orientation.
fn closed_components(faces: Vec<mt::Face>) -> Result<Vec<mt::Shell>> {
    const MAX_EDGE_USES: usize = 500_000;
    const MAX_COMPONENTS: usize = 1024;
    let fail = |s: &str| KernelError::Invalid(format!("sew: {s}"));
    let mut by_edge: HashMap<mt::EdgeID, Vec<usize>> = HashMap::new();
    let mut edge_uses = 0usize;
    for (i, face) in faces.iter().enumerate() {
        for edge in face.edge_iter() {
            edge_uses = edge_uses.saturating_add(1);
            if edge_uses > MAX_EDGE_USES {
                return Err(fail("edge budget exceeded"));
            }
            let uses = by_edge.entry(edge.id()).or_default();
            if uses.len() >= 2 {
                return Err(fail("nonmanifold edge contact"));
            }
            uses.push(i);
        }
    }
    if by_edge.values().any(|uses| uses.len() != 2) {
        return Err(fail("the faces do not close"));
    }
    let mut seen = vec![false; faces.len()];
    let mut vertices = HashMap::new();
    let mut components = Vec::new();
    for first in 0..faces.len() {
        if seen.get(first).copied().unwrap_or(true) {
            continue;
        }
        if components.len() >= MAX_COMPONENTS {
            return Err(fail("component budget exceeded"));
        }
        let component = components.len();
        let mut todo = vec![first];
        if let Some(slot) = seen.get_mut(first) {
            *slot = true;
        }
        let mut indices = Vec::new();
        while let Some(i) = todo.pop() {
            let face = faces.get(i).ok_or_else(|| fail("missing component face"))?;
            indices.push(i);
            for vertex in face.vertex_iter() {
                if vertices.insert(vertex.id(), component).is_some_and(|previous| previous != component) {
                    return Err(fail("bodies touch only at a vertex"));
                }
            }
            for edge in face.edge_iter() {
                let adjacent = by_edge.get(&edge.id()).ok_or_else(|| fail("missing component edge"))?;
                for j in adjacent {
                    let slot = seen.get_mut(*j).ok_or_else(|| fail("missing adjacent face"))?;
                    if !*slot {
                        *slot = true;
                        todo.push(*j);
                    }
                }
            }
        }
        indices.sort_unstable();
        let component_faces =
            indices.into_iter().map(|i| faces.get(i).cloned().ok_or_else(|| fail("missing component face"))).collect::<Result<Vec<_>>>()?;
        components.push(component_faces.into());
    }
    Ok(components)
}

/// A contact's edge sheets can terminate in separate vertex fans. Give each fan
/// its own vertex at the same coordinates so the boundary remains a manifold.
fn split_vertex_fans(faces: Vec<mt::Face>) -> Result<Vec<mt::Face>> {
    let mut by_vertex: HashMap<mt::VertexID, Vec<usize>> = HashMap::new();
    let mut by_edge: HashMap<mt::EdgeID, Vec<usize>> = HashMap::new();
    let mut vertices = HashMap::new();
    for (i, f) in faces.iter().enumerate() {
        for v in f.vertex_iter() {
            vertices.insert(v.id(), v.clone());
            by_vertex.entry(v.id()).or_default().push(i);
        }
        for e in f.edge_iter() {
            by_edge.entry(e.id()).or_default().push(i);
        }
    }
    let mut replacements: HashMap<(mt::VertexID, usize), mt::Vertex> = HashMap::new();
    for (id, members) in &mut by_vertex {
        members.sort_unstable();
        members.dedup();
        let mut seen = std::collections::HashSet::new();
        let mut fans = Vec::new();
        for start in members.iter().copied() {
            if seen.contains(&start) {
                continue;
            }
            let mut todo = vec![start];
            let mut fan = Vec::new();
            while let Some(i) = todo.pop() {
                if !seen.insert(i) {
                    continue;
                }
                fan.push(i);
                if let Some(f) = faces.get(i) {
                    for e in f.edge_iter().filter(|e| e.absolute_front().id() == *id || e.absolute_back().id() == *id) {
                        if let Some(adjacent) = by_edge.get(&e.id()) {
                            todo.extend(adjacent.iter().copied().filter(|j| !seen.contains(j)));
                        }
                    }
                }
            }
            fans.push(fan);
        }
        if fans.len() > 1 {
            let original = vertices.get(id).ok_or_else(|| KernelError::Failed("sew: missing contact vertex".into()))?;
            for fan in fans {
                let vertex = builder::vertex(original.point());
                for i in fan {
                    replacements.insert((*id, i), vertex.clone());
                }
            }
        }
    }
    let mut edges = HashMap::new();
    for (i, f) in faces.iter().enumerate() {
        for e in f.edge_iter() {
            edges.entry(e.id()).or_insert_with(|| {
                let (a, b) = (e.absolute_front(), e.absolute_back());
                let a = replacements.get(&(a.id(), i)).unwrap_or(a);
                let b = replacements.get(&(b.id(), i)).unwrap_or(b);
                mt::Edge::new(a, b, e.curve())
            });
        }
    }
    faces
        .into_iter()
        .map(|f| {
            let wires: Vec<mt::Wire> = f
                .absolute_boundaries()
                .iter()
                .map(|w| {
                    w.edge_iter()
                        .map(|e| {
                            let rebuilt = edges.get(&e.id()).ok_or_else(|| KernelError::Failed("sew: missing contact edge".into()))?;
                            Ok(if e.front() == e.absolute_front() { rebuilt.clone() } else { rebuilt.inverse() })
                        })
                        .collect::<Result<Vec<_>>>()
                        .map(Into::into)
                })
                .collect::<Result<_>>()?;
            let mut rebuilt = mt::Face::try_new(wires, f.surface()).map_err(|e| KernelError::Failed(format!("sew: contact face: {e}")))?;
            if !f.orientation() {
                rebuilt.invert();
            }
            Ok(rebuilt)
        })
        .collect()
}

fn sew_impl(faces: &[FaceSpec], touching: bool, sources: Option<&[Option<crate::FaceSource>]>) -> Result<(Body, Option<Vec<usize>>)> {
    if faces.len() < 4 || faces.len() > 200_000 {
        return Err(KernelError::Invalid("a solid needs 4…200000 faces".into()));
    }
    let mut edge_uses = 0usize;
    for f in faces {
        for l in &f.loops {
            edge_uses = edge_uses.saturating_add(l.len().max(1));
            if edge_uses > 500_000 {
                return Err(KernelError::Invalid("sew: edge budget exceeded".into()));
            }
            for e in l {
                let (a, b) = e.ends();
                // Ends that weld to one vertex would make truck panic on a zero-length edge (#268).
                if !e.valid() || !a.is_finite() || !b.is_finite() || a.dist(b) < 1e-9 || key(a) == key(b) {
                    return Err(KernelError::Invalid("sew: a degenerate edge".into()));
                }
            }
        }
    }
    guard("sew", || {
        let sheets = if touching { touching_sheets(faces)? } else { HashMap::new() };
        let mut cache = Cache::default();
        let mut out: Vec<mt::Face> = Vec::with_capacity(faces.len());
        for (fi, f) in faces.iter().enumerate() {
            let wires: Vec<mt::Wire> = f
                .loops
                .iter()
                .enumerate()
                .map(|(li, l)| {
                    l.iter()
                        .enumerate()
                        .map(|(ei, e)| cache.edge(e, sheets.get(&(fi, li, ei)).copied().unwrap_or(0)))
                        .collect::<Result<Vec<_>>>()
                        .map(Into::into)
                })
                .collect::<Result<_>>()?;
            let face = match &f.surface {
                SurfSpec::Plane => match builder::try_attach_plane(&wires) {
                    Ok(face) => face,
                    // truck's fit loses thin faces away from the origin: see `plane_about_the_mean`.
                    Err(e) => {
                        let plane = plane_about_the_mean(&wires).ok_or_else(|| KernelError::Failed(format!("sew: face {fi}: {e}")))?;
                        mt::Face::try_new(wires, mt::Surface::Plane(plane)).map_err(|e| KernelError::Failed(format!("sew: face {fi}: {e}")))?
                    }
                },
                SurfSpec::Revolved { a, b, axis_origin, axis, angle } => {
                    let k = axis.normalized().ok_or_else(|| KernelError::Invalid("sew: axis".into()))?;
                    let (va, vb) = (builder::vertex(p3(*a)), builder::vertex(p3(*b)));
                    let swept: mt::Shell = builder::rsweep(&builder::line(&va, &vb), p3(*axis_origin), v3(k), mt::Rad(*angle));
                    let tmp = swept.face_iter().next().cloned().ok_or_else(|| KernelError::Failed("sew: revolved surface".into()))?;
                    curved(tmp.oriented_surface(), wires, f.probe, fi)?
                }
                SurfSpec::Extruded { a, b, mid, dir } => {
                    let (va, vb) = (builder::vertex(p3(*a)), builder::vertex(p3(*b)));
                    let tmp: mt::Face = builder::tsweep(&arc_edge(&va, &vb, *mid), v3(*dir));
                    curved(tmp.oriented_surface(), wires, f.probe, fi)?
                }
                SurfSpec::Ruled { a, b } => {
                    let (ca, cb) = (cache.chain(a)?, cache.chain(b)?);
                    if ca.control_points().len() != cb.control_points().len() {
                        return Err(KernelError::Invalid("sew: ruled chains differ in pieces".into()));
                    }
                    let surface = mt::Surface::BSplineSurface(mt::BSplineSurface::homotopy(ca, cb));
                    curved(surface, wires, f.probe, fi)?
                }
            };
            out.push(face);
        }
        let source_ids: HashMap<_, _> = out
            .iter()
            .enumerate()
            .filter_map(|(i, f)| sources.and_then(|s| s.get(i)).and_then(Option::as_ref).map(|s| (f.id(), (f.orientation(), s))))
            .collect();
        if !sheets.is_empty() && !faces_connected(&out) {
            return Err(KernelError::Invalid("sew: bodies touch only along an edge".into()));
        }
        if !sheets.is_empty() {
            out = split_vertex_fans(out)?;
        }
        // Faces keep their identity into the solid (grouped into shells) and the body.
        let built: HashMap<mt::FaceID, usize> = out.iter().enumerate().map(|(i, f)| (f.id(), i)).collect();
        let shells = closed_components(out)?;
        let solid = Solid::try_new(shells).map_err(|e| KernelError::Failed(format!("sew: the faces do not close: {e}")))?;
        let from = solid.face_iter().map(|f| built.get(&f.id()).copied()).collect();
        let body = Body::new(solid)?;
        let mapped =
            body.solid.face_iter().map(|f| source_ids.get(&f.id()).filter(|(o, _)| *o == f.orientation()).map(|(_, s)| (*s).clone())).collect();
        Ok((body.with_face_sources(sources.map(|_| std::sync::Arc::new(mapped))), from))
    })
}

/// Below this sagitta-to-half-chord ratio an arc is built as its exact rational quadratic from
/// its ends and midpoint. truck's three-point arc (circumcentre, then a turn about it) misses
/// its own ends on very flat arcs: one 12 mm long with a 0.08 µm sagitta (a 225 m radius) ends
/// about 1e-6 mm off its vertex, and a planar face holding it far from the origin could not be
/// given a plane (its wire's points no longer closed).
const FLAT_ARC: f64 = 1e-2;

/// The arc from `a` to `b` through `mid`: the exact rational quadratic for a flat arc, truck's
/// three-point arc otherwise (and should the curve be refused, which its exact ends make
/// impossible).
fn arc_edge(a: &mt::Vertex, b: &mt::Vertex, mid: Vec3) -> mt::Edge {
    let (pa, pb) = (a.point(), b.point());
    let (pa, pb) = (Vec3::new(pa.x, pa.y, pa.z), Vec3::new(pb.x, pb.y, pb.z));
    flat_arc(pa, pb, mid)
        .and_then(|curve| mt::Edge::try_new(a, b, mt::Curve::NurbsCurve(curve)).ok())
        .unwrap_or_else(|| builder::circle_arc(a, b, p3(mid)))
}

/// A flat arc (see [`FLAT_ARC`]) through its midpoint `mid` as one rational quadratic: the middle
/// control point on the line from the chord's midpoint through `mid`, 2 / (1 − t²) as far, weight
/// (1 − t²) / (1 + t²), where t is the sagitta over the half chord. None for a deeper arc, or
/// when `mid` is not halfway along the chord.
fn flat_arc(a: Vec3, b: Vec3, mid: Vec3) -> Option<mt::NurbsCurve<mt::Vector4>> {
    let m = (a + b) * 0.5;
    let half = a.dist(b) / 2.0;
    let chord = (b - a).normalized()?;
    let bow = mid - m;
    let t = bow.len() / half;
    if !(half > 1e-9) || !(t < FLAT_ARC) || bow.dot(chord).abs() > 1e-9 * half.max(1.0) {
        return None;
    }
    let w = (1.0 - t * t) / (1.0 + t * t);
    let apex = m + bow * (2.0 / (1.0 - t * t));
    let h = |p: Vec3, w: f64| mt::Vector4::new(p.x * w, p.y * w, p.z * w, w);
    Some(mt::NurbsCurve::new(mt::BSplineCurve::new(mt::KnotVec::bezier_knot(2), vec![h(a, 1.0), h(apex, w), h(b, 1.0)])))
}

/// A curved face on `surf`, turned so its normal at the probe points outward.
fn curved(mut surf: mt::Surface, wires: Vec<mt::Wire>, probe: Option<(Vec3, Vec3)>, fi: usize) -> Result<mt::Face> {
    use mt::{ParametricSurface3D, SearchParameter};
    if let Some((p, outward)) = probe
        && let Some((u, v)) = surf.search_parameter(p3(p), None, 100)
    {
        let n = surf.normal(u, v);
        if Vec3::new(n.x, n.y, n.z).dot(outward) < 0.0 {
            surf = mt::Invertible::inverse(&surf);
        }
    }
    mt::Face::try_new(wires, surf).map_err(|e| KernelError::Failed(format!("sew: face {fi}: {e}")))
}

/// The plane of a boundary that truck's fit (`builder::try_attach_plane`) refused: truck's fit
/// again, on the boundary moved so that the mean of its control points is the origin, and the
/// plane moved back. None when truck refuses that one too (the boundary is not flat within
/// truck's 1e-6, or has no area).
///
/// truck sums the cross products of consecutive control points about a centre, and takes the
/// points' sum for that centre (it divides by the number of wires, not of points): eight times as
/// far from the origin as a rectangle of four lines. The sum is the same about any centre; its
/// rounding is not. It grows with the square of the centre's distance, and on a thin face, whose
/// normal is short, turns the normal far enough for truck's own flatness test to fail: truck
/// finds a 0.00013 x 30 mm face 360 mm from the origin 2e-6 mm out of the plane it fitted. With
/// the centre at the origin the same face is 1e-14 mm out at most.
fn plane_about_the_mean(wires: &[mt::Wire]) -> Option<mt::Plane> {
    let (mut sum, mut count) = (Vec3::ZERO, 0usize);
    for p in wires.iter().flat_map(|w| w.edge_iter()).flat_map(|e| e.oriented_curve().lift_up().control_points().clone()) {
        sum += Vec3::new(p.x / p.w, p.y / p.w, p.z / p.w);
        count += 1;
    }
    let mean = sum * (1.0 / count.max(1) as f64);
    if !mean.is_finite() {
        return None;
    }
    let moved: Vec<mt::Wire> = wires.iter().map(|w| builder::transformed(w, mt::Matrix4::from_translation(v3(-mean)))).collect();
    match builder::try_attach_plane(&moved).ok()?.surface() {
        mt::Surface::Plane(plane) => Some(mt::Transformed::transformed(&plane, mt::Matrix4::from_translation(v3(mean)))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> FaceSpec {
        FaceSpec {
            loops: vec![vec![EdgeSpec::Line { a, b }, EdgeSpec::Line { a: b, b: c }, EdgeSpec::Line { a: c, b: d }, EdgeSpec::Line { a: d, b: a }]],
            surface: SurfSpec::Plane,
            probe: None,
        }
    }

    fn box_faces(origin: Vec3, size: Vec3) -> Vec<FaceSpec> {
        let p = |x: f64, y: f64, z: f64| origin + Vec3::new(x * size.x, y * size.y, z * size.z);
        let (a, b, c, d) = (p(0., 0., 0.), p(1., 0., 0.), p(1., 1., 0.), p(0., 1., 0.));
        let (e, f, g, h) = (p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.));
        vec![quad(a, d, c, b), quad(e, f, g, h), quad(a, b, f, e), quad(b, c, g, f), quad(c, d, h, g), quad(d, a, e, h)]
    }

    #[test]
    fn disconnected_closed_faces_keep_every_material_component() {
        let mut faces = box_faces(Vec3::ZERO, Vec3::new(2.0, 3.0, 4.0));
        faces.extend(box_faces(Vec3::X * 10.0, Vec3::new(2.0, 3.0, 4.0)));
        for reverse in [false, true] {
            if reverse {
                faces.reverse();
            }
            let body = sew(&faces).unwrap();
            assert_eq!(body.shell_count(), 2);
            assert!(body.validity().is_empty());
            assert!((crate::measure(&body).unwrap().volume - 48.0).abs() < 1e-9);
            let lumps = body.lumps().unwrap();
            assert_eq!(lumps.len(), 2);
            assert!(lumps.iter().all(|b| (crate::measure(b).unwrap().volume - 24.0).abs() < 1e-9));
            let mesh = body.tessellate(0.01).unwrap();
            assert!(mesh.contains(Vec3::new(1.0, 1.0, 1.0)));
            assert!(mesh.contains(Vec3::new(11.0, 1.0, 1.0)));
            assert!(!mesh.contains(Vec3::new(5.0, 1.0, 1.0)));
            let imported = crate::step_import(&crate::step_export(&[&body], "sewn pieces").unwrap()).unwrap();
            assert!(imported.warnings.is_empty());
            let volume: f64 = imported.bodies.iter().map(|b| crate::measure(&b.body).unwrap().volume).sum();
            assert!((volume - 48.0).abs() < 1e-6);
        }
    }

    #[test]
    fn sewing_preserves_inward_cavity_orientation_and_refuses_vertex_contact() {
        let outer = box_faces(Vec3::ZERO, Vec3::new(2.0, 3.0, 4.0));
        let mut inner = box_faces(Vec3::new(0.5, 0.75, 1.0), Vec3::new(1.0, 1.5, 2.0));
        for face in &mut inner {
            for lp in &mut face.loops {
                lp.reverse();
                for edge in lp {
                    if let EdgeSpec::Line { a, b } = edge {
                        std::mem::swap(a, b);
                    }
                }
            }
        }
        let cavity = sew(&outer.iter().chain(&inner).cloned().collect::<Vec<_>>()).unwrap();
        assert_eq!(cavity.shell_count(), 2);
        assert_eq!(cavity.lumps().unwrap().len(), 1);
        assert!((crate::measure(&cavity).unwrap().volume - 21.0).abs() < 1e-9);
        assert!(!cavity.tessellate(0.01).unwrap().contains(Vec3::new(1.0, 1.5, 2.0)));
        let touching = box_faces(Vec3::new(2.0, 3.0, 4.0), Vec3::new(2.0, 3.0, 4.0));
        assert!(sew(&outer.iter().chain(&touching).cloned().collect::<Vec<_>>()).unwrap_err().to_string().contains("vertex"));
        let mut hostile = outer.clone();
        hostile[0].loops = vec![Vec::new(); 500_001];
        assert!(sew(&hostile).unwrap_err().to_string().contains("budget"));
    }

    #[test]
    fn flat_arcs_far_from_the_origin_keep_their_ends_and_their_faces_planes() {
        // A 12 x 30 x 2 slab 1000 mm out whose y = 0 side bows 0.00008 mm (a 225 m radius).
        for (o, bow) in [(Vec3::new(1000.0, 500.0, 0.0), 8e-5), (Vec3::new(-1238.4, 4.0, 14.0), 8e-5), (Vec3::ZERO, 0.5)] {
            let p = |x: f64, y: f64, z: f64| o + Vec3::new(x, y, z);
            let (a, b, c, d) = (p(0., 0., 0.), p(12., 0., 0.), p(12., 30., 0.), p(0., 30., 0.));
            let up = Vec3::new(0., 0., 2.);
            let (e, f, g, h) = (a + up, b + up, c + up, d + up);
            let (m0, m1) = (p(6., -bow, 0.), p(6., -bow, 2.));
            let line = |a, b| EdgeSpec::Line { a, b };
            let plane = |l: Vec<EdgeSpec>| FaceSpec { loops: vec![l], surface: SurfSpec::Plane, probe: None };
            let wall = FaceSpec {
                loops: vec![vec![EdgeSpec::Arc { a, b, mid: m0 }, line(b, f), EdgeSpec::Arc { a: f, b: e, mid: m1 }, line(e, a)]],
                surface: SurfSpec::Extruded { a, b, mid: m0, dir: up },
                probe: Some((p(6., -bow, 1.), Vec3::new(0., -1., 0.))),
            };
            let faces = vec![
                plane(vec![line(a, d), line(d, c), line(c, b), EdgeSpec::Arc { a: b, b: a, mid: m0 }]),
                plane(vec![EdgeSpec::Arc { a: e, b: f, mid: m1 }, line(f, g), line(g, h), line(h, e)]),
                wall,
                quad(b, c, g, f),
                quad(c, d, h, g),
                quad(d, a, e, h),
            ];
            let body = sew(&faces).unwrap_or_else(|e| panic!("{o:?} {bow}: {e}"));
            assert!(body.validity().is_empty(), "{o:?}");
            // The circular segment added to the slab (the measure follows flat arcs by chords).
            let r = (36.0 + bow * bow) / (2.0 * bow);
            let half = (6.0 / r).asin();
            let segment = r * r * (half - half.sin() * half.cos());
            let want = (360.0 + segment) * 2.0;
            let got = crate::measure(&body).unwrap().volume;
            assert!((got - want).abs() < 1e-4 * want, "{o:?} {bow}: {got} vs {want}");
        }
    }

    #[test]
    fn a_sewn_box() {
        let p = |x: f64, y: f64, z: f64| Vec3::new(x, y, z);
        let (a, b, c, d) = (p(0., 0., 0.), p(2., 0., 0.), p(2., 3., 0.), p(0., 3., 0.));
        let (e, f, g, h) = (p(0., 0., 4.), p(2., 0., 4.), p(2., 3., 4.), p(0., 3., 4.));
        let faces = vec![quad(a, d, c, b), quad(e, f, g, h), quad(a, b, f, e), quad(b, c, g, f), quad(c, d, h, g), quad(d, a, e, h)];
        let body = sew(&faces).unwrap();
        let m = crate::measure(&body).unwrap();
        assert!((m.volume - 24.0).abs() < 1e-9, "{}", m.volume);
        assert_eq!(body.face_count(), 6);
        assert!(sew(&faces[..5]).is_err());
    }

    /// A 4 × 3 × 2 box whose +x wall bulges out as the cubic (4,0) (5,0) (5,3) (4,3), given as a
    /// chain of its two halves at the bottom (`z = 0`) and the top, the wall ruled between them.
    fn bulged_box(bottom: EdgeSpec, top: EdgeSpec) -> Vec<FaceSpec> {
        let p = |x: f64, y: f64, z: f64| Vec3::new(x, y, z);
        let (a, b, c, d) = (p(0., 0., 0.), p(4., 0., 0.), p(4., 3., 0.), p(0., 3., 0.));
        let (e, f, g, h) = (p(0., 0., 2.), p(4., 0., 2.), p(4., 3., 2.), p(0., 3., 2.));
        let line = |a: Vec3, b: Vec3| EdgeSpec::Line { a, b };
        let flat = |edges: Vec<EdgeSpec>| FaceSpec { loops: vec![edges], surface: SurfSpec::Plane, probe: None };
        vec![
            flat(vec![line(a, d), line(d, c), bottom.reversed(), line(b, a)]),
            flat(vec![line(e, f), top.clone(), line(g, h), line(h, e)]),
            quad(a, b, f, e),
            quad(c, d, h, g),
            quad(d, a, e, h),
            FaceSpec {
                loops: vec![vec![bottom.clone(), line(c, g), top.reversed(), line(f, b)]],
                surface: SurfSpec::Ruled { a: bottom, b: top },
                probe: Some((p(4.75, 1.5, 1.0), Vec3::X)),
            },
        ]
    }

    /// The bulge's cubic split at its middle (de Casteljau), at height `z`.
    fn bulge(z: f64) -> EdgeSpec {
        let ctrl = [(4.0, 0.0), (4.5, 0.0), (4.75, 0.75), (4.75, 1.5), (4.75, 2.25), (4.5, 3.0), (4.0, 3.0)];
        EdgeSpec::Chain { ctrl: ctrl.iter().map(|(x, y)| Vec3::new(*x, *y, z)).collect() }
    }

    #[test]
    fn a_ruled_wall_between_bezier_chains_closes_the_solid() {
        let body = sew(&bulged_box(bulge(0.0), bulge(2.0))).unwrap();
        assert_eq!(body.face_count(), 6);
        assert!(body.validity().is_empty(), "{:?}", body.validity());
        // x = 4 + 3t(1 − t), y = 3(3t² − 2t³): the bulge adds ∫ (x − 4) dy = 54 ∫ t²(1 − t)² dt
        // = 1.8 to the 4 × 3 section; the solid is that section 2 high.
        let m = crate::measure(&body).unwrap();
        assert!((m.volume - 27.6).abs() < 2e-3, "{}", m.volume);
        assert!((m.bbox.max.x - 4.75).abs() < 1e-3, "{:?}", m.bbox);
    }

    #[test]
    fn hostile_bezier_chains_are_refused() {
        let chain = |ctrl: Vec<Vec3>| EdgeSpec::Chain { ctrl: ctrl.into() };
        let EdgeSpec::Chain { ctrl } = bulge(2.0) else { panic!() };
        // Not 3n + 1 control points; a control point that is not a number; too many pieces.
        let mut nan = ctrl.to_vec();
        nan[2].x = f64::NAN;
        let long: Vec<Vec3> = (0..MAX_CHAIN_POINTS + 3).map(|i| Vec3::new(4.0, 3.0 * i as f64 / (MAX_CHAIN_POINTS + 2) as f64, 2.0)).collect();
        for top in [chain(ctrl[..6].to_vec()), chain(ctrl[..3].to_vec()), chain(Vec::new()), chain(nan), chain(long)] {
            assert!(sew(&bulged_box(bulge(0.0), top)).is_err());
        }
        // A ruled wall needs chains with the same number of pieces.
        let whole = chain(vec![Vec3::new(4.0, 0.0, 2.0), Vec3::new(5.0, 0.0, 2.0), Vec3::new(5.0, 3.0, 2.0), Vec3::new(4.0, 3.0, 2.0)]);
        let e = sew(&bulged_box(bulge(0.0), whole)).unwrap_err().to_string();
        assert!(e.contains("ruled chains differ"), "{e}");
    }

    #[test]
    fn thin_faces_away_from_the_origin_get_their_planes() {
        // Slabs 30 x 2 mm and 0.00013 to 0.1 mm thick, away from the origin and turned off the
        // axes. truck's plane fit sums the cross products of a boundary's points about a centre
        // and takes the points' sum for it, eight times as far from the origin as a rectangle;
        // on the thin faces its normal then comes out so far off that truck finds a
        // 0.00013 x 30 mm face at (-300, -200, 5) 2e-6 mm out of its own plane and refuses it:
        // `cannot attach a plane to a wire that is not on one plane`. Nine of these 45 slabs
        // have such a face. (Thinner slabs meet another limit, at the origin too: a face
        // 0.00003 mm wide and askew to the axes is sewn but gets no triangles.)
        let turned = |about_z: f64, about_x: f64| {
            let (sz, cz) = about_z.sin_cos();
            let (sx, cx) = about_x.sin_cos();
            // The x, y and z axes turned about x, then about z.
            (Vec3::new(cz, sz, 0.0), Vec3::new(-sz * cx, cz * cx, sx), Vec3::new(sz * sx, -cz * sx, cx))
        };
        // A box on the corner `o` with the edges `u`, `v` and `w` (right-handed).
        let skew_box = |o: Vec3, u: Vec3, v: Vec3, w: Vec3| {
            let p = |x: f64, y: f64, z: f64| o + u * x + v * y + w * z;
            let (a, b, c, d) = (p(0., 0., 0.), p(1., 0., 0.), p(1., 1., 0.), p(0., 1., 0.));
            let (e, f, g, h) = (p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.));
            vec![quad(a, d, c, b), quad(e, f, g, h), quad(a, b, f, e), quad(b, c, g, f), quad(c, d, h, g), quad(d, a, e, h)]
        };
        for o in [Vec3::new(-300.0, -200.0, 5.0), Vec3::new(2500.0, -1800.0, 40.0), Vec3::ZERO] {
            for width in [1.3e-4, 1e-3, 0.1] {
                for (about_z, about_x) in [(0.0, 0.0), (0.0, 0.45), (0.9, 0.0), (-0.65, 0.45), (2.2, 1.1)] {
                    let tag = format!("{o:?}, {width} wide, turned {about_z} and {about_x}");
                    let (u, v, w) = turned(about_z, about_x);
                    let body = sew(&skew_box(o, u * width, v * 2.0, w * 30.0)).unwrap_or_else(|e| panic!("{tag}: {e}"));
                    assert!(body.validity().is_empty() && body.shell_count() == 1 && body.face_count() == 6, "{tag}: {:?}", body.validity());
                    // (The measure sums about the origin: it is good to about 1e-14 of the
                    // distance cubed, which three metres out is more than the thinnest slabs hold.)
                    let (got, want) = (crate::measure(&body).unwrap().volume, width * 60.0);
                    assert!((got - want).abs() < 1e-5 * want + 1e-14 * o.len().powi(3) + 1e-9, "{tag}: {got} vs {want}");
                }
            }
        }
        // A boundary that leaves its plane is refused as before: one corner of a face lifted by
        // a thousandth of a millimetre, on a cube and on a sliver.
        for size in [Vec3::new(10.0, 10.0, 10.0), Vec3::new(10.0, 2e-4, 10.0)] {
            let mut warped = box_faces(Vec3::new(500.0, 300.0, 0.0), size);
            if let EdgeSpec::Line { b, .. } = &mut warped[1].loops[0][1] {
                b.z += 1e-3;
            }
            if let EdgeSpec::Line { a, .. } = &mut warped[1].loops[0][2] {
                a.z += 1e-3;
            }
            let e = sew(&warped).unwrap_err().to_string();
            assert!(e.contains("face 1") && e.contains("not on one plane"), "{e}");
        }
        // So is one with no area.
        let mut flat = box_faces(Vec3::ZERO, Vec3::new(10.0, 10.0, 10.0));
        let p = Vec3::new(0.0, 0.0, 10.0);
        flat[1] = quad(p, p + Vec3::X * 3.0, p + Vec3::X * 7.0, p + Vec3::X * 10.0);
        assert!(sew(&flat).unwrap_err().to_string().contains("not on one plane"));
    }
}
