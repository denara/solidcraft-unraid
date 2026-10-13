//! Edge fillets and chamfers as a local B-rep operation.
//!
//! Supported locally: a straight edge between two planar faces whose two end vertices each
//! touch exactly one more planar face. Fillets require perpendicular end faces; chamfers
//! intersect their offset rails with the end planes, including sloped walls. The end faces get an arc
//! (fillet) or a line (chamfer) at the corner, and a cylindrical (or planar) face is inserted.
//! The result is exact. Edges meeting at an already blended corner, curved edges and other
//! configurations are reported as not supported yet.

use std::collections::HashMap;

use solidcraft_geom::Vec3;
use truck_modeling::{self as mt, builder};

use crate::body::{Body, Solid, from_p3, p3, v3};
use crate::history::{FaceOrigin, History};
use crate::record::{self, Rebuild};
use crate::{KernelError, Result, guard};

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Round,
    Flat,
}

pub(crate) fn unsupported(msg: &str) -> KernelError {
    KernelError::Failed(format!("not supported yet: {msg}"))
}

/// A blend too large for the faces around it.
pub(crate) fn too_large(msg: &str) -> KernelError {
    KernelError::TooLarge(msg.into())
}

pub(crate) fn vtx(v: &mt::Vertex) -> Vec3 {
    from_p3(v.point())
}

/// Outward unit normal of a planar face (from its oriented surface), if planar.
pub(crate) fn plane_normal(f: &mt::Face) -> Option<Vec3> {
    match f.oriented_surface() {
        mt::Surface::Plane(p) => {
            let n = p.normal();
            Vec3::new(n.x, n.y, n.z).normalized()
        }
        _ => None,
    }
}

/// Replace edges in a face's oriented boundary (by id, keeping orientation), and insert extra
/// edges where consecutive edges no longer connect.
pub(crate) fn rebuild_face(f: &mt::Face, subst: &HashMap<mt::EdgeID, mt::Edge>, connectors: &[mt::Edge]) -> Result<mt::Face> {
    rebuild_face_dropping(f, subst, &[], connectors)
}

/// [`rebuild_face`], also leaving out the `dropped` edges (consumed by a blend).
fn rebuild_face_dropping(f: &mt::Face, subst: &HashMap<mt::EdgeID, mt::Edge>, dropped: &[mt::EdgeID], connectors: &[mt::Edge]) -> Result<mt::Face> {
    let mut wires = Vec::new();
    for w in f.absolute_boundaries() {
        let mut edges: Vec<mt::Edge> = Vec::new();
        for e in w.edge_iter().filter(|e| !dropped.contains(&e.id())) {
            let ne = match subst.get(&e.id()) {
                Some(n) => {
                    // `n` was built in the orientation of the stored edge; match this use.
                    let stored_forward = e.front() == e.absolute_front();
                    if stored_forward { n.clone() } else { n.inverse() }
                }
                None => e.clone(),
            };
            edges.push(ne);
        }
        // Close gaps with connector edges.
        let n = edges.len();
        let mut out: Vec<mt::Edge> = Vec::new();
        for i in 0..n {
            let (Some(cur), Some(next)) = (edges.get(i), edges.get((i + 1) % n)) else { continue };
            out.push(cur.clone());
            if cur.back() != next.front() {
                let c = connectors
                    .iter()
                    .find_map(|c| {
                        if c.front() == cur.back() && c.back() == next.front() {
                            Some(c.clone())
                        } else if c.back() == cur.back() && c.front() == next.front() {
                            Some(c.inverse())
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| KernelError::Failed("blend: could not close a face boundary".into()))?;
                out.push(c);
            }
        }
        wires.push(mt::Wire::from(out));
    }
    crate::heal::absolute_face(f, wires).ok_or_else(|| KernelError::Failed("blend: a face could not be rebuilt".into()))
}

/// Blend one edge of `solid`, with what was built from it (see [`Rebuild`]).
fn blend_one(solid: &Solid, edge: &mt::Edge, size: f64, r: f64, shape: Shape) -> Result<(Solid, Rebuild)> {
    if !matches!(edge.curve(), mt::Curve::Line(_)) {
        return Err(unsupported("only straight edges can be blended"));
    }
    let shells = solid.boundaries();
    let (si, shell) = shells
        .iter()
        .enumerate()
        .find(|(_, sh)| sh.edge_iter().any(|e| e.id() == edge.id()))
        .ok_or_else(|| KernelError::Failed("edge not found".into()))?;
    let faces: Vec<&mt::Face> = shell.face_iter().collect();
    let on_edge: Vec<usize> = (0..faces.len()).filter(|i| faces.get(*i).is_some_and(|f| f.edge_iter().any(|e| e.id() == edge.id()))).collect();
    let [i1, i2] = on_edge[..] else { return Err(unsupported("the edge is not between two faces")) };
    let (Some(f1), Some(f2)) = (faces.get(i1), faces.get(i2)) else { return Err(KernelError::Failed("faces".into())) };
    let (Some(n1), Some(n2)) = (plane_normal(f1), plane_normal(f2)) else { return Err(unsupported("both faces next to the edge must be planar")) };
    let (v0, v1) = (edge.absolute_front().clone(), edge.absolute_back().clone());
    let (p0, p1) = (vtx(&v0), vtx(&v1));
    let d = (p1 - p0).normalized().ok_or_else(|| KernelError::Failed("zero-length edge".into()))?;
    // In-face directions perpendicular to the edge, into each face: a face's boundary runs
    // counter-clockwise about its outward normal, so its interior is to the left.
    let into = |f: &mt::Face, n: Vec3| -> Option<Vec3> {
        let e = f.boundary_iters().into_iter().flatten().find(|e| e.id() == edge.id())?;
        let dir = (vtx(e.back()) - vtx(e.front())).normalized()?;
        n.cross(dir).normalized()
    };
    let (Some(t1), Some(t2)) = (into(f1, n1), into(f2, n2)) else { return Err(KernelError::Failed("blend: edge direction".into())) };
    // Convex edge: each face's outward normal points away from the other face; concave: toward.
    let cos_phi = t1.dot(t2).clamp(-1.0, 1.0);
    let phi = cos_phi.acos();
    let convex = n1.dot(t2) < -1e-9 && n2.dot(t1) < -1e-9;
    let concave = n1.dot(t2) > 1e-9 && n2.dot(t1) > 1e-9;
    if !(phi > 1e-3 && phi < std::f64::consts::PI - 1e-3) || !(convex || concave) {
        return Err(unsupported("the faces at the edge are tangent or the edge is ambiguous"));
    }
    // The blend's centre line is inside the material for a convex edge, outside for a concave one.
    let side = if convex { -1.0 } else { 1.0 };
    blend_geometry(solid, si, &faces, (i1, i2), edge, (v0, v1), (p0, p1, d), (n1, t1, t2, phi, side), size, r, shape)
}

/// Distance along each face from the edge to the blend boundary.
fn setback(r: f64, phi: f64, shape: Shape) -> f64 {
    match shape {
        Shape::Round => r / (phi / 2.0).tan(),
        Shape::Flat => r,
    }
}

/// An extended corner edge must stay in the trimmed side face, not just its infinite
/// supporting plane. Reject crossings and contact with any boundary after the old corner,
/// including holes. Curved boundaries are sampled more finely than the clearance tolerance.
pub(crate) fn extension_fits(face: &mt::Face, p: Vec3, q: Vec3, tol: f64) -> bool {
    use mt::{BoundedCurve, ParameterDivision1D};
    // truck requires a curve sampling tolerance of at least 1e-6. Keep the
    // boundary clearance four times larger than the chord approximation error.
    let tol = tol.max(4e-6);
    let (Some(x), Some(n)) = ((q - p).normalized(), plane_normal(face)) else { return false };
    let y = n.cross(x);
    let len = p.dist(q);
    if !len.is_finite() || len <= tol {
        return false;
    }
    let mut inside = false;
    for edge in face.edge_iter() {
        let points = if matches!(edge.curve(), mt::Curve::Line(_)) {
            vec![edge.front().point(), edge.back().point()]
        } else {
            let curve = edge.curve();
            curve.parameter_division(curve.range_tuple(), tol * 0.25).1
        };
        if points.len() < 2 || points.len() > 100_000 {
            return false;
        }
        for pair in points.windows(2) {
            let [a, b] = pair else { return false };
            let a = from_p3(*a) - p;
            let b = from_p3(*b) - p;
            let (ax, ay, bx, by) = (a.dot(x), a.dot(y), b.dot(x), b.dot(y));
            if ![ax, ay, bx, by].iter().all(|v| v.is_finite()) {
                return false;
            }
            // Contact/overlap with the extension, except its original endpoint.
            if (ay.abs() <= tol && by.abs() <= tol && ax.max(bx) > tol && ax.min(bx) <= len + tol)
                || [(ax, ay), (bx, by)].iter().any(|(u, v)| *u > tol && *u <= len + tol && v.abs() <= tol)
            {
                return false;
            }
            if (ay > 0.0) != (by > 0.0) {
                let hit = ax - ay * (bx - ax) / (by - ay);
                if hit > tol && hit <= len + tol {
                    return false;
                }
                // Even-odd containment of q over all loops (outer boundary and holes).
                if hit > len {
                    inside = !inside;
                }
            }
        }
    }
    inside
}

/// Parameters along `x + λ d` where the line meets `surf`, within `reach` of `x` (Newton on
/// (u, v, λ) from several starts; nearest first).
fn line_surface_hits(surf: &mt::Surface, x: Vec3, d: Vec3, reach: f64, tol: f64) -> Vec<f64> {
    use mt::{ParametricSurface, SearchNearestParameter};
    let mut out: Vec<f64> = Vec::new();
    for k in [0i32, 1, -1, 2, -2, 4, -4] {
        let mut l = reach * f64::from(k) / 4.0;
        let Some((mut u, mut v)) = surf.search_nearest_parameter(p3(x + d * l), None, 100) else { continue };
        for _ in 0..60 {
            let f = from_p3(surf.subs(u, v)) - (x + d * l);
            if !f.is_finite() {
                break;
            }
            if f.len() <= tol {
                if l.abs() <= reach && !out.iter().any(|o| (o - l).abs() <= tol * 10.0) {
                    out.push(l);
                }
                break;
            }
            let (su, sv) = (surf.uder(u, v), surf.vder(u, v));
            let m = [[su.x, sv.x, -d.x], [su.y, sv.y, -d.y], [su.z, sv.z, -d.z]];
            let Some([du, dv, dl]) = crate::polyhedron::solve3_pub(m, [-f.x, -f.y, -f.z]) else { break };
            if ![du, dv, dl].iter().all(|z| z.is_finite()) {
                break;
            }
            // Damp long steps: the start may be far from the root.
            let step = (Vec3::new(su.x, su.y, su.z) * du + Vec3::new(sv.x, sv.y, sv.z) * dv).len().max(dl.abs());
            let k = if step > reach { reach / step } else { 1.0 };
            u += du * k;
            v += dv * k;
            l += dl * k;
        }
    }
    out.sort_by(|a, b| a.abs().total_cmp(&b.abs()));
    out
}

/// A circle through the curve of `e` (centre, axis, radius), when it is one.
fn edge_circle(e: &mt::Edge, tol: f64) -> Option<(Vec3, Vec3, f64)> {
    use mt::{BoundedCurve, ParametricCurve};
    let c = e.curve();
    let (lo, hi) = c.range_tuple();
    let at = |f: f64| from_p3(c.subs(lo + (hi - lo) * f));
    let (a, b, m) = (at(0.0), at(1.0), at(0.5));
    let (u, w) = (b - a, m - a);
    let n = u.cross(w);
    let n2 = n.len2();
    if !(n2 > 1e-30) {
        return None;
    }
    // Circumcentre of a, b, m.
    let centre = a + (n.cross(u) * w.len2() + w.cross(n) * u.len2()) / (2.0 * n2);
    let radius = centre.dist(a);
    let axis = n.normalized()?;
    let ok = (1..16).all(|i| {
        let q = at(f64::from(i) / 16.0);
        (q.dist(centre) - radius).abs() <= tol && (q - centre).dot(axis).abs() <= tol
    });
    (ok && radius.is_finite()).then_some((centre, axis, radius))
}

/// Where `q` lies on the edge's curve: its parameter, if within `tol` of the curve and inside
/// the edge's range.
pub(crate) fn on_edge(e: &mt::Edge, q: Vec3, tol: f64) -> Option<f64> {
    use mt::{BoundedCurve, ParametricCurve, SearchNearestParameter};
    let c = e.curve();
    let (lo, hi) = c.range_tuple();
    let hint =
        (0..=32).map(|i| lo + (hi - lo) * f64::from(i) / 32.0).min_by(|x, y| from_p3(c.subs(*x)).dist(q).total_cmp(&from_p3(c.subs(*y)).dist(q)))?;
    let t = c.search_nearest_parameter(p3(q), hint, 100)?;
    let margin = (hi - lo) * 1e-9;
    (t.is_finite() && t >= lo - margin && t <= hi + margin && from_p3(c.subs(t)).dist(q) <= tol).then_some(t.clamp(lo, hi))
}

/// Whether `q` lies on the edge's curve continued past its ends (a line or a circle).
pub(crate) fn on_extension(e: &mt::Edge, q: Vec3, tol: f64) -> bool {
    match e.curve() {
        mt::Curve::Line(_) => {
            let (a, b) = (vtx(e.absolute_front()), vtx(e.absolute_back()));
            (b - a).normalized().is_some_and(|u| (q - a - u * (q - a).dot(u)).len() <= tol)
        }
        _ => edge_circle(e, tol).is_some_and(|(c, n, r)| ((q - c).dot(n)).abs() <= tol && ((q - c - n * (q - c).dot(n)).len() - r).abs() <= tol),
    }
}

/// The edge `e` with its end at `v` moved to `nv`, along its own curve: shortened (any curve),
/// or continued past `v` (a line or a circle). In the stored orientation of `e`.
pub(crate) fn retarget(e: &mt::Edge, v: &mt::Vertex, nv: &mt::Vertex, tol: f64) -> Result<mt::Edge> {
    let (f, bk) = (e.absolute_front(), e.absolute_back());
    if f == bk {
        return Err(unsupported("a closed edge where the blend ends"));
    }
    if matches!(e.curve(), mt::Curve::Line(_)) {
        return Ok(if f == v { builder::line(nv, bk) } else { builder::line(f, nv) });
    }
    use mt::Cut;
    let q = vtx(nv);
    if let Some(t) = on_edge(e, q, tol) {
        use mt::BoundedCurve;
        let mut c = e.curve();
        // The piece kept must not shrink to nothing.
        let (lo, hi) = c.range_tuple();
        let margin = (hi - lo) * 1e-6;
        if (f == v && t >= hi - margin) || (f != v && t <= lo + margin) {
            return Err(too_large("the blend is larger than the neighbouring faces"));
        }
        let rest = c.cut(t);
        let (piece, a, b) = if f == v { (rest, nv, bk) } else { (c, f, nv) };
        return mt::Edge::try_new(a, b, piece).map_err(|err| KernelError::Failed(format!("blend: corner edge: {err}")));
    }
    // Continued past v: the arc from the new end through v to the far end.
    if edge_circle(e, tol).is_some() && on_extension(e, q, tol) {
        let through = p3(vtx(v));
        return Ok(if f == v { builder::circle_arc(nv, bk, through) } else { builder::circle_arc(f, nv, through) });
    }
    Err(too_large("the blend is larger than the neighbouring faces"))
}

#[allow(clippy::too_many_arguments)]
fn blend_geometry(
    solid: &Solid,
    si: usize,
    faces: &[&mt::Face],
    (i1, i2): (usize, usize),
    edge: &mt::Edge,
    (v0, v1): (mt::Vertex, mt::Vertex),
    (p0, p1, d): (Vec3, Vec3, Vec3),
    (n1, t1, t2, phi, side): (Vec3, Vec3, Vec3, f64, f64),
    size: f64,
    r: f64,
    shape: Shape,
) -> Result<(Solid, Rebuild)> {
    let s = setback(r, phi, shape);
    let tol = (size * 1e-7).max(1e-9);
    // The arc centre is r from both faces (inside the material for a convex edge, outside for a
    // concave one), in the cross-section through `p`.
    let centre = |p: Vec3| p + t1 * s + n1 * (r * side);
    // Each end of the blend lies in one more face (the end face, square to the edge or at an
    // angle) or, for a fillet, in two that meet at the edge's end (Vida, Martin and Várady 1994:
    // trimlines joined across existing faces): the blend then crosses the edge between them,
    // which is cut back to the crossing. The rails run parallel to the edge, set back by s in
    // their side faces, and end in the end plane they reach (`ga` from f1, `gb` from f2). A side
    // edge that the setback uses up exactly is consumed: the rail ends at its far vertex (`ca`,
    // `cb`; one end face only).
    struct End {
        v: mt::Vertex,
        g: Vec<usize>,
        e1: mt::Edge,
        e2: mt::Edge,
        /// The edge between two end faces, and where the blend crosses it.
        e3: Option<(mt::Edge, Vec3)>,
        a: Vec3,
        b: Vec3,
        /// The end's section of the blend, from A to B (across both faces when two).
        planes: Vec<(Vec3, Vec3)>,
        ca: Option<mt::Vertex>,
        cb: Option<mt::Vertex>,
        /// Points of the end's section on a curved end face, from A to B.
        cap: Option<Vec<Vec3>>,
        /// How far along the edge the section reaches from the end vertex.
        reach: f64,
    }
    let mut ends: Vec<End> = Vec::new();
    for (v, p) in [(&v0, p0), (&v1, p1)] {
        let at_v: Vec<usize> = (0..faces.len()).filter(|i| faces.get(*i).is_some_and(|f| f.vertex_iter().any(|x| &x == v))).collect();
        let others: Vec<usize> = at_v.iter().copied().filter(|i| *i != i1 && *i != i2).collect();
        // The edge at v that face `fi` shares with face `g`, other than the blended edge.
        let shared = |fi: usize, g: usize| -> Option<mt::Edge> {
            let (f, gf) = (faces.get(fi)?, faces.get(g)?);
            f.edge_iter().find(|e| e.id() != edge.id() && (e.front() == v || e.back() == v) && gf.edge_iter().any(|x| x.id() == e.id()))
        };
        let (ga, gb) = match others[..] {
            [g] => (g, g),
            [x, y] if shared(i1, x).is_some() && shared(i2, y).is_some() => (x, y),
            [x, y] if shared(i1, y).is_some() && shared(i2, x).is_some() => (y, x),
            _ => return Err(unsupported("each end of the edge must touch exactly one more face")),
        };
        // An end face that is not a plane, or meets the side faces in curves: the rails end where
        // they reach its surface, and the blend's end is where its surface meets the end face
        // (the cross-section through `p` slid along d onto it, point by point).
        let is_line = |e: &mt::Edge| matches!(e.curve(), mt::Curve::Line(_));
        if ga == gb
            && let (Some(e1), Some(e2)) = (shared(i1, ga), shared(i2, gb))
            && (faces.get(ga).and_then(|f| plane_normal(f)).is_none() || !is_line(&e1) || !is_line(&e2))
        {
            let g = faces.get(ga).ok_or_else(|| KernelError::Failed("face".into()))?;
            let surf = g.surface();
            let on_tol = (size * 1e-5).max(1e-6);
            let reach = 3.0 * (s + r);
            let (sa, sb) = (p + t1 * s, p + t2 * s);
            let c0 = centre(p);
            let theta = std::f64::consts::PI - phi;
            let profile = |f: f64| -> Vec3 {
                match shape {
                    Shape::Flat => sa + (sb - sa) * f,
                    Shape::Round => c0 + ((sa - c0) * ((1.0 - f) * theta).sin() + (sb - c0) * (f * theta).sin()) / theta.sin(),
                }
            };
            // A rail's end: on the side edge, else on its continuation within the side face.
            let rail_end = |x: Vec3, e: &mt::Edge, fi: usize| -> Result<(Vec3, f64)> {
                let hits = line_surface_hits(&surf, x, d, reach, tol);
                if let Some(l) = hits.iter().find(|l| on_edge(e, x + d * **l, on_tol).is_some()) {
                    return Ok((x + d * *l, *l));
                }
                let side = faces.get(fi).ok_or_else(|| KernelError::Failed("face".into()))?;
                hits.iter()
                    .find(|l| on_extension(e, x + d * **l, on_tol) && extension_fits(side, p, x + d * **l, tol))
                    .map(|l| (x + d * *l, *l))
                    .ok_or_else(|| too_large("the blend is larger than the neighbouring faces"))
            };
            let ((a, la), (b, lb)) = (rail_end(sa, &e1, i1)?, rail_end(sb, &e2, i2)?);
            let flat_plane = shape == Shape::Flat && plane_normal(g).is_some();
            let mut cap: Option<Vec<Vec3>> = None;
            let mut far = la.abs().max(lb.abs());
            for n in [16usize, 32, 64] {
                if flat_plane {
                    cap = Some(vec![a, b]);
                    break;
                }
                // The section's points on the end face, each continuing from the last.
                let mut pts = vec![a];
                let mut last = la;
                let mut ok = true;
                for i in 1..n {
                    let x = profile(i as f64 / n as f64);
                    match line_surface_hits(&surf, x, d, reach, tol).into_iter().min_by(|u, v| (u - last).abs().total_cmp(&(v - last).abs())) {
                        Some(l) => {
                            pts.push(x + d * l);
                            last = l;
                            far = far.max(l.abs());
                        }
                        None => {
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok {
                    break;
                }
                pts.push(b);
                // The fitted curve must stay on both surfaces between the points.
                let Some(bs) = crate::build::interpolate_cubic(&pts) else { break };
                let fits = (0..n).all(|i| {
                    use mt::{ParametricCurve, ParametricSurface, SearchNearestParameter};
                    let q = from_p3(bs.subs((i as f64 + 0.5) / n as f64));
                    let on_g = surf.search_nearest_parameter(p3(q), None, 100).is_some_and(|(u, v)| from_p3(surf.subs(u, v)).dist(q) <= on_tol);
                    let on_blend = match shape {
                        Shape::Round => {
                            let w = q - c0;
                            ((w - d * w.dot(d)).len() - r).abs() <= on_tol
                        }
                        Shape::Flat => (sb - sa).cross(d).normalized().is_some_and(|m| (q - sa).dot(m).abs() <= on_tol),
                    };
                    on_g && on_blend
                });
                if fits {
                    cap = Some(pts);
                    break;
                }
            }
            let cap = cap.ok_or_else(|| unsupported("the blend's end on a curved face"))?;
            ends.push(End { v: v.clone(), g: vec![ga], e1, e2, e3: None, a, b, planes: Vec::new(), ca: None, cb: None, cap: Some(cap), reach: far });
            continue;
        }
        let normal = |g: usize| -> Result<Vec3> {
            let n = faces.get(g).and_then(|f| plane_normal(f)).ok_or_else(|| unsupported("the faces at the edge ends must be planar"))?;
            if n.dot(d).abs() < 1e-9 {
                return Err(unsupported("an end face parallel to the edge"));
            }
            Ok(n)
        };
        let (na, nb) = (normal(ga)?, normal(gb)?);
        // Slide a point of the cross-section through `p` along d onto the end plane through p.
        let slide = |x: Vec3, n: Vec3| x - d * (n.dot(x - p) / n.dot(d));
        let (a, b) = (slide(p + t1 * s, na), slide(p + t2 * s, nb));
        let (e1, e2) = (
            shared(i1, ga).ok_or_else(|| unsupported("unexpected corner topology"))?,
            shared(i2, gb).ok_or_else(|| unsupported("unexpected corner topology"))?,
        );
        let mut consumed: [Option<mt::Vertex>; 2] = [None, None];
        for (k, (e, q, fi)) in [(&e1, a, i1), (&e2, b, i2)].into_iter().enumerate() {
            if !matches!(e.curve(), mt::Curve::Line(_)) {
                return Err(unsupported("the edges at the corners must be straight"));
            }
            let far = if e.front() == v { e.back().clone() } else { e.front().clone() };
            let delta = vtx(&far) - p;
            let len = delta.len();
            let dir = delta.normalized().ok_or_else(|| unsupported("zero-length corner edge"))?;
            let along = (q - p).dot(dir);
            if (q - p - dir * along).len() > tol * 100.0 + 1e-7 {
                return Err(unsupported("the blend is larger than the neighbouring faces"));
            }
            // A fillet's setback that uses up the side edge exactly consumes it.
            if shape == Shape::Round && ga == gb && (along - len).abs() <= tol {
                if let Some(slot) = consumed.get_mut(k) {
                    *slot = Some(far);
                }
                continue;
            }
            // A rail ending before the old corner extends the neighbouring rim edge away from its
            // other vertex: valid only within the trimmed side face.
            if along >= len - tol || (along < 0.0 && !faces.get(fi).is_some_and(|f| extension_fits(f, p, q, tol))) {
                return Err(too_large("the blend is larger than the neighbouring faces"));
            }
        }
        // Across two end faces: where the blend's cylinder crosses the edge between them, on the
        // blend's side of the axis (between its two rails).
        let e3 = if ga == gb {
            None
        } else {
            let (Some(fa), Some(fb)) = (faces.get(ga), faces.get(gb)) else { return Err(KernelError::Failed("face".into())) };
            let e3 = fa
                .edge_iter()
                .find(|e| (e.front() == v || e.back() == v) && fb.edge_iter().any(|x| x.id() == e.id()))
                .ok_or_else(|| unsupported("unexpected corner topology"))?;
            if !matches!(e3.curve(), mt::Curve::Line(_)) {
                return Err(unsupported("the edges at the corners must be straight"));
            }
            let far = if e3.front() == v { vtx(e3.back()) } else { vtx(e3.front()) };
            let (len3, u) = (far.dist(p), (far - p).normalized().ok_or_else(|| unsupported("zero-length corner edge"))?);
            let c0 = centre(p);
            let perp = |x: Vec3| x - d * x.dot(d);
            if shape == Shape::Flat {
                // The chamfer plane through both rails crosses the edge between them.
                let (sa, sb) = (p + t1 * s, p + t2 * s);
                let m = (sb - sa).cross(d).normalized().ok_or_else(|| KernelError::Failed("chamfer plane".into()))?;
                let den = m.dot(u);
                let root = if den.abs() > 1e-12 { m.dot(sa - p) / den } else { f64::NAN };
                let x = perp(p + u * root - sa);
                let along = x.dot(sb - sa) / (sb - sa).len2();
                if !(root > tol && root < len3 - tol && along > 1e-9 && along < 1.0 - 1e-9) {
                    return Err(unsupported("the blend does not cross the edge between the end faces"));
                }
                ends.push(End {
                    v: v.clone(),
                    g: vec![ga, gb],
                    e1: e1.clone(),
                    e2: e2.clone(),
                    e3: Some((e3.clone(), p + u * root)),
                    a,
                    b,
                    planes: vec![(p, na), (p, nb)],
                    ca: None,
                    cb: None,
                    cap: None,
                    reach: 0.0,
                });
                continue;
            }
            let (w, up) = (perp(p - c0), perp(u));
            // |w + λ up|² = r²
            let (qa, qb, qc) = (up.len2(), 2.0 * w.dot(up), w.len2() - r * r);
            let disc = qb * qb - 4.0 * qa * qc;
            if qa < 1e-18 || disc < 0.0 {
                return Err(unsupported("the blend does not cross the edge between the end faces"));
            }
            let (ra, rb) = (perp(p + t1 * s - c0), perp(p + t2 * s - c0));
            let angle = |x: Vec3, y: Vec3| (x.dot(y) / (x.len() * y.len()).max(1e-300)).clamp(-1.0, 1.0).acos();
            let between = |x: Vec3| (angle(ra, x) + angle(x, rb) - angle(ra, rb)).abs() < 1e-6;
            let root = [(-qb - disc.sqrt()) / (2.0 * qa), (-qb + disc.sqrt()) / (2.0 * qa)]
                .into_iter()
                .filter(|l| *l > tol && *l < len3 - tol)
                .find(|l| between(w + up * *l))
                .ok_or_else(|| unsupported("the blend does not cross the edge between the end faces"))?;
            Some((e3.clone(), p + u * root))
        };
        let planes = if ga == gb { vec![(p, na)] } else { vec![(p, na), (p, nb)] };
        let [ca, cb] = consumed;
        ends.push(End { v: v.clone(), g: if ga == gb { vec![ga] } else { vec![ga, gb] }, e1, e2, e3, a, b, planes, ca, cb, cap: None, reach: 0.0 });
    }
    let [end0, end1] = &ends[..] else { return Err(KernelError::Failed("ends".into())) };
    if end0.g.iter().any(|g| end1.g.contains(g)) {
        return Err(unsupported("both ends touch the same face"));
    }
    if (end1.a - end0.a).dot(d) <= tol || (end1.b - end0.b).dot(d) <= tol {
        return Err(too_large("the blend is larger than the neighbouring faces"));
    }
    // A side face whose corner edges are consumed at both ends is used up: its remaining edge,
    // parallel to the old edge at the setback, becomes the rail and the face is removed.
    let used_up = |fi: usize, c0: &Option<mt::Vertex>, c1: &Option<mt::Vertex>| -> Result<Option<mt::Edge>> {
        let (Some(x0), Some(x1)) = (c0, c1) else {
            return match (c0, c1) {
                (None, None) => Ok(None),
                _ => Err(unsupported("the blend uses up a neighbouring face at one end only")),
            };
        };
        let f = faces.get(fi).ok_or_else(|| KernelError::Failed("face".into()))?;
        if f.boundaries().len() != 1 || f.edge_iter().count() != 4 {
            return Err(unsupported("the blend uses up a neighbouring face with more edges"));
        }
        let rail = f
            .edge_iter()
            .find(|e| matches!(e.curve(), mt::Curve::Line(_)) && ((e.front() == x0 && e.back() == x1) || (e.front() == x1 && e.back() == x0)))
            .ok_or_else(|| unsupported("the blend uses up a neighbouring face with more edges"))?;
        Ok(Some(if rail.front() == x0 { rail.clone() } else { rail.inverse() }))
    };
    let rail_a = used_up(i1, &end0.ca, &end1.ca)?;
    let rail_b = used_up(i2, &end0.cb, &end1.cb)?;
    let vertex = |c: &Option<mt::Vertex>, p: Vec3| c.clone().unwrap_or_else(|| builder::vertex(p3(p)));
    // Corner curves on the end faces (A → B): the blend's cross-section through the end vertex,
    // slid along the edge onto each end plane (an ellipse arc; the circle itself on a square
    // end face).
    let section = |p: Vec3, va: &mt::Vertex, vb: &mt::Vertex, from: Vec3, to: Vec3, plane: (Vec3, Vec3)| -> Result<mt::Edge> {
        if shape == Shape::Flat {
            return Ok(builder::line(va, vb));
        }
        let c = centre(p);
        let mid = c + ((from - c) + (to - c)).normalized().unwrap_or(-n1) * r;
        let (o, n) = plane;
        if n.dot(d).abs() >= 1.0 - 1e-12 {
            return Ok(builder::circle_arc(va, vb, p3(mid)));
        }
        let arc = builder::circle_arc(&builder::vertex(p3(from)), &builder::vertex(p3(to)), p3(mid));
        let den = d.dot(n);
        // x ↦ x − d ((x − o)·n) / (d·n), column-major.
        let col = |j: usize| {
            let ej = [Vec3::X, Vec3::Y, Vec3::Z][j];
            ej - d * (ej.dot(n) / den)
        };
        let (c0, c1, c2) = (col(0), col(1), col(2));
        let tr = d * (o.dot(n) / den);
        let mat = mt::Matrix4::new(c0.x, c0.y, c0.z, 0.0, c1.x, c1.y, c1.z, 0.0, c2.x, c2.y, c2.z, 0.0, tr.x, tr.y, tr.z, 1.0);
        let mut curve = arc.curve();
        mt::Transformed::transform_by(&mut curve, mat);
        Ok(mt::Edge::new(va, vb, curve))
    };
    let mut corners: Vec<(mt::Vertex, mt::Vertex, Vec<mt::Edge>, Option<mt::Vertex>)> = Vec::new();
    for (end, p) in [(end0, p0), (end1, p1)] {
        let (va, vb) = (vertex(&end.ca, end.a), vertex(&end.cb, end.b));
        let (sa, sb) = (p + t1 * s, p + t2 * s);
        let (chain, vq) = match (&end.e3, &end.planes[..]) {
            _ if end.cap.is_some() => {
                let pts = end.cap.as_deref().unwrap_or_default();
                let curve = if pts.len() == 2 {
                    builder::line(&va, &vb)
                } else {
                    let bs = crate::build::interpolate_cubic(pts).ok_or_else(|| unsupported("the blend's end on a curved face"))?;
                    mt::Edge::try_new(&va, &vb, mt::Curve::BSplineCurve(bs)).map_err(|e| KernelError::Failed(format!("blend end: {e}")))?
                };
                (vec![curve], None)
            }
            (None, [plane]) => (vec![section(p, &va, &vb, sa, sb, *plane)?], None),
            (Some((_, q)), [pa, pb]) => {
                let vq = builder::vertex(p3(*q));
                let sq = *q - d * (*q - p).dot(d);
                (vec![section(p, &va, &vq, sa, sq, *pa)?, section(p, &vq, &vb, sq, sb, *pb)?], Some(vq))
            }
            _ => return Err(KernelError::Failed("blend end".into())),
        };
        corners.push((va, vb, chain, vq));
    }
    let [(a0, b0, c0, q0), (a1, b1, c1, q1)] = &corners[..] else { return Err(KernelError::Failed("ends".into())) };
    let la = rail_a.clone().unwrap_or_else(|| builder::line(a0, a1));
    let lb = rail_b.clone().unwrap_or_else(|| builder::line(b0, b1));

    // Substitutions: side edges at the ends now stop at A/B (consumed ones are dropped), an edge
    // between two end faces stops where the blend crosses it; the blended edge moves to A or B.
    let mut subst: HashMap<mt::EdgeID, mt::Edge> = HashMap::new();
    let mut dropped: Vec<mt::EdgeID> = Vec::new();
    let on_tol = (size * 1e-5).max(1e-6);
    let shorten = |e: &mt::Edge, v: &mt::Vertex, nv: &mt::Vertex| retarget(e, v, nv, on_tol);
    for (end, a, b, q) in [(end0, a0, b0, q0), (end1, a1, b1, q1)] {
        for (e, nv, used) in [(&end.e1, a, &end.ca), (&end.e2, b, &end.cb)] {
            if used.is_some() {
                dropped.push(e.id());
                continue;
            }
            subst.insert(e.id(), shorten(e, &end.v, nv)?);
        }
        if let (Some((e3, _)), Some(q)) = (&end.e3, q) {
            subst.insert(e3.id(), shorten(e3, &end.v, q)?);
        }
    }
    // The side edges at the ends are pieces of themselves; the rails and corner curves are new.
    let mut rec = Rebuild::default();
    rec.pieces(&subst);
    let mut new_faces: Vec<mt::Face> = Vec::with_capacity(faces.len() + 1);
    for (i, f) in faces.iter().enumerate() {
        if (i == i1 && rail_a.is_some()) || (i == i2 && rail_b.is_some()) {
            continue;
        }
        let nf = if i == i1 {
            let mut m = subst.clone();
            m.insert(edge.id(), la.clone());
            rebuild_face(f, &m, &[])?
        } else if i == i2 {
            let mut m = subst.clone();
            m.insert(edge.id(), lb.clone());
            rebuild_face(f, &m, &[])?
        } else if end0.g.contains(&i) {
            rebuild_face_dropping(f, &subst, &dropped, c0)?
        } else if end1.g.contains(&i) {
            rebuild_face_dropping(f, &subst, &dropped, c1)?
        } else if f.edge_iter().any(|e| subst.contains_key(&e.id())) {
            rebuild_face(f, &subst, &[])?
        } else {
            (*f).clone()
        };
        rec.face(&nf, f);
        new_faces.push(nf);
    }
    // The blend face uses each new edge opposite to its neighbour.
    let f1_forward = faces.get(i1).is_some_and(|f| f.boundaries().iter().any(|w| w.edge_iter().any(|e| e.id() == edge.id() && e.front() == &v0)));
    let reversed = |chain: &[mt::Edge]| chain.iter().rev().map(|e| e.inverse()).collect::<Vec<_>>();
    let mut edges_out: Vec<mt::Edge> = Vec::new();
    if f1_forward {
        edges_out.extend(c0.iter().cloned());
        edges_out.push(lb.clone());
        edges_out.extend(reversed(c1));
        edges_out.push(la.inverse());
    } else {
        edges_out.push(la.clone());
        edges_out.extend(c1.iter().cloned());
        edges_out.push(lb.inverse());
        edges_out.extend(reversed(c0));
    }
    let wire: mt::Wire = edges_out.into();
    let blend_face = match shape {
        Shape::Flat => builder::try_attach_plane(&[wire]).map_err(|e| KernelError::Failed(format!("chamfer face: {e}")))?,
        Shape::Round => {
            // The cylinder, from a cross-section before the first end to one past the second, so
            // it holds the end sections wherever the end planes slide them.
            let reach = ends
                .iter()
                .flat_map(|e| e.planes.iter().map(|(_, n)| (s + r) * (1.0 - n.dot(d).powi(2)).max(0.0).sqrt() / n.dot(d).abs()))
                .chain(ends.iter().map(|e| e.reach))
                .fold(0.0, f64::max)
                + r;
            let start = p0 - d * reach;
            let (sa, sb) = (start + t1 * s, start + t2 * s);
            let c = centre(start);
            let mid = c + ((sa - c) + (sb - c)).normalized().unwrap_or(-n1) * r;
            let base = builder::circle_arc(&builder::vertex(p3(sa)), &builder::vertex(p3(sb)), p3(mid));
            let swept: mt::Face = builder::tsweep(&base, v3(p1 - p0 + d * (2.0 * reach)));
            let mut surf = swept.oriented_surface();
            // Outward normal at the arc middle: away from the centre on a convex edge (the
            // centre is in the material), toward it on a concave one.
            let outward = (mid - c) * (-side);
            use mt::{ParametricSurface3D, SearchParameter};
            if let Some((u, v)) = surf.search_parameter(p3(mid), None, 100) {
                let nn = surf.normal(u, v);
                if Vec3::new(nn.x, nn.y, nn.z).dot(outward) < 0.0 {
                    surf = mt::Invertible::inverse(&surf);
                }
            }
            mt::Face::try_new(vec![wire], surf).map_err(|e| KernelError::Failed(format!("fillet face: {e}")))?
        }
    };
    rec.blend(&blend_face, [edge.id()]);
    new_faces.push(blend_face);
    let mut shells = solid.boundaries().clone();
    let new_shell: mt::Shell = new_faces.into();
    if let Some(sh) = shells.get_mut(si) {
        *sh = new_shell;
    }
    let solid = Solid::try_new(shells).map_err(|e| KernelError::Failed(format!("blend produced an invalid solid: {e}")))?;
    Ok((solid, rec))
}

fn blend(body: &Body, edges: &[Vec3], r: f64, shape: Shape, what: &str) -> Result<Body> {
    if !(r.is_finite() && r > 1e-6 && r < 1e6) {
        return Err(KernelError::Invalid(format!("{what} size must be positive")));
    }
    if edges.is_empty() {
        return Err(KernelError::Invalid("no edges selected".into()));
    }
    if edges.len() > 1000 {
        return Err(KernelError::Invalid("too many edges".into()));
    }
    // Each step's result carries its history over `[body]`.
    let counts = record::counts(&[body]);
    // Coplanar neighbours as one face (as after a join), so corners look as they should.
    let mut cur = crate::boolean_record::healed(body)?;
    // Every edge of a convex planar body: rounded as a whole (sphere corners).
    if shape == Shape::Round && edges.len() >= 6 {
        let solid = cur.deep_copy();
        let all = Body::unique_edges(&solid);
        if all.len() == edges.len() {
            let size = cur.size();
            let tol = (size * 2e-3).max(1e-3);
            let mut hit = std::collections::HashSet::new();
            for p in edges {
                if let Some((idx, _)) = cur.nearest_edge(*p, tol)? {
                    hit.insert(idx);
                }
            }
            if hit.len() == all.len()
                && let Ok(b) = crate::polyhedron::round_all_edges(&cur, r)
            {
                return Ok(record::after(b, &cur, &counts));
            }
        }
    }
    // Whole smooth loops of planar faces are blended in one go, loop by loop.
    // Loops and other edges together (every edge of an extruded part): the other edges
    // first, which leaves the loops smooth, then the loops.
    if let Some((groups, rest)) = loop_split(&cur, edges)?
        && !rest.is_empty()
        && rest.len() < edges.len()
    {
        let _ = groups;
        let loop_pts: Vec<Vec3> = edges.iter().filter(|p| !rest.iter().any(|q| q.dist(**p) < 1e-12)).copied().collect();
        cur = record::after(blend(&cur, &rest, r, shape, what)?, &cur, &counts);
        return Ok(record::after(blend(&cur, &loop_pts, r, shape, what)?, &cur, &counts));
    }
    if let Some(groups) = loop_groups(&cur, edges)? {
        for g in groups {
            let size = cur.size();
            let solid = cur.deep_copy();
            let ids = edge_ids(&cur, &solid, &g)?;
            match crate::loopblend::loop_blend(&solid, &ids, size, r, shape == Shape::Round) {
                Some(res) => {
                    let (made, rec) = guard(what, || res)?;
                    cur = record::after(record::rebuilt(made, &[&solid], &rec)?, &cur, &counts);
                }
                None => return Err(unsupported("the edges are not a whole loop of a planar face")),
            }
        }
        return Ok(cur);
    }
    // One at a time, in whatever order works: a run along a planar face's loop (bridging arcs
    // that earlier blends left between selected edges, so corners round off), a single edge,
    // or a chain of curved edges.
    let mut todo: Vec<Vec3> = edges.to_vec();
    // Edges refused since the last blend that could have changed them: (pick, the box around the
    // edge, the refusal). A blend only changes the body near the edges it rounds, so an edge
    // refused away from every later blend would be refused the same way again: it is not tried
    // again (a refusal costs a rolling-ball attempt, seconds each on a long curved edge).
    let mut refused: Vec<(Vec3, (Vec3, Vec3), KernelError)> = Vec::new();
    for _ in 0..(2 * edges.len() + 4) {
        if todo.is_empty() {
            return Ok(cur);
        }
        let size = cur.size();
        let tol = (size * 2e-3).max(1e-3);
        let solid = cur.deep_copy();
        if todo.len() >= 2
            && let Some((ids, covered)) = run_on_loop(&cur, &solid, &todo)?
            && let Some(Ok((res, rec))) = crate::loopblend::loop_blend(&solid, &ids, size, r, shape == Shape::Round)
            && let Ok(b) = guard(what, || Ok(res)).and_then(|res| record::rebuilt(res, &[&solid], &rec))
        {
            let near = edges_box(&solid, &ids, r + tol);
            refused.retain(|(_, b, _)| !boxes_meet(*b, near));
            cur = record::after(b, &cur, &counts);
            todo.retain(|p| !covered.iter().any(|q| q.dist(*p) < 1e-12));
            continue;
        }
        let mut first_err: Option<KernelError> = None;
        let mut done: Option<(Body, Vec<usize>, (Vec3, Vec3))> = None;
        for (j, p) in todo.iter().enumerate() {
            if let Some((_, _, e)) = refused.iter().find(|(q, _, _)| q.dist(*p) < 1e-12) {
                if first_err.is_none() || (matches!(e, KernelError::TooLarge(_)) && !matches!(first_err, Some(KernelError::TooLarge(_)))) {
                    first_err = Some(e.clone());
                }
                continue;
            }
            let Some((idx, dist)) = cur.nearest_edge(*p, tol)? else { return Err(KernelError::Invalid("the body has no edges".into())) };
            if dist > size * 0.05 + 1e-3 {
                return Err(KernelError::Invalid(format!("no edge near {:?}", [p.x, p.y, p.z])));
            }
            let edge = Body::unique_edges(&solid).into_iter().nth(idx).ok_or_else(|| KernelError::Invalid("edge index".into()))?;
            // Roll a ball along the edge's smooth chain (the points of `todo` on it go with it).
            let roll = |ids: &[mt::EdgeID]| {
                let on_chain: Vec<usize> = todo
                    .iter()
                    .enumerate()
                    .filter(|(_, q)| {
                        cur.nearest_edge(**q, tol)
                            .ok()
                            .flatten()
                            .and_then(|(i, _)| Body::unique_edges(&solid).into_iter().nth(i))
                            .is_some_and(|x| ids.contains(&x.id()))
                    })
                    .map(|(i, _)| i)
                    .collect();
                #[cfg(test)]
                ROLLS.with(|n| n.set(n.get() + 1));
                guard(what, || crate::curveblend::curve_blend(&solid, ids, size, r, shape == Shape::Round))
                    .map(|o| (o, if on_chain.is_empty() { vec![j] } else { on_chain }))
            };
            // A straight edge running on smoothly into curved ones (a flange into its bend, a
            // side into a rounded corner): the blend follows the whole smooth chain, as the
            // rolling ball would.
            let chain = smooth_chain(&solid, &edge);
            let runs_on = !curved_case(&solid, &edge)
                && chain.as_ref().is_some_and(|ids| {
                    ids.len() > 1 && Body::unique_edges(&solid).iter().any(|x| ids.contains(&x.id()) && !matches!(x.curve(), mt::Curve::Line(_)))
                });
            let attempt = match chain.as_deref().filter(|_| runs_on).map(roll) {
                Some(Ok(o)) => Ok(o),
                Some(Err(e2)) => guard(what, || blend_one(&solid, &edge, size, r, shape)).map(|o| (o, vec![j])).map_err(|_| e2),
                None => match guard(what, || blend_one(&solid, &edge, size, r, shape)) {
                    Ok(o) => Ok((o, vec![j])),
                    // A curved edge, or one between curved faces: roll a ball along its smooth chain.
                    Err(e) if !curved_case(&solid, &edge) => Err(e),
                    Err(e) => match chain.as_deref() {
                        Some(ids) => roll(ids).map_err(|e2| match e2 {
                            KernelError::TooLarge(m) => KernelError::TooLarge(format!("{m} ({e})")),
                            e2 => KernelError::Failed(format!("{e2} ({e})")),
                        }),
                        None => Err(e),
                    },
                },
            };
            // What this attempt could change: the edge and its smooth chain, with room for the blend.
            let reach = edges_box(&solid, chain.as_deref().unwrap_or(&[edge.id()]), r + tol);
            match attempt.and_then(|((o, rec), js)| record::rebuilt(o, &[&solid], &rec).map(|b| (b, js))) {
                Ok((b, js)) => {
                    done = Some((b, js, reach));
                    break;
                }
                // Reported: the first refusal, unless a later one is a size a smaller radius may
                // fit (the others may be refused only until that edge is blended).
                Err(e) => {
                    if first_err.is_none() || (matches!(e, KernelError::TooLarge(_)) && !matches!(first_err, Some(KernelError::TooLarge(_)))) {
                        first_err = Some(e.clone());
                    }
                    refused.push((*p, reach, e));
                }
            }
        }
        match done {
            Some((b, js, near)) => {
                refused.retain(|(_, b2, _)| !boxes_meet(*b2, near));
                cur = record::after(b, &cur, &counts);
                let mut k = 0;
                todo.retain(|_| {
                    let keep = !js.contains(&k);
                    k += 1;
                    keep
                });
            }
            None => return Err(first_err.unwrap_or_else(|| unsupported("these edges"))),
        }
    }
    if todo.is_empty() { Ok(cur) } else { Err(unsupported("blending these edges together")) }
}

#[cfg(test)]
thread_local! {
    /// Rolling-ball attempts made by [`blend`] on this thread (tests count them).
    pub(crate) static ROLLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The box around these edges of `solid` (their ends and middles), grown by `margin`.
fn edges_box(solid: &Solid, ids: &[mt::EdgeID], margin: f64) -> (Vec3, Vec3) {
    use mt::{BoundedCurve, ParametricCurve};
    let mut lo = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut hi = -lo;
    for e in Body::unique_edges(solid).iter().filter(|e| ids.contains(&e.id())) {
        let c = e.curve();
        let (t0, t1) = c.range_tuple();
        for k in 0..=8 {
            let q = from_p3(c.subs(t0 + (t1 - t0) * f64::from(k) / 8.0));
            lo = lo.min(q);
            hi = hi.max(q);
        }
    }
    let m = Vec3::new(margin, margin, margin);
    (lo - m, hi + m)
}

/// Whether two boxes overlap (an empty box meets every box: nothing is known about it).
fn boxes_meet(a: (Vec3, Vec3), b: (Vec3, Vec3)) -> bool {
    let empty = |x: (Vec3, Vec3)| !(x.0.x <= x.1.x && x.0.y <= x.1.y && x.0.z <= x.1.z);
    empty(a) || empty(b) || (a.0.x <= b.1.x && b.0.x <= a.1.x && a.0.y <= b.1.y && b.0.y <= a.1.y && a.0.z <= b.1.z && b.0.z <= a.1.z)
}

/// Neighbouring edges along one boundary loop of a planar face that between them hold at least
/// two of the points' edges, joined by arcs that meet them smoothly (left by earlier blends):
/// the run's edge ids and the points it covers.
#[allow(clippy::type_complexity)]
fn run_on_loop(cur: &Body, solid: &Solid, pts: &[Vec3]) -> Result<Option<(Vec<mt::EdgeID>, Vec<Vec3>)>> {
    use mt::{BoundedCurve, ParametricCurve};
    let size = cur.size();
    let tol = (size * 2e-3).max(1e-3);
    let all = Body::unique_edges(solid);
    let mut per: Vec<(Vec3, mt::EdgeID)> = Vec::new();
    for p in pts {
        if let Some((idx, dist)) = cur.nearest_edge(*p, tol)?
            && dist <= size * 0.05 + 1e-3
            && let Some(e) = all.get(idx)
        {
            per.push((*p, e.id()));
        }
    }
    let dir = |e: &mt::Edge, at_end: bool| -> Option<Vec3> {
        let c = e.oriented_curve();
        let (t0, t1) = c.range_tuple();
        let d = c.der(if at_end { t1 } else { t0 });
        Vec3::new(d.x, d.y, d.z).normalized()
    };
    let mut best: Option<(Vec<mt::EdgeID>, Vec<Vec3>)> = None;
    for f in solid.face_iter() {
        if !matches!(f.oriented_surface(), mt::Surface::Plane(_)) {
            continue;
        }
        for w in f.boundaries() {
            let es: Vec<mt::Edge> = w.edge_iter().cloned().collect();
            let m = es.len();
            let sel = |i: usize| es.get(i % m).is_some_and(|e| per.iter().any(|(_, id)| *id == e.id()));
            // A filler: an unselected arc meeting both neighbours smoothly.
            let smooth = |a: &mt::Edge, b: &mt::Edge| match (dir(a, true), dir(b, false)) {
                (Some(x), Some(y)) => x.dot(y) > 1.0 - 1e-6,
                _ => false,
            };
            let filler = |i: usize| {
                let (Some(prev), Some(e), Some(next)) = (es.get((i + m - 1) % m), es.get(i % m), es.get((i + 1) % m)) else { return false };
                !matches!(e.curve(), mt::Curve::Line(_)) && smooth(prev, e) && smooth(e, next)
            };
            let usable = |i: usize| sel(i) || filler(i);
            if (0..m).all(usable) {
                // The whole loop.
                if (0..m).filter(|i| sel(*i)).count() >= 2 {
                    let ids: Vec<mt::EdgeID> = es.iter().map(|e| e.id()).collect();
                    let covered: Vec<Vec3> = per.iter().filter(|(_, id)| ids.contains(id)).map(|(p, _)| *p).collect();
                    if best.as_ref().is_none_or(|b| covered.len() > b.1.len()) {
                        best = Some((ids, covered));
                    }
                }
                continue;
            }
            // Runs: start after an unusable edge; trim fillers off both ends.
            for s0 in 0..m {
                if usable(s0) && !usable(s0 + m - 1) {
                    let mut idx: Vec<usize> = (0..m).map(|j| s0 + j).take_while(|i| usable(*i)).collect();
                    while idx.first().is_some_and(|i| !sel(*i)) {
                        idx.remove(0);
                    }
                    while idx.last().is_some_and(|i| !sel(*i)) {
                        idx.pop();
                    }
                    if idx.iter().filter(|i| sel(**i)).count() < 2 {
                        continue;
                    }
                    let ids: Vec<mt::EdgeID> = idx.iter().filter_map(|i| es.get(i % m).map(|e| e.id())).collect();
                    // Not yet when another selected edge meets one of the run's inner corners:
                    // that edge goes first, and the run then rounds the corner.
                    let inner: Vec<mt::Vertex> = idx.iter().skip(1).filter_map(|i| es.get(i % m).map(|e| e.front().clone())).collect();
                    let blocked = per
                        .iter()
                        .filter(|(_, id)| !ids.contains(id))
                        .any(|(_, id)| all.iter().find(|e| e.id() == *id).is_some_and(|e| inner.iter().any(|v| v == e.front() || v == e.back())));
                    if blocked {
                        continue;
                    }
                    let covered: Vec<Vec3> = per.iter().filter(|(_, id)| ids.contains(id)).map(|(p, _)| *p).collect();
                    if best.as_ref().is_none_or(|b| covered.len() > b.1.len()) {
                        best = Some((ids, covered));
                    }
                }
            }
        }
    }
    Ok(best)
}

/// Is the edge curved, or does it lie between faces that aren't both planar?
fn curved_case(solid: &Solid, edge: &mt::Edge) -> bool {
    if !matches!(edge.curve(), mt::Curve::Line(_)) {
        return true;
    }
    solid.face_iter().filter(|f| f.edge_iter().any(|e| e.id() == edge.id())).any(|f| !matches!(f.oriented_surface(), mt::Surface::Plane(_)))
}

/// The chain of edges continuing `edge` smoothly at its ends (its ids): closed, or open where
/// the next edge turns sharply.
fn smooth_chain(solid: &Solid, edge: &mt::Edge) -> Option<Vec<mt::EdgeID>> {
    use mt::{BoundedCurve, ParametricCurve};
    let all = Body::unique_edges(solid);
    // Along the edge as it runs from its front to its back (an edge may run against its curve).
    let dir = |e: &mt::Edge, at_end: bool| -> Option<Vec3> {
        let c = e.oriented_curve();
        let (t0, t1) = c.range_tuple();
        let d = c.der(if at_end { t1 } else { t0 });
        Vec3::new(d.x, d.y, d.z).normalized()
    };
    let mut ids = vec![edge.id()];
    // Walk on from one end of the edge (leaving vertex `v` in direction `d`).
    let walk = |mut v: mt::Vertex, mut d: Vec3, ids: &mut Vec<mt::EdgeID>, stop: &mt::Vertex| -> Option<bool> {
        for _ in 0..200 {
            if v == *stop {
                return Some(true);
            }
            let next = all.iter().filter(|e| !ids.contains(&e.id()) && (*e.front() == v || *e.back() == v)).find_map(|e| {
                let fwd = *e.front() == v;
                let t = if fwd { dir(e, false)? } else { -dir(e, true)? };
                (t.dot(d) > 1.0 - 1e-4).then(|| (e.clone(), fwd))
            });
            let Some((e, fwd)) = next else { return Some(false) };
            ids.push(e.id());
            if fwd {
                v = e.back().clone();
                d = dir(&e, true)?;
            } else {
                v = e.front().clone();
                d = -dir(&e, false)?;
            }
        }
        None
    };
    let closed = walk(edge.back().clone(), dir(edge, true)?, &mut ids, edge.front())?;
    if !closed {
        walk(edge.front().clone(), -dir(edge, false)?, &mut ids, edge.back())?;
    }
    Some(ids)
}

pub(crate) fn edge_mid(e: &mt::Edge) -> Option<Vec3> {
    use mt::{BoundedCurve, ParametricCurve};
    let c = e.curve();
    let (t0, t1) = c.range_tuple();
    Some(from_p3(c.subs((t0 + t1) * 0.5)))
}

/// Do the loop's edges meet tangentially everywhere?
fn smooth_loop(w: &mt::Wire) -> bool {
    use mt::{BoundedCurve, ParametricCurve};
    let es: Vec<mt::Edge> = w.edge_iter().cloned().collect();
    let n = es.len();
    let dir = |e: &mt::Edge, at_end: bool| -> Option<Vec3> {
        let c = e.oriented_curve();
        let (t0, t1) = c.range_tuple();
        let t = if at_end { t1 } else { t0 };
        let d = c.der(t);
        Vec3::new(d.x, d.y, d.z).normalized()
    };
    (0..n).all(|i| match (es.get(i), es.get((i + 1) % n)) {
        (Some(a), Some(b)) => match (dir(a, true), dir(b, false)) {
            (Some(x), Some(y)) => x.dot(y) > 1.0 - 1e-6,
            _ => false,
        },
        _ => false,
    })
}

/// The B-rep edges nearest the points (each once).
fn edge_ids(cur: &Body, solid: &Solid, pts: &[Vec3]) -> Result<Vec<mt::EdgeID>> {
    let all = Body::unique_edges(solid);
    let mut ids = Vec::new();
    for p in pts {
        if let Some(id) = nearest_edge_id(cur, &all, *p)?
            && !ids.contains(&id)
        {
            ids.push(id);
        }
    }
    Ok(ids)
}

/// The edge of `all` (`cur`'s edges, in their order) a point picks: the nearest, if near enough.
fn nearest_edge_id(cur: &Body, all: &[mt::Edge], p: Vec3) -> Result<Option<mt::EdgeID>> {
    let size = cur.size();
    let tol = (size * 2e-3).max(1e-3);
    Ok(match cur.nearest_edge(p, tol)? {
        Some((idx, dist)) if dist <= size * 0.05 + 1e-3 => all.get(idx).map(|e| e.id()),
        _ => None,
    })
}

/// When the edges make up whole boundary loops of planar faces: the edge points per loop.
fn loop_groups(cur: &Body, pts: &[Vec3]) -> Result<Option<Vec<Vec<Vec3>>>> {
    Ok(loop_split(cur, pts)?.and_then(|(g, rest)| rest.is_empty().then_some(g)))
}

/// Edges in whole loops of planar faces (points per loop), and the other edges' points.
fn loop_split(cur: &Body, pts: &[Vec3]) -> Result<Option<(Vec<Vec<Vec3>>, Vec<Vec3>)>> {
    let solid = cur.deep_copy();
    let size = cur.size();
    let tol = (size * 2e-3).max(1e-3);
    let all = Body::unique_edges(&solid);
    // Each point's edge. A point on a vertex is as near to the edges meeting there (a circle
    // and a cylinder's seam line): prefer one on a planar face's boundary.
    let polys = cur.edges(tol)?;
    let planar_edge: std::collections::HashSet<mt::EdgeID> = solid
        .face_iter()
        .filter(|f| matches!(f.oriented_surface(), mt::Surface::Plane(_)))
        .flat_map(|f| f.edge_iter().map(|e| e.id()).collect::<Vec<_>>())
        .collect();
    let mut per: Vec<(Vec3, mt::EdgeID)> = Vec::new();
    for p in pts {
        let dists: Vec<(usize, f64)> =
            polys.iter().map(|e| (e.index, e.points.windows(2).map(|w| p.dist_to_segment(w[0], w[1])).fold(f64::INFINITY, f64::min))).collect();
        let best = dists.iter().map(|x| x.1).fold(f64::INFINITY, f64::min);
        if best > size * 0.05 + 1e-3 {
            return Ok(None);
        }
        let near: Vec<&mt::Edge> = dists.iter().filter(|x| x.1 <= best + tol).filter_map(|x| all.get(x.0)).collect();
        let Some(e) = near.iter().find(|e| planar_edge.contains(&e.id())).or(near.first()) else { return Ok(None) };
        per.push((*p, e.id()));
    }
    let ids: Vec<mt::EdgeID> = per.iter().map(|x| x.1).collect();
    let mut covered: Vec<mt::EdgeID> = Vec::new();
    let mut groups = Vec::new();
    for f in solid.face_iter() {
        if !matches!(f.oriented_surface(), mt::Surface::Plane(_)) {
            continue;
        }
        for w in f.boundaries() {
            let wids: Vec<mt::EdgeID> = w.edge_iter().map(|e| e.id()).collect();
            if wids.iter().any(|x| covered.contains(x)) {
                continue;
            }
            // The whole loop is selected, or one of its edges and the loop is smooth (tangent
            // chain, which also covers a circle that a boolean left in pieces).
            let all_sel = wids.iter().all(|x| ids.contains(x));
            let some_sel = wids.iter().any(|x| ids.contains(x));
            if all_sel || (some_sel && smooth_loop(&w)) {
                covered.extend(wids.iter().copied());
                // One point per loop edge, on it.
                let pts: Vec<Vec3> = w.edge_iter().filter_map(edge_mid).collect();
                groups.push(pts);
            }
        }
    }
    let rest: Vec<Vec3> = per.iter().filter(|(_, id)| !covered.contains(id)).map(|(p, _)| *p).collect();
    Ok((!groups.is_empty()).then_some((groups, rest)))
}

/// Constant-radius fillet of the edges nearest to the given points. A radius too large for the
/// faces around the edges is refused with [`KernelError::TooLarge`] ([`fillet_max`] finds the
/// largest that works).
pub fn fillet(body: &Body, edges: &[Vec3], radius: f64) -> Result<Body> {
    body.require_brep("fillet")?;
    // Corners the local blends refuse (an earlier round at an edge's end): radius per edge.
    blend(body, edges, radius, Shape::Round, "fillet").or_else(|e| {
        let sel: Vec<(Vec3, f64)> = edges.iter().map(|p| (*p, radius)).collect();
        // The local blend's refusal, unless only the corner build found the radius too large.
        crate::blend_corner::fillet_radii(body, &sel).map_err(|e2| match (&e, e2) {
            (KernelError::TooLarge(_), _) => e,
            (_, e2 @ KernelError::TooLarge(_)) => e2,
            _ => e,
        })
    })
}

/// The step of the radius search: three significant digits of the requested radius.
fn radius_step(radius: f64) -> f64 {
    10f64.powf(radius.log10().floor() - 3.0)
}

/// The largest radius below `radius` (to three significant digits) at which the fillet of
/// these edges works, with its result; `None` when none does. Each candidate is a real fillet
/// of the edges, so the radius returned is one that was built; the search runs up to 16 of
/// them, so it is for an explicit request, not every refusal.
pub fn fillet_max(body: &Body, edges: &[Vec3], radius: f64) -> Option<(f64, Body)> {
    if !(radius.is_finite() && radius > 1e-6) {
        return None;
    }
    let q = radius_step(radius);
    // Radii n·q for n in [0, hi): hi fails (it is the request, or above it), lo works.
    let mut hi = (radius / q).ceil() as u64;
    let mut lo = 0u64;
    let mut best: Option<Body> = None;
    // log2(10⁴) steps reach the step size; the cap keeps a slow body from stalling.
    for _ in 0..16 {
        if hi - lo <= 1 {
            break;
        }
        let mid = lo + (hi - lo) / 2;
        match fillet(body, edges, mid as f64 * q) {
            Ok(b) => {
                lo = mid;
                best = Some(b);
            }
            Err(_) => hi = mid,
        }
    }
    best.map(|b| (lo as f64 * q, b))
}

/// [`fillet_max`] for each group of the edges that may limit each other's radius (sharing a
/// face or a vertex, directly or through other edges in the group), so a round that fits isn't
/// reduced for a tight edge elsewhere. The radius per edge (in `edges`' order) and the result;
/// one radius for all (as [`fillet_max`]) when a group is refused for another reason than its
/// size, the groups don't build together, or there are too many to search (each search is up
/// to 16 fillets, and this runs in the live preview).
pub fn fillet_max_each(body: &Body, edges: &[Vec3], radius: f64) -> Option<(Vec<f64>, Body)> {
    const MAX_GROUPS: usize = 8;
    const MAX_SEARCHES: usize = 2;
    let one = || fillet_max(body, edges, radius).map(|(r, b)| (vec![r; edges.len()], b));
    let Some(groups) = interacting_groups(body, edges).filter(|g| g.len() > 1 && g.len() <= MAX_GROUPS) else { return one() };
    let mut radii = vec![radius; edges.len()];
    let mut searches = 0;
    for g in &groups {
        let pts: Vec<Vec3> = g.iter().filter_map(|i| edges.get(*i).copied()).collect();
        let r = match fillet(body, &pts, radius) {
            Ok(_) => continue,
            Err(KernelError::TooLarge(_)) if searches < MAX_SEARCHES => {
                searches += 1;
                match fillet_max(body, &pts, radius) {
                    Some((r, _)) => r,
                    None => return one(),
                }
            }
            Err(_) => return one(),
        };
        for i in g {
            if let Some(x) = radii.get_mut(*i) {
                *x = r;
            }
        }
    }
    match fillet_each(body, edges, &radii) {
        Ok(b) => Some((radii, b)),
        Err(_) => one(),
    }
}

/// The fillet of each edge at its own radius (`radii` in `edges`' order): built together where
/// the corner build takes it, else the edges of each radius together, the largest first.
pub fn fillet_each(body: &Body, edges: &[Vec3], radii: &[f64]) -> Result<Body> {
    if radii.len() != edges.len() {
        return Err(KernelError::Invalid("one radius per edge".into()));
    }
    if let Some(r) = radii.first().filter(|r| radii.iter().all(|x| x.to_bits() == r.to_bits())) {
        return fillet(body, edges, *r);
    }
    let sel: Vec<(Vec3, f64)> = edges.iter().copied().zip(radii.iter().copied()).collect();
    if let Ok(b) = crate::blend_corner::fillet_radii(body, &sel) {
        return Ok(b);
    }
    let mut by_radius: Vec<(f64, Vec<Vec3>)> = Vec::new();
    for (p, r) in sel {
        match by_radius.iter_mut().find(|(x, _)| x.to_bits() == r.to_bits()) {
            Some((_, pts)) => pts.push(p),
            None => by_radius.push((r, vec![p])),
        }
    }
    by_radius.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut cur = body.clone();
    for (r, pts) in &by_radius {
        cur = fillet(&cur, pts, *r)?;
    }
    Ok(cur)
}

/// The points' edges (found as [`edge_ids`] finds them, on the body as the blends see it) grouped
/// where they share a face or a vertex, directly or through other points' edges: indices into
/// `pts`. `None` when a point has no edge near it.
fn interacting_groups(body: &Body, pts: &[Vec3]) -> Option<Vec<Vec<usize>>> {
    let cur = crate::boolean_record::healed(body).ok()?;
    let all = Body::unique_edges(&cur.solid);
    // What each point's edge touches: its faces and its ends.
    let mut groups: Vec<(Vec<mt::FaceID>, Vec<mt::VertexID>, Vec<usize>)> = Vec::new();
    for (i, p) in pts.iter().enumerate() {
        let id = nearest_edge_id(&cur, &all, *p).ok()??;
        let edge = all.iter().find(|e| e.id() == id)?;
        let mut faces: Vec<mt::FaceID> = cur.solid.face_iter().filter(|f| f.edge_iter().any(|e| e.id() == id)).map(|f| f.id()).collect();
        let mut ends = vec![edge.front().id(), edge.back().id()];
        let mut members = vec![i];
        // Join every group touching one of these faces or ends.
        let (meet, rest): (Vec<_>, Vec<_>) =
            groups.into_iter().partition(|(fs, vs, _)| fs.iter().any(|f| faces.contains(f)) || vs.iter().any(|v| ends.contains(v)));
        for (fs, vs, ms) in meet {
            for f in fs {
                if !faces.contains(&f) {
                    faces.push(f);
                }
            }
            for v in vs {
                if !ends.contains(&v) {
                    ends.push(v);
                }
            }
            members.extend(ms);
        }
        groups = rest;
        groups.push((faces, ends, members));
    }
    Some(
        groups
            .into_iter()
            .map(|(_, _, mut ms)| {
                ms.sort_unstable();
                ms
            })
            .collect(),
    )
}

/// Equal-distance chamfer of the edges nearest to the given points. Each edge is cut with a
/// prism whose only face inside the body is the chamfer plane (so neighbouring chamfers meet in
/// a mitre); the local operation is the fallback.
pub fn chamfer(body: &Body, edges: &[Vec3], distance: f64) -> Result<Body> {
    body.require_brep("chamfer")?;
    if !(distance.is_finite() && distance > 1e-6 && distance < 1e6) {
        return Err(KernelError::Invalid("chamfer size must be positive".into()));
    }
    if edges.is_empty() || edges.len() > 1000 {
        return Err(KernelError::Invalid("select 1…1000 edges".into()));
    }
    // Each step's result carries its history over `[body]`.
    let counts = record::counts(&[body]);
    // All cutters at once (their union mitres the corners), else one edge at a time.
    let tools: Result<Vec<Cutter>> = edges.iter().enumerate().map(|(i, p)| chamfer_tool(body, *p, distance, i)).collect();
    if let Ok(tools) = tools {
        // The union so far, with its history over the cutters it joined.
        let mut acc: Option<(Body, Option<History>)> = None;
        let mut ok = true;
        for t in &tools {
            let next = match &acc {
                None => Some((t.body.clone(), Some(History::identity(&t.body)))),
                Some((a, h)) => crate::ops::boolean(a, &t.body, crate::BoolOp::Union).ok().flatten().map(|u| {
                    let h = crate::boolean_record::through(&u, 0, h.as_ref());
                    (u, h)
                }),
            };
            if next.is_none() {
                ok = false;
                break;
            }
            acc = next;
        }
        if ok
            && let Some((tool, h)) = acc
            && let Ok(Some(r)) = crate::ops::boolean(body, &tool, crate::BoolOp::Cut)
        {
            let h = cut_history(crate::boolean_record::through(&r, 1, h.as_ref()), body, &tools);
            return Ok(r.with_checked_history(h, &counts));
        }
    }
    // At a pocket rim the cutters cannot extend beyond the selected edges. Rebuild
    // the corners locally, allowing adjacent chamfers to reach different depths
    // on their common wall seam.
    if edges.len() > 1
        && let Ok(result) = crate::blend_corner::chamfer(body, edges, distance)
    {
        return Ok(result);
    }
    // Coplanar neighbours as one face (as after a join), so corners look as they should.
    let mut cur = crate::boolean_record::healed(body)?;
    if let Some(groups) = loop_groups(&cur, edges)? {
        for g in groups {
            let size = cur.size();
            let solid = cur.deep_copy();
            let ids = edge_ids(&cur, &solid, &g)?;
            match crate::loopblend::loop_blend(&solid, &ids, size, distance, false) {
                Some(res) => {
                    let (made, rec) = guard("chamfer", || res)?;
                    cur = record::after(record::rebuilt(made, &[&solid], &rec)?, &cur, &counts);
                }
                None => return Err(unsupported("the edges are not a whole loop of a planar face")),
            }
        }
        return Ok(cur);
    }
    for p in edges {
        let step = match chamfer_tool(&cur, *p, distance, 0).and_then(|t| cut_one(&cur, &t)) {
            Ok(Some(b)) => b,
            _ => blend(&cur, std::slice::from_ref(p), distance, Shape::Flat, "chamfer")?,
        };
        cur = record::after(step, &cur, &counts);
    }
    Ok(cur)
}

/// `cur` cut by one cutter, with its history over `[cur]` (`None`: nothing left).
fn cut_one(cur: &Body, t: &Cutter) -> Result<Option<Body>> {
    let counts = record::counts(&[cur]);
    Ok(crate::ops::boolean(cur, &t.body, crate::BoolOp::Cut)?.map(|r| {
        let h = cut_history(r.history().cloned(), cur, std::slice::from_ref(t));
        r.with_checked_history(h, &counts)
    }))
}

/// A cut of `cur` by cutters (their union), from its history `h` over `[cur, cutter 0, …]`, as
/// over `[cur]`: each cutter's chamfer face is made in place of its edge, and nothing else of a
/// cutter may be left (`None` where something is).
fn cut_history(h: Option<History>, cur: &Body, tools: &[Cutter]) -> Option<History> {
    let parts: Vec<History> = std::iter::once(History::identity(cur)).chain(tools.iter().map(Cutter::history)).collect();
    crate::boolean_record::composed(h?.over(&parts.iter().collect::<Vec<_>>()))
}

/// Geometry of a straight convex edge between two planar faces.
struct EdgeFrame {
    p0: Vec3,
    p1: Vec3,
    d: Vec3,
    n1: Vec3,
    n2: Vec3,
    t1: Vec3,
    t2: Vec3,
}

/// The edge nearest `p` (its index in `cur`'s edge order) and its frame.
fn edge_frame(cur: &Body, p: Vec3) -> Result<(usize, EdgeFrame)> {
    let size = cur.size();
    let tol = (size * 2e-3).max(1e-3);
    let Some((idx, dist)) = cur.nearest_edge(p, tol)? else { return Err(KernelError::Invalid("the body has no edges".into())) };
    if dist > size * 0.05 + 1e-3 {
        return Err(KernelError::Invalid(format!("no edge near {:?}", [p.x, p.y, p.z])));
    }
    let solid = &*cur.solid;
    let edge = Body::unique_edges(solid).into_iter().nth(idx).ok_or_else(|| KernelError::Invalid("edge index".into()))?;
    if !matches!(edge.curve(), mt::Curve::Line(_)) {
        return Err(unsupported("only straight edges"));
    }
    let faces: Vec<&mt::Face> = solid.face_iter().filter(|f| f.edge_iter().any(|e| e.id() == edge.id())).collect();
    let [f1, f2] = faces[..] else { return Err(unsupported("the edge is not between two faces")) };
    let (Some(n1), Some(n2)) = (plane_normal(f1), plane_normal(f2)) else { return Err(unsupported("both faces next to the edge must be planar")) };
    let (p0, p1) = (vtx(edge.absolute_front()), vtx(edge.absolute_back()));
    let d = (p1 - p0).normalized().ok_or_else(|| KernelError::Failed("zero-length edge".into()))?;
    let mut t1 = n1.cross(d);
    if t1.dot(n2) > 0.0 {
        t1 = -t1;
    }
    let mut t2 = n2.cross(d);
    if t2.dot(n1) > 0.0 {
        t2 = -t2;
    }
    if n1.dot(t2) > 1e-9 || n2.dot(t1) > 1e-9 || t1.dot(t2) < -1.0 + 1e-6 {
        return Err(unsupported("only convex edges"));
    }
    Ok((idx, EdgeFrame { p0, p1, d, n1, n2, t1, t2 }))
}

/// The prism that cuts a chamfer of size `s` on the edge nearest `p`. `k` varies the parts that
/// stay outside the body so that neighbouring cutters don't share edges.
/// The second side of an unequal chamfer: a distance along the other face, or the angle the
/// chamfer makes with the first face.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChamferSide {
    Distance(f64),
    Angle(f64),
}

/// A chamfer with different setbacks on the two faces of each (straight) edge: `d1` along the
/// first face (the one facing up most, the other with `flip`), the second side by distance or
/// angle.
pub fn chamfer_sides(body: &Body, edges: &[Vec3], d1: f64, side: ChamferSide, flip: bool) -> Result<Body> {
    body.require_brep("chamfer")?;
    let ok = |x: f64| x.is_finite() && x > 1e-6 && x < 1e6;
    let valid = ok(d1)
        && match side {
            ChamferSide::Distance(d) => ok(d),
            ChamferSide::Angle(a) => a.is_finite() && a > 1e-3 && a < std::f64::consts::PI - 1e-3,
        };
    if !valid {
        return Err(KernelError::Invalid("chamfer sizes must be positive (the angle within 0°…180°)".into()));
    }
    if edges.is_empty() || edges.len() > 1000 {
        return Err(KernelError::Invalid("select 1…1000 edges".into()));
    }
    let counts = record::counts(&[body]);
    let mut cur = crate::boolean_record::operand(body, 0, &counts);
    for (k, p) in edges.iter().enumerate() {
        let t = chamfer_tool_sides(&cur, *p, d1, Some((side, flip)), k)?;
        let step = cut_one(&cur, &t)?.ok_or_else(|| KernelError::Failed("the chamfer removed everything".into()))?;
        cur = record::after(step, &cur, &counts);
    }
    Ok(cur)
}

/// The prism that cuts a chamfer: the body, which of its faces is the chamfer plane, and the
/// edge (of the body it chamfers, in its edge order) that face is made in place of.
pub(crate) struct Cutter {
    body: Body,
    face: Option<usize>,
    edge: usize,
}

impl Cutter {
    /// The cutter as made from the body it chamfers: its chamfer face in place of the edge, its
    /// other faces (all outside that body) from nothing of it.
    fn history(&self) -> History {
        let faces = (0..self.body.face_count())
            .map(|i| record::generated(if Some(i) == self.face { vec![FaceOrigin::Edge { operand: 0, edge: self.edge }] } else { Vec::new() }))
            .collect();
        record::from_faces(&self.body.solid, 1, faces)
    }
}

pub(crate) fn chamfer_tool(cur: &Body, p: Vec3, s: f64, k: usize) -> Result<Cutter> {
    chamfer_tool_sides(cur, p, s, None, k)
}

fn chamfer_tool_sides(cur: &Body, p: Vec3, s: f64, sides: Option<(ChamferSide, bool)>, k: usize) -> Result<Cutter> {
    let (edge, mut f) = edge_frame(cur, p)?;
    // Setbacks along each face: equal, or by the second side's rule.
    let (s1, s2) = match sides {
        None => (s, s),
        Some((side, flip)) => {
            // Side 1: the face facing up most (then +y, +x), unless flipped.
            let up = Vec3::new(1e-6, 1e-3, 1.0);
            if (f.n2.dot(up) > f.n1.dot(up)) != flip {
                std::mem::swap(&mut f.n1, &mut f.n2);
                std::mem::swap(&mut f.t1, &mut f.t2);
            }
            let s2 = match side {
                ChamferSide::Distance(d) => d,
                ChamferSide::Angle(a) => {
                    // The triangle edge–A–B: angle φ at the edge, `a` at A (on face 1).
                    let phi = f.t1.dot(f.t2).clamp(-1.0, 1.0).acos();
                    let den = (phi + a).sin();
                    if den <= 1e-9 {
                        return Err(KernelError::Invalid("that chamfer angle runs past the other face".into()));
                    }
                    s * a.sin() / den
                }
            };
            (s, s2)
        }
    };
    let s = s1.max(s2);
    let size = cur.size();
    let len = f.p0.dist(f.p1);
    let ext = (s * 2.0).max(size * 0.01) * (1.0 + 0.093 * (k % 5) as f64);
    // Cross-section at p0 - d*ext, in a plane with normal d.
    let base = f.p0 - f.d * ext;
    let plane = solidcraft_geom::Plane::from_normal(base, f.d).ok_or_else(|| KernelError::Failed("chamfer plane".into()))?;
    let a = base + f.t1 * s1;
    let b = base + f.t2 * s2;
    let ab = (a - b).normalized().ok_or_else(|| KernelError::Failed("chamfer".into()))?;
    let out = (f.n1 + f.n2).normalized().ok_or_else(|| KernelError::Failed("chamfer".into()))?;
    let vary = 1.0 + 0.137 * (k % 7) as f64;
    let reach = (s * 4.0 + size * 0.02) * vary;
    let a_ext = a + ab * (s * 0.5 + reach * 0.05);
    let b_ext = b - ab * (s * 0.5 + reach * 0.07);
    let e_far = base + out * reach;
    // The material beyond both ends must be empty (the cutter runs past them).
    let mid = (a + b + base) / 3.0;
    for probe in [mid + f.d * (ext * 0.5), mid + f.d * (len + ext * 1.5)] {
        if cur.contains(probe) {
            return Err(unsupported("chamfers ending at a concave corner"));
        }
    }
    let loc: Vec<solidcraft_geom::Vec2> = [a_ext, b_ext, e_far].iter().map(|q| plane.to_local(*q)).collect();
    let mut lp = solidcraft_geom::Loop2::polygon(&loc);
    // The chamfer plane is swept from the segment a → b: first, or last once turned round.
    let mut chamfer_segment = 0;
    if lp.signed_area() < 0.0 {
        lp = lp.reversed();
        chamfer_segment = lp.segs.len().saturating_sub(1);
    }
    let region = solidcraft_geom::Region2 { outer: lp, holes: vec![] };
    let body = crate::build::extrude(&plane, &[region], 0.0, len + 2.0 * ext)?.pop().ok_or_else(|| KernelError::Failed("chamfer tool".into()))?;
    let face = body.history().and_then(|h| h.faces.iter().position(|f| f.origins == [FaceOrigin::Profile { segment: chamfer_segment }]));
    Ok(Cutter { body, face, edge })
}

#[cfg(test)]
mod tests {
    use super::*;
    use solidcraft_geom::{Loop2, Plane, Region2, Vec2};

    #[test]
    fn chamfer_extension_cannot_cross_a_hole_and_reenter_the_face() {
        let rectangle = |a, b, c, d| Loop2::polygon(&[Vec2::new(a, b), Vec2::new(c, b), Vec2::new(c, d), Vec2::new(a, d)]);
        let region =
            Region2 { outer: rectangle(0.0, 0.0, 10.0, 10.0), holes: vec![rectangle(4.0, 4.0, 6.0, 6.0), Loop2::circle(Vec2::new(4.0, 1.5), 0.25)] };
        for plane in [Plane::XY, Plane::XZ] {
            let face = crate::build::face(&plane, &region).unwrap();
            let p = plane.to_world(Vec2::new(4.0, 4.0));
            let fits = |y| extension_fits(&face, p, plane.to_world(Vec2::new(4.0, y)), 1e-6);
            assert!(fits(2.0));
            assert!(!fits(1.5)); // endpoint in the circular hole
            assert!(!fits(0.5)); // endpoint in material, but the extension crossed the hole
            assert!(!fits(-1e8)); // an arbitrarily long slide cannot escape the face
        }
    }

    /// A box with a cylinder joined across one side (#556): the bottom edge of that side runs
    /// into the cylinder, which is curved where the edge ends.
    fn box_and_boss() -> Body {
        let b = crate::build::box_solid(Vec3::ZERO, Vec3::new(40.0, 40.0, 20.0)).unwrap();
        let c = crate::build::cylinder(Vec3::new(40.0, 20.0, 0.0), Vec3::Z, 12.0, 40.0).unwrap();
        crate::ops::boolean(&b, &c, crate::BoolOp::Union).unwrap().unwrap()
    }

    #[test]
    fn a_blend_runs_into_a_cylinder_at_its_end() {
        let body = box_and_boss();
        let vol = |b: &Body| b.tessellate(1e-4).unwrap().measure().volume;
        let v0 = vol(&body);
        // The edge (40, 0..8, 0) ends in the cylinder wall at y = 8.
        let len = 20.0 - (144.0f64 - 0.0).sqrt();
        for (shape, size, area) in [(Shape::Round, 0.5, (1.0 - std::f64::consts::PI / 4.0) * 0.25), (Shape::Flat, 0.5, 0.125)] {
            let out = blend(&body, &[Vec3::new(40.0, 4.0, 0.0)], size, shape, "blend").unwrap();
            assert!(out.validity().is_empty(), "{:?}", out.validity());
            let removed = v0 - vol(&out);
            // The section runs on a little past y = 8 under the cylinder wall.
            assert!(removed > area * len * 0.99 && removed < area * (len + 0.02), "{removed} vs {}", area * len);
            assert_eq!(out.face_count(), body.face_count() + 1);
        }
    }

    #[test]
    fn a_blend_ends_on_a_hole_wall() {
        // A plate with a hole cutting through the middle of its top front edge: each piece of
        // that edge ends on the hole's wall.
        let plate = crate::build::box_solid(Vec3::ZERO, Vec3::new(40.0, 20.0, 10.0)).unwrap();
        let hole = crate::build::cylinder(Vec3::new(20.0, 0.0, -1.0), Vec3::Z, 5.0, 12.0).unwrap();
        let body = crate::ops::boolean(&plate, &hole, crate::BoolOp::Cut).unwrap().unwrap();
        let vol = |b: &Body| b.tessellate(1e-4).unwrap().measure().volume;
        let v0 = vol(&body);
        for (shape, area) in [(Shape::Round, (1.0 - std::f64::consts::PI / 4.0)), (Shape::Flat, 0.5)] {
            let out = blend(&body, &[Vec3::new(5.0, 0.0, 10.0)], 1.0, shape, "blend").unwrap();
            assert!(out.validity().is_empty(), "{:?}", out.validity());
            let removed = v0 - vol(&out);
            assert!(removed > area * 14.0 && removed < area * 15.2, "{removed}");
        }
    }

    #[test]
    fn a_chamfer_ends_across_two_faces() {
        // A plate's outline chamfered, then the mitre at a corner chamfered: its low end meets
        // both side faces, and the chamfer plane cuts back the vertical edge between them.
        let k = 5.0;
        let b = crate::build::box_solid(Vec3::ZERO, Vec3::new(100.0, 50.0, 20.0)).unwrap();
        let outline = [Vec3::new(50.0, 0.0, 20.0), Vec3::new(100.0, 25.0, 20.0), Vec3::new(50.0, 50.0, 20.0), Vec3::new(0.0, 25.0, 20.0)];
        let c = chamfer(&b, &outline, k).unwrap();
        let out = blend(&c, &[Vec3::new(k / 2.0, k / 2.0, 20.0 - k / 2.0)], 1.0, Shape::Flat, "chamfer").unwrap();
        assert!(out.validity().is_empty(), "{:?}", out.validity());
        assert_eq!(out.lumps().unwrap().len(), 1);
        assert_eq!(out.face_count(), c.face_count() + 1);
        let vol = |b: &Body| b.tessellate(1e-4).unwrap().measure().volume;
        // About the triangle (sides 1 and 1 at 120°) along the mitre; its ends slant.
        let removed = vol(&c) - vol(&out);
        let along = 0.5 * 120f64.to_radians().sin() * k * 3f64.sqrt();
        assert!(removed > along * 0.8 && removed < along * 1.2, "{removed} vs {along}");
    }

    #[test]
    fn a_curved_edge_blend_ends_in_a_face_at_an_angle() {
        // A groove of radius 8 along y with its axis 4 below the top: the arcs where it meets the
        // front and back faces end in the top face at 60° to it.
        let b = crate::build::box_solid(Vec3::ZERO, Vec3::new(40.0, 20.0, 20.0)).unwrap();
        let c = crate::build::cylinder(Vec3::new(20.0, -1.0, 16.0), Vec3::Y, 8.0, 22.0).unwrap();
        let body = crate::ops::boolean(&b, &c, crate::BoolOp::Cut).unwrap().unwrap();
        let vol = |b: &Body| b.tessellate(1e-4).unwrap().measure().volume;
        let v0 = vol(&body);
        for shape in [Shape::Round, Shape::Flat] {
            let out = blend(&body, &[Vec3::new(20.0, 0.0, 8.0)], 0.5, shape, "blend").unwrap();
            assert!(out.validity().is_empty(), "{:?}", out.validity());
            // One blend face per piece of the arc (the groove's surface is in two faces).
            assert_eq!(out.face_count(), body.face_count() + 2);
            // About the section along the arc (240°, at the section's distance from the axis).
            let area = if shape == Shape::Round { (1.0 - std::f64::consts::PI / 4.0) * 0.25 } else { 0.125 };
            let along = 8.2 * 240f64.to_radians();
            let removed = v0 - vol(&out);
            assert!(removed > area * along * 0.9 && removed < area * along * 1.1, "{removed} vs {}", area * along);
        }
    }

    #[test]
    fn a_blend_of_an_edge_running_into_a_rounded_corner_follows_it() {
        // A box with one vertical edge rounded: a top edge meeting that round smoothly is blended
        // with the round's top arc and the next top edge, as one smooth chain.
        let b = crate::build::box_solid(Vec3::ZERO, Vec3::new(40.0, 30.0, 20.0)).unwrap();
        let rounded = blend(&b, &[Vec3::new(40.0, 30.0, 10.0)], 5.0, Shape::Round, "fillet").unwrap();
        for shape in [Shape::Round, Shape::Flat] {
            let out = blend(&rounded, &[Vec3::new(20.0, 30.0, 20.0)], 1.0, shape, "blend").unwrap();
            assert!(out.validity().is_empty(), "{:?}", out.validity());
            assert_eq!(out.face_count(), rounded.face_count() + 3);
        }
    }

    #[test]
    fn local_chamfer_intersects_sloped_end_planes() {
        for slope in [-0.5, 0.5] {
            let region = Region2 {
                outer: Loop2::polygon(&[Vec2::ZERO, Vec2::new(40.0, 0.0), Vec2::new(40.0 - slope * 20.0, 20.0), Vec2::new(slope * 20.0, 20.0)]),
                holes: vec![],
            };
            let body = crate::extrude(&Plane::XY, &[region], 0.0, 20.0).unwrap().pop().unwrap();
            let pick = [Vec3::new(20.0, 0.0, 20.0)];
            let result = blend(&body, &pick, 2.0, Shape::Flat, "chamfer").unwrap();
            let removed = 2.0 * (40.0 - 2.0 * slope * 2.0 / 3.0);
            let actual = crate::measure(&body).unwrap().volume - crate::measure(&result).unwrap().volume;
            assert!((actual - removed).abs() < 1e-6, "{actual} vs {removed}");
            assert!(result.validity().is_empty());
            // The new corner vertices lie on both original end planes.
            for point in [Vec3::new(slope * 2.0, 2.0, 20.0), Vec3::new(40.0 - slope * 2.0, 2.0, 20.0)] {
                assert!(result.solid.vertex_iter().any(|v| vtx(&v).dist(point) < 1e-8));
            }
            assert!(blend(&body, &pick, 25.0, Shape::Flat, "chamfer").is_err());
            // A fillet ends in each sloped end plane along an ellipse: the removed section's
            // length is taken at its centroid, c from the edge.
            let rounded = blend(&body, &pick, 2.0, Shape::Round, "fillet").unwrap();
            let (area, c) =
                ((1.0 - std::f64::consts::PI / 4.0) * 4.0, 2.0 * (10.0 - 3.0 * std::f64::consts::PI) / (3.0 * (4.0 - std::f64::consts::PI)));
            let removed = area * (40.0 - 2.0 * slope * c);
            let actual = crate::measure(&body).unwrap().volume - crate::measure(&rounded).unwrap().volume;
            // `measure` reads a round about 6e-4 heavy, square ends too.
            assert!((actual - removed).abs() < 1e-3 * removed, "{actual} vs {removed}");
            assert!(rounded.validity().is_empty());
        }
    }
}
