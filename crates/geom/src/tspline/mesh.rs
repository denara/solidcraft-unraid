//! The T-mesh in index space (Sederberg, Zheng, Bakenov, Nasri 2003, §3; Scott, Li,
//! Sederberg, Hughes 2012, §2): a rectangle of index lines — columns `0..ns` carrying the
//! non-decreasing knot values `s[i]`, rows `0..nt` carrying `t[j]` — and the unit edges of
//! that grid that are present. Equal knot values on neighbouring lines give zero-length knot
//! intervals (repeated knots), which is how clamped boundaries and multiple knots are written.
//!
//! A valid T-mesh has its outer rectangle, rectangular faces (every grid point is empty, an
//! edge passing straight through, a T-junction, a crossing, or one of the four outer
//! corners), and at least one edge on every index line. A *vertex* is a point on both a column
//! edge and a row edge; every vertex carries a control point.

use crate::bspline::{Dir, SplineError, SplineResult, check_knots, invalid};

/// Most index lines in either direction.
pub const MAX_INDEX_LINES: usize = 1 << 12;
/// Most grid points (`ns · nt`).
pub const MAX_GRID_POINTS: usize = 1 << 20;

/// A run of edges on one index line, given to [`TMesh::from_segments`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Segment {
    /// Edges on column `i` (constant s) from row `j0` to row `j1`.
    Column { i: usize, j0: usize, j1: usize },
    /// Edges on row `j` (constant t) from column `i0` to column `i1`.
    Row { j: usize, i0: usize, i1: usize },
}

/// A rectangle in index space, `[i0, i1] × [j0, j1]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IndexRect {
    pub i0: usize,
    pub i1: usize,
    pub j0: usize,
    pub j1: usize,
}

/// The index lines and the unit edges present on them; no validity implied.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Grid {
    pub(crate) s: Vec<f64>,
    pub(crate) t: Vec<f64>,
    /// Row edge `(i, j)–(i+1, j)` at `j·(ns−1) + i`.
    h: Vec<bool>,
    /// Column edge `(i, j)–(i, j+1)` at `i·(nt−1) + j`.
    v: Vec<bool>,
}

impl Grid {
    /// An empty grid (no edges) after checking sizes and knots.
    pub(crate) fn empty(s: Vec<f64>, t: Vec<f64>) -> SplineResult<Grid> {
        for k in [&s, &t] {
            if k.len() < 2 {
                return invalid("a T-mesh needs at least two index lines each way");
            }
            if k.len() > MAX_INDEX_LINES {
                return Err(SplineError::TooLarge(format!("{} index lines (at most {MAX_INDEX_LINES})", k.len())));
            }
            check_knots(k)?;
        }
        if s.len() * t.len() > MAX_GRID_POINTS {
            return Err(SplineError::TooLarge(format!("{} grid points (at most {MAX_GRID_POINTS})", s.len() * t.len())));
        }
        let (ns, nt) = (s.len(), t.len());
        Ok(Grid { h: vec![false; (ns - 1) * nt], v: vec![false; ns * (nt - 1)], s, t })
    }

    pub(crate) fn ns(&self) -> usize {
        self.s.len()
    }
    pub(crate) fn nt(&self) -> usize {
        self.t.len()
    }

    /// Row edge from `(i, j)` to `(i+1, j)`.
    pub(crate) fn h(&self, i: usize, j: usize) -> bool {
        i + 1 < self.ns() && self.h.get(j * (self.ns() - 1) + i).copied().unwrap_or(false)
    }
    /// Column edge from `(i, j)` to `(i, j+1)`.
    pub(crate) fn v(&self, i: usize, j: usize) -> bool {
        j + 1 < self.nt() && self.v.get(i * (self.nt() - 1) + j).copied().unwrap_or(false)
    }
    pub(crate) fn set_h(&mut self, i: usize, j: usize) {
        let ns = self.ns();
        if i + 1 < ns
            && let Some(e) = self.h.get_mut(j * (ns - 1) + i)
        {
            *e = true;
        }
    }
    pub(crate) fn set_v(&mut self, i: usize, j: usize) {
        let nt = self.nt();
        if j + 1 < nt
            && let Some(e) = self.v.get_mut(i * (nt - 1) + j)
        {
            *e = true;
        }
    }

    /// The point `(i, j)` lies on a column edge (passing through or ending there).
    pub(crate) fn on_column(&self, i: usize, j: usize) -> bool {
        (j > 0 && self.v(i, j - 1)) || self.v(i, j)
    }
    /// The point `(i, j)` lies on a row edge.
    pub(crate) fn on_row(&self, i: usize, j: usize) -> bool {
        (i > 0 && self.h(i - 1, j)) || self.h(i, j)
    }

    /// The first `count` index lines a ray from `(i, j)` crosses (2003 §3: the knots of a
    /// blending function are read off rays cast from its anchor). Along s it crosses columns
    /// carrying an edge at row `j`; along t, rows carrying an edge at column `i`. Nearest
    /// first; a ray that leaves the mesh repeats the boundary line (clamped ends).
    pub(crate) fn ray(&self, i: usize, j: usize, dir: Dir, forward: bool, count: usize) -> Vec<usize> {
        let (start, last) = dir.pick((i, self.ns() - 1), (j, self.nt() - 1));
        let hit = |x: usize| match dir {
            Dir::S => self.on_column(x, j),
            Dir::T => self.on_row(i, x),
        };
        let mut out = Vec::with_capacity(count);
        let mut x = start;
        while out.len() < count {
            let next = if forward { (x < last).then(|| x + 1) } else { x.checked_sub(1) };
            match next {
                Some(n) => {
                    x = n;
                    if hit(x) {
                        out.push(x);
                    }
                }
                None => out.push(x),
            }
        }
        out
    }

    /// The local knot vector (as line indices) of a degree-`p` blending function anchored at
    /// `(i, j)` in direction `dir`: `(p+1)/2` crossings each side of the anchor's own line
    /// (odd degree; 2003 §3).
    pub(crate) fn local_knots(&self, i: usize, j: usize, dir: Dir, p: usize) -> Vec<usize> {
        let h = p.div_ceil(2);
        let own = dir.pick(i, j);
        let mut k: Vec<usize> = self.ray(i, j, dir, false, h);
        k.reverse();
        k.push(own);
        k.extend(self.ray(i, j, dir, true, h));
        k
    }

    /// The face containing the unit cell whose lower-left corner is `(ci, cj)`.
    pub(crate) fn face_of_cell(&self, ci: usize, cj: usize) -> IndexRect {
        let i0 = (0..=ci).rev().find(|&x| self.v(x, cj)).unwrap_or(0);
        let i1 = (ci + 1..self.ns()).find(|&x| self.v(x, cj)).unwrap_or(self.ns() - 1);
        let j0 = (0..=cj).rev().find(|&y| self.h(ci, y)).unwrap_or(0);
        let j1 = (cj + 1..self.nt()).find(|&y| self.h(ci, y)).unwrap_or(self.nt() - 1);
        IndexRect { i0, i1, j0, j1 }
    }

    /// Split a face with a full-height column edge at `x` so that the point `(x, y)` lies on
    /// a column edge (the face above the point, or below it on the top row).
    pub(crate) fn split_column_at(&mut self, x: usize, y: usize) {
        let cj = if y + 1 < self.nt() { y } else { y.saturating_sub(1) };
        let f = self.face_of_cell(x.min(self.ns() - 2), cj);
        for j in f.j0..f.j1 {
            self.set_v(x, j);
        }
    }
    /// Split a face with a full-width row edge at `y` so that `(x, y)` lies on a row edge (the
    /// face to the right of the point, or to its left on the last column).
    pub(crate) fn split_row_at(&mut self, x: usize, y: usize) {
        let ci = if x + 1 < self.ns() { x } else { x.saturating_sub(1) };
        let f = self.face_of_cell(ci, y.min(self.nt() - 2));
        for i in f.i0..f.i1 {
            self.set_h(i, y);
        }
    }

    /// Insert a new column at index `at` (`0 < at < ns`) with knot value `value`; row edges
    /// crossing it are split in two, the new column has no edges.
    pub(crate) fn insert_column(&mut self, at: usize, value: f64) -> SplineResult<()> {
        let (ns, nt) = (self.ns(), self.nt());
        if at == 0 || at >= ns || ns + 1 > MAX_INDEX_LINES || (ns + 1) * nt > MAX_GRID_POINTS {
            return Err(SplineError::TooLarge("refinement needs too many index lines".into()));
        }
        let mut h = Vec::with_capacity(ns * nt);
        for j in 0..nt {
            for i in 0..ns {
                // New row edge i runs (i, i+1) in the new numbering.
                let old = if i + 1 < at {
                    self.h(i, j)
                } else if i + 1 == at || i == at {
                    self.h(at - 1, j)
                } else {
                    self.h(i - 1, j)
                };
                h.push(old);
            }
        }
        let mut v = Vec::with_capacity((ns + 1) * (nt - 1));
        for i in 0..=ns {
            for j in 0..nt - 1 {
                v.push(match i.cmp(&at) {
                    std::cmp::Ordering::Less => self.v(i, j),
                    std::cmp::Ordering::Equal => false,
                    std::cmp::Ordering::Greater => self.v(i - 1, j),
                });
            }
        }
        self.s.insert(at, value);
        self.h = h;
        self.v = v;
        Ok(())
    }

    /// The grid transposed (s ↔ t), so row operations can reuse column code.
    pub(crate) fn transposed(&self) -> Grid {
        let (ns, nt) = (self.ns(), self.nt());
        let mut g = Grid { s: self.t.clone(), t: self.s.clone(), h: vec![false; (nt - 1) * ns], v: vec![false; nt * (ns - 1)] };
        for i in 0..ns {
            for j in 0..nt {
                if self.h(i, j) {
                    g.set_v(j, i);
                }
                if self.v(i, j) {
                    g.set_h(j, i);
                }
            }
        }
        g
    }
}

/// A validated T-mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct TMesh {
    pub(crate) grid: Grid,
    vertices: Vec<(usize, usize)>,
    /// Vertex number of grid point `(i, j)` at `j·ns + i`, `u32::MAX` where there is none.
    vid: Vec<u32>,
}

impl TMesh {
    /// The full tensor-product mesh on the given knot lines.
    pub fn tensor(s: Vec<f64>, t: Vec<f64>) -> SplineResult<TMesh> {
        let (ns, nt) = (s.len(), t.len());
        let segs: Vec<Segment> = (0..ns)
            .map(|i| Segment::Column { i, j0: 0, j1: nt.saturating_sub(1) })
            .chain((0..nt).map(|j| Segment::Row { j, i0: 0, i1: ns.saturating_sub(1) }))
            .collect();
        TMesh::from_segments(s, t, &segs)
    }

    /// A T-mesh from knot lines and runs of edges (the outer rectangle must be among them).
    pub fn from_segments(s: Vec<f64>, t: Vec<f64>, segments: &[Segment]) -> SplineResult<TMesh> {
        let mut g = Grid::empty(s, t)?;
        if segments.len() > MAX_GRID_POINTS {
            return Err(SplineError::TooLarge("too many segments".into()));
        }
        let (ns, nt) = (g.ns(), g.nt());
        for seg in segments {
            match *seg {
                Segment::Column { i, j0, j1 } => {
                    if i >= ns || j0 >= j1 || j1 >= nt {
                        return invalid(format!("column segment {i}: rows {j0}..{j1} are not inside the mesh"));
                    }
                    (j0..j1).for_each(|j| g.set_v(i, j));
                }
                Segment::Row { j, i0, i1 } => {
                    if j >= nt || i0 >= i1 || i1 >= ns {
                        return invalid(format!("row segment {j}: columns {i0}..{i1} are not inside the mesh"));
                    }
                    (i0..i1).for_each(|i| g.set_h(i, j));
                }
            }
        }
        TMesh::from_grid(g)
    }

    /// Validate a grid as a T-mesh.
    pub(crate) fn from_grid(g: Grid) -> SplineResult<TMesh> {
        let (ns, nt) = (g.ns(), g.nt());
        if (0..ns - 1).any(|i| !g.h(i, 0) || !g.h(i, nt - 1)) || (0..nt - 1).any(|j| !g.v(0, j) || !g.v(ns - 1, j)) {
            return invalid("the T-mesh's outer rectangle is not closed");
        }
        if let Some(i) = (0..ns).find(|&i| !(0..nt - 1).any(|j| g.v(i, j))) {
            return invalid(format!("column {i} carries no edge"));
        }
        if let Some(j) = (0..nt).find(|&j| !(0..ns - 1).any(|i| g.h(i, j))) {
            return invalid(format!("row {j} carries no edge"));
        }
        let mut vertices = Vec::new();
        let mut vid = vec![u32::MAX; ns * nt];
        for j in 0..nt {
            for i in 0..ns {
                let l = i > 0 && g.h(i - 1, j);
                let r = g.h(i, j);
                let d = j > 0 && g.v(i, j - 1);
                let u = g.v(i, j);
                let n = [l, r, d, u].iter().filter(|&&e| e).count();
                let corner = (i == 0 || i == ns - 1) && (j == 0 || j == nt - 1);
                match n {
                    1 => return invalid(format!("an edge ends without meeting another at ({i}, {j})")),
                    2 if (l != r) && !corner => return invalid(format!("a face is not rectangular at ({i}, {j})")),
                    _ => {}
                }
                if (l || r) && (d || u) {
                    if let Some(slot) = vid.get_mut(j * ns + i) {
                        *slot = vertices.len() as u32;
                    }
                    vertices.push((i, j));
                }
            }
        }
        Ok(TMesh { grid: g, vertices, vid })
    }

    /// Knot values of the columns.
    pub fn s_knots(&self) -> &[f64] {
        &self.grid.s
    }
    /// Knot values of the rows.
    pub fn t_knots(&self) -> &[f64] {
        &self.grid.t
    }
    /// The vertices `(column, row)`, row by row (column fastest). Control points follow this
    /// order.
    pub fn vertices(&self) -> &[(usize, usize)] {
        &self.vertices
    }
    /// Vertex number of grid point `(i, j)`.
    pub fn vertex_at(&self, i: usize, j: usize) -> Option<usize> {
        if i >= self.grid.ns() {
            return None;
        }
        self.vid.get(j * self.grid.ns() + i).filter(|&&k| k != u32::MAX).map(|&k| k as usize)
    }
    /// The parameter point `(s, t)` of vertex `k`.
    pub fn vertex_param(&self, k: usize) -> Option<(f64, f64)> {
        let &(i, j) = self.vertices.get(k)?;
        Some((*self.grid.s.get(i)?, *self.grid.t.get(j)?))
    }
    /// Is the edge from `(i, j)` to `(i+1, j)` present?
    pub fn row_edge(&self, i: usize, j: usize) -> bool {
        self.grid.h(i, j)
    }
    /// Is the edge from `(i, j)` to `(i, j+1)` present?
    pub fn column_edge(&self, i: usize, j: usize) -> bool {
        self.grid.v(i, j)
    }
    /// Every unit edge present: a tensor-product mesh without T-junctions.
    pub fn is_regular(&self) -> bool {
        (0..self.grid.nt()).all(|j| (0..self.grid.ns() - 1).all(|i| self.grid.h(i, j)))
            && (0..self.grid.ns()).all(|i| (0..self.grid.nt() - 1).all(|j| self.grid.v(i, j)))
    }
    /// The faces, by their lower-left corner.
    pub fn faces(&self) -> Vec<IndexRect> {
        let g = &self.grid;
        let mut out = Vec::new();
        for cj in 0..g.nt() - 1 {
            for ci in 0..g.ns() - 1 {
                if g.v(ci, cj) && g.h(ci, cj) {
                    out.push(g.face_of_cell(ci, cj));
                }
            }
        }
        out
    }

    /// The local knot vector (line indices) of a degree-`p` blending function anchored at
    /// vertex `(i, j)` in direction `dir` (2003 §3).
    pub fn local_knots(&self, i: usize, j: usize, dir: Dir, p: usize) -> Vec<usize> {
        self.grid.local_knots(i, j, dir, p)
    }

    /// The T-junction extensions for bi-degree `degree` (Scott, Li, Sederberg, Hughes 2012,
    /// §2, after Li, Zheng, Sederberg, Hughes, Scott 2012): from each interior T-junction a face
    /// extension runs in the direction of the missing edge across `⌈p/2⌉` perpendicular lines
    /// carrying an edge there, and an edge extension the other way across `⌊p/2⌋` of them.
    /// `p` is the degree along the extension.
    pub fn extensions(&self, degree: [usize; 2]) -> Vec<Extension> {
        let g = &self.grid;
        let (ns, nt) = (g.ns(), g.nt());
        let mut out = Vec::new();
        for &(i, j) in &self.vertices {
            if i == 0 || j == 0 || i == ns - 1 || j == nt - 1 {
                continue;
            }
            let (l, r, d, u) = (g.h(i - 1, j), g.h(i, j), g.v(i, j - 1), g.v(i, j));
            if [l, r, d, u].iter().filter(|&&e| e).count() != 3 {
                continue;
            }
            let (dir, missing_forward) = if !l || !r { (Dir::S, !r) } else { (Dir::T, !u) };
            let p = dir.pick(degree[0], degree[1]);
            let reach = |forward: bool, count: usize| -> usize {
                if count == 0 {
                    return dir.pick(i, j);
                }
                g.ray(i, j, dir, forward, count).last().copied().unwrap_or(0)
            };
            let face = reach(missing_forward, p.div_ceil(2));
            let edge = reach(!missing_forward, p / 2);
            let range = if face < edge { [face, edge] } else { [edge, face] };
            let at = dir.pick(j, i);
            out.push(Extension { dir, at, range, from: (i, j) });
        }
        out
    }

    /// Analysis-suitable for bi-degree `degree`: no extension along s meets an extension along
    /// t (Scott et al. 2012, §2). Analysis-suitable T-splines have linearly independent
    /// blending functions that form a partition of unity.
    pub fn is_analysis_suitable(&self, degree: [usize; 2]) -> bool {
        self.conflict(degree).is_none()
    }

    /// The first extension along s that meets an extension along t.
    pub(crate) fn conflict(&self, degree: [usize; 2]) -> Option<Extension> {
        let ns = self.grid.ns();
        let ext = self.extensions(degree);
        let mut covered: Vec<Option<usize>> = vec![None; ns * self.grid.nt()];
        for (k, e) in ext.iter().enumerate().filter(|(_, e)| e.dir == Dir::S) {
            for x in e.range[0]..=e.range[1] {
                if let Some(c) = covered.get_mut(e.at * ns + x) {
                    *c = Some(k);
                }
            }
        }
        ext.iter()
            .filter(|e| e.dir == Dir::T)
            .find_map(|e| (e.range[0]..=e.range[1]).find_map(|y| covered.get(y * ns + e.at).copied().flatten()))
            .and_then(|k| ext.get(k).copied())
    }
}

/// A T-junction extension: a closed segment along `dir` on index line `at` (the row for
/// [`Dir::S`], the column for [`Dir::T`]) spanning the lines `range`, from the T-junction
/// `from`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extension {
    pub dir: Dir,
    pub at: usize,
    pub range: [usize; 2],
    pub from: (usize, usize),
}
