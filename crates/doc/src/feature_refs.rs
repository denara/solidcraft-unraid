//! References from a feature to the rest of the design: the sketches it is built from (by id),
//! the features it names (patterned and mirrored features, the construction plane a plane is
//! built on) and the bodies it works on (by name). The walk (`crate::walk`) is behind
//! `input_sketches`, `remap_refs` and `remap_bodies`.

use std::collections::BTreeMap;

use crate::FeatureKind;
use crate::walk::Walk;

/// A reference a feature holds: `S` is `&u64` or `&mut u64` (a sketch id), `N` is `&String` or
/// `&mut String` (a feature or body name).
pub(crate) enum Ref<S, N> {
    Sketch(S),
    Feature(N),
    Body(N),
}

impl FeatureKind {
    /// Sketches the feature is built from (profiles, paths, points): deleting one of them
    /// deletes the feature.
    pub fn input_sketches(&self) -> Vec<u64> {
        let mut v = Vec::new();
        self.walk_refs(&mut |r| {
            if let Ref::Sketch(id) = r {
                v.push(*id);
            }
        });
        v
    }

    /// Point references to sketches (by id) and features (by name) at their copies, or at a
    /// feature's new name (`timeline.rename`).
    pub fn remap_refs(&mut self, sketches: &BTreeMap<u64, u64>, names: &BTreeMap<String, String>) {
        self.walk_refs_mut(&mut |r| match r {
            Ref::Sketch(id) => {
                if let Some(n) = sketches.get(id) {
                    *id = *n;
                }
            }
            Ref::Feature(name) => {
                if let Some(n) = names.get(name) {
                    *name = n.clone();
                }
            }
            Ref::Body(_) => {}
        });
    }

    /// Point references to bodies (by name) at their copies.
    pub fn remap_bodies(&mut self, bodies: &BTreeMap<String, String>) {
        self.walk_refs_mut(&mut |r| {
            if let Ref::Body(name) = r
                && let Some(n) = bodies.get(name)
            {
                *name = n.clone();
            }
        });
    }
}
