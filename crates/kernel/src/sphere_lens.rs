//! A lens extruded from a sphere's projected outline (#2750). One of its two
//! cylinders encloses the whole target and only touches the sphere. Removing that
//! redundant constraint gives the same cut without a tangent intersection.

use solidcraft_geom::{Loop2, Plane, Region2, Seg2, Vec2, Vec3};

use crate::boolean_record as recorded;
use crate::history::Change;
use crate::{Body, BoolOp, FaceSource, FaceSurface, Result, face_surface};

/// Maximum signed height of a line or circular arc, including interior extrema.
fn height(segment: Seg2, plane: Plane, cap: Plane) -> Option<f64> {
    let at = |p| cap.height(plane.to_world(p));
    let ends = at(segment.start()).max(at(segment.end()));
    match segment {
        Seg2::Line { .. } => Some(ends),
        Seg2::Arc { center, radius, .. } => {
            let n = cap.normal();
            let direction = Vec2::new(n.dot(plane.x), n.dot(plane.y));
            let probe = center + direction.normalized().unwrap_or(Vec2::X) * radius;
            Some(ends.max(at(segment.point_at(segment.closest_param(probe)))))
        }
        _ => None,
    }
}

/// Recognize a quarter ball by its complete boundary: one spherical face, two
/// perpendicular semicircular caps sharing their diameter, each cap inside the
/// other's outward halfspace. The opposite cap orientation (three quarters of a
/// ball) is refused. Whole-span curve certificates prove the caps, not samples.
fn enclosing_ball(body: &Body, tol: f64) -> Option<(Vec3, f64, Vec<Plane>)> {
    if body.is_mesh() || body.shell_count() != 1 || body.face_count() != 3 {
        return None;
    }
    let surfaces = (0..3).map(|i| face_surface(body, i)).collect::<Option<Vec<_>>>()?;
    let (sphere, center, radius) = surfaces.iter().enumerate().find_map(|(i, surface)| match *surface {
        FaceSurface::Sphere { center, radius } => Some((i, center, radius)),
        _ => None,
    })?;
    let spherical = body.solid.face_iter().nth(sphere)?;
    if spherical.boundaries().len() != 1 || spherical.edge_iter().count() != 2 {
        return None;
    }
    let sphere_edges: std::collections::HashSet<_> = spherical.edge_iter().map(|e| e.id()).collect();
    let caps: Vec<_> = surfaces
        .iter()
        .filter_map(|surface| match *surface {
            FaceSurface::Plane { normal, point } => Plane::from_normal(point, normal),
            _ => None,
        })
        .collect();
    let (cap0, cap1) = (*caps.first()?, *caps.get(1)?);
    if caps.len() != 2 || cap0.normal().dot(cap1.normal()).abs() > 1e-9 || cap0.height(center).abs() > tol || cap1.height(center).abs() > tol {
        return None;
    }
    let mut work = 2_000_000usize;
    let mut diameters = Vec::new();
    for (i, face) in body.solid.face_iter().enumerate() {
        if i == sphere {
            continue;
        }
        let FaceSurface::Plane { normal, point } = *surfaces.get(i)? else { return None };
        let plane = Plane::from_normal(point, normal)?;
        if face.boundaries().len() != 1 || face.edge_iter().count() != 2 {
            return None;
        }
        let c = plane.to_local(center);
        let mut arc = 0.;
        let mut lines = 0;
        for edge in face.edge_iter() {
            if plane.height(crate::body::from_p3(edge.front().point())).abs() > tol
                || plane.height(crate::body::from_p3(edge.back().point())).abs() > tol
            {
                return None;
            }
            for segment in crate::prism_curve::segment(&edge, &plane, tol, &mut work)? {
                match segment {
                    Seg2::Line { a, b } if (a + b - c * 2.).len() <= tol && (a.dist(c) - radius).abs() <= tol => {
                        diameters.push(edge.id());
                        lines += 1;
                    }
                    Seg2::Arc { center: q, radius: r, sweep, .. } if q.dist(c) + (r - radius).abs() <= tol && sphere_edges.contains(&edge.id()) => {
                        arc += sweep.abs();
                    }
                    _ => return None,
                }
                if height(segment, plane, cap0)? > tol || height(segment, plane, cap1)? > tol {
                    return None;
                }
            }
        }
        if lines != 1 || (arc - std::f64::consts::PI).abs() > 1e-8 {
            return None;
        }
    }
    if diameters.len() != 2 || diameters.first()? != diameters.get(1)? {
        return None;
    }
    Some((center, radius, caps))
}

/// Replace only a construction-certified, convex two-circle lens. Its cutting cylinder stays exact; a cap moves
/// only into a halfspace proved to be outside the target. The added tool material is outside the ball.
fn relaxed_tool(a: &Body, b: &Body) -> Option<Body> {
    if b.is_mesh() || b.shell_count() != 1 || b.face_count() != 4 {
        return None;
    }
    let tol = (a.size().max(b.size()) * 1e-9).max(1e-8);
    let sources = b.face_sources()?;
    let (region, plane) = sources.iter().find_map(|source| match source {
        Some(FaceSource::ExtrudeCap { high: true, region, plane }) => Some((region, *plane)),
        _ => None,
    })?;
    let bottom = sources.iter().find_map(|source| match source {
        Some(FaceSource::ExtrudeCap { high: false, region: r, plane }) if r == region => Some(*plane),
        _ => None,
    })?;
    if !region.holes.is_empty() || region.outer.segs.len() != 2 || !region.outer.is_closed(tol) {
        return None;
    }
    let circles: Vec<(Vec2, f64)> = region
        .outer
        .segs
        .iter()
        .map(|s| match *s {
            Seg2::Arc { center, radius, sweep, .. } if sweep > 0.0 && sweep <= std::f64::consts::PI + 1e-9 => Some((center, radius)),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let ((c0, r0), (c1, r1)) = (*circles.first()?, *circles.get(1)?);
    let distance = c0.dist(c1);
    if distance <= (r0 - r1).abs() + tol || distance >= r0 + r1 - tol {
        return None;
    }
    let (center, radius, caps) = enclosing_ball(a, tol)?;
    let projected = plane.to_local(center);
    let redundant = circles.iter().position(|(c, r)| projected.dist(*c) + radius <= *r + tol)?;
    let (c, r) = *circles.get(1 - redundant)?;
    let height = plane.height(bottom.origin);
    if !height.is_finite() || height >= -tol || plane.normal().dot(bottom.normal()) > -1.0 + 1e-9 {
        return None;
    }
    // The proven quarter ball lies behind each cap. Extend a coincident tool
    // end outward into that empty halfspace; this avoids the generic nudge's
    // paper-thin lid across the opening, without moving the cutting cylinder.
    let on_cap = |end: Plane| caps.iter().any(|cap| cap.normal().dot(end.normal()) > 1.0 - 1e-9 && cap.height(end.origin).abs() <= tol);
    let (low, high) = (on_cap(bottom), on_cap(plane));
    if !low && !high {
        return None;
    }
    let margin = (a.size().max(b.size()) * 0.01).max(1e-5);
    let lo = height - if low { margin } else { 0. };
    let hi = if high { margin } else { 0. };
    let mut tool =
        crate::extrude(&plane, &[Region2 { outer: Loop2::circle_from(c, r, crate::build::SEAM_ANGLE), holes: vec![] }], lo, hi).ok()?.pop()?;
    // Each rebuilt surface names its original tool face. Extended portions never
    // meet the target, so every face used in the cut still lies in that input face.
    let mut faces = Vec::new();
    for i in 0..tool.face_count() {
        let surface = face_surface(&tool, i)?;
        let original = (0..b.face_count()).find(|j| match (surface, face_surface(b, *j)) {
            (FaceSurface::Plane { normal: n, .. }, Some(FaceSurface::Plane { normal: m, .. })) => n.dot(m) > 1.0 - 1e-9,
            (FaceSurface::Cylinder { origin: p, axis: n, radius: r }, Some(FaceSurface::Cylinder { origin: q, axis: m, radius: s })) => {
                n.dot(m).abs() > 1.0 - 1e-9 && (p - q).cross(n).len() + (r - s).abs() <= tol
            }
            _ => false,
        })?;
        faces.push(crate::record::from_face(0, original, Change::Modified));
    }
    let history = crate::record::from_faces(&tool.solid, 1, faces);
    tool = tool.with_checked_history(Some(history), &crate::record::counts(&[b]));
    Some(tool)
}

pub(crate) fn cut(a: &Body, b: &Body, run: &dyn Fn(&Body, &Body, BoolOp) -> Result<Option<Body>>) -> Option<Result<Option<Body>>> {
    let tool = crate::guard("sphere lens tool", || Ok(relaxed_tool(a, b))).ok()??;
    let result = run(a, &tool, BoolOp::Cut).ok()?;
    crate::boolean_guard::check_retry(a, b, BoolOp::Cut, result.as_ref()).ok()?;
    Some(Ok(result.map(|body| {
        let history = recorded::through(&body, 1, tool.history());
        body.with_checked_history(history, &crate::record::counts(&[a, b]))
    })))
}

#[cfg(test)]
#[path = "sphere_lens_tests.rs"]
mod tests;
