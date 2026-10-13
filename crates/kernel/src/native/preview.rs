//! Bounded provisional tessellation. Fixed sampling avoids untrusted adaptive
//! subdivision before output budgets can be checked; editing uses the normal mesh.
use super::{Body, Result, invalid};
use mt::{BoundedCurve, ParametricCurve};
use solidcraft_geom::{Mesh, Vec3};

use truck_modeling as mt;

fn curve_safe(c: &mt::Curve, depth: usize) -> bool {
    if depth > 4 {
        return false;
    }
    match c {
        mt::Curve::Line(_) => true,
        mt::Curve::BSplineCurve(c) => c.control_points().len() <= 256,
        mt::Curve::NurbsCurve(c) => c.control_points().len() <= 256,
        mt::Curve::IntersectionCurve(c) => {
            let leader = match c.leader() {
                mt::Leader::Polyline(p) => p.len() <= 256,
                mt::Leader::BSpline(p) => p.control_points().len() <= 256,
            };
            leader && surface_safe(c.surface0(), depth + 1) && surface_safe(c.surface1(), depth + 1)
        }
    }
}
fn surface_safe(s: &mt::Surface, depth: usize) -> bool {
    if depth > 4 {
        return false;
    }
    match s {
        mt::Surface::Plane(_) => true,
        mt::Surface::BSplineSurface(s) => s.control_points().iter().map(Vec::len).sum::<usize>() <= 256,
        mt::Surface::NurbsSurface(s) => s.control_points().iter().map(Vec::len).sum::<usize>() <= 256,
        mt::Surface::RevolutedCurve(s) => {
            !matches!(s.entity().entity_curve(), mt::Curve::IntersectionCurve(_)) && curve_safe(s.entity().entity_curve(), depth + 1)
        }
    }
}

/// Approximate read-only display. Boundary and grid sampling are fixed before
/// triangulation, and the complete body's upper bound is checked before projection.
pub fn native_preview_mesh(body: &Body, detail: usize, remaining: &mut usize) -> Result<Mesh> {
    crate::guard("bounded native preview", || {
        if body.mesh.is_some() {
            return Err(invalid("native preview requires B-rep"));
        }
        let detail = detail.clamp(1, 8);

        let mut prepared = Vec::new();
        let mut estimate = 0usize;
        for face in body.solid.face_iter() {
            let surface = face.surface();
            let grid = if matches!(surface, mt::Surface::Plane(_)) { 1 } else { detail * 2 };
            if !surface_safe(&surface, 0) {
                return Err(invalid("preview surface complexity"));
            }
            let mut loops = Vec::new();
            let mut count = 0usize;
            for wire in face.absolute_boundaries() {
                let mut points = Vec::new();
                for edge in wire {
                    let curve = edge.oriented_curve();
                    if !curve_safe(&curve, 0) {
                        return Err(invalid("preview curve complexity"));
                    }
                    let n = if matches!(curve, mt::Curve::Line(_)) { 1 } else { detail * 4 };
                    count = count.saturating_add(n);
                    if count > 4096 {
                        return Err(invalid("preview boundary budget"));
                    }
                    let (a, b) = curve.range_tuple();
                    for i in 0..n {
                        let t = a * (1. - i as f64 / n as f64) + b * (i as f64 / n as f64);
                        let point = match &curve {
                            mt::Curve::IntersectionCurve(c) => c.leader().subs(t),
                            _ => curve.subs(t),
                        };
                        if !point.x.is_finite() || !point.y.is_finite() || !point.z.is_finite() {
                            return Err(invalid("preview sample"));
                        }
                        points.push(point);
                    }
                }
                let first = points.first().copied().ok_or_else(|| invalid("empty preview wire"))?;
                points.push(first);
                count = count.saturating_add(1);
                if count > 4096 {
                    return Err(invalid("preview boundary budget"));
                }
                loops.push(points);
            }
            // Projection duplicates at most each input point, seams add eight
            // fixed polylines, and a planar triangulation has fewer than 2V faces.
            estimate = estimate.saturating_add(2 * (2 * count + 8 * (grid + 1) + (grid + 1).pow(2)) + 16);
            if estimate > *remaining {
                return Err(invalid("preview allocation budget"));
            }
            prepared.push((surface, face.orientation(), loops, grid));
        }
        *remaining -= estimate;
        let mut out = Mesh::default();
        for (surface, forward, loops, grid) in prepared {
            let mesh = truck_meshalgo::tessellation::preview_face(&surface, &loops, grid).ok_or_else(|| invalid("preview projection"))?;
            let mut count = 0;
            for triangle in mesh.faces().triangle_iter() {
                if out.triangles.len() >= estimate {
                    return Err(invalid("preview output budget"));
                }
                let mut indices = [0u32; 3];
                for (slot, index) in indices.iter_mut().zip(triangle) {
                    let p = mesh.positions().get(index.pos).ok_or_else(|| invalid("preview vertex"))?;
                    if !p.x.is_finite() || !p.y.is_finite() || !p.z.is_finite() {
                        return Err(invalid("preview vertex"));
                    }
                    *slot = u32::try_from(out.positions.len()).map_err(invalid)?;
                    out.positions.push(Vec3::new(p.x, p.y, p.z));
                }
                if !forward {
                    indices.swap(1, 2);
                }
                out.triangles.push(indices);
                count += 1;
            }
            if count == 0 {
                return Err(invalid("missing preview face"));
            }
        }
        Ok(out)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_preview_is_finite_and_budgeted_before_meshing() {
        for body in [
            crate::box_solid(Vec3::ZERO, Vec3::new(10., 20., 30.)).unwrap(),
            crate::sphere(Vec3::ZERO, 10.).unwrap(),
            crate::torus(Vec3::ZERO, 20., 4.).unwrap(),
        ] {
            let body = crate::decode_native_body(&crate::encode_native_body(&body).unwrap()).unwrap();
            assert!(native_preview_mesh(&body, 8, &mut 0).is_err());
            let mut budget = 100_000;
            let mesh = native_preview_mesh(&body, 8, &mut budget).unwrap();
            assert!(!mesh.triangles.is_empty());
            assert!(mesh.triangles.len() <= 100_000 - budget);
            assert!(mesh.positions.iter().all(|p| p.x.is_finite() && p.y.is_finite() && p.z.is_finite()));
        }
    }
}
