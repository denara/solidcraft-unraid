//! Conversions: a tensor NURBS surface as a T-spline and back (a T-mesh without T-junctions is
//! a tensor B-spline; 2003 §3 and §5), and Bézier extraction — the surface as rational Bézier
//! patches over the rectangles where every blending function is one polynomial.

use super::mesh::{Grid, TMesh};
use super::{Blend, TSpline, frame};
use crate::bspline::{BezierPatch, HPoint, NurbsSurface, SplineError, SplineResult, basis_bezier, invalid};

/// Most unit-edge marks and coefficient products in one Bézier extraction.
const MAX_EXTRACT_WORK: usize = 1 << 26;

impl TSpline {
    /// A tensor NURBS surface of odd bi-degree as a T-spline on the full grid: column `i` of the
    /// mesh is knot `i + (p+1)/2` of the knot vector, so the blending function at a vertex is
    /// the B-spline basis function of that control point (rays that leave the mesh repeat the
    /// boundary line, 2003 §3). Knot vectors whose first and last `(p+1)/2 + 1` knots are not
    /// equal are clamped first (same surface on the same domain).
    pub fn from_nurbs(n: &NurbsSurface) -> SplineResult<TSpline> {
        let degree = n.degree();
        if degree.iter().any(|p| p % 2 == 0) {
            return Err(SplineError::Unsupported("a NURBS surface of even degree as a T-spline".into()));
        }
        let ends_ok = |k: &[f64], p: usize| {
            let h = p.div_ceil(2);
            let same = |w: Option<&[f64]>| w.is_some_and(|w| w.windows(2).all(|x| matches!(x, [a, b] if a == b)));
            same(k.get(..=h)) && same(k.len().checked_sub(h + 1).and_then(|s| k.get(s..)))
        };
        let n = if (0..2).all(|d| ends_ok(&n.knots()[d], degree[d])) { n.clone() } else { n.clamped()? };
        let lines = |d: usize| -> Vec<f64> {
            let h = degree[d].div_ceil(2);
            let k = &n.knots()[d];
            k.get(h..k.len().saturating_sub(h)).map(<[f64]>::to_vec).unwrap_or_default()
        };
        let mesh = TMesh::tensor(lines(0), lines(1))?;
        TSpline::from_homogeneous(mesh, degree, n.homogeneous().to_vec())
    }

    /// The T-spline as a tensor NURBS surface, when its mesh has no T-junctions.
    pub fn to_nurbs(&self) -> SplineResult<NurbsSurface> {
        if !self.mesh.is_regular() {
            return Err(SplineError::Unsupported("a T-mesh with T-junctions as one tensor surface".into()));
        }
        let pad = |k: &[f64], p: usize| -> Vec<f64> {
            let h = p.div_ceil(2);
            let (first, last) = (k.first().copied().unwrap_or(0.0), k.last().copied().unwrap_or(0.0));
            std::iter::repeat_n(first, h).chain(k.iter().copied()).chain(std::iter::repeat_n(last, h)).collect()
        };
        let g = &self.mesh.grid;
        NurbsSurface::from_homogeneous(self.degree, [pad(&g.s, self.degree[0]), pad(&g.t, self.degree[1])], self.ctrl.clone())
    }

    /// The surface as rational Bézier patches over rectangles of the domain, inside each of
    /// which every blending function is one polynomial: the knot lines of all blending
    /// functions (each function's s-knots across its t-support and t-knots across its
    /// s-support) are marked on the index grid and the domain's cells are gathered into
    /// rectangles that no marked line crosses. Each function's Bernstein coefficients on a
    /// rectangle come from knot insertion ([`basis_bezier`]). Patches tile the domain and meet
    /// at T-points where their corners land on a neighbour's side.
    pub fn bezier_elements(&self) -> SplineResult<Vec<BezierPatch>> {
        let g = &self.mesh.grid;
        let marks = knot_lines(g, &self.blends)?;
        let [cs, ct] = frame(self.degree);
        let (ns, nt) = (g.ns(), g.nt());
        let (ci_end, cj_end) = (ns - 1 - cs, nt - 1 - ct);
        let mut taken = vec![false; ns * nt];
        let is_taken = |taken: &[bool], i: usize, j: usize| taken.get(j * ns + i).copied().unwrap_or(true);
        let mut rects = Vec::new();
        for cj in ct..cj_end {
            for ci in cs..ci_end {
                if is_taken(&taken, ci, cj) {
                    continue;
                }
                let mut i1 = ci + 1;
                while i1 < ci_end && !is_taken(&taken, i1, cj) && !marks.v(i1, cj) {
                    i1 += 1;
                }
                let mut j1 = cj + 1;
                while j1 < cj_end && (ci..i1).all(|x| !is_taken(&taken, x, j1) && !marks.h(x, j1)) && (ci + 1..i1).all(|x| !marks.v(x, j1)) {
                    j1 += 1;
                }
                for y in cj..j1 {
                    for x in ci..i1 {
                        if let Some(c) = taken.get_mut(y * ns + x) {
                            *c = true;
                        }
                    }
                }
                rects.push((ci, i1, cj, j1));
            }
        }
        let val = |k: &[f64], i: usize| k.get(i).copied().unwrap_or(0.0);
        let [ps, pt] = self.degree;
        let mut work = 0usize;
        let mut out = Vec::new();
        for (i0, i1, j0, j1) in rects {
            let (s0, s1, t0, t1) = (val(&g.s, i0), val(&g.s, i1), val(&g.t, j0), val(&g.t, j1));
            if !(s0 < s1 && t0 < t1) {
                continue;
            }
            let mut ctrl = vec![HPoint::ZERO; (ps + 1) * (pt + 1)];
            for (b, c) in self.blends.iter().zip(&self.ctrl) {
                let [[bs0, bs1], [bt0, bt1]] = b.support();
                if !(s1.min(bs1) > s0.max(bs0) && t1.min(bt1) > t0.max(bt0)) {
                    continue;
                }
                work += ctrl.len();
                if work > MAX_EXTRACT_WORK {
                    return Err(SplineError::TooLarge("Bézier extraction needs too much work".into()));
                }
                let cs = basis_bezier(&b.sv, s0, s1)?;
                let ct = basis_bezier(&b.tv, t0, t1)?;
                for (j, bj) in ct.iter().enumerate() {
                    for (i, bi) in cs.iter().enumerate() {
                        if let Some(slot) = ctrl.get_mut(j * (ps + 1) + i) {
                            *slot += *c * (bi * bj);
                        }
                    }
                }
            }
            if ctrl.iter().any(|h| !h.is_finite() || !(h.w > 0.0)) {
                return invalid("a Bézier patch has a non-positive weight");
            }
            out.push(BezierPatch { s: [s0, s1], t: [t0, t1], degree: self.degree, ctrl });
        }
        Ok(out)
    }
}

/// The unit edges of the index grid that carry a knot line of some blending function: each
/// function's s-knots across its t-support and its t-knots across its s-support.
fn knot_lines(g: &Grid, blends: &[Blend]) -> SplineResult<Grid> {
    let mut m = Grid::empty(g.s.clone(), g.t.clone())?;
    let mut work = 0usize;
    for b in blends {
        let (Some(&s0), Some(&s1), Some(&t0), Some(&t1)) = (b.s.first(), b.s.last(), b.t.first(), b.t.last()) else { continue };
        work += (b.s.len() * (t1 - t0)) + (b.t.len() * (s1 - s0));
        if work > MAX_EXTRACT_WORK {
            return Err(SplineError::TooLarge("Bézier extraction needs too much work".into()));
        }
        for &x in &b.s {
            (t0..t1).for_each(|y| m.set_v(x, y));
        }
        for &y in &b.t {
            (s0..s1).for_each(|x| m.set_h(x, y));
        }
    }
    Ok(m)
}
