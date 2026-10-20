//! Reference triangles belong to a definition, even while its occurrences move. Keep one lazy
//! source mesh and map it into each pose; retain the exact placed B-rep separately in ModelBody.
use std::sync::{Arc, OnceLock};

use solidcraft_geom::tess::TessPlan;
use solidcraft_geom::{Mesh, Vec3};
use solidcraft_kernel::{self as kernel, Body};

use super::ModelBody;

#[derive(Debug)]
pub(super) struct PlacedMesh {
    body: Body,
    mesh: Arc<OnceLock<Arc<Mesh>>>,
    /// The definition's display plans and their meshes by display quality (its own cells):
    /// each placement draws them moved, so a part is planned once however often it is placed.
    plans: Arc<[OnceLock<Option<Arc<TessPlan>>>; 4]>,
    plan_meshes: Arc<[OnceLock<Option<Arc<Mesh>>>; 4]>,
    transform: crate::Mat,
    pub(super) tolerance: f64,
}

impl PlacedMesh {
    /// Share the original definition, never a chain of previously placed meshes. A source's
    /// OnceLock lives with its immutable geometry, so edits get a new source automatically.
    pub(super) fn new(source: &ModelBody, transform: &crate::Mat) -> Option<Self> {
        if !proper_rigid(transform) {
            return None;
        }
        Some(match source.mesh_placement.as_deref() {
            Some(original) => Self {
                body: original.body.clone(),
                mesh: original.mesh.clone(),
                plans: original.plans.clone(),
                plan_meshes: original.plan_meshes.clone(),
                transform: crate::mat_mul(transform, &original.transform),
                tolerance: original.tolerance,
            },
            None => Self {
                body: source.body.clone(),
                mesh: source.mesh.clone(),
                plans: source.plan.clone(),
                plan_meshes: source.plan_mesh.clone(),
                transform: *transform,
                tolerance: source.display_tol(),
            },
        })
    }

    pub(super) fn mesh(&self) -> Arc<Mesh> {
        Arc::new(self.moved(&reference_mesh(&self.body, &self.mesh, self.tolerance)))
    }

    /// The definition's display plan at quality `q` (made once, in its own cell), moved here.
    pub(super) fn plan(&self, q: crate::display_quality::DisplayQuality, name: &str) -> Option<Arc<TessPlan>> {
        let source = definition_plan(&self.body, self.plans.get(q.index())?, q, name)?;
        Some(Arc::new(source.transformed(&self.transform)))
    }

    /// The definition's display plan mesh at quality `q`, moved here.
    pub(super) fn plan_mesh(&self, q: crate::display_quality::DisplayQuality, name: &str) -> Option<Arc<Mesh>> {
        let cell = self.plan_meshes.get(q.index())?;
        let source = cell
            .get_or_init(|| {
                let plan = definition_plan(&self.body, self.plans.get(q.index())?, q, name)?;
                self.body.display_mesh_of(&plan).ok().map(Arc::new)
            })
            .clone()?;
        Some(Arc::new(self.moved(&source)))
    }

    /// `source` (in the definition's place) moved by this placement.
    fn moved(&self, source: &Mesh) -> Mesh {
        Mesh {
            positions: source.positions.iter().map(|p| crate::apply_point(&self.transform, *p)).collect(),
            normals: source.normals.iter().map(|n| crate::apply_vector(&self.transform, *n)).collect(),
            edges: source.edges.iter().map(|edge| edge.iter().map(|p| crate::apply_point(&self.transform, *p)).collect()).collect(),
            triangles: source.triangles.clone(),
            tri_face: source.tri_face.clone(),
            edge_faces: source.edge_faces.clone(),
            seams: source.seams.clone(),
            smooth_edges: source.smooth_edges.clone(),
            sharp_edges: source.sharp_edges.clone(),
        }
    }
}

/// The display plan of a definition `body` at quality `q`, made once in `cell`.
pub(super) fn definition_plan(
    body: &Body,
    cell: &OnceLock<Option<Arc<TessPlan>>>,
    q: crate::display_quality::DisplayQuality,
    name: &str,
) -> Option<Arc<TessPlan>> {
    cell.get_or_init(|| match body.display_plan_angular(q.chord(body.size()), q.angle()) {
        Ok(plan) => Some(Arc::new(plan)),
        Err(e) => {
            log::debug!("{name}: no display plan ({e})");
            None
        }
    })
    .clone()
}

pub(super) fn reference_mesh(body: &Body, mesh: &OnceLock<Arc<Mesh>>, tolerance: f64) -> Arc<Mesh> {
    mesh.get_or_init(|| Arc::new(body.reference_display_mesh_angular(tolerance, kernel::DISPLAY_ANGLE).unwrap_or_default())).clone()
}

/// Only proper rotations and translations preserve triangle winding, normal lengths and the
/// reference chord tolerance. Scale, shear, reflection and malformed matrices use the mesher.
fn proper_rigid(m: &crate::Mat) -> bool {
    if m.iter().flatten().any(|v| !v.is_finite()) || m[0][3] != 0. || m[1][3] != 0. || m[2][3] != 0. || m[3][3] != 1. {
        return false;
    }
    let [x, y, z] = [Vec3::new(m[0][0], m[0][1], m[0][2]), Vec3::new(m[1][0], m[1][1], m[1][2]), Vec3::new(m[2][0], m[2][1], m[2][2])];
    [x, y, z].iter().all(|v| (v.dot(*v) - 1.).abs() < 1e-9)
        && x.dot(y).abs() < 1e-9
        && x.dot(z).abs() < 1e-9
        && y.dot(z).abs() < 1e-9
        && (x.cross(y).dot(z) - 1.).abs() < 1e-9
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placed_mesh_is_lazy_and_composes_from_one_immutable_source() {
        let source = ModelBody::new("Part".into(), kernel::box_solid(Vec3::ZERO, Vec3::new(2., 3., 4.)).unwrap(), 1);
        let a = crate::rigid(Vec3::new(10., 20., 30.), Vec3::ZERO, Vec3::Z, 0.7);
        let b = crate::rigid(Vec3::new(3., -5., 2.), Vec3::ZERO, Vec3::X, 0.4);
        let mut first = ModelBody::new("First".into(), kernel::transform_matrix(&source.body, a).unwrap(), 1);
        first.mesh_placement = Some(Arc::new(PlacedMesh::new(&source, &a).unwrap()));
        let mut nested = ModelBody::new("Nested".into(), kernel::transform_matrix(&first.body, b).unwrap(), 1);
        nested.mesh_placement = Some(Arc::new(PlacedMesh::new(&first, &b).unwrap()));
        assert_eq!(first.display_tol(), source.display_tol());
        assert_eq!(nested.display_tol(), source.display_tol());
        assert!(source.cached_mesh().is_none());
        assert!(first.cached_mesh().is_none());
        let mesh = nested.mesh();
        assert!(source.cached_mesh().is_some(), "the canonical source is warmed on demand");
        assert!(first.cached_mesh().is_none(), "composition does not retain or evaluate intermediate poses");
        let local = source.mesh();
        let matrix = crate::mat_mul(&b, &a);
        for (p, q) in local.positions.iter().zip(&mesh.positions) {
            assert!(crate::apply_point(&matrix, *p).dist(*q) < 1e-8);
        }
        let (other, work) = kernel::profile::capture(|| first.mesh());
        assert_eq!(work.tessellate_ms, 0.);
        assert_eq!(other.tri_face, local.tri_face);
        assert!(Arc::ptr_eq(&mesh, &nested.mesh()));
    }

    /// Placed parts share their definition's display plan (#3044): the definition is planned
    /// once, every placement moves that plan, and a moved plan evaluates to the definition's
    /// points moved, on every kind of surface (planes, revolved and spline surfaces).
    #[test]
    fn placed_parts_are_planned_once_and_draw_the_moved_plan() {
        use crate::display_quality::DisplayQuality;
        let q = DisplayQuality::parse("normal").unwrap();
        for body in [
            kernel::box_solid(Vec3::ZERO, Vec3::new(2., 3., 4.)).unwrap(),
            kernel::cylinder(Vec3::new(1., 0., 0.), Vec3::Z, 2., 8.).unwrap(),
            kernel::torus(Vec3::ZERO, 6., 1.5).unwrap(),
            kernel::sphere(Vec3::new(0., 1., 2.), 3.).unwrap(),
        ] {
            let source = ModelBody::new("Part".into(), body, 1);
            let a = crate::rigid(Vec3::new(10., 20., 30.), Vec3::ZERO, Vec3::Z, 0.7);
            let b = crate::rigid(Vec3::new(-4., 5., 1.), Vec3::new(1., 1., 0.), Vec3::X, 1.1);
            let placed: Vec<ModelBody> = [a, b]
                .iter()
                .map(|m| {
                    let mut p = ModelBody::new("Copy".into(), kernel::transform_matrix(&source.body, *m).unwrap(), 1);
                    p.mesh_placement = Some(Arc::new(PlacedMesh::new(&source, m).unwrap()));
                    p
                })
                .collect();
            assert!(source.plan.get(q.index()).unwrap().get().is_none());
            let plans: Vec<_> = placed.iter().map(|p| p.display_plan_at(q).unwrap()).collect();
            assert!(source.plan.get(q.index()).unwrap().get().is_some(), "placing plans the definition, in its own cell");
            let own = source.display_plan_at(q).unwrap();
            let local = own.evaluate();
            for (m, plan) in [a, b].iter().zip(&plans) {
                assert!(!Arc::ptr_eq(plan, &own));
                let moved = plan.evaluate();
                assert_eq!(moved.triangles, local.triangles);
                for (p, q) in local.positions.iter().zip(&moved.positions) {
                    assert!(crate::apply_point(m, *p).dist(*q) < 1e-9, "{p:?} -> {q:?}");
                }
                for (n, k) in local.normals.iter().zip(&moved.normals) {
                    assert!(crate::apply_vector(m, *n).dist(*k) < 1e-9);
                }
            }
            for (m, p) in [a, b].iter().zip(&placed) {
                let mesh = p.plan_mesh_at(q).unwrap();
                let base = source.plan_mesh_at(q).unwrap();
                assert_eq!(mesh.triangles, base.triangles);
                for (x, y) in base.positions.iter().zip(&mesh.positions) {
                    assert!(crate::apply_point(m, *x).dist(*y) < 1e-9);
                }
            }
        }
    }

    #[test]
    fn world_instances_warm_one_definition_and_an_edit_replaces_it() {
        let mut doc = crate::Document::new("instances");
        let component = doc.add_component(Some("Part"), 0).unwrap();
        doc.body_components.insert("Pin".into(), component);
        let first = doc.occurrences[0].id;
        doc.occurrences[0].transform = crate::rigid(Vec3::new(10., 0., 0.), Vec3::ZERO, Vec3::Y, 0.3);
        doc.add_occurrence(component, 0, crate::rigid(Vec3::new(0., 20., 0.), Vec3::ZERO, Vec3::X, 0.6)).unwrap();
        let mut state = crate::ModelState::default();
        state.bodies.push(ModelBody::new("Pin".into(), kernel::cylinder(Vec3::ZERO, Vec3::Z, 2., 8.).unwrap(), 1));
        let mut cache = super::super::PlacedBodies::default();
        let before = super::super::world_state_reusing(&doc, &state, &mut cache);
        assert!(state.bodies[0].cached_mesh().is_none(), "placing bodies must remain lazy");
        let old = before.bodies[0].mesh();
        assert!(state.bodies[0].cached_mesh().is_some());
        let (_, work) = kernel::profile::capture(|| before.bodies[1].mesh());
        assert_eq!(work.tessellate_ms, 0., "the second instance shares the first one's triangulation");
        doc.occurrences.iter_mut().find(|o| o.id == first).unwrap().transform = crate::rigid(Vec3::new(30., 0., 0.), Vec3::ZERO, Vec3::Y, 0.9);
        let moved = super::super::world_state_reusing(&doc, &state, &mut cache);
        let (_, work) = kernel::profile::capture(|| moved.bodies[0].mesh());
        assert_eq!(work.tessellate_ms, 0., "a new pose shares the same definition");
        state.bodies[0] = ModelBody::new("Pin".into(), kernel::cylinder(Vec3::ZERO, Vec3::Z, 4., 8.).unwrap(), 1);
        let edited = super::super::world_state_reusing(&doc, &state, &mut cache);
        assert!(state.bodies[0].cached_mesh().is_none());
        let new = edited.bodies[0].mesh();
        assert!(state.bodies[0].cached_mesh().is_some(), "changed geometry warms its new definition cache");
        assert_ne!(new.bounds(), moved.bodies[0].mesh().bounds());
        assert!(Arc::ptr_eq(&old, &before.bodies[0].mesh()), "editing does not mutate older snapshots");
    }
    #[test]
    fn only_proper_rigid_affine_matrices_reuse_reference_meshes() {
        let source = ModelBody::new("Part".into(), kernel::box_solid(Vec3::ZERO, Vec3::new(2., 3., 4.)).unwrap(), 1);
        assert!(PlacedMesh::new(&source, &crate::IDENTITY).is_some());
        for (col, row, value) in [(0, 0, -1.), (0, 0, 2.), (0, 1, 0.5), (0, 3, 0.1), (3, 3, 2.), (3, 0, f64::NAN), (3, 2, f64::INFINITY)] {
            let mut matrix = crate::IDENTITY;
            matrix[col][row] = value;
            assert!(PlacedMesh::new(&source, &matrix).is_none(), "{matrix:?}");
        }
        // A positive unit determinant alone is insufficient: this scale changes normals and tolerance.
        let mut scale = crate::IDENTITY;
        scale[0][0] = 2.;
        scale[1][1] = 0.5;
        assert!(PlacedMesh::new(&source, &scale).is_none());
    }
}
