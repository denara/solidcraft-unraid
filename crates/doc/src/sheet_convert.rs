//! Convert to Sheet Metal with bend edges: the faces of a solid, or the walls of a shelled one,
//! become one sheet.
//!
//! The planar faces reached from the fixed face across the bend edges are the sheet's panels,
//! all on one skin of it (the one the fixed face is on). A body with a parallel face the same
//! small distance behind the fixed face and behind the faces at every bend is a shell: it keeps
//! its walls and their thickness. Any other body's faces become the outside of the sheet, with
//! the rule's thickness taken into the body. A bend edge stays its bend's virtual sharp: the
//! panels on either side are set back from it by (R + T)·tan(θ/2) where the skin is the bend's
//! outside and R·tan(θ/2) where it is the inside, so the body keeps its envelope. An edge between
//! two panels that is no bend is cut open: each panel ends square to the sheet, half the rule's
//! gap from the plane halfway between the two.
//!
//! The result is the model every sheet metal feature builds ([`SheetBody`]): the fixed face is
//! the base panel and every other panel a flange on its parent's edge, its outline taken through
//! the inverse of its fold. Unfold, the flat pattern, the bend table and cuts then work as on a
//! sheet made from flanges.
//!
//! Not supported yet: curved faces (rolled sheet, a filleted edge as a bend), faces with curved
//! edges or holes, loops of bends, two faces that meet along more than one edge.

use std::collections::BTreeMap;

use solidcraft_geom::{Loop2, Mesh, Plane, Region2, Vec2, Vec3};
use solidcraft_kernel::{Body, FaceSurface, face_surface};

use super::{RuleValues, SheetBody, SheetFlange, base_flange, polys_cross, signed_area, snap};
use crate::{DocError, ModelBody, Result, apply_point, mat_inverse};

const MAX_BENDS: usize = 1000;

fn err<T>(msg: impl Into<String>) -> Result<T> {
    Err(DocError::Invalid(msg.into()))
}

fn at(p: Vec3) -> String {
    format!("({:.3}, {:.3}, {:.3})", p.x, p.y, p.z)
}

/// A planar face of the body: its outward normal and its outline, counter-clockwise about the
/// normal. Edge `i` runs from corner `i` to the next one, along face `across[i]`.
struct Face {
    n: Vec3,
    pts: Vec<Vec3>,
    across: Vec<Option<usize>>,
}

/// A face of the sheet: the panel it hangs off (none for the fixed face), and the bend between
/// the two: its angle (positive toward the material, as a flange's) and its setback on this skin.
struct Panel {
    face: Face,
    parent: Option<usize>,
    angle: f64,
    setback: f64,
}

/// Area of a loop in space, positive when it runs counter-clockwise about `n`.
fn area_about(pts: &[Vec3], n: Vec3) -> f64 {
    let Some(&o) = pts.first() else { return 0.0 };
    pts.windows(2).map(|w| (w[0] - o).cross(w[1] - o).dot(n)).sum::<f64>() / 2.0
}

/// Turn a loop's per-edge values round with its corners (edge `i` starts at corner `i`).
fn turn<T>(per_edge: &mut [T]) {
    per_edge.reverse();
    if !per_edge.is_empty() {
        per_edge.rotate_left(1);
    }
}

/// Face `f` of the body as a panel outline. Only a planar face bounded by one loop of straight
/// edges can be one.
fn outline(b: &Body, mesh: &Mesh, f: usize, tol: f64) -> Result<Face> {
    let Some(FaceSurface::Plane { normal: n, .. }) = face_surface(b, f) else {
        return err("not supported yet: curved faces in a converted sheet (only planar faces convert)");
    };
    // Its edges, each with the face on its other side.
    let mut edges = Vec::new();
    for (line, faces) in mesh.edges.iter().zip(&mesh.edge_faces) {
        if !faces.iter().any(|g| *g as usize == f) {
            continue;
        }
        let (Some(&p), Some(&q)) = (line.first(), line.last()) else { continue };
        if p.dist(q) <= tol || line.iter().any(|x| x.dist_to_segment(p, q) > tol) {
            return err("not supported yet: faces with curved edges in a converted sheet");
        }
        edges.push((p, q, faces.iter().map(|g| *g as usize).find(|g| *g != f)));
    }
    // Chained head to tail from the first one, they close one loop.
    let Some((start, mut end, across)) = edges.pop() else { return err("a face of the body has no edges") };
    let (mut pts, mut tags) = (vec![start], vec![across]);
    while end.dist(start) > tol {
        let Some(k) = edges.iter().position(|e| e.0.dist(end) <= tol || e.1.dist(end) <= tol) else {
            return err("the outline of a face of the body does not close");
        };
        let (p, q, across) = edges.swap_remove(k);
        pts.push(end);
        tags.push(across);
        end = if p.dist(end) <= tol { q } else { p };
    }
    if !edges.is_empty() {
        return err("not supported yet: faces with holes in a converted sheet (cut them after converting)");
    }
    // A straight run along one neighbour is one edge, however the body's edges divide it (a
    // shell's outer corners are divided at the height of its floor).
    let m = pts.len();
    let corner = |i: usize| -> bool {
        let before = (i + m - 1) % m;
        let (Some(&a), Some(&p), Some(&b)) = (pts.get(before), pts.get(i), pts.get((i + 1) % m)) else { return true };
        tags.get(before) != tags.get(i) || p.dist_to_segment(a, b) > tol
    };
    let keep: Vec<bool> = (0..m).map(corner).collect();
    let (mut pts, mut tags): (Vec<Vec3>, Vec<Option<usize>>) = pts.into_iter().zip(tags).zip(keep).filter(|(_, kept)| *kept).map(|(x, _)| x).unzip();
    if area_about(&pts, n) < 0.0 {
        pts.reverse();
        turn(&mut tags);
    }
    Ok(Face { n, pts, across: tags })
}

/// The face straight through the material behind planar face `f`, when it is parallel to it,
/// and how far behind it lies.
fn behind(b: &Body, mesh: &Mesh, f: usize) -> Option<(usize, f64)> {
    let Some(FaceSurface::Plane { normal: n, point: o }) = face_surface(b, f) else { return None };
    // From the middle of the face's largest triangle.
    let area = |t: &[Vec3; 3]| (t[1] - t[0]).cross(t[2] - t[0]).len();
    let tri = mesh.triangles.iter().zip(&mesh.tri_face).filter(|(_, g)| **g as usize == f).filter_map(|(t, _)| mesh.tri(t));
    let [p, q, r] = tri.max_by(|x, y| area(x).total_cmp(&area(y)))?;
    let (_, hit) = mesh.raycast((p + q + r) * (1.0 / 3.0), n * -1.0)?;
    let g = *mesh.tri_face.get(hit)? as usize;
    let Some(FaceSurface::Plane { normal, point }) = face_surface(b, g) else { return None };
    (normal.dot(n) < -1.0 + 1e-9).then_some((g, (o - point).dot(n)))
}

/// How far a panel's edge along a rip moves in, so that the panel, cut square to the sheet,
/// stays half the gap from the plane halfway between it and the panel `y` across the rip.
/// `e` is the edge's direction round panel `x`.
fn rip_cut(x: &Face, y: &Face, e: Vec3, t: f64, gap: f64) -> Result<f64> {
    // Into each panel from the edge, square to the edge.
    let (wx, wy) = (x.n.cross(e), e.cross(y.n));
    let mid = (wx - wy).normalized().filter(|m| wx.dot(*m) > 1e-9);
    let Some(mid) = mid else { return err("two faces of the sheet lie on each other along the edge between them") };
    // The sheet's other face lies `t` behind the panel: nearer that plane, or further from it.
    let lean = -x.n.dot(mid) * t;
    Ok((gap / 2.0 - lean.min(0.0)) / wx.dot(mid))
}

/// A counter-clockwise outline with each edge moved in by its own amount. Moved edges meet
/// where their lines cross; two edges in one line that move by different amounts are joined by
/// a square step. Returns the corners and, per edge, the edge it was (none for a step).
fn inset(pts: &[Vec2], by: &[f64]) -> Result<(Vec<Vec2>, Vec<Option<usize>>)> {
    let n = pts.len();
    if n < 3 {
        return err("a face of the sheet has no outline");
    }
    let moved = |i: usize| -> Option<(Vec2, Vec2)> {
        let (a, b) = (*pts.get(i % n)?, *pts.get((i + 1) % n)?);
        let d = (b - a).normalized()?;
        Some((a + Vec2::new(-d.y, d.x) * *by.get(i % n)?, d))
    };
    let (mut out, mut from) = (Vec::new(), Vec::new());
    for i in 0..n {
        let (Some((p0, d0)), Some((p1, d1))) = (moved(i + n - 1), moved(i)) else { return err("a face of the sheet has an edge without length") };
        let cross = d0.cross(d1);
        if cross.abs() > 1e-9 {
            out.push(p0 + d0 * ((p1 - p0).cross(d1) / cross));
        } else if d0.dot(d1) > 0.0 {
            let end = p0 + d0 * (p1 - p0).dot(d0);
            if end.dist(p1) > 1e-9 {
                out.push(end);
                from.push(None);
            }
            out.push(p1);
        } else {
            return err("the outline of a face of the sheet doubles back on itself");
        }
        from.push(Some(i));
    }
    // Every edge still runs the way it did, and the outline is still one simple loop.
    let m = out.len();
    let kept = (0..m).all(|j| match (from.get(j).copied().flatten().and_then(moved), out.get(j), out.get((j + 1) % m)) {
        (Some((_, d)), Some(&a), Some(&b)) => (b - a).dot(d) > 1e-9,
        _ => true,
    });
    if !kept || !(signed_area(&out) > 0.0) || polys_cross(&out, &out) {
        return err("the bends' setbacks and the gaps at the cut edges use up an edge of a face");
    }
    Ok((out, from))
}

/// Give flange `i` its panel: `outline` in flat coordinates, counter-clockwise, its edge at the
/// bend first trimmed to the end of the bend zone.
fn set_panel(sh: &mut SheetBody, i: usize, outline: &[Vec2]) {
    let ba = sh.flanges.get(i).map_or(0.0, |f| sh.allowance(f));
    let Some(f) = sh.flanges.get_mut(i) else { return };
    // Its corners along the bend (u) and past the bend zone (v); those at the zone exactly on it.
    let uv: Vec<(f64, f64)> =
        outline.iter().map(|q| ((*q - f.p).dot(f.d), (*q - f.p).dot(f.n) - ba)).map(|(u, v)| (u, if v.abs() < 1e-6 { 0.0 } else { v })).collect();
    f.leg = uv.iter().fold(0.0, |m, (_, v)| m.max(*v));
    let level = |h: f64| -> Vec<f64> { uv.iter().filter(|(_, v)| (v - h).abs() < 1e-6).map(|(u, _)| *u).collect() };
    match (level(0.0).as_slice(), level(f.leg).as_slice()) {
        // Two corners at the bend and two on a far edge parallel to it: a flange's own panel,
        // with its run-ons past the bend's ends and its slanted ends.
        (&[a, b], &[c, d]) if uv.len() == 4 && f.leg > 1e-6 => {
            let zero = |x: f64| if x.abs() < 1e-12 { 0.0 } else { x };
            f.ext = (zero(-a.min(b)), zero(a.max(b) - f.len));
            f.slope = (zero((a.min(b) - c.min(d)) / f.leg), zero((c.max(d) - a.max(b)) / f.leg));
        }
        _ => f.shape = Some(uv.iter().map(|(u, v)| f.p + f.d * *u + f.n * (ba + *v)).collect()),
    }
}

/// Names of the edges of `body` at the bend points, to find them again after the body changes.
/// A point on no edge gets none: the conversion then says so, instead of bending the edge that
/// happens to be nearest.
pub fn bend_edge_names(st: &crate::ModelState, body: &str, pts: &[Vec3]) -> Vec<String> {
    let Some(b) = st.body(body) else { return Vec::new() };
    let (mesh, near) = (b.mesh(), (b.body.size() * 2e-3).max(1e-3));
    let on = |p: &Vec3| mesh.edges.iter().any(|e| e.windows(2).any(|w| p.dist_to_segment(w[0], w[1]) <= near));
    pts.iter().zip(crate::naming::edge_names_at(b, pts)).map(|(p, name)| if on(p) { name } else { String::new() }).collect()
}

/// Convert body `mb` to a sheet under the rule `rule` with values `rv`: `face` is a point on the
/// fixed face and `bends` are points on the sharp edges that become bends (the body's
/// coordinates, mm). See the module docs.
pub fn convert_body(mb: &ModelBody, rule: &str, rv: &RuleValues, face: Vec3, bends: &[Vec3]) -> Result<SheetBody> {
    if bends.is_empty() || bends.len() > MAX_BENDS {
        return err(format!("pick 1…{MAX_BENDS} bend edges"));
    }
    if !(rv.gap >= 0.0 && rv.bend_radius >= 0.0) {
        return err("the rule's gap and bend radius must not be negative");
    }
    let (b, mesh) = (&mb.body, mb.mesh());
    let size = b.size();
    if !(size.is_finite() && size > 0.0) {
        return err("the body has no size");
    }
    // Points closer than `tol` are one corner; a pick lies within `near` of what it picks.
    let (tol, near) = ((size * 1e-6).max(1e-6), (size * 2e-3).max(1e-3));
    let Some((_, f0)) = crate::appearance::nearest_face(mb, face).filter(|(d, _)| *d <= near) else {
        return err(format!("no face of the body at the fixed face point {}", at(face)));
    };
    let fixed = outline(b, &mesh, f0, tol).map_err(|e| DocError::Invalid(format!("the fixed face can't be a face of the sheet: {e}")))?;
    // The two faces along each bend edge.
    let mut picks = Vec::with_capacity(bends.len());
    for p in bends {
        let dist = |e: &Vec<Vec3>| e.windows(2).map(|w| p.dist_to_segment(w[0], w[1])).fold(f64::INFINITY, f64::min);
        let edge = mesh.edges.iter().zip(&mesh.edge_faces).map(|(e, faces)| (dist(e), faces)).min_by(|x, y| x.0.total_cmp(&y.0));
        match edge.filter(|(d, _)| *d <= near).map(|(_, faces)| faces.as_slice()) {
            Some(&[x, y]) if x != y => picks.push((*p, (x as usize, y as usize))),
            Some(_) => return err(format!("the edge at the bend point {} does not lie between two faces", at(*p))),
            None => return err(format!("no edge of the body at the bend point {}", at(*p))),
        }
    }
    // A shell: the same small distance of material behind the fixed face and the faces at every
    // bend. It keeps its thickness, and a bend picked on one skin is that bend on the other too.
    let thin = behind(b, &mesh, f0).map(|(_, d)| d).filter(|d| *d > 0.0 && d * d * 16.0 <= area_about(&fixed.pts, fixed.n));
    let wall = |f: usize, d: f64| behind(b, &mesh, f).filter(|(_, e)| (e - d).abs() <= 1e-6 * d.max(1.0)).map(|(g, _)| g);
    let backs: Vec<Option<(usize, usize)>> = picks.iter().map(|(_, (x, y))| thin.and_then(|d| Some((wall(*x, d)?, wall(*y, d)?)))).collect();
    let shell = thin.filter(|_| backs.iter().all(Option::is_some));
    let t = shell.map_or(rv.thickness, |d| (d * 1e9).round() / 1e9);
    let r = rv.bend_radius;
    let joined = |x: usize, y: usize| mesh.edge_faces.iter().any(|faces| faces.contains(&(x as u32)) && faces.contains(&(y as u32)));
    let mut todo: Vec<(Vec3, Vec<(usize, usize)>)> = Vec::with_capacity(picks.len());
    for (&(p, pair), back) in picks.iter().zip(&backs) {
        let other = back.filter(|(x, y)| shell.is_some() && joined(*x, *y));
        todo.push((p, std::iter::once(pair).chain(other).collect()));
    }
    // The panels: the faces reached from the fixed face across the bends, each hanging off the
    // face it was reached from.
    let mut panels = BTreeMap::from([(f0, Panel { face: fixed, parent: None, angle: 0.0, setback: 0.0 })]);
    let mut order = vec![f0];
    loop {
        let waiting = todo.len();
        let mut rest = Vec::new();
        for (p, pairs) in todo {
            let reached = pairs.iter().find(|(x, y)| panels.contains_key(x) || panels.contains_key(y));
            let Some(&(x, y)) = reached else {
                rest.push((p, pairs));
                continue;
            };
            let (from, to) = if panels.contains_key(&x) { (x, y) } else { (y, x) };
            if let Some(seen) = panels.get(&to) {
                // The same bend picked twice is one bend; any other edge between two panels
                // would close a ring of bends, which no flat sheet folds into.
                if seen.parent == Some(from) || panels.get(&from).is_some_and(|k| k.parent == Some(to)) {
                    continue;
                }
                return err(format!("the bend at {} closes a loop of bends (leave one of its edges unpicked: it is cut open)", at(p)));
            }
            let child = outline(b, &mesh, to, tol)
                .map_err(|e| DocError::Invalid(format!("the face across the bend at {} can't be a face of the sheet: {e}", at(p))))?;
            let Some(parent) = panels.get(&from) else { continue };
            let theta = parent.face.n.dot(child.n).clamp(-1.0, 1.0).acos();
            if !(theta > 1e-6 && theta < std::f64::consts::PI - 1e-6) {
                return err(format!("the faces at the bend at {} must meet at an angle between 0 and 180°", at(p)));
            }
            // Up: the child rises on the material side of its parent, which is behind its face.
            let o = parent.face.pts.first().copied().unwrap_or_default();
            let lift = child.pts.iter().map(|q| (*q - o).dot(parent.face.n)).fold(0.0, |m: f64, h| if h.abs() > m.abs() { h } else { m });
            let (angle, sharp) = if lift < 0.0 { (theta, r + t) } else { (-theta, r) };
            panels.insert(to, Panel { face: child, parent: Some(from), angle, setback: sharp * (theta / 2.0).tan() });
            order.push(to);
        }
        todo = rest;
        if todo.is_empty() || todo.len() == waiting {
            break;
        }
    }
    if let Some((p, _)) = todo.first() {
        return err(format!("the bend at {} does not connect to the fixed face through the other bends", at(*p)));
    }
    // The flat frame: on the fixed face, x along its longest edge, z into the material.
    let Some(base) = panels.get(&f0) else { return err("fixed face") };
    let corners = base.face.pts.iter().zip(base.face.pts.iter().cycle().skip(1));
    let longest = corners.max_by(|x, y| x.0.dist(*x.1).total_cmp(&y.0.dist(*y.1)));
    let frame = longest
        .and_then(|(a, b)| Some((*a, (*b - *a).normalized()?)))
        .and_then(|(a, x)| Plane::new(a, x, x.cross(base.face.n)))
        .ok_or_else(|| DocError::Invalid("the fixed face has no plane".into()))?;
    let mut sh = base_flange(&mb.name, rule, &RuleValues { thickness: t, ..*rv }, frame, Region2::default());
    // Lay the panels out, parents first: each is its face, seen through the inverse of its
    // fold, with its edges moved in by a bend's setback or a rip's cut.
    let mut queue: Vec<(usize, Option<usize>)> = vec![(f0, None)];
    let mut next = 0;
    while let Some(&(pf, fl)) = queue.get(next) {
        next += 1;
        let Some(panel) = panels.get(&pf) else { continue };
        let inv = mat_inverse(&sh.world(fl)).ok_or_else(|| DocError::Invalid("sheet panel frame".into()))?;
        let n = panel.face.pts.len();
        let (mut pts, mut by) = (Vec::with_capacity(n), Vec::with_capacity(n));
        for (i, p) in panel.face.pts.iter().enumerate() {
            let q = apply_point(&inv, *p);
            if !(q.z.abs() <= tol) {
                return err(format!("the face at {} does not lie where its bend puts it ({:.6} mm off)", at(*p), q.z));
            }
            pts.push(Vec2::new(q.x, q.y));
            let e = panel.face.pts.get((i + 1) % n).and_then(|b| (*b - *p).normalized()).unwrap_or(Vec3::X);
            by.push(match panel.face.across.get(i).copied().flatten().and_then(|g| Some((g, panels.get(&g)?))) {
                Some((g, _)) if panel.parent == Some(g) => panel.setback,
                Some((_, other)) if other.parent == Some(pf) => other.setback,
                Some((_, other)) => rip_cut(&panel.face, &other.face, e, t, rv.gap)?,
                None => 0.0,
            });
        }
        let mut across = panel.face.across.clone();
        if signed_area(&pts) < 0.0 {
            pts.reverse();
            turn(&mut by);
            turn(&mut across);
        }
        let first = panel.face.pts.first().copied().unwrap_or_default();
        let (trimmed, from) = inset(&pts, &by).map_err(|e| DocError::Invalid(format!("{e} (the face at {})", at(first))))?;
        match fl {
            None => sh.base.outer = Loop2::polygon(&trimmed.iter().map(|q| snap(*q)).collect::<Vec<_>>()),
            Some(i) => set_panel(&mut sh, i, &trimmed),
        }
        // Its children: flanges on the trimmed edges they bend at.
        let m = trimmed.len();
        for &g in &order {
            let Some(child) = panels.get(&g).filter(|c| c.parent == Some(pf)) else { continue };
            let runs: Vec<usize> =
                (0..m).filter(|j| from.get(*j).copied().flatten().and_then(|i| across.get(i).copied().flatten()) == Some(g)).collect();
            let (a, b) = match runs.as_slice() {
                &[j] => (trimmed.get(j).copied().map(snap), trimmed.get((j + 1) % m).copied().map(snap)),
                _ => return err("not supported yet: two faces of the sheet that meet along more than one edge"),
            };
            let (Some(a), Some(b)) = (a, b) else { continue };
            let Some(d) = (b - a).normalized() else { return err("a bend edge has no length") };
            sh.flanges.push(SheetFlange {
                parent: fl,
                p: a,
                d,
                len: a.dist(b),
                n: Vec2::new(d.y, -d.x),
                angle: child.angle,
                radius: r,
                leg: 0.0,
                ext: (0.0, 0.0),
                slope: (0.0, 0.0),
                shape: None,
                tag: String::new(),
            });
            queue.push((g, Some(sh.flanges.len() - 1)));
        }
    }
    Ok(sh)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_move_in_by_their_own_amounts_and_step_along_one_line() {
        let v = Vec2::new;
        let square = [v(0.0, 0.0), v(10.0, 0.0), v(10.0, 10.0), v(0.0, 10.0)];
        let (p, from) = inset(&square, &[1.0, 0.0, 2.0, 0.0]).unwrap();
        assert_eq!(p, vec![v(0.0, 1.0), v(10.0, 1.0), v(10.0, 8.0), v(0.0, 8.0)]);
        assert_eq!(from, vec![Some(0), Some(1), Some(2), Some(3)]);
        // Two edges in one line that move by different amounts: a square step between them,
        // so 5 x 2 goes from the 10 x 10.
        let split = [v(0.0, 0.0), v(5.0, 0.0), v(10.0, 0.0), v(10.0, 10.0), v(0.0, 10.0)];
        let (p, from) = inset(&split, &[0.0, 2.0, 0.0, 0.0, 0.0]).unwrap();
        assert_eq!(p, vec![v(0.0, 0.0), v(5.0, 0.0), v(5.0, 2.0), v(10.0, 2.0), v(10.0, 10.0), v(0.0, 10.0)]);
        assert_eq!(from, vec![Some(0), None, Some(1), Some(2), Some(3), Some(4)]);
        // A slanted end keeps its line: the corner slides along it (2 up a 45° edge is 2 along).
        let slanted = [v(0.0, 0.0), v(10.0, 0.0), v(15.0, 5.0), v(0.0, 5.0)];
        let (p, _) = inset(&slanted, &[2.0, 0.0, 0.0, 0.0]).unwrap();
        assert!(p[1].dist(v(12.0, 2.0)) < 1e-12 && p[0].dist(v(0.0, 2.0)) < 1e-12, "{p:?}");
        // Setbacks longer than the face leave nothing of it.
        assert!(inset(&square, &[6.0, 0.0, 6.0, 0.0]).is_err());
        assert!(inset(&square, &[f64::NAN, 0.0, 0.0, 0.0]).is_err());
    }
}
