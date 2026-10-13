use truck_meshalgo::prelude::*;
use truck_modeling::*;

#[test]
fn punched_cube() {
    let v = builder::vertex(Point3::origin());
    let e = builder::tsweep(&v, Vector3::unit_x());
    let f = builder::tsweep(&e, Vector3::unit_y());
    let cube = builder::tsweep(&f, Vector3::unit_z());

    let v = builder::vertex(Point3::new(0.5, 0.25, -0.5));
    let w = builder::rsweep(&v, Point3::new(0.5, 0.5, 0.0), Vector3::unit_z(), Rad(7.0));
    let f = builder::try_attach_plane(&[w]).unwrap();
    let mut cylinder = builder::tsweep(&f, Vector3::unit_z() * 2.0);
    cylinder.not();
    let and = crate::and(&cube, &cylinder, 0.05).unwrap();

    let poly = and.triangulation(0.01).to_polygon();
    let file = std::fs::File::create("punched-cube.obj").unwrap();
    obj::write(&poly, file).unwrap();
}

// SolidCraft: boolean history.

use crate::{EdgeOrigin, FaceIndex, History};

fn cuboid(origin: Point3, size: Vector3) -> Solid {
    let v = builder::vertex(origin);
    let e = builder::tsweep(&v, size.x * Vector3::unit_x());
    let f = builder::tsweep(&e, size.y * Vector3::unit_y());
    builder::tsweep(&f, size.z * Vector3::unit_z())
}

/// Distinct edges in order of first use along the faces.
fn edges_in_order(solid: &Solid) -> Vec<Edge> {
    let mut seen = std::collections::HashSet::new();
    solid.face_iter().flat_map(Face::edge_iter).filter(|e| seen.insert(e.id())).collect()
}

fn on_surface(surface: &Surface, pt: Point3, tol: f64) -> bool {
    surface.search_nearest_parameter(pt, None, 100).is_some_and(|(u, v)| surface.subs(u, v).distance(pt) < tol)
}

fn on_curve(curve: &Curve, pt: Point3, tol: f64) -> bool {
    curve.search_nearest_parameter(pt, None, 100).is_some_and(|t| curve.subs(t).distance(pt) < tol)
}

/// Checks `history` against the geometry of `result` and the operands; returns how many result
/// faces are kept operand faces and the intersection edges.
fn check(result: &Solid, operands: [&Solid; 2], history: &History, tol: f64) -> (usize, Vec<[FaceIndex; 2]>) {
    let operand_faces = operands.map(|s| s.face_iter().collect::<Vec<_>>());
    let face = |f: FaceIndex| operand_faces[f.operand][f.face];
    assert_eq!(history.faces.len(), result.face_iter().count());
    for (out, origin) in result.face_iter().zip(&history.faces) {
        let src = face(origin.source);
        assert_eq!(out.orientation(), src.orientation());
        assert_eq!(out.id() == src.id(), origin.kept);
        assert!(out.vertex_iter().all(|v| on_surface(&src.surface(), v.point(), tol)));
    }
    let edges = edges_in_order(result);
    assert_eq!(history.edges.len(), edges.len());
    let operand_edges = operands.map(edges_in_order);
    let mut meets = Vec::new();
    for (out, origin) in edges.iter().zip(&history.edges) {
        let ends = [out.front().point(), out.back().point()];
        match *origin {
            EdgeOrigin::Edge { operand, edge } => {
                let src = &operand_edges[operand][edge];
                assert!(ends.iter().all(|&p| on_curve(&src.curve(), p, tol)));
            }
            EdgeOrigin::Meet { a, b } => {
                assert!(a < b);
                for f in [a, b] {
                    assert!(ends.iter().all(|&p| on_surface(&face(f).surface(), p, tol)));
                }
                meets.push([a, b]);
            }
        }
    }
    (history.faces.iter().filter(|o| o.kept).count(), meets)
}

#[test]
fn box_union_box_history() {
    let a = cuboid(Point3::origin(), Vector3::new(1.0, 1.0, 1.0));
    let b = cuboid(Point3::new(0.5, 0.25, 0.25), Vector3::new(1.0, 0.5, 0.5));
    let (or, history) = crate::or_with_history(&a, &b, 0.05).unwrap();
    let history = history.unwrap();
    let (kept, meets) = check(&or, [&a, &b], &history, 0.05);
    // a's five faces away from b and b's far end are kept; a's face at x = 1 gets a hole and
    // b's four sides are trimmed where they enter a.
    assert_eq!(history.faces.len(), 11);
    assert_eq!(kept, 6);
    let pieces: Vec<_> = history.faces.iter().filter(|o| !o.kept).map(|o| o.source).collect();
    assert_eq!(pieces.iter().filter(|f| f.operand == 0).count(), 1);
    assert_eq!(pieces.iter().filter(|f| f.operand == 1).count(), 4);
    // b's sides meet a's face at x = 1 along four new edges.
    assert_eq!(meets.len(), 4);
    assert!(meets.iter().all(|[x, y]| x.operand == 0 && y.operand == 1 && *x == pieces[0]));
    // a's 12 edges, b's 4 far-end edges and the 4 trimmed edges along x.
    assert_eq!(history.edges.len(), 24);
    let from_b = history.edges.iter().filter(|o| matches!(o, EdgeOrigin::Edge { operand: 1, .. })).count();
    assert_eq!(from_b, 8);
}

#[test]
fn box_minus_cylinder_history() {
    let cube = cuboid(Point3::origin(), Vector3::new(1.0, 1.0, 1.0));
    let v = builder::vertex(Point3::new(0.5, 0.25, -0.5));
    let w = builder::rsweep(&v, Point3::new(0.5, 0.5, 0.0), Vector3::unit_z(), Rad(7.0));
    let f = builder::try_attach_plane(&[w]).unwrap();
    let mut cylinder = builder::tsweep(&f, Vector3::unit_z() * 2.0);
    cylinder.not();
    let (cut, history) = crate::and_with_history(&cube, &cylinder, 0.05).unwrap();
    let history = history.unwrap();
    let (kept, meets) = check(&cut, [&cube, &cylinder], &history, 0.05);
    // The cube's four sides are kept; its top and bottom get holes; only the cylinder's wall
    // remains of the tool, trimmed to the cube.
    assert_eq!(kept, 4);
    assert!(history.faces.iter().filter(|o| o.source.operand == 1).all(|o| !o.kept));
    // The holes' rims: the wall meets the top and the bottom.
    assert!(!meets.is_empty());
    assert!(meets.iter().all(|[x, y]| x.operand == 0 && y.operand == 1));
    let rims: std::collections::BTreeSet<_> = meets.iter().map(|[x, _]| *x).collect();
    assert_eq!(rims.len(), 2);
}

/// A cube [0, 3]³ with a cubic void [1, 2]³: two shells, the void's faces after the outer ones.
fn hollow_cube() -> Solid {
    let mut void = cuboid(Point3::new(1.0, 1.0, 1.0), Vector3::new(1.0, 1.0, 1.0));
    void.not();
    let outer = cuboid(Point3::origin(), Vector3::new(3.0, 3.0, 3.0));
    Solid::new(outer.into_boundaries().into_iter().chain(void.into_boundaries()).collect())
}

#[test]
fn multi_shell_operand_history() {
    let hollow = hollow_cube();
    // Covers the upper half of the hollow cube: its bottom crosses both shells.
    let slab = cuboid(Point3::new(-0.5, -0.5, 1.5), Vector3::new(4.0, 4.0, 2.5));
    let (and, history) = crate::and_with_history(&hollow, &slab, 0.05).unwrap();
    let history = history.unwrap();
    let (kept, meets) = check(&and, [&hollow, &slab], &history, 0.05);
    // The outer top and the void's top (numbered after the outer shell's six faces) are kept.
    assert_eq!(kept, 2);
    assert!(history.faces.iter().any(|o| o.kept && o.source.operand == 0 && o.source.face >= 6));
    // The slab's bottom meets the four outer sides and the void's four sides.
    let outer = meets.iter().filter(|[x, _]| x.operand == 0 && x.face < 6).count();
    let void = meets.iter().filter(|[x, _]| x.operand == 0 && x.face >= 6).count();
    assert_eq!((outer, void), (4, 4), "{meets:?}");
    let slab_faces: std::collections::BTreeSet<_> = meets.iter().map(|[_, y]| *y).collect();
    assert_eq!(slab_faces.len(), 1);
}
