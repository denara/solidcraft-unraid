//! Analytic wire records are mathematical loci, independent of backend parameterizations.
use super::*;

pub(super) fn full_turn(angle: &[f64; 2]) -> Result<()> {
    if (angle[1] - angle[0] - std::f64::consts::TAU).abs() > 1e-12 {
        Err(invalid("partial angular surface domains need another adapter representation"))
    } else {
        Ok(())
    }
}

/// Exact rational conic locus. The adapter reparameterizes angle by rational spans.
pub(super) fn arc(frame: &b::Frame, radii: [f64; 2], angle: [f64; 2]) -> Result<mt::Curve> {
    let span = angle[1] - angle[0];
    let count = (span / std::f64::consts::FRAC_PI_2).ceil().clamp(1., 4.) as usize;
    let step = span / count as f64;
    let weight = (step / 2.).cos();
    let at = |t: f64, scale: f64| p(frame.origin) + v(frame.x_axis) * (radii[0] * t.cos() / scale) + v(frame.y_axis) * (radii[1] * t.sin() / scale);
    let mut controls = vec![at(angle[0], 1.).to_homogeneous()];
    let mut knots = vec![0.; 3];
    for i in 0..count {
        let start = angle[0] + i as f64 * step;
        controls.push(at(start + step / 2., weight).to_homogeneous() * weight);
        controls.push(at(start + step, 1.).to_homogeneous());
        knots.extend(std::iter::repeat_n((i + 1) as f64 / count as f64, if i + 1 == count { 3 } else { 2 }));
    }
    Ok(mt::Curve::NurbsCurve(mt::NurbsCurve::new(mt::BSplineCurve::try_new(knots.into(), controls).map_err(invalid)?)))
}

pub(super) fn revolved(profile: mt::Curve, origin: mt::Point3, axis: mt::Vector3, normal_flip: bool) -> mt::Surface {
    let mut s = mt::Processor::new(mt::RevolutedCurve::by_revolution(profile, origin, axis));
    // Internal parameter swapping is allowed; no such flag is persisted in a surface record.
    if normal_flip {
        s.invert();
    }
    mt::Surface::RevolutedCurve(s)
}

pub(super) fn load_analytic(surface: b::Surface) -> Result<mt::Surface> {
    let axis = |f: &b::Frame| v(f.x_axis).cross(v(f.y_axis));
    Ok(match surface {
        b::Surface::Cylinder { frame, radius, angle, height } => {
            full_turn(&angle)?;
            let z = axis(&frame);
            let at = |h| p(frame.origin) + v(frame.x_axis) * radius + z * h;
            revolved(mt::Curve::Line(mt::Line(at(height[0]), at(height[1]))), p(frame.origin), z, true)
        }
        b::Surface::Cone { frame, radius_at_origin, half_angle, angle, height } => {
            full_turn(&angle)?;
            let z = axis(&frame);
            let at = |h| p(frame.origin) + v(frame.x_axis) * (radius_at_origin + h * half_angle.tan()) + z * h;
            revolved(mt::Curve::Line(mt::Line(at(height[0]), at(height[1]))), p(frame.origin), z, true)
        }
        b::Surface::Sphere { frame, radius, longitude, latitude } => {
            full_turn(&longitude)?;
            let z = axis(&frame);
            let meridian = b::Frame { origin: frame.origin, x_axis: frame.x_axis, y_axis: vector(z) };
            revolved(arc(&meridian, [radius; 2], latitude)?, p(frame.origin), z, true)
        }
        b::Surface::Torus { frame, major_radius, minor_radius, longitude, meridian } => {
            full_turn(&longitude)?;
            let z = axis(&frame);
            let profile = b::Frame { origin: point(p(frame.origin) + v(frame.x_axis) * major_radius), x_axis: frame.x_axis, y_axis: vector(z) };
            revolved(arc(&profile, [minor_radius; 2], meridian)?, p(frame.origin), z, true)
        }
        _ => return Err(invalid("expected analytic surface")),
    })
}
