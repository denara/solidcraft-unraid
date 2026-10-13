//! Bounded export of procedural intersections as ordinary spatial cubic splines.
//! The stored tolerance is the fitting target; the result is always provisional.
use super::*;

fn point_at(c: &mt::Curve, t: f64, budget: &mut usize) -> Result<mt::Point3> {
    *budget = budget.checked_sub(1).ok_or_else(|| invalid("intersection fitting budget"))?;
    let p = c.subs(t);
    if !p.x.is_finite() || !p.y.is_finite() || !p.z.is_finite() {
        return Err(invalid("intersection sample"));
    }
    Ok(p)
}
pub(super) fn fit_intersection(c: &mt::Curve, budget: &mut usize) -> Result<b::Curve> {
    let tolerance = 1e-4;
    let (start, end) = c.range_tuple();
    if !start.is_finite() || !end.is_finite() || start >= end {
        return Err(invalid("intersection range"));
    }
    // Fit each smooth parameter interval using spatial samples, without backend derivative conventions.
    // Only the resulting spatial spline crosses the persisted format boundary.
    let mt::Curve::IntersectionCurve(intersection) = c else {
        return Err(invalid("expected intersection"));
    };
    let mut breaks = vec![start, end];
    match intersection.leader() {
        mt::Leader::Polyline(points) => {
            if points.len() > 4096 {
                return Err(invalid("intersection parameter budget"));
            }
            breaks.extend((0..points.len()).map(|i| i as f64).filter(|t| *t > start && *t < end));
        }
        mt::Leader::BSpline(curve) => {
            if curve.knot_vec().len() > 4096 {
                return Err(invalid("intersection parameter budget"));
            }
            breaks.extend(curve.knot_vec().iter().copied().filter(|t| *t > start && *t < end));
        }
    }
    breaks.sort_by(f64::total_cmp);
    breaks.dedup();
    let mut stack = Vec::new();
    for pair in breaks.windows(2).rev() {
        let (a, b) = (pair[0], pair[1]);
        stack.push((a, b, point_at(c, a, budget)?, point_at(c, b, budget)?, 0u8));
    }
    let mut pieces = Vec::new();
    while let Some((a, b, pa, pb, depth)) = stack.pop() {
        let p1 = point_at(c, a + (b - a) / 3., budget)?.to_vec();
        let p2 = point_at(c, a + (b - a) * 2. / 3., budget)?.to_vec();
        let x = p1 * 27. - pa.to_vec() * 8. - pb.to_vec();
        let y = p2 * 27. - pa.to_vec() - pb.to_vec() * 8.;
        let controls = [pa, mt::Point3::from_vec((x * 2. - y) / 18.), mt::Point3::from_vec((y * 2. - x) / 18.), pb];
        let mut fits = true;
        for fraction in [0.125, 0.25, 0.5, 0.75, 0.875] {
            let actual = point_at(c, a + (b - a) * fraction, budget)?;
            let u = 1. - fraction;
            let fitted = controls[0].to_vec() * u * u * u
                + controls[1].to_vec() * 3. * u * u * fraction
                + controls[2].to_vec() * 3. * u * fraction * fraction
                + controls[3].to_vec() * fraction * fraction * fraction;
            if (actual.to_vec() - fitted).magnitude() > tolerance * 0.1 {
                fits = false;
            }
        }
        if fits {
            pieces.push((a, b, controls));
        } else {
            if depth >= 12 || pieces.len() + stack.len() >= 1023 {
                return Err(invalid("intersection fit did not converge"));
            }
            let middle = a + (b - a) / 2.;
            let pm = point_at(c, middle, budget)?;
            stack.push((middle, b, pm, pb, depth + 1));
            stack.push((a, middle, pa, pm, depth + 1));
        }
    }
    let first = pieces.first().ok_or_else(|| invalid("empty intersection fit"))?;
    let mut points = vec![point(first.2[0])];
    let mut knots = vec![start; 4];
    for (i, (_, b, controls)) in pieces.iter().enumerate() {
        points.extend(controls.iter().skip(1).copied().map(point));
        knots.extend(std::iter::repeat_n(*b, if i + 1 == pieces.len() { 4 } else { 3 }));
    }
    Ok(b::Curve::Intersection { knots, points, fitting_tolerance: tolerance, faces: None })
}
