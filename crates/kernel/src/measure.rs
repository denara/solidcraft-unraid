use serde::Serialize;
use solidcraft_geom::{Aabb3, Mesh, Vec3};

use crate::Result;
use crate::body::Body;
use crate::topo::{TopoCounts, merged_topology};

/// Measured properties of a body.
#[derive(Clone, Debug, Serialize)]
pub struct BodyMeasure {
    pub volume: f64,
    pub area: f64,
    pub centroid: Vec3,
    pub bbox: Aabb3,
    /// Raw kernel topology counts (closed curves and surfaces may be split in pieces).
    pub faces: usize,
    pub edges: usize,
    pub vertices: usize,
    pub shells: usize,
    /// Counts after merging split pieces of the same analytic surface/curve (comparable with
    /// systems that use periodic faces and edges).
    pub merged: TopoCounts,
}

/// Per-face sums: area, 6 × signed volume and 24 × first moments (divergence theorem, origin 0).
fn face_sums(m: &Mesh, nf: usize) -> Vec<(f64, f64, Vec3)> {
    let mut out = vec![(0.0, 0.0, Vec3::ZERO); nf];
    for (t, f) in m.triangles.iter().zip(&m.tri_face) {
        let (Some([a, b, c]), Some(slot)) = (m.tri(t), out.get_mut(*f as usize)) else { continue };
        let d = a.dot(b.cross(c));
        slot.0 += (b - a).cross(c - a).len() * 0.5;
        slot.1 += d;
        slot.2 += (a + b + c) * d;
    }
    out
}

/// Smallest radius of curvature along the mesh's edge polylines (straight edges ignored).
fn min_edge_radius(m: &Mesh) -> Option<f64> {
    let mut best: Option<f64> = None;
    for e in &m.edges {
        for w in e.windows(3) {
            let (Some(&a), Some(&b), Some(&c)) = (w.first(), w.get(1), w.get(2)) else { continue };
            let (ab, bc, ca) = (b - a, c - b, a - c);
            let cross = ab.cross(bc).len();
            if !(cross > 1e-12 * ab.len() * bc.len()) {
                continue;
            }
            let r = ab.len() * bc.len() * ca.len() / (2.0 * cross);
            if r.is_finite() && r > 0.0 && best.is_none_or(|x| r < x) {
                best = Some(r);
            }
        }
    }
    best
}

/// Volume, area, centre and box of a body, as [`measure`] works them out, without the topology
/// counts (and the tessellation they take).
#[derive(Clone, Copy, Debug, Serialize)]
pub struct MassMeasure {
    pub volume: f64,
    pub area: f64,
    pub centroid: Vec3,
    pub bbox: Aabb3,
}

/// Measure a body from a fine tessellation (chord tolerance 5e-5 of the body size, finer for
/// small radii down to 5e-6). Faces whose fine tessellation disagrees with a coarse one (a
/// meshing failure) use the coarse result.
pub fn measure(b: &Body) -> Result<BodyMeasure> {
    if let Some((pos, tris)) = b.triangle_mesh() {
        // Exact for a mesh body; topology counts are facets, sides and vertices.
        let m = b.tessellate(1.0)?;
        let mm = m.measure();
        let edges = b.mesh.as_ref().map(|m| m.edge_count()).unwrap_or(0);
        let merged =
            TopoCounts { faces: tris.len(), edges, vertices: pos.len(), face_types: [("mesh".to_string(), tris.len())].into_iter().collect() };
        return Ok(BodyMeasure {
            volume: mm.volume,
            area: mm.area,
            centroid: mm.centroid,
            bbox: m.bounds(),
            faces: tris.len(),
            edges,
            vertices: pos.len(),
            shells: 1,
            merged,
        });
    }
    let (m, topo) = mass(b, true)?;
    let topo = match topo {
        Some(t) => t,
        None => b.tessellate((b.size() * 1e-3).max(1e-3))?,
    };
    Ok(BodyMeasure {
        volume: m.volume,
        area: m.area,
        centroid: m.centroid,
        bbox: m.bbox,
        faces: b.face_count(),
        edges: b.edge_count(),
        vertices: b.vertex_count(),
        shells: b.shell_count(),
        merged: merged_topology(b, &topo)?,
    })
}

/// [`measure`]'s volume, area, centre and box alone: the same numbers, a tessellation fewer
/// (for callers that need no topology counts, such as an interference check).
pub fn measure_mass(b: &Body) -> Result<MassMeasure> {
    if b.triangle_mesh().is_some() {
        let m = measure(b)?;
        return Ok(MassMeasure { volume: m.volume, area: m.area, centroid: m.centroid, bbox: m.bbox });
    }
    Ok(mass(b, false)?.0)
}

/// The mass properties of a B-rep body from three tessellations: a coarse one (which sets how
/// fine the others go), then a fine and a medium one at once on desktop (one after the other
/// in the browser). With `topology`, also the coarse unrefined mesh the topology counts take,
/// made alongside them.
fn mass(b: &Body, topology: bool) -> Result<(MassMeasure, Option<Mesh>)> {
    let size = b.size();
    let coarse_tol = (size * 1e-3).max(1e-3);
    let coarse = b.tessellate_with(coarse_tol, true)?;
    // Chord error relative to a radius sets the volume error: keep it near 3e-4 of the
    // smallest radius (thin tubes, small fillets).
    let tight = min_edge_radius(&coarse).map(|r| r * 3e-4).unwrap_or(f64::INFINITY);
    let fine_tol = (size * 5e-5).min(tight).max(size * 5e-6).max(1e-4);
    let mut jobs = vec![(fine_tol, true), ((fine_tol * 5.0).max(2e-4), true)];
    if topology {
        jobs.push((coarse_tol, false));
    }
    // Independent tessellations of one body: each on its own thread, the calling thread's
    // profile counting their wall time once as tessellation. Not from inside a worker pool: a
    // pool thread blocked on threads of its own could leave their meshing (which runs on the
    // shared pool) without a worker.
    #[cfg(not(target_arch = "wasm32"))]
    let threads = if rayon::current_thread_index().is_none() { jobs.len() } else { 1 };
    #[cfg(target_arch = "wasm32")]
    let threads = 1;
    let made = crate::profile::timed(crate::profile::Kind::Tessellate, || {
        crate::par::map_on_threads(&jobs, threads, |&(tol, refine)| b.tessellate_with(tol, refine))
    });
    let mut made = made.into_iter().map(|m| m.unwrap_or_else(|| Err(crate::KernelError::Internal("tessellation failed".into()))));
    let missing = || crate::KernelError::Internal("tessellation missing".into());
    let fine = made.next().ok_or_else(missing)??;
    let medium = made.next().ok_or_else(missing)??;
    let topo = match made.next() {
        Some(m) => Some(m?),
        None => None,
    };
    let nf = b.face_count();
    let (sf, sm, sc) = (face_sums(&fine, nf), face_sums(&medium, nf), face_sums(&coarse, nf));
    let close = |x: f64, y: f64| (x - y).abs() <= 0.01 * x.abs().max(y.abs()) + 1e-9;
    let (mut area, mut v6, mut mom) = (0.0, 0.0, Vec3::ZERO);
    for ((f, m), c) in sf.iter().zip(&sm).zip(&sc) {
        // Trust the finest tessellation that agrees with the next coarser one.
        let pick = if close(f.0, m.0) {
            f
        } else if close(m.0, c.0) {
            m
        } else {
            f
        };
        area += pick.0;
        v6 += pick.1;
        mom += pick.2;
    }
    let volume = v6 / 6.0;
    let centroid = if v6.abs() > 1e-12 { mom / (4.0 * v6) } else { fine.bounds().center() };
    Ok((MassMeasure { volume, area, centroid, bbox: coarse.bounds().union(&fine.bounds()) }, topo))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BoolOp, boolean, cylinder, torus};

    /// The mass properties alone are [`measure`]'s, to the bit, with the tessellations made at
    /// once; and on a curved intersection they are the textbook ones.
    #[test]
    fn mass_alone_is_the_full_measure() {
        let ring = torus(Vec3::ZERO, 10.0, 2.0).unwrap();
        let post = cylinder(Vec3::new(10.0, 0.0, -5.0), Vec3::Z, 1.5, 10.0).unwrap();
        let shared = boolean(&ring, &post, BoolOp::Intersect).unwrap().unwrap();
        for b in [&ring, &shared] {
            let (full, mass) = (measure(b).unwrap(), measure_mass(b).unwrap());
            assert_eq!(full.volume.to_bits(), mass.volume.to_bits());
            assert_eq!(full.area.to_bits(), mass.area.to_bits());
            assert_eq!(full.centroid, mass.centroid);
            assert_eq!((full.bbox.min, full.bbox.max), (mass.bbox.min, mass.bbox.max));
        }
        let want = 2.0 * std::f64::consts::PI.powi(2) * 10.0 * 4.0;
        let got = measure_mass(&ring).unwrap().volume;
        assert!((got - want).abs() < 1e-3 * want, "{got} against {want}");
        assert!(measure(&ring).unwrap().merged.faces > 0);
    }
}
