//! Independent analytic reference: the centre of a ball tangent to two perpendicular
//! cylinders lies on the intersection of their radius-offset cylinders.

use mt::{BoundedCurve, ParametricCurve, ParametricSurface, ParametricSurface3D, SearchNearestParameter};

use super::*;
use crate::{Body, BoolOp, boolean, cylinder, fillet};

fn tee(scale: f64, origin: Vec3) -> Body {
    let main = cylinder(origin + Vec3::new(-40.0, 0.0, 0.0) * scale, Vec3::X, 15.0 * scale, 80.0 * scale).unwrap();
    let branch = cylinder(origin, Vec3::Z, 10.0 * scale, 35.0 * scale).unwrap();
    boolean(&main, &branch, BoolOp::Union).unwrap().unwrap()
}

fn analytic_section(theta: f64, scale: f64, origin: Vec3) -> (Vec3, Vec3, Vec3) {
    let centre = Vec3::new(13.0 * theta.cos(), 13.0 * theta.sin(), (324.0 - 169.0 * theta.sin().powi(2)).sqrt());
    let main = Vec3::new(centre.x, centre.y * 15.0 / 18.0, centre.z * 15.0 / 18.0);
    let branch = Vec3::new(centre.x * 10.0 / 13.0, centre.y * 10.0 / 13.0, centre.z);
    (origin + centre * scale, origin + main * scale, origin + branch * scale)
}

fn nearest_blend(surfaces: &[mt::Surface], point: Vec3) -> (f64, Vec3) {
    surfaces
        .iter()
        .filter_map(|s| {
            let (u, v) = s.search_nearest_parameter(p3(point), None, 100)?;
            let actual = from_p3(s.subs(u, v));
            let n = s.normal(u, v);
            Some((actual.dist(point), Vec3::new(n.x, n.y, n.z).normalized()?))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap()
}

fn distance_to_analytic_arc(theta: f64, point: Vec3) -> f64 {
    let (centre, main, branch) = analytic_section(theta, 1.0, Vec3::ZERO);
    let a = (main - centre).normalized().unwrap();
    let b = (branch - centre).normalized().unwrap();
    let plane = a.cross(b).normalized().unwrap();
    let radial = point - centre;
    let projected = radial - plane * radial.dot(plane);
    let direction = projected.normalized().unwrap();
    let angle = a.dot(b).clamp(-1.0, 1.0).acos();
    let from_main = a.dot(direction).clamp(-1.0, 1.0).acos();
    let from_branch = b.dot(direction).clamp(-1.0, 1.0).acos();
    if (from_main + from_branch - angle).abs() < 1e-7 { point.dist(centre + direction * 3.0) } else { point.dist(main).min(point.dist(branch)) }
}

fn distance_to_analytic_band(point: Vec3) -> f64 {
    // The arc's XY azimuth stays between its two contacts' azimuths. Their difference
    // for this tee is less than 0.1 rad; bracket generously around the query azimuth.
    let azimuth = point.y.atan2(point.x);
    let (mut lo, mut hi) = (azimuth - 0.3, azimuth + 0.3);
    for _ in 0..55 {
        let a = lo + (hi - lo) * 0.38196601125;
        let b = lo + (hi - lo) * 0.61803398875;
        if distance_to_analytic_arc(a, point) < distance_to_analytic_arc(b, point) {
            hi = b;
        } else {
            lo = a;
        }
    }
    distance_to_analytic_arc((lo + hi) * 0.5, point)
}

#[test]
fn fitted_contact_edges_coincide_with_their_surface_boundaries() {
    let sections: Vec<Vec<Section>> = (0..4)
        .map(|k| {
            (0..SAMPLES)
                .map(|i| {
                    let theta = std::f64::consts::FRAC_PI_2 * (k as f64 + i as f64 / (SAMPLES - 1) as f64);
                    let (c, q1, q2) = analytic_section(theta, 1.0, Vec3::ZERO);
                    Section { c, q1, q2 }
                })
                .collect()
        })
        .collect();
    for k in 0..sections.len() {
        let fit = crate::guard("contact consistency", || fit_sections(&sections, k, true, true)).unwrap();
        for (side, curve) in fit.contacts.iter().enumerate() {
            let (lo, hi) = curve.range_tuple();
            for i in 0..=200 {
                let parameter = lo + (hi - lo) * i as f64 / 200.0;
                let edge = from_p3(curve.subs(parameter));
                let surface = from_p3(fit.surface.subs(side as f64, parameter));
                assert!(edge.dist(surface) < 1e-9, "contact-to-surface gap {}", edge.dist(surface));
            }
        }
    }
}

#[test]
fn rolling_ball_matches_analytic_tee_and_contacts() {
    for (scale, origin) in [(1.0, Vec3::ZERO), (0.25, Vec3::new(23.0, -17.0, 9.0))] {
        let body = tee(scale, origin);
        let point = origin + Vec3::new(0.0, 10.0, 125f64.sqrt()) * scale;
        let blend = fillet(&body, &[point], 3.0 * scale).unwrap();
        assert!(blend.validity().is_empty(), "{:?}", blend.validity());
        let surfaces: Vec<_> = blend.solid.face_iter().map(|f| f.oriented_surface()).filter(|s| matches!(s, mt::Surface::NurbsSurface(_))).collect();
        assert!(!surfaces.is_empty());
        let mut distance: f64 = 0.0;
        let mut tangent_angle: f64 = 0.0;
        for i in 0..96 {
            // Offset the angular samples from patch seams; contact endpoints are tested below.
            let theta = std::f64::consts::TAU * (i as f64 + 0.37) / 96.0;
            let (centre, main, branch) = analytic_section(theta, scale, origin);
            let a = (main - centre).normalized().unwrap();
            let b = (branch - centre).normalized().unwrap();
            let angle = a.dot(b).clamp(-1.0, 1.0).acos();
            for fraction in [0.0, 0.25, 0.5, 0.75, 1.0] {
                // Spherical linear interpolation of the radial unit vectors, independent of
                // the production rational control-point construction and fitting parameters.
                let radial = (a * ((1.0 - fraction) * angle).sin() + b * (fraction * angle).sin()) / angle.sin();
                let exact = centre + radial * (3.0 * scale);
                let (error, normal) = nearest_blend(&surfaces, exact);
                distance = distance.max(error);
                if fraction == 0.0 || fraction == 1.0 {
                    let want = (centre - exact).normalized().unwrap();
                    tangent_angle = tangent_angle.max(normal.dot(want).abs().min(1.0).acos().to_degrees());
                }
            }
        }
        assert!(distance <= 0.005, "scale {scale}: analytic surface deviation {distance} mm");
        assert!(tangent_angle <= 0.5, "scale {scale}: contact normals differ by {tangent_angle} degrees");

        // The reverse direction uses points on the actual trimmed face mesh, so an
        // untrimmed neighbouring surface cannot conceal an overshooting blend patch.
        let mesh = blend.tessellate_with(0.002, true).unwrap();
        let mut reverse_distance: f64 = 0.0;
        let mut probes = 0;
        for point in mesh.positions.iter().step_by(19) {
            let p = (*point - origin) / scale;
            if (p.x * p.x + p.y * p.y).sqrt() <= 10.000001
                || (p.y * p.y + p.z * p.z).sqrt() <= 15.000001
                || !(10.0..20.0).contains(&p.z)
                || p.x.abs() > 16.0
            {
                continue;
            }
            probes += 1;
            reverse_distance = reverse_distance.max(distance_to_analytic_band(p) * scale);
        }
        assert!(probes > 300, "only {probes} trimmed surface points checked");
        assert!(reverse_distance <= 0.005, "scale {scale}: reverse analytic deviation {reverse_distance} mm");
        eprintln!("scale {scale}: surface {distance:.9} mm, reverse {reverse_distance:.9} mm, contact normals {tangent_angle:.6} degrees");

        let mut contact_count = 0;
        for edge in Body::unique_edges(&blend.solid) {
            let curve = edge.oriented_curve();
            let (lo, hi) = curve.range_tuple();
            let endpoints = [from_p3(curve.subs(lo)), from_p3(curve.subs(hi))].map(|p| (p - origin) / scale);
            if endpoints.iter().any(|p| !(9.0..20.0).contains(&p.z) || p.x.abs() > 16.0) {
                continue;
            }
            let branch = endpoints.iter().all(|p| ((p.x * p.x + p.y * p.y).sqrt() - 10.0).abs() < 1e-5);
            let main = endpoints.iter().all(|p| ((p.y * p.y + p.z * p.z).sqrt() - 15.0).abs() < 1e-5);
            if !branch && !main {
                continue;
            }
            contact_count += 1;
            for i in 0..=200 {
                let p = (from_p3(curve.subs(lo + (hi - lo) * i as f64 / 200.0)) - origin) / scale;
                let radius_error = if branch { ((p.x * p.x + p.y * p.y).sqrt() - 10.0).abs() } else { ((p.y * p.y + p.z * p.z).sqrt() - 15.0).abs() };
                assert!(radius_error * scale <= 0.002, "supporting cylinder deviation {} mm", radius_error * scale);
            }
        }
        assert!(contact_count >= 4, "only {contact_count} contacts checked");
    }
}

#[test]
fn rolling_ball_refinement_cap_returns_an_error_and_preserves_input() {
    let body = tee(1.0, Vec3::ZERO);
    let solid = body.deep_copy();
    let ids: Vec<_> = Body::unique_edges(&solid)
        .iter()
        .filter(|edge| {
            let curve = edge.oriented_curve();
            let (lo, hi) = curve.range_tuple();
            let p = from_p3(curve.subs((lo + hi) * 0.5));
            p.z > 10.0 && ((p.x * p.x + p.y * p.y).sqrt() - 10.0).abs() < 0.01 && ((p.y * p.y + p.z * p.z).sqrt() - 15.0).abs() < 0.01
        })
        .map(mt::Edge::id)
        .collect();
    assert!(!ids.is_empty());
    let before = crate::step_export(&[&body], "unchanged").unwrap();
    let input_before = crate::step_export(&[&Body::new(solid.clone()).unwrap()], "unchanged").unwrap();
    // Four sections deliberately undersample the saddle; production starts at 24.
    let error = crate::guard("limited rolling ball", || curve_blend_with_limit(&solid, &ids, body.size(), 3.0, true, 4)).unwrap_err();
    assert!(error.to_string().contains("accuracy and section limits"), "{error}");
    assert_eq!(crate::step_export(&[&body], "unchanged").unwrap(), before);
    assert_eq!(crate::step_export(&[&Body::new(solid.clone()).unwrap()], "unchanged").unwrap(), input_before);
}

/// Plan-view construction of a round between two overlapping circles (a concave vertical edge
/// where two Z cylinders meet): the ball centre `f`, the circle and fillet arc angles, and the
/// area the round fills in, all independent of the kernel.
struct Gap {
    /// Points along the round's arc in plan, contacts included.
    arc: Vec<Vec3>,
    /// Arc angles on circle 1, circle 2 and the fillet between the corner and the contacts.
    angles: [f64; 3],
    area: f64,
}

fn gap(c1: Vec3, r1: f64, c2: Vec3, r2: f64, corner: Vec3, r: f64) -> Gap {
    // The ball centre is r from both circles, on the corner's side of the centre line.
    let (a, b) = (r1 + r, r2 + r);
    let d = c1.dist(c2);
    let u = (c2 - c1) * (1.0 / d);
    let along = (a * a - b * b + d * d) / (2.0 * d);
    let mut side = Vec3::new(-u.y, u.x, 0.0);
    if (corner - c1).dot(side) < 0.0 {
        side = side * -1.0;
    }
    let f = c1 + u * along + side * (a * a - along * along).sqrt();
    let (t1, t2) = (c1 + (f - c1) * (r1 / a), c2 + (f - c2) * (r2 / b));
    let angle = |c: Vec3, p: Vec3, q: Vec3| (p - c).normalized().unwrap().dot((q - c).normalized().unwrap()).clamp(-1.0, 1.0).acos();
    let angles = [angle(c1, t1, corner), angle(c2, corner, t2), angle(f, t2, t1)];
    // Shoelace over the three short arcs: circle 1 to the corner, circle 2 on, fillet back.
    let mut ring = Vec::new();
    for (c, p, q) in [(c1, t1, corner), (c2, corner, t2), (f, t2, t1)] {
        let (p0, q0) = (p - c, q - c);
        let (a0, a1) = (p0.y.atan2(p0.x), q0.y.atan2(q0.x));
        let mut sweep = a1 - a0;
        while sweep > std::f64::consts::PI {
            sweep -= std::f64::consts::TAU;
        }
        while sweep < -std::f64::consts::PI {
            sweep += std::f64::consts::TAU;
        }
        let rr = p0.len();
        for i in 0..20_000 {
            let t = a0 + sweep * i as f64 / 20_000.0;
            ring.push(c + Vec3::new(t.cos(), t.sin(), 0.0) * rr);
        }
    }
    let area = ring.iter().zip(ring.iter().cycle().skip(1)).map(|(p, q)| p.x * q.y - q.x * p.y).sum::<f64>().abs() * 0.5;
    let (p, q) = ((t1 - f).normalized().unwrap(), (t2 - f).normalized().unwrap());
    let arc = (0..=8).map(|i| angles[2] * i as f64 / 8.0).map(|t| f + (p * ((angles[2] - t).sin()) + q * t.sin()) * (r / angles[2].sin())).collect();
    Gap { arc, angles, area }
}

#[test]
fn rolling_ball_along_a_straight_edge_between_cylinders_trims_curved_end_seams() {
    // Two Z cylinders crossing square to each other: the vertical lines where they meet are
    // concave edges between two curved faces, and the blend ends on the planar caps, where the
    // seams are arcs.
    let (r1, r2, h, r) = (17.0, 9.5, 10.0, 6.0);
    let (c1, c2) = (Vec3::ZERO, Vec3::new(r1, r2, 0.0));
    let a = cylinder(c1, Vec3::Z, r1, h).unwrap();
    let b = cylinder(c2, Vec3::Z, r2, h).unwrap();
    let body = boolean(&a, &b, BoolOp::Union).unwrap().unwrap();
    // The corner: one of the two points where the circles cross.
    let d = c1.dist(c2);
    let u = (c2 - c1) * (1.0 / d);
    let along = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let corner = c1 + u * along + Vec3::new(-u.y, u.x, 0.0) * (r1 * r1 - along * along).sqrt();
    let g = gap(c1, r1, c2, r2, corner, r);
    let blend = fillet(&body, &[corner + Vec3::Z * (h / 2.0)], r).unwrap();
    assert!(blend.validity().is_empty(), "{:?}", blend.validity());
    // One blend face; the edge is replaced by two contact lines and two end arcs.
    let (before, after) = (crate::measure(&body).unwrap(), crate::measure(&blend).unwrap());
    assert_eq!((after.faces, after.edges, after.vertices), (before.faces + 1, before.edges + 3, before.vertices + 2), "{before:?} -> {after:?}");
    // The round fills the gap over the full height. The caps gain it twice; the walls lose
    // their arcs to the contacts and gain the round.
    let (m0, m1) = (body.tessellate_with(0.0005, true).unwrap().measure(), blend.tessellate_with(0.0005, true).unwrap().measure());
    let volume = h * g.area;
    assert!((m1.volume - m0.volume - volume).abs() < volume * 3e-3, "added volume {} vs {volume}", m1.volume - m0.volume);
    let [a1, a2, af] = g.angles;
    let area = 2.0 * g.area + h * (r * af - r1 * a1 - r2 * a2);
    assert!((m1.area - m0.area - area).abs() < area.abs() * 3e-3, "added area {} vs {area}", m1.area - m0.area);
    // The round is the analytic arc at every height, from contact to contact.
    let surfaces: Vec<_> = blend.solid.face_iter().map(|f| f.oriented_surface()).filter(|s| matches!(s, mt::Surface::NurbsSurface(_))).collect();
    let mut worst: f64 = 0.0;
    for z in [0.0, 0.25, 0.5, 0.75, 1.0] {
        for p in &g.arc {
            worst = worst.max(nearest_blend(&surfaces, *p + Vec3::Z * (h * z)).0);
        }
    }
    assert!(worst < 1e-3, "blend surface is {worst} mm off the analytic round");
}

#[test]
fn closed_curved_seam_is_not_supported() {
    // A closed seam (one edge starting and ending at the same vertex, as a full cap circle can
    // be in imported geometry) has no "between" two contacts, so trimming it is refused rather
    // than guessed.
    let body = cylinder(Vec3::ZERO, Vec3::Z, 10.0, 5.0).unwrap();
    let arc = body.solid.edge_iter().find(|e| !matches!(e.curve(), mt::Curve::Line(_))).expect("a cap arc");
    let circle = mt::Edge::new_unchecked(arc.front(), arc.front(), arc.curve());
    assert_eq!(circle.front(), circle.back());
    // Contacts on the circle itself: a third and two thirds of the way round.
    let (lo, hi) = circle.curve().range_tuple();
    let on = |f: f64| mt::Vertex::new(circle.curve().subs(lo + (hi - lo) * f));
    for (a, b) in [(Some(on(1.0 / 3.0)), None), (None, Some(on(2.0 / 3.0))), (Some(on(1.0 / 3.0)), Some(on(2.0 / 3.0))), (None, None)] {
        let err = trim_curved_seam(&circle, a, b, 20.0).unwrap_err();
        assert!(err.to_string().contains("not supported yet: a closed curved edge"), "{err}");
    }
}

#[test]
fn rolling_ball_past_the_end_seam_is_too_large_and_reduces() {
    // The crossing cylinders of the test above with the small one's centre inside the large
    // one: a round of 3 would reach past the end of the cap arc it trims, so it is too large
    // (a smaller one fits), and the largest that fits is the analytic round over the full height
    // (checked on its surface: its added volume is too small beside the body's to measure).
    let (r1, r2, d, h) = (17.0, 9.5, 12.0, 10.0);
    let (c1, c2) = (Vec3::ZERO, Vec3::new(d, 0.0, 0.0));
    let body = boolean(&cylinder(c1, Vec3::Z, r1, h).unwrap(), &cylinder(c2, Vec3::Z, r2, h).unwrap(), BoolOp::Union).unwrap().unwrap();
    let along = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let corner = Vec3::new(along, (r1 * r1 - along * along).sqrt(), 0.0);
    let at = [corner + Vec3::Z * (h / 2.0)];
    let e = fillet(&body, &at, 3.0).unwrap_err();
    assert!(matches!(e, KernelError::TooLarge(_)), "{e}");
    let (r, blend) = crate::fillet_max(&body, &at, 3.0).unwrap();
    assert!(r > 1.5 && r < 3.0, "{r}");
    assert!(blend.validity().is_empty(), "{:?}", blend.validity());
    let g = gap(c1, r1, c2, r2, corner, r);
    let surfaces: Vec<_> = blend.solid.face_iter().map(|f| f.oriented_surface()).filter(|s| matches!(s, mt::Surface::NurbsSurface(_))).collect();
    let mut worst: f64 = 0.0;
    for z in [0.0, 0.25, 0.5, 0.75, 1.0] {
        for p in &g.arc {
            worst = worst.max(nearest_blend(&surfaces, *p + Vec3::Z * (h * z)).0);
        }
    }
    assert!(worst < 1e-3, "blend surface is {worst} mm off the analytic round at r {r}");
}

#[test]
fn a_refused_edge_is_not_rolled_again_after_blends_far_from_it() {
    // The crossing cylinders' seam refuses a round of 3 (too large). A bar joined to them has
    // two straight edges at its far end that round on their own. Each blend there leaves the
    // seam's surroundings as they were, so the seam is tried once, not once per blend, and the
    // refusal is the same.
    let (r1, r2, d, h) = (17.0, 9.5, 12.0, 10.0);
    let body = boolean(&cylinder(Vec3::ZERO, Vec3::Z, r1, h).unwrap(), &cylinder(Vec3::new(d, 0.0, 0.0), Vec3::Z, r2, h).unwrap(), BoolOp::Union)
        .unwrap()
        .unwrap();
    let bar = crate::box_solid(Vec3::new(-80.0, -5.0, 0.0), Vec3::new(-10.0, 5.0, h)).unwrap();
    let body = boolean(&body, &bar, BoolOp::Union).unwrap().unwrap();
    let along = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let seam = Vec3::new(along, (r1 * r1 - along * along).sqrt(), h / 2.0);
    let ends = [Vec3::new(-80.0, -5.0, h / 2.0), Vec3::new(-80.0, 5.0, h / 2.0)];
    crate::blend::ROLLS.with(|n| n.set(0));
    let e = fillet(&body, &[seam, ends[0], ends[1]], 3.0).unwrap_err();
    assert!(matches!(e, KernelError::TooLarge(_)), "{e}");
    let rolls = crate::blend::ROLLS.with(|n| n.get());
    assert_eq!(rolls, 1, "the seam was rolled {rolls} times");
    // The far edges alone still round.
    assert!(fillet(&body, &ends, 3.0).is_ok());
}
