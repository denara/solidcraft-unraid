//! Planes fitted to points around a loop (Newell's method), for planar faces of polygons read
//! from files and for patches filling a planar loop of edges, and points sampled along edges
//! (shared with the fill of a loop that is not planar).

use mt::{BoundedCurve, EuclideanSpace, InnerSpace, MetricSpace, ParametricCurve};
use solidcraft_geom::Vec3;
use truck_modeling as mt;

use crate::body::{from_p3, p3};
use crate::{KernelError, Result};

/// The plane through points round a loop: its normal by Newell's method (turning with the
/// loop), through their centroid; `None` for fewer than three points or no area.
pub(crate) fn plane_through(pts: &[mt::Point3]) -> Option<mt::Plane> {
    let n = pts.len();
    if n < 3 {
        return None;
    }
    let mut nrm = mt::Vector3::new(0.0, 0.0, 0.0);
    let mut c = mt::Vector3::new(0.0, 0.0, 0.0);
    for i in 0..n {
        let (a, b) = (pts.get(i)?, pts.get((i + 1) % n)?);
        nrm.x += (a.y - b.y) * (a.z + b.z);
        nrm.y += (a.z - b.z) * (a.x + b.x);
        nrm.z += (a.x - b.x) * (a.y + b.y);
        c += a.to_vec();
    }
    let len = nrm.magnitude();
    if !(len > 1e-12) {
        return None;
    }
    let z = nrm / len;
    let o = mt::Point3::from_vec(c / n as f64);
    let a = if z.x.abs() < 0.9 { mt::Vector3::unit_x() } else { mt::Vector3::unit_y() };
    let x = (a - z * a.dot(z)).normalize();
    let y = z.cross(x);
    Some(mt::Plane::new(o, o + x, o + y))
}

/// The area vector of a closed polygon (Newell's method): normal to the loop, as long as the area
/// it encloses, pointing the way the loop runs round by the right-hand rule.
pub(crate) fn loop_area(points: &[Vec3]) -> Vec3 {
    let mut n = Vec3::ZERO;
    for (i, p) in points.iter().enumerate() {
        let Some(q) = points.get((i + 1) % points.len()) else { continue };
        n += Vec3::new((p.y - q.y) * (p.z + q.z), (p.z - q.z) * (p.x + q.x), (p.x - q.x) * (p.y + q.y));
    }
    n * 0.5
}

/// The point of `c` at `t` (`None` where an intersection curve cannot find it).
fn curve_point(c: &mt::Curve, t: f64) -> Option<Vec3> {
    let p = match c {
        mt::Curve::IntersectionCurve(ic) => ic.search_triple(t)?.0,
        other => other.subs(t),
    };
    let p = from_p3(p);
    p.is_finite().then_some(p)
}

/// Points along an edge, in its own direction, at `s ∈ [0, 1]` of its parameter range. An
/// intersection curve's point is searched from the surface parameters of the point found before
/// when that one is near (an eighth of the range), and kept only when it lies near the curve's
/// leader (a seeded search can slide to another branch of the surfaces' intersection); else the
/// unseeded search decides. `None` where no point is found.
pub(crate) struct EdgeSampler {
    curve: mt::Curve,
    range: (f64, f64),
    hint: Option<(f64, mt::Point2, mt::Point2)>,
}

impl EdgeSampler {
    pub(crate) fn new(edge: &mt::Edge) -> Option<EdgeSampler> {
        let curve = edge.oriented_curve();
        let range = curve.range_tuple();
        (range.0.is_finite() && range.1.is_finite() && range.0 < range.1).then_some(EdgeSampler { curve, range, hint: None })
    }

    pub(crate) fn at(&mut self, s: f64) -> Option<Vec3> {
        let (t0, t1) = self.range;
        let t = if s >= 1.0 { t1 } else { t0 + (t1 - t0) * s };
        let mt::Curve::IntersectionCurve(ic) = &self.curve else { return curve_point(&self.curve, t) };
        let leader = ic.leader();
        let (p, d) = (leader.subs(t), leader.der(t));
        let near = |q: mt::Point3| q.distance(p) <= 4.0 * ic.tolerance() + 1e-6;
        let seeded = self
            .hint
            .filter(|(tp, _, _)| (t - tp).abs() <= (t1 - t0) / 8.0)
            .and_then(|(_, a, b)| mt::double_projection(ic.surface0(), Some((a.x, a.y)), ic.surface1(), Some((b.x, b.y)), p, d, 100))
            .filter(|f| near(f.0));
        let found = seeded.or_else(|| mt::double_projection(ic.surface0(), None, ic.surface1(), None, p, d, 100))?;
        self.hint = Some((t, found.1, found.2));
        let q = from_p3(found.0);
        q.is_finite().then_some(q)
    }
}

/// The planar face bounded by the closed loop `chain` (each edge oriented along it) when its
/// edges are certainly in one plane, `None` otherwise (also when an edge cannot be sampled).
/// The plane is fitted to points along the edges ([`plane_through`], the normal turning with the
/// loop); the edges are certified within `tol` of it conservatively: a line by its ends, a
/// B-spline or NURBS curve with positive weights by its control points (the curve lies in their
/// convex hull), an intersection curve only when one of its two surfaces is a plane that the
/// loop's points lie on and the fitted plane agrees with over the loop (the curve lies in that
/// plane). Samples alone are never trusted: a spline can bulge out of the plane between them.
pub(crate) fn planar_face(chain: &[mt::Edge], tol: f64) -> Result<Option<mt::Face>> {
    const SAMPLES: usize = 16;
    let mut pts = Vec::with_capacity(chain.len() * SAMPLES);
    for e in chain {
        let Some(mut sampler) = EdgeSampler::new(e) else { return Ok(None) };
        for k in 0..SAMPLES {
            let Some(p) = sampler.at(k as f64 / SAMPLES as f64) else { return Ok(None) };
            pts.push(p3(p));
        }
    }
    let Some(plane) = plane_through(&pts) else { return Ok(None) };
    let (o, n) = (plane.origin(), plane.normal());
    let off = |p: mt::Point3, o: mt::Point3, n: mt::Vector3| (p - o).dot(n).abs();
    let on_plane = |p: mt::Point3| off(p, o, n) <= tol;
    if !pts.iter().all(|p| on_plane(*p)) {
        return Ok(None);
    }
    // A support plane `q` of an intersection curve holds the loop when the loop's points lie on
    // it and their feet on the fitted plane do too.
    let holds = |q: &mt::Plane| {
        let (qo, qn) = (q.origin(), q.normal());
        pts.iter().all(|p| off(*p, qo, qn) <= tol && off(*p - n * (*p - o).dot(n), qo, qn) <= tol)
    };
    for e in chain {
        let certified = match e.curve() {
            mt::Curve::Line(l) => on_plane(l.0) && on_plane(l.1),
            mt::Curve::BSplineCurve(b) => b.control_points().iter().all(|p| on_plane(*p)),
            mt::Curve::NurbsCurve(c) => {
                c.non_rationalized().control_points().iter().all(|h| h.w > 0.0 && on_plane(mt::Point3::new(h.x / h.w, h.y / h.w, h.z / h.w)))
            }
            mt::Curve::IntersectionCurve(ic) => [ic.surface0(), ic.surface1()].into_iter().any(|s| matches!(s, mt::Surface::Plane(q) if holds(q))),
        };
        if !certified {
            return Ok(None);
        }
    }
    let wire: mt::Wire = chain.to_vec().into();
    Ok(Some(mt::Face::try_new(vec![wire], mt::Surface::Plane(plane)).map_err(|e| KernelError::Failed(format!("patch: {e}")))?))
}
