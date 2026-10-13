//! Geometry comparison helpers: an STL reader, bounding boxes, volume, centre of mass and principal
//! second moments of a closed triangle mesh, and a sampled symmetric Hausdorff distance between two
//! triangle meshes.

use std::collections::HashMap;

/// A triangle soup in mm.
pub type Tri = [[f64; 3]; 3];

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// Read a binary (or ASCII) STL file.
pub fn read_stl(path: &str) -> Result<Vec<Tri>, String> {
    let b = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    if b.len() >= 84 {
        let n = u32::from_le_bytes([b[80], b[81], b[82], b[83]]) as usize;
        if b.len() == 84 + 50 * n {
            let f = |o: usize| f64::from(f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]));
            return Ok((0..n)
                .map(|i| {
                    let o = 84 + 50 * i + 12;
                    let v = |k: usize| [f(o + 12 * k), f(o + 12 * k + 4), f(o + 12 * k + 8)];
                    [v(0), v(1), v(2)]
                })
                .collect());
        }
    }
    let text = String::from_utf8_lossy(&b);
    let mut verts = Vec::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        if it.next() == Some("vertex") {
            let c: Vec<f64> = it.filter_map(|x| x.parse().ok()).collect();
            if c.len() == 3 {
                verts.push([c[0], c[1], c[2]]);
            }
        }
    }
    if verts.is_empty() || verts.len() % 3 != 0 {
        return Err(format!("{path}: not an STL file"));
    }
    Ok(verts.chunks(3).map(|c| [c[0], c[1], c[2]]).collect())
}

/// Volume, centre of mass and the principal second moments of volume (mm^5, sorted ascending) of a
/// closed, outward-oriented triangle soup, by signed tetrahedra from the origin.
pub fn mass_properties(tris: &[Tri]) -> (f64, [f64; 3], [f64; 3]) {
    let (mut v6, mut m1) = (0.0, [0.0; 3]);
    let mut c = [[0.0; 3]; 3]; // ∫ x xᵀ dV
    for t in tris {
        let d = dot(t[0], cross(t[1], t[2]));
        v6 += d;
        let s = add(add(t[0], t[1]), t[2]);
        m1 = add(m1, scale(s, d / 24.0));
        for (i, row) in c.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                let vv: f64 = t.iter().map(|p| p[i] * p[j]).sum::<f64>() + s[i] * s[j];
                *cell += d / 120.0 * vv;
            }
        }
    }
    let vol = v6 / 6.0;
    if vol.abs() < 1e-12 {
        return (0.0, [0.0; 3], [0.0; 3]);
    }
    let com = scale(m1, 1.0 / vol);
    // Central covariance and inertia tensor I = tr(C)·1 − C.
    let mut cc = c;
    for (i, row) in cc.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell -= vol * com[i] * com[j];
        }
    }
    let tr = cc[0][0] + cc[1][1] + cc[2][2];
    let mut inertia = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            inertia[i][j] = if i == j { tr - cc[i][j] } else { -cc[i][j] };
        }
    }
    let mut ev = eigen_sym3(inertia);
    ev.sort_by(f64::total_cmp);
    (vol, com, ev)
}

/// Eigenvalues of a symmetric 3×3 matrix (Jacobi rotations).
fn eigen_sym3(mut a: [[f64; 3]; 3]) -> [f64; 3] {
    for _ in 0..50 {
        let (mut p, mut q, mut big) = (0, 1, 0.0);
        for (i, j) in [(0, 1), (0, 2), (1, 2)] {
            if a[i][j].abs() > big {
                (p, q, big) = (i, j, a[i][j].abs());
            }
        }
        if big <= 1e-15 * (a[0][0].abs() + a[1][1].abs() + a[2][2].abs()).max(1e-300) {
            break;
        }
        let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
        let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
        let t = if theta == 0.0 { 1.0 } else { t };
        let (cs, sn) = (1.0 / (t * t + 1.0).sqrt(), t / (t * t + 1.0).sqrt());
        let mut b = a;
        for k in 0..3 {
            b[k][p] = cs * a[k][p] - sn * a[k][q];
            b[k][q] = sn * a[k][p] + cs * a[k][q];
        }
        let mut c2 = b;
        for k in 0..3 {
            c2[p][k] = cs * b[p][k] - sn * b[q][k];
            c2[q][k] = sn * b[p][k] + cs * b[q][k];
        }
        a = c2;
    }
    [a[0][0], a[1][1], a[2][2]]
}

/// Axis-aligned bounds of a triangle soup.
pub fn bounds(tris: &[Tri]) -> ([f64; 3], [f64; 3]) {
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in tris.iter().flatten() {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

/// Distance from a point to a triangle (Ericson, Real-Time Collision Detection 5.1.5).
fn point_tri(p: [f64; 3], t: &Tri) -> f64 {
    let (a, b, c) = (t[0], t[1], t[2]);
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    let q = if d1 <= 0.0 && d2 <= 0.0 {
        a
    } else {
        let bp = sub(p, b);
        let (d3, d4) = (dot(ab, bp), dot(ac, bp));
        if d3 >= 0.0 && d4 <= d3 {
            b
        } else {
            let vc = d1 * d4 - d3 * d2;
            if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
                add(a, scale(ab, d1 / (d1 - d3)))
            } else {
                let cp = sub(p, c);
                let (d5, d6) = (dot(ab, cp), dot(ac, cp));
                if d6 >= 0.0 && d5 <= d6 {
                    c
                } else {
                    let vb = d5 * d2 - d1 * d6;
                    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
                        add(a, scale(ac, d2 / (d2 - d6)))
                    } else {
                        let va = d3 * d6 - d5 * d4;
                        if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
                            add(b, scale(sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6))))
                        } else {
                            let den = va + vb + vc;
                            if den.abs() < 1e-300 { a } else { add(a, add(scale(ab, vb / den), scale(ac, vc / den))) }
                        }
                    }
                }
            }
        }
    };
    let d = sub(p, q);
    dot(d, d).sqrt()
}

/// Uniform grid of triangles for distance queries up to `cap`.
struct Grid<'a> {
    tris: &'a [Tri],
    cell: f64,
    cells: HashMap<(i64, i64, i64), Vec<u32>>,
}

impl<'a> Grid<'a> {
    fn new(tris: &'a [Tri], cell: f64) -> Self {
        let mut cells: HashMap<(i64, i64, i64), Vec<u32>> = HashMap::new();
        let key = |x: f64| (x / cell).floor() as i64;
        for (i, t) in tris.iter().enumerate() {
            let (lo, hi) = bounds(std::slice::from_ref(t));
            for x in key(lo[0])..=key(hi[0]) {
                for y in key(lo[1])..=key(hi[1]) {
                    for z in key(lo[2])..=key(hi[2]) {
                        cells.entry((x, y, z)).or_default().push(i as u32);
                    }
                }
            }
        }
        Grid { tris, cell, cells }
    }

    /// Distance from `p` to the nearest triangle, or `cap` if none is that close. Searches shells
    /// of cells outwards and stops once the nearest hit is closer than the unsearched shells.
    fn distance(&self, p: [f64; 3], cap: f64, seen: &mut [u32], stamp: u32) -> f64 {
        let rmax = (cap / self.cell).ceil() as i64;
        let k = |x: f64| (x / self.cell).floor() as i64;
        let (cx, cy, cz) = (k(p[0]), k(p[1]), k(p[2]));
        let mut best = cap;
        for r in 0..=rmax {
            for x in cx - r..=cx + r {
                for y in cy - r..=cy + r {
                    for z in cz - r..=cz + r {
                        // only the shell at Chebyshev distance r
                        if (x - cx).abs() != r && (y - cy).abs() != r && (z - cz).abs() != r {
                            continue;
                        }
                        let Some(list) = self.cells.get(&(x, y, z)) else { continue };
                        for &i in list {
                            let Some(slot) = seen.get_mut(i as usize) else { continue };
                            if *slot == stamp {
                                continue;
                            }
                            *slot = stamp;
                            if let Some(t) = self.tris.get(i as usize) {
                                best = best.min(point_tri(p, t));
                            }
                        }
                    }
                }
            }
            // Everything not yet searched is at least r·cell away from p.
            if best <= r as f64 * self.cell {
                break;
            }
        }
        best
    }
}

/// At most this many sample points per side (larger meshes are sampled with an even stride).
const MAX_SAMPLES: usize = 250_000;

/// Sample points of a triangle soup: its vertices and triangle centroids (every k-th triangle for
/// very large meshes).
fn samples(tris: &[Tri]) -> Vec<[f64; 3]> {
    let stride = (tris.len() * 4).div_ceil(MAX_SAMPLES).max(1);
    let mut out: Vec<[f64; 3]> = Vec::with_capacity(tris.len() * 4 / stride + 4);
    for t in tris.iter().step_by(stride) {
        out.extend_from_slice(t);
        out.push(scale(add(add(t[0], t[1]), t[2]), 1.0 / 3.0));
    }
    out
}

/// One-sided sampled distance from `from` to `to`: (max, mean, worst sample point). Distances are
/// capped at `cap`.
fn one_sided(from: &[Tri], to: &[Tri], cap: f64) -> (f64, f64, [f64; 3]) {
    let (lo, hi) = bounds(to);
    let diag = dot(sub(hi, lo), sub(hi, lo)).sqrt();
    // Cells about twice the typical triangle size (so a cell holds a handful of triangles), but no
    // finer than diag/400 (so big flat triangles don't fill too many cells).
    let area: f64 = to.iter().map(|t| dot(cross(sub(t[1], t[0]), sub(t[2], t[0])), cross(sub(t[1], t[0]), sub(t[2], t[0]))).sqrt() / 2.0).sum();
    let typical = (area / to.len().max(1) as f64).sqrt();
    let grid = Grid::new(to, (2.0 * typical).max(diag / 400.0).max(1e-3));
    let mut seen = vec![0u32; to.len()];
    let (mut max, mut sum, mut worst, mut n) = (0.0f64, 0.0, [0.0; 3], 0usize);
    for (i, p) in samples(from).into_iter().enumerate() {
        let d = grid.distance(p, cap, &mut seen, i as u32 + 1);
        sum += d;
        n += 1;
        if d > max {
            max = d;
            worst = p;
        }
    }
    (max, if n > 0 { sum / n as f64 } else { 0.0 }, worst)
}

/// Sampled symmetric Hausdorff distance between two triangle soups (distances capped at `cap`).
pub struct Hausdorff {
    pub a_to_b: f64,
    pub b_to_a: f64,
    pub mean: f64,
    /// Where the larger one-sided distance was found (mm, world).
    pub worst_at: [f64; 3],
}

pub fn hausdorff(a_mesh: &[Tri], b_mesh: &[Tri], cap: f64) -> Hausdorff {
    let (a, am, ap) = one_sided(a_mesh, b_mesh, cap);
    let (b, bm, bp) = one_sided(b_mesh, a_mesh, cap);
    Hausdorff { a_to_b: a, b_to_a: b, mean: (am + bm) / 2.0, worst_at: if a >= b { ap } else { bp } }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(s: f64, o: [f64; 3]) -> Vec<Tri> {
        let v = |x: f64, y: f64, z: f64| [o[0] + x * s, o[1] + y * s, o[2] + z * s];
        let q = |a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]| vec![[a, b, c], [a, c, d]];
        let mut t = Vec::new();
        t.extend(q(v(0., 0., 0.), v(0., 1., 0.), v(1., 1., 0.), v(1., 0., 0.)));
        t.extend(q(v(0., 0., 1.), v(1., 0., 1.), v(1., 1., 1.), v(0., 1., 1.)));
        t.extend(q(v(0., 0., 0.), v(1., 0., 0.), v(1., 0., 1.), v(0., 0., 1.)));
        t.extend(q(v(0., 1., 0.), v(0., 1., 1.), v(1., 1., 1.), v(1., 1., 0.)));
        t.extend(q(v(0., 0., 0.), v(0., 0., 1.), v(0., 1., 1.), v(0., 1., 0.)));
        t.extend(q(v(1., 0., 0.), v(1., 1., 0.), v(1., 1., 1.), v(1., 0., 1.)));
        t
    }

    #[test]
    fn box_moments_and_distance() {
        let c = cube(10.0, [5.0, -3.0, 2.0]);
        let (v, com, pm) = mass_properties(&c);
        assert!((v - 1000.0).abs() < 1e-9 && (com[0] - 10.0).abs() < 1e-9 && (com[2] - 7.0).abs() < 1e-9);
        // V·(a²+b²)/12 for a cube of side 10: 1000·200/12.
        for m in pm {
            assert!((m - 1000.0 * 200.0 / 12.0).abs() < 1e-6, "{pm:?}");
        }
        let moved = cube(10.0, [5.0, -3.0, 2.3]);
        let h = hausdorff(&c, &moved, 1.0);
        assert!((h.a_to_b - 0.3).abs() < 1e-9 && (h.b_to_a - 0.3).abs() < 1e-9);
        let same = hausdorff(&c, &c, 1.0);
        assert!(same.a_to_b < 1e-12);
    }
}
