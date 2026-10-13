//! Local refinement that keeps the surface exactly (Sederberg, Cardon, Finnigan, North,
//! Zheng, Lyche 2004, §3).
//!
//! The requested knot lines are cut into the T-mesh first. Then every blending function of
//! the original surface is carried as a record — its local knot vectors (as index lines) and
//! its homogeneous coefficient — and the records are made to agree with the refined mesh:
//!
//! * **Violation 1** — the mesh's ray from a record's anchor crosses a line inside the
//!   record's support that the record lacks: insert that knot into the record, which splits
//!   it into two records (knot insertion into one blending function, 2004 §3).
//! * **Violation 2** — the record has a knot where the ray crosses nothing: split the face
//!   there so the mesh dictates that knot.
//! * **Violation 3** — a record's anchor is not a vertex: split a face so it becomes one.
//!
//! Records with the same knot vectors merge. When nothing is violated, each record is exactly
//! the blending function of the vertex at its anchor, so summing the coefficients per vertex
//! gives control points of the refined T-spline, and the surface is unchanged. All knots are
//! handled as line indices, so every comparison is exact; no new index line appears after
//! the cuts, so the work is bounded by the grid.

use std::collections::BTreeMap;

use super::TSpline;
use super::mesh::{Grid, TMesh};
use crate::bspline::{Dir, HPoint, SplineError, SplineResult, invalid, split_basis, split_knots};

/// Most cuts in one refinement.
const MAX_CUTS: usize = 1 << 12;
/// Most record checks and mesh changes in one refinement.
const MAX_REFINE_STEPS: usize = 1 << 22;

/// A knot line to cut into a T-spline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cut {
    /// The line `s = s` from `t = t0` to `t = t1`: every face it crosses there is split.
    S { s: f64, t0: f64, t1: f64 },
    /// The line `t = t` from `s = s0` to `s = s1`.
    T { t: f64, s0: f64, s1: f64 },
}

impl Cut {
    /// The direction the cut line is constant in: [`Dir::S`] for a line `s = …`.
    fn dir(&self) -> Dir {
        match self {
            Cut::S { .. } => Dir::S,
            Cut::T { .. } => Dir::T,
        }
    }
}

/// A record's knot vectors (s, t) as line indices.
type Key = (Vec<usize>, Vec<usize>);

enum Fix {
    /// Violation 1: insert line `k` into the record along the direction.
    Split(Dir, usize),
    /// Violations 2 and 3: the point (column, row) must lie on a column edge.
    Column(usize, usize),
    /// … on a row edge.
    Row(usize, usize),
}

impl TSpline {
    /// The same surface on a refined T-mesh with the given knot lines cut in (2004 §3; see the
    /// module notes). The cut lines must lie strictly inside the domain; a cut at the value
    /// of an existing knot line extends that line. Further faces may be split to keep the
    /// surface exact and, when this T-spline is analysis-suitable, to keep the result
    /// analysis-suitable (T-junctions are carried across faces until no s-extension meets a
    /// t-extension; Scott et al. 2012).
    pub fn refine(&self, cuts: &[Cut]) -> SplineResult<TSpline> {
        if cuts.len() > MAX_CUTS {
            return Err(SplineError::TooLarge(format!("{} cuts (at most {MAX_CUTS})", cuts.len())));
        }
        let mut g = self.mesh.grid.clone();
        let mut smap: Vec<usize> = (0..g.ns()).collect();
        let mut tmap: Vec<usize> = (0..g.nt()).collect();
        let [ds, dt] = self.domain();
        // Row cuts run as column cuts on the transposed grid; consecutive cuts along one
        // direction share a transposition.
        for run in cuts.chunk_by(|a, b| a.dir() == b.dir()) {
            let along_t = run.first().is_some_and(|c| c.dir() == Dir::T);
            if along_t {
                g = g.transposed();
            }
            for cut in run {
                match *cut {
                    Cut::S { s, t0, t1 } => cut_column(&mut g, &mut smap, s, [t0, t1], ds)?,
                    Cut::T { t, s0, s1 } => cut_column(&mut g, &mut tmap, t, [s0, s1], dt)?,
                }
            }
            if along_t {
                g = g.transposed();
            }
        }
        let remap = |idx: &[usize], map: &[usize]| -> Vec<usize> { idx.iter().map(|&x| map.get(x).copied().unwrap_or(x)).collect() };
        let mut rec: BTreeMap<Key, HPoint> = BTreeMap::new();
        for (b, c) in self.blends.iter().zip(&self.ctrl) {
            *rec.entry((remap(&b.s, &smap), remap(&b.t, &tmap))).or_insert(HPoint::ZERO) += *c;
        }
        // Keep analysis-suitability (Scott et al. 2012): while an s-extension meets a
        // t-extension, carry the s-extension's T-junction across its face, then make the
        // records agree with the mesh again. Every round adds edges, so this ends.
        let keep_suitable = self.is_analysis_suitable();
        let mesh = loop {
            close(&mut g, &mut rec, self.degree)?;
            let mesh = TMesh::from_grid(g.clone())?;
            match mesh.conflict(self.degree).filter(|_| keep_suitable) {
                Some(e) => extend_t_junction(&mut g, e.from),
                None => break mesh,
            }
        };
        let mut ctrl = vec![HPoint::ZERO; mesh.vertices().len()];
        let mut seen = vec![false; ctrl.len()];
        for ((s, t), c) in &rec {
            let (ci, cj) = (centre(s), centre(t));
            let k = mesh.vertex_at(ci, cj).ok_or_else(|| SplineError::Invalid("refinement left a blending function without a vertex".into()))?;
            if let (Some(slot), Some(f)) = (ctrl.get_mut(k), seen.get_mut(k)) {
                *slot += *c;
                *f = true;
            }
        }
        if let Some(k) = seen.iter().position(|&f| !f) {
            let (i, j) = mesh.vertices().get(k).copied().unwrap_or_default();
            return Err(SplineError::Unsupported(format!(
                "this refinement leaves the control point at ({i}, {j}) without a blending function (the refined T-mesh is not analysis-suitable)"
            )));
        }
        let out = TSpline::from_homogeneous(mesh, self.degree, ctrl)?;
        // The records are the refined blending functions, one per vertex.
        for (s, t) in rec.keys() {
            let k = out.mesh.vertex_at(centre(s), centre(t));
            if k.and_then(|k| out.blends.get(k)).is_none_or(|b| &b.s != s || &b.t != t) {
                return Err(SplineError::Invalid("refinement did not converge to the refined mesh's blending functions".into()));
            }
        }
        Ok(out)
    }
}

/// Carry a T-junction's missing edge across the face beyond it.
fn extend_t_junction(g: &mut Grid, (i, j): (usize, usize)) {
    let (l, r, d, u) = (i > 0 && g.h(i - 1, j), g.h(i, j), j > 0 && g.v(i, j - 1), g.v(i, j));
    if !r {
        g.split_row_at(i, j);
    } else if !l {
        g.split_row_at(i.saturating_sub(1), j);
    } else if !u {
        g.split_column_at(i, j);
    } else if !d {
        g.split_column_at(i, j.saturating_sub(1));
    }
}

/// The anchor line of a local knot vector.
fn centre(k: &[usize]) -> usize {
    k.get(k.len() / 2).copied().unwrap_or(0)
}

/// Cut the column line `value` into every face it crosses between `range[0]` and `range[1]`,
/// adding the column if needed.
fn cut_column(g: &mut Grid, map: &mut [usize], value: f64, range: [f64; 2], dom: [f64; 2]) -> SplineResult<()> {
    let [a, b] = range;
    if !(value.is_finite() && a.is_finite() && b.is_finite()) {
        return invalid("a cut needs finite numbers");
    }
    if !(a < b) {
        return invalid("a cut needs a non-empty range");
    }
    if !(value > dom[0] && value < dom[1]) {
        return invalid(format!("the cut at {value} is not inside the domain"));
    }
    let same: Vec<usize> = (0..g.ns()).filter(|&x| g.s.get(x) == Some(&value)).collect();
    let (x, new) = match same.as_slice() {
        [] => {
            let at = g.s.iter().position(|&k| k > value).unwrap_or(g.ns() - 1);
            g.insert_column(at, value)?;
            map.iter_mut().filter(|m| **m >= at).for_each(|m| *m += 1);
            (at, true)
        }
        [x] => (*x, false),
        _ => return Err(SplineError::Unsupported(format!("cutting along the repeated knot {value}"))),
    };
    let mut added = false;
    for y in 0..g.nt() - 1 {
        if g.v(x, y) {
            continue;
        }
        let f = g.face_of_cell(x, y);
        let (fa, fb) = (g.t.get(f.j0).copied().unwrap_or(0.0), g.t.get(f.j1).copied().unwrap_or(0.0));
        // Faces overlapping the range, and faces of zero height inside it (so a cut from a
        // clamped domain end runs on through the zero-height frame to the boundary).
        if fb.min(b) - fa.max(a) > 0.0 || (fa == fb && fa >= a && fb <= b) {
            (f.j0..f.j1).for_each(|j| g.set_v(x, j));
            added = true;
        }
    }
    if new && !added {
        return invalid(format!("the cut at {value} crosses no face"));
    }
    Ok(())
}

/// Resolve the violations until every record is a blending function of the mesh.
fn close(g: &mut Grid, rec: &mut BTreeMap<Key, HPoint>, degree: [usize; 2]) -> SplineResult<()> {
    let mut steps = 0usize;
    let mut tick = |n: usize| -> SplineResult<()> {
        steps += n;
        if steps > MAX_REFINE_STEPS { Err(SplineError::TooLarge("the refinement needs too much work".into())) } else { Ok(()) }
    };
    loop {
        // Violation 1 first (knot insertion into records), until none is left.
        loop {
            let keys: Vec<Key> = rec.keys().cloned().collect();
            tick(keys.len())?;
            let mut changed = false;
            for key in keys {
                if let Some(Fix::Split(dir, k)) = check(g, &key, degree)
                    && let Some(c) = rec.remove(&key)
                {
                    for (piece, f) in split(g, &key, dir, k)? {
                        if f != 0.0 {
                            *rec.entry(piece).or_insert(HPoint::ZERO) += c * f;
                        }
                    }
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        // Then one mesh change for the first record that needs one.
        tick(rec.len())?;
        let fix = rec.keys().find_map(|k| match check(g, k, degree) {
            Some(Fix::Split(..)) | None => None,
            other => other,
        });
        match fix {
            Some(Fix::Column(x, y)) => g.split_column_at(x, y),
            Some(Fix::Row(x, y)) => g.split_row_at(x, y),
            _ => return Ok(()),
        }
    }
}

/// The first violation of a record against the mesh, if any.
fn check(g: &Grid, key: &Key, degree: [usize; 2]) -> Option<Fix> {
    let (s, t) = key;
    let (ci, cj) = (centre(s), centre(t));
    let ms = g.local_knots(ci, cj, Dir::S, degree[0]);
    match compare(s, &ms) {
        Some(Ok(k)) => return Some(Fix::Split(Dir::S, k)),
        Some(Err(x)) => return Some(Fix::Column(x, cj)),
        None => {}
    }
    let mt = g.local_knots(ci, cj, Dir::T, degree[1]);
    match compare(t, &mt) {
        Some(Ok(k)) => return Some(Fix::Split(Dir::T, k)),
        Some(Err(y)) => return Some(Fix::Row(ci, y)),
        None => {}
    }
    if !g.on_column(ci, cj) {
        return Some(Fix::Column(ci, cj));
    }
    if !g.on_row(ci, cj) {
        return Some(Fix::Row(ci, cj));
    }
    None
}

/// Compare a record's knots with the mesh's, outward from the anchor: `Ok(k)` the mesh
/// crosses line `k` first (insert it into the record), `Err(x)` the record has line `x` where
/// the mesh crosses nothing (the mesh must gain it).
fn compare(rec: &[usize], mesh: &[usize]) -> Option<Result<usize, usize>> {
    let mid = rec.len() / 2;
    for k in mid + 1..rec.len() {
        let (r, m) = (rec.get(k)?, mesh.get(k)?);
        if m < r {
            return Some(Ok(*m));
        }
        if m > r {
            return Some(Err(*r));
        }
    }
    for k in (0..mid).rev() {
        let (r, m) = (rec.get(k)?, mesh.get(k)?);
        if m > r {
            return Some(Ok(*m));
        }
        if m < r {
            return Some(Err(*r));
        }
    }
    None
}

/// Insert line `k` into a record along `dir`: the two pieces and their factors (2004 §3).
/// Pieces whose knots all have one value are the zero function and are dropped.
fn split(g: &Grid, key: &Key, dir: Dir, k: usize) -> SplineResult<Vec<(Key, f64)>> {
    let (idx, vals) = dir.pick((&key.0, &g.s), (&key.1, &g.t));
    let value = |x: usize| vals.get(x).copied().unwrap_or(0.0);
    let values: Vec<f64> = idx.iter().map(|&x| value(x)).collect();
    let (c1, c2) = split_basis(&values, value(k)).ok_or_else(|| SplineError::Invalid("refinement inserted a knot outside a support".into()))?;
    let mut out = Vec::with_capacity(2);
    for (piece, c) in split_knots(idx, k).into_iter().zip([c1, c2]) {
        let (Some(&lo), Some(&hi)) = (piece.first(), piece.last()) else { continue };
        if value(lo) == value(hi) {
            continue;
        }
        let key = match dir {
            Dir::S => (piece, key.1.clone()),
            Dir::T => (key.0.clone(), piece),
        };
        out.push((key, c));
    }
    Ok(out)
}
