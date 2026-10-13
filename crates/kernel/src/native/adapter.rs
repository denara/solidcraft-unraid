//! The truck adapter. Persisted data belongs to solidcraft_geom::brep, never truck.
use super::{Result, invalid};
use mt::{BoundedCurve, EuclideanSpace, InnerSpace, Invertible, ParametricCurve, Transform, Transformed};
use solidcraft_geom::brep as b;
use truck_modeling as mt;
use truck_topology::compress as t;

type Packed = t::CompressedSolid<mt::Point3, mt::Curve, mt::Surface>;
#[path = "analytic.rs"]
mod analytic;
#[path = "fit.rs"]
mod fit;
use analytic::{arc, full_turn, load_analytic, revolved};
use fit::fit_intersection;
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
    let mut fitting_budget = 4096usize;
    for shell in solid.compress().boundaries {
        let vertices = shell.vertices.into_iter().map(point).collect();
        let edges = shell
            .edges
            .into_iter()
            .map(|e| Ok(b::Edge { vertices: [index(e.vertices.0)?, index(e.vertices.1)?], curve: save_curve(&e.curve, 0, &mut fitting_budget)? }))
            .collect::<Result<_>>()?;
        let faces = shell
            .faces
            .into_iter()
            .map(|f| {
                let (surface, normal_flip) = save_surface(&f.surface, 0, &mut fitting_budget)?;
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
                    forward: f.orientation != normal_flip,
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
    if body.shells.iter().flat_map(|s| &s.edges).any(|e| e.vertices[0] == e.vertices[1]) {
        return Err(invalid("closed single-vertex edges are unsupported by this adapter"));
    }
    let packed = Packed {
        boundaries: body
            .shells
            .into_iter()
            .map(|s| {
                Ok(t::CompressedShell {
                    vertices: s.vertices.into_iter().map(p).collect(),
                    edges: s
                        .edges
                        .into_iter()
                        .map(|e| Ok(t::CompressedEdge { vertices: (e.vertices[0] as usize, e.vertices[1] as usize), curve: load_curve(e.curve)? }))
                        .collect::<Result<_>>()?,
                    faces: s
                        .faces
                        .into_iter()
                        .map(|f| {
                            let mut loops = f.loops;
                            if !f.forward {
                                for wire in &mut loops {
                                    reverse_wire(wire);
                                }
                            }
                            Ok(t::CompressedFace {
                                boundaries: loops
                                    .into_iter()
                                    .map(|w| {
                                        w.into_iter().map(|e| t::CompressedEdgeIndex { index: e.edge as usize, orientation: e.forward }).collect()
                                    })
                                    .collect(),
                                orientation: f.forward,
                                surface: load_surface(f.surface)?,
                            })
                        })
                        .collect::<Result<_>>()?,
                })
            })
            .collect::<Result<_>>()?,
    };
    let shells = packed.boundaries.into_iter().map(mt::Shell::extract).collect::<std::result::Result<Vec<_>, _>>().map_err(invalid)?;
    if shells.iter().any(|s| s.shell_condition() == mt::ShellCondition::Irregular) {
        return Err(invalid("non-manifold shell"));
    }
    Ok(mt::Solid::new_unchecked(shells))
}
fn reverse_wire(wire: &mut [b::EdgeUse]) {
    wire.reverse();
    for edge in wire {
        edge.forward = !edge.forward;
    }
}
fn save_curve(c: &mt::Curve, d: usize, fitting_budget: &mut usize) -> Result<b::Curve> {
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
        mt::Curve::IntersectionCurve(_) => fit_intersection(c, fitting_budget)?,
    })
}
fn save_surface(s: &mt::Surface, d: usize, fitting_budget: &mut usize) -> Result<(b::Surface, bool)> {
    depth(d)?;
    let mut normal_flip = false;
    let saved = match s {
        mt::Surface::Plane(s) => b::Surface::Plane { origin: point(s.origin()), u_axis: vector(s.u_axis()), v_axis: vector(s.v_axis()) },
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
            let m = s.transform();
            let (x, y, z) = (m.x.truncate(), m.y.truncate(), m.z.truncate());
            let scale = x.magnitude();
            let tolerance = scale * 1e-10;
            if !scale.is_finite()
                || scale <= 1e-12
                || (y.magnitude() - scale).abs() > tolerance
                || (z.magnitude() - scale).abs() > tolerance
                || x.dot(y).abs() > scale * tolerance
                || x.dot(z).abs() > scale * tolerance
                || y.dot(z).abs() > scale * tolerance
                || m.x.w != 0.
                || m.y.w != 0.
                || m.z.w != 0.
                || m.w.w != 1.
            {
                return Err(invalid("non-similarity revolution transform needs an exact spline conversion"));
            }
            // Bake a similarity into the geometry, never into persisted processor data.
            // An improper rotation reverses the oriented axis in the rotation identity.
            let sign = if x.dot(y.cross(z)) < 0. { -1. } else { 1. };
            let axis = m.transform_vector(s.entity().axis()).normalize() * sign;
            let profile = s.entity().entity_curve().transformed(*m);
            normal_flip = !s.orientation();
            b::Surface::Revolution {
                profile: Box::new(save_curve(&profile, d + 1, fitting_budget)?),
                origin: point(m.transform_point(s.entity().origin())),
                axis: vector(axis),
                angle: [0., std::f64::consts::TAU],
            }
        }
    };
    Ok((saved, normal_flip))
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
        analytic => load_analytic(analytic)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn intersection_fit_is_spatial_bounded_and_independent_of_surface_solvers() {
        use mt::MetricSpace;
        use solidcraft_geom::Vec3;
        let cyl = crate::cylinder(Vec3::ZERO, Vec3::Z, 5., 40.).unwrap();
        let block = crate::box_solid(Vec3::new(-20., -20., 20.), Vec3::new(20., 20., 60.)).unwrap();
        let block = crate::transform(&block, Vec3::ZERO, Vec3::new(0., 0., 20.), Vec3::X, 0.4).unwrap();
        let cut = crate::boolean(&cyl, &block, crate::BoolOp::Cut).unwrap().unwrap();
        let curve = cut.solid.edge_iter().map(|e| e.curve()).find(|c| matches!(c, mt::Curve::IntersectionCurve(_))).unwrap();
        assert!(save_curve(&curve, 0, &mut 0).is_err());
        let record = save_curve(&curve, 0, &mut 4096).unwrap();
        let encoded = serde_json::to_string(&record).unwrap();
        assert!(!encoded.contains("leader") && !encoded.contains("surfaces"));
        let b::Curve::Intersection { fitting_tolerance, .. } = &record else { panic!("explicit fitted record required") };
        let tolerance = *fitting_tolerance;
        let restored = load_curve(record).unwrap();
        let (a, b) = curve.range_tuple();
        for i in 0..=512 {
            let t = a + (b - a) * i as f64 / 512.;
            assert!(curve.subs(t).distance(restored.subs(t)) <= tolerance * 1.1, "sample {i}: error {}", curve.subs(t).distance(restored.subs(t)));
        }
        let decoded = crate::decode_native_body(&crate::encode_native_body(&cut).unwrap()).unwrap();
        let original = crate::measure(&cut).unwrap().volume;
        assert!((crate::measure(&decoded).unwrap().volume - original).abs() < original * 1e-5);
    }
}
