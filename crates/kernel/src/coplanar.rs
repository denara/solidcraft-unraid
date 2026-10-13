//! Booleans of bodies with coincident planar faces, by moving faces apart first.
//!
//! The mesh-based intersection cannot handle faces that lie in the same plane, nor the edges
//! lying in them. A face whose neighbours stand perpendicular to it (any extruded or
//! box-like part) can be pushed along its normal exactly, by rebuilding it and stretching or
//! shrinking its neighbours. Then:
//! - cut and intersect: the tool's coincident face moves out of the way (outward when the
//!   bodies are on the same side of the plane, away from the target when they touch back to
//!   back), which leaves the result unchanged;
//! - union of faces facing the same way: both faces move outward by different amounts, the
//!   union is taken, and the slab beyond the plane is cut off (now a transversal cut);
//! - union of faces back to back: the tool's face moves into the target, which overlaps it.

use std::collections::HashMap;

use solidcraft_geom::{Aabb3, Vec3};
use truck_modeling::{self as mt, builder};

use crate::body::shell_groups::boxes_separate;
use crate::body::{Body, Solid, from_p3, p3, v3};
use crate::history::{Change, EdgeOrigin, FaceOrigin, FaceProv, History};
use crate::{BoolOp, KernelError, Result, guard, record};

#[derive(Clone, Copy, Debug)]
struct PlaneFace {
    n: Vec3,
    d: f64,
}

fn planes(b: &Body) -> Vec<(usize, PlaneFace)> {
    b.solid
        .face_iter()
        .enumerate()
        .filter_map(|(i, f)| match f.oriented_surface() {
            mt::Surface::Plane(p) => {
                let n = p.normal();
                let n = Vec3::new(n.x, n.y, n.z).normalized()?;
                Some((i, PlaneFace { n, d: n.dot(from_p3(p.origin())) }))
            }
            _ => None,
        })
        .collect()
}

/// Push the planar faces of `b` lying in the plane (n, d) — n their outward normal — along n
/// by `delta` (negative: inward). Their neighbours must stand perpendicular to the plane. The
/// result's history is over `[b]`.
pub(crate) fn push_faces(b: &Body, n: Vec3, d: f64, delta: f64) -> Result<Body> {
    // The exact rebuild, unless it came out unsound (a curved side it couldn't re-sweep).
    match push_faces_exact(b, n, d, delta)
        .and_then(|r| if r.validity().is_empty() { Ok(r) } else { Err(KernelError::Failed("pushing a face: an unsound result".into())) })
    {
        Ok(r) => Ok(r),
        // Faces of planes and cylinders: every face follows (any neighbours).
        Err(e) => {
            let size = b.size();
            let tol = (size * 1e-7).max(1e-9) * 10.0;
            let planes: Vec<bool> = b
                .solid
                .face_iter()
                .map(|f| match f.oriented_surface() {
                    mt::Surface::Plane(p) => {
                        let m = p.normal();
                        let m = Vec3::new(m.x, m.y, m.z);
                        m.dot(n) > 1.0 - 1e-9 && (n.dot(from_p3(p.origin())) - d).abs() < tol
                    }
                    _ => false,
                })
                .collect();
            crate::offset::offset_body(b, |i, _| if planes.get(i).copied().unwrap_or(false) { delta } else { 0.0 }).map_err(|_| e)
        }
    }
}

pub(crate) fn push_faces_exact(b: &Body, n: Vec3, d: f64, delta: f64) -> Result<Body> {
    let size = b.size();
    let tol = (size * 1e-7).max(1e-9);
    let fail = |m: &str| KernelError::Failed(format!("pushing a face: {m}"));
    guard("push face", || {
        let solid = b.deep_copy();
        // The copy's edges are b's, in order; the faces come out in place, one for one.
        let mut edge_origins = record::edge_origins(&solid, 0);
        let mut faces_from = Vec::new();
        let shift = n * delta;
        let mat = mt::Matrix4::from_translation(v3(shift));
        let mut shells = Vec::new();
        for sh in solid.boundaries() {
            let faces: Vec<mt::Face> = sh.face_iter().cloned().collect();
            let on_plane = |f: &mt::Face| match f.oriented_surface() {
                mt::Surface::Plane(p) => {
                    let m = p.normal();
                    let m = Vec3::new(m.x, m.y, m.z);
                    m.dot(n) > 1.0 - 1e-9 && (n.dot(from_p3(p.origin())) - d).abs() < tol * 10.0
                }
                _ => false,
            };
            let moving: Vec<usize> = (0..faces.len()).filter(|i| faces.get(*i).is_some_and(on_plane)).collect();
            let first = faces_from.len();
            if moving.is_empty() {
                faces_from.extend((first..first + faces.len()).map(|i| record::from_face(0, i, Change::Kept)));
                shells.push(sh.clone());
                continue;
            }
            // Vertices and edges of the moving faces.
            let mut vmap: HashMap<mt::VertexID, mt::Vertex> = HashMap::new();
            let mut emap: HashMap<mt::EdgeID, mt::Edge> = HashMap::new();
            for i in &moving {
                let f = faces.get(*i).ok_or_else(|| fail("face"))?;
                for v in f.vertex_iter() {
                    vmap.entry(v.id()).or_insert_with(|| builder::vertex(p3(from_p3(v.point()) + shift)));
                }
            }
            for i in &moving {
                let f = faces.get(*i).ok_or_else(|| fail("face"))?;
                for e in f.edge_iter() {
                    if emap.contains_key(&e.id()) {
                        continue;
                    }
                    let (a, bb) = (
                        vmap.get(&e.absolute_front().id()).ok_or_else(|| fail("vertex"))?,
                        vmap.get(&e.absolute_back().id()).ok_or_else(|| fail("vertex"))?,
                    );
                    let curve = mt::Transformed::transformed(&e.curve(), mat);
                    emap.insert(e.id(), mt::Edge::new(a, bb, curve));
                }
            }
            // Edges leaving the plane at moved vertices: straight, along n.
            for f in &faces {
                for e in f.edge_iter() {
                    if emap.contains_key(&e.id()) {
                        continue;
                    }
                    let (fa, fb) = (e.absolute_front(), e.absolute_back());
                    let (ma, mb) = (vmap.get(&fa.id()), vmap.get(&fb.id()));
                    if ma.is_none() && mb.is_none() {
                        continue;
                    }
                    if !matches!(e.curve(), mt::Curve::Line(_)) {
                        return Err(fail("a neighbouring edge is not straight"));
                    }
                    let dir = (from_p3(fb.point()) - from_p3(fa.point())).normalized().ok_or_else(|| fail("zero edge"))?;
                    if dir.cross(n).len() > 1e-9 {
                        return Err(fail("a neighbouring face is not perpendicular"));
                    }
                    let (na, nb) = (ma.cloned().unwrap_or_else(|| fa.clone()), mb.cloned().unwrap_or_else(|| fb.clone()));
                    if (from_p3(nb.point()) - from_p3(na.point())).dot(dir) <= tol {
                        return Err(fail("the face would pass its opposite side"));
                    }
                    emap.insert(e.id(), builder::line(&na, &nb));
                }
            }
            let rebuild_wires = |f: &mt::Face| -> Vec<mt::Wire> {
                f.absolute_boundaries()
                    .iter()
                    .map(|w| {
                        w.edge_iter()
                            .map(|e| match emap.get(&e.id()) {
                                Some(ne) => {
                                    if e.front() == e.absolute_front() {
                                        ne.clone()
                                    } else {
                                        ne.inverse()
                                    }
                                }
                                None => e.clone(),
                            })
                            .collect::<Vec<_>>()
                            .into()
                    })
                    .collect()
            };
            // Each moved or stretched edge stands for the one it replaces.
            for (old, new) in &emap {
                if let Some(o) = edge_origins.get(old).cloned() {
                    edge_origins.insert(new.id(), o);
                }
            }
            use mt::{ParametricSurface3D, SearchNearestParameter};
            let mut out: Vec<mt::Face> = Vec::new();
            for (i, f) in faces.iter().enumerate() {
                let touches = f.edge_iter().any(|e| emap.contains_key(&e.id()));
                let change = if touches { Change::Modified } else { Change::Kept };
                faces_from.push(record::from_face(0, first + i, change));
                if !touches {
                    out.push(f.clone());
                    continue;
                }
                let wires = rebuild_wires(f);
                let surface = if moving.contains(&i) {
                    mt::Transformed::transformed(&f.surface(), mat)
                } else if matches!(f.surface(), mt::Surface::Plane(_)) {
                    f.surface()
                } else {
                    // A curved side: sweep its edge in the plane along the new height.
                    let base = f.edge_iter().find_map(|e| {
                        emap.get(&e.id()).filter(|_| {
                            let (a, bb) = (from_p3(e.absolute_front().point()), from_p3(e.absolute_back().point()));
                            (n.dot(a) - d).abs() < tol * 10.0 && (n.dot(bb) - d).abs() < tol * 10.0
                        })
                    });
                    let base = base.ok_or_else(|| fail("a curved side without an edge in the plane"))?;
                    let height = f.vertex_iter().map(|v| n.dot(from_p3(v.point())) - d).fold(0.0f64, |m, x| if x.abs() > m.abs() { x } else { m });
                    let swept: mt::Face = builder::tsweep(base, v3(n * (height - delta)));
                    let mut s = swept.surface();
                    // Same normal as the old side at a point both share.
                    let probe = f.vertex_iter().map(|v| from_p3(v.point())).fold(Vec3::ZERO, |a, x| a + x) / f.vertex_iter().count().max(1) as f64;
                    let old = f.surface();
                    if let (Some((u0, v0)), Some((u1, v1))) =
                        (old.search_nearest_parameter(p3(probe), None, 100), s.search_nearest_parameter(p3(probe), None, 100))
                    {
                        let (a, bb) = (old.normal(u0, v0), s.normal(u1, v1));
                        if mt::InnerSpace::dot(a, bb) < 0.0 {
                            s = mt::Invertible::inverse(&s);
                        }
                    }
                    s
                };
                let mut nf = mt::Face::try_new(wires, surface).map_err(|e| fail(&e.to_string()))?;
                if !f.orientation() {
                    nf.invert();
                }
                out.push(nf);
            }
            shells.push(out.into());
        }
        let s = Solid::try_new(shells).map_err(|e| fail(&e.to_string()))?;
        record::build(s, &record::counts(&[b]), Some(faces_from), Some(&edge_origins))
    })
}

/// A box covering everything beyond the plane (n, d) by up to `depth`.
fn slab_beyond(n: Vec3, d: f64, depth: f64, extent: f64) -> Result<Body> {
    let u = n.any_perp();
    let w = n.cross(u);
    let o = n * d;
    let pl = solidcraft_geom::Plane::new(o, u, w).ok_or_else(|| KernelError::Failed("slab plane".into()))?;
    let sq = solidcraft_geom::Loop2::polygon(&[
        solidcraft_geom::Vec2::new(-extent, -extent),
        solidcraft_geom::Vec2::new(extent, -extent),
        solidcraft_geom::Vec2::new(extent, extent),
        solidcraft_geom::Vec2::new(-extent, extent),
    ]);
    let r = solidcraft_geom::Region2 { outer: sq, holes: vec![] };
    crate::extrude(&pl, &[r], 0.0, depth)?.pop().ok_or_else(|| KernelError::Failed("slab".into()))
}

/// The box of a face's boundary sampled to within `tol` of each curve: the face, planar, lies
/// within `tol` of it. `None` when the boundary can't be sampled.
fn face_box(f: &mt::Face, tol: f64) -> Option<Aabb3> {
    use mt::{BoundedCurve, ParameterDivision1D};
    const MAX_POINTS: usize = 100_000;
    guard("face box", || {
        let mut bb = Aabb3::EMPTY;
        let mut count = 0usize;
        for e in f.edge_iter() {
            let points = match e.curve() {
                mt::Curve::Line(_) => vec![e.front().point(), e.back().point()],
                // The division checks its deviation at midpoints only (as in blend.rs).
                c => c.parameter_division(c.range_tuple(), tol * 0.25).1,
            };
            count = count.saturating_add(points.len());
            if count > MAX_POINTS {
                return Err(KernelError::Failed("face box: too many boundary points".into()));
            }
            for p in points {
                bb.add(from_p3(p));
            }
        }
        if bb.is_empty() {
            return Err(KernelError::Failed("face box: no boundary".into()));
        }
        Ok(bb)
    })
    .ok()
}

/// The planes (normal, offset) in which a planar face of `a` and one of `b` coincide, as `a`'s
/// face lies in them, each with whether `b`'s face points the same way. Faces in a common plane but apart from
/// each other (the tops of two posts standing on one floor) don't coincide: they need no
/// pushing apart. A face whose extent can't be found counts as meeting every face in its plane.
pub(crate) fn coincident_planes(a: &Body, b: &Body) -> Vec<(Vec3, f64, bool)> {
    let size = a.size().max(b.size());
    let tol = (size * 1e-7).max(1e-9) * 10.0;
    let sample = (size * 1e-4).max(tol);
    let (fa, fb): (Vec<&mt::Face>, Vec<&mt::Face>) = (a.solid.face_iter().collect(), b.solid.face_iter().collect());
    let (pa, pb) = (planes(a), planes(b));
    let (mut boxes_a, mut boxes_b): (Vec<Option<Option<Aabb3>>>, Vec<Option<Option<Aabb3>>>) = (vec![None; fa.len()], vec![None; fb.len()]);
    let mut found: Vec<(Vec3, f64, bool)> = Vec::new();
    for (ia, x) in &pa {
        for (ib, y) in &pb {
            let cos = x.n.dot(y.n);
            if cos.abs() <= 1.0 - 1e-9 || (x.d - y.d * cos).abs() >= tol {
                continue;
            }
            let same = cos > 0.0;
            if found.iter().any(|(n, d, s)| *s == same && n.dot(x.n) > 1.0 - 1e-9 && (d - x.d).abs() < tol) {
                continue;
            }
            let ba = boxes_a.get_mut(*ia).and_then(|b| *b.get_or_insert_with(|| fa.get(*ia).and_then(|f| face_box(f, sample))));
            let bb = boxes_b.get_mut(*ib).and_then(|b| *b.get_or_insert_with(|| fb.get(*ib).and_then(|f| face_box(f, sample))));
            // Each box is within `sample` of its face.
            if !ba.zip(bb).is_some_and(|(p, q)| boxes_separate(p, q, 2.0 * sample)) {
                found.push((x.n, x.d, same));
            }
        }
    }
    found
}

/// Retry a failed boolean with coincident planar faces pushed apart. `None` when there are no
/// coincident faces or the faces can't be pushed.
pub(crate) fn boolean_apart(a: &Body, b: &Body, op: BoolOp) -> Option<Result<Option<Body>>> {
    let size = a.size().max(b.size());
    let tol = (size * 1e-7).max(1e-9);
    let coincident = coincident_planes(a, b);
    if coincident.is_empty() {
        return None;
    }
    let same: Vec<(Vec3, f64)> = coincident.iter().filter(|(_, _, s)| *s).map(|(n, d, _)| (*n, *d)).collect();
    let back: Vec<(Vec3, f64)> = coincident.iter().filter(|(_, _, s)| !*s).map(|(n, d, _)| (*n, *d)).collect();
    let (d1, d2) = (size * 0.0137, size * 0.0229);
    let counts = record::counts(&[a, b]);
    let run = || -> Result<Option<Body>> {
        let (mut a2, mut b2) = (a.clone(), b.clone());
        // What the pushed bodies are made of: their histories over [a] and [b].
        let (mut ha, mut hb) = (crate::boolean_record::kept(a, 0, 1), crate::boolean_record::kept(b, 0, 1));
        for (n, d) in &same {
            // b's face points along n too.
            b2 = push_faces(&b2, *n, *d, d2)?;
            hb = crate::boolean_record::through(&b2, 0, hb.as_ref());
            if op == BoolOp::Union {
                a2 = push_faces(&a2, *n, *d, d1)?;
                ha = crate::boolean_record::through(&a2, 0, ha.as_ref());
            }
        }
        for (n, d) in &back {
            // a's face points along n; b's face points along −n at the same plane (−d).
            let shift = if op == BoolOp::Union && crate::ops::narrowing_bores() {
                // Pushed into `a`, b's face must have a's material behind it all over and that
                // deep: over a hole in a's face (a narrowed bore's rim over the other body's
                // bore) it would fill the hole, through a thin wall it would stick out the far
                // side. Only checked inside `bore_union`, which then tries its other way; a
                // general check turns some joins that come out slightly wrong today (a pad
                // half past an edge) into failures, until such faces can be split first.
                [d2, d2 * 0.25, d2 * 0.0625]
                    .into_iter()
                    .find(|depth| backed(a, b, -*n, -*d, *depth))
                    .ok_or_else(|| KernelError::Failed("pushing a face: part of it lies over a hole or past the edge of the other body".into()))?
            } else if op == BoolOp::Union {
                d2
            } else {
                -d2
            };
            b2 = push_faces(&b2, -*n, -*d, shift)?;
            hb = crate::boolean_record::through(&b2, 0, hb.as_ref());
        }
        let Some(mut r) = crate::ops::boolean(&a2, &b2, op)? else { return Ok(None) };
        // The pushed bodies' histories as over [a, b], a's and b's in their places.
        let (ha, hb) = (crate::boolean_record::placed(ha, 0, 2), crate::boolean_record::placed(hb, 1, 2));
        let mut h = crate::boolean_record::of_pair(&r, &ha, &hb);
        if op == BoolOp::Union {
            for (n, d) in &same {
                let slab = slab_beyond(*n, *d, d1.max(d2) * 3.0, size * 4.0)?;
                match crate::ops::boolean(&r, &slab, BoolOp::Cut)? {
                    Some(x) => {
                        h = h.and_then(|h| trimmed(&x, h, &slab, [a, b], *n, *d, tol));
                        r = x;
                    }
                    None => return Ok(None),
                }
            }
        }
        Ok(Some(r.with_checked_history(h, &counts)))
    };
    Some(run())
}

/// The history over `[a, b]` of `x`, the union `r` of `a` and `b` (with history `h` over them),
/// pushed out past the plane (n, d), with the slab beyond it (`slab`, whose start cap lies in
/// the plane) cut off. The slab's cap in `x` is where the operands' faces in that plane were
/// (all of them, so only a cap that stays one face says which), and where it meets a side of
/// an operand is that side's edge along one of those faces. `None` where that is not so.
fn trimmed(x: &Body, h: History, slab: &Body, operands: [&Body; 2], n: Vec3, d: f64, tol: f64) -> Option<History> {
    let mut cut = x.history()?.clone();
    cut.substitute(0, &h).ok()?;
    let cap = slab.history()?.faces.iter().position(|f| f.origins == [FaceOrigin::Cap { end: false }])?;
    let cap = FaceOrigin::Face { operand: 2, face: cap };
    // The operands' faces in the plane, facing along n, and their edges with each other face.
    let in_plane: Vec<Vec<usize>> = operands
        .iter()
        .map(|o| planes(o).into_iter().filter(|(_, p)| p.n.dot(n) > 1.0 - 1e-9 && (p.d - d).abs() < tol * 10.0).map(|(i, _)| i).collect())
        .collect();
    let bounds: Vec<Vec<Vec<usize>>> = operands.iter().map(|o| crate::history::edge_faces(&o.solid)).collect();
    if cut.faces.iter().filter(|f| f.origins.contains(&cap)).count() != 1 {
        return None;
    }
    for f in &mut cut.faces {
        if f.origins.contains(&cap) {
            let mut origins: std::collections::BTreeSet<FaceOrigin> = f.origins.iter().copied().filter(|o| *o != cap).collect();
            for (operand, faces) in in_plane.iter().enumerate() {
                origins.extend(faces.iter().map(|face| FaceOrigin::Face { operand, face: *face }));
            }
            f.change = if origins.len() > 1 { Change::Merged } else { Change::Modified };
            f.origins = origins.into_iter().collect();
        }
    }
    let cap = crate::history::Side::from(cap);
    for e in &mut cut.edges {
        for o in &mut e.origins {
            let side = match o {
                EdgeOrigin::Meet { a, b } if *a == cap => b,
                EdgeOrigin::Meet { a, b } if *b == cap => a,
                _ => continue,
            };
            let Some(FaceOrigin::Face { operand, face }) = side.single() else { return None };
            let (plane_faces, bounds) = (in_plane.get(operand)?, bounds.get(operand)?);
            let mut along = (0..bounds.len())
                .filter(|k| bounds.get(*k).is_some_and(|fs| fs.len() == 2 && fs.contains(&face) && fs.iter().any(|f| plane_faces.contains(f))));
            let (Some(edge), None) = (along.next(), along.next()) else { return None };
            *o = EdgeOrigin::Edge { operand, edge };
        }
    }
    cut.renumber(&[Some(0), Some(1), None], 2).ok()?;
    Some(cut)
}

/// Has `a` material behind every part of b's faces in the plane (m, d) (m their outward
/// normal, pointing into `a`), to `depth`? Checked at points inside the faces' triangles.
fn backed(a: &Body, b: &Body, m: Vec3, d: f64, depth: f64) -> bool {
    let size = a.size().max(b.size());
    let tol = (size * 1e-7).max(1e-9) * 10.0;
    let faces: Vec<usize> = planes(b).into_iter().filter(|(_, p)| p.n.dot(m) > 1.0 - 1e-9 && (p.d - d).abs() < tol).map(|(i, _)| i).collect();
    let (Ok(ma), Ok(mb)) = (a.tessellate_with(size * 1e-3, true), b.tessellate_with(size * 1e-3, true)) else { return false };
    let inside = ma.inside_index();
    let mut points = Vec::new();
    for (t, fi) in mb.triangles.iter().zip(&mb.tri_face) {
        if !faces.contains(&(*fi as usize)) {
            continue;
        }
        let Some([p, q, r]) = mb.tri(t) else { continue };
        let c = (p + q + r) / 3.0;
        points.extend([c, c.lerp(p, 0.8), c.lerp(q, 0.8), c.lerp(r, 0.8)]);
        if points.len() > 4000 {
            break;
        }
    }
    !points.is_empty() && points.iter().all(|p| [0.05, 0.5, 1.0].iter().all(|k| inside.contains(*p + m * (depth * k))))
}

/// Union of bodies that touch along one identical planar face, back to back (a body joined
/// with its mirror image): the two faces go and the shells are stitched along their edges.
/// `None` when no such pair of faces exists.
pub(crate) fn glue(a: &Body, b: &Body) -> Option<Result<Body>> {
    use mt::{BoundedCurve, ParametricCurve};
    let size = a.size().max(b.size());
    let tol = (size * 1e-7).max(1e-9) * 10.0;
    let key = |p: Vec3| ((p.x / tol).round() as i64, (p.y / tol).round() as i64, (p.z / tol).round() as i64);
    let mid = |e: &mt::Edge| {
        let c = e.curve();
        let (t0, t1) = c.range_tuple();
        from_p3(c.subs((t0 + t1) * 0.5))
    };
    // Matching faces: same plane, opposite normals, the same edges.
    let (pa, pb) = (planes(a), planes(b));
    let fa_list: Vec<&mt::Face> = a.solid.face_iter().collect();
    let fb_list: Vec<&mt::Face> = b.solid.face_iter().collect();
    for (ia, x) in &pa {
        for (ib, y) in &pb {
            if x.n.dot(y.n) > -1.0 + 1e-9 || (x.d + y.d).abs() > tol {
                continue;
            }
            let (Some(fa), Some(fb)) = (fa_list.get(*ia), fb_list.get(*ib)) else { continue };
            let ea: Vec<mt::Edge> = fa.edge_iter().collect();
            let eb: Vec<mt::Edge> = fb.edge_iter().collect();
            if ea.len() != eb.len() {
                continue;
            }
            // B edge → the matching A edge.
            let mut map: HashMap<mt::EdgeID, mt::Edge> = HashMap::new();
            let mut vmap: HashMap<mt::VertexID, mt::Vertex> = HashMap::new();
            let mut ok = true;
            for e in &eb {
                let ends = (key(from_p3(e.absolute_front().point())), key(from_p3(e.absolute_back().point())));
                let m = key(mid(e));
                let hit = ea.iter().find(|f| {
                    let fe = (key(from_p3(f.absolute_front().point())), key(from_p3(f.absolute_back().point())));
                    (fe == ends || fe == (ends.1, ends.0)) && key(mid(f)) == m
                });
                let Some(f) = hit else {
                    ok = false;
                    break;
                };
                let same = key(from_p3(f.absolute_front().point())) == ends.0;
                map.insert(e.id(), if same { f.absolute_clone() } else { f.absolute_clone().inverse() });
                let (fa_v, fb_v) = if same {
                    (f.absolute_front().clone(), f.absolute_back().clone())
                } else {
                    (f.absolute_back().clone(), f.absolute_front().clone())
                };
                vmap.insert(e.absolute_front().id(), fa_v);
                vmap.insert(e.absolute_back().id(), fb_v);
            }
            if !ok {
                continue;
            }
            return Some(guard("glue", || {
                let fail = |m: &str| KernelError::Failed(format!("joining touching faces: {m}"));
                // B's other edges, rebuilt on A's vertices where they meet the seam.
                let mut edges: HashMap<mt::EdgeID, mt::Edge> = map.clone();
                for e in b.solid.edge_iter() {
                    if edges.contains_key(&e.id()) {
                        continue;
                    }
                    let (f0, f1) = (e.absolute_front(), e.absolute_back());
                    if !vmap.contains_key(&f0.id()) && !vmap.contains_key(&f1.id()) {
                        edges.insert(e.id(), e.absolute_clone());
                        continue;
                    }
                    let v0 = vmap.get(&f0.id()).cloned().unwrap_or_else(|| f0.clone());
                    let v1 = vmap.get(&f1.id()).cloned().unwrap_or_else(|| f1.clone());
                    edges.insert(e.id(), mt::Edge::new(&v0, &v1, e.curve()));
                }
                let mut faces: Vec<mt::Face> = fa_list.iter().enumerate().filter(|(i, _)| i != ia).map(|(_, f)| (*f).clone()).collect();
                // Every face but the two glued is kept; an edge along the seam is an edge of both.
                let kept = |operand: usize, n: usize, skip: usize| {
                    (0..n).filter(move |i| *i != skip).map(move |i| record::from_face(operand, i, Change::Kept))
                };
                let from: Vec<FaceProv> = kept(0, fa_list.len(), *ia).chain(kept(1, fb_list.len(), *ib)).collect();
                let mut origins = record::edge_origins(&a.solid, 0);
                for (old, o) in record::edge_origins(&b.solid, 1) {
                    if let Some(new) = edges.get(&old) {
                        origins.entry(new.id()).or_default().extend(o);
                    }
                }
                for (i, f) in fb_list.iter().enumerate() {
                    if i == *ib {
                        continue;
                    }
                    let wires: Vec<mt::Wire> = f
                        .absolute_boundaries()
                        .iter()
                        .map(|w| {
                            w.edge_iter()
                                .filter_map(|e| edges.get(&e.id()).map(|ne| if e.front() == e.absolute_front() { ne.clone() } else { ne.inverse() }))
                                .collect::<Vec<_>>()
                                .into()
                        })
                        .collect();
                    let mut nf = mt::Face::try_new(wires, f.surface()).map_err(|e| fail(&e.to_string()))?;
                    if !f.orientation() {
                        nf.invert();
                    }
                    faces.push(nf);
                }
                let solid = Solid::try_new(vec![faces.into()]).map_err(|e| fail(&e.to_string()))?;
                // Each edge's origins in the solid's edge order.
                let origins = crate::body::edge_order(solid.face_iter()).0.iter().map(|e| origins.get(&e.id()).cloned()).collect();
                crate::boolean_record::heal_build(solid, size, &[], Some(from), origins, &record::counts(&[a, b]))
            }));
        }
    }
    None
}

/// A cylindrical face: its index, a point on the axis, the unit axis and the radius.
#[derive(Clone, Copy, Debug)]
struct CylFace {
    index: usize,
    origin: Vec3,
    axis: Vec3,
    radius: f64,
}

fn cylinders(b: &Body) -> Vec<CylFace> {
    (0..b.face_count())
        .filter_map(|index| match crate::face_surface(b, index)? {
            crate::FaceSurface::Cylinder { origin, axis, radius } => Some(CylFace { index, origin, axis, radius }),
            _ => None,
        })
        .collect()
}

/// Do the two cylinders lie on the same surface?
fn same_cylinder(x: &CylFace, y: &CylFace, tol: f64) -> bool {
    let off = y.origin - x.origin;
    x.axis.dot(y.axis).abs() > 1.0 - 1e-9 && (x.radius - y.radius).abs() < tol && (off - x.axis * off.dot(x.axis)).len() < tol
}

/// The history over `[a, b]` of `r`, made in [`bore_union`]: `joined` (the target joined with
/// `narrowed`, the tool with its bore narrowed) less `cut`, the cylinder whose wall restores the
/// tool's bore faces `bore` (the tool is operand `tool_operand`). Nothing of the cylinder but its
/// wall may be left. Only a bore of one face: the cylinder's wall faces are split where its own
/// construction splits them, so which of several bore faces a piece of it lies over is not known.
fn recut(r: &Body, joined: &Body, narrowed: &Body, cut: &Body, bore: &[usize], tool_operand: usize) -> Option<History> {
    if bore.len() != 1 {
        return None;
    }
    // Over [target, tool].
    let joined = crate::boolean_record::through(joined, 1, narrowed.history())?;
    let wall = || record::region(bore.iter().map(|&face| FaceOrigin::Face { operand: 1, face }).collect(), false);
    let faces = cut
        .history()?
        .faces
        .iter()
        .map(|f| match f.origins.first() {
            Some(FaceOrigin::Profile { .. }) => wall(),
            _ => record::generated(Vec::new()),
        })
        .collect();
    let cut = record::from_faces(&cut.solid, 2, faces);
    let h = crate::boolean_record::of_pair(r, &Some(joined), &Some(cut))?;
    let map = if tool_operand == 1 { [Some(0), Some(1)] } else { [Some(1), Some(0)] };
    crate::boolean_record::composed(h.renumbered(&map, 2))
}

/// Do the bodies have cylindrical faces on a common cylinder?
pub(crate) fn has_coincident_cylinders(a: &Body, b: &Body) -> bool {
    let tol = (a.size().max(b.size()) * 1e-7).max(1e-9) * 10.0;
    let cb = cylinders(b);
    cylinders(a).iter().any(|x| cb.iter().any(|y| same_cylinder(x, y, tol)))
}

/// Union of bodies with bores on a common cylinder (a drain pad's bore around the drain hole
/// it is joined over). The mesh intersection can't take the coincident walls, so the tool's
/// bore is narrowed (its walls pushed toward the axis, now clear of the target's), the bodies
/// are joined, and the cylinder of the original bore is cut out again over the tool's length.
/// Recutting is valid only where that cylinder holds no material of either body. The
/// samples are plausibility checks; a whole-boundary guard rejects additional boundaries
/// crossing the recut. Original common-bore trims retain the legacy eligibility policy.
/// This is not a general emptiness or Boolean-equivalence certificate. Each direction
/// returns a result only when `accept` takes it; otherwise this returns `None`. The result's
/// history is over `[a, b]`.
pub(crate) fn bore_union(a: &Body, b: &Body, accept: &dyn Fn(&Body) -> bool) -> Option<Body> {
    let size = a.size().max(b.size());
    let tol = (size * 1e-7).max(1e-9) * 10.0;
    let mtol = size * 2e-3;
    let (ma, mb) = (a.tessellate_with(mtol, true).ok()?, b.tessellate_with(mtol, true).ok()?);
    let (ia, ib) = (ma.inside_index(), mb.inside_index());
    let empty = |p: Vec3| !ia.contains(p) && !ib.contains(p);
    let (ca, cb) = (cylinders(a), cylinders(b));
    // (body to narrow, its operand number, its mesh and inside test, the other body, their faces
    // on the cylinder).
    let ways = [(b, 1, &mb, &ib, a, &cb, &ca), (a, 0, &ma, &ia, b, &ca, &cb)];
    for (tool, tool_operand, mesh, inside, target, ct, co) in ways {
        for c in ct {
            if !co.iter().any(|o| same_cylinder(c, o, tol)) {
                continue;
            }
            let bore: Vec<usize> = ct.iter().filter(|x| same_cylinder(c, x, tol)).map(|x| x.index).collect();
            // The tool's extent along the axis on that cylinder, and whether it is a bore (the
            // material outside it) at a point of the face.
            let (mut t0, mut t1) = (f64::INFINITY, f64::NEG_INFINITY);
            let mut probe = None;
            for (t, fi) in mesh.triangles.iter().zip(&mesh.tri_face) {
                if !bore.contains(&(*fi as usize)) {
                    continue;
                }
                let Some(ps) = mesh.tri(t) else { continue };
                for p in ps {
                    let s = (p - c.origin).dot(c.axis);
                    t0 = t0.min(s);
                    t1 = t1.max(s);
                }
                if probe.is_none() {
                    probe = Some((ps[0] + ps[1] + ps[2]) / 3.0);
                }
            }
            let Some(q) = probe else { continue };
            if !(t1 - t0 > tol && c.radius > tol) {
                continue;
            }
            let along = c.origin + c.axis * (q - c.origin).dot(c.axis);
            let Some(radial) = (q - along).normalized() else { continue };
            let on = along + radial * c.radius;
            let e = (c.radius * 0.05).max(size * 1e-4);
            if !inside.contains(on + radial * e) || inside.contains(on - radial * e) {
                continue;
            }
            // The cylinder over the tool's length holds no material of either body.
            let (u, w) = (c.axis.any_perp(), c.axis.cross(c.axis.any_perp()));
            let at = |s: f64, r: f64, ang: f64| c.origin + c.axis * s + (u * ang.cos() + w * ang.sin()) * r;
            let clear = (1..=9).all(|i| {
                let s = t0 + (t1 - t0) * f64::from(i) / 10.0;
                empty(at(s, 0.0, 0.0))
                    && (0..8).all(|k| {
                        let ang = f64::from(k) * std::f64::consts::FRAC_PI_4 + 0.3;
                        empty(at(s, c.radius * 0.5, ang)) && empty(at(s, c.radius * 0.9, ang))
                    })
            });
            if !clear {
                continue;
            }
            // Past an end where the space is empty well beyond the radius too, the cut runs on a
            // little (so its cap doesn't lie in a face's plane). Not where the other body's bore
            // goes on: the cut's side would lie on its wall.
            let ext = size * 0.01;
            let past = |s: f64| {
                empty(at(s, 0.0, 0.0))
                    && (0..8).all(|k| {
                        let ang = f64::from(k) * std::f64::consts::FRAC_PI_4 + 0.3;
                        empty(at(s, c.radius * 0.9, ang)) && empty(at(s, c.radius * 1.1, ang)) && empty(at(s, c.radius * 1.5, ang))
                    })
            };
            let lo = if past(t0 - ext * 0.5) && past(t0 - ext) { t0 - ext } else { t0 };
            let hi = if past(t1 + ext * 0.5) && past(t1 + ext) { t1 + ext } else { t1 };
            // The samples above are plausibility checks, not proof that an
            // arbitrarily thin web is absent. Every additional current boundary
            // must be wholly clear of the recut; unsupported trims refuse.
            let other_bore: Vec<usize> = co.iter().filter(|x| same_cylinder(c, x, tol)).map(|x| x.index).collect();
            if crate::bore_guard::boundaries_clear(tool, &bore, c.origin, c.axis, c.radius, lo, hi, tol).is_none()
                || crate::bore_guard::boundaries_clear(target, &other_bore, c.origin, c.axis, c.radius, lo, hi, tol).is_none()
            {
                continue;
            }
            let d = (c.radius * 0.2).min(size * 0.01);
            let run = || -> Result<Option<Body>> {
                let narrowed = crate::offset::offset_body(tool, |i, _| if bore.contains(&i) { d } else { 0.0 })?;
                if !narrowed.validity().is_empty() {
                    return Err(KernelError::Failed("narrowing a bore: an unsound result".into()));
                }
                let Some(joined) = crate::ops::boolean(target, &narrowed, BoolOp::Union)? else { return Ok(None) };
                let cut = crate::build::cylinder(at(lo, 0.0, 0.0), c.axis, c.radius, hi - lo)?;
                let Some(r) = crate::ops::boolean(&joined, &cut, BoolOp::Cut)? else { return Ok(None) };
                let h = recut(&r, &joined, &narrowed, &cut, &bore, tool_operand);
                Ok(Some(r.with_checked_history(h, &record::counts(&[a, b]))))
            };
            let (result, diagnostics) = crate::capture_boolean_diagnostics(run);
            match result {
                Ok(Some(r)) if accept(&r) => {
                    crate::boolean_diagnostics::merge(diagnostics);
                    let mut event = crate::BooleanFallback::new(crate::BooleanStrategy::CommonBore);
                    event.face_shift_mm = d;
                    crate::boolean_diagnostics::record(event);
                    return Some(r);
                }
                _ => continue,
            }
        }
    }
    None
}
