//! SolidCraft: where each face and edge of a boolean's result came from, recorded by the code
//! that builds them (never reconstructed from geometry).
//!
//! Topology ids are addresses of shared data, so they identify a face or an edge only while it
//! lives. [`Lineage`] therefore stores, for every face and edge an operation creates, its origin
//! in terms of the operands at the moment it is created (while its parent is alive), and never a
//! chain of ids: a dead entity's address that is reused is overwritten when the new entity is
//! recorded.

use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use truck_topology::*;

/// A face of one of a boolean's two operands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FaceIndex {
    /// 0 for the first operand, 1 for the second.
    pub operand: usize,
    /// Position in that operand's `face_iter` order.
    pub face: usize,
}

/// Where a face of a boolean's result lies. Every result face is a region of exactly one
/// operand face (a boolean makes no face of its own).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FaceOrigin {
    /// The operand face it is a region of (same surface, same orientation).
    pub source: FaceIndex,
    /// `true` when it is that very face, passed through untouched (no intersection reached
    /// it); `false` for a region rebuilt from it by the face division.
    pub kept: bool,
}

/// Where an edge of a boolean's result came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EdgeOrigin {
    /// An operand edge or a piece of it.
    Edge {
        /// 0 for the first operand, 1 for the second.
        operand: usize,
        /// Position in that operand's edge order (see [`History`]).
        edge: usize,
    },
    /// The intersection of two operand faces, or a piece of it (`a < b`).
    Meet {
        /// The lesser of the two faces.
        a: FaceIndex,
        /// The greater of the two faces.
        b: FaceIndex,
    },
}

/// History of a boolean's result: one entry per face in its `face_iter` order and one per edge
/// in its edge order. A solid's edge order lists its distinct edges in order of first use,
/// walking `face_iter` and each face's `edge_iter` (that is, `Solid::edge_iter` without repeats).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct History {
    /// Origin of each result face.
    pub faces: Vec<FaceOrigin>,
    /// Origin of each result edge.
    pub edges: Vec<EdgeOrigin>,
}

/// The ids of `solid`'s edges in its edge order (see [`History`]).
fn edge_order<P, C, S>(solid: &Solid<P, C, S>) -> Vec<EdgeID<C>> {
    let mut seen = HashSet::default();
    solid.edge_iter().map(|e| e.id()).filter(|id| seen.insert(*id)).collect()
}

/// Records `origin` under `key`, or, when the origin is unknown, leaves nothing stale under the
/// new entity's address.
fn set<K: std::hash::Hash + Eq, V>(map: &mut HashMap<K, V>, key: K, origin: Option<V>) {
    match origin {
        Some(o) => map.insert(key, o),
        None => map.remove(&key),
    };
}

/// Origins, in operand terms, of the faces and edges alive during one boolean.
///
/// The operands must not share faces or edges (a shared one is attributed to the second).
#[derive(Debug)]
pub(crate) struct Lineage<C, S> {
    faces: HashMap<FaceID<S>, FaceOrigin>,
    edges: HashMap<EdgeID<C>, EdgeOrigin>,
}

impl<C, S> Default for Lineage<C, S> {
    fn default() -> Self { Self { faces: HashMap::default(), edges: HashMap::default() } }
}

impl<C, S> Lineage<C, S> {
    /// Every face and edge of the operands, as itself.
    pub fn new<P>(operands: [&Solid<P, C, S>; 2]) -> Self {
        let mut lineage = Self::default();
        for (operand, solid) in operands.into_iter().enumerate() {
            for (face, f) in solid.face_iter().enumerate() {
                let source = FaceIndex { operand, face };
                lineage.faces.insert(f.id(), FaceOrigin { source, kept: true });
            }
            for (edge, id) in edge_order(solid).into_iter().enumerate() {
                lineage.edges.insert(id, EdgeOrigin::Edge { operand, edge });
            }
        }
        lineage
    }

    /// `piece` is a region trimmed out of the live face `parent`.
    pub fn face_piece(&mut self, parent: FaceID<S>, piece: FaceID<S>) {
        let origin = self.faces.get(&parent).map(|&o| FaceOrigin { kept: false, ..o });
        set(&mut self.faces, piece, origin);
    }

    /// `piece` is the live edge `parent` or a piece of it.
    pub fn edge_piece(&mut self, parent: EdgeID<C>, piece: EdgeID<C>) {
        let origin = self.edges.get(&parent).copied();
        set(&mut self.edges, piece, origin);
    }

    /// `edge` is (part of) the intersection of the live faces `faces`.
    pub fn meet(&mut self, edge: EdgeID<C>, faces: [FaceID<S>; 2]) {
        let [a, b] = faces.map(|f| self.faces.get(&f).map(|o| o.source));
        let origin = a.zip(b).map(|(a, b)| EdgeOrigin::Meet { a: a.min(b), b: a.max(b) });
        set(&mut self.edges, edge, origin);
    }

    /// The history of `solid`, made from the operands by the recorded steps; `None` when one
    /// of its faces or edges was not recorded.
    pub fn history<P>(&self, solid: &Solid<P, C, S>) -> Option<History> {
        let faces = solid.face_iter().map(|f| self.faces.get(&f.id()).copied()).collect::<Option<_>>();
        let edges = edge_order(solid).into_iter().map(|id| self.edges.get(&id).copied()).collect::<Option<_>>();
        Some(History { faces: crate::tr!(faces, "face history"), edges: crate::tr!(edges, "edge history") })
    }
}
