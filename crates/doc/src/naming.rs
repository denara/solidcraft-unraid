//! Persistent names of faces and edges.
//!
//! A fillet stores the edges it rounds, a sketch the face it sits on: by name, so that an
//! upstream edit that adds, removes, splits or reorders faces does not move them to another
//! face. Every face of an evaluated body is named after how it was made: a face that came from a
//! face of the feature's input keeps that face's name; a face new to the feature is named by its
//! role (`F<feature>:side:<sketch curve>`, `F<feature>:start`/`top`/`bottom`/`end`,
//! `F<feature>:blend:<edge>`, `F<feature>:inner:<face>`, …). Pieces of a split face are told
//! apart by the faces around them (`F2:top#[F2:side:l4,F4:side:l4]`). Edges are named by the
//! faces they separate (`<face A>|<face B>`, `~k` for several edges between the same two).
//!
//! Where the feature that made a body reports exact history (`crate::lineage`), the names follow
//! it (`crate::history_names`). Bodies without history (made by operations that do not report
//! it yet, imported meshes, legacy chains) are named from geometry: sample points of each face
//! are looked up on the input's faces. Those names are marked unverified, and references that
//! resolve through them say so. Names are worked out lazily, on first request.

use std::sync::{Arc, OnceLock};

use solidcraft_geom::{Mesh, Seg2, Vec2, Vec3};

use crate::history_names::Names;
use crate::lineage::Lineage;
use crate::{Feature, FeatureKind, ModelBody, ModelState};

/// How a body came to be: the feature and the model just before it.
#[derive(Debug)]
pub struct NamingOrigin {
    pub feature: Feature,
    pub before: Arc<ModelState>,
    pub depth: usize,
    /// Instance transforms of a pattern or mirror (its copies are named after their sources).
    pub mats: Vec<crate::Mat>,
    /// The sheet metal bodies after a sheet feature (their folds place moved faces).
    pub sheets: Vec<crate::sheet::SheetBody>,
}

/// The lazily computed names of a body, shared by its clones.
#[derive(Clone, Debug, Default)]
pub struct NameCell {
    pub origin: Option<Arc<NamingOrigin>>,
    /// What the body's faces came from, when the feature that made it reports it.
    lineage: Option<Arc<Lineage>>,
    names: Arc<OnceLock<Arc<Names>>>,
    /// The names designs saved before format 14 gave its faces (see [`LEGACY_MARK`]).
    legacy: Arc<OnceLock<Arc<Vec<String>>>>,
    /// The body this one is a moved copy of (its legacy names are that body's).
    copy_of: Option<Arc<ModelBody>>,
}

impl NameCell {
    pub fn new(origin: Option<Arc<NamingOrigin>>) -> Self {
        Self::traced(origin, None)
    }
    /// Names of a body made by `origin`'s feature, following `lineage` where it is known.
    pub(crate) fn traced(origin: Option<Arc<NamingOrigin>>, lineage: Option<Arc<Lineage>>) -> Self {
        NameCell { origin, lineage, ..NameCell::default() }
    }
    /// Names already known, taken as they are.
    pub fn known(names: Arc<Vec<String>>) -> Self {
        let legacy = OnceLock::new();
        let _ = legacy.set(names.clone());
        NameCell { legacy: Arc::new(legacy), ..Self::shared(Arc::new(Names::given(names)), None) }
    }
    /// The names of `body` (a moved copy of a body keeps its faces' names, and how sure they are).
    pub fn same(body: &ModelBody) -> Self {
        Self::shared(names(body), Some(Arc::new(body.clone())))
    }
    fn shared(names: Arc<Names>, copy_of: Option<Arc<ModelBody>>) -> Self {
        let cell = OnceLock::new();
        let _ = cell.set(names);
        NameCell { names: Arc::new(cell), copy_of, ..NameCell::default() }
    }
}

pub use crate::name_lookup::{EdgeHit, FaceHit, LEGACY_MARK, face_has_name, faces_with_stem, legacy_name, locate_edge, locate_face};
pub use crate::piece_names::parent_piece;

/// Deeper chains than this are named by position only (bounded work and stack).
const MAX_DEPTH: usize = 400;

/// Up to this many sample points per face.
const SAMPLES: usize = 9;

/// Unit normals of each face's triangles.
fn normals(m: &Mesh, nf: usize) -> Vec<Vec<Vec3>> {
    let mut per: Vec<Vec<Vec3>> = vec![Vec::new(); nf];
    for (t, f) in m.triangles.iter().zip(&m.tri_face) {
        if let (Some(v), Some([a, b, c])) = (per.get_mut(*f as usize), m.tri(t))
            && let Some(n) = (b - a).cross(c - a).normalized()
        {
            v.push(n);
        }
    }
    per
}

/// Triangle centroids spread over each face: face → points.
fn samples(m: &Mesh, nf: usize) -> Vec<Vec<Vec3>> {
    let mut per: Vec<Vec<Vec3>> = vec![Vec::new(); nf];
    for (t, f) in m.triangles.iter().zip(&m.tri_face) {
        if let (Some(v), Some([a, b, c])) = (per.get_mut(*f as usize), m.tri(t)) {
            v.push((a + b + c) * (1.0 / 3.0));
        }
    }
    per.into_iter()
        .map(|v| {
            if v.len() <= SAMPLES {
                return v;
            }
            let step = v.len() as f64 / SAMPLES as f64;
            (0..SAMPLES).filter_map(|i| v.get((i as f64 * step) as usize).copied()).collect()
        })
        .collect()
}

/// The point of triangle `abc` nearest `p`. A zero-length edge (a cone's tip repeats its apex
/// vertex) has no interior to project onto, so its branch is skipped rather than dividing 0 by 0.
pub(crate) fn closest(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 && d1 - d3 > 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 && d2 - d6 > 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 && (d4 - d3) + (d5 - d6) > 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let den = va + vb + vc;
    if den.abs() < 1e-300 {
        return a;
    }
    a + ab * (vb / den) + ac * (vc / den)
}

/// A body's faces as triangle lists with bounding boxes, for "does this point lie on face f".
struct FaceIndex {
    faces: Vec<(Vec3, Vec3, Vec<[Vec3; 3]>)>,
}

impl FaceIndex {
    fn new(m: &Mesh, nf: usize) -> FaceIndex {
        let mut faces: Vec<(Vec3, Vec3, Vec<[Vec3; 3]>)> =
            vec![(Vec3::new(f64::MAX, f64::MAX, f64::MAX), Vec3::new(f64::MIN, f64::MIN, f64::MIN), Vec::new()); nf];
        for (t, f) in m.triangles.iter().zip(&m.tri_face) {
            if let (Some(slot), Some(tri)) = (faces.get_mut(*f as usize), m.tri(t)) {
                for p in tri {
                    slot.0 = Vec3::new(slot.0.x.min(p.x), slot.0.y.min(p.y), slot.0.z.min(p.z));
                    slot.1 = Vec3::new(slot.1.x.max(p.x), slot.1.y.max(p.y), slot.1.z.max(p.z));
                }
                slot.2.push(tri);
            }
        }
        FaceIndex { faces }
    }

    /// The face a point lies on (within `tol`).
    fn face_at(&self, p: Vec3, tol: f64) -> Option<usize> {
        let mut best: Option<(f64, usize)> = None;
        for (i, (lo, hi, tris)) in self.faces.iter().enumerate() {
            if p.x < lo.x - tol || p.y < lo.y - tol || p.z < lo.z - tol || p.x > hi.x + tol || p.y > hi.y + tol || p.z > hi.z + tol {
                continue;
            }
            let d = tris.iter().map(|[a, b, c]| closest(p, *a, *b, *c).dist(p)).fold(f64::MAX, f64::min);
            if d <= tol && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, i));
            }
        }
        best.map(|(_, i)| i)
    }
}

fn face_count(b: &ModelBody) -> usize {
    b.mesh().tri_face.iter().map(|f| *f as usize + 1).max().unwrap_or(0)
}

/// Names of a body's faces (by B-rep face index).
pub fn face_names(b: &ModelBody) -> Arc<Vec<String>> {
    names(b).faces.clone()
}

/// Whether face `index`'s name follows exact history (`false`: worked out from geometry, which an
/// upstream edit can fool).
pub fn face_name_exact(b: &ModelBody, index: usize) -> bool {
    names(b).verified(index)
}

/// A body's names, with what finding them again needs: from the history of the feature that
/// made it where it reports one, else from geometry (unverified).
pub(crate) fn names(b: &ModelBody) -> Arc<Names> {
    b.names
        .names
        .get_or_init(|| {
            // Exact history needs no mesh; geometry does (its face count is the mesh's).
            let names = match b.names.origin.as_ref() {
                Some(o) if o.depth < MAX_DEPTH => b
                    .names
                    .lineage
                    .as_ref()
                    .and_then(|l| crate::history_names::derive(b, o, l))
                    .unwrap_or_else(|| Names::unverified(geometric_names(b, o, face_count(b)), &mesh_adjacency(b))),
                _ => Names::unverified((0..face_count(b)).map(|k| format!("{}:face:{k}", b.name)).collect(), &[]),
            };
            Arc::new(names)
        })
        .clone()
}

/// The names builds before format 14 gave a body's faces: from geometry, over the inputs' names
/// as those builds gave them, pieces of a split face numbered along the longest axis of their
/// spread (`#1`, `#2`, …).
pub(crate) fn legacy_face_names(b: &ModelBody) -> Arc<Vec<String>> {
    if let Some(source) = &b.names.copy_of {
        return legacy_face_names(source);
    }
    b.names
        .legacy
        .get_or_init(|| {
            let nf = face_count(b);
            let names = match b.names.origin.as_ref() {
                Some(o) if o.depth < MAX_DEPTH => {
                    let mut names = geometric_from(b, o, nf, legacy_face_names);
                    number_pieces(&mut names, &samples(&b.mesh(), nf), '#');
                    names
                }
                _ => (0..nf).map(|k| format!("{}:face:{k}", b.name)).collect(),
            };
            Arc::new(names)
        })
        .clone()
}

/// The names builds before format 14 gave a body's edges.
pub(crate) fn legacy_edge_names(b: &ModelBody) -> Vec<String> {
    edge_names_from(b, &legacy_face_names(b))
}

/// Pairs of faces of a body that share an edge (from its mesh's edges).
pub(crate) fn mesh_adjacency(b: &ModelBody) -> Vec<(usize, usize)> {
    crate::history_names::face_pairs(b.mesh().edge_faces.iter().map(|fs| fs.iter().map(|f| *f as usize)))
}

/// Work out a body's face names from the feature that made it and the model before it, by
/// geometry: where each face's sample points lie on the input's faces. Faces sharing a name are
/// pieces of one, told apart afterwards.
pub(crate) fn geometric_names(b: &ModelBody, o: &NamingOrigin, nf: usize) -> Vec<String> {
    geometric_from(b, o, nf, face_names)
}

/// [`geometric_names`], the input bodies' faces named by `names_of`.
fn geometric_from(b: &ModelBody, o: &NamingOrigin, nf: usize, names_of: fn(&ModelBody) -> Arc<Vec<String>>) -> Vec<String> {
    // A rigid Move changes coordinates, not topology or face ownership. Geometric
    // overlap with the unmoved input cannot recover the names at the new position.
    if matches!(o.feature.kind, FeatureKind::Move { .. })
        && let Some(input) = o.before.body(&b.name)
    {
        return names_of(input).as_ref().clone();
    }
    let m = b.mesh();
    let size = b.body.size().max(1e-6);
    let tol = (size * 1e-4).max(1e-6);
    let pts = samples(&m, nf);
    let bb = m.bounds();
    // Inputs that overlap this body.
    let overlaps = |x: &ModelBody| {
        let xb = x.mesh().bounds();
        !(xb.max.x < bb.min.x - tol
            || xb.min.x > bb.max.x + tol
            || xb.max.y < bb.min.y - tol
            || xb.min.y > bb.max.y + tol
            || xb.max.z < bb.min.z - tol
            || xb.min.z > bb.max.z + tol)
    };
    // Independently created solids own their faces even when another body
    // occupies the same coordinates. Boolean operations still inherit inputs.
    let independent_solid = matches!(
        o.feature.kind,
        FeatureKind::Box { operation: crate::Operation::NewBody, .. }
            | FeatureKind::Cylinder { operation: crate::Operation::NewBody, .. }
            | FeatureKind::Sphere { operation: crate::Operation::NewBody, .. }
            | FeatureKind::Torus { operation: crate::Operation::NewBody, .. }
            | FeatureKind::Extrude { operation: crate::Operation::NewBody, .. }
            | FeatureKind::Revolve { operation: crate::Operation::NewBody, .. }
    );
    let inputs: Vec<(&ModelBody, FaceIndex, Arc<Vec<String>>)> = o
        .before
        .bodies
        .iter()
        .filter(|x| !independent_solid && !x.body.is_mesh() && overlaps(x))
        .take(64)
        .map(|x| {
            let n = face_count(x);
            (x, FaceIndex::new(&x.mesh(), n), names_of(x))
        })
        .collect();
    let fid = o.feature.id;
    let ns = normals(&m, nf);
    let mut names: Vec<Option<String>> = pts.iter().map(|ps| vote(ps, &inputs, tol, None)).collect();
    // A pattern or mirror copy: its faces lie on its source's faces moved by one instance.
    let found = names.iter().filter(|n| n.is_some()).count();
    if !o.mats.is_empty() && found * 2 < nf.max(1) {
        let all: Vec<(&ModelBody, FaceIndex, Arc<Vec<String>>)> = o
            .before
            .bodies
            .iter()
            .filter(|x| !x.body.is_mesh())
            .take(64)
            .map(|x| (x, FaceIndex::new(&x.mesh(), face_count(x)), names_of(x)))
            .collect();
        let mut best: Option<(usize, usize, Vec<Option<String>>)> = None;
        for (k, mat) in o.mats.iter().enumerate() {
            let Some(inv) = crate::mat_inverse(mat) else { continue };
            let got: Vec<Option<String>> = pts.iter().map(|ps| vote(ps, &all, tol, Some(&inv))).collect();
            let n = got.iter().filter(|x| x.is_some()).count();
            if n > found && best.as_ref().is_none_or(|(bn, _, _)| n > *bn) {
                best = Some((n, k, got));
            }
        }
        if let Some((_, k, got)) = best {
            names = got.into_iter().map(|n| n.map(|n| format!("{n}@F{fid}.{}", k + 1))).collect();
        }
    }
    // A sheet feature that moved panels (a fold, an unfold): faces it carried along lie on the
    // faces they were, taken back through the panel they are on.
    if let (Some(after), Some(before), Some(input)) = (
        o.sheets.iter().find(|s| s.body == b.name),
        o.before.sheets.iter().find(|s| s.body == b.name),
        inputs.iter().find(|(x, _, _)| x.name == b.name),
    ) {
        let input = std::slice::from_ref(input);
        for (name, ps) in names.iter_mut().zip(&pts) {
            if name.is_some() {
                continue;
            }
            let back: Option<Vec<Vec3>> = ps.iter().map(|p| after.point_before(before, *p)).collect();
            if let Some(back) = back.filter(|b| !b.is_empty()) {
                *name = vote(&back, input, tol, None);
            }
        }
    }
    let empty = Vec::new();
    let names: Vec<String> = names
        .into_iter()
        .enumerate()
        .map(|(fi, n)| {
            n.unwrap_or_else(|| {
                let (ps, nn) = (pts.get(fi).map(Vec::as_slice).unwrap_or(&[]), ns.get(fi).unwrap_or(&empty));
                // A shell's inner face: the face its wall runs back to.
                if matches!(o.feature.kind, FeatureKind::Shell { .. })
                    && let Some(n) = behind(ps, nn, &inputs)
                {
                    return format!("F{fid}:inner:{}", strip_piece(&n));
                }
                role_name(&o.feature, &o.before, ps, nn, fi, fid)
            })
        })
        .collect();
    names
}

/// The input face a face looks back at through the material (along minus its normal): the
/// face a shell wall's inner side offsets.
fn behind(ps: &[Vec3], ns: &[Vec3], inputs: &[(&ModelBody, FaceIndex, Arc<Vec<String>>)]) -> Option<String> {
    let n = ns.first().copied()?;
    let d = n * -1.0;
    let mut votes: Vec<(String, usize)> = Vec::new();
    for p in ps {
        let mut best: Option<(f64, String)> = None;
        for (_, idx, nm) in inputs {
            for (fi, (_, _, tris)) in idx.faces.iter().enumerate() {
                for [a, b, c] in tris {
                    if let Some(t) = ray_hit(*p, d, *a, *b, *c)
                        && t > 1e-9
                        && best.as_ref().is_none_or(|(bt, _)| t < *bt)
                        && let Some(name) = nm.get(fi)
                    {
                        best = Some((t, name.clone()));
                    }
                }
            }
        }
        if let Some((_, n)) = best {
            match votes.iter_mut().find(|(v, _)| *v == n) {
                Some(v) => v.1 += 1,
                None => votes.push((n, 1)),
            }
        }
    }
    votes.sort_by_key(|v| std::cmp::Reverse(v.1));
    votes.first().filter(|(_, k)| *k * 2 >= ps.len().max(1)).map(|(n, _)| n.clone())
}

pub(crate) fn ray_hit(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f64> {
    let (e1, e2) = (b - a, c - a);
    let h = d.cross(e2);
    let det = e1.dot(h);
    if det.abs() < 1e-14 {
        return None;
    }
    let f = 1.0 / det;
    let s = o - a;
    let u = f * s.dot(h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = f * d.dot(q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    Some(f * e2.dot(q))
}

/// The input face most of a face's sample points lie on (mapped by `map` first), if it holds
/// at least half of them.
fn vote(ps: &[Vec3], inputs: &[(&ModelBody, FaceIndex, Arc<Vec<String>>)], tol: f64, map: Option<&crate::Mat>) -> Option<String> {
    let mut votes: Vec<(String, usize)> = Vec::new();
    for p in ps {
        let p = match map {
            Some(m) => crate::apply_point(m, *p),
            None => *p,
        };
        for (_, idx, nm) in inputs {
            if let Some(f) = idx.face_at(p, tol)
                && let Some(n) = nm.get(f)
            {
                // A later operation may merge several pieces of the same input face.
                // Count their shared ancestry before testing the majority; a piece
                // number identifies topology subdivision, not a different source.
                let n = strip_piece(n);
                match votes.iter_mut().find(|(v, _)| v == n) {
                    Some(v) => v.1 += 1,
                    None => votes.push((n.to_string(), 1)),
                }
                break;
            }
        }
    }
    votes.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    votes.first().filter(|(_, n)| *n * 2 >= ps.len().max(1) && votes.get(1).is_none_or(|(_, other)| n > other)).map(|(n, _)| n.clone())
}

/// A face name without its piece qualifiers: the face it is a piece of (`F2:top` for
/// `F2:top#[F4:side:l1]`, for pieces of pieces, and for the numbered pieces of older designs).
pub fn strip_piece(n: &str) -> &str {
    let mut stem = n;
    while let Some(parent) = parent_piece(stem) {
        stem = parent;
    }
    stem
}

/// An edge name without piece numbers: its own (`~k`) and its faces' (`#[…]`, and the numbered
/// pieces of older designs), so pieces of a split edge, or of an edge between split faces, match.
pub fn strip_edge_piece(n: &str) -> String {
    crate::piece_names::strip_edge_piece(n)
}

/// Edges sharing a name: number them along the longest axis of their spread.
fn number_pieces(names: &mut [String], pts: &[Vec<Vec3>], mark: char) {
    // The pieces of each name, gathered in one pass (a faceted body has thousands of names).
    let mut groups: std::collections::BTreeMap<String, Vec<usize>> = std::collections::BTreeMap::new();
    for (i, n) in names.iter().enumerate() {
        groups.entry(n.clone()).or_default().push(i);
    }
    for (n, same) in groups {
        if same.len() < 2 {
            continue;
        }
        let centre = |j: usize| {
            let ps = pts.get(j).cloned().unwrap_or_default();
            ps.iter().fold(Vec3::ZERO, |a, p| a + *p) * (1.0 / ps.len().max(1) as f64)
        };
        let cs: Vec<(usize, Vec3)> = same.iter().map(|j| (*j, centre(*j))).collect();
        let (lo, hi) = cs.iter().fold((Vec3::new(f64::MAX, f64::MAX, f64::MAX), Vec3::new(f64::MIN, f64::MIN, f64::MIN)), |(l, h), (_, c)| {
            (Vec3::new(l.x.min(c.x), l.y.min(c.y), l.z.min(c.z)), Vec3::new(h.x.max(c.x), h.y.max(c.y), h.z.max(c.z)))
        });
        let ext = hi - lo;
        let key = |c: Vec3| {
            if ext.x >= ext.y && ext.x >= ext.z {
                (c.x, c.y, c.z)
            } else if ext.y >= ext.z {
                (c.y, c.z, c.x)
            } else {
                (c.z, c.x, c.y)
            }
        };
        let mut order = cs.clone();
        order.sort_by(|a, b| key(a.1).partial_cmp(&key(b.1)).unwrap_or(std::cmp::Ordering::Equal));
        for (k, (j, _)) in order.iter().enumerate() {
            if let Some(slot) = names.get_mut(*j) {
                *slot = format!("{n}{mark}{}", k + 1);
            }
        }
    }
}

/// The name of a face new to `f`.
fn role_name(f: &Feature, before: &ModelState, ps: &[Vec3], ns: &[Vec3], fi: usize, fid: u64) -> String {
    let c = ps.iter().fold(Vec3::ZERO, |a, p| a + *p) * (1.0 / ps.len().max(1) as f64);
    match &f.kind {
        FeatureKind::Extrude { sketch, .. } => {
            let Some(ss) = before.sketch(*sketch) else { return format!("F{fid}:face:{fi}") };
            let n = ss.plane.normal();
            // A face across the extrusion is a cap; others sweep a sketch curve.
            let flat = !ns.is_empty() && ns.iter().all(|m| m.cross(n).len() < 1e-6);
            if flat {
                // On the sketch plane, above it or below it.
                return format!("F{fid}:{}", crate::lineage::extrude_cap_role((c - ss.plane.origin).dot(n)));
            }
            let q = ss.plane.to_local(c);
            nearest_curve(ss, Vec2::new(q.x, q.y)).map(|id| format!("F{fid}:side:{id}")).unwrap_or_else(|| format!("F{fid}:face:{fi}"))
        }
        FeatureKind::Revolve { sketch, axis, .. } => {
            let Some(ss) = before.sketch(*sketch) else { return format!("F{fid}:face:{fi}") };
            let Ok((o2, d2)) = crate::eval::revolve_axis(ss, axis) else { return format!("F{fid}:face:{fi}") };
            let (o, d) = (ss.plane.to_world(o2), (ss.plane.to_world(o2 + d2) - ss.plane.to_world(o2)).normalized().unwrap_or(Vec3::Z));
            // A flat face holding the axis is an angle cap: on the sketch plane, or the far one.
            let flat = !ns.is_empty() && ns.windows(2).all(|w| w[0].cross(w[1]).len() < 1e-6) && ns.iter().all(|m| m.dot(d).abs() < 1e-6);
            if flat {
                let on_plane = ps.iter().all(|p| (*p - ss.plane.origin).dot(ss.plane.normal()).abs() < 1e-6 * (1.0 + p.len()));
                return format!("F{fid}:{}", if on_plane { "start" } else { "end" });
            }
            // A swept face: turn its centre back into the sketch plane and find the curve.
            let q = c - o;
            let t = q.dot(d);
            let r = (q - d * t).len();
            let u3 = ss.plane.normal().cross(d).normalized().unwrap_or(Vec3::X);
            let best = [u3, u3 * -1.0]
                .iter()
                .filter_map(|u| {
                    let w = ss.plane.to_local(o + d * t + *u * r);
                    let q2 = Vec2::new(w.x, w.y);
                    nearest_curve_dist(ss, q2)
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));
            best.map(|(_, id)| format!("F{fid}:side:{id}")).unwrap_or_else(|| format!("F{fid}:face:{fi}"))
        }
        FeatureKind::Sweep { .. } | FeatureKind::Loft { .. } => {
            let sk = match &f.kind {
                FeatureKind::Sweep { sketch, .. } => Some(*sketch),
                FeatureKind::Loft { sections, .. } => sections.first().map(|x| x.sketch),
                _ => None,
            };
            let Some(ss) = sk.and_then(|k| before.sketch(k)) else { return format!("F{fid}:face:{fi}") };
            let n = ss.plane.normal();
            let h = |p: &Vec3| (*p - ss.plane.origin).dot(n);
            let flat = !ns.is_empty() && ns.windows(2).all(|w| w[0].cross(w[1]).len() < 1e-6);
            if flat && ps.iter().all(|p| h(p).abs() < 1e-6 * (1.0 + p.len())) {
                return format!("F{fid}:start");
            }
            if flat && ns.iter().all(|m| m.cross(n).len() < 1e-6) {
                return format!("F{fid}:end");
            }
            // A side: the profile curve under its sample nearest the start.
            let Some(p0) = ps.iter().min_by(|a, b| h(a).abs().total_cmp(&h(b).abs())) else { return format!("F{fid}:face:{fi}") };
            let q = ss.plane.to_local(*p0);
            nearest_curve(ss, Vec2::new(q.x, q.y)).map(|id| format!("F{fid}:side:{id}")).unwrap_or_else(|| format!("F{fid}:face:{fi}"))
        }
        FeatureKind::Box { .. } => {
            let n = ns.first().copied().unwrap_or(Vec3::Z);
            let axis = |v: f64, p: &str, m: &str| {
                if v > 0.5 {
                    Some(p.to_string())
                } else if v < -0.5 {
                    Some(m.to_string())
                } else {
                    None
                }
            };
            let role =
                axis(n.x, "+x", "-x").or_else(|| axis(n.y, "+y", "-y")).or_else(|| axis(n.z, "+z", "-z")).unwrap_or_else(|| format!("face:{fi}"));
            format!("F{fid}:{role}")
        }
        FeatureKind::Cylinder { axis, .. } => {
            let flat = !ns.is_empty() && ns.iter().all(|m| m.cross(*axis).len() < 1e-6);
            let role = if !flat {
                "side"
            } else if ns.first().is_some_and(|m| m.dot(*axis) > 0.0) {
                "top"
            } else {
                "bottom"
            };
            format!("F{fid}:{role}")
        }
        FeatureKind::Fillet { edges, .. } | FeatureKind::Chamfer { edges, .. } => {
            let k = edges.iter().enumerate().min_by(|a, b| a.1.dist(c).total_cmp(&b.1.dist(c))).map(|(k, _)| k).unwrap_or(0);
            match f.edge_names.get(k) {
                Some(e) => format!("F{fid}:blend:{e}"),
                None => format!("F{fid}:blend:{k}"),
            }
        }
        _ => format!("F{fid}:face:{fi}"),
    }
}

/// The sketch curve (id) nearest a point in the sketch plane, and how near.
fn nearest_curve_dist(ss: &crate::SolvedSketch, q: Vec2) -> Option<(f64, String)> {
    let mut best: Option<(f64, String)> = None;
    for (ci, c) in ss.sketch.curves.iter().enumerate() {
        if c.construction {
            continue;
        }
        for s in ss.sketch.segs(ci) {
            let d = seg_dist(&s, q);
            if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                best = Some((d, c.id.clone()));
            }
        }
    }
    best
}

/// The sketch curve (id) nearest a point in the sketch plane.
fn nearest_curve(ss: &crate::SolvedSketch, q: Vec2) -> Option<String> {
    let mut best: Option<(f64, String)> = None;
    for (ci, c) in ss.sketch.curves.iter().enumerate() {
        if c.construction {
            continue;
        }
        for s in ss.sketch.segs(ci) {
            let d = seg_dist(&s, q);
            if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                best = Some((d, c.id.clone()));
            }
        }
    }
    best.map(|(_, id)| id)
}

fn seg_dist(s: &Seg2, q: Vec2) -> f64 {
    let pts = s.polyline(1e-3);
    pts.windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let d = b - a;
            let t = if d.len2() > 0.0 { ((q - a).dot(d) / d.len2()).clamp(0.0, 1.0) } else { 0.0 };
            (a + d * t).dist(q)
        })
        .fold(f64::MAX, f64::min)
}

/// Names of a body's edges (by mesh edge index): `<face A>|<face B>`, numbered when repeated.
pub fn edge_names(b: &ModelBody) -> Vec<String> {
    edge_names_from(b, &face_names(b))
}

/// Names of a body's edges from its faces' names `faces`.
fn edge_names_from(b: &ModelBody, faces: &[String]) -> Vec<String> {
    let m = b.mesh();
    let mut names: Vec<String> = m
        .edge_faces
        .iter()
        .map(|fs| {
            let mut n: Vec<&str> = fs.iter().filter_map(|f| faces.get(*f as usize).map(String::as_str)).collect();
            n.sort();
            n.join("|")
        })
        .collect();
    let pts: Vec<Vec<Vec3>> = m.edges.clone();
    number_pieces(&mut names, &pts, '~');
    names
}

/// The edge of a body with this name: exact, else the pieces of the same edge nearest `near`
/// (`Err` explains when there is none).
pub fn find_edge(b: &ModelBody, name: &str, near: Vec3) -> Result<(Vec3, bool), String> {
    locate_edge(b, name, near).map(|hit| (hit.point, hit.split))
}

/// Names of the edges nearest each point on a body (for storing references).
pub fn edge_names_at(b: &ModelBody, pts: &[Vec3]) -> Vec<String> {
    let m = b.mesh();
    let names = edge_names(b);
    pts.iter()
        .map(|p| {
            m.edges
                .iter()
                .enumerate()
                .map(|(i, e)| (e.windows(2).map(|w| p.dist_to_segment(w[0], w[1])).fold(f64::MAX, f64::min), i))
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .and_then(|(_, i)| names.get(i).cloned())
                .unwrap_or_default()
        })
        .collect()
}

/// Names of the faces holding each point (any body; empty when none does).
pub fn face_names_at(st: &ModelState, pts: &[Vec3]) -> Vec<String> {
    pts.iter()
        .map(|p| {
            for b in st.bodies.iter().filter(|b| !b.body.is_mesh()) {
                let tol = (b.body.size() * 1e-3).max(1e-3);
                let idx = FaceIndex::new(&b.mesh(), face_count(b));
                if let Some(f) = idx.face_at(*p, tol) {
                    return face_names(b).get(f).cloned().unwrap_or_default();
                }
            }
            String::new()
        })
        .collect()
}

/// Capture a face name from the recorded projection body only.
pub fn face_name_at_body(body: &ModelBody, point: Vec3) -> Option<String> {
    if body.body.is_mesh() || !point.is_finite() {
        return None;
    }
    let mesh = body.mesh();
    let count = face_count(body);
    if mesh.triangles.len() > 2_000_000 || count > 50_000 {
        return None;
    }
    let tolerance = (body.body.size() * 1e-3).max(1e-3);
    let index = FaceIndex::new(&mesh, count).face_at(point, tolerance)?;
    face_names(body).get(index).filter(|name| !name.is_empty()).cloned()
}

/// The point on the face with this name nearest `p` (a split face: its nearest piece, with
/// `true` for "not exactly that face": split, or merged into another); `None` when no face has
/// the name.
pub fn point_on_face(st: &ModelState, name: &str, p: Vec3) -> Option<(Vec3, bool)> {
    locate_face(st, name, p).map(|hit| (hit.point, hit.split || hit.merged))
}

/// Resolve a legacy named face only on its recorded source body. Return the
/// selected face index as well as the closest point, which may lie on a shared
/// edge and therefore must not be used to pick the face a second time.
pub fn face_on_body(body: &ModelBody, name: &str, p: Vec3) -> Option<(usize, Vec3, bool)> {
    if body.body.is_mesh() {
        return None;
    }
    crate::name_lookup::locate_on(body, name, p).map(|hit| (hit.face, hit.point, hit.split || hit.merged))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cone's tip repeats its apex vertex, so a triangle can have a zero-length edge. The nearest
    /// point on it must still be a real point (it used to be NaN, which broke picking the tip).
    #[test]
    fn closest_point_on_a_degenerate_triangle_is_finite() {
        let (a, c) = (Vec3::new(0., 0., 0.), Vec3::new(2., 0., 0.));
        for (b, c) in [(a, c), (c, c), (a, a)] {
            for p in [Vec3::new(1., 1., 0.), Vec3::new(-1., 0.5, 0.), Vec3::new(3., -2., 1.), a] {
                let q = closest(p, a, b, c);
                assert!(q.x.is_finite() && q.y.is_finite() && q.z.is_finite(), "{a:?} {b:?} {c:?} {p:?} -> {q:?}");
            }
        }
        assert_eq!(closest(Vec3::new(1., 1., 0.), a, a, c), Vec3::new(1., 0., 0.));
    }

    #[test]
    fn merged_face_votes_collectively_for_pieces_of_one_source() {
        let body = ModelBody::new("Input".into(), solidcraft_kernel::box_solid(Vec3::ZERO, Vec3::new(4., 1., 1.)).unwrap(), 1);
        let mut mesh = Mesh::default();
        let mut points = Vec::new();
        for i in 0..3u32 {
            let x = f64::from(i);
            mesh.positions.extend([Vec3::new(x, 0., 0.), Vec3::new(x + 0.8, 0., 0.), Vec3::new(x, 0.8, 0.)]);
            mesh.triangles.push([i * 3, i * 3 + 1, i * 3 + 2]);
            mesh.tri_face.push(i);
            points.push(Vec3::new(x + 0.2, 0.2, 0.));
        }
        let pieces = Arc::new(vec!["F1:side:wall#1".into(), "F1:side:wall#2".into(), "F1:side:wall#3".into()]);
        assert_eq!(vote(&points, &[(&body, FaceIndex::new(&mesh, 3), pieces)], 1e-8, None).as_deref(), Some("F1:side:wall"));
        let distinct = Arc::new(vec!["F1:side:wall#1".into(), "F2:side:wall#2".into(), "F3:side:wall#3".into()]);
        let inputs = [(&body, FaceIndex::new(&mesh, 3), distinct)];
        assert_eq!(vote(&points, &inputs, 1e-8, None), None);
        assert_eq!(vote(&points[..2], &inputs, 1e-8, None), None, "equal different-source votes must not pick a lexicographic winner");
    }
}
