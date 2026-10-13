//! Rolling-ball blends along a closed chain of smooth edges between two smooth groups of faces
//! of any surfaces (a branch pipe meeting a main pipe, a boss on a cylinder). A ball of the
//! blend's radius rolls along the edge touching both sides; its centre traces the spine and its
//! contacts the curves where the blend meets each side. The blend face is a rational B-spline
//! through the circular cross-sections (exact arcs at the samples), the side faces are trimmed
//! back to the contact curves, and seams that met the edge (lines, or arcs of an end face) are
//! shortened to the contacts.

use std::collections::HashMap;

use solidcraft_geom::Vec3;
use truck_modeling::{self as mt, builder};

use crate::blend::too_large;
use crate::body::{Solid, from_p3, p3};
use crate::record::Rebuild;
use crate::{KernelError, Result};

fn unsupported(msg: &str) -> KernelError {
    KernelError::Failed(format!("not supported yet: {msg}"))
}

/// Samples per edge for the contact curves and the blend surface.
const SAMPLES: usize = 24;
const MAX_SAMPLES: usize = 369;
const MAX_CHAIN_SAMPLES: usize = 100_000;
/// Sections past a slanted end of an open chain.
const EXTENSION: usize = 8;

struct Fit {
    surface: mt::Surface,
    contacts: [mt::Curve; 2],
    /// Parameter range belonging to this edge (the fit includes neighbouring sections).
    range: (f64, f64),
}

struct Guide {
    sections: Vec<Section>,
    lengths: Vec<f64>,
}

impl Guide {
    fn new(sections: Vec<Section>) -> Result<Self> {
        let mut lengths = vec![0.0];
        let mut total = 0.0;
        for pair in sections.windows(2) {
            let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else { continue };
            total += a.c.dist(b.c);
            lengths.push(total);
        }
        if !total.is_finite() || total <= 1e-12 {
            return Err(unsupported("degenerate rolling-ball spine"));
        }
        Ok(Self { sections, lengths })
    }

    fn anchor(&self, along: f64) -> Option<(Vec3, Vec3)> {
        if self.lengths.len() < 2 || !along.is_finite() || !(0.0..=1.0).contains(&along) {
            return None;
        }
        let distance = self.lengths.last()? * along;
        let hi = self.lengths.partition_point(|length| *length < distance).clamp(1, self.lengths.len().checked_sub(1)?);
        let (a, b) = (self.sections.get(hi - 1)?, self.sections.get(hi)?);
        let (lo, hi_length) = (self.lengths.get(hi - 1)?, self.lengths.get(hi)?);
        let span = hi_length - lo;
        if span <= 1e-12 {
            return None;
        }
        let fraction = (distance - lo) / span;
        Some((a.c + (b.c - a.c) * fraction, (b.c - a.c).normalized()?))
    }
}

/// One interpolation for both the face and its boundary curves. Fitting the contacts
/// independently gives different end conditions and leaves gaps between the same faces.
fn fit_sections(sections: &[Vec<Section>], k: usize, closed: bool, round: bool) -> Result<Fit> {
    let fail = |m: &str| KernelError::Failed(format!("curve blend: {m}"));
    let n = sections.len();
    let list = sections.get(k).ok_or_else(|| fail("section"))?;
    let mut before = Vec::new();
    if closed || k > 0 {
        let prev = sections.get((k + n - 1) % n).ok_or_else(|| fail("section"))?;
        before.extend(prev.iter().rev().skip(1).take(2).rev().copied());
    }
    let mut after = Vec::new();
    if closed || k + 1 < n {
        let next = sections.get((k + 1) % n).ok_or_else(|| fail("section"))?;
        after.extend(next.iter().skip(1).take(2).copied());
    }
    fit_strip(list, &before, &after, round)
}

/// [`fit_sections`] with the neighbouring sections given: best spaced as the strip's own, since
/// the interpolation's parameters are uniform.
fn fit_strip(list: &[Section], before: &[Section], after: &[Section], round: bool) -> Result<Fit> {
    use mt::Cut;
    let fail = |m: &str| KernelError::Failed(format!("curve blend: {m}"));
    let mut ext = Vec::with_capacity(list.len() + before.len() + after.len());
    ext.extend(before.iter().copied());
    ext.extend(list.iter().copied());
    ext.extend(after.iter().copied());
    let before = before.len();
    let ctrl: Vec<[mt::Vector4; 3]> = ext.iter().map(|s| section_controls(s, round)).collect::<Option<_>>().ok_or_else(|| fail("section"))?;
    let mut rows = Vec::with_capacity(3);
    let mut vknots = None;
    for j in 0..3 {
        let xyz: Vec<Vec3> = ctrl.iter().filter_map(|c| c.get(j)).map(|c| Vec3::new(c.x, c.y, c.z)).collect();
        let ww: Vec<Vec3> = ctrl.iter().filter_map(|c| c.get(j)).map(|c| Vec3::new(c.w, 0.0, 0.0)).collect();
        let bx = crate::build::interpolate_cubic(&xyz).ok_or_else(|| fail("blend surface"))?;
        let bw = crate::build::interpolate_cubic(&ww).ok_or_else(|| fail("blend surface"))?;
        vknots = Some(bx.knot_vec().clone());
        rows.push(bx.control_points().iter().zip(bw.control_points()).map(|(p, w)| mt::Vector4::new(p.x, p.y, p.z, w.x)).collect::<Vec<_>>());
    }
    let vk = vknots.ok_or_else(|| fail("knots"))?;
    let den = ext.len().saturating_sub(1) as f64;
    if den <= 0.0 {
        return Err(fail("empty sections"));
    }
    let range = (before as f64 / den, (before + list.len().saturating_sub(1)) as f64 / den);
    let contact = |row: usize| -> Result<mt::Curve> {
        let points = rows.get(row).cloned().ok_or_else(|| fail("contact row"))?;
        let bs = mt::BSplineCurve::try_new(vk.clone(), points).map_err(|e| fail(&e.to_string()))?;
        let mut curve = mt::NurbsCurve::new(bs);
        if range.0 > 0.0 {
            curve = curve.cut(range.0);
        }
        if range.1 < 1.0 {
            let _ = curve.cut(range.1);
        }
        Ok(mt::Curve::NurbsCurve(curve))
    };
    let contacts = [contact(0)?, contact(2)?];
    let bs = mt::BSplineSurface::try_new((mt::KnotVec::bezier_knot(2), vk), rows).map_err(|e| fail(&e.to_string()))?;
    Ok(Fit { surface: mt::Surface::NurbsSurface(mt::NurbsSurface::new(bs)), contacts, range })
}

/// Compare the fitted strip with a solved circular section between interpolation nodes.
/// This checks the supporting contacts as well as the middle of the fillet band.
fn section_fits(fit: &Fit, section: &Section, along: f64, round: bool, sigma: f64, tolerance: f64) -> bool {
    use mt::{ParametricSurface, ParametricSurface3D, SearchNearestParameter};
    let Some(ctrl) = section_controls(section, round) else { return false };
    let v = fit.range.0 + (fit.range.1 - fit.range.0) * along;
    for u in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let [a, b, c] = ctrl;
        let h = a * (1.0 - u) * (1.0 - u) + b * (2.0 * u * (1.0 - u)) + c * u * u;
        if !h.w.is_finite() || h.w <= 1e-12 {
            return false;
        }
        let exact = Vec3::new(h.x / h.w, h.y / h.w, h.z / h.w);
        // The input intersection edge may use piecewise leader-curve parameters. A
        // parameter-speed change is not a geometric error: compare nearest points, with
        // contacts constrained to the corresponding boundary and all probes to the strip.
        let (probe_u, probe_v) = if u == 0.0 || u == 1.0 {
            let Some(contact) = fit.contacts.get(usize::from(u == 1.0)) else { return false };
            let Some(parameter) = contact.search_nearest_parameter(p3(exact), v, 32) else { return false };
            (u, parameter.clamp(fit.range.0, fit.range.1))
        } else {
            let Some((pu, pv)) = fit.surface.search_nearest_parameter(p3(exact), Some((u, v)), 32) else { return false };
            (pu.clamp(0.0, 1.0), pv.clamp(fit.range.0, fit.range.1))
        };
        let actual = from_p3(fit.surface.subs(probe_u, probe_v));
        if !actual.is_finite() || actual.dist(exact) > tolerance {
            return false;
        }
        if round && (u == 0.0 || u == 1.0) {
            let Some(want) = ((section.c - exact) * sigma).normalized() else { return false };
            let normal = fit.surface.normal(probe_u, probe_v);
            let Some(normal) = Vec3::new(normal.x, normal.y, normal.z).normalized() else { return false };
            if normal.dot(want).abs() < 0.5f64.to_radians().cos() {
                return false;
            }
        }
    }
    true
}

/// Where a blend strip run on past a chain end meets the end face: the contacts' parameters
/// there and the end curve from contact 1 to contact 2 (fitted through points where the
/// strip's cross-section curves meet the face's surface, and checked against both).
fn end_cap(fit: &Fit, g: &mt::Face, at: &Section, round: bool, tolerance: f64) -> Result<(f64, f64, mt::Curve)> {
    use mt::{ParametricCurve, ParametricSurface, SearchNearestParameter};
    let fail = || unsupported("the blend's end on its end face");
    let (surf, gs) = (&fit.surface, g.surface());
    let mid = if round { at.c + ((at.q1 - at.c) + (at.q2 - at.c)).normalized().ok_or_else(fail)? * at.c.dist(at.q1) } else { (at.q1 + at.q2) * 0.5 };
    let (_, v_start) = surf.search_nearest_parameter(p3(mid), None, 100).ok_or_else(fail)?;
    // S(u, v) = G(a, b) for v, a, b by Newton from v.
    let meet = |u: f64, v: f64| -> Option<f64> {
        let mut v = v;
        let (mut a, mut b) = gs.search_nearest_parameter(surf.subs(u, v), None, 100)?;
        for _ in 0..60 {
            let f = from_p3(surf.subs(u, v)) - from_p3(gs.subs(a, b));
            if !f.is_finite() {
                return None;
            }
            if f.len() <= tolerance * 1e-3 {
                return (-1e-9..=1.0 + 1e-9).contains(&v).then_some(v.clamp(0.0, 1.0));
            }
            let (sv, ga, gb) = (surf.vder(u, v), gs.uder(a, b), gs.vder(a, b));
            let m = [[sv.x, -ga.x, -gb.x], [sv.y, -ga.y, -gb.y], [sv.z, -ga.z, -gb.z]];
            let [dv, da, db] = crate::polyhedron::solve3_pub(m, [-f.x, -f.y, -f.z])?;
            // Keep each step within the strip's parameters.
            let k = if dv.abs() > 0.1 { 0.1 / dv.abs() } else { 1.0 };
            v += dv * k;
            a += da * k;
            b += db * k;
        }
        None
    };
    for m in [16usize, 32, 64] {
        let mut pts = Vec::with_capacity(m + 1);
        let mut vs = Vec::with_capacity(m + 1);
        // From the middle out to both contacts, each search starting where the last ended.
        let half = m / 2;
        let mid_v = meet(0.5_f64.min(half as f64 / m as f64), v_start).ok_or_else(fail)?;
        let mut found = vec![None; m + 1];
        if let Some(slot) = found.get_mut(half) {
            *slot = Some(mid_v);
        }
        let mut last = mid_v;
        for i in (0..half).rev() {
            last = meet(i as f64 / m as f64, last).ok_or_else(fail)?;
            if let Some(slot) = found.get_mut(i) {
                *slot = Some(last);
            }
        }
        last = mid_v;
        for i in half + 1..=m {
            last = meet(i as f64 / m as f64, last).ok_or_else(fail)?;
            if let Some(slot) = found.get_mut(i) {
                *slot = Some(last);
            }
        }
        for (i, v) in found.iter().enumerate() {
            let v = v.ok_or_else(fail)?;
            vs.push(v);
            pts.push(from_p3(surf.subs(i as f64 / m as f64, v)));
        }
        let bs = crate::build::interpolate_cubic(&pts).ok_or_else(fail)?;
        let fits = (0..m).all(|i| {
            let q = p3(from_p3(bs.subs((i as f64 + 0.5) / m as f64)));
            let hint = vs.get(i).map(|v| ((i as f64 + 0.5) / m as f64, *v));
            let on_g = gs.search_nearest_parameter(q, None, 100).is_some_and(|(a, b)| from_p3(gs.subs(a, b)).dist(from_p3(q)) <= tolerance);
            let on_s = surf.search_nearest_parameter(q, hint, 100).is_some_and(|(u, v)| from_p3(surf.subs(u, v)).dist(from_p3(q)) <= tolerance);
            on_g && on_s
        });
        if fits {
            let (Some(t1), Some(t2)) = (vs.first(), vs.last()) else { return Err(fail()) };
            // Both contacts are cut there: strictly inside the strip's own part.
            let inside = |t: f64| t > fit.range.0 + 1e-6 && t < fit.range.1 - 1e-6;
            if !(inside(*t1) && inside(*t2)) {
                return Err(too_large("the blend is larger than the neighbouring faces"));
            }
            return Ok((*t1, *t2, mt::Curve::BSplineCurve(bs)));
        }
    }
    Err(fail())
}

/// One cross-section of the blend: ball centre and its contacts with side 1 and side 2.
#[derive(Clone, Copy, Debug)]
struct Section {
    c: Vec3,
    q1: Vec3,
    q2: Vec3,
}

/// Nearest point of a face's surface and the outward normal there.
fn project(f: &mt::Face, p: Vec3, hint: Option<(f64, f64)>) -> Option<(Vec3, Vec3, (f64, f64))> {
    use mt::{ParametricSurface, ParametricSurface3D, SearchNearestParameter};
    let s = f.oriented_surface();
    let uv = s.search_nearest_parameter(p3(p), hint, 100).or_else(|| s.search_nearest_parameter(p3(p), None, 100))?;
    let q = from_p3(s.subs(uv.0, uv.1));
    let n = s.normal(uv.0, uv.1);
    Some((q, Vec3::new(n.x, n.y, n.z).normalized()?, uv))
}

/// Whether `uv` lies within the parameters of the face's surface (unbounded directions pass).
fn in_range(f: &mt::Face, uv: (f64, f64)) -> bool {
    use mt::ParametricSurface;
    use std::ops::Bound;
    let inside = |range: (Bound<f64>, Bound<f64>), x: f64| {
        let slack = |a: f64, b: f64| (b - a).abs() * 1e-6 + 1e-9;
        match range {
            (Bound::Included(a) | Bound::Excluded(a), Bound::Included(b) | Bound::Excluded(b)) => x >= a - slack(a, b) && x <= b + slack(a, b),
            (Bound::Included(a) | Bound::Excluded(a), Bound::Unbounded) => x >= a - 1e-9,
            (Bound::Unbounded, Bound::Included(b) | Bound::Excluded(b)) => x <= b + 1e-9,
            (Bound::Unbounded, Bound::Unbounded) => true,
        }
    };
    let (ru, rv) = f.oriented_surface().parameter_range();
    uv.0.is_finite() && uv.1.is_finite() && inside(ru, uv.0) && inside(rv, uv.1)
}

/// Solve for the ball touching both sides near `p`. `extra` adds a third condition (a plane the
/// side-k contact must lie in); without it the centre stays in the plane through `p` square to
/// `t`. `sigma` is +1 when the ball sits outside the material (concave edge), −1 inside.
#[allow(clippy::too_many_arguments)]
fn solve(f1: &[&mt::Face], f2: &[&mt::Face], p: Vec3, t: Vec3, r: f64, sigma: f64, extra: Option<(usize, Vec3, Vec3)>, tol: f64) -> Option<Section> {
    // The nearest face of a side (several faces on one smooth surface group). Each face's
    // search starts from its last answer: the centre moves little between iterations. An
    // answer outside the surface's parameters, or farther than that last point (the search
    // ran away or settled on the far side), is searched again cold.
    let last = std::cell::RefCell::new(HashMap::<mt::FaceID, ((f64, f64), Vec3)>::new());
    let near = |fs: &[&mt::Face], c: Vec3| -> Option<(Vec3, Vec3)> {
        fs.iter()
            .filter_map(|f| {
                let prev = last.borrow().get(&f.id()).copied();
                let warm =
                    prev.and_then(|(uv, q)| project(f, c, Some(uv)).filter(|(x, _, uv)| in_range(f, *uv) && x.dist(c) <= q.dist(c) + tol * 1e-3));
                let (q, n, uv) = match warm {
                    Some(x) => x,
                    None => project(f, c, None)?,
                };
                last.borrow_mut().insert(f.id(), (uv, q));
                Some((q, n))
            })
            .min_by(|a, b| a.0.dist(c).total_cmp(&b.0.dist(c)))
    };
    let (q1, n1) = near(f1, p)?;
    let (q2, n2) = near(f2, p)?;
    let _ = (q1, q2);
    let den = 1.0 + n1.dot(n2);
    if den.abs() < 1e-9 {
        return None;
    }
    let mut c = p + (n1 + n2) * (sigma * r / den);
    for _ in 0..60 {
        let (q1, n1) = near(f1, c)?;
        let (q2, n2) = near(f2, c)?;
        let g1 = (c - q1).dot(n1) - sigma * r;
        let g2 = (c - q2).dot(n2) - sigma * r;
        // Third row: stay in the section plane, or put a contact on the seam plane.
        let (row3, g3) = match extra {
            None => (t, (c - p).dot(t)),
            Some((k, o, m)) => {
                let q = if k == 1 { q1 } else { q2 };
                (m, (q - o).dot(m))
            }
        };
        let a = [[n1.x, n1.y, n1.z], [n2.x, n2.y, n2.z], [row3.x, row3.y, row3.z]];
        let b = [-g1, -g2, -g3];
        let x = crate::polyhedron::solve3_pub(a, b)?;
        let dc = Vec3::new(x[0], x[1], x[2]);
        c += dc;
        if dc.len() < tol * 1e-3 && g1.abs() < tol && g2.abs() < tol && g3.abs() < tol {
            let (q1, _) = near(f1, c)?;
            let (q2, _) = near(f2, c)?;
            return Some(Section { c, q1, q2 });
        }
    }
    None
}

/// Homogeneous control points of the circular (or straight) cross-section from q1 to q2.
fn section_controls(s: &Section, round: bool) -> Option<[mt::Vector4; 3]> {
    let h = |p: Vec3, w: f64| mt::Vector4::new(p.x * w, p.y * w, p.z * w, w);
    if !round {
        let m = (s.q1 + s.q2) * 0.5;
        return Some([h(s.q1, 1.0), h(m, 1.0), h(s.q2, 1.0)]);
    }
    let (a, b) = (s.q1 - s.c, s.q2 - s.c);
    let theta = a.normalized()?.dot(b.normalized()?).clamp(-1.0, 1.0).acos();
    if !(theta > 1e-6 && theta < std::f64::consts::PI - 1e-6) {
        return None;
    }
    let half = (theta / 2.0).cos();
    let bis = (a + b).normalized()?;
    let m = s.c + bis * (a.len() / half);
    Some([h(s.q1, 1.0), h(m, half), h(s.q2, 1.0)])
}

/// A curved seam (an arc of an end face) shortened to the blend's contacts: the part of its
/// curve between the new ends `a` and `b` (`None` keeps that end), which must lie on it, in
/// order, short of each other.
fn trim_curved_seam(seam: &mt::Edge, a: Option<mt::Vertex>, b: Option<mt::Vertex>, size: f64) -> Result<mt::Edge> {
    // A closed seam (a full circle starting and ending at one vertex) has no "between": which
    // end a contact replaces, and which way round the kept piece runs, are both ambiguous.
    if seam.front() == seam.back() {
        return Err(unsupported("a closed curved edge where the blend ends"));
    }
    use mt::{BoundedCurve, Cut, SearchNearestParameter};
    let mut curve = seam.curve();
    let (lo, hi) = curve.range_tuple();
    let at = |v: &Option<mt::Vertex>, end: f64| -> Result<f64> {
        let Some(v) = v else { return Ok(end) };
        // Start from the nearest of a few samples: from the far end of a long arc
        // the search can settle on the opposite side of the circle.
        let q = from_p3(v.point());
        let hint = (0..=32)
            .map(|i| lo + (hi - lo) * i as f64 / 32.0)
            .min_by(|x, y| {
                let d = |t: f64| from_p3(mt::ParametricCurve::subs(&curve, t)).dist(q);
                d(*x).total_cmp(&d(*y))
            })
            .unwrap_or(end);
        let t = curve.search_nearest_parameter(v.point(), hint, 100).ok_or_else(|| unsupported("the blend is larger than the neighbouring faces"))?;
        if !(t.is_finite() && from_p3(mt::ParametricCurve::subs(&curve, t)).dist(from_p3(v.point())) <= size * 1e-5) {
            return Err(too_large("the blend is larger than the neighbouring faces"));
        }
        Ok(t)
    };
    let (ta, tb) = (at(&a, lo)?, at(&b, hi)?);
    let margin = (hi - lo) * 1e-9;
    if ta < lo - margin || tb > hi + margin || tb - ta < (hi - lo) * 1e-6 {
        return Err(too_large("the blend is larger than the neighbouring faces"));
    }
    if tb < hi {
        let _ = curve.cut(tb);
    }
    let piece = if ta > lo { curve.cut(ta) } else { curve };
    let a = a.unwrap_or_else(|| seam.front().clone());
    let b = b.unwrap_or_else(|| seam.back().clone());
    mt::Edge::try_new(&a, &b, piece).map_err(|e| KernelError::Failed(format!("curve blend: seam: {e}")))
}

/// Blend the closed chain of edges (ids in any order) with radius `r`; with what it built from
/// `solid` (each blend face in place of its chain edge).
pub(crate) fn curve_blend(solid: &Solid, ids: &[mt::EdgeID], size: f64, r: f64, round: bool) -> Result<(Solid, Rebuild)> {
    curve_blend_with_limit(solid, ids, size, r, round, MAX_SAMPLES)
}

fn curve_blend_with_limit(solid: &Solid, ids: &[mt::EdgeID], size: f64, r: f64, round: bool, max_samples: usize) -> Result<(Solid, Rebuild)> {
    if !(size.is_finite() && size > 0.0 && r.is_finite() && r > 0.0) {
        return Err(KernelError::Invalid("curve blend size and radius must be finite and positive".into()));
    }
    if !(4..=MAX_SAMPLES).contains(&max_samples) {
        return Err(KernelError::Invalid("curve blend section limit is outside the supported range".into()));
    }
    let tol = (size * 1e-7).max(1e-9);
    let shells = solid.boundaries();
    let (si, shell) = shells
        .iter()
        .enumerate()
        .find(|(_, sh)| sh.edge_iter().any(|e| ids.contains(&e.id())))
        .ok_or_else(|| KernelError::Failed("edge not found".into()))?;
    let faces: Vec<mt::Face> = shell.face_iter().cloned().collect();
    // Order the chain: edges as stored (absolute direction), each starting where the last ended.
    let mut chain: Vec<mt::Edge> = Vec::new();
    let all: Vec<mt::Edge> = {
        let mut seen = std::collections::HashSet::new();
        shell.edge_iter().filter(|e| ids.contains(&e.id()) && seen.insert(e.id())).map(|e| e.absolute_clone()).collect()
    };
    // An open chain starts at a vertex only one of its edges touches.
    let uses = |v: &mt::Vertex| all.iter().filter(|e| e.front() == v || e.back() == v).count();
    let first = match all.iter().find_map(|e| {
        if uses(e.front()) == 1 {
            Some(e.clone())
        } else if uses(e.back()) == 1 {
            Some(e.inverse())
        } else {
            None
        }
    }) {
        Some(e) => e,
        None => all.first().cloned().ok_or_else(|| KernelError::Failed("no edges".into()))?,
    };
    chain.push(first.clone());
    while chain.len() < all.len() {
        let end = chain.last().map(|e| e.back().clone()).ok_or_else(|| KernelError::Failed("chain".into()))?;
        let next = all
            .iter()
            .find(|e| !chain.iter().any(|c| c.id() == e.id()) && (*e.front() == end || *e.back() == end))
            .ok_or_else(|| unsupported("the edges don't form one chain"))?;
        chain.push(if *next.front() == end { next.clone() } else { next.inverse() });
    }
    let closed = chain.first().map(|e| e.front().clone()) == chain.last().map(|e| e.back().clone());
    let n = chain.len();
    // The chain's vertices (n for a closed chain, n + 1 for an open one).
    let mut verts: Vec<mt::Vertex> = chain.iter().map(|e| e.front().clone()).collect();
    if !closed && let Some(e) = chain.last() {
        verts.push(e.back().clone());
    }
    let nv = verts.len();
    // Faces of each edge, split into the two sides.
    let faces_of = |e: &mt::Edge| -> Vec<usize> {
        (0..faces.len()).filter(|i| faces.get(*i).is_some_and(|f| f.edge_iter().any(|x| x.id() == e.id()))).collect()
    };
    let mut side1: Vec<usize> = Vec::new();
    let mut side2: Vec<usize> = Vec::new();
    for (k, e) in chain.iter().enumerate() {
        let fs = faces_of(e);
        let [a, b] = fs[..] else { return Err(unsupported("an edge without two faces")) };
        if k == 0 {
            side1.push(a);
            side2.push(b);
            continue;
        }
        // The side-1 face of this edge is the one whose normal matches side 1 at the joint.
        let p = from_p3(e.front().point());
        let nrm = |fi: usize| faces.get(fi).and_then(|f| project(f, p, None)).map(|x| x.1);
        let prev1 = side1.last().copied().and_then(nrm);
        let (na, nb) = (nrm(a), nrm(b));
        let a_is_1 = match (prev1, na, nb) {
            (Some(x), Some(y), Some(z)) => x.dot(y) >= x.dot(z),
            _ => return Err(KernelError::Failed("blend: normals".into())),
        };
        if a_is_1 {
            side1.push(a);
            side2.push(b);
        } else {
            side1.push(b);
            side2.push(a);
        }
    }
    let s1: Vec<&mt::Face> = side1.iter().filter_map(|i| faces.get(*i)).collect();
    let s2: Vec<&mt::Face> = side2.iter().filter_map(|i| faces.get(*i)).collect();
    // The faces the ball may touch near edges `ks` of the chain: their own, on each side. (The
    // surface of another face of a side, such as a wall's plane past the bend it runs into,
    // may pass nearer than the face the ball actually rolls on.)
    let edge_sides = |ks: &[usize]| -> (Vec<&mt::Face>, Vec<&mt::Face>) {
        let pick = |side: &[usize]| {
            let mut out: Vec<usize> = Vec::new();
            for k in ks {
                if let Some(fi) = side.get(*k)
                    && !out.contains(fi)
                {
                    out.push(*fi);
                }
            }
            out.iter().filter_map(|i| faces.get(*i)).collect::<Vec<_>>()
        };
        (pick(&side1), pick(&side2))
    };
    // The chain's edges at vertex k.
    let vertex_edges = |k: usize| -> Vec<usize> {
        if closed {
            vec![(k + n - 1) % n, k % n]
        } else if k == 0 {
            vec![0]
        } else if k >= n {
            vec![n - 1]
        } else {
            vec![k - 1, k]
        }
    };
    // Convex or concave, from the first edge as its side-1 face runs it.
    let e0 = chain.first().ok_or_else(|| KernelError::Failed("chain".into()))?;
    let f1 = s1.first().ok_or_else(|| KernelError::Failed("side".into()))?;
    let in_f1 = f1.boundary_iters().into_iter().flatten().find(|e| e.id() == e0.id()).ok_or_else(|| KernelError::Failed("edge use".into()))?;
    let (pm, t1) = {
        use mt::{BoundedCurve, ParametricCurve};
        let c = in_f1.oriented_curve();
        let (a, b) = c.range_tuple();
        let m = (a + b) * 0.5;
        let d = c.der(m);
        (from_p3(c.subs(m)), Vec3::new(d.x, d.y, d.z).normalized().ok_or_else(|| KernelError::Failed("tangent".into()))?)
    };
    let (n1, n2) = match (project(f1, pm, None), s2.first().and_then(|f| project(f, pm, None))) {
        (Some(x), Some(y)) => (x.1, y.1),
        _ => return Err(KernelError::Failed("blend: normals".into())),
    };
    let convex = t1.dot(n1.cross(n2)) > 0.0;
    let sigma = if convex { -1.0 } else { 1.0 };
    if n1.dot(n2) > 1.0 - 1e-6 {
        return Err(unsupported("blending a smooth edge"));
    }
    // Seams at the chain's vertices: edges there that aren't in the chain, and their side.
    let chain_ids: Vec<mt::EdgeID> = chain.iter().map(|e| e.id()).collect();
    let mut seams: Vec<Vec<(usize, mt::Edge)>> = Vec::with_capacity(nv);
    // At an open chain's ends: the end face (planar, square to the edge there).
    let mut end_faces: Vec<(usize, usize)> = Vec::new(); // (vertex index, face index)
    // Chain ends whose end face is not a plane square to the edge: (vertex, face, point, tangent).
    let mut slanted: Vec<(usize, usize, Vec3, Vec3)> = Vec::new();
    for (k, v) in verts.iter().enumerate() {
        let at_end = !closed && (k == 0 || k == nv - 1);
        let (k1, k2) = {
            let ei = if k == nv - 1 && !closed { n - 1 } else { k.min(n - 1) };
            (side1.get(ei).copied().unwrap_or(usize::MAX), side2.get(ei).copied().unwrap_or(usize::MAX))
        };
        let mut found: Vec<(usize, mt::Edge)> = Vec::new();
        for (fi, f) in faces.iter().enumerate() {
            if !f.vertex_iter().any(|x| x == *v) {
                continue;
            }
            let on_side = side1.contains(&fi) || side2.contains(&fi);
            if !on_side {
                if !at_end || end_faces.iter().any(|(kk, _)| *kk == k) {
                    return Err(unsupported("a vertex of the edge chain where other faces meet"));
                }
                end_faces.push((k, fi));
                continue;
            }
            for x in f.edge_iter() {
                if chain_ids.contains(&x.id()) || !(*x.front() == *v || *x.back() == *v) || found.iter().any(|(_, y)| y.id() == x.id()) {
                    continue;
                }
                let side = if fi == k1 || (side1.contains(&fi) && fi != k2) { 1 } else { 2 };
                found.push((side, x.absolute_clone()));
            }
        }
        if at_end && !end_faces.iter().any(|(kk, _)| *kk == k) {
            return Err(unsupported("an open chain of curved edges must end at a face"));
        }
        // At most one seam per side.
        if found.iter().filter(|x| x.0 == 1).count() > 1 || found.iter().filter(|x| x.0 == 2).count() > 1 {
            return Err(unsupported("a vertex of the edge chain where several seams meet"));
        }
        seams.push(found);
    }
    // Sections: at each vertex (a contact on the seam there), then along each edge.
    let tangent_at = |e: &mt::Edge, s: f64| -> Option<(Vec3, Vec3)> {
        use mt::{BoundedCurve, ParametricCurve};
        let c = e.oriented_curve();
        let (a, b) = c.range_tuple();
        let t = a + (b - a) * s;
        let d = c.der(t);
        Some((from_p3(c.subs(t)), Vec3::new(d.x, d.y, d.z).normalized()?))
    };
    let mut vsec: Vec<Section> = Vec::with_capacity(nv);
    for k in 0..nv {
        let (p, t) = if k < n {
            tangent_at(chain.get(k).ok_or_else(|| KernelError::Failed("edge".into()))?, 0.0)
        } else {
            tangent_at(chain.get(n - 1).ok_or_else(|| KernelError::Failed("edge".into()))?, 1.0)
        }
        .ok_or_else(|| KernelError::Failed("tangent".into()))?;
        let here = seams.get(k).cloned().unwrap_or_default();
        let at_end = end_faces.iter().any(|(kk, _)| *kk == k);
        if at_end {
            // A plane square to the edge holds the blend's end section. Any other end face: the
            // blend runs on past the end and is cut back where it meets that face.
            let (_, fi) = end_faces.iter().find(|(kk, _)| *kk == k).copied().unwrap_or_default();
            let g = faces.get(fi).ok_or_else(|| KernelError::Failed("end face".into()))?;
            let square = match g.oriented_surface() {
                mt::Surface::Plane(pl) => {
                    let m = pl.normal();
                    Vec3::new(m.x, m.y, m.z).normalized().is_some_and(|ng| ng.dot(t).abs() >= 1.0 - 1e-6)
                }
                _ => false,
            };
            if !square {
                slanted.push((k, fi, p, t));
            }
        }
        let extra = match here.first().cloned().filter(|_| !at_end) {
            None => None,
            Some((side, seam)) => {
                // The plane through the seam square to that side's surface.
                let o = from_p3(seam.front().point());
                let d = (from_p3(seam.back().point()) - o).normalized().ok_or_else(|| KernelError::Failed("seam".into()))?;
                if !matches!(seam.curve(), mt::Curve::Line(_)) {
                    return Err(unsupported("a curved seam where the edge chain turns"));
                }
                let fs = if side == 1 { &s1 } else { &s2 };
                // The side's normal at the joint: from its face nearest the joint (any face of a
                // longer side is no guide here).
                let nn = fs
                    .iter()
                    .filter_map(|f| project(f, p, None))
                    .min_by(|a, b| a.0.dist(p).total_cmp(&b.0.dist(p)))
                    .map(|x| x.1)
                    .ok_or_else(|| KernelError::Failed("seam normal".into()))?;
                Some((side, o, d.cross(nn).normalized().ok_or_else(|| KernelError::Failed("seam plane".into()))?))
            }
        };
        let (a1, a2) = edge_sides(&vertex_edges(k));
        let s = solve(&a1, &a2, p, t, r, sigma, extra, tol * 100.0).ok_or_else(|| unsupported("the ball doesn't fit along the edge"))?;
        // A seam on the other side too must pass through that side's contact.
        if let Some((side, seam)) = here.get(1).filter(|_| !at_end) {
            let q = if *side == 1 { s.q1 } else { s.q2 };
            let (a, b) = (from_p3(seam.front().point()), from_p3(seam.back().point()));
            if !matches!(seam.curve(), mt::Curve::Line(_)) || q.dist_to_segment(a, b) > tol * 1e3 {
                return Err(unsupported("seams on both sides of a corner of the edge chain"));
            }
        }
        vsec.push(s);
    }
    // Past a slanted end: sections of the same rolling ball beyond the end vertex (far to near
    // at the chain's start, near to far at its end), far enough to cross the end face.
    let mut beyond: Vec<(usize, Vec<Section>)> = Vec::new();
    for (k, fi, p, t) in &slanted {
        let g = faces.get(*fi).ok_or_else(|| KernelError::Failed("end face".into()))?;
        let at = vsec.get(*k).copied().ok_or_else(|| KernelError::Failed("section".into()))?;
        let (_, ng, _) = project(g, *p, None).ok_or_else(|| unsupported("the blend's end on its end face"))?;
        let cos = ng.dot(*t).abs();
        if cos < 0.15 {
            return Err(unsupported("an end face almost along the edge"));
        }
        let width = at.q1.dist(*p).max(at.q2.dist(*p)).max(at.c.dist(*p)) + r;
        let reach = 1.5 * width / cos;
        let out = if *k == 0 { -*t } else { *t };
        let (e1, e2) = edge_sides(&vertex_edges(*k));
        let mut list = Vec::with_capacity(EXTENSION);
        for j in 1..=EXTENSION {
            let anchor = *p + out * (reach * j as f64 / EXTENSION as f64);
            list.push(
                solve(&e1, &e2, anchor, *t, r, sigma, None, tol * 100.0)
                    .ok_or_else(|| unsupported("the ball doesn't fit past the end of the edge"))?,
            );
        }
        if *k == 0 {
            list.reverse();
        }
        beyond.push((*k, list));
    }
    // Uniform refinement preserves a common spacing across neighbouring patches. Work is
    // capped: a difficult strip returns an error instead of accepting an inaccurate fit.
    let fit_tolerance = (size * 1e-5).clamp(1e-5, 0.001);
    // At a seam the contact lies on the seam plane, which need not be square to the
    // original edge. Its ball centre therefore need not lie in the edge's normal plane.
    // Interpolate that plane's displacement so the interior sections tend to the exact
    // seam sections, rather than leaving a finite jump as sampling gets finer.
    let offsets: Vec<(f64, f64)> = chain
        .iter()
        .enumerate()
        .map(|(k, edge)| {
            let (p0, t0) = tangent_at(edge, 0.0).ok_or_else(|| KernelError::Failed("tangent".into()))?;
            let (p1, t1) = tangent_at(edge, 1.0).ok_or_else(|| KernelError::Failed("tangent".into()))?;
            let start = vsec.get(k).ok_or_else(|| KernelError::Failed("section".into()))?;
            let end = vsec.get((k + 1) % nv).ok_or_else(|| KernelError::Failed("section".into()))?;
            Ok(((start.c - p0).dot(t0), (end.c - p1).dot(t1)))
        })
        .collect::<Result<_>>()?;
    let section_at = |k: usize, edge: &mt::Edge, along: f64| -> Result<Section> {
        let (p, t) = tangent_at(edge, along).ok_or_else(|| KernelError::Failed("tangent".into()))?;
        let (start, end) = offsets.get(k).copied().ok_or_else(|| KernelError::Failed("section offset".into()))?;
        let anchor = p + t * (start * (1.0 - along) + end * along);
        let (a1, a2) = edge_sides(&[k]);
        solve(&a1, &a2, anchor, t, r, sigma, None, tol * 100.0).ok_or_else(|| unsupported("the ball doesn't fit along the edge"))
    };
    let mut samples = SAMPLES.min(max_samples);
    let (esec, fits) = loop {
        if n.checked_mul(samples).is_none_or(|count| count > MAX_CHAIN_SAMPLES) {
            return Err(unsupported("curve blend exceeds the section fitting limit"));
        }
        let mut guides = Vec::with_capacity(n);
        for (k, e) in chain.iter().enumerate() {
            let mut list = vec![*vsec.get(k).ok_or_else(|| KernelError::Failed("section".into()))?];
            for j in 1..samples - 1 {
                list.push(section_at(k, e, j as f64 / (samples - 1) as f64)?);
            }
            list.push(*vsec.get((k + 1) % nv).ok_or_else(|| KernelError::Failed("section".into()))?);
            for (kv, ext) in &beyond {
                if *kv == k && k == 0 {
                    list.splice(0..0, ext.iter().copied());
                } else if *kv == k + 1 && k + 1 == nv - 1 {
                    list.extend(ext.iter().copied());
                }
            }
            guides.push(Guide::new(list)?);
        }
        // Intersection curves inherit their leader's piecewise parameter speed. Fitting
        // those parameters creates oscillations even on a geometrically smooth spine.
        // Resample by distance along the rolling centres, projecting each interpolated
        // centre back onto the exact two offset surfaces before it becomes a section.
        let uniform_section = |k: usize, along: f64| -> Result<Section> {
            let guide = guides.get(k).ok_or_else(|| KernelError::Failed("spine guide".into()))?;
            if along == 0.0 {
                return guide.sections.first().copied().ok_or_else(|| KernelError::Failed("section".into()));
            }
            if along == 1.0 {
                return guide.sections.last().copied().ok_or_else(|| KernelError::Failed("section".into()));
            }
            let (anchor, tangent) = guide.anchor(along).ok_or_else(|| unsupported("degenerate rolling-ball spine"))?;
            let (a1, a2) = edge_sides(&[k]);
            solve(&a1, &a2, anchor, tangent, r, sigma, None, tol * 100.0).ok_or_else(|| unsupported("the ball doesn't fit along the resampled spine"))
        };
        let esec: Vec<Vec<Section>> = (0..n)
            .map(|k| (0..samples).map(|j| uniform_section(k, j as f64 / (samples - 1) as f64)).collect::<Result<Vec<_>>>())
            .collect::<Result<_>>()?;
        // Each strip's neighbouring sections at its own spacing (edges of different lengths
        // have differently spaced samples, which makes a uniform interpolation overshoot).
        let length = |k: usize| guides.get(k).and_then(|g| g.lengths.last().copied()).unwrap_or(0.0);
        let mut fits = Vec::with_capacity(n);
        for k in 0..n {
            let h = length(k) / (samples - 1) as f64;
            let spaced = |nb: usize, from_end: bool| -> Result<Option<Vec<Section>>> {
                let l = length(nb);
                if !(l > 2.0 * h && h > 0.0) {
                    return Ok(None);
                }
                let mut out = Vec::with_capacity(2);
                for j in [1.0, 2.0] {
                    out.push(uniform_section(nb, if from_end { 1.0 - j * h / l } else { j * h / l })?);
                }
                if from_end {
                    out.reverse();
                }
                Ok(Some(out))
            };
            let before = if closed || k > 0 { spaced((k + n - 1) % n, true)? } else { Some(Vec::new()) };
            let after = if closed || k + 1 < n { spaced((k + 1) % n, false)? } else { Some(Vec::new()) };
            let list = esec.get(k).ok_or_else(|| KernelError::Failed("section".into()))?;
            let fit = match (before, after) {
                (Some(b), Some(a)) => crate::guard("fit curve blend", || fit_strip(list, &b, &a, round))?,
                _ => crate::guard("fit curve blend", || fit_sections(&esec, k, closed, round))?,
            };
            fits.push(fit);
        }
        let mut accurate = true;
        'edges: for k in 0..n {
            let fit = fits.get(k).ok_or_else(|| KernelError::Failed("curve blend fit".into()))?;
            for j in 0..samples - 1 {
                for fraction in [0.25, 0.5, 0.75] {
                    let along = (j as f64 + fraction) / (samples - 1) as f64;
                    let section = uniform_section(k, along)?;
                    if !crate::guard("check curve blend fit", || Ok(section_fits(fit, &section, along, round, sigma, fit_tolerance)))? {
                        accurate = false;
                        break 'edges;
                    }
                }
            }
        }
        if accurate {
            break (esec, fits);
        }
        if samples >= max_samples {
            return Err(KernelError::Failed("curve blend: could not fit the rolling-ball surface within the accuracy and section limits".into()));
        }
        samples = (2 * samples - 1).min(max_samples);
    };
    // Where each extended blend meets its slanted end face: (vertex, edge, cut parameters of
    // the two contacts, the end curve from contact 1 to contact 2).
    let mut caps: Vec<(usize, usize, f64, f64, mt::Curve)> = Vec::new();
    for (k, fi, _, _) in &slanted {
        let ke = if *k == 0 { 0 } else { n - 1 };
        let fit = fits.get(ke).ok_or_else(|| KernelError::Failed("curve blend fit".into()))?;
        let g = faces.get(*fi).ok_or_else(|| KernelError::Failed("end face".into()))?;
        let at = vsec.get(*k).copied().ok_or_else(|| KernelError::Failed("section".into()))?;
        let cap = crate::guard("curve blend end", || end_cap(fit, g, &at, round, fit_tolerance))?;
        caps.push((*k, ke, cap.0, cap.1, cap.2));
    }
    // Both ends of one edge cut back: the contacts must keep a piece between them.
    if let [(_, 0, s1, s2, _), (_, 0, e1, e2, _)] = &caps[..]
        && n == 1
        && !(s1 + 1e-6 < *e1 && s2 + 1e-6 < *e2)
    {
        return Err(too_large("the blend is larger than the neighbouring faces"));
    }
    crate::guard("curve blend", || {
        let fail = |m: &str| KernelError::Failed(format!("curve blend: {m}"));
        // Vertices and arcs at the chain's vertices.
        let mut v1: Vec<mt::Vertex> = vsec.iter().map(|s| builder::vertex(p3(s.q1))).collect();
        let mut v2: Vec<mt::Vertex> = vsec.iter().map(|s| builder::vertex(p3(s.q2))).collect();
        for (k, _, _, _, curve) in &caps {
            use mt::{BoundedCurve, ParametricCurve};
            let (lo, hi) = curve.range_tuple();
            if let (Some(a), Some(b)) = (v1.get_mut(*k), v2.get_mut(*k)) {
                *a = builder::vertex(curve.subs(lo));
                *b = builder::vertex(curve.subs(hi));
            }
        }
        let mut arcs = Vec::with_capacity(nv);
        for (k, s) in vsec.iter().enumerate() {
            let (a, b) = (v1.get(k).ok_or_else(|| fail("vertex"))?, v2.get(k).ok_or_else(|| fail("vertex"))?);
            let e = if round {
                let m = s.c + ((s.q1 - s.c) + (s.q2 - s.c)).normalized().ok_or_else(|| fail("arc"))? * r;
                builder::circle_arc(a, b, p3(m))
            } else {
                builder::line(a, b)
            };
            let e = match caps.iter().find(|c| c.0 == k) {
                Some((_, _, _, _, curve)) => mt::Edge::try_new(a, b, curve.clone()).map_err(|e| fail(&e.to_string()))?,
                None => e,
            };
            arcs.push(e);
        }
        // Contact curves and blend faces per edge.
        let mut c1: Vec<mt::Edge> = Vec::with_capacity(n);
        let mut c2: Vec<mt::Edge> = Vec::with_capacity(n);
        let mut blends: Vec<mt::Surface> = Vec::with_capacity(n);
        for (k, list) in esec.iter().enumerate() {
            let k1 = (k + 1) % nv;
            let (a1, b1, a2, b2) = (v1.get(k), v1.get(k1), v2.get(k), v2.get(k1));
            let (Some(a1), Some(b1), Some(a2), Some(b2)) = (a1, b1, a2, b2) else { return Err(fail("vertex")) };
            let fit = fits.get(k).ok_or_else(|| fail("fit"))?;
            let [cv1, cv2] = &fit.contacts;
            // Contacts past a slanted end stop at the end curve.
            let (mut cv1, mut cv2) = (cv1.clone(), cv2.clone());
            for (kv, _, t1, t2, _) in caps.iter().filter(|c| c.1 == k) {
                use mt::Cut;
                if *kv == k + 1 {
                    let _ = cv1.cut(*t1);
                    let _ = cv2.cut(*t2);
                }
            }
            for (kv, _, t1, t2, _) in caps.iter().filter(|c| c.1 == k) {
                use mt::Cut;
                if *kv == k && k == 0 {
                    cv1 = cv1.cut(*t1);
                    cv2 = cv2.cut(*t2);
                }
            }
            let e1 = mt::Edge::new(a1, b1, cv1);
            let e2 = mt::Edge::new(a2, b2, cv2);
            let mut surface = fit.surface.clone();
            // Outward: toward the ball centre on a concave edge, away on a convex one.
            {
                use mt::{ParametricSurface3D, SearchNearestParameter};
                let s = list.get(list.len() / 2).ok_or_else(|| fail("section"))?;
                let mid = s.c + ((s.q1 - s.c) + (s.q2 - s.c)).normalized().ok_or_else(|| fail("mid"))? * r;
                if let Some((u, v)) = surface.search_nearest_parameter(p3(mid), None, 100) {
                    let nn = surface.normal(u, v);
                    let want = (s.c - mid) * sigma;
                    if Vec3::new(nn.x, nn.y, nn.z).dot(want) < 0.0 {
                        surface = mt::Invertible::inverse(&surface);
                    }
                }
            }
            c1.push(e1);
            c2.push(e2);
            blends.push(surface);
        }
        // Rebuild the side faces: chain edges become contact curves, seams start at contacts.
        let mut subst: HashMap<mt::EdgeID, (mt::Edge, mt::Edge)> = HashMap::new();
        for (k, e) in chain.iter().enumerate() {
            let (Some(x), Some(y)) = (c1.get(k), c2.get(k)) else { return Err(fail("contact")) };
            subst.insert(e.id(), (x.clone(), y.clone()));
        }
        // Seams get new ends at the contacts (an edge between two chain vertices at both).
        let mut ends: HashMap<mt::EdgeID, (mt::Edge, Option<mt::Vertex>, Option<mt::Vertex>)> = HashMap::new();
        for (k, sm) in seams.iter().enumerate().flat_map(|(k, l)| l.iter().map(move |x| (k, x))) {
            let (side, seam) = sm;
            let v = verts.get(k).cloned().ok_or_else(|| fail("vertex"))?;
            let nv = if *side == 1 { v1.get(k) } else { v2.get(k) }.ok_or_else(|| fail("vertex"))?.clone();
            let slot = ends.entry(seam.id()).or_insert_with(|| (seam.clone(), None, None));
            if *seam.front() == v {
                slot.1 = Some(nv);
            } else {
                slot.2 = Some(nv);
            }
        }
        let mut seam_subst: HashMap<mt::EdgeID, mt::Edge> = HashMap::new();
        for (id, (seam, a, b)) in ends {
            // At a slanted end the blend may stop past the seam's old end: the seam then runs
            // on along its line or circle, within its planar face.
            let one_end = match (&a, &b) {
                (Some(nv), None) => Some((seam.front().clone(), nv.clone())),
                (None, Some(nv)) => Some((seam.back().clone(), nv.clone())),
                _ => None,
            };
            if let Some((old, nv)) = one_end
                && slanted.iter().any(|(k, ..)| verts.get(*k) == Some(&old))
            {
                let on_tol = (size * 1e-5).max(1e-6);
                let q = from_p3(nv.point());
                if crate::blend::on_edge(&seam, q, on_tol).is_none() {
                    let fits = faces
                        .iter()
                        .filter(|f| f.edge_iter().any(|e| e.id() == id))
                        .any(|f| crate::blend::plane_normal(f).is_some() && crate::blend::extension_fits(f, from_p3(old.point()), q, tol));
                    if !fits || !crate::blend::on_extension(&seam, q, on_tol) {
                        return Err(too_large("the blend is larger than the neighbouring faces"));
                    }
                }
                seam_subst.insert(id, crate::blend::retarget(&seam, &old, &nv, on_tol)?);
                continue;
            }
            if !matches!(seam.curve(), mt::Curve::Line(_)) {
                seam_subst.insert(id, trim_curved_seam(&seam, a, b, size)?);
                continue;
            }
            // The new ends must stay on the seam, in order, short of each other.
            let (p0, p1) = (from_p3(seam.front().point()), from_p3(seam.back().point()));
            let at = |v: &mt::Vertex| {
                let q = from_p3(v.point());
                let d = p1 - p0;
                ((q - p0).dot(d) / d.len2().max(1e-300), (q - p0 - d * ((q - p0).dot(d) / d.len2().max(1e-300))).len())
            };
            let (ta, oa) = a.as_ref().map(at).unwrap_or((0.0, 0.0));
            let (tb, ob) = b.as_ref().map(at).unwrap_or((1.0, 0.0));
            if oa > size * 1e-5 || ob > size * 1e-5 || ta < -1e-9 || tb > 1.0 + 1e-9 || tb - ta < 1e-6 {
                return Err(too_large("the blend is larger than the neighbouring faces"));
            }
            let a = a.unwrap_or_else(|| seam.front().clone());
            let b = b.unwrap_or_else(|| seam.back().clone());
            seam_subst.insert(id, builder::line(&a, &b));
        }
        let mut out: Vec<mt::Face> = Vec::new();
        let mut rec = Rebuild::default();
        rec.pieces(&seam_subst);
        for (fi, f) in faces.iter().enumerate() {
            let corners: Vec<usize> = end_faces.iter().filter(|(_, g)| *g == fi).map(|(k, _)| *k).collect();
            if !corners.is_empty() {
                // The end face (one or both ends of the chain): its two edges at a chain end now
                // stop at the contacts, with the blend's end arc between them.
                let mut wires = Vec::new();
                for w in f.absolute_boundaries() {
                    let mut es: Vec<mt::Edge> = Vec::new();
                    for e in w.edge_iter() {
                        let ne = match seam_subst.get(&e.id()) {
                            Some(s) => {
                                if e.front() == e.absolute_front() {
                                    s.clone()
                                } else {
                                    s.inverse()
                                }
                            }
                            None => e.clone(),
                        };
                        // Arriving at an old corner: continue along that end's arc.
                        let arrives = corners.iter().copied().find(|k| verts.get(*k).is_some_and(|v| *e.back() == *v));
                        es.push(ne.clone());
                        if let Some(k) = arrives {
                            let arc = arcs.get(k).ok_or_else(|| fail("arc"))?;
                            let (a1, a2) = (v1.get(k).ok_or_else(|| fail("vertex"))?, v2.get(k).ok_or_else(|| fail("vertex"))?);
                            let to = ne.back();
                            let corner = if to == a1 {
                                arc.clone()
                            } else if to == a2 {
                                arc.inverse()
                            } else {
                                return Err(fail("end face corner"));
                            };
                            es.push(corner);
                        }
                    }
                    wires.push(mt::Wire::from(es));
                }
                let rebuilt = crate::heal::absolute_face(f, wires).ok_or_else(|| fail("an end face could not be rebuilt"))?;
                rec.face(&rebuilt, f);
                out.push(rebuilt);
                continue;
            }
            let on1 = side1.contains(&fi);
            let on2 = side2.contains(&fi);
            if !on1 && !on2 && !f.edge_iter().any(|e| seam_subst.contains_key(&e.id())) {
                out.push(f.clone());
                continue;
            }
            let mut wires = Vec::new();
            for w in f.absolute_boundaries() {
                let mut es = Vec::new();
                for e in w.edge_iter() {
                    let ne = if let Some((x, y)) = subst.get(&e.id()) {
                        let base = if on1 { x } else { y };
                        // Same direction as the chain edge, or reversed.
                        let chain_e = chain.iter().find(|c| c.id() == e.id()).ok_or_else(|| fail("edge"))?;
                        if e.front() == chain_e.front() { base.clone() } else { base.inverse() }
                    } else if let Some(s) = seam_subst.get(&e.id()) {
                        if e.front() == e.absolute_front() { s.clone() } else { s.inverse() }
                    } else {
                        e.clone()
                    };
                    es.push(ne);
                }
                wires.push(mt::Wire::from(es));
            }
            let rebuilt = crate::heal::absolute_face(f, wires).ok_or_else(|| fail("a side face could not be rebuilt"))?;
            rec.face(&rebuilt, f);
            out.push(rebuilt);
        }
        // Blend faces: each runs its side-1 contact the other way from side 1's face.
        for (k, bf) in blends.iter().enumerate() {
            let (Some(e1), Some(e2), Some(ak), Some(ak1)) = (c1.get(k), c2.get(k), arcs.get(k), arcs.get((k + 1) % nv)) else {
                return Err(fail("blend"));
            };
            let f1i = side1.get(k).ok_or_else(|| fail("side"))?;
            let f1 = faces.get(*f1i).ok_or_else(|| fail("side"))?;
            let chain_e = chain.get(k).ok_or_else(|| fail("edge"))?;
            let used_fwd = f1
                .boundary_iters()
                .into_iter()
                .flatten()
                .find(|e| e.id() == chain_e.id())
                .map(|e| e.front() == chain_e.front())
                .ok_or_else(|| fail("edge use"))?;
            // Side 1 runs the contact forward (as the chain) when it ran the edge forward.
            let wire: mt::Wire = if used_fwd {
                vec![e1.inverse(), ak.clone(), e2.clone(), ak1.inverse()].into()
            } else {
                vec![e1.clone(), ak1.clone(), e2.inverse(), ak.inverse()].into()
            };
            let face = mt::Face::try_new(vec![wire], bf.clone()).map_err(|e| fail(&e.to_string()))?;
            rec.blend(&face, [chain_e.id()]);
            out.push(face);
        }
        let mut shells = solid.boundaries().clone();
        if let Some(sh) = shells.get_mut(si) {
            *sh = out.into();
        }
        Ok((Solid::try_new(shells).map_err(|e| fail(&format!("invalid solid: {e}")))?, rec))
    })
}

#[cfg(test)]
#[path = "curveblend_tests.rs"]
mod tests;
