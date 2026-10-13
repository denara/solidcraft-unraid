//! Surface bodies (open shells) and the operations that turn them into solids: Patch (a face
//! filling a closed loop of edges — planar, or B-spline patches for a loop that is not — or a
//! planar sketch region), Stitch (surfaces sharing edges sewn
//! into one body, a solid when they close), Trim (a surface cut by a tool, one side kept),
//! Thicken (a surface offset along its normals into a solid) and copying faces out of a body.

use std::collections::HashMap;

use mt::{BoundedCurve, InnerSpace, MetricSpace, ParametricCurve, ParametricSurface3D, SearchNearestParameter};
use solidcraft_geom::{Plane, Region2, Vec3};
use truck_modeling as mt;

use crate::body::{Body, from_p3, p3};
use crate::history::{Change, EdgeOrigin, FaceOrigin};
use crate::record::{Rebuild, rebuilt_body};
use crate::{KernelError, Result, guard};

impl Body {
    /// A surface body: some shell does not close.
    pub fn is_surface(&self) -> bool {
        !self.is_mesh() && self.solid.boundaries().iter().any(|s| s.shell_condition() != mt::ShellCondition::Closed)
    }
}

/// A body of faces as they are (no orientation fix: a surface has no inside).
pub(crate) fn surface_body(faces: Vec<mt::Face>) -> Body {
    surface_of(mt::Shell::from(faces))
}

/// A body of a shell as it is.
fn surface_of(shell: mt::Shell) -> Body {
    Body {
        solid: std::sync::Arc::new(mt::Solid::new_unchecked(vec![shell])),
        size: 0.0,
        volume: Some(std::sync::Arc::new(std::sync::Mutex::new(None))),
        coarse_missing: None,
        mesh: None,
        color: None,
        paint: None,
        splits: None,
        face_sources: None,
        history: None,
        regularization_protected: false,
    }
}

/// A body from faces: a solid when they close, else a surface; with the history of the
/// operation that made the faces from `operands` as `rec` records.
fn body_of(faces: Vec<mt::Face>, operands: &[&mt::Solid], rec: &Rebuild) -> Result<Body> {
    let shell = mt::Shell::from(faces);
    if shell.shell_condition() == mt::ShellCondition::Closed {
        crate::record::rebuilt(mt::Solid::new_unchecked(vec![shell]), operands, rec)
    } else {
        Ok(rebuilt_body(surface_of(shell), operands, rec))
    }
}

/// A planar surface filling a sketch region. Its history has no operands: the face is new.
pub fn patch_region(plane: &Plane, region: &Region2) -> Result<Body> {
    let f = crate::build::face(plane, region)?;
    let mut rec = Rebuild::default();
    rec.made(&f, Vec::new());
    Ok(rebuilt_body(surface_body(vec![f]), &[], &rec))
}

/// The edge of a body nearest a point (by samples along each edge).
pub(crate) fn edge_near(body: &Body, p: Vec3) -> Option<mt::Edge> {
    let mut best: Option<(f64, mt::Edge)> = None;
    for e in body.solid.edge_iter() {
        let c = e.curve();
        let (t0, t1) = c.range_tuple();
        for k in 0..=32 {
            let d = from_p3(c.subs(t0 + (t1 - t0) * k as f64 / 32.0)).dist(p);
            if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                best = Some((d, e.clone()));
            }
        }
    }
    best.filter(|(d, _)| *d <= body.size() * 0.05 + 1e-6).map(|(_, e)| e)
}

/// A surface filling a closed loop of a body's edges (each picked by a point on it): one face on
/// the body's own edges, planar when the loop is certainly planar ([`crate::plane_fit`]), else a
/// B-spline height field over the loop's plane ([`crate::fill`]; loops that do not bound one are
/// refused). Its history is over `[body]`: the face is new, its edges the body's own.
pub fn patch_edges(body: &Body, at: &[Vec3]) -> Result<Body> {
    body.require_brep("patch")?;
    if at.is_empty() || at.len() > 10_000 {
        return Err(KernelError::Invalid("pick the edges of a closed loop".into()));
    }
    guard("patch", || {
        let chain = picked_loop(body, at)?;
        let Some(face) = crate::plane_fit::planar_face(&chain, crate::fill::planarity_tolerance(body))? else {
            // Not certainly planar: a fitted surface.
            return crate::fill::fill_loop(body, &chain);
        };
        // A new face on the body's own edges.
        let mut rec = Rebuild::default();
        rec.made(&face, Vec::new());
        Ok(rebuilt_body(surface_body(vec![face]), &[&body.solid], &rec))
    })
}

/// The edges of `body` picked by points on them, chained into one closed loop (each turned to
/// run along it). Call it under [`guard`]: sampling an intersection curve can fail in truck.
pub(crate) fn picked_loop(body: &Body, at: &[Vec3]) -> Result<Vec<mt::Edge>> {
    let mut edges: Vec<mt::Edge> = Vec::new();
    for p in at {
        let e = edge_near(body, *p).ok_or_else(|| KernelError::Invalid(format!("no edge at {p:?}")))?;
        if !edges.iter().any(|x| x.id() == e.id()) {
            edges.push(e);
        }
    }
    let mut pool = edges;
    let first = pool.pop().ok_or_else(|| KernelError::Invalid("no edges".into()))?;
    let start = first.front().id();
    let mut at_v = first.back().clone();
    let mut chain = vec![first];
    while at_v.id() != start {
        let k = pool
            .iter()
            .position(|e| e.front().id() == at_v.id() || e.back().id() == at_v.id())
            .ok_or_else(|| KernelError::Invalid("the edges do not form a closed loop".into()))?;
        let e = pool.remove(k);
        let e = if e.front().id() == at_v.id() { e } else { e.inverse() };
        at_v = e.back().clone();
        chain.push(e);
    }
    if !pool.is_empty() {
        return Err(KernelError::Invalid("the edges form more than one loop".into()));
    }
    Ok(chain)
}

/// Faces of a body copied out as a surface body (faces picked by points on them). Its history is
/// over `[body]`: the faces and their edges are the body's.
pub fn copy_faces(body: &Body, at: &[Vec3]) -> Result<Body> {
    body.require_brep("copy faces")?;
    let faces: Vec<&mt::Face> = body.solid.face_iter().collect();
    let mut chosen = Vec::new();
    for p in at {
        // The face whose mesh passes nearest the point.
        let i = nearest_face(body, *p)?;
        if !chosen.contains(&i) {
            chosen.push(i);
        }
    }
    let copied = surface_body(chosen.into_iter().filter_map(|i| faces.get(i).map(|f| (*f).clone())).collect());
    Ok(rebuilt_body(copied, &[&body.solid], &Rebuild::default()))
}

/// Index of the face nearest a point (over its mesh triangles).
fn nearest_face(body: &Body, p: Vec3) -> Result<usize> {
    let m = body.tessellate(body.size() * 2e-3)?;
    let mut best: Option<(f64, u32)> = None;
    for (t, f) in m.triangles.iter().zip(&m.tri_face) {
        let Some([a, b, c]) = m.tri(t) else { continue };
        let d = ((a + b + c) / 3.0).dist(p).min(a.dist(p)).min(b.dist(p)).min(c.dist(p));
        if best.is_none_or(|(x, _)| d < x) {
            best = Some((d, *f));
        }
    }
    best.map(|(_, f)| f as usize).ok_or_else(|| KernelError::Invalid("no face there".into()))
}

/// Sew surface bodies along edges that coincide (within `tol`): one body, a solid when the
/// faces close up. Faces are turned so that neighbours agree. The result's history is over
/// `bodies`: each face the one it was sewn from, each edge the edges sewn into it.
pub fn stitch(bodies: &[&Body], tol: f64) -> Result<Body> {
    if bodies.is_empty() {
        return Err(KernelError::Invalid("nothing to stitch".into()));
    }
    for b in bodies {
        b.require_brep("stitch")?;
    }
    let size = bodies.iter().map(|b| b.size()).fold(0.0, f64::max);
    let tol = if tol.is_finite() && tol > 0.0 { tol } else { size * 1e-6 + 1e-6 };
    guard("stitch", || {
        let faces: Vec<mt::Face> = bodies.iter().flat_map(|b| b.solid.face_iter().cloned().collect::<Vec<_>>()).collect();
        // Shared vertices by position, shared edges by their ends and middle.
        let mut verts: Vec<mt::Vertex> = Vec::new();
        let mut vmap: HashMap<mt::VertexID, mt::Vertex> = HashMap::new();
        let mut canon = |v: &mt::Vertex, verts: &mut Vec<mt::Vertex>| -> mt::Vertex {
            if let Some(c) = vmap.get(&v.id()) {
                return c.clone();
            }
            let p = v.point();
            let c = match verts.iter().find(|u| u.point().distance(p) <= tol) {
                Some(u) => u.clone(),
                None => {
                    verts.push(v.clone());
                    v.clone()
                }
            };
            vmap.insert(v.id(), c.clone());
            c
        };
        let mut edges: Vec<(mt::Edge, mt::Point3)> = Vec::new();
        let mut out = Vec::with_capacity(faces.len());
        let mut rec = Rebuild::default();
        for f in &faces {
            let mut wires = Vec::new();
            for w in f.absolute_boundaries() {
                let mut nw = Vec::new();
                for e in w.edge_iter() {
                    let abs = if e.orientation() { e.clone() } else { e.inverse() };
                    let (a, b) = (canon(abs.front(), &mut verts), canon(abs.back(), &mut verts));
                    let c = abs.curve();
                    let (t0, t1) = c.range_tuple();
                    let mid = c.subs(0.5 * (t0 + t1));
                    let found = edges.iter().find_map(|(x, m)| {
                        if m.distance(mid) > tol * 10.0 {
                            return None;
                        }
                        if x.front().id() == a.id() && x.back().id() == b.id() {
                            Some(x.clone())
                        } else if x.front().id() == b.id() && x.back().id() == a.id() {
                            Some(x.inverse())
                        } else {
                            None
                        }
                    });
                    let ne = match found {
                        Some(x) => x,
                        None => {
                            let x = mt::Edge::new_unchecked(&a, &b, c);
                            edges.push((x.clone(), mid));
                            x
                        }
                    };
                    rec.piece(&ne, &abs);
                    nw.push(if e.orientation() { ne } else { ne.inverse() });
                }
                wires.push(mt::Wire::from(nw));
            }
            let mut nf = mt::Face::new_unchecked(wires, f.surface());
            if !f.orientation() {
                nf.invert();
            }
            rec.whole(&nf, f);
            out.push(nf);
        }
        // Neighbours run their shared edges opposite ways.
        let mut uses: HashMap<mt::EdgeID, Vec<(usize, bool)>> = HashMap::new();
        for (i, f) in out.iter().enumerate() {
            for e in f.boundaries().iter().flat_map(|w| w.edge_iter()) {
                uses.entry(e.id()).or_default().push((i, e.orientation()));
            }
        }
        let mut flip: Vec<Option<bool>> = vec![None; out.len()];
        for s in 0..out.len() {
            if flip[s].is_some() {
                continue;
            }
            flip[s] = Some(false);
            let mut stack = vec![s];
            while let Some(i) = stack.pop() {
                let fl = flip[i].unwrap_or(false);
                let Some(f) = out.get(i) else { continue };
                for e in f.boundaries().iter().flat_map(|w| w.edge_iter()) {
                    let d = e.orientation() != fl;
                    for &(j, dj) in uses.get(&e.id()).into_iter().flatten() {
                        if j != i && flip[j].is_none() {
                            flip[j] = Some(dj == d);
                            stack.push(j);
                        }
                    }
                }
            }
        }
        let out: Vec<mt::Face> = out.into_iter().zip(flip).map(|(f, fl)| if fl == Some(true) { f.inverse() } else { f }).collect();
        let operands: Vec<&mt::Solid> = bodies.iter().map(|b| &*b.solid).collect();
        body_of(out, &operands, &rec)
    })
}

/// How a face moves along its normal: a plane slides, a circular cylinder changes radius.
#[derive(Clone, Copy)]
enum Offset {
    Plane { n: Vec3 },
    Cylinder { o: Vec3, axis: Vec3, r: f64, out: f64 },
}

impl Offset {
    fn of(f: &mt::Face) -> Option<Offset> {
        match crate::splitface::analytic_field(&f.oriented_surface())? {
            crate::splitface::Analytic::Plane { n } => Some(Offset::Plane { n }),
            crate::splitface::Analytic::Cylinder { o, axis, r, out } => Some(Offset::Cylinder { o, axis, r, out }),
        }
    }
    fn map(&self, p: Vec3, d: f64) -> Vec3 {
        match *self {
            Offset::Plane { n } => p + n * d,
            // Scaled about the axis (control points of a NURBS circle stay exact).
            Offset::Cylinder { o, axis, r, out } => {
                let v = p - o;
                let radial = v - axis * v.dot(axis);
                p + radial * (out * d / r)
            }
        }
    }
}

fn map_pt(off: &Offset, p: mt::Point3, d: f64) -> mt::Point3 {
    p3(off.map(from_p3(p), d))
}

/// A curve moved by an offset (control points mapped; exact for lines, and for circles square
/// to a cylinder's axis or in a plane).
fn map_curve(off: &Offset, c: &mt::Curve, d: f64) -> Result<mt::Curve> {
    Ok(match c {
        mt::Curve::Line(l) => mt::Curve::Line(mt::Line(map_pt(off, l.0, d), map_pt(off, l.1, d))),
        mt::Curve::BSplineCurve(b) => {
            let pts: Vec<mt::Point3> = b.control_points().iter().map(|p| map_pt(off, *p, d)).collect();
            mt::Curve::BSplineCurve(mt::BSplineCurve::new(b.knot_vec().clone(), pts))
        }
        mt::Curve::NurbsCurve(n) => {
            let nb = n.non_rationalized();
            let pts: Vec<mt::Vector4> = nb
                .control_points()
                .iter()
                .map(|h| {
                    let w = h.w;
                    let q = map_pt(off, mt::Point3::new(h.x / w, h.y / w, h.z / w), d);
                    mt::Vector4::new(q.x * w, q.y * w, q.z * w, w)
                })
                .collect();
            mt::Curve::NurbsCurve(mt::NurbsCurve::new(mt::BSplineCurve::new(nb.knot_vec().clone(), pts)))
        }
        other => {
            // Anything else: a cubic through moved samples.
            let (t0, t1) = other.range_tuple();
            let pts: Vec<Vec3> = (0..=32).map(|k| off.map(from_p3(other.subs(t0 + (t1 - t0) * k as f64 / 32.0)), d)).collect();
            mt::Curve::BSplineCurve(crate::build::interpolate_cubic(&pts).ok_or_else(|| KernelError::Failed("offset edge".into()))?)
        }
    })
}

/// A surface moved by an offset (control points mapped; exact for planes and NURBS cylinders).
fn map_surface(off: &Offset, s: &mt::Surface, d: f64) -> Result<mt::Surface> {
    Ok(match s {
        mt::Surface::Plane(pl) => {
            let (o, a, b) = (pl.origin(), pl.u_axis(), pl.v_axis());
            let o2 = map_pt(off, o, d);
            mt::Surface::Plane(mt::Plane::new(o2, o2 + a, o2 + b))
        }
        mt::Surface::BSplineSurface(b) => {
            let rows: Vec<Vec<mt::Point3>> = b.control_points().iter().map(|r| r.iter().map(|p| map_pt(off, *p, d)).collect()).collect();
            mt::Surface::BSplineSurface(mt::BSplineSurface::new((b.uknot_vec().clone(), b.vknot_vec().clone()), rows))
        }
        mt::Surface::NurbsSurface(n) => {
            let nb = n.non_rationalized();
            let rows: Vec<Vec<mt::Vector4>> = nb
                .control_points()
                .iter()
                .map(|r| {
                    r.iter()
                        .map(|h| {
                            let w = h.w;
                            let q = map_pt(off, mt::Point3::new(h.x / w, h.y / w, h.z / w), d);
                            mt::Vector4::new(q.x * w, q.y * w, q.z * w, w)
                        })
                        .collect()
                })
                .collect();
            mt::Surface::NurbsSurface(mt::NurbsSurface::new(mt::BSplineSurface::new((nb.uknot_vec().clone(), nb.vknot_vec().clone()), rows)))
        }
        _ => return Err(KernelError::Invalid("not supported yet: thickening this kind of surface".into())),
    })
}

/// A surface body thickened into a solid: offset along the faces' normals by `thickness`
/// (negative: against them), or by half each way when `symmetric`. Planar faces and faces on
/// circular cylinders; neighbouring faces must meet smoothly (their offsets agree).
pub fn thicken(body: &Body, thickness: f64, symmetric: bool) -> Result<Body> {
    body.require_brep("thicken")?;
    if !(thickness.is_finite() && thickness.abs() > 1e-9) {
        return Err(KernelError::Invalid("thickness must be non-zero".into()));
    }
    let (d0, d1) = if symmetric { (-thickness / 2.0, thickness / 2.0) } else { (0.0, thickness) };
    let (lo, hi) = (d0.min(d1), d0.max(d1));
    let size = body.size();
    let tol = size * 1e-7 + 1e-9;
    guard("thicken", || {
        let faces: Vec<mt::Face> = body.solid.face_iter().cloned().collect();
        if faces.is_empty() {
            return Err(KernelError::Invalid("nothing to thicken".into()));
        }
        let offs: Vec<Offset> = faces
            .iter()
            .map(|f| {
                Offset::of(f).ok_or_else(|| KernelError::Invalid("not supported yet: thickening faces that are not planar or cylindrical".into()))
            })
            .collect::<Result<_>>()?;
        // Each vertex and edge moved to both sides (once; every face using it must agree).
        let mut vmoved: HashMap<mt::VertexID, (mt::Vertex, mt::Vertex)> = HashMap::new();
        let mut emoved: HashMap<mt::EdgeID, (mt::Edge, mt::Edge)> = HashMap::new();
        let mut uses: HashMap<mt::EdgeID, usize> = HashMap::new();
        for (f, off) in faces.iter().zip(&offs) {
            for e in f.absolute_boundaries().iter().flat_map(|w| w.edge_iter()) {
                let abs = if e.orientation() { e.clone() } else { e.inverse() };
                *uses.entry(abs.id()).or_default() += 1;
                for v in [abs.front(), abs.back()] {
                    let (a, b) = (map_pt(off, v.point(), lo), map_pt(off, v.point(), hi));
                    match vmoved.get(&v.id()) {
                        Some((va, vb)) => {
                            if va.point().distance(a) > tol * 1e3 || vb.point().distance(b) > tol * 1e3 {
                                return Err(KernelError::Invalid("not supported yet: thickening across a sharp edge between faces".into()));
                            }
                        }
                        None => {
                            vmoved.insert(v.id(), (mt::Vertex::new(a), mt::Vertex::new(b)));
                        }
                    }
                }
                if let std::collections::hash_map::Entry::Vacant(slot) = emoved.entry(abs.id()) {
                    let (Some((a0, a1)), Some((b0, b1))) = (vmoved.get(&abs.front().id()).cloned(), vmoved.get(&abs.back().id()).cloned()) else {
                        continue;
                    };
                    let c = abs.curve();
                    let lo_e = mt::Edge::new_unchecked(&a0, &b0, map_curve(off, &c, lo)?);
                    let hi_e = mt::Edge::new_unchecked(&a1, &b1, map_curve(off, &c, hi)?);
                    slot.insert((lo_e, hi_e));
                }
            }
        }
        // Lines joining each vertex's two copies.
        let mut sides: HashMap<mt::VertexID, mt::Edge> = HashMap::new();
        for (id, (a, b)) in &vmoved {
            sides.insert(*id, mt::Edge::new_unchecked(a, b, mt::Curve::Line(mt::Line(a.point(), b.point()))));
        }
        let moved_face = |f: &mt::Face, off: &Offset, which: usize, d: f64| -> Result<mt::Face> {
            let wires: Vec<mt::Wire> = f
                .absolute_boundaries()
                .iter()
                .map(|w| {
                    w.edge_iter()
                        .filter_map(|e| {
                            let abs_id = if e.orientation() { e.id() } else { e.inverse().id() };
                            let (a, b) = emoved.get(&abs_id)?;
                            let m = if which == 0 { a.clone() } else { b.clone() };
                            Some(if e.orientation() { m } else { m.inverse() })
                        })
                        .collect::<Vec<_>>()
                        .into()
                })
                .collect();
            let mut nf = mt::Face::new_unchecked(wires, map_surface(off, &f.surface(), d)?);
            if !f.orientation() {
                nf.invert();
            }
            Ok(nf)
        };
        let mut out = Vec::new();
        // History (one operand, the surface): a copy at no distance is the face itself, a copy
        // further out its offset, a wall is made along its free edge.
        let operand = crate::record::Operand::new(body);
        let edge_index = &operand.edges;
        let mut history = Vec::new();
        let copy = |i: usize, d: f64| {
            if d == 0.0 {
                crate::record::from_face(0, i, Change::Kept)
            } else {
                crate::record::generated(vec![FaceOrigin::Offset { operand: 0, face: i }])
            }
        };
        for (i, (f, off)) in faces.iter().zip(&offs).enumerate() {
            // The lower copy faces back, the upper one forward.
            out.push(moved_face(f, off, 0, lo)?.inverse());
            out.push(moved_face(f, off, 1, hi)?);
            history.extend([copy(i, lo), copy(i, hi)]);
            // Walls along free edges, outward across the edge.
            for w in f.boundaries() {
                for e in w.edge_iter() {
                    let abs_id = if e.orientation() { e.id() } else { e.inverse().id() };
                    if uses.get(&abs_id).copied().unwrap_or(0) != 1 {
                        continue;
                    }
                    let Some((le, he)) = emoved.get(&abs_id) else { continue };
                    let (le, he) = if e.orientation() { (le.clone(), he.clone()) } else { (le.inverse(), he.inverse()) };
                    let (Some(sa), Some(sb)) = (sides.get(&e.front().id()), sides.get(&e.back().id())) else { continue };
                    // A → B low, B → B' up, B' → A' high back, A' → A down.
                    let wire: mt::Wire = vec![le.clone(), sb.clone(), he.inverse(), sa.inverse()].into();
                    let ruled = mt::builder::homotopy(&le, &he);
                    let surf = ruled.surface();
                    // The wall's outward normal: across the edge, away from the face.
                    let c = le.oriented_curve();
                    let (t0, t1) = c.range_tuple();
                    let tm = 0.5 * (t0 + t1);
                    let p = c.subs(tm);
                    let tangent = c.der(tm);
                    let n =
                        f.oriented_surface().search_nearest_parameter(p, None, 50).map(|(u, v)| f.oriented_surface().normal(u, v)).unwrap_or(tangent);
                    let outward = tangent.cross(n);
                    let sn = surf.search_nearest_parameter(p, None, 50).map(|(u, v)| surf.normal(u, v)).unwrap_or(outward);
                    let wall = if sn.dot(outward) >= 0.0 {
                        mt::Face::new_unchecked(vec![wire], surf)
                    } else {
                        let mut x = mt::Face::new_unchecked(vec![wire.inverse()], surf);
                        x.invert();
                        x
                    };
                    out.push(wall);
                    let edge = edge_index.get(&abs_id).copied().ok_or_else(|| KernelError::Failed("thicken: an edge outside the surface".into()))?;
                    history.push(crate::record::generated(vec![FaceOrigin::Edge { operand: 0, edge }]));
                }
            }
        }
        let shell = mt::Shell::from(out);
        if shell.shell_condition() != mt::ShellCondition::Closed {
            return Err(KernelError::Failed("the thickened surface does not close".into()));
        }
        // The edges copied at no distance are the surface's own edges, rebuilt.
        let mut copies = HashMap::new();
        for (id, (low, high)) in &emoved {
            for (copy, d) in [(low, lo), (high, hi)] {
                if let Some(&edge) = edge_index.get(id)
                    && d == 0.0
                {
                    copies.insert(copy.id(), EdgeOrigin::Edge { operand: 0, edge });
                }
            }
        }
        crate::record::attach(Body::new(mt::Solid::new_unchecked(vec![shell]))?, &[operand], Some(history), copies)
    })
}

/// A surface body trimmed by a tool: its faces are cut where the tool meets them and the
/// pieces on the side of `keep` are kept. The result's history is over `[body]`.
pub fn trim(body: &Body, tool: &crate::SplitTool, keep: Vec3) -> Result<Body> {
    body.require_brep("trim")?;
    let total = body.solid.face_iter().count();
    let all: Vec<usize> = (0..total).collect();
    let (split, _) = crate::splitface::split_any(body, &all, tool)?;
    let side = crate::splitface::side_of_point(tool, keep)?;
    guard("trim", || {
        let mut kept = Vec::new();
        for f in split.solid.face_iter() {
            let x = crate::splitface::face_side(f, tool, split.size())?;
            if (x >= 0.0) == (side >= 0.0) {
                kept.push(f.clone());
            }
        }
        if kept.is_empty() {
            return Err(KernelError::Invalid("nothing is left on that side of the tool".into()));
        }
        // The kept pieces are the split's own; its history carries them back to `body`.
        let trimmed = rebuilt_body(surface_body(kept), &[&split.solid], &Rebuild::default());
        Ok(crate::record::after(trimmed, &split, &crate::record::counts(&[body])))
    })
}

/// A planar surface extended: the chosen straight edges (picked by points on them) move
/// outward in the face's plane by `distance`, their neighbours lengthened to meet them. The
/// result's history is over `[body]`: the face and each edge the one it was moved from.
pub fn extend(body: &Body, at: &[Vec3], distance: f64) -> Result<Body> {
    body.require_brep("extend")?;
    if !(distance.is_finite() && distance > 0.0) {
        return Err(KernelError::Invalid("the extension distance must be positive".into()));
    }
    let faces: Vec<mt::Face> = body.solid.face_iter().cloned().collect();
    let [face] = faces.as_slice() else { return Err(KernelError::Invalid("not supported yet: extending a surface of more than one face".into())) };
    let Some(crate::splitface::Analytic::Plane { n }) = crate::splitface::analytic_field(&face.oriented_surface()) else {
        return Err(KernelError::Invalid("not supported yet: extending a surface that is not planar".into()));
    };
    let chosen: Vec<mt::EdgeID> = at
        .iter()
        .map(|p| edge_near(body, *p).map(|e| e.id()).ok_or_else(|| KernelError::Invalid(format!("no edge at {p:?}"))))
        .collect::<Result<_>>()?;
    guard("extend", || {
        let bounds = face.boundaries();
        let [outer] = bounds.as_slice() else { return Err(KernelError::Invalid("not supported yet: extending a surface with holes".into())) };
        let edges: Vec<mt::Edge> = outer.edge_iter().cloned().collect();
        let m = edges.len();
        // Each edge as a line (point, direction) moved outward when chosen.
        let mut lines = Vec::with_capacity(m);
        for e in &edges {
            let (a, b) = (from_p3(e.front().point()), from_p3(e.back().point()));
            if !matches!(e.curve(), mt::Curve::Line(_)) {
                return Err(KernelError::Invalid("not supported yet: extending a surface with curved edges".into()));
            }
            let d = (b - a).normalized().ok_or_else(|| KernelError::Invalid("a zero-length edge".into()))?;
            // Outward: right of the edge seen along the normal (the face is on its left).
            let out = d.cross(n);
            let shift = if chosen.contains(&e.id()) || chosen.contains(&e.inverse().id()) { distance } else { 0.0 };
            lines.push((a + out * shift, d));
        }
        // Corners: where consecutive lines meet.
        let meet = |(p, d): (Vec3, Vec3), (q, e): (Vec3, Vec3)| -> Option<Vec3> {
            let w = q - p;
            let c = d.cross(e);
            let cc = c.dot(c);
            if cc < 1e-24 {
                return None;
            }
            Some(p + d * (w.cross(e).dot(c) / cc))
        };
        let mut corners = Vec::with_capacity(m);
        for i in 0..m {
            let (Some(prev), Some(cur)) = (lines.get((i + m - 1) % m), lines.get(i)) else { continue };
            let p = meet(*prev, *cur).unwrap_or(cur.0);
            corners.push(mt::Vertex::new(p3(p)));
        }
        let mut rec = Rebuild::default();
        let wire: mt::Wire = (0..m)
            .filter_map(|i| {
                let (a, b, old) = (corners.get(i)?, corners.get((i + 1) % m)?, edges.get(i)?);
                let e = mt::Edge::new_unchecked(a, b, mt::Curve::Line(mt::Line(a.point(), b.point())));
                rec.piece(&e, old);
                Some(e)
            })
            .collect::<Vec<_>>()
            .into();
        let mut f = mt::builder::try_attach_plane(&[wire]).map_err(|e| KernelError::Failed(format!("extend: {e}")))?;
        if f.oriented_surface().normal(0.0, 0.0).dot(crate::body::v3(n)) < 0.0 {
            f = f.inverse();
        }
        rec.face(&f, face);
        Ok(rebuilt_body(surface_body(vec![f]), &[&body.solid], &rec))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{box_solid, cylinder, measure};
    use solidcraft_geom::{Loop2, Vec2};

    fn rel(a: f64, b: f64) -> f64 {
        (a - b).abs() / b.abs().max(1e-9)
    }

    #[test]
    fn patch_and_thicken_a_square() {
        let r = Region2 {
            outer: Loop2::polygon(&[Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)]),
            holes: vec![],
        };
        let s = patch_region(&Plane::XY, &r).unwrap();
        assert!(s.is_surface());
        let t = thicken(&s, 2.0, false).unwrap();
        assert!(!t.is_surface());
        assert!(rel(measure(&t).unwrap().volume, 200.0) < 1e-9);
        let t = thicken(&s, 2.0, true).unwrap();
        let m = measure(&t).unwrap();
        assert!(rel(m.volume, 200.0) < 1e-9 && (m.bbox.min.z + 1.0).abs() < 1e-9, "{:?}", m.bbox);
    }

    #[test]
    fn thicken_a_cylinder_side_and_a_box_top() {
        let c = cylinder(Vec3::ZERO, Vec3::Z, 10.0, 20.0).unwrap();
        // One half of the side, offset outward by 1: half of a ring.
        let half = copy_faces(&c, &[Vec3::new(7.0, 7.0, 10.0)]).unwrap();
        let v = measure(&thicken(&half, 1.0, false).unwrap()).unwrap().volume;
        let want = std::f64::consts::PI * (121.0 - 100.0) / 2.0 * 20.0;
        assert!(rel(v, want) < 3e-4, "{v} vs {want}"); // (measured on meshes)
        // Both halves (meeting smoothly): the whole ring, inward.
        let d = 10.0 / 2f64.sqrt();
        let side = copy_faces(&c, &[Vec3::new(d, d, 10.0), Vec3::new(-d, -d, 10.0)]).unwrap();
        assert_eq!(side.face_count(), 2);
        let v = measure(&thicken(&side, -1.0, false).unwrap()).unwrap().volume;
        let want = std::f64::consts::PI * (100.0 - 81.0) * 20.0;
        assert!(rel(v, want) < 3e-4, "{v} vs {want}"); // (measured on meshes)
        // Faces meeting at a sharp edge are refused.
        let b = box_solid(Vec3::ZERO, Vec3::new(10.0, 10.0, 10.0)).unwrap();
        let two = copy_faces(&b, &[Vec3::new(5.0, 5.0, 10.0), Vec3::new(5.0, 0.0, 5.0)]).unwrap();
        assert!(thicken(&two, 1.0, false).is_err());
    }

    #[test]
    fn stitch_six_faces_into_a_box_and_patch_a_hole() {
        let b = box_solid(Vec3::ZERO, Vec3::new(10.0, 20.0, 30.0)).unwrap();
        let pts = [
            Vec3::new(5.0, 10.0, 0.0),
            Vec3::new(5.0, 10.0, 30.0),
            Vec3::new(0.0, 10.0, 15.0),
            Vec3::new(10.0, 10.0, 15.0),
            Vec3::new(5.0, 0.0, 15.0),
            Vec3::new(5.0, 20.0, 15.0),
        ];
        let parts: Vec<Body> = pts.iter().map(|p| copy_faces(&b, &[*p]).unwrap()).collect();
        let refs: Vec<&Body> = parts.iter().collect();
        let s = stitch(&refs, 0.0).unwrap();
        assert!(!s.is_surface());
        assert!(rel(measure(&s).unwrap().volume, 6000.0) < 1e-9);
        // Five faces leave an open box; patching the top's edges closes it again.
        let open = stitch(&[refs[0], refs[2], refs[3], refs[4], refs[5]], 0.0).unwrap();
        assert!(open.is_surface());
        let lid =
            patch_edges(&open, &[Vec3::new(5.0, 0.0, 30.0), Vec3::new(10.0, 10.0, 30.0), Vec3::new(5.0, 20.0, 30.0), Vec3::new(0.0, 10.0, 30.0)])
                .unwrap();
        let closed = stitch(&[&open, &lid], 0.0).unwrap();
        assert!(!closed.is_surface());
        assert!(rel(measure(&closed).unwrap().volume, 6000.0) < 1e-9);
    }

    #[test]
    fn extend_a_planar_surface() {
        let r = Region2 {
            outer: Loop2::polygon(&[Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)]),
            holes: vec![],
        };
        let s = patch_region(&Plane::XY, &r).unwrap();
        let e = extend(&s, &[Vec3::new(10.0, 5.0, 0.0)], 5.0).unwrap();
        assert!(rel(measure(&e).unwrap().area, 150.0) < 1e-9);
        let e = extend(&s, &[Vec3::new(10.0, 5.0, 0.0), Vec3::new(5.0, 10.0, 0.0)], 5.0).unwrap();
        assert!(rel(measure(&e).unwrap().area, 225.0) < 1e-9);
        assert!(extend(&s, &[Vec3::new(10.0, 5.0, 0.0)], -1.0).is_err());
    }

    #[test]
    fn trim_keeps_one_side() {
        let r = Region2 {
            outer: Loop2::polygon(&[Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)]),
            holes: vec![],
        };
        let s = patch_region(&Plane::XY, &r).unwrap();
        let pl = Plane::new(Vec3::new(3.0, 0.0, 0.0), Vec3::Y, Vec3::Z).unwrap();
        let t = trim(&s, &crate::SplitTool::Plane(pl), Vec3::new(1.0, 5.0, 0.0)).unwrap();
        assert!(rel(measure(&t).unwrap().area, 30.0) < 1e-9);
        let t = trim(&s, &crate::SplitTool::Plane(pl), Vec3::new(8.0, 5.0, 0.0)).unwrap();
        assert!(rel(measure(&t).unwrap().area, 70.0) < 1e-9);
    }
}
