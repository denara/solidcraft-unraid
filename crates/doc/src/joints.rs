//! Joints between occurrences: joint origins (snap frames on component geometry, re-found on
//! every solve so they follow edits), joints (rigid, revolute, slider, cylindrical, pin-slot,
//! planar, ball) with their motion values and limits, motion links, and the solver that places
//! occurrences from the joint graph.
//!
//! A joint makes B's joint frame coincide with A's frame moved by the joint's motion:
//! `T_B · F_b = T_A · F_a · M(q) · Flip`, so `T_B = T_A · F_a · M(q) · Flip · F_b⁻¹`, where `T`
//! are occurrence world transforms, `F` the joint frames in each component's own frame, and `q`
//! the joint's values (angles in radians, distances in mm). The solver walks the graph from
//! grounded occurrences (and the root, which never moves). Joints that close a loop (a four-bar
//! linkage, a slider-crank) are met by moving the values of the other joints around the loop;
//! those that still disagree are reported.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};
use solidcraft_geom::Vec3;

use solidcraft_sketch::linalg;

use crate::{DocError, Document, IDENTITY, Mat, ModelState, Result, apply_point, apply_vector, mat_inverse, mat_mul};

/// Joints the solver (and the URDF export) reads at most.
pub const MAX_JOINTS: usize = 10_000;

/// Where a joint origin sits on a component's geometry (component coordinates).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Snap {
    /// An explicit frame (origin, z axis, x axis).
    Frame {
        origin: Vec3,
        z: Vec3,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        x: Option<Vec3>,
    },
    /// The centre of the planar face nearest `pick`, z along its outward normal. `face` is the
    /// face's persistent name (found first).
    FaceCenter {
        pick: Vec3,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        face: Option<String>,
    },
    /// The centre of the circular end nearest `pick` of the cylindrical face there, z along its
    /// axis (pointing out of the face's end).
    CircleCenter {
        pick: Vec3,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        face: Option<String>,
    },
    /// A point, z along `z` (default +Z).
    Point {
        point: Vec3,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        z: Option<Vec3>,
    },
}

/// A joint origin: a snap on an occurrence's component (occurrence 0 = the root design).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JointOrigin {
    pub occurrence: u64,
    pub snap: Snap,
}

/// A named joint origin (Joint Origin command), referenced by joints by name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NamedOrigin {
    pub name: String,
    pub origin: JointOrigin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JointKind {
    Rigid,
    Revolute,
    Slider,
    Cylindrical,
    PinSlot,
    Planar,
    Ball,
}

impl JointKind {
    pub fn parse(s: &str) -> Option<JointKind> {
        Some(match s.to_ascii_lowercase().replace([' ', '_', '-'], "").as_str() {
            "rigid" => JointKind::Rigid,
            "revolute" | "rotate" => JointKind::Revolute,
            "slider" | "slide" => JointKind::Slider,
            "cylindrical" => JointKind::Cylindrical,
            "pinslot" => JointKind::PinSlot,
            "planar" => JointKind::Planar,
            "ball" => JointKind::Ball,
            _ => return None,
        })
    }
    /// Names of the joint's values, in order (angles in radians, distances in mm).
    pub fn dofs(self) -> &'static [&'static str] {
        match self {
            JointKind::Rigid => &[],
            JointKind::Revolute => &["angle"],
            JointKind::Slider => &["distance"],
            JointKind::Cylindrical => &["angle", "distance"],
            JointKind::PinSlot => &["angle", "slide"],
            JointKind::Planar => &["x", "y", "angle"],
            JointKind::Ball => &["yaw", "pitch", "roll"],
        }
    }
    pub fn is_angle(self, i: usize) -> bool {
        matches!(self.dofs().get(i), Some(&("angle" | "yaw" | "pitch" | "roll")))
    }
}

/// A joint between two occurrences.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Joint {
    pub id: u64,
    pub name: String,
    pub kind: JointKind,
    pub a: JointOrigin,
    pub b: JointOrigin,
    /// Current values of the joint's degrees of freedom.
    #[serde(default)]
    pub values: Vec<f64>,
    /// Per value: (min, max) limits.
    #[serde(default)]
    pub limits: Vec<Option<(f64, f64)>>,
    /// Don't mate: B's z along A's instead of against it (by default the origins mate, as two
    /// faces meeting).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub flip: bool,
    /// Distance from A's frame to B's along z, and a turn about it (radians).
    #[serde(default)]
    pub offset: f64,
    #[serde(default)]
    pub angle: f64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub suppressed: bool,
}

/// Value of joint `b` (index `ib`) = ratio · value of joint `a` (index `ia`) + offset.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MotionLink {
    pub a: u64,
    #[serde(default)]
    pub ia: usize,
    pub b: u64,
    #[serde(default)]
    pub ib: usize,
    pub ratio: f64,
    #[serde(default)]
    pub offset: f64,
}

/// Joints, joint origins and motion links of a design.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Assembly {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub joints: Vec<Joint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub origins: Vec<NamedOrigin>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<MotionLink>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contact_sets: Vec<crate::motion::ContactSet>,
    /// Contact sets take effect.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub contact_enabled: bool,
    /// Every occurrence is in contact with every other (Enable All Contact).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub contact_all: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub studies: Vec<crate::motion::MotionStudy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exploded: Vec<crate::motion::ExplodedView>,
}

impl Assembly {
    pub fn is_empty(&self) -> bool {
        self.joints.is_empty()
            && self.origins.is_empty()
            && self.links.is_empty()
            && self.contact_sets.is_empty()
            && !self.contact_enabled
            && !self.contact_all
            && self.studies.is_empty()
            && self.exploded.is_empty()
    }
}

/// A frame: origin and orthonormal axes, as a transform (local → parent).
fn frame(o: Vec3, z: Vec3, x: Option<Vec3>) -> Option<Mat> {
    let z = z.normalized()?;
    let x = x.and_then(|x| (x - z * x.dot(z)).normalized()).unwrap_or_else(|| z.any_perp());
    let y = z.cross(x).normalized()?;
    Some([[x.x, x.y, x.z, 0.0], [y.x, y.y, y.z, 0.0], [z.x, z.y, z.z, 0.0], [o.x, o.y, o.z, 1.0]])
}

fn rot_z(a: f64) -> Mat {
    let (s, c) = a.sin_cos();
    [[c, s, 0.0, 0.0], [-s, c, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]
}
fn rot_y(a: f64) -> Mat {
    let (s, c) = a.sin_cos();
    [[c, 0.0, -s, 0.0], [0.0, 1.0, 0.0, 0.0], [s, 0.0, c, 0.0], [0.0, 0.0, 0.0, 1.0]]
}
fn shift(x: f64, y: f64, z: f64) -> Mat {
    let mut m = IDENTITY;
    m[3] = [x, y, z, 1.0];
    m
}

/// The joint's motion in A's frame for its values.
pub fn motion(kind: JointKind, q: &[f64]) -> Mat {
    let v = |i: usize| q.get(i).copied().unwrap_or(0.0);
    match kind {
        JointKind::Rigid => IDENTITY,
        JointKind::Revolute => rot_z(v(0)),
        JointKind::Slider => shift(0.0, 0.0, v(0)),
        JointKind::Cylindrical => mat_mul(&rot_z(v(0)), &shift(0.0, 0.0, v(1))),
        JointKind::PinSlot => mat_mul(&shift(v(1), 0.0, 0.0), &rot_z(v(0))),
        JointKind::Planar => mat_mul(&shift(v(0), v(1), 0.0), &rot_z(v(2))),
        JointKind::Ball => mat_mul(&mat_mul(&rot_z(v(0)), &rot_y(v(1))), &rot_z(v(2))),
    }
}

/// Bodies of a component in the model (its own frame).
fn component_bodies<'a>(doc: &Document, st: &'a ModelState, component: u64) -> Vec<&'a crate::ModelBody> {
    st.bodies.iter().filter(|b| doc.model_body_component(b) == component).collect()
}

/// The component an occurrence places (0 for the root).
fn component_of(doc: &Document, occ: u64) -> Result<u64> {
    if occ == 0 {
        return Ok(0);
    }
    doc.occurrences.iter().find(|o| o.id == occ).map(|o| o.component).ok_or_else(|| DocError::Unknown(format!("occurrence {occ}")))
}

/// Resolve a joint origin to a frame in its component's coordinates.
/// A snap's pick point moved onto its named face (an error when the face is gone: a joint must
/// not quietly attach elsewhere).
fn named_pick(st: &ModelState, pick: Vec3, face: Option<&str>) -> Result<Vec3> {
    match face {
        Some(n) => crate::naming::point_on_face(st, n, pick)
            .map(|(q, _)| q)
            .ok_or_else(|| DocError::Invalid(format!("joint origin: the face `{n}` no longer exists (edit the joint to pick again)"))),
        None => Ok(pick),
    }
}

pub fn resolve_origin(doc: &Document, st: &ModelState, o: &JointOrigin) -> Result<Mat> {
    let bad = || DocError::Invalid("joint origin: degenerate frame".into());
    match &o.snap {
        Snap::Frame { origin, z, x } => frame(*origin, *z, *x).ok_or_else(bad),
        Snap::Point { point, z } => frame(*point, z.unwrap_or(Vec3::Z), None).ok_or_else(bad),
        Snap::FaceCenter { pick, face } => {
            let pick = &named_pick(st, *pick, face.as_deref())?;
            let comp = component_of(doc, o.occurrence)?;
            let mut best: Option<(f64, Vec3, Vec3)> = None;
            for b in component_bodies(doc, st, comp) {
                let tol = (b.body.size() * 2e-3).max(1e-3);
                for f in b.body.faces(tol)? {
                    let Some(n) = f.plane_normal else { continue };
                    let d = (*pick - f.centroid).dot(n).abs() + 1e-3 * (*pick - f.centroid).len();
                    if best.as_ref().is_none_or(|(bd, _, _)| d < *bd) {
                        best = Some((d, f.centroid, n));
                    }
                }
            }
            let (_, c, n) = best.ok_or_else(|| DocError::Invalid("joint origin: no planar face there".into()))?;
            frame(c, n, None).ok_or_else(bad)
        }
        Snap::CircleCenter { pick, face } => {
            let pick = &named_pick(st, *pick, face.as_deref())?;
            let comp = component_of(doc, o.occurrence)?;
            let cyl = component_bodies(doc, st, comp)
                .into_iter()
                .filter_map(|b| solidcraft_kernel::cylinder_face_at(&b.body, *pick))
                .min_by(|a, b| {
                    let off = |c: &solidcraft_kernel::CylinderFace| {
                        let v = *pick - c.axis_point;
                        ((v - c.axis * v.dot(c.axis)).len() - c.radius).abs()
                    };
                    off(a).total_cmp(&off(b))
                })
                .ok_or_else(|| DocError::Invalid("joint origin: no cylindrical face there".into()))?;
            let s = (*pick - cyl.axis_point).dot(cyl.axis);
            let (at, dir) = if (s - cyl.start).abs() <= (cyl.end - s).abs() { (cyl.start, -1.0) } else { (cyl.end, 1.0) };
            frame(cyl.axis_point + cyl.axis * at, cyl.axis * dir, None).ok_or_else(bad)
        }
    }
}

/// World transform of an occurrence (through its parents).
pub fn occurrence_world(doc: &Document, occ: u64) -> Mat {
    let mut m = IDENTITY;
    let mut cur = doc.occurrences.iter().find(|o| o.id == occ);
    for _ in 0..1000 {
        let Some(o) = cur else { break };
        m = mat_mul(&o.transform, &m);
        if o.parent == 0 {
            break;
        }
        cur = doc.occurrence_of(o.parent);
    }
    m
}

/// Clamp values to the joint's limits.
pub fn clamp(j: &Joint, q: &mut [f64]) {
    for (i, v) in q.iter_mut().enumerate() {
        if let Some(Some((lo, hi))) = j.limits.get(i) {
            *v = v.clamp(lo.min(*hi), hi.max(*lo));
        }
    }
}

/// The result of placing occurrences by their joints.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Solution {
    /// New world transforms of occurrences the joints place.
    #[serde(skip)]
    pub world: BTreeMap<u64, Mat>,
    /// New values of the joints that moved to close loops (joint id → values).
    #[serde(skip)]
    pub values: BTreeMap<u64, Vec<f64>>,
    /// Joints that disagree with the placement found through others: (joint id, name, why).
    pub conflicts: Vec<(u64, String, String)>,
    /// Joints that could not be applied (bad origin…): (joint id, name, why).
    pub errors: Vec<(u64, String, String)>,
    /// Remaining degrees of freedom per occurrence (6 = free, 0 = fixed).
    pub dof: BTreeMap<u64, usize>,
    /// Occurrences fixed in place (grounded, or the root's).
    pub fixed: Vec<u64>,
}

/// Joint values by joint id.
type Values = BTreeMap<u64, Vec<f64>>;

/// How far two placements differ: (translation mm, rotation rad).
fn mismatch(a: &Mat, b: &Mat) -> (f64, f64) {
    let t = Vec3::new(a[3][0] - b[3][0], a[3][1] - b[3][1], a[3][2] - b[3][2]).len();
    // The turn between them from both its cosine and its sine: the cosine alone loses small
    // angles to rounding.
    let c = |i: usize, k: usize| (0..3).map(|n| a[i][n] * b[k][n]).sum::<f64>();
    let sin2 = Vec3::new(c(1, 2) - c(2, 1), c(2, 0) - c(0, 2), c(0, 1) - c(1, 0)).len();
    (t, sin2.atan2(c(0, 0) + c(1, 1) + c(2, 2) - 1.0))
}

/// Passes over the motion links at most: a chain of up to this many links carries its value
/// through, whatever order its links are stored in.
const MAX_LINK_PASSES: usize = 64;

/// Motion links, pass after pass until nothing changes (a chain stored from its end takes a pass
/// per link), each linked value within its joint's limits as it is set. The result is the same
/// whatever the values were before and however the links are stored, so applying them again
/// changes nothing (links that form a cycle stop after `MAX_LINK_PASSES`).
fn apply_links(asm: &Assembly, vals: &mut Values) {
    let limits: BTreeMap<(u64, usize), (f64, f64)> = asm
        .links
        .iter()
        .filter_map(|l| {
            let (lo, hi) = asm.joints.iter().find(|j| j.id == l.b)?.limits.get(l.ib).copied().flatten()?;
            Some(((l.b, l.ib), (lo.min(hi), hi.max(lo))))
        })
        .collect();
    // The linked values, to see whether a pass changed any (several links writing one value
    // change it within a pass, but settle from one pass to the next).
    let linked = |vals: &Values| -> Vec<Option<f64>> { asm.links.iter().map(|l| vals.get(&l.b).and_then(|q| q.get(l.ib)).copied()).collect() };
    for _ in 0..MAX_LINK_PASSES {
        let before = linked(vals);
        for l in &asm.links {
            let Some(src) = vals.get(&l.a).and_then(|q| q.get(l.ia)).copied() else { continue };
            let mut next = l.ratio * src + l.offset;
            if let Some((lo, hi)) = limits.get(&(l.b, l.ib)) {
                next = next.max(*lo).min(*hi);
            }
            if let Some(v) = vals.get_mut(&l.b).and_then(|q| q.get_mut(l.ib)) {
                *v = next;
            }
        }
        if linked(vals) == before {
            break;
        }
    }
}

/// Effective values of every joint after motion links and limits.
pub fn joint_values(asm: &Assembly) -> BTreeMap<u64, Vec<f64>> {
    let mut vals: Values = asm
        .joints
        .iter()
        .map(|j| {
            let mut q = j.values.clone();
            q.resize(j.kind.dofs().len(), 0.0);
            (j.id, q)
        })
        .collect();
    apply_links(asm, &mut vals);
    for j in &asm.joints {
        if let Some(q) = vals.get_mut(&j.id) {
            clamp(j, q);
        }
    }
    vals
}

/// The turn and offset from A's joint frame to B's: mated (B's z against A's, as faces meet)
/// unless flipped.
fn flip_of(j: &Joint) -> Mat {
    let mut m = rot_z(j.angle);
    m[3][2] = j.offset;
    if !j.flip { mat_mul(&m, &[[1.0, 0.0, 0.0, 0.0], [0.0, -1.0, 0.0, 0.0], [0.0, 0.0, -1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]) } else { m }
}

/// B's world transform from A's (or A's from B's, going the other way) for the joint's values.
fn through(j: &Joint, (fa, fb): &(Mat, Mat), q: &[f64], from_a: bool, t: &Mat) -> Option<Mat> {
    let link = mat_mul(&mat_mul(&motion(j.kind, q), &flip_of(j)), &mat_inverse(fb)?);
    if from_a {
        Some(mat_mul(&mat_mul(t, fa), &link))
    } else {
        // T_A = T_B · link⁻¹ · F_a⁻¹
        Some(mat_mul(&mat_mul(t, &mat_inverse(&link)?), &mat_inverse(fa)?))
    }
}

/// The spanning tree the solver places occurrences along, out from the fixed ones.
struct Tree<'a> {
    joints: BTreeMap<u64, &'a Joint>,
    /// Joint frames in component coordinates (A's, B's).
    frames: BTreeMap<u64, (Mat, Mat)>,
    fixed: BTreeMap<u64, Mat>,
    /// Placed occurrence → (the occurrence it is placed from, the joint, from the joint's A side).
    parent: BTreeMap<u64, (u64, u64, bool)>,
    /// Placed occurrences in placement order.
    order: Vec<u64>,
}

impl Tree<'_> {
    fn step(&self, joint: u64, from_a: bool, t: &Mat, vals: &Values) -> Option<Mat> {
        let q = vals.get(&joint).map(Vec::as_slice).unwrap_or(&[]);
        through(self.joints.get(&joint)?, self.frames.get(&joint)?, q, from_a, t)
    }

    /// Where every placed occurrence is for these values.
    fn place(&self, vals: &Values) -> BTreeMap<u64, Mat> {
        let mut placed = self.fixed.clone();
        for occ in &self.order {
            let Some(&(from, joint, from_a)) = self.parent.get(occ) else { continue };
            if let Some(m) = placed.get(&from).and_then(|t| self.step(joint, from_a, t, vals)) {
                placed.insert(*occ, m);
            }
        }
        placed
    }

    /// The fixed occurrence an occurrence is placed from, and the tree joints from it up to that one.
    fn path(&self, mut occ: u64) -> (u64, Vec<(u64, bool)>) {
        let mut up = Vec::new();
        while let Some(&(from, joint, from_a)) = self.parent.get(&occ) {
            if up.len() > self.order.len() {
                break;
            }
            up.push((joint, from_a));
            occ = from;
        }
        (occ, up)
    }

    /// One occurrence's world placement for these values.
    fn world(&self, occ: u64, vals: &Values) -> Option<Mat> {
        let (root, up) = self.path(occ);
        let mut t = *self.fixed.get(&root)?;
        for (joint, from_a) in up.iter().rev() {
            t = self.step(*joint, *from_a, &t, vals)?;
        }
        Some(t)
    }
}

/// Values the loop solver moves at most in one group of loops, and loop-closing joints it takes
/// on at most: larger ones are only checked.
const MAX_LOOP_VALUES: usize = 64;
const MAX_LOOPS: usize = 100;
/// Residual evaluations one solve may spend on all its loops.
const MAX_LOOP_WORK: usize = 200_000;

/// Held joint values by (joint, value index): the value each moves from.
pub type Held = BTreeMap<(u64, usize), f64>;

/// The values that set the `around` joints' values through motion links, followed back through
/// every link to them (several links may write one value; `apply_links`' order decides which
/// wins on the way) for as many links as `apply_links` passes: all the values reached, and those
/// that no link sets (the ones a solve can move).
fn link_chains(asm: &Assembly, tree: &Tree, around: &BTreeSet<u64>, linked: &BTreeSet<(u64, usize)>) -> (BTreeSet<(u64, usize)>, Vec<(u64, usize)>) {
    let (mut feeds, mut starts) = (BTreeSet::new(), Vec::new());
    let mut frontier: BTreeSet<(u64, usize)> =
        around.iter().filter_map(|id| tree.joints.get(id)).flat_map(|j| (0..j.kind.dofs().len()).map(|i| (j.id, i))).collect();
    let mut seen = frontier.clone();
    for _ in 0..MAX_LINK_PASSES {
        let next: BTreeSet<(u64, usize)> =
            asm.links.iter().filter(|l| frontier.contains(&(l.b, l.ib))).map(|l| (l.a, l.ia)).filter(|v| !seen.contains(v)).collect();
        if next.is_empty() {
            break;
        }
        for &(id, i) in &next {
            feeds.insert((id, i));
            if !linked.contains(&(id, i)) && tree.joints.get(&id).is_some_and(|j| i < j.kind.dofs().len()) {
                starts.push((id, i));
            }
        }
        seen.extend(next.iter().copied());
        frontier = next;
    }
    (feeds, starts)
}

/// The held values among `touches` that move: (joint, index, from, to), `from` within the
/// joint's limits. A suppressed joint still sets values through its motion links.
fn held_moves(asm: &Assembly, held: &Held, touches: &BTreeSet<(u64, usize)>, vals: &Values) -> Vec<(u64, usize, f64, f64)> {
    let mut moves = Vec::new();
    for ((id, i), from) in held.iter().filter(|(v, _)| touches.contains(v)) {
        let (Some(j), Some(to)) = (asm.joints.iter().find(|j| j.id == *id), vals.get(id).and_then(|q| q.get(*i))) else { continue };
        let from = match j.limits.get(*i).copied().flatten() {
            Some((lo, hi)) => from.max(lo.min(hi)).min(hi.max(lo)),
            None => *from,
        };
        if from != *to {
            moves.push((*id, *i, from, *to));
        }
    }
    moves
}

/// Close the loops, each group of loops that share joints on its own: see `close_group`.
/// Returns the moved joints' new stored values (and updates `vals`), and the closing joints of
/// the groups whose held values moved but which can't stay closed on the way (nothing in those
/// moves; a group that can't close without moving is left to the conflict check).
fn close_loops(tree: &Tree, closing: &[u64], asm: &Assembly, held: &Held, vals: &mut Values) -> (Values, BTreeSet<u64>) {
    let (mut changed, mut stuck) = (BTreeMap::new(), BTreeSet::new());
    if closing.len() > MAX_LOOPS {
        return (Values::new(), stuck);
    }
    let linked: BTreeSet<(u64, usize)> = asm.links.iter().map(|l| (l.b, l.ib)).collect();
    let groups = loop_groups(tree, closing, asm, &linked);
    let spent = std::cell::Cell::new(0usize);
    for (group, around, touches) in &groups {
        let moves = held_moves(asm, held, touches, vals);
        match close_group(tree, group, around, &moves, asm, held, &linked, &spent, vals) {
            Some(m) => changed.extend(m),
            None if !moves.is_empty() => stuck.extend(group.iter().copied()),
            None => {}
        }
    }
    // The stored values with what each group moved (groups may move different values of one
    // joint).
    let mut moved = Values::new();
    for ((id, i), v) in changed {
        let Some(j) = tree.joints.get(&id) else { continue };
        let q = moved.entry(id).or_insert_with(|| {
            let mut q = j.values.clone();
            q.resize(j.kind.dofs().len(), 0.0);
            q
        });
        if let Some(slot) = q.get_mut(i) {
            *slot = v;
        }
    }
    (moved, stuck)
}

/// A group of loops solved together: its closing joints, the joints around them, and the values
/// it touches.
type Group = (Vec<u64>, BTreeSet<u64>, BTreeSet<(u64, usize)>);

/// The loops' groups. Each loop: its closing joint and the joints on the tree paths between its
/// sides; loops that share a value (of a joint, or one feeding them through links) are solved
/// together.
fn loop_groups(tree: &Tree, closing: &[u64], asm: &Assembly, linked: &BTreeSet<(u64, usize)>) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for id in closing {
        let Some(j) = tree.joints.get(id) else { continue };
        // A joint between an occurrence and itself (only a hand-edited file has one) is only checked.
        if j.a.occurrence == j.b.occurrence {
            continue;
        }
        let side = |occ: u64| -> BTreeSet<u64> { tree.path(occ).1.iter().map(|(k, _)| *k).collect() };
        let mut around: BTreeSet<u64> = side(j.a.occurrence).symmetric_difference(&side(j.b.occurrence)).copied().collect();
        around.insert(*id);
        // The values the loop depends on: its joints' and those its links read. Loops tie together
        // through a shared value only (a rigid joint on both paths has none; two values of one
        // joint can feed two independent loops).
        let mut touches = link_chains(asm, tree, &around, linked).0;
        touches.extend(around.iter().filter_map(|id| tree.joints.get(id)).flat_map(|j| (0..j.kind.dofs().len()).map(|i| (j.id, i))));
        let mut members = vec![*id];
        groups.retain(|(c, a, t)| {
            if t.is_disjoint(&touches) {
                return true;
            }
            members.extend(c.iter().copied());
            around.extend(a.iter().copied());
            touches.extend(t.iter().copied());
            false
        });
        groups.push((members, around, touches));
    }
    groups
}

/// The values a solve of the loops `around` moves: every value of their joints but the `held`
/// ones and motion-link targets, and the values the targets' link chains start from.
fn unknowns(tree: &Tree, around: &BTreeSet<u64>, asm: &Assembly, linked: &BTreeSet<(u64, usize)>, held: &Held) -> Vec<(u64, usize)> {
    let mut vars: Vec<(u64, usize)> = Vec::new();
    for j in around.iter().filter_map(|id| tree.joints.get(id)) {
        vars.extend((0..j.kind.dofs().len()).map(|i| (j.id, i)).filter(|v| !linked.contains(v) && !held.contains_key(v)));
    }
    for v in link_chains(asm, tree, around, linked).1 {
        if !held.contains_key(&v) && !vars.contains(&v) {
            vars.push(v);
        }
    }
    vars
}

/// The loops' size: the diagonal of the box around their joints' A frames, 1 mm to 1 km. Points
/// that far apart weigh turns like moves.
fn loop_scale(tree: &Tree, around: &BTreeSet<u64>, vals: &Values) -> f64 {
    let (mut lo, mut hi) = (Vec3::new(f64::MAX, f64::MAX, f64::MAX), Vec3::new(f64::MIN, f64::MIN, f64::MIN));
    for id in around {
        let (Some(j), Some((fa, _))) = (tree.joints.get(id), tree.frames.get(id)) else { continue };
        let Some(t) = tree.world(j.a.occurrence, vals) else { continue };
        let p = apply_point(&mat_mul(&t, fa), Vec3::ZERO);
        lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
        hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
    }
    if lo.x <= hi.x { (hi - lo).len().clamp(1.0, 1e6) } else { 1.0 }
}

/// How far each closing joint is from meeting: three points of B's joint frame, `scale` apart,
/// where the joint wants them against where the tree puts them (appended to `out`).
fn closure_gaps(tree: &Tree, closing: &[u64], vals: &Values, scale: f64, out: &mut Vec<f64>) -> Option<()> {
    for id in closing {
        let (j, (_, fb)) = (tree.joints.get(id)?, tree.frames.get(id)?);
        let tb = tree.world(j.b.occurrence, vals)?;
        let want = tree.step(*id, true, &tree.world(j.a.occurrence, vals)?, vals)?;
        for p in [Vec3::ZERO, Vec3::X * scale, Vec3::Y * scale] {
            let at = apply_point(fb, p);
            let d = apply_point(&want, at) - apply_point(&tb, at);
            out.extend([d.x, d.y, d.z]);
        }
    }
    Some(())
}

/// Occurrences whose degrees of freedom are worked out through their loops, at most; the rest
/// keep the tree's count.
const MAX_DOF_OCCURRENCES: usize = 256;

/// The degrees of freedom of the occurrences loops constrain: how many independent ways each can
/// still move with every loop kept closed, the rank of its placement over the joint values once
/// the loops' closure is taken out (rank [J_loops; J_placement] - rank J_loops). An occurrence's
/// values are those on its tree path and the unknowns of the loops on that path. The tree's
/// count stands where a loop group is over the solver's caps or the work runs out.
fn loop_dof(tree: &Tree, closing: &[u64], asm: &Assembly, vals: &Values, dof: &mut BTreeMap<u64, usize>) {
    if closing.len() > MAX_LOOPS {
        return;
    }
    let linked: BTreeSet<(u64, usize)> = asm.links.iter().map(|l| (l.b, l.ib)).collect();
    let groups = loop_groups(tree, closing, asm, &linked);
    // The joints loops depend on: those around them, those whose values they read, and those a
    // motion link carries any of these values into.
    let mut relevant: BTreeSet<u64> = groups.iter().flat_map(|(_, a, t)| a.iter().copied().chain(t.iter().map(|(j, _)| *j))).collect();
    for _ in 0..MAX_LINK_PASSES {
        let more: Vec<u64> = asm.links.iter().filter(|l| relevant.contains(&l.a) && !relevant.contains(&l.b)).map(|l| l.b).collect();
        if more.is_empty() {
            break;
        }
        relevant.extend(more);
    }
    // The occurrences a loop may hold or move: placed by such a joint, or below one that is
    // (worked out down the tree, parents first). The others keep the tree's count.
    let mut candidates: BTreeSet<u64> = BTreeSet::new();
    for occ in &tree.order {
        if let Some(&(from, joint, _)) = tree.parent.get(occ)
            && (relevant.contains(&joint) || candidates.contains(&from))
        {
            candidates.insert(*occ);
        }
    }
    let (mut spent, mut counted) = (0usize, 0usize);
    for occ in tree.order.iter().filter(|o| candidates.contains(o)) {
        let path: Vec<u64> = tree.path(*occ).1.iter().map(|(k, _)| *k).collect();
        let (mut values, mut have): (Vec<(u64, usize)>, BTreeSet<(u64, usize)>) = (Vec::new(), BTreeSet::new());
        let mut add = |values: &mut Vec<(u64, usize)>, v: (u64, usize)| {
            if have.insert(v) {
                values.push(v);
            }
        };
        for j in path.iter().filter_map(|k| tree.joints.get(k)) {
            for v in (0..j.kind.dofs().len()).map(|i| (j.id, i)).filter(|v| !linked.contains(v)) {
                add(&mut values, v);
            }
            if values.len() > MAX_LOOP_VALUES {
                break;
            }
        }
        if values.len() > MAX_LOOP_VALUES {
            continue;
        }
        // A value on the path a motion link sets moves with the values its link chain starts from.
        let on_path: BTreeSet<u64> = path.iter().copied().collect();
        for v in link_chains(asm, tree, &on_path, &linked).1 {
            add(&mut values, v);
        }
        // The loops on the path, and every loop whose values those reach (a linked source may be
        // held by a loop of its own), each once.
        let (mut closers, mut around) = (Vec::new(), BTreeSet::new());
        let mut taken: BTreeSet<usize> = BTreeSet::new();
        loop {
            let more: Vec<usize> = groups
                .iter()
                .enumerate()
                .filter(|(g, _)| !taken.contains(g))
                .filter(|(_, (_, a, t))| path.iter().any(|k| a.contains(k)) || t.iter().any(|v| values.contains(v)))
                .map(|(g, _)| g)
                .collect();
            if more.is_empty() {
                break;
            }
            for (c, a, _) in more.iter().filter_map(|g| groups.get(*g)) {
                closers.extend(c.iter().copied());
                around.extend(a.iter().copied());
                for v in unknowns(tree, a, asm, &linked, &Held::new()) {
                    add(&mut values, v);
                }
            }
            taken.extend(more);
        }
        // No loop reaches this occurrence's values (not even through a motion link it drives):
        // the tree's count stands.
        if taken.is_empty() {
            continue;
        }
        counted += 1;
        if counted > MAX_DOF_OCCURRENCES || spent > MAX_LOOP_WORK {
            return;
        }
        if values.len() > MAX_LOOP_VALUES {
            continue;
        }
        let scale = loop_scale(tree, &around, vals);
        let mut work = vals.clone();
        // The loops' gaps, then three points of the occurrence `scale` apart.
        let mut f = |x: &[f64]| -> Option<Vec<f64>> {
            spent += 1;
            for ((id, i), v) in values.iter().zip(x) {
                if let Some(slot) = work.get_mut(id).and_then(|q| q.get_mut(*i)) {
                    *slot = *v;
                }
            }
            apply_links(asm, &mut work);
            let mut out = Vec::with_capacity(closers.len() * 9 + 9);
            closure_gaps(tree, &closers, &work, scale, &mut out)?;
            let t = tree.world(*occ, &work)?;
            for p in [Vec3::ZERO, Vec3::X * scale, Vec3::Y * scale] {
                let q = apply_point(&t, p);
                out.extend([q.x, q.y, q.z]);
            }
            out.iter().all(|v| v.is_finite()).then_some(out)
        };
        let x0: Vec<f64> = values.iter().map(|(id, i)| vals.get(id).and_then(|q| q.get(*i)).copied().unwrap_or(0.0)).collect();
        let (n, gaps) = (values.len(), closers.len() * 9);
        let Some(jac) = jacobian(&mut f, &x0, gaps + 9) else { continue };
        let rank = |rows: usize| {
            let mut a = linalg::Mat::zeros(rows, n);
            let mut big = 0.0f64;
            for r in 0..rows {
                for c in 0..n {
                    let v = jac.get(r * n + c).copied().unwrap_or(0.0);
                    big = big.max(v.abs());
                    a.set(r, c, v);
                }
            }
            linalg::rref(&mut a, 1e-6 * big.max(1.0)).len()
        };
        let free = rank(gaps + 9).saturating_sub(rank(gaps));
        dof.insert(*occ, free.min(6));
    }
}

/// Close one group of loops: move the values of the joints `around` them (all but the `held`
/// values, motion-link targets and rigid joints, within their limits; a linked value moves
/// through the start of its link chain) until every loop-closing joint meets. A loop open at the
/// start is closed first, from where it is. Then the held values `moves` go from the values they
/// are held from to their current ones in small steps, each solved from the last, so the mechanism
/// stays on its branch (an open four-bar does not flip crossed). Returns the values that changed
/// from what is stored, by (joint, index), and updates `vals` (empty when there is nothing to do
/// or the group is too big to solve), or `None` when the loops can't close, or can't stay closed
/// on the way.
fn close_group(
    tree: &Tree,
    closing: &[u64],
    around: &BTreeSet<u64>,
    moves: &[(u64, usize, f64, f64)],
    asm: &Assembly,
    held: &Held,
    linked: &BTreeSet<(u64, usize)>,
    spent: &std::cell::Cell<usize>,
    vals: &mut Values,
) -> Option<BTreeMap<(u64, usize), f64>> {
    let vars = unknowns(tree, around, asm, linked, held);
    if vars.len() > MAX_LOOP_VALUES {
        return Some(BTreeMap::new());
    }
    let bounds: Vec<(f64, f64)> = vars
        .iter()
        .map(|(id, i)| match tree.joints.get(id).and_then(|j| j.limits.get(*i).copied().flatten()) {
            Some((lo, hi)) => (lo.min(hi), hi.max(lo)),
            None => (f64::NEG_INFINITY, f64::INFINITY),
        })
        .collect();
    let scale = loop_scale(tree, around, vals);
    // Closer than the conflict check needs: 1e-7 mm, and turns of 1e-10 rad across the loop.
    let tol = (1e-10 * scale).min(1e-7);
    // The values with the held ones `t` of the way along (and the unknowns at `x`).
    let place = |work: &mut Values, x: &[f64], t: f64| {
        for (id, i, from, to) in moves {
            if let Some(slot) = work.get_mut(id).and_then(|q| q.get_mut(*i)) {
                *slot = from + (to - from) * t;
            }
        }
        for ((id, i), (v, (lo, hi))) in vars.iter().zip(x.iter().zip(&bounds)) {
            if let Some(slot) = work.get_mut(id).and_then(|q| q.get_mut(*i)) {
                *slot = v.max(*lo).min(*hi);
            }
        }
        apply_links(asm, work);
    };
    let x0: Vec<f64> = vars.iter().map(|(id, i)| vals.get(id).and_then(|q| q.get(*i)).copied().unwrap_or(0.0)).collect();
    // Steps that move no value around the loops (links included) more than 5 degrees or a
    // twentieth of the loops' size.
    // How many such steps apart two sets of values are, over the loops' values.
    let stride = |a: &Values, b: &Values| -> f64 {
        let mut most = 0.0f64;
        for j in around.iter().filter_map(|id| tree.joints.get(id)) {
            for i in 0..j.kind.dofs().len() {
                let (Some(p), Some(q)) = (a.get(&j.id).and_then(|v| v.get(i)), b.get(&j.id).and_then(|v| v.get(i))) else { continue };
                let step = if j.kind.is_angle(i) { 5f64.to_radians() } else { 0.05 * scale };
                most = most.max((q - p).abs() / step);
            }
        }
        most
    };
    let effective = |x: &[f64], t: f64| {
        let mut v = vals.clone();
        place(&mut v, x, t);
        v
    };
    let mut first = 1.0f64;
    if !moves.is_empty() {
        first = first.min(1.0 / stride(&effective(&x0, 0.0), &effective(&x0, 1.0)));
    }
    let mut work = vals.clone();
    // How far each closing joint is from meeting with the held values `t` of the way along:
    // three points of B's joint frame, where the joint wants them against where the tree puts them.
    let mut gaps = |x: &[f64], t: f64| -> Option<Vec<f64>> {
        spent.set(spent.get() + 1);
        if spent.get() > MAX_LOOP_WORK {
            return None;
        }
        place(&mut work, x, t);
        let mut out = Vec::with_capacity(closing.len() * 9);
        closure_gaps(tree, closing, &work, scale, &mut out)?;
        out.iter().all(|v| v.is_finite()).then_some(out)
    };
    // Already closed, with no held value moving. (A moving one is followed all the way: a
    // rocker driven a full turn ends where it started, but can't get there.)
    if moves.is_empty() && gaps(&x0, 1.0).is_some_and(|r| largest(&r) <= tol) {
        return Some(BTreeMap::new());
    }
    // A loop open at the start closes first.
    let mut x = x0;
    if largest(&gaps(&x, 0.0)?) > tol && !least_squares(&mut |x| gaps(x, 0.0), &mut x, &bounds, tol) {
        return None;
    }
    let (mut t, mut dt) = (if moves.is_empty() { 1.0 } else { 0.0 }, first);
    let mut at = effective(&x, t);
    while t < 1.0 {
        if dt < first * 1e-4 {
            return None;
        }
        let t1 = (t + dt).min(1.0);
        let mut trial = x.clone();
        if least_squares(&mut |x| gaps(x, t1), &mut trial, &bounds, tol) {
            // A step that moved some loop value too far (through a link and a limit, or close to
            // a dead point) is taken again in halves.
            let next = effective(&trial, t1);
            if stride(&at, &next) > 2.0 {
                dt *= 0.5;
                continue;
            }
            (x, t, dt, at) = (trial, t1, (dt * 2.0).min(first), next);
        } else if spent.get() > MAX_LOOP_WORK {
            return None;
        } else {
            dt *= 0.5;
        }
    }
    if largest(&gaps(&x, 1.0)?) > tol {
        return None;
    }
    // The values that changed from what is stored.
    let moved = vars
        .iter()
        .zip(&x)
        .filter(|((id, i), v)| tree.joints.get(id).is_some_and(|j| j.values.get(*i).is_none_or(|s| (s - *v).abs() > 1e-12)))
        .map(|(var, v)| (*var, *v))
        .collect();
    *vals = work;
    Some(moved)
}

fn largest(r: &[f64]) -> f64 {
    r.iter().fold(0.0f64, |m, v| m.max(v.abs()))
}

/// Levenberg–Marquardt: move `x` (kept within `bounds`) until every value of `f` is within
/// `tol` of zero. False when it can't get there.
/// The Jacobian of `f` (`m` values) at `x` by central differences, row-major.
fn jacobian(f: &mut dyn FnMut(&[f64]) -> Option<Vec<f64>>, x: &[f64], m: usize) -> Option<Vec<f64>> {
    const H: f64 = 1e-6;
    let n = x.len();
    let mut jac = vec![0.0; m * n];
    for k in 0..n {
        let (mut xp, mut xm) = (x.to_vec(), x.to_vec());
        if let (Some(p), Some(q)) = (xp.get_mut(k), xm.get_mut(k)) {
            *p += H;
            *q -= H;
        }
        let (rp, rm) = (f(&xp)?, f(&xm)?);
        for (i, (a, b)) in rp.iter().zip(&rm).enumerate().take(m) {
            if let Some(slot) = jac.get_mut(i * n + k) {
                *slot = (a - b) / (2.0 * H);
            }
        }
    }
    Some(jac)
}

fn least_squares(f: &mut dyn FnMut(&[f64]) -> Option<Vec<f64>>, x: &mut Vec<f64>, bounds: &[(f64, f64)], tol: f64) -> bool {
    let n = x.len();
    let norm = |r: &[f64]| r.iter().map(|v| v * v).sum::<f64>();
    let Some(mut r) = f(x) else { return false };
    if n == 0 {
        return largest(&r) <= tol;
    }
    let mut lambda = 1e-3;
    for _ in 0..50 {
        if largest(&r) <= tol {
            return true;
        }
        // The Jacobian by central differences, then the normal equations JᵀJ δ = −Jᵀr.
        let m = r.len();
        let Some(jac) = jacobian(f, x, m) else { return false };
        let mut jtj = linalg::Mat::zeros(n, n);
        let mut g = vec![0.0; n];
        for (i, ri) in r.iter().enumerate() {
            let row = jac.get(i * n..(i + 1) * n).unwrap_or(&[]);
            for (p, a) in row.iter().enumerate().filter(|(_, a)| **a != 0.0) {
                if let Some(gp) = g.get_mut(p) {
                    *gp -= a * ri;
                }
                for (q, b) in row.iter().enumerate().skip(p) {
                    jtj.add(p, q, a * b);
                }
            }
        }
        for p in 0..n {
            for q in 0..p {
                jtj.set(p, q, jtj.get(q, p));
            }
        }
        let mut stepped = false;
        while lambda < 1e12 {
            let mut a = jtj.clone();
            for p in 0..n {
                a.add(p, p, lambda * (1.0 + jtj.get(p, p)));
            }
            if let Some(d) = linalg::solve(a, g.clone()) {
                let next: Vec<f64> = x.iter().zip(&d).zip(bounds).map(|((v, dv), (lo, hi))| (v + dv).max(*lo).min(*hi)).collect();
                if let Some(rn) = f(&next)
                    && norm(&rn) < norm(&r)
                {
                    *x = next;
                    r = rn;
                    lambda = (lambda / 3.0).max(1e-9);
                    stepped = true;
                    break;
                }
            }
            lambda *= 4.0;
        }
        if !stepped {
            return false;
        }
    }
    largest(&r) <= tol
}

/// Place occurrences from the joint graph.
pub fn solve(doc: &Document, st: &ModelState) -> Solution {
    solve_holding(doc, st, &BTreeMap::new())
}

/// Place occurrences from the joint graph. Joints that close loops are met by moving the values
/// of the other joints around the loop. The `held` values (the ones being driven) keep theirs,
/// moving there from the values given (where the loops last closed) so the others follow them.
pub fn solve_holding(doc: &Document, st: &ModelState, held: &Held) -> Solution {
    let asm = &doc.assembly;
    let mut sol = Solution::default();
    let joints: Vec<&Joint> = asm.joints.iter().filter(|j| !j.suppressed).take(MAX_JOINTS).collect();
    let mut vals = joint_values(asm);
    // Joint frames in component coordinates (errors leave the joint out).
    let mut frames: BTreeMap<u64, (Mat, Mat)> = BTreeMap::new();
    for j in &joints {
        match (resolve_origin(doc, st, &j.a), resolve_origin(doc, st, &j.b)) {
            (Ok(fa), Ok(fb)) => {
                frames.insert(j.id, (fa, fb));
            }
            (Err(e), _) | (_, Err(e)) => sol.errors.push((j.id, j.name.clone(), e.to_string())),
        }
    }
    // Fixed: the root, grounded occurrences; with neither involved, the first joint's A side.
    let mut placed: BTreeMap<u64, Mat> = BTreeMap::new();
    let mut dof: BTreeMap<u64, usize> = BTreeMap::new();
    placed.insert(0, IDENTITY);
    dof.insert(0, 0);
    for o in doc.occurrences.iter().filter(|o| o.grounded) {
        placed.insert(o.id, occurrence_world(doc, o.id));
        dof.insert(o.id, 0);
    }
    let involved: BTreeSet<u64> = joints.iter().flat_map(|j| [j.a.occurrence, j.b.occurrence]).collect();
    if !involved.iter().any(|o| placed.contains_key(o))
        && let Some(j) = joints.first()
    {
        placed.insert(j.a.occurrence, occurrence_world(doc, j.a.occurrence));
        dof.insert(j.a.occurrence, 0);
    }
    sol.fixed = placed.keys().copied().collect();
    let mut tree =
        // Every joint's values are known, suppressed ones too (their motion links still set values).
        Tree { joints: asm.joints.iter().map(|j| (j.id, j)).collect(), frames, fixed: placed.clone(), parent: BTreeMap::new(), order: Vec::new() };
    let mut used: BTreeSet<u64> = BTreeSet::new();
    let mut queue: VecDeque<u64> = placed.keys().copied().collect();
    while let Some(occ) = queue.pop_front() {
        let Some(t) = placed.get(&occ).copied() else { continue };
        for j in &joints {
            if used.contains(&j.id) || !tree.frames.contains_key(&j.id) {
                continue;
            }
            let (other, from_a) = if j.a.occurrence == occ {
                (j.b.occurrence, true)
            } else if j.b.occurrence == occ {
                (j.a.occurrence, false)
            } else {
                continue;
            };
            if placed.contains_key(&other) {
                continue;
            }
            let Some(m) = tree.step(j.id, from_a, &t, &vals) else { continue };
            used.insert(j.id);
            placed.insert(other, m);
            tree.parent.insert(other, (occ, j.id, from_a));
            tree.order.push(other);
            dof.insert(other, dof.get(&occ).copied().unwrap_or(0) + j.kind.dofs().len());
            queue.push_back(other);
        }
    }
    // Joints that close loops: move the loops' values to meet them, then check they agree.
    let closing: Vec<u64> = joints
        .iter()
        .filter(|j| !used.contains(&j.id) && tree.frames.contains_key(&j.id))
        .filter(|j| placed.contains_key(&j.a.occurrence) && placed.contains_key(&j.b.occurrence))
        .map(|j| j.id)
        .collect();
    let mut stuck = BTreeSet::new();
    if !closing.is_empty() {
        (sol.values, stuck) = close_loops(&tree, &closing, asm, held, &mut vals);
        if !sol.values.is_empty() {
            placed = tree.place(&vals);
        }
    }
    for id in &closing {
        let Some(j) = tree.joints.get(id) else { continue };
        let (Some(ta), Some(tb)) = (placed.get(&j.a.occurrence), placed.get(&j.b.occurrence)) else { continue };
        let (Some(want), Some((_, fb))) = (tree.step(*id, true, ta, &vals), tree.frames.get(id)) else { continue };
        // Measured at the joint, where the loop solver closes it.
        let (dt, da) = mismatch(&mat_mul(&want, fb), &mat_mul(tb, fb));
        if dt > 1e-6 || da > 1e-8 {
            sol.conflicts.push((
                j.id,
                j.name.clone(),
                format!("off by {dt:.4} mm and {:.3}° from the placement the other joints give", da.to_degrees()),
            ));
        } else if stuck.contains(id) {
            // It meets there, but the loop would have to come apart on the way (a rocker
            // driven a full turn ends where it started).
            sol.conflicts.push((j.id, j.name.clone(), "the loop can't stay closed on the way to these values".into()));
        }
    }
    if !closing.is_empty() {
        loop_dof(&tree, &closing, asm, &vals, &mut dof);
    }
    for o in &doc.occurrences {
        dof.entry(o.id).or_insert(6);
    }
    placed.remove(&0);
    sol.world = placed;
    sol.dof = dof;
    sol
}

impl Document {
    /// Apply a solution: occurrence transforms (in their parents) for the occurrences it places,
    /// and the values of the joints that moved to close loops. Returns true when something moved.
    pub fn apply_joint_solution(&mut self, sol: &Solution) -> bool {
        let mut changed = false;
        for (id, q) in &sol.values {
            if let Some(j) = self.assembly.joints.iter_mut().find(|j| j.id == *id) {
                j.values.clone_from(q);
                changed = true;
            }
        }
        let parents: BTreeMap<u64, u64> = self.occurrences.iter().map(|o| (o.id, o.parent)).collect();
        for (occ, world) in &sol.world {
            let parent = parents.get(occ).copied().unwrap_or(0);
            let pw = if parent == 0 { IDENTITY } else { self.component_transform(parent) };
            let Some(local) = mat_inverse(&pw).map(|inv| mat_mul(&inv, world)) else { continue };
            if let Some(o) = self.occurrences.iter_mut().find(|o| o.id == *occ) {
                let (dt, da) = mismatch(&o.transform, &local);
                if dt > 1e-9 || da > 1e-10 {
                    o.transform = local;
                    changed = true;
                }
            }
        }
        changed
    }

    /// A frame for an as-built joint: the world frame `w` seen from an occurrence.
    pub fn frame_in_occurrence(&self, occ: u64, w: &Mat) -> Option<Snap> {
        let t = if occ == 0 { IDENTITY } else { occurrence_world(self, occ) };
        let local = mat_mul(&mat_inverse(&t)?, w);
        let o = apply_point(&local, Vec3::ZERO);
        let z = apply_vector(&local, Vec3::Z);
        let x = apply_vector(&local, Vec3::X);
        Some(Snap::Frame { origin: o, z, x: Some(x) })
    }

    /// A joint origin's frame in the world (for as-built joints and display).
    pub fn origin_world(&self, st: &ModelState, o: &JointOrigin) -> Result<Mat> {
        let f = resolve_origin(self, st, o)?;
        let t = if o.occurrence == 0 { IDENTITY } else { occurrence_world(self, o.occurrence) };
        Ok(mat_mul(&t, &f))
    }
}

/// The frame matrix for an explicit origin / z / x (public for commands).
pub fn frame_of(o: Vec3, z: Vec3, x: Option<Vec3>) -> Option<Mat> {
    frame(o, z, x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &Mat, b: &Mat) -> bool {
        let (t, r) = mismatch(a, b);
        t < 1e-9 && r < 1e-9
    }

    /// Revolute joints 1…n (joint 1 at `start`, the others at 0) linked 1 → 2 → … → n with
    /// ratio 2, the links stored from the end of the chain; joint 3 limited to ±50.
    fn chain(n: u64, start: f64) -> Assembly {
        let origin = JointOrigin { occurrence: 0, snap: Snap::Point { point: Vec3::ZERO, z: None } };
        let joints = (1..=n)
            .map(|id| Joint {
                id,
                name: format!("J{id}"),
                kind: JointKind::Revolute,
                a: origin.clone(),
                b: origin.clone(),
                values: vec![if id == 1 { start } else { 0.0 }],
                limits: vec![(id == 3).then_some((-50.0, 50.0))],
                flip: false,
                offset: 0.0,
                angle: 0.0,
                suppressed: false,
            })
            .collect();
        let links = (1..n).rev().map(|a| MotionLink { a, ia: 0, b: a + 1, ib: 0, ratio: 2.0, offset: 0.0 }).collect();
        Assembly { joints, links, ..Assembly::default() }
    }

    /// Links carry a value down a chain stored from its end, each linked value within its limits
    /// as it is set, and applying them again changes nothing.
    #[test]
    fn links_settle_whatever_order_they_are_stored_in() {
        let asm = chain(8, 10.0);
        let vals = joint_values(&asm);
        let at = |id: u64| vals.get(&id).and_then(|q| q.first()).copied().unwrap_or(f64::NAN);
        // 10 → 20 → 40 (within joint 3's limit) → 80 → … → 1280.
        assert_eq!((at(1), at(2), at(3), at(4), at(8)), (10.0, 20.0, 40.0, 80.0, 1280.0));
        let asm = chain(8, 40.0);
        let vals = joint_values(&asm);
        let at = |id: u64| vals.get(&id).and_then(|q| q.first()).copied().unwrap_or(f64::NAN);
        // Joint 3 would be 160: held at 50, and what follows it follows the 50.
        assert_eq!((at(3), at(4), at(8)), (50.0, 100.0, 1600.0));
        let mut again = vals.clone();
        apply_links(&asm, &mut again);
        assert_eq!(again, vals);
    }

    #[test]
    fn motions_compose() {
        assert!(close(&motion(JointKind::Revolute, &[std::f64::consts::FRAC_PI_2]), &rot_z(std::f64::consts::FRAC_PI_2)));
        let m = motion(JointKind::Cylindrical, &[0.0, 5.0]);
        assert!((m[3][2] - 5.0).abs() < 1e-12);
        let m = motion(JointKind::PinSlot, &[0.0, 3.0]);
        assert!((m[3][0] - 3.0).abs() < 1e-12);
        assert_eq!(JointKind::Ball.dofs().len(), 3);
        assert!(JointKind::parse("Pin Slot").is_some() && JointKind::parse("nope").is_none());
        let f = frame(Vec3::new(1.0, 2.0, 3.0), Vec3::Z, None).unwrap();
        assert!(close(&mat_mul(&f, &mat_inverse(&f).unwrap()), &IDENTITY));
    }

    #[test]
    fn mismatch_resolves_tiny_turns() {
        let (_, a) = mismatch(&rot_z(1e-9), &IDENTITY);
        assert!((a - 1e-9).abs() < 1e-15, "{a}");
        let (_, a) = mismatch(&rot_z(3.0), &IDENTITY);
        assert!((a - 3.0).abs() < 1e-12, "{a}");
    }
}
