//! Structured meshing of a four-cornered face whose sides are not parameter lines (a fillet
//! strip with mitred ends, the wall of a cross hole): a transfinite (Coons) blend of the four
//! sides gives an interior grid in (u, v), joined to the shared edge nodes by zipped strips.
//! A blend that folds (a strongly non-convex boundary) is refused, and the face goes to the
//! constrained Delaunay path.

use solidcraft_geom::Vec3;

/// Smallest turn (radians) of the boundary at a corner, in the tolerance metric.
const CORNER_TURN: f64 = 0.6;

/// One side: boundary vertex indices from one corner to the next, with each node's position
/// along the side (0 to 1, by length in 3D).
struct Side {
    nodes: Vec<u32>,
    at: Vec<f64>,
}

/// The four corners of a single boundary loop, by how sharply the loop turns there in (u, v)
/// scaled by `scale` (one unit per tolerance cell); `None` unless there are exactly four.
fn corners(uv: &[[f64; 2]], scale: [f64; 2]) -> Option<[usize; 4]> {
    let n = uv.len();
    if n < 4 {
        return None;
    }
    let p = |i: usize| uv.get(i % n).map(|q| [q[0] * scale[0], q[1] * scale[1]]);
    let mut found = Vec::new();
    for i in 0..n {
        let (a, b, c) = (p(i + n - 1)?, p(i)?, p(i + 1)?);
        let (d0, d1) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
        let (l0, l1) = (d0[0].hypot(d0[1]), d1[0].hypot(d1[1]));
        // Two copies of a pole meet with no length between them: that is a corner.
        if l0 == 0.0 || l1 == 0.0 {
            found.push(i);
            continue;
        }
        let turn = (d0[0] * d1[1] - d0[1] * d1[0]).atan2(d0[0] * d1[0] + d0[1] * d1[1]).abs();
        if turn > CORNER_TURN {
            found.push(i);
        }
    }
    let [a, b, c, d] = found[..] else { return None };
    Some([a, b, c, d])
}

fn side(fixed: &[Vec3], n: usize, from: usize, to: usize) -> Option<Side> {
    let len = (to + n - from) % n;
    if len == 0 {
        return None;
    }
    let nodes: Vec<u32> = (0..=len).map(|k| ((from + k) % n) as u32).collect();
    let mut at = vec![0.0];
    let mut total = 0.0;
    for w in nodes.windows(2) {
        total += fixed.get(w[0] as usize)?.dist(*fixed.get(w[1] as usize)?);
        at.push(total);
    }
    if !(total > 0.0) {
        // A pole side: evenly along it.
        let m = (nodes.len() - 1).max(1) as f64;
        return Some(Side { at: (0..nodes.len()).map(|k| k as f64 / m).collect(), nodes });
    }
    Some(Side { nodes, at: at.into_iter().map(|x| x / total).collect() })
}

/// The point of a side at position `s` (0 to 1), linear between its nodes.
fn along(side: &Side, uv: &[[f64; 2]], s: f64) -> Option<[f64; 2]> {
    let k = side.at.partition_point(|x| *x < s).clamp(1, side.at.len().saturating_sub(1).max(1));
    let (a0, a1) = (*side.at.get(k - 1)?, *side.at.get(k)?);
    let (p, q) = (uv.get(*side.nodes.get(k - 1)? as usize)?, uv.get(*side.nodes.get(k)? as usize)?);
    let f = if a1 > a0 { ((s - a0) / (a1 - a0)).clamp(0.0, 1.0) } else { 0.0 };
    Some([p[0] + (q[0] - p[0]) * f, p[1] + (q[1] - p[1]) * f])
}

/// Interior line positions for a pair of opposite sides: the sides' own when they have the
/// same nodes (so the grid meets both node for node), else evenly as many as the denser side.
fn lines(a: &Side, b: &Side) -> Vec<f64> {
    let inner = |s: &Side| s.at.get(1..s.at.len().saturating_sub(1)).map(<[f64]>::to_vec).unwrap_or_default();
    if a.at.len() == b.at.len() {
        let l: Vec<f64> = inner(a).iter().zip(inner(b)).map(|(x, y)| 0.5 * (x + y)).collect();
        if !l.is_empty() {
            return l;
        }
    }
    let n = a.at.len().max(b.at.len()).saturating_sub(1).max(2);
    (1..n).map(|i| i as f64 / n as f64).collect()
}

/// Triangulate the loop `uv` (with its fixed 3D nodes) as a Coons patch: boundary vertices
/// keep their indices, the interior grid follows. Triangles counter-clockwise in (u, v).
/// `fine(a, b)`: whether the surface between two parameter points needs no point between them
/// (the grid lines, taken from the sides, are split inside the face where it does not).
pub(crate) fn coons(
    uv: &[[f64; 2]],
    fixed: &[Vec3],
    scale: [f64; 2],
    max_points: usize,
    fine: &dyn Fn([f64; 2], [f64; 2]) -> bool,
) -> Option<(Vec<[f64; 2]>, Vec<[u32; 3]>)> {
    let n = uv.len();
    let [c0, c1, c2, c3] = corners(uv, scale)?;
    // Sides in loop order; the bottom (0) and right (1) run with the loop, the top (2) and
    // left (3) against it, so s and t both run from corner c0.
    let bottom = side(fixed, n, c0, c1)?;
    let right = side(fixed, n, c1, c2)?;
    let top = reversed(side(fixed, n, c2, c3)?);
    let left = reversed(side(fixed, n, c3, c0)?);
    let corner = |i: usize| uv.get(i).copied();
    let (p00, p10, p11, p01) = (corner(c0)?, corner(c1)?, corner(c2)?, corner(c3)?);
    // The blend at (s, t): the sides' points, less the corners counted twice.
    let at = |s: f64, t: f64| -> Option<[f64; 2]> {
        let (b, tp, l, r) = (along(&bottom, uv, s)?, along(&top, uv, s)?, along(&left, uv, t)?, along(&right, uv, t)?);
        let blend = |d: usize| {
            (1.0 - t) * b[d] + t * tp[d] + (1.0 - s) * l[d] + s * r[d]
                - ((1.0 - s) * (1.0 - t) * p00[d] + s * (1.0 - t) * p10[d] + (1.0 - s) * t * p01[d] + s * t * p11[d])
        };
        Some([blend(0), blend(1)])
    };
    let ss = refine(lines(&bottom, &top), max_points, &|a, b, x| Some(fine(at(a, x)?, at(b, x)?)));
    let ts = refine(lines(&left, &right), max_points, &|a, b, x| Some(fine(at(x, a)?, at(x, b)?)));
    if ss.len().checked_mul(ts.len())? > max_points {
        return None;
    }
    let mut out: Vec<[f64; 2]> = uv.to_vec();
    let base = u32::try_from(n).ok()?;
    for &t in &ts {
        for &s in &ss {
            out.push(at(s, t)?);
        }
    }
    let (ns, nt) = (ss.len(), ts.len());
    let g = |i: usize, j: usize| base + (j * ns + i) as u32;
    let mut tris = Vec::new();
    for j in 0..nt.saturating_sub(1) {
        for i in 0..ns.saturating_sub(1) {
            tris.push([g(i, j), g(i + 1, j), g(i + 1, j + 1)]);
            tris.push([g(i, j), g(i + 1, j + 1), g(i, j + 1)]);
        }
    }
    // Strips: each side against its grid row or column, by position along the side. In (s, t)
    // the bottom and right strips come out counter-clockwise, the top and left clockwise.
    let row = |j: usize| (0..ns).map(|i| g(i, j)).collect::<Vec<u32>>();
    let col = |i: usize| (0..nt).map(|j| g(i, j)).collect::<Vec<u32>>();
    zip(&bottom, &row(0), &ss, false, &mut tris)?;
    zip(&right, &col(ns - 1), &ts, false, &mut tris)?;
    zip(&top, &row(nt - 1), &ss, true, &mut tris)?;
    zip(&left, &col(0), &ts, true, &mut tris)?;
    // The (s, t) square maps to (u, v) keeping or reversing orientation, with the loop: all
    // triangles must share one sign, which is then made counter-clockwise.
    let area = |t: &[u32; 3]| -> Option<f64> {
        let (a, b, c) = (out.get(t[0] as usize)?, out.get(t[1] as usize)?, out.get(t[2] as usize)?);
        Some((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]))
    };
    let first = area(tris.first()?)?;
    let sign = if first > 0.0 { 1.0 } else { -1.0 };
    for t in &tris {
        if !(area(t)? * sign > 0.0) {
            return None;
        }
    }
    if sign < 0.0 {
        for t in &mut tris {
            t.swap(1, 2);
        }
    }
    let extra = out.split_off(n);
    Some((extra, tris))
}

/// Interior lines (strictly between 0 and 1) with every interval, ends included, split while
/// `fine(a, b, x)` fails at some position `x` across (or the lines reach `max`).
fn refine(lines: Vec<f64>, max: usize, fine: &dyn Fn(f64, f64, f64) -> Option<bool>) -> Vec<f64> {
    const ACROSS: [f64; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];
    let all: Vec<f64> = std::iter::once(0.0).chain(lines).chain(std::iter::once(1.0)).collect();
    let mut out = vec![0.0];
    let mut stack: Vec<(f64, f64, u32)> = all.windows(2).rev().map(|w| (w[0], w[1], 0)).collect();
    while let Some((a, b, depth)) = stack.pop() {
        let ok = depth >= 16 || out.len() + stack.len() >= max || ACROSS.iter().all(|x| fine(a, b, *x).unwrap_or(true));
        if ok {
            out.push(b);
        } else {
            let m = 0.5 * (a + b);
            stack.push((m, b, depth + 1));
            stack.push((a, m, depth + 1));
        }
    }
    out.into_iter().filter(|x| *x > 0.0 && *x < 1.0).collect()
}

fn reversed(s: Side) -> Side {
    Side { nodes: s.nodes.into_iter().rev().collect(), at: s.at.into_iter().rev().map(|x| 1.0 - x).collect() }
}

/// Join a side (its nodes at positions `side.at`) to a grid row at positions `inner_at`.
fn zip(side: &Side, inner: &[u32], inner_at: &[f64], flip: bool, tris: &mut Vec<[u32; 3]>) -> Option<()> {
    let mut push = |a: u32, b: u32, c: u32| tris.push(if flip { [a, c, b] } else { [a, b, c] });
    let (outer, oat) = (&side.nodes, &side.at);
    let (mut a, mut b) = (0usize, 0usize);
    while a + 1 < outer.len() || b + 1 < inner.len() {
        let advance_outer = if a + 1 >= outer.len() {
            false
        } else if b + 1 >= inner.len() {
            true
        } else {
            oat.get(a + 1)? <= inner_at.get(b + 1)?
        };
        if advance_outer {
            push(*outer.get(a)?, *outer.get(a + 1)?, *inner.get(b)?);
            a += 1;
        } else {
            push(*outer.get(a)?, *inner.get(b + 1)?, *inner.get(b)?);
            b += 1;
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(uv: &[[f64; 2]], tris: &[[u32; 3]]) -> f64 {
        tris.iter()
            .map(|t| {
                let (a, b, c) = (uv[t[0] as usize], uv[t[1] as usize], uv[t[2] as usize]);
                0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]))
            })
            .sum()
    }

    /// A trapezoid (a strip with mitred ends), counter-clockwise, `k` nodes per side.
    fn trapezoid(k: usize) -> Vec<[f64; 2]> {
        let corners = [[0.0, 0.0], [10.0, 0.0], [8.0, 2.0], [2.0, 2.0]];
        let mut b = Vec::new();
        for w in 0..4 {
            let (p, q) = (corners[w], corners[(w + 1) % 4]);
            for i in 0..k {
                let f = i as f64 / k as f64;
                b.push([p[0] + (q[0] - p[0]) * f, p[1] + (q[1] - p[1]) * f]);
            }
        }
        b
    }

    #[test]
    fn trapezoid_is_tiled_exactly() {
        for k in [2, 5] {
            for reverse in [false, true] {
                let mut b = trapezoid(k);
                if reverse {
                    b.reverse();
                }
                let fixed: Vec<Vec3> = b.iter().map(|p| Vec3::new(p[0], p[1], 0.0)).collect();
                let Some((extra, tris)) = coons(&b, &fixed, [1.0, 1.0], 10_000, &|_, _| true) else { panic!("meshes (k {k})") };
                let mut uv = b.clone();
                uv.extend(extra);
                assert!((area(&uv, &tris) - 16.0).abs() < 1e-9, "k {k} reverse {reverse}: {}", area(&uv, &tris));
            }
        }
    }

    #[test]
    fn five_corners_is_not_a_coons_patch() {
        let b = [[0.0, 0.0], [1.0, 0.0], [1.0, 0.5], [0.5, 1.0], [0.0, 1.0]];
        let fixed: Vec<Vec3> = b.iter().map(|p| Vec3::new(p[0], p[1], 0.0)).collect();
        assert!(coons(&b, &fixed, [1.0, 1.0], 100, &|_, _| true).is_none());
    }
}
