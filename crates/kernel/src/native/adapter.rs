//! The truck adapter. Persisted data belongs to solidcraft_geom::brep, never truck.
use super::{Result, invalid};
use mt::Invertible;
use solidcraft_geom::brep as b;
use truck_modeling as mt;

#[path = "analytic.rs"]
mod analytic;
use analytic::{arc, full_turn, load_analytic, revolved};
fn point(p: mt::Point3) -> [f64; 3] {
    [p.x, p.y, p.z]
}
fn vector(p: mt::Vector3) -> [f64; 3] {
    [p.x, p.y, p.z]
}
fn vector4(p: mt::Vector4) -> [f64; 4] {
    [p.x, p.y, p.z, p.w]
}
fn p([x, y, z]: [f64; 3]) -> mt::Point3 {
    mt::Point3::new(x, y, z)
}
fn v([x, y, z]: [f64; 3]) -> mt::Vector3 {
    mt::Vector3::new(x, y, z)
}
fn v4([x, y, z, w]: [f64; 4]) -> mt::Vector4 {
    mt::Vector4::new(x, y, z, w)
}
fn index(i: usize) -> Result<u32> {
    u32::try_from(i).map_err(invalid)
}
fn depth(d: usize) -> Result<()> {
    if d > 16 { Err(invalid("geometry nesting")) } else { Ok(()) }
}

// Truck evaluates the full knot interval. The portable domain is the standard
// active interval, so only coincident (clamped) domains can cross this adapter.
fn clamped(knots: &[f64], count: usize) -> Result<()> {
    let degree = knots.len().checked_sub(count + 1).ok_or_else(|| invalid("spline degree"))?;
    if knots.first() != knots.get(degree) || knots.last() != knots.get(count) {
        return Err(invalid("unclamped spline domain unsupported by this adapter"));
    }
    Ok(())
}
fn compatible_curve(c: &b::Curve) -> Result<()> {
    match c {
        b::Curve::Spline { knots, points } => clamped(knots, points.len()),
        b::Curve::RationalSpline { knots, homogeneous } => clamped(knots, homogeneous.len()),
        b::Curve::Intersection { knots, points, .. } => clamped(knots, points.len()),
        // The producer's own trace: its surfaces must load; its leader is kept as it was.
        b::Curve::SurfaceIntersection { surfaces, .. } => surfaces.iter().try_for_each(|s| compatible_surface(s)),
        _ => Ok(()),
    }
}
fn compatible_surface(s: &b::Surface) -> Result<()> {
    match s {
        b::Surface::Plane { origin, u_axis, v_axis } => {
            let plane = mt::Plane::new(p(*origin), p(*origin) + v(*u_axis), p(*origin) + v(*v_axis));
            if vector(plane.u_axis()) != *u_axis || vector(plane.v_axis()) != *v_axis {
                return Err(invalid("plane axes cannot be reconstructed exactly"));
            }
            Ok(())
        }
        b::Surface::Spline { u_knots, v_knots, points } => {
            clamped(u_knots, points.len())?;
            clamped(v_knots, points.first().map_or(0, Vec::len))
        }
        b::Surface::RationalSpline { u_knots, v_knots, homogeneous } => {
            clamped(u_knots, homogeneous.len())?;
            clamped(v_knots, homogeneous.first().map_or(0, Vec::len))
        }
        b::Surface::Revolution { profile, angle, .. } => {
            full_turn(angle)?;
            compatible_curve(profile)
        }
        b::Surface::PlaneThrough { .. } => Ok(()),
        b::Surface::PlacedRevolution { profile, .. } => compatible_curve(profile),
        b::Surface::Cylinder { angle, .. } | b::Surface::Cone { angle, .. } => full_turn(angle),
        b::Surface::Sphere { longitude, .. } | b::Surface::Torus { longitude, .. } => full_turn(longitude),
    }
}
fn compatible(body: &b::Brep) -> Result<()> {
    for shell in &body.shells {
        for edge in &shell.edges {
            compatible_curve(&edge.curve)?;
        }
        for face in &shell.faces {
            compatible_surface(&face.surface)?;
        }
    }
    Ok(())
}

pub(super) fn encode(solid: &mt::Solid) -> Result<b::Brep> {
    let mut shells = Vec::new();
    for shell in solid.compress().boundaries {
        let vertices = shell.vertices.into_iter().map(point).collect();
        let edges = shell
            .edges
            .into_iter()
            .map(|e| Ok(b::Edge { vertices: [index(e.vertices.0)?, index(e.vertices.1)?], curve: save_curve(&e.curve, 0)? }))
            .collect::<Result<_>>()?;
        let faces = shell
            .faces
            .into_iter()
            .map(|f| {
                let surface = save_surface(&f.surface, 0)?;
                Ok(b::Face {
                    loops: f
                        .boundaries
                        .into_iter()
                        .map(|w| {
                            let mut uses = w
                                .into_iter()
                                .map(|e| Ok(b::EdgeUse { edge: index(e.index)?, forward: e.orientation }))
                                .collect::<Result<Vec<_>>>()?;
                            // Wire direction is explicit; Face.forward changes only the normal.
                            if !f.orientation {
                                reverse_wire(&mut uses);
                            }
                            Ok(uses)
                        })
                        .collect::<Result<_>>()?,
                    surface,
                    forward: f.orientation,
                })
            })
            .collect::<Result<_>>()?;
        shells.push(b::Shell { vertices, edges, faces });
    }
    let result = b::Brep { version: b::VERSION, shells };
    result.validate().map_err(invalid)?;
    compatible(&result)?;
    Ok(result)
}
pub(super) fn decode(body: b::Brep) -> Result<mt::Solid> {
    body.validate().map_err(invalid)?;
    compatible(&body)?;
    let shells = body.shells.into_iter().map(shell).collect::<Result<Vec<_>>>()?;
    if shells.iter().any(|s| s.shell_condition() == mt::ShellCondition::Irregular) {
        return Err(invalid("non-manifold shell"));
    }
    Ok(mt::Solid::new_unchecked(shells))
}
/// A shell's topology as recorded, references already checked: built as the kernel built it,
/// without truck's checks for wires that touch themselves (a seam edge used twice, a closed edge
/// on one vertex), which imported and modelled faces have.
fn shell(s: b::Shell) -> Result<mt::Shell> {
    let vertices: Vec<mt::Vertex> = s.vertices.into_iter().map(|v| mt::Vertex::new(p(v))).collect();
    let vertex = |i: u32| vertices.get(i as usize).ok_or_else(|| invalid("edge vertices"));
    let edges = s
        .edges
        .into_iter()
        .map(|e| Ok(mt::Edge::new_unchecked(vertex(e.vertices[0])?, vertex(e.vertices[1])?, load_curve(e.curve)?)))
        .collect::<Result<Vec<_>>>()?;
    s.faces
        .into_iter()
        .map(|f| {
            let mut loops = f.loops;
            if !f.forward {
                for wire in &mut loops {
                    reverse_wire(wire);
                }
            }
            let wires = loops
                .into_iter()
                .map(|w| {
                    w.into_iter()
                        .map(|u| {
                            let e = edges.get(u.edge as usize).ok_or_else(|| invalid("edge reference"))?;
                            Ok(if u.forward { e.clone() } else { e.inverse() })
                        })
                        .collect::<Result<mt::Wire>>()
                })
                .collect::<Result<Vec<_>>>()?;
            let mut face = mt::Face::new_unchecked(wires, load_surface(f.surface)?);
            if !f.forward {
                face.invert();
            }
            Ok(face)
        })
        .collect()
}
fn reverse_wire(wire: &mut [b::EdgeUse]) {
    wire.reverse();
    for edge in wire {
        edge.forward = !edge.forward;
    }
}
/// Exact: every record holds the producer's own numbers, so loading it gives back the same
/// curve, never a refit.
fn save_curve(c: &mt::Curve, d: usize) -> Result<b::Curve> {
    depth(d)?;
    Ok(match c {
        mt::Curve::Line(c) => b::Curve::Line { start: point(c.0), end: point(c.1) },
        mt::Curve::BSplineCurve(c) => {
            b::Curve::Spline { knots: c.knot_vec().iter().copied().collect(), points: c.control_points().iter().copied().map(point).collect() }
        }
        mt::Curve::NurbsCurve(c) => b::Curve::RationalSpline {
            knots: c.knot_vec().iter().copied().collect(),
            homogeneous: c.control_points().iter().copied().map(vector4).collect(),
        },
        mt::Curve::IntersectionCurve(c) => {
            let leader = match c.leader() {
                mt::Leader::Polyline(p) => b::Curve::Polyline { points: p.0.iter().copied().map(point).collect() },
                mt::Leader::BSpline(c) => b::Curve::Spline {
                    knots: c.knot_vec().iter().copied().collect(),
                    points: c.control_points().iter().copied().map(point).collect(),
                },
            };
            b::Curve::SurfaceIntersection {
                surfaces: [Box::new(save_surface(c.surface0(), d + 1)?), Box::new(save_surface(c.surface1(), d + 1)?)],
                leader: Box::new(leader),
                tolerance: c.tolerance(),
            }
        }
    })
}
/// A plane as truck holds it: three points.
#[derive(serde::Serialize, serde::Deserialize)]
struct PlanePoints {
    o: mt::Point3,
    p: mt::Point3,
    q: mt::Point3,
}

/// A revolution as truck holds it, built from its parts as they are (its constructors normalize
/// the axis and compose the placement with the identity, either of which can round).
#[derive(serde::Serialize)]
struct PlacedRevolution {
    entity: Revolved,
    transform: mt::Matrix4,
    orientation: bool,
}
#[derive(serde::Serialize)]
struct Revolved {
    curve: mt::Curve,
    revolution: Axis,
}
#[derive(serde::Serialize)]
struct Axis {
    origin: mt::Point3,
    axis: mt::Vector3,
}

fn save_surface(s: &mt::Surface, d: usize) -> Result<b::Surface> {
    depth(d)?;
    Ok(match s {
        mt::Surface::Plane(s) => {
            let PlanePoints { o, p, q } = serde_json::from_value(serde_json::to_value(s).map_err(invalid)?).map_err(invalid)?;
            b::Surface::PlaneThrough { origin: point(o), u_point: point(p), v_point: point(q) }
        }
        mt::Surface::BSplineSurface(s) => b::Surface::Spline {
            u_knots: s.uknot_vec().iter().copied().collect(),
            v_knots: s.vknot_vec().iter().copied().collect(),
            points: s.control_points().iter().map(|r| r.iter().copied().map(point).collect()).collect(),
        },
        mt::Surface::NurbsSurface(s) => b::Surface::RationalSpline {
            u_knots: s.uknot_vec().iter().copied().collect(),
            v_knots: s.vknot_vec().iter().copied().collect(),
            homogeneous: s.control_points().iter().map(|r| r.iter().copied().map(vector4).collect()).collect(),
        },
        mt::Surface::RevolutedCurve(s) => {
            let m = *s.transform();
            b::Surface::PlacedRevolution {
                profile: Box::new(save_curve(s.entity().entity_curve(), d + 1)?),
                origin: point(s.entity().origin()),
                axis: vector(s.entity().axis()),
                matrix: [m.x, m.y, m.z, m.w].map(vector4),
                forward: s.orientation(),
            }
        }
    })
}
fn load_curve(c: b::Curve) -> Result<mt::Curve> {
    Ok(match c {
        b::Curve::Line { start, end } => mt::Curve::Line(mt::Line(p(start), p(end))),
        b::Curve::CircleArc { frame, radius, angle } => arc(&frame, [radius, radius], angle)?,
        b::Curve::EllipseArc { frame, radii, angle } => arc(&frame, radii, angle)?,
        b::Curve::Spline { knots, points } => {
            mt::Curve::BSplineCurve(mt::BSplineCurve::try_new(knots.into(), points.into_iter().map(p).collect()).map_err(invalid)?)
        }
        b::Curve::RationalSpline { knots, homogeneous } => mt::Curve::NurbsCurve(mt::NurbsCurve::new(
            mt::BSplineCurve::try_new(knots.into(), homogeneous.into_iter().map(v4).collect()).map_err(invalid)?,
        )),
        b::Curve::Polyline { .. } => return Err(invalid("standalone polyline unsupported by this adapter")),
        b::Curve::Intersection { knots, points, .. } => {
            mt::Curve::BSplineCurve(mt::BSplineCurve::try_new(knots.into(), points.into_iter().map(p).collect()).map_err(invalid)?)
        }
        b::Curve::SurfaceIntersection { surfaces: [s0, s1], leader, tolerance } => {
            let leader = match *leader {
                b::Curve::Polyline { points } => mt::Leader::Polyline(mt::PolylineCurve(points.into_iter().map(p).collect())),
                b::Curve::Spline { knots, points } => {
                    mt::Leader::BSpline(mt::BSplineCurve::try_new(knots.into(), points.into_iter().map(p).collect()).map_err(invalid)?)
                }
                _ => return Err(invalid("intersection leader")),
            };
            let (s0, s1) = (load_surface(*s0)?, load_surface(*s1)?);
            mt::Curve::IntersectionCurve(mt::IntersectionCurve::new_unchecked(Box::new(s0), Box::new(s1), leader, tolerance))
        }
    })
}
fn load_surface(s: b::Surface) -> Result<mt::Surface> {
    Ok(match s {
        b::Surface::Plane { origin, u_axis, v_axis } => mt::Surface::Plane(mt::Plane::new(p(origin), p(origin) + v(u_axis), p(origin) + v(v_axis))),
        b::Surface::Spline { u_knots, v_knots, points } => mt::Surface::BSplineSurface(
            mt::BSplineSurface::try_new((u_knots.into(), v_knots.into()), points.into_iter().map(|r| r.into_iter().map(p).collect()).collect())
                .map_err(invalid)?,
        ),
        b::Surface::RationalSpline { u_knots, v_knots, homogeneous } => mt::Surface::NurbsSurface(mt::NurbsSurface::new(
            mt::BSplineSurface::try_new((u_knots.into(), v_knots.into()), homogeneous.into_iter().map(|r| r.into_iter().map(v4).collect()).collect())
                .map_err(invalid)?,
        )),
        b::Surface::Revolution { profile, origin, axis, angle } => {
            full_turn(&angle)?;
            revolved(load_curve(*profile)?, p(origin), v(axis), false)
        }
        b::Surface::PlaneThrough { origin, u_point, v_point } => mt::Surface::Plane(mt::Plane::new(p(origin), p(u_point), p(v_point))),
        b::Surface::PlacedRevolution { profile, origin, axis, matrix: [x, y, z, w], forward } => {
            let placed = PlacedRevolution {
                entity: Revolved { curve: load_curve(*profile)?, revolution: Axis { origin: p(origin), axis: v(axis) } },
                transform: mt::Matrix4::from_cols(v4(x), v4(y), v4(z), v4(w)),
                orientation: forward,
            };
            mt::Surface::RevolutedCurve(serde_json::from_value(serde_json::to_value(placed).map_err(invalid)?).map_err(invalid)?)
        }
        analytic => load_analytic(analytic)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mt::{BoundedCurve, InnerSpace, ParametricCurve, Transformed};

    #[test]
    fn refuses_domain_or_axis_changes_instead_of_approximating() {
        let curve = b::Curve::Spline { knots: vec![0., 1., 2., 3.], points: vec![[0.; 3], [1.; 3]] };
        assert!(compatible_curve(&curve).is_err());
        let plane = b::Surface::Plane { origin: [1e12; 3], u_axis: [1e-6, 0., 0.], v_axis: [0., 1., 0.] };
        assert!(compatible_surface(&plane).is_err());
        assert!(clamped(&[0., 0., 1., 1.], 2).is_ok());
    }

    #[test]
    fn analytic_records_decode_as_their_mathematical_loci_and_normals() {
        use mt::{MetricSpace, ParametricSurface, ParametricSurface3D};
        use std::f64::consts::{FRAC_PI_2, TAU};
        let frame = b::Frame { origin: [1., 2., 3.], x_axis: [1., 0., 0.], y_axis: [0., 1., 0.] };
        let circle = load_curve(b::Curve::CircleArc { frame: frame.clone(), radius: 2., angle: [0., FRAC_PI_2] }).unwrap();
        assert!(circle.subs(0.).distance(p([3., 2., 3.])) < 1e-12);
        assert!(circle.subs(1.).distance(p([1., 4., 3.])) < 1e-12);
        let ellipse = load_curve(b::Curve::EllipseArc { frame: frame.clone(), radii: [2., 3.], angle: [0., FRAC_PI_2] }).unwrap();
        assert!(ellipse.subs(1.).distance(p([1., 5., 3.])) < 1e-12);
        let cases = [
            (b::Surface::Cylinder { frame: frame.clone(), radius: 2., angle: [0., TAU], height: [-1., 3.] }, 0.5, [3., 2., 4.], [1., 0., 0.]),
            (
                b::Surface::Cone { frame: frame.clone(), radius_at_origin: 2., half_angle: 0.2, angle: [0., TAU], height: [0., 3.] },
                0.5,
                [3. + 1.5 * 0.2f64.tan(), 2., 4.5],
                [1., 0., -0.2f64.tan()],
            ),
            (
                b::Surface::Sphere { frame: frame.clone(), radius: 2., longitude: [0., TAU], latitude: [-FRAC_PI_2, FRAC_PI_2] },
                0.5,
                [3., 2., 3.],
                [1., 0., 0.],
            ),
            (
                b::Surface::Torus { frame, major_radius: 4., minor_radius: 1., longitude: [0., TAU], meridian: [0., TAU] },
                0.,
                [6., 2., 3.],
                [1., 0., 0.],
            ),
        ];
        for (record, parameter, expected, normal) in cases {
            let surface = load_surface(record).unwrap();
            assert!(surface.subs(0., parameter).distance(p(expected)) < 1e-10);
            assert!(surface.normal(0., parameter).normalize().dot(v(normal).normalize()) > 1. - 1e-10);
        }
    }

    #[test]
    fn intersections_and_revolutions_reload_exactly() {
        use solidcraft_geom::Vec3;
        let cyl = crate::cylinder(Vec3::ZERO, Vec3::Z, 5., 40.).unwrap();
        let block = crate::box_solid(Vec3::new(-20., -20., 20.), Vec3::new(20., 20., 60.)).unwrap();
        let block = crate::transform(&block, Vec3::ZERO, Vec3::new(0., 0., 20.), Vec3::X, 0.4).unwrap();
        let cut = crate::boolean(&cyl, &block, crate::BoolOp::Cut).unwrap().unwrap();
        let curve = cut.solid.edge_iter().map(|e| e.curve()).find(|c| matches!(c, mt::Curve::IntersectionCurve(_))).unwrap();
        let record = save_curve(&curve, 0).unwrap();
        assert!(matches!(record, b::Curve::SurfaceIntersection { .. }), "the procedural curve itself, not a fit");
        let restored = load_curve(record.clone()).unwrap();
        let (a, b) = curve.range_tuple();
        for i in 0..=64 {
            let t = a + (b - a) * i as f64 / 64.;
            assert_eq!(curve.subs(t), restored.subs(t), "sample {i}");
        }
        assert_eq!(save_curve(&restored, 0).unwrap(), record);
        // The whole body: the same records again, and the same volume to the last bit.
        let bytes = crate::encode_native_body(&cut).unwrap();
        let decoded = crate::decode_native_body(&bytes).unwrap();
        assert_eq!(crate::encode_native_body(&decoded).unwrap(), bytes);
        assert_eq!(crate::measure(&decoded).unwrap().volume, crate::measure(&cut).unwrap().volume);
        // A revolution keeps its own placement, reversed normal included.
        let line = mt::Curve::Line(mt::Line(mt::Point3::new(2., 0., 0.), mt::Point3::new(2.5, 0., 3.)));
        let mt::Surface::RevolutedCurve(mut placed) = revolved(line, mt::Point3::new(0., 1., 0.), mt::Vector3::new(0., 0., 1.), true) else {
            panic!("a revolution")
        };
        placed.transform_by(mt::Matrix4::from_translation(mt::Vector3::new(1., 2., 3.)) * mt::Matrix4::from_angle_x(mt::Rad(0.3)));
        let placed = mt::Surface::RevolutedCurve(placed);
        let record = save_surface(&placed, 0).unwrap();
        assert!(matches!(record, b::Surface::PlacedRevolution { forward: false, .. }));
        let restored = load_surface(record.clone()).unwrap();
        assert_eq!(save_surface(&restored, 0).unwrap(), record);
        use mt::{ParametricSurface, ParametricSurface3D};
        for (u, v) in [(0., 0.), (1.3, 0.4), (4., 1.)] {
            assert_eq!(placed.subs(u, v), restored.subs(u, v));
            assert_eq!(placed.normal(u, v), restored.normal(u, v));
        }
    }
}
