//! Filling a closed loop of a body's edges that is not planar with one face: a B-spline height
//! field over the loop's best-fit plane (`solidcraft_geom::fill::fill_loop`), bounded by the
//! body's own edges, whole. Loops that do not bound a height field over a plane are refused.

use solidcraft_geom::Vec3;
use solidcraft_geom::bspline::SplineError;
use truck_modeling as mt;

use crate::body::{Body, from_p3};
use crate::plane_fit::EdgeSampler;
use crate::record::{Rebuild, rebuilt_body};
use crate::{KernelError, Result};

/// A surface body of one face filling the closed loop `chain` of `body`'s edges (each edge
/// oriented along the loop, each ending where the next starts), within [`face_tolerance`] of
/// every edge. Its history is over `[body]`: the face is new, its edges the body's own. Call it
/// under [`crate::guard`].
pub(crate) fn fill_loop(body: &Body, chain: &[mt::Edge]) -> Result<Body> {
    let tol = face_tolerance(body);
    let mut sides: Vec<(EdgeSampler, Vec3)> = Vec::with_capacity(chain.len());
    for e in chain {
        let sampler = EdgeSampler::new(e).ok_or_else(|| KernelError::Invalid("an edge has no parameter range".into()))?;
        sides.push((sampler, from_p3(e.front().point())));
    }
    let mut point = |k: usize, t: f64| -> Option<Vec3> {
        let (sampler, start) = sides.get_mut(k)?;
        // The loop's corners are its vertices (an imported edge's curve may end a little off).
        if t <= 0.0 { Some(*start) } else { sampler.at(t) }
    };
    let fill = solidcraft_geom::fill::fill_loop(chain.len(), &mut point, tol).map_err(|e| match e {
        SplineError::Unsupported(m) => KernelError::Invalid(format!("not supported yet: {m}")),
        other => KernelError::Invalid(format!("cannot fill the loop: {other}")),
    })?;
    // The wire runs counter-clockwise about the surface's normal.
    let wire: mt::Wire =
        if fill.counter_clockwise { chain.to_vec().into() } else { chain.iter().rev().map(mt::Edge::inverse).collect::<Vec<_>>().into() };
    let face = mt::Face::try_new(vec![wire], crate::spline_conv::surface(&fill.surface)?).map_err(|e| KernelError::Failed(format!("fill: {e}")))?;
    let mut rec = Rebuild::default();
    rec.made(&face, Vec::new());
    Ok(rebuilt_body(crate::surfaces::surface_body(vec![face]), &[&body.solid], &rec))
}

/// How far a loop's edges may be from a plane for its patch to be that plane: `10⁻⁷` of the
/// body's size, at least the linear tolerance (`10⁻⁶` mm) — the body's own "same point"
/// tolerance, since a planar face is exact and other operations treat it as containing its
/// edges to that tolerance. A fitted face gets [`face_tolerance`], ten times looser.
pub(crate) fn planarity_tolerance(body: &Body) -> f64 {
    (body.size() * 1e-7).max(solidcraft_geom::LINEAR_TOL)
}

/// How far the fill's surface may be from the loop's edges: `10⁻⁶` of the body's size, at least
/// `10⁻⁵` mm — ten times [`planarity_tolerance`]. Truck meshes a face from its edges' own
/// polylines (shared with the neighbouring faces), so the mesh stays watertight whatever the
/// gap: measured on a five-sided rim, fills whose edges were up to `4.5·10⁻⁴` mm off the surface
/// meshed cleanly and stitched back into closed solids. Ten times the linear tolerance because a
/// fitted surface approaches curved edges like a cubic spline, so each further factor of ten
/// costs about twice the spans each way, and imported edges may already lie `10⁻⁵` mm or more off
/// their vertices; two orders below what meshing tolerates. Later operations must take the face
/// to contain its edges within this tolerance, not the linear one: the face's edges are the
/// body's exact edges, the surface between them is the fit.
pub(crate) fn face_tolerance(body: &Body) -> f64 {
    (body.size() * 1e-6).max(1e-5)
}

#[cfg(test)]
mod tests {
    use mt::{BoundedCurve, InnerSpace, MetricSpace, ParametricCurve, ParametricSurface};
    use solidcraft_geom::{Loop2, Plane, Region2, Vec2, Vec3};
    use truck_modeling as mt;

    use crate::body::{Body, from_p3, p3};
    use crate::boolean_record::tests::{Reach, checked};
    use crate::history::{Change, EdgeOrigin};
    use crate::{BoolOp, boolean, copy_faces, cylinder, extrude, measure, patch_edges, stitch};

    /// The cutting cylinder: axis along y at x = 0, z = 50, radius 40, so the top of a prism
    /// within 20 of the axis and 20 high is cut away everywhere (to between 10 and 15.4 mm).
    fn cut_height(x: f64) -> f64 {
        50.0 - (1600.0 - x * x).sqrt()
    }

    /// A prism on the polygon `corners`, 20 high, its top cut by the cylinder; the cut solid, the
    /// open cup without its top, and a point on each rim edge (the edges' midpoints in plan).
    /// `inside` is a point inside the polygon.
    fn cup(corners: &[Vec2], inside: Vec2) -> (Body, Body, Vec<Vec3>) {
        let prism = extrude(&Plane::XY, &[Region2 { outer: Loop2::polygon(corners), holes: vec![] }], 0.0, 20.0).unwrap().pop().unwrap();
        let tool = cylinder(Vec3::new(0.0, -60.0, 50.0), Vec3::Y, 40.0, 120.0).unwrap();
        let solid = boolean(&prism, &tool, BoolOp::Cut).unwrap().unwrap();
        let n = corners.len();
        let mut keep = vec![inside.extend(0.0)];
        let mut rim = Vec::new();
        for k in 0..n {
            let m = corners[k].lerp(corners[(k + 1) % n], 0.5);
            keep.push(m.extend(5.0));
            rim.push(m.extend(cut_height(m.x)));
        }
        let open = copy_faces(&solid, &keep).unwrap();
        assert!(open.is_surface());
        (solid, open, rim)
    }

    fn regular(n: usize, phase: f64) -> Vec<Vec2> {
        (0..n).map(|k| Vec2::from_angle(phase + k as f64 * std::f64::consts::TAU / n as f64) * 20.0).collect()
    }

    /// The distance from `p` to the surface: the nearest of a grid of samples, then Gauss–Newton
    /// within the domain.
    fn to_surface(s: &mt::Surface, p: mt::Point3) -> f64 {
        let (Some((u0, u1)), Some((v0, v1))) = s.try_range_tuple() else { panic!("a bounded surface") };
        let n = 80;
        let mut best = (f64::MAX, u0, v0);
        for i in 0..=n {
            for j in 0..=n {
                let (u, v) = (u0 + (u1 - u0) * i as f64 / n as f64, v0 + (v1 - v0) * j as f64 / n as f64);
                let d = s.subs(u, v).distance(p);
                if d < best.0 {
                    best = (d, u, v);
                }
            }
        }
        let (_, mut u, mut v) = best;
        for _ in 0..50 {
            let (q, a, b) = (s.subs(u, v), s.uder(u, v), s.vder(u, v));
            let r = q - p;
            let (aa, ab, bb) = (a.dot(a), a.dot(b), b.dot(b));
            let det = aa * bb - ab * ab;
            if det.abs() < 1e-300 {
                break;
            }
            let (ga, gb) = (a.dot(r), b.dot(r));
            u = (u - (bb * ga - ab * gb) / det).clamp(u0, u1);
            v = (v - (aa * gb - ab * ga) / det).clamp(v0, v1);
        }
        s.subs(u, v).distance(p)
    }

    /// The farthest point along the face's edges from its surface.
    fn edges_off_surface(face: &mt::Face) -> f64 {
        let s = face.surface();
        let mut worst = 0.0f64;
        for e in face.boundaries().iter().flat_map(|w| w.edge_iter()) {
            let c = e.curve();
            let (t0, t1) = c.range_tuple();
            for k in 0..=24 {
                worst = worst.max(to_surface(&s, c.subs(t0 + (t1 - t0) * k as f64 / 24.0)));
            }
        }
        worst
    }

    /// The fill's mesh: every normal finite, and no triangle folded over (all face the same way
    /// as the mesh's total area vector, by a clear margin).
    fn mesh_is_clean(fill: &Body) -> f64 {
        let m = fill.tessellate(0.01).unwrap();
        assert!(!m.triangles.is_empty());
        assert!(m.normals.iter().all(|n| n.is_finite() && n.len() > 0.5), "a mesh normal is not finite");
        let tri = |t: &[u32; 3]| m.tri(t).unwrap();
        let total = m.triangles.iter().map(|t| {
            let [a, b, c] = tri(t);
            (b - a).cross(c - a)
        });
        let total = total.fold(Vec3::ZERO, |s, v| s + v);
        let n = total.normalized().unwrap();
        for t in &m.triangles {
            let [a, b, c] = tri(t);
            let v = (b - a).cross(c - a);
            if v.len() > 1e-12 {
                assert!(v.dot(n) > 0.0, "a folded triangle");
            }
        }
        measure(fill).unwrap().area
    }

    /// Fill the rim, check the face, stitch it back: (the fill, the closed solid's volume).
    fn fill_rim(open: &Body, rim: &[Vec3]) -> (Body, f64) {
        let fill = patch_edges(open, rim).unwrap();
        assert!(fill.is_surface());
        assert_eq!(fill.face_count(), 1, "one face");
        let face = fill.solid.face_iter().next().unwrap().clone();
        assert!(!matches!(face.surface(), mt::Surface::Plane(_)));
        // Its edges are the open body's own, by identity, and nothing else.
        let own: Vec<mt::EdgeID> = open.solid.edge_iter().map(|e| e.id()).collect();
        assert!(fill.solid.edge_iter().all(|e| own.contains(&e.id())), "an edge that is not the body's");
        let h = checked(&fill, &[open], 0.05, Reach { made: 0.0, moved: 0.0 }, &|_| true);
        assert!(h.faces.iter().all(|f| f.change == Change::Generated && f.origins.is_empty()));
        assert!(h.edges.iter().all(|e| matches!(e.origins.as_slice(), [EdgeOrigin::Edge { operand: 0, .. }])));
        let off = edges_off_surface(&face);
        let tol = super::face_tolerance(open);
        assert!(off <= tol, "an edge is {off} off the face (tolerance {tol})");
        mesh_is_clean(&fill);
        let closed = stitch(&[open, &fill], 0.0).unwrap();
        assert!(!closed.is_surface(), "the fill closes the cup");
        (fill, measure(&closed).unwrap().volume)
    }

    /// The fill's height over the point `(x, y)`: Newton's method on the surface's `x`, `y` from
    /// the nearest of a grid of samples.
    fn height_over(s: &mt::Surface, x: f64, y: f64, start: &mut Option<(f64, f64)>) -> Option<f64> {
        let (Some((u0, u1)), Some((v0, v1))) = s.try_range_tuple() else { return None };
        let (mut u, mut v) = start.unwrap_or(((u0 + u1) / 2.0, (v0 + v1) / 2.0));
        for _ in 0..30 {
            let (q, a, b) = (s.subs(u, v), s.uder(u, v), s.vder(u, v));
            let (rx, ry) = (q.x - x, q.y - y);
            let det = a.x * b.y - a.y * b.x;
            if det.abs() < 1e-300 {
                return None;
            }
            u = (u - (b.y * rx - b.x * ry) / det).clamp(u0, u1);
            v = (v - (a.x * ry - a.y * rx) / det).clamp(v0, v1);
        }
        let q = s.subs(u, v);
        *start = Some((u, v));
        ((q.x - x).hypot(q.y - y) < 1e-9).then_some(q.z)
    }

    /// The volume the cut prism gains with the fill for its top: the fill's height above the
    /// cylinder integrated over the polygon (midpoint rule on a 300 × 300 grid; the integrand
    /// vanishes on the rim, so cells across it add little error).
    fn gained(fill: &Body, corners: &[Vec2]) -> f64 {
        let s = fill.solid.face_iter().next().unwrap().surface();
        let (lo, hi) = corners.iter().fold((Vec2::new(f64::MAX, f64::MAX), Vec2::new(f64::MIN, f64::MIN)), |(lo, hi), p| {
            (Vec2::new(lo.x.min(p.x), lo.y.min(p.y)), Vec2::new(hi.x.max(p.x), hi.y.max(p.y)))
        });
        let n = 300;
        let (dx, dy) = ((hi.x - lo.x) / n as f64, (hi.y - lo.y) / n as f64);
        let inside = |x: f64, y: f64| {
            let mut w = false;
            for k in 0..corners.len() {
                let (a, b) = (corners[k], corners[(k + 1) % corners.len()]);
                if (a.y > y) != (b.y > y) && x < a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x) {
                    w = !w;
                }
            }
            w
        };
        let mut sum = 0.0;
        let mut start = None;
        for i in 0..n {
            let x = lo.x + (i as f64 + 0.5) * dx;
            for j in 0..n {
                let y = lo.y + (j as f64 + 0.5) * dy;
                if inside(x, y) {
                    let h = height_over(&s, x, y, &mut start).expect("the fill covers the polygon");
                    sum += (h - cut_height(x)) * dx * dy;
                }
            }
        }
        sum
    }

    /// The closed solid's volume against the cut prism's plus what the fill adds over the
    /// cylinder (integrated independently, [`gained`]); they agree to the integration's error
    /// (0.1% of what the fill adds) plus the error of measuring both solids on meshes
    /// (`5·10⁻⁵` of the volume).
    fn check_volume(name: &str, solid: &Body, fill: &Body, corners: &[Vec2], got: f64) {
        let prism = measure(solid).unwrap().volume;
        let extra = gained(fill, corners);
        let want = prism + extra;
        let bound = 1e-3 * extra.abs() + 5e-5 * want;
        eprintln!("{name}: closed volume {got:.4}, cut prism {prism:.4} + fill above the cylinder {extra:.4} = {want:.4} (bound ±{bound:.4})");
        assert!((got - want).abs() <= bound, "{name}: {got} vs {want}");
    }

    #[test]
    fn regular_rims_of_three_to_eight_sides_fill_with_one_face_and_close_the_cup() {
        for n in 3..=8 {
            let corners = regular(n, 0.3);
            let (solid, open, rim) = cup(&corners, Vec2::ZERO);
            let (fill, got) = fill_rim(&open, &rim);
            assert_eq!(fill.edge_count(), n, "the rim's own edges");
            check_volume(&format!("{n} sides"), &solid, &fill, &corners, got);
        }
    }

    #[test]
    fn concave_rims_fill_with_one_face() {
        let v = |x: f64, y: f64| Vec2::new(x, y);
        let shapes = [
            ("dart", vec![v(-10.0, 0.0), v(15.0, -12.0), v(3.0, 0.0), v(15.0, 12.0)], v(0.0, 0.0)),
            ("arrow", vec![v(-12.0, 0.0), v(0.0, -10.0), v(0.0, -4.0), v(14.0, 0.0), v(0.0, 4.0)], v(0.0, 0.0)),
            (
                "plus",
                vec![
                    v(-4.0, -14.0),
                    v(4.0, -14.0),
                    v(4.0, -4.0),
                    v(14.0, -4.0),
                    v(14.0, 4.0),
                    v(4.0, 4.0),
                    v(4.0, 14.0),
                    v(-4.0, 14.0),
                    v(-4.0, 4.0),
                    v(-14.0, 4.0),
                    v(-14.0, -4.0),
                    v(-4.0, -4.0),
                ],
                v(0.0, 0.0),
            ),
        ];
        for (name, corners, inside) in shapes {
            let (solid, open, rim) = cup(&corners, inside);
            let (fill, got) = fill_rim(&open, &rim);
            assert_eq!(fill.edge_count(), corners.len(), "{name}: the rim's own edges");
            check_volume(name, &solid, &fill, &corners, got);
        }
    }

    #[test]
    fn a_flat_corner_between_two_edges_is_kept() {
        // A rim of five straight edges, two of them in line (a 180° corner between them), the
        // four true corners at different heights; walls hang below it.
        let p = |x: f64, y: f64, z: f64| mt::Vertex::new(mt::Point3::new(x, y, z));
        let v = [p(-14.0, -14.0, 12.0), p(0.0, -14.0, 12.0), p(14.0, -14.0, 12.0), p(14.0, 14.0, 15.0), p(-14.0, 14.0, 10.0)];
        let line = |a: &mt::Vertex, b: &mt::Vertex| mt::Edge::new(a, b, mt::Curve::Line(mt::Line(a.point(), b.point())));
        let wire: mt::Wire = (0..5).map(|k| line(&v[k], &v[(k + 1) % 5])).collect::<Vec<_>>().into();
        let low = mt::builder::translated(&wire, mt::Vector3::new(0.0, 0.0, -10.0));
        let walls = mt::builder::try_wire_homotopy(&wire, &low).unwrap();
        let body = crate::surfaces::surface_body(walls.face_iter().cloned().collect());
        let rim: Vec<Vec3> = (0..5).map(|k| from_p3(v[k].point()).lerp(from_p3(v[(k + 1) % 5].point()), 0.5)).collect();
        let fill = patch_edges(&body, &rim).unwrap();
        assert_eq!((fill.face_count(), fill.edge_count()), (1, 5), "one face on all five edges");
        let face = fill.solid.face_iter().next().unwrap().clone();
        assert!(edges_off_surface(&face) <= super::face_tolerance(&body));
        let area = mesh_is_clean(&fill);
        eprintln!("flat corner: one face of area {area}");
        assert!(area > 28.0 * 28.0 * 0.99 && area < 28.0 * 28.0 * 1.05, "{area}");
    }

    #[test]
    fn the_fill_does_not_depend_on_pick_order_or_where_the_body_is() {
        let (_, open, rim) = cup(&regular(5, 0.3), Vec2::ZERO);
        let (a, _) = fill_rim(&open, &rim);
        let mut shuffled = rim.clone();
        shuffled.rotate_left(3);
        shuffled.swap(0, 1);
        let (b, _) = fill_rim(&open, &shuffled);
        let (sa, sb) = (a.solid.face_iter().next().unwrap().surface(), b.solid.face_iter().next().unwrap().surface());
        let (Some((u0, u1)), Some((v0, v1))) = sa.try_range_tuple() else { panic!() };
        let mut gap = 0.0f64;
        for (x, y) in [(0.2, 0.3), (0.5, 0.5), (0.7, 0.9)] {
            let (u, v) = (u0 + (u1 - u0) * x, v0 + (v1 - v0) * y);
            gap = gap.max(sa.subs(u, v).distance(sb.subs(u, v)));
        }
        assert!(gap < 1e-7, "pick order moved the surface by {gap}");
        let (area_a, area_b) = (mesh_is_clean(&a), mesh_is_clean(&b));
        assert!((area_a - area_b).abs() < area_a * 1e-9);
        // Moved and turned: the same area.
        let moved = crate::transform(&open, Vec3::new(130.0, -70.0, 40.0), Vec3::ZERO, Vec3::new(1.0, 2.0, 3.0), 0.7).unwrap();
        let ax = Vec3::new(1.0, 2.0, 3.0).normalized().unwrap();
        let turn = |p: Vec3| {
            // Rodrigues' rotation by 0.7 about `ax`, then the shift.
            let (c, s) = (0.7f64.cos(), 0.7f64.sin());
            p * c + ax.cross(p) * s + ax * (ax.dot(p) * (1.0 - c)) + Vec3::new(130.0, -70.0, 40.0)
        };
        let moved_rim: Vec<Vec3> = rim.iter().map(|p| turn(*p)).collect();
        let (c, _) = fill_rim(&moved, &moved_rim);
        let area_c = mesh_is_clean(&c);
        eprintln!("pick order: surfaces within {gap:e}; areas {area_a} / {area_b} / moved {area_c}");
        assert!((area_a - area_c).abs() < area_a * 1e-6, "{area_a} vs moved {area_c}");
    }

    #[test]
    fn a_saddle_rim_of_two_intersection_curves_fills() {
        // A round post with its top cut by the sideways cylinder: the rim is two intersection
        // curves; the fill is one face on them.
        let post = cylinder(Vec3::ZERO, Vec3::Z, 10.0, 20.0).unwrap();
        let tool = cylinder(Vec3::new(0.0, -60.0, 50.0), Vec3::Y, 40.0, 120.0).unwrap();
        let solid = boolean(&post, &tool, BoolOp::Cut).unwrap().unwrap();
        let sides: Vec<Vec3> = (0..8).map(|k| Vec2::from_angle(k as f64 * 0.8).extend(0.0) * 10.0 + Vec3::new(0.0, 0.0, 5.0)).collect();
        let open = copy_faces(&solid, &[&[Vec3::ZERO][..], &sides].concat()).unwrap();
        let rim: Vec<Vec3> = [-0.98, 2.18]
            .map(|a: f64| {
                let (x, y) = (10.0 * a.cos(), 10.0 * a.sin());
                Vec3::new(x, y, cut_height(x))
            })
            .to_vec();
        let (fill, got) = fill_rim(&open, &rim);
        let circle: Vec<Vec2> = (0..720).map(|k| Vec2::from_angle(k as f64 * std::f64::consts::TAU / 720.0) * 10.0).collect();
        check_volume("saddle rim", &solid, &fill, &circle, got);
    }

    #[test]
    fn imported_edges_whose_vertices_are_off_their_curves_fill() {
        // The cup's vertices moved 2·10⁻⁵ mm off their curves, as a file might give them.
        let corners = regular(5, 0.3);
        let (solid, open, rim) = cup(&corners, Vec2::ZERO);
        let shell = open.solid.boundaries()[0].mapped(
            |p: &mt::Point3| *p + mt::Vector3::new(2e-5, 0.0, 0.0),
            |c: &mt::Curve| c.clone(),
            |s: &mt::Surface| s.clone(),
        );
        let imported = crate::surfaces::surface_body(shell.face_iter().cloned().collect());
        let (fill, got) = fill_rim(&imported, &rim);
        check_volume("imported", &solid, &fill, &corners, got);
    }

    #[test]
    fn a_spline_edge_bulging_out_of_plane_is_never_given_a_flat_face() {
        // A flat rectangle of edges but for one cubic edge with a control point raised 1.5 mm: no
        // plane can be certified. A one-span bump is followed by the fitted face; a narrow one
        // (64 spans, about 1 mm wide, its third derivative jumping at every knot) may be beyond
        // a 32 × 32 grid, and then it is refused.
        for (spans, raised, must_fill) in [(1, 1, true), (64, 30, false)] {
            let count = spans + 3;
            let pts: Vec<mt::Point3> =
                (0..count).map(|i| mt::Point3::new(20.0 * i as f64 / (count - 1) as f64, 0.0, if i == raised { 1.5 } else { 0.0 })).collect();
            let spline = mt::BSplineCurve::new(mt::KnotVec::uniform_knot(3, spans), pts);
            let v: Vec<mt::Vertex> =
                [(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (0.0, 10.0)].iter().map(|&(x, y)| mt::Vertex::new(mt::Point3::new(x, y, 0.0))).collect();
            let line = |a: &mt::Vertex, b: &mt::Vertex| mt::Edge::new(a, b, mt::Curve::Line(mt::Line(a.point(), b.point())));
            let bottom = mt::Edge::new(&v[0], &v[1], mt::Curve::BSplineCurve(spline.clone()));
            let wire: mt::Wire = vec![bottom, line(&v[1], &v[2]), line(&v[2], &v[3]), line(&v[3], &v[0])].into();
            let top = mt::builder::translated(&wire, mt::Vector3::new(0.0, 0.0, -5.0));
            let walls = mt::builder::try_wire_homotopy(&wire, &top).unwrap();
            let body = crate::surfaces::surface_body(walls.face_iter().cloned().collect());
            let peak = from_p3(spline.subs((raised as f64 + 1.0) / (count + 1) as f64));
            let rim = [from_p3(spline.subs(0.5)), Vec3::new(20.0, 5.0, 0.0), Vec3::new(10.0, 10.0, 0.0), Vec3::new(0.0, 5.0, 0.0)];
            match patch_edges(&body, &rim) {
                Ok(fill) => {
                    let face = fill.solid.face_iter().next().unwrap().clone();
                    assert!(!matches!(face.surface(), mt::Surface::Plane(_)), "a plane would miss the bulge");
                    let (off, tol) = (edges_off_surface(&face), super::face_tolerance(&body));
                    eprintln!("{spans}-span bump: filled, edges within {off:e} of the face");
                    assert!(off <= tol && to_surface(&face.surface(), p3(peak)) <= tol);
                    mesh_is_clean(&fill);
                }
                Err(e) => {
                    eprintln!("{spans}-span bump: {e}");
                    assert!(!must_fill && e.to_string().contains("not supported yet"), "{e}");
                }
            }
        }
    }

    /// A point on each edge that bounds one face of `body` (its free edges).
    fn free_edge_points(body: &Body) -> Vec<Vec3> {
        let mut uses: std::collections::HashMap<mt::EdgeID, (usize, mt::Edge)> = std::collections::HashMap::new();
        for f in body.solid.face_iter() {
            for e in f.boundaries().iter().flat_map(|w| w.edge_iter()) {
                uses.entry(e.id()).or_insert((0, e.clone())).0 += 1;
            }
        }
        uses.values()
            .filter(|(n, _)| *n == 1)
            .map(|(_, e)| {
                let c = e.curve();
                let (t0, t1) = c.range_tuple();
                from_p3(c.subs(0.5 * (t0 + t1)))
            })
            .collect()
    }

    #[test]
    fn a_loop_that_does_not_bound_a_height_field_is_refused() {
        // Half a round post's side: two straight edges up and two half circles, whose plane
        // stands on the post's axis; the half circles run along its normal at their ends.
        let post = cylinder(Vec3::ZERO, Vec3::Z, 10.0, 20.0).unwrap();
        let halves = crate::split_by_plane(&post, &Plane::from_normal(Vec3::ZERO, Vec3::X).unwrap()).unwrap();
        let half = halves.into_iter().find(|b| measure(b).unwrap().bbox.min.x > -1e-6).unwrap();
        let at: Vec<Vec3> =
            (-4..=4).map(|k| Vec2::from_angle(k as f64 * 0.35).extend(10.0) * 1.0).map(|p| Vec3::new(p.x * 10.0, p.y * 10.0, 10.0)).collect();
        let side = copy_faces(&half, &at).unwrap();
        let rim = free_edge_points(&side);
        let err = patch_edges(&side, &rim).unwrap_err().to_string();
        eprintln!("half a post's side: {err}");
        assert!(err.contains("not supported yet"), "{err}");
    }

    #[test]
    fn planar_loops_of_intersection_curves_are_one_planar_face() {
        // A round post cut by planes tilted every way, gently to steeply: each rim is an ellipse
        // of intersection curves (split where the post's side has seams) lying on the cutting
        // plane, so the patch is one planar face, and the post closes again.
        for tilt in [Vec3::new(0.0, -0.4, 1.0), Vec3::new(0.7, -0.9, 1.0), Vec3::new(1.3, 0.2, 1.0)] {
            let post = cylinder(Vec3::ZERO, Vec3::Z, 10.0, 30.0).unwrap();
            let n = tilt.normalized().unwrap();
            let plane = Plane::from_normal(Vec3::new(0.0, 0.0, 15.0), n).unwrap();
            let pieces = crate::split_by_plane(&post, &plane).unwrap();
            let solid = pieces.into_iter().find(|b| measure(b).unwrap().bbox.min.z < 0.5).unwrap();
            let height = |x: f64, y: f64| 15.0 - (tilt.x * x + tilt.y * y) / tilt.z;
            let sides: Vec<Vec3> = (0..8).map(|k| Vec2::from_angle(k as f64 * 0.8).extend(0.0) * 10.0 + Vec3::new(0.0, 0.0, 1.0)).collect();
            let open = copy_faces(&solid, &[&[Vec3::ZERO][..], &sides].concat()).unwrap();
            let rim: Vec<Vec3> = (0..12)
                .map(|k| {
                    let a = k as f64 * std::f64::consts::TAU / 12.0 + 0.2;
                    let (x, y) = (10.0 * a.cos(), 10.0 * a.sin());
                    Vec3::new(x, y, height(x, y))
                })
                .collect();
            let lid = patch_edges(&open, &rim).unwrap();
            assert_eq!(lid.face_count(), 1);
            let edges = lid.edge_count();
            assert!(matches!(lid.solid.face_iter().next().unwrap().surface(), mt::Surface::Plane(_)), "tilt {tilt:?}");
            let closed = stitch(&[&open, &lid], 0.0).unwrap();
            assert!(!closed.is_surface());
            let (want, got) = (measure(&solid).unwrap().volume, measure(&closed).unwrap().volume);
            eprintln!("rim cut at {tilt:?}: {edges} intersection curves, planar face, volume {got} against {want}");
            assert!((got - want).abs() < want * 1e-6, "{got} vs {want}");
        }
    }

    #[test]
    fn planar_loops_are_still_one_planar_face() {
        let b = crate::box_solid(Vec3::ZERO, Vec3::new(10.0, 10.0, 10.0)).unwrap();
        let open = copy_faces(
            &b,
            &[Vec3::new(5.0, 5.0, 0.0), Vec3::new(5.0, 0.0, 5.0), Vec3::new(10.0, 5.0, 5.0), Vec3::new(5.0, 10.0, 5.0), Vec3::new(0.0, 5.0, 5.0)],
        )
        .unwrap();
        let rim = [Vec3::new(5.0, 0.0, 10.0), Vec3::new(10.0, 5.0, 10.0), Vec3::new(5.0, 10.0, 10.0), Vec3::new(0.0, 5.0, 10.0)];
        let lid = patch_edges(&open, &rim).unwrap();
        assert_eq!(lid.face_count(), 1);
        assert!(matches!(lid.solid.face_iter().next().unwrap().surface(), mt::Surface::Plane(_)));
        let closed = stitch(&[&open, &lid], 0.0).unwrap();
        assert!((measure(&closed).unwrap().volume - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn open_partial_or_hostile_picks_are_refused() {
        let (_, open, rim) = cup(&regular(5, 0.3), Vec2::ZERO);
        assert!(patch_edges(&open, &rim[..4]).is_err(), "a loop with a missing edge");
        assert!(patch_edges(&open, &[]).is_err());
        assert!(patch_edges(&open, &[Vec3::new(f64::NAN, 0.0, 0.0)]).is_err());
        assert!(patch_edges(&open, &[Vec3::new(1e300, 0.0, 0.0)]).is_err());
        assert!(patch_edges(&open, &vec![rim[0]; 20_000]).is_err());
    }
}
