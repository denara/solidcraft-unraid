//! Operations on bodies: booleans and rigid transforms.

use serde::{Deserialize, Serialize};
use solidcraft_geom::Vec3;
use truck_modeling as mt;

use std::collections::HashMap;

use crate::body::{Body, Solid, from_p3};
use crate::boolean_diagnostics::{BooleanFallback, BooleanStrategy, capture_boolean_diagnostics, merge, record};
use crate::boolean_record::{self as recorded, Traced};
use crate::{KernelError, Result, guard, record};

#[cfg(test)]
#[path = "bore_union_tests.rs"]
mod bore_union_tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoolOp {
    Union,
    Cut,
    Intersect,
}

/// Whether a union can sew two matching planar faces without intersecting them.
/// Callers can retain the exact touching tool instead of extending it into material.
pub fn can_join_touching_faces(a: &Body, b: &Body) -> bool {
    a.solid.boundaries().len() == 1 && b.solid.boundaries().len() == 1 && crate::coplanar::glue(a, b).is_some_and(|r| r.is_ok())
}

/// Shifts applied to both operands on retries. The boolean classifies some faces by casting a
/// ray whose direction is derived from point coordinates; moving both solids (and the result
/// back) leaves the geometry unchanged but changes those rays.
const JITTER: [[f64; 3]; 4] = [[0.0, 0.0, 0.0], [0.1373, 0.2719, 0.3911], [-0.5127, 0.0833, 0.2291], [0.6911, -0.3337, -0.6127]];

fn volume(b: &Body) -> f64 {
    // Measured once per (solid, tolerance) and shared by clones, so an operand a later
    // boolean takes again is not meshed twice for the same number.
    b.volume_at(b.size() * 2e-3).unwrap_or(f64::NAN)
}

/// Halton low-discrepancy value (base `b`) for index `i`.
fn halton(mut i: usize, b: usize) -> f64 {
    let (mut f, mut r) = (1.0, 0.0);
    while i > 0 {
        f /= b as f64;
        r += f * (i % b) as f64;
        i /= b;
    }
    r
}

/// Fraction of sample points where `result` disagrees with `op` applied to `a` and `b`
/// (membership by ray parity). Samples cover the overlap box densely and the union box lightly.
fn mismatch(a: &solidcraft_geom::Mesh, b: &solidcraft_geom::Mesh, result: &solidcraft_geom::Mesh, op: BoolOp) -> Option<f64> {
    if a.triangles.len() + b.triangles.len() + result.triangles.len() > 60_000 {
        return None;
    }
    let (ba, bb) = (a.bounds(), b.bounds());
    let lo = ba.min.max(bb.min);
    let hi = ba.max.min(bb.max);
    let all = ba.union(&bb);
    let mut boxes = vec![(all, 300usize)];
    // A touching seam can overlap by round-off only. Samples there sit on the
    // operands' boundaries, where ray parity cannot classify membership reliably.
    let gap = all.diagonal().max(1.0) * 1e-9;
    if hi.x - lo.x > gap && hi.y - lo.y > gap && hi.z - lo.z > gap {
        boxes.push((solidcraft_geom::Aabb3 { min: lo, max: hi }, 900));
    }
    let (ia_idx, ib_idx, ir_idx) = (a.inside_index(), b.inside_index(), result.inside_index());
    let (mut n, mut bad) = (0usize, 0usize);
    for (bx, count) in boxes {
        let s = bx.size();
        for i in 1..=count {
            let p = bx.min + Vec3::new(s.x * halton(i, 2), s.y * halton(i, 3), s.z * halton(i, 5));
            let (ia, ib) = (ia_idx.contains(p), ib_idx.contains(p));
            let want = match op {
                BoolOp::Union => ia || ib,
                BoolOp::Cut => ia && !ib,
                BoolOp::Intersect => ia && ib,
            };
            n += 1;
            if ir_idx.contains(p) != want {
                bad += 1;
            }
        }
    }
    Some(bad as f64 / n.max(1) as f64)
}

/// How one body sits relative to another, from sample points: (fraction of `b`'s samples
/// inside `a`, fraction of `a`'s samples inside `b`).
fn overlap(
    a: &solidcraft_geom::Mesh,
    b: &solidcraft_geom::Mesh,
    ia: &solidcraft_geom::InsideIndex,
    ib: &solidcraft_geom::InsideIndex,
) -> Option<(f64, f64)> {
    let frac = |inner: &solidcraft_geom::Mesh, inner_i: &solidcraft_geom::InsideIndex, outer_i: &solidcraft_geom::InsideIndex| -> Option<f64> {
        let bx = inner.bounds();
        let s = bx.size();
        let (mut n, mut hit) = (0usize, 0usize);
        for i in 1..=2000 {
            let p = bx.min + Vec3::new(s.x * halton(i, 2), s.y * halton(i, 3), s.z * halton(i, 5));
            if !inner_i.contains(p) {
                continue;
            }
            n += 1;
            if outer_i.contains(p) {
                hit += 1;
            }
        }
        (n >= 50).then(|| hit as f64 / n as f64)
    };
    Some((frac(b, ib, ia)?, frac(a, ia, ib)?))
}

/// Whether some facet of `m` has material just under it (`depth` in from its middle) that is
/// inside the other body (`inside`) or outside it. With `within`, only facets in that box.
fn under_surface(
    m: &solidcraft_geom::Mesh,
    own: &solidcraft_geom::InsideIndex,
    other: &solidcraft_geom::InsideIndex,
    depth: f64,
    inside: bool,
    within: Option<&solidcraft_geom::Aabb3>,
) -> bool {
    m.triangles.iter().filter_map(|t| m.tri(t)).any(|[p, q, r]| {
        let Some(n) = (q - p).cross(r - p).normalized() else { return false };
        let c = (p + q + r) * (1.0 / 3.0) - n * depth;
        let near = within.is_none_or(|b| c.x >= b.min.x && c.x <= b.max.x && c.y >= b.min.y && c.y <= b.max.y && c.z >= b.min.z && c.z <= b.max.z);
        // A wall thinner than `depth` puts the point outside its own body: no evidence.
        near && other.contains(c) == inside && own.contains(c)
    })
}

/// Whether the bodies' facetings show that the samples' answer (see [`trivial`]) is wrong:
/// their surfaces pass through each other, or a facet of one has material under it on the
/// wrong side of the other. Overlaps and gaps shallower than `tol` are not seen.
fn contradicted(
    (ma, ia): (&solidcraft_geom::Mesh, &solidcraft_geom::InsideIndex),
    (mb, ib): (&solidcraft_geom::Mesh, &solidcraft_geom::InsideIndex),
    (disjoint, b_in_a, a_in_b): (bool, bool, bool),
    tol: f64,
) -> bool {
    if !(disjoint || b_in_a || a_in_b) {
        return false;
    }
    if ma.crosses(mb, tol) {
        return true;
    }
    if disjoint {
        let (ba, bb) = (ma.bounds(), mb.bounds());
        let shared = solidcraft_geom::Aabb3 { min: ba.min.max(bb.min), max: ba.max.min(bb.max) };
        return !shared.is_empty() && (under_surface(ma, ia, ib, tol, true, Some(&shared)) || under_surface(mb, ib, ia, tol, true, Some(&shared)));
    }
    (a_in_b && under_surface(ma, ia, ib, tol, false, None)) || (b_in_a && !a_in_b && under_surface(mb, ib, ia, tol, false, None))
}

/// Sampled overlap fraction below which two bodies share no material.
const NONE: f64 = 0.003;

/// Booleans that need no intersection: one body clear of the other, or inside it. These are
/// common with coincident curved faces (a hole drilled where one already is, a body joined
/// with itself), which the intersection can't handle. `counts`: the operands' faces and edges.
/// `tol`: how far the meshes' surfaces may pass through each other and still count as touching
/// (they are facetings of the bodies, so a few times their tolerance).
///
/// The second value is false when the facetings contradict the samples ([`contradicted`]):
/// the answer is then only what to fall back on if the bodies can't be intersected.
fn trivial(
    a: &Body,
    b: &Body,
    op: BoolOp,
    ma: &solidcraft_geom::Mesh,
    mb: &solidcraft_geom::Mesh,
    counts: &[(usize, usize)],
    tol: f64,
) -> Option<(Option<Body>, bool)> {
    if ma.triangles.len() + mb.triangles.len() > 80_000 {
        return None;
    }
    let (ia, ib) = (ma.inside_index(), mb.inside_index());
    let (b_in_a, a_in_b) = overlap(ma, mb, &ia, &ib)?;
    const ALL: f64 = 0.997;
    let disjoint = b_in_a < NONE && a_in_b < NONE;
    // The samples only say that nearly all of one body is clear of, or inside, the other. A
    // small part can still be on the wrong side (a peg overlapping a rib, a chamfer taken off
    // one edge), and then the bodies have to be intersected.
    let sure = !contradicted((ma, &ia), (mb, &ib), (disjoint, b_in_a > ALL, a_in_b > ALL), tol);
    let answer = match op {
        // Sampling only selects this candidate. Validate every shell boundary and the
        // complete material/void nesting before preserving a disconnected union.
        BoolOp::Union if disjoint => {
            let pieces = vec![(a.clone(), recorded::kept(a, 0, 2)), (b.clone(), recorded::kept(b, 1, 2))];
            let joined = recorded::join(pieces, counts).ok().flatten()?;
            joined.separated_lumps().ok()?;
            Some(Some(joined))
        }
        BoolOp::Cut if disjoint => Some(Some(recorded::operand(a, 0, counts))),
        BoolOp::Cut if a_in_b > ALL => Some(None),
        BoolOp::Intersect if disjoint => Some(None),
        BoolOp::Intersect if b_in_a > ALL => Some(Some(recorded::operand(b, 1, counts))),
        BoolOp::Intersect if a_in_b > ALL => Some(Some(recorded::operand(a, 0, counts))),
        BoolOp::Union if b_in_a > ALL => Some(Some(recorded::operand(a, 0, counts))),
        BoolOp::Union if a_in_b > ALL => Some(Some(recorded::operand(b, 1, counts))),
        // The tool wholly inside, clear of the outside: a void.
        BoolOp::Cut if b_in_a > ALL => {
            let candidate = void(a, b, counts).ok().flatten()?;
            candidate.separated_lumps().ok()?;
            Some(Some(candidate))
        }
        _ => None,
    };
    answer.map(|r| (r, sure))
}

#[cfg(test)]
mod shortcut_tests {
    use super::*;

    #[test]
    fn curved_near_contact_never_uses_a_shell_only_boolean_candidate() {
        let direction = Vec3::new(1.0, 2.0, 3.0).normalized().unwrap();
        let outer = crate::sphere(Vec3::ZERO, 10.0).unwrap();
        let mesh = outer.tessellate(0.03).unwrap();
        for distance in [19.999, 20.001] {
            let other = crate::sphere(direction * distance, 10.0).unwrap();
            let other_mesh = other.tessellate(0.03).unwrap();
            let counts = record::counts(&[&outer, &other]);
            assert!(trivial(&outer, &other, BoolOp::Union, &mesh, &other_mesh, &counts, 0.09).is_none());
            assert!(trivial(&other, &outer, BoolOp::Union, &other_mesh, &mesh, &counts, 0.09).is_none());
        }
        let protruding = crate::sphere(direction * 9.001, 1.0).unwrap();
        let protruding_mesh = protruding.tessellate(0.03).unwrap();
        let counts = record::counts(&[&outer, &protruding]);
        assert!(trivial(&outer, &protruding, BoolOp::Cut, &mesh, &protruding_mesh, &counts, 0.09).is_none());
    }

    /// Bodies sharing less than the samples can see still meet: no shortcut, and the boolean
    /// gives the shared part.
    #[test]
    fn a_small_overlap_is_not_taken_for_apart_or_inside() {
        let cube = |x: f64, y: f64| crate::box_solid(Vec3::new(x, y, 0.0), Vec3::new(x + 10.0, y + 10.0, 10.0)).unwrap();
        let a = cube(0.0, 0.0);
        // A 0.01 slab between parallel faces (0.1 % of each cube), and a 0.02 x 0.02 corner.
        for (b, want) in [(cube(9.99, 0.0), 1.0), (cube(9.98, 9.98), 0.004)] {
            let (ma, mb) = (a.tessellate(0.001).unwrap(), b.tessellate(0.001).unwrap());
            let counts = record::counts(&[&a, &b]);
            for op in [BoolOp::Intersect, BoolOp::Cut, BoolOp::Union] {
                assert!(trivial(&a, &b, op, &ma, &mb, &counts, 0.003).is_none_or(|(_, sure)| !sure), "{op:?} {want}");
            }
            let shared = boolean(&a, &b, BoolOp::Intersect).unwrap().unwrap();
            assert!((crate::measure(&shared).unwrap().volume - want).abs() < 1e-9, "{want}");
            let cut = boolean(&a, &b, BoolOp::Cut).unwrap().unwrap();
            assert!((crate::measure(&cut).unwrap().volume - (1000.0 - want)).abs() < 1e-9, "{want}");
        }
        // Touching on a face, and one cube well inside the other, still take the shortcut.
        let (touching, inner) = (cube(10.0, 0.0), crate::box_solid(Vec3::new(2.0, 2.0, 2.0), Vec3::new(8.0, 8.0, 8.0)).unwrap());
        let ma = a.tessellate(0.001).unwrap();
        for (b, op) in [(&touching, BoolOp::Intersect), (&inner, BoolOp::Intersect), (&inner, BoolOp::Union)] {
            let counts = record::counts(&[&a, b]);
            assert!(trivial(&a, b, op, &ma, &b.tessellate(0.001).unwrap(), &counts, 0.003).is_some_and(|(_, sure)| sure), "{op:?}");
        }
    }
}

/// `a` with the inside of `b` (wholly within it) taken out: a second, inward-facing shell.
fn void(a: &Body, b: &Body, counts: &[(usize, usize)]) -> Result<Option<Body>> {
    guard("void", || {
        let mut inner = b.deep_copy();
        let inner_edges = record::edge_origins(&inner, 1);
        inner.not();
        let mut shells = a.solid.boundaries().clone();
        shells.extend(inner.boundaries().iter().cloned());
        let s = crate::body::Solid::try_new(shells).map_err(|e| KernelError::Failed(format!("void: {e}")))?;
        let (faces, edges) = recorded::void(a, inner_edges, b.face_count());
        Ok(Some(record::build(s, counts, Some(faces), Some(&edges))?))
    })
}

thread_local! {
    /// Set while a boolean runs on bodies whose coincident faces were pushed apart (no
    /// second round of pushing).
    static APART: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Set while a union runs with a bore narrowed off a common cylinder (no second round).
    static BORES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Boolean of two bodies. The result may be empty (`Ok(None)`) for a cut that removes
/// everything or an intersection of disjoint bodies. Results are checked against volume bounds
/// and retried with shifted copies and other tolerances when they fail or look wrong.
///
/// The result carries its exact history ([`Body::history`]) over the operands `[a, b]`,
/// whichever way it was made, or none where a step it took does not report one.
pub fn boolean(a: &Body, b: &Body, op: BoolOp) -> Result<Option<Body>> {
    let (mut r, mut diagnostics) = capture_boolean_diagnostics(|| boolean_whole(a, b, op));
    // Bodies in several pieces: when the whole failed (or came out empty), a piece at a time.
    let retry = match &r {
        Err(_) => true,
        Ok(None) => op != BoolOp::Intersect,
        Ok(Some(_)) => false,
    };
    if retry && (a.solid.boundaries().len() > 1 || b.solid.boundaries().len() > 1) {
        let (la, lb) = (a.lumps()?, b.lumps()?);
        if la.len() > 1 || lb.len() > 1 {
            let (pieces, report) = capture_boolean_diagnostics(|| by_lumps(a, b, la, lb, op));
            if let Ok(Some(x)) = pieces {
                r = Ok(Some(x));
                diagnostics = report;
                diagnostics.push(BooleanFallback::new(BooleanStrategy::Piecewise));
            }
        }
    }
    if op == BoolOp::Union
        && let Ok(Some(body)) = &r
        && !preserves_planar_surfaces(a, b, body)
    {
        // Numerical retries must not quietly alter a planar design's dimensions.
        // Prefer the exact polygon result; reject an edge-only pinch if it cannot
        // form a closed solid without displacing either operand.
        r = match crate::polybool::planar_boolean(a, b, op) {
            Ok(Some(exact)) if preserves_planar_surfaces(a, b, &exact) => {
                diagnostics = Default::default();
                diagnostics.events.push(BooleanFallback::new(BooleanStrategy::PlanarFallback));
                Ok(Some(exact))
            }
            _ => Err(KernelError::Invalid(
                "join cannot preserve the original faces; check for zero-thickness edge contact or add intentional overlap".into(),
            )),
        };
    }
    r = r.map(|result| result.map(|body| body.inherit_regularization_protection(a).inherit_regularization_protection(b)));
    match op {
        BoolOp::Cut => r = r.and_then(|result| result.map(|body| body.with_preserved_boolean_planar_sources(a, b, true)).transpose()),
        BoolOp::Union => r = r.and_then(|result| result.map(|body| body.with_preserved_union_planar_sources(a, b)).transpose()),
        BoolOp::Intersect => {}
    }
    // A retry's result is held to what the operation can do at all; one that fails is refused
    // rather than returned wrong (issue #616).
    if (!diagnostics.events.is_empty() || diagnostics.truncated)
        && let Ok(result) = &r
        && let Err(e) = crate::boolean_guard::check_retry(a, b, op, result.as_ref())
    {
        let tried = diagnostics.warning().unwrap_or_default();
        r = Err(match e {
            KernelError::Failed(m) => KernelError::Failed(format!("{m} ({})", tried.trim_start_matches("Boolean retry: "))),
            other => other,
        });
    }
    if r.is_ok() {
        merge(diagnostics);
    }
    r
}

/// Every face of an all-planar union must remain on an operand's original plane.
/// Curved operands keep their existing surface-specific boolean handling.
fn preserves_planar_surfaces(a: &Body, b: &Body, result: &Body) -> bool {
    guard("validate planar union", || Ok(planar_surfaces_match(a, b, result))).unwrap_or(false)
}

fn planar_surfaces_match(a: &Body, b: &Body, result: &Body) -> bool {
    let planes: Option<Vec<(Vec3, f64)>> = a
        .solid
        .face_iter()
        .chain(b.solid.face_iter())
        .map(|f| {
            let mt::Surface::Plane(p) = f.oriented_surface() else { return None };
            let n = p.normal();
            let n = Vec3::new(n.x, n.y, n.z).normalized()?;
            Some((n, n.dot(from_p3(p.origin()))))
        })
        .collect();
    let Some(planes) = planes else { return true };
    let tol = (a.size().max(b.size()) * 1e-9).max(1e-9);
    result.solid.face_iter().all(|f| {
        matches!(f.oriented_surface(), mt::Surface::Plane(_))
            && planes.iter().any(|(n, d)| f.vertex_iter().all(|v| (n.dot(from_p3(v.point())) - d).abs() <= tol))
    })
}

/// Smallest side of the box where the bodies' vertex boxes overlap (infinite when they don't).
fn meeting_size(a: &Body, b: &Body) -> f64 {
    let bbox = |x: &Body| {
        let mut bb = solidcraft_geom::Aabb3::EMPTY;
        for v in x.solid.vertex_iter() {
            bb.add(from_p3(v.point()));
        }
        bb
    };
    let (ba, bb) = (bbox(a), bbox(b));
    let s = solidcraft_geom::Aabb3 { min: ba.min.max(bb.min), max: ba.max.min(bb.max) }.size();
    if s.x > 0.0 && s.y > 0.0 && s.z > 0.0 { s.x.min(s.y).min(s.z) } else { f64::INFINITY }
}

/// Whether a result volume is plausible for `op`: within the bounds the operands' volumes
/// leave, plus the slack the meshed volumes are measured to.
fn plausible(v: f64, op: BoolOp, va: f64, vb: f64, slack: f64) -> bool {
    v > 0.0
        && match op {
            BoolOp::Union => v >= va.max(vb) - slack && v <= va + vb + slack,
            BoolOp::Cut => v >= va - vb - slack && v <= va + slack,
            BoolOp::Intersect => v <= va.min(vb) + slack,
        }
}

/// The prism route's result: `Some` when both operands are prisms along one direction (see
/// [`crate::prism::prism_boolean`]), with a union's slab seams healed as the general B-rep
/// boolean does.
fn prism_result(a: &Body, b: &Body, op: BoolOp, size: f64, counts: &[(usize, usize)]) -> Option<Result<Option<Body>>> {
    // Slab-by-slab prism booleans sew a closed solid, but leave cap and wall pieces along slab
    // boundaries: a cut along Z through a box whose side wall already has a port in it splits
    // that wall into strips at the port's top and bottom, though the cut never touches it. Merge
    // those coplanar faces for every operation, just as the general B-rep boolean does, retaining
    // intentional Split Face lines. Authored boundaries (painted faces, regularization
    // protection) keep their pieces.
    let keep: Vec<Vec<Vec3>> = a.split_keep().iter().chain(b.split_keep()).cloned().collect();
    let authored = |x: &Body| x.regularization_protected || x.paint().is_some_and(|p| !p.faces.is_empty());
    crate::prism::prism_boolean(a, b, op).map(|result| {
        result.map(|body| {
            body.map(|body| {
                if op == BoolOp::Union {
                    guard("heal prism boolean", || recorded::heal_body(&body, (*body.solid).clone(), size, &keep, counts)).unwrap_or(body)
                } else if authored(a) || authored(b) {
                    body
                } else {
                    guard("heal prism boolean", || recorded::heal_prism_result(&body, size, &keep, counts)).unwrap_or(body)
                }
            })
        })
    })
}
fn boolean_whole(a: &Body, b: &Body, op: BoolOp) -> Result<Option<Body>> {
    a.require_brep("a boolean")?;
    b.require_brep("a boolean")?;
    let size = a.size().max(b.size());
    let counts = record::counts(&[a, b]);
    // The checks mesh both bodies; where they meet in a region much thinner than they are (a
    // thin part trimmed by a long one), a step from their size would facet the small faces
    // there coarsely enough to fail the membership test on a correct result. The meshes are
    // refined: a round side trimmed to a thin band (a short boss crossing a gently sloping
    // floor) is otherwise spanned by triangles cutting through it, which reads as missing
    // material.
    let tol_m = (size * 5e-4).min(meeting_size(a, b) * 2e-3).max(size * 1e-5);
    // A prism pair (a plate and the tool through it) is decided exactly in 2D, with no mesh of
    // either operand. The sampled shortcuts below need a refined mesh of both, and that mesh is
    // most of what a boolean costs: a pattern or a multi-profile sketch rebuilds the whole
    // target's mesh once per tool. Only a cut or an intersection meets its tool often enough to
    // be worth asking first - parts that stay apart are settled far more cheaply by the shell
    // join the sampled shortcut does. A pair the route cannot answer still takes the path below,
    // unchanged.
    let early_prism = matches!(op, BoolOp::Cut | BoolOp::Intersect);
    let prism = early_prism.then(|| prism_result(a, b, op, size, &counts)).flatten();
    if let Some(Ok(Some(body))) = &prism {
        let (va0, vb0) = (volume(a), volume(b));
        let (v0, slack0) = (volume(body), 2e-3 * (va0 + vb0) + 1e-9);
        // A tool that misses its target, or covers it whole, leaves it as it is, and the
        // sampled shortcut below answers that by reusing the operand itself: the very same body.
        // The 2D route rebuilds it instead - the same shape on new faces - which would turn an
        // untouched body in a timeline into a different solid. Those pairs go back to the path
        // below, which decides them, and the identity they keep, exactly as it always has.
        let unchanged = 1e-4 * (va0 + vb0);
        let rebuilt_from_an_operand = match op {
            BoolOp::Cut => v0 >= va0 - unchanged,
            BoolOp::Intersect => (v0 - va0).abs() <= unchanged || (v0 - vb0).abs() <= unchanged,
            BoolOp::Union => false,
        };
        if !rebuilt_from_an_operand && plausible(v0, op, va0, vb0, slack0) {
            return Ok(Some(body.clone()));
        }
    }
    let meshes = (a.tessellate_with(tol_m, true).ok(), b.tessellate_with(tol_m, true).ok());
    let (va, vb) = match &meshes {
        (Some(ma), Some(mb)) => (ma.measure().volume, mb.measure().volume),
        _ => (volume(a), volume(b)),
    };
    let slack = 2e-3 * (va + vb) + 1e-9;
    // Volume bounds, then membership against the operands (catches misclassified pieces), on
    // one mesh of the result. Returns (plausible, volume).
    // Shells that touch were never fused: coincident back-to-back faces leave the intersection
    // nothing to cut, so it returns both operands side by side (#396). Volume and point
    // sampling cannot see that; the shell grouping can. Only contact over an area counts
    // (bodies tangent along a line stay two lumps), and operands already like that, saved
    // before this was caught, stay joinable.
    let unfused = |r: &Body| {
        op == BoolOp::Union
            && r.shell_count() > 1
            && r.union_lumps().is_err()
            && a.union_lumps().is_ok()
            && b.union_lumps().is_ok()
            && crate::coplanar::faces_touch(a, b)
    };
    let check = |r: &Body| -> (bool, f64) {
        let Ok(mr) = r.tessellate_with(tol_m, true) else { return (false, f64::NAN) };
        let v = mr.measure().volume;
        let bounds = plausible(v, op, va, vb, slack);
        if !bounds {
            return (false, v);
        }
        if unfused(r) {
            return (false, v);
        }
        let consistent = match &meshes {
            (Some(ma), Some(mb)) => mismatch(ma, mb, &mr, op).is_none_or(|f| f < 0.004),
            _ => true,
        };
        (consistent, v)
    };
    // One body clear of or inside the other: no intersection needed.
    // Kept for the end when the facetings disagree with it: bodies that touch along curved
    // faces can pass through each other slightly, and often can't be intersected.
    let mut doubted = None;
    if let (Some(ma), Some(mb)) = &meshes
        && let Some((r, sure)) = trivial(a, b, op, ma, mb, &counts, 3.0 * tol_m)
    {
        if sure {
            return Ok(r);
        }
        doubted = Some(r);
    }
    let mut last = String::new();
    // Coincident faces make the intersection fail after many retries; push them apart first.
    // Prisms along one direction (plates with holes): exact, in 2D (so only the volume bounds
    // are checked). For a cut or an intersection this already ran above; the other operations
    // ask here, after the sampled shortcut, which is where that shortcut is cheapest.
    let prism = prism.or_else(|| (!early_prism).then(|| prism_result(a, b, op, size, &counts)).flatten());
    if let Some(result) = prism {
        match result {
            Ok(Some(body)) => {
                // Measured as the operands were: a coarser mesh of a plate with many holes
                // overstates its volume by more than the slack.
                let v = match &meshes {
                    (Some(_), Some(_)) => body.tessellate_with(tol_m, true).map(|m| m.measure().volume).unwrap_or(f64::NAN),
                    _ => volume(&body),
                };
                if plausible(v, op, va, vb, slack) {
                    return Ok(Some(body));
                }
            }
            Ok(None) => return Ok(None),
            // A failed prism attempt keeps trying the other strategies, as it did before.
            Err(_) => {}
        }
    }
    // Bodies touching along one whole face (a part joined with its mirror image).
    if op == BoolOp::Union
        && a.solid.boundaries().len() == 1
        && b.solid.boundaries().len() == 1
        && let Some(Ok(body)) = crate::coplanar::glue(a, b)
    {
        let (ok, _) = check(&body);
        if ok {
            return Ok(Some(body));
        }
    }
    // Coincident planar faces: push them apart (exactly). Tried once, before the general
    // boolean; why it failed is reported if everything else fails too.
    let mut apart_failed = String::new();
    if !APART.with(|c| c.get()) {
        APART.with(|c| c.set(true));
        let (r, diagnostics) = capture_boolean_diagnostics(|| crate::coplanar::boolean_apart(a, b, op));
        APART.with(|c| c.set(false));
        match r {
            Some(Ok(Some(body))) => {
                let (ok, v) = check(&body);
                if ok {
                    merge(diagnostics);
                    let mut event = BooleanFallback::new(BooleanStrategy::CoplanarSeparation);
                    event.face_shift_mm = size * 0.0229;
                    record(event);
                    return Ok(Some(body));
                }
                apart_failed = format!("; faces pushed apart gave an implausible volume {v:.4}");
            }
            Some(Err(e)) => apart_failed = format!("; {e}"),
            Some(Ok(None)) | None => {}
        }
    }
    // A face standing wholly inside a coincident face of the other body, back to back, and no
    // other contact (#396): exact, by topology alone.
    if op == BoolOp::Union
        && a.solid.boundaries().len() == 1
        && b.solid.boundaries().len() == 1
        && let Some(Ok(body)) = crate::coplanar::inset(a, b)
        && let (Some(ma), Some(mb)) = &meshes
        && overlap(ma, mb, &ma.inside_index(), &mb.inside_index()).is_some_and(|(b_in_a, a_in_b)| b_in_a < NONE && a_in_b < NONE)
        && check(&body).0
    {
        return Ok(Some(body));
    }
    // Bores on a common cylinder (a pad's bore around the hole it is joined over).
    if op == BoolOp::Union && !BORES.with(|c| c.get()) && crate::coplanar::has_coincident_cylinders(a, b) {
        BORES.with(|c| c.set(true));
        let (r, diagnostics) = capture_boolean_diagnostics(|| crate::coplanar::bore_union(a, b, &|body| check(body).0));
        BORES.with(|c| c.set(false));
        if let Some(body) = r {
            merge(diagnostics);
            return Ok(Some(body));
        }
    }
    // Faces back to back over part of their area: imprinted on each other and glued (exact; held
    // to the retry guard's checks all the same).
    if op == BoolOp::Union && crate::coplanar::faces_touch(a, b) {
        let accept = |body: &Body| check(body).0 && !unfused(body) && crate::boolean_guard::check_retry(a, b, op, Some(body)).is_ok();
        if let Some(body) = crate::coplanar::imprint_union(a, b, &accept) {
            return Ok(Some(body));
        }
    }
    let mut empty_votes = 0;
    let mut empty_diagnostics = crate::BooleanDiagnostics::default();
    let keep: Vec<Vec<Vec3>> = a.split_keep().iter().chain(b.split_keep()).cloned().collect();
    for (attempt, j) in JITTER.iter().enumerate() {
        let shift = mt::Vector3::new(j[0], j[1], j[2]) * (size * 0.01);
        for k in [2e-3, 5e-4] {
            let tol = (size * k).max(1e-5);
            let r = truck_boolean(a, b, op, if attempt > 0 { Some(shift) } else { None }, tol);
            match r {
                Ok((s, _)) if s.boundaries().is_empty() || s.face_iter().next().is_none() => {
                    empty_votes += 1;
                    if attempt > 0 || k != 2e-3 {
                        let mut event =
                            BooleanFallback::new(if attempt > 0 { BooleanStrategy::ShiftedOperands } else { BooleanStrategy::RefinedTolerance });
                        event.translation_mm = Vec3::new(j[0], j[1], j[2]).len() * (size * 0.01);
                        event.tolerance_mm = tol;
                        empty_diagnostics.push(event);
                    }
                    last = "empty result".into();
                }
                Ok((s, h)) => match guard("boolean result", || recorded::truck_result(s, h, size, &keep, &counts)) {
                    Ok(body) => {
                        let (ok, v) = check(&body);
                        if ok {
                            if attempt > 0 || k != 2e-3 {
                                let mut event = BooleanFallback::new(if attempt > 0 {
                                    BooleanStrategy::ShiftedOperands
                                } else {
                                    BooleanStrategy::RefinedTolerance
                                });
                                event.translation_mm = Vec3::new(j[0], j[1], j[2]).len() * (size * 0.01);
                                event.tolerance_mm = tol;
                                record(event);
                            }
                            return Ok(Some(body));
                        }
                        last = if unfused(&body) {
                            "the bodies touch along faces that could not be fused".into()
                        } else {
                            format!("implausible result volume {v:.4}")
                        };
                    }
                    Err(e) => last = e.to_string(),
                },
                Err(e) => last = e.to_string(),
            }
        }
        if empty_votes >= 2 && op != BoolOp::Union {
            merge(empty_diagnostics);
            return Ok(None);
        }
    }
    last.push_str(&apart_failed);
    // Coincident faces defeat the B-rep boolean; bodies with only planar faces have an exact
    // polygon fallback.
    match crate::polybool::planar_boolean(a, b, op) {
        Ok(Some(body)) => {
            let (ok, v) = check(&body);
            if ok {
                record(BooleanFallback::new(BooleanStrategy::PlanarFallback));
                return Ok(Some(body));
            }
            last = format!("planar fallback gave an implausible volume {v:.4}");
        }
        Ok(None) => {
            record(BooleanFallback::new(BooleanStrategy::PlanarFallback));
            return Ok(None);
        }
        Err(e) => last = format!("{last}; {e}"),
    }
    // Last resort: nudge the tool by a micron-scale screw motion (turned 2e-6 rad about a
    // skew axis and moved 2e-6 of the size), which breaks tangencies and seams lying exactly on
    // the other body. The result moves by no more than that.
    if !APART.with(|c| c.get()) {
        APART.with(|c| c.set(true));
        let c = b.solid.vertex_iter().fold(Vec3::ZERO, |acc, v| acc + from_p3(v.point())) * (1.0 / b.solid.vertex_iter().count().max(1) as f64);
        let translation = Vec3::new(0.61, -0.37, 0.71) * (size * 2e-6);
        let (r, diagnostics) = capture_boolean_diagnostics(|| {
            transform(b, translation, c, Vec3::new(0.27, 0.83, -0.49), 2e-6).and_then(|nb| Ok(boolean(a, &nb, op)?.map(|r| nudged(r, &nb, &counts))))
        });
        APART.with(|c| c.set(false));
        match r {
            Ok(Some(body)) => {
                let (ok, v) = check(&body);
                if ok {
                    merge(diagnostics);
                    let mut event = BooleanFallback::new(BooleanStrategy::ToolNudge);
                    event.translation_mm = translation.len();
                    event.rotation_rad = 2e-6;
                    record(event);
                    return Ok(Some(body));
                }
                last = format!("{last}; the nudged tool gave an implausible volume {v:.4}");
            }
            Ok(None) => {
                merge(diagnostics);
                let mut event = BooleanFallback::new(BooleanStrategy::ToolNudge);
                event.translation_mm = translation.len();
                event.rotation_rad = 2e-6;
                record(event);
                return Ok(None);
            }
            Err(e) => last = format!("{last}; nudged: {e}"),
        }
    }
    match doubted {
        Some(r) => {
            let mut event = BooleanFallback::new(BooleanStrategy::UnverifiedShortcut);
            event.tolerance_mm = 3.0 * tol_m;
            record(event);
            Ok(r)
        }
        None => Err(KernelError::Failed(format!("boolean {op:?}: {last}"))),
    }
}

/// `r`, a boolean of the target and `nb` (the tool nudged), with its history over the target and
/// the tool itself (the nudge keeps every face and edge).
pub(crate) fn nudged(r: Body, nb: &Body, counts: &[(usize, usize)]) -> Body {
    let h = recorded::through(&r, 1, nb.history());
    r.with_checked_history(h, counts)
}

/// What truck's boolean makes of `a` and `b`: the solid, and its history in our terms (see
/// [`recorded::shapeops`]). On copies of the operands moved by `shift` (when given; the result
/// is moved back), which changes the rays the boolean classifies faces by. Copies (and moved
/// copies) keep the operands' face and edge order, and so does moving the result back, so the
/// history truck records holds for the operands themselves.
pub(crate) fn truck_boolean(a: &Body, b: &Body, op: BoolOp, shift: Option<mt::Vector3>, tol: f64) -> Result<(Solid, Option<recorded::Shapeops>)> {
    guard("boolean", || {
        let (mut sa, mut sb) = (a.deep_copy(), b.deep_copy());
        if let Some(shift) = shift {
            sa = mt::builder::translated(&sa, shift);
            sb = mt::builder::translated(&sb, shift);
        }
        let tool_edges = (op == BoolOp::Cut).then(|| record::edge_origins(&sb, 1));
        let res = match op {
            BoolOp::Union => truck_shapeops::or_with_history(&sa, &sb, tol),
            BoolOp::Cut => {
                sb.not();
                truck_shapeops::and_with_history(&sa, &sb, tol)
            }
            BoolOp::Intersect => truck_shapeops::and_with_history(&sa, &sb, tol),
        };
        let (s, h) = res.ok_or_else(|| KernelError::Failed("no result".into()))?;
        let h = h.and_then(|h| recorded::shapeops(&h, &sb, tool_edges.as_ref()));
        Ok((if let Some(shift) = shift { mt::builder::translated(&s, -shift) } else { s }, h))
    })
}

/// A boolean of `a` and `b` given as their pieces (`la`, `lb`); the result's pieces in one body.
pub(crate) fn by_lumps(a: &Body, b: &Body, la: Vec<Body>, lb: Vec<Body>, op: BoolOp) -> Result<Option<Body>> {
    let boxes_meet = |x: &Body, y: &Body| -> bool {
        let (Ok(mx), Ok(my)) = (x.tessellate(x.size() * 0.05 + 1e-3), y.tessellate(y.size() * 0.05 + 1e-3)) else { return true };
        let (bx, by) = (mx.bounds(), my.bounds());
        let g = (x.size() + y.size()) * 0.01;
        bx.min.x <= by.max.x + g
            && by.min.x <= bx.max.x + g
            && bx.min.y <= by.max.y + g
            && by.min.y <= bx.max.y + g
            && bx.min.z <= by.max.z + g
            && by.min.z <= bx.max.z + g
    };
    // Each piece with its history over [a, b] (a piece's faces and edges are its body's own).
    let traced = |pieces: Vec<Body>, whole: &Body, operand: usize| -> Vec<Traced> {
        let histories = recorded::parts_of(&pieces, whole);
        pieces.into_iter().zip(histories).map(|(p, h)| (p, recorded::placed(h, operand, 2))).collect()
    };
    let (la, lb) = (traced(la, a, 0), traced(lb, b, 1));
    let pair = |(x, hx): &Traced, (y, hy): &Traced, op: BoolOp| -> Result<Option<Traced>> {
        Ok(boolean(x, y, op)?.map(|r| {
            let h = recorded::of_pair(&r, hx, hy);
            (r, h)
        }))
    };
    let mut out: Vec<Traced> = Vec::new();
    match op {
        BoolOp::Cut => {
            for x in la {
                let mut cur = Some(x);
                for y in &lb {
                    cur = match cur {
                        Some(c) if boxes_meet(&c.0, &y.0) => pair(&c, y, BoolOp::Cut)?,
                        other => other,
                    };
                }
                out.extend(cur);
            }
        }
        BoolOp::Intersect => {
            for x in &la {
                for y in &lb {
                    if boxes_meet(&x.0, &y.0)
                        && let Some(r) = pair(x, y, BoolOp::Intersect)?
                    {
                        out.push(r);
                    }
                }
            }
        }
        BoolOp::Union => {
            out = la;
            for y in lb {
                let mut cur = y;
                let mut rest = Vec::new();
                for x in out {
                    if boxes_meet(&x.0, &cur.0) {
                        match pair(&x, &cur, BoolOp::Union)? {
                            Some(u) if u.0.solid.boundaries().len() == 1 => cur = u,
                            _ => rest.push(x),
                        }
                    } else {
                        rest.push(x);
                    }
                }
                rest.push(cur);
                out = rest;
            }
        }
    }
    recorded::join(out, &record::counts(&[a, b]))
}

/// Bodies whose bounding boxes are clearly apart, as one body of several pieces, without any
/// intersection work (many identical pattern tools, #261). `None` when any two may meet, or a
/// body is a mesh: the caller unions those the usual way.
pub fn union_apart(bodies: &[Body]) -> Result<Option<Body>> {
    if bodies.len() < 2 || bodies.len() > 10_000 || bodies.iter().any(Body::is_mesh) {
        return Ok(None);
    }
    let mut boxes = Vec::with_capacity(bodies.len());
    for b in bodies {
        // A coarse mesh is enough: the chord tolerance bounds how far the true surface lies
        // outside it, and the box grows by twice that.
        let tol = (b.size() * 0.02).max(1e-3);
        let bb = b.tessellate(tol)?.bounds();
        if bb.is_empty() {
            return Ok(None);
        }
        let pad = Vec3::new(1.0, 1.0, 1.0) * (2.0 * tol);
        boxes.push(solidcraft_geom::Aabb3 { min: bb.min - pad, max: bb.max + pad });
    }
    // Sweep along x: only boxes whose x ranges overlap are compared.
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_by(|&i, &j| boxes.get(i).map(|b| b.min.x).unwrap_or(0.0).total_cmp(&boxes.get(j).map(|b| b.min.x).unwrap_or(0.0)));
    for (k, &i) in order.iter().enumerate() {
        let Some(a) = boxes.get(i) else { continue };
        for &j in order.iter().skip(k + 1) {
            let Some(b) = boxes.get(j) else { continue };
            if b.min.x > a.max.x {
                break;
            }
            if a.min.y <= b.max.y && b.min.y <= a.max.y && a.min.z <= b.max.z && b.min.z <= a.max.z {
                return Ok(None);
            }
        }
    }
    // The pieces stay as they are, side by side: each face and edge of the result is its piece's
    // own (the history over `bodies`).
    let Some(joined) = join_pieces(bodies.to_vec())? else { return Ok(None) };
    let operands: Vec<&Body> = bodies.iter().collect();
    let faces = bodies
        .iter()
        .enumerate()
        .flat_map(|(operand, b)| {
            (0..b.face_count()).map(move |face| (std::collections::BTreeSet::from([crate::history::FaceOrigin::Face { operand, face }]), true))
        })
        .collect();
    let mut edges = HashMap::new();
    for (operand, b) in bodies.iter().enumerate() {
        edges.extend(record::edge_origins(&b.solid, operand));
    }
    Ok(Some(record::traced(joined, &record::counts(&operands), Some(record::regions(faces)), Some(&edges))))
}

/// Bodies (apart from each other) as one body of several pieces.
pub(crate) fn join_pieces(pieces: Vec<Body>) -> Result<Option<Body>> {
    match pieces.len() {
        0 => Ok(None),
        1 => Ok(pieces.into_iter().next()),
        _ => {
            let shells: Vec<mt::Shell> = pieces.iter().flat_map(|p| p.solid.boundaries().clone()).collect();
            let solid = guard("pieces", || Solid::try_new(shells).map_err(|e| KernelError::Failed(e.to_string())))?;
            let protected = pieces.iter().any(|piece| piece.regularization_protected);
            Body::new(solid).map(|mut body| {
                body.regularization_protected = protected;
                Some(body)
            })
        }
    }
}

/// Determinant of the linear part of a column-major 4x4 matrix: negative means the placement
/// mirrors, which turns a solid inside out.
fn det3(m: &[[f64; 4]; 4]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// Whether the linear part is a rigid motion (rotation or reflection: no scale or shear).
fn rigid_placement(m: &[[f64; 4]; 4]) -> bool {
    let r = [Vec3::new(m[0][0], m[0][1], m[0][2]), Vec3::new(m[1][0], m[1][1], m[1][2]), Vec3::new(m[2][0], m[2][1], m[2][2])];
    (0..3).all(|i| (r[i].dot(r[i]) - 1.0).abs() < 1e-9) && r[0].dot(r[1]).abs() < 1e-9 && r[0].dot(r[2]).abs() < 1e-9 && r[1].dot(r[2]).abs() < 1e-9
}

/// Affine transform by a column-major 4x4 matrix (rotations, translations, reflections).
pub fn transform_matrix(body: &Body, m: [[f64; 4]; 4]) -> Result<Body> {
    if m.iter().flatten().any(|x| !x.is_finite()) {
        return Err(KernelError::Invalid("non-finite transform".into()));
    }
    if let Some(placed) = body.mesh_transformed(&m) {
        let history = crate::history::History::identity(&placed);
        return placed.with_history(history, &[body]);
    }
    guard("transform", || {
        let mat = mt::Matrix4::new(
            m[0][0], m[0][1], m[0][2], m[0][3], m[1][0], m[1][1], m[1][2], m[1][3], m[2][0], m[2][1], m[2][2], m[2][3], m[3][0], m[3][1], m[3][2],
            m[3][3],
        );
        // An affine placement retains face order and appearance metadata. Its edges are the
        // body's by identity: a reflection turns the solid inside out, which walks each face's
        // edges the other way and so changes the edge order.
        let sources = body.transformed_sources(m)?;
        let (solid, copies) = placed_topology_mapped(&body.solid, mat)?;
        let mut rec = record::Rebuild::default();
        rec.pieces(&copies);
        for (copy, face) in solid.face_iter().zip(body.solid.face_iter()) {
            rec.whole(copy, face);
        }
        let (faces, edges) = rec.history(&solid, &[&body.solid]).unzip();
        let counts = record::solid_counts(&[&body.solid]);
        // A rigid placement leaves every face's mesh, and so the missing-face count, as it was;
        // a mirrored one turns the solid inside out. Either way the orientation is read from the
        // matrix instead of meshing the whole copy again.
        let placed = record::build_placed(
            solid,
            &counts,
            faces,
            edges.as_ref(),
            det3(&m) < 0.0,
            rigid_placement(&m).then_some(body.coarse_missing).flatten(),
        )?;
        Ok(placed.with_color(body.color()).with_paint(body.paint().cloned()).with_face_sources(sources).inherit_regularization_protection(body))
    })
}

/// `source` moved by `matrix`, keeping its face and edge order (transforms report an identity
/// [`crate::history::History`] on that basis). Built here rather than by generic topology
/// builders: a STEP periodic face may traverse a seam edge twice, and those reject that wire
/// while cloning even though placement preserves its topology.
pub(crate) fn placed_topology(source: &Solid, matrix: mt::Matrix4) -> Result<Solid> {
    placed_topology_mapped(source, matrix).map(|(solid, _)| solid)
}

/// [`placed_topology`], with each edge of `source` (by identity) and its copy.
fn placed_topology_mapped(source: &Solid, matrix: mt::Matrix4) -> Result<(Solid, HashMap<mt::EdgeID, mt::Edge>)> {
    use mt::{Transform, Transformed};
    let mut vertices = HashMap::new();
    let mut edges = HashMap::new();
    let mut shells = Vec::new();
    let mut face_count = 0usize;
    let mut edge_visits = 0usize;
    for shell in source.boundaries() {
        let mut faces = Vec::new();
        for face in shell {
            face_count = face_count.saturating_add(1);
            if face_count > 500_000 {
                return Err(KernelError::Invalid("placement exceeds face work limit".into()));
            }
            let mut wires = Vec::new();
            for wire in face.absolute_boundaries() {
                let mut mapped = Vec::new();
                for edge in wire {
                    edge_visits = edge_visits.saturating_add(1);
                    if edge_visits > 4_000_000 {
                        return Err(KernelError::Invalid("placement exceeds edge work limit".into()));
                    }
                    if let std::collections::hash_map::Entry::Vacant(entry) = edges.entry(edge.id()) {
                        for vertex in [edge.absolute_front(), edge.absolute_back()] {
                            if let std::collections::hash_map::Entry::Vacant(entry) = vertices.entry(vertex.id()) {
                                let point = matrix.transform_point(vertex.point());
                                if !from_p3(point).is_finite() {
                                    return Err(KernelError::Invalid("placement produced nonfinite geometry".into()));
                                }
                                entry.insert(mt::Vertex::new(point));
                            }
                        }
                        let front =
                            vertices.get(&edge.absolute_front().id()).ok_or_else(|| KernelError::Failed("placement vertex missing".into()))?;
                        let back = vertices.get(&edge.absolute_back().id()).ok_or_else(|| KernelError::Failed("placement vertex missing".into()))?;
                        entry.insert(mt::Edge::new_unchecked(front, back, edge.curve().transformed(matrix)));
                    }
                    let copied = edges.get(&edge.id()).ok_or_else(|| KernelError::Failed("placement edge missing".into()))?;
                    mapped.push(if edge.orientation() { copied.clone() } else { copied.inverse() });
                }
                wires.push(mt::Wire::from(mapped));
            }
            // Wire connectivity, seam sharing and orientation are copied exactly;
            // no new topology is introduced by this geometry-only operation.
            let mut copied = mt::Face::new_unchecked(wires, face.surface().transformed(matrix));
            if !face.orientation() {
                copied.invert();
            }
            faces.push(copied);
        }
        shells.push(mt::Shell::from(faces));
    }
    Ok((Solid::new_unchecked(shells), edges))
}

#[cfg(test)]
#[test]
fn affine_placement_keeps_body_and_face_paint_attached() {
    let paint = crate::Paint { opacity: 0.4, faces: vec![crate::FacePaint { face: 0, color: [0.8, 0.2, 0.1], opacity: 0.3 }] };
    let body = crate::box_solid(Vec3::ZERO, Vec3::new(10.0, 20.0, 30.0)).unwrap().with_color(Some([0.2, 0.6, 0.8])).with_paint(Some(paint.clone()));
    let source_faces = body.faces(0.01).unwrap();
    let transform = [[0.0, 2.0, 0.0, 0.0], [-1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.5, 0.0], [30.0, 40.0, 50.0, 1.0]];
    let placed = transform_matrix(&body, transform).unwrap();
    assert_eq!(placed.color(), body.color());
    assert_eq!(placed.paint(), Some(&paint));
    let faces = placed.faces(0.01).unwrap();
    for source in source_faces {
        let target = faces.iter().find(|f| f.index == source.index).unwrap();
        let p = source.centroid;
        assert!(target.centroid.dist(Vec3::new(30.0 - p.y, 40.0 + 2.0 * p.x, 50.0 + 0.5 * p.z)) < 1e-6);
    }
}

/// Rigid transform: rotate about `axis` through `origin` by `angle` radians, then translate.
pub fn transform(body: &Body, translate: Vec3, origin: Vec3, axis: Vec3, angle: f64) -> Result<Body> {
    if !(translate.is_finite() && origin.is_finite() && angle.is_finite()) {
        return Err(KernelError::Invalid("non-finite transform".into()));
    }
    {
        // Same motion as a matrix: rotate about the line through `origin`, then translate.
        let ax = if angle.abs() > 1e-12 { axis.normalized().ok_or_else(|| KernelError::Invalid("rotation axis".into()))? } else { Vec3::Z };
        let (c, s) = (angle.cos(), angle.sin());
        let t = 1.0 - c;
        let r = [
            [t * ax.x * ax.x + c, t * ax.x * ax.y + s * ax.z, t * ax.x * ax.z - s * ax.y],
            [t * ax.x * ax.y - s * ax.z, t * ax.y * ax.y + c, t * ax.y * ax.z + s * ax.x],
            [t * ax.x * ax.z + s * ax.y, t * ax.y * ax.z - s * ax.x, t * ax.z * ax.z + c],
        ];
        let ro = Vec3::new(
            r[0][0] * origin.x + r[1][0] * origin.y + r[2][0] * origin.z,
            r[0][1] * origin.x + r[1][1] * origin.y + r[2][1] * origin.z,
            r[0][2] * origin.x + r[1][2] * origin.y + r[2][2] * origin.z,
        );
        let tr = origin - ro + translate;
        let m = [[r[0][0], r[0][1], r[0][2], 0.0], [r[1][0], r[1][1], r[1][2], 0.0], [r[2][0], r[2][1], r[2][2], 0.0], [tr.x, tr.y, tr.z, 1.0]];
        transform_matrix(body, m)
    }
}

/// Split a body with a plane: the parts on the positive and the negative side of the plane
/// (only the non-empty ones).
pub fn split_by_plane(body: &Body, plane: &solidcraft_geom::Plane) -> Result<Vec<Body>> {
    let mut out = Vec::new();
    for positive in [true, false] {
        out.extend(half_space(body, plane, positive)?);
    }
    Ok(out)
}

/// The part of a body on one side of a plane (`None` when nothing is there): its intersection
/// with [`half_box`] (whose history it carries, over the body and the box).
pub(crate) fn half_space(body: &Body, plane: &solidcraft_geom::Plane, positive: bool) -> Result<Option<Body>> {
    boolean(body, &half_box(body, plane, positive)?, BoolOp::Intersect)
}

/// A box covering everything of `body` on one side of a plane, its start cap in the plane.
pub(crate) fn half_box(body: &Body, plane: &solidcraft_geom::Plane, positive: bool) -> Result<Body> {
    body.require_brep("split")?;
    let size = body.size();
    let mut bb = solidcraft_geom::Aabb3::EMPTY;
    for v in body.solid.vertex_iter() {
        bb.add(crate::body::from_p3(v.point()));
    }
    let reach = (bb.diagonal() + plane.origin.dist(bb.center())) * 2.0 + size;
    let c = plane.to_local(bb.center());
    let sq = solidcraft_geom::Region2 {
        outer: solidcraft_geom::Loop2::polygon(&[
            solidcraft_geom::Vec2::new(c.x - reach, c.y - reach),
            solidcraft_geom::Vec2::new(c.x + reach, c.y - reach),
            solidcraft_geom::Vec2::new(c.x + reach, c.y + reach),
            solidcraft_geom::Vec2::new(c.x - reach, c.y + reach),
        ]),
        holes: vec![],
    };
    let (lo, hi) = if positive { (0.0, reach) } else { (-reach, 0.0) };
    crate::build::extrude(plane, std::slice::from_ref(&sq), lo, hi)?.pop().ok_or_else(|| KernelError::Failed("half space".into()))
}

#[cfg(test)]
#[path = "prism_fast_path_tests.rs"]
mod prism_fast_path_tests;

#[cfg(test)]
#[path = "placement_tests.rs"]
mod placement_tests;
