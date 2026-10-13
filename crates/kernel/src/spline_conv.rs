//! geom's B-spline curves and surfaces as truck's NURBS geometry, weights kept (homogeneous
//! control points carried over as they are).

use solidcraft_geom::bspline::{HPoint, NurbsSurface};
use truck_modeling as mt;

use crate::{KernelError, Result};

fn fail(m: impl std::fmt::Display) -> KernelError {
    KernelError::Failed(format!("spline geometry: {m}"))
}

/// A homogeneous control point as truck's.
pub(crate) fn h4(h: HPoint) -> mt::Vector4 {
    mt::Vector4::new(h.x, h.y, h.z, h.w)
}

/// A NURBS curve of knots and homogeneous control points.
pub(crate) fn nurbs_curve(knots: mt::KnotVec, ctrl: &[HPoint]) -> Result<mt::Curve> {
    let c = mt::BSplineCurve::try_new(knots, ctrl.iter().copied().map(h4).collect()).map_err(fail)?;
    Ok(mt::Curve::NurbsCurve(mt::NurbsCurve::new(c)))
}

/// A NURBS surface of knots and homogeneous control points (`rows[i][j]`: i along u).
pub(crate) fn nurbs_surface(knots: (mt::KnotVec, mt::KnotVec), rows: Vec<Vec<mt::Vector4>>) -> Result<mt::Surface> {
    let s = mt::BSplineSurface::try_new(knots, rows).map_err(fail)?;
    Ok(mt::Surface::NurbsSurface(mt::NurbsSurface::new(s)))
}

/// geom's tensor surface as truck's.
pub(crate) fn surface(s: &NurbsSurface) -> Result<mt::Surface> {
    let [nu, nv] = s.count();
    let [ku, kv] = s.knots();
    let rows: Vec<Vec<mt::Vector4>> = (0..nu).map(|i| (0..nv).map(|j| h4(s.point(i, j))).collect()).collect();
    nurbs_surface((mt::KnotVec::from(ku.clone()), mt::KnotVec::from(kv.clone())), rows)
}
