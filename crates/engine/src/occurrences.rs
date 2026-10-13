//! Exact placement identity and session view/authoring state.
use crate::{EngineError, Result, Sel, Session};
use solidcraft_doc::{IDENTITY, Mat, mat_mul};

#[derive(Clone, Debug)]
pub struct OccurrenceBody {
    pub source: String,
    pub world: String,
}

#[derive(Clone, Debug)]
pub struct OccurrencePlacement {
    pub occurrence: u64,
    pub component: u64,
    pub path: Vec<u64>,
    pub name: String,
    pub world: Mat,
    pub local: Mat,
    pub parent_world: Mat,
    pub grounded: bool,
    pub bodies: Vec<OccurrenceBody>,
}

fn invalid(message: &str) -> EngineError {
    EngineError::Other(message.to_string())
}

pub(crate) struct VisibilityCache {
    revision: u64,
    paths: Vec<Vec<u64>>,
    bodies: std::collections::BTreeSet<String>,
}

impl Session {
    /// Document with uncaptured occurrence positions applied, without changing saved geometry.
    pub fn placed_document(&self) -> std::borrow::Cow<'_, solidcraft_doc::Document> {
        if self.pending_moves.is_empty() {
            return std::borrow::Cow::Borrowed(&self.doc);
        }
        let mut doc = (*self.doc).clone();
        for o in &mut doc.occurrences {
            if let Some(transform) = self.pending_moves.get(&o.id) {
                o.transform = *transform;
            }
        }
        std::borrow::Cow::Owned(doc)
    }
    /// Resolve only the supplied path, never an exponentially expanded assembly.
    pub fn occurrence_placement(&self, path: &[u64]) -> Result<OccurrencePlacement> {
        if path.is_empty() || path.len() > 64 {
            return Err(invalid("occurrence path must contain 1..64 ids"));
        }
        let mut component = 0;
        let mut world = IDENTITY;
        let mut parent_world = IDENTITY;
        let mut seen = std::collections::BTreeSet::new();
        for id in path {
            if !seen.insert(*id) {
                return Err(invalid("occurrence path repeats an id"));
            }
            let o = self
                .doc
                .occurrences
                .iter()
                .find(|o| o.id == *id && o.parent == component)
                .ok_or_else(|| invalid("occurrence path has a missing or mismatched parent"))?;
            if !self.doc.components.iter().any(|c| c.id == o.component) {
                return Err(invalid("occurrence path has a missing component"));
            }
            parent_world = world;
            world = mat_mul(&world, self.pending_moves.get(id).unwrap_or(&o.transform));
            if world.iter().flatten().any(|x| !x.is_finite()) {
                return Err(invalid("occurrence path has a non-finite transform"));
            }
            component = o.component;
        }
        let leaf = self.doc.occurrences.iter().find(|o| Some(&o.id) == path.last()).ok_or_else(|| invalid("occurrence path has no leaf"))?;
        let state = self.model.state();
        let bodies = state
            .bodies
            .iter()
            .filter(|b| self.doc.model_body_component(b) == component)
            .map(|b| OccurrenceBody { source: b.name.clone(), world: self.doc.world_body_name(&state, &b.name, component, path) })
            .collect();
        Ok(OccurrencePlacement {
            occurrence: leaf.id,
            component,
            path: path.to_vec(),
            name: leaf.name.clone(),
            world,
            local: self.pending_moves.get(&leaf.id).copied().unwrap_or(leaf.transform),
            parent_world,
            grounded: leaf.grounded,
            bodies,
        })
    }

    /// Children are definitions shared by every placement of the parent component.
    pub fn occurrence_children(&self, path: &[u64]) -> Result<Vec<OccurrencePlacement>> {
        let component = if path.is_empty() { 0 } else { self.occurrence_placement(path)?.component };
        self.doc
            .occurrences
            .iter()
            .filter(|o| o.parent == component)
            .map(|o| {
                let mut child = path.to_vec();
                child.push(o.id);
                self.occurrence_placement(&child)
            })
            .collect()
    }

    /// Bounded descendant expansion. Never return an incomplete selection or bounds list.
    pub fn occurrence_bodies(&self, path: &[u64], descendants: bool) -> Result<Vec<String>> {
        let mut stack = vec![self.occurrence_placement(path)?];
        let mut out = Vec::new();
        let mut count = 0;
        while let Some(placement) = stack.pop() {
            count += 1;
            if count > 100_000 || out.len() + placement.bodies.len() > 100_000 {
                return Err(invalid("occurrence expansion exceeds 100000 placements or bodies"));
            }
            out.extend(placement.bodies.into_iter().map(|b| b.world));
            if descendants {
                stack.extend(self.occurrence_children(&placement.path)?);
            }
        }
        Ok(out)
    }

    /// Selected authoring placement, or the compatible first-placement frame.
    pub fn active_component_frame(&self) -> Mat {
        if let Some(path) = &self.active_occurrence_path
            && let Ok(p) = self.occurrence_placement(path)
            && p.component == self.active_component
        {
            return p.world;
        }
        let path = self.doc.first_path(self.active_component);
        self.occurrence_placement(&path).map(|p| p.world).unwrap_or(IDENTITY)
    }

    pub fn occurrence_visible(&self, path: &[u64]) -> bool {
        !self.visibility.hidden_occurrences.iter().any(|hidden| path.starts_with(hidden))
    }

    pub fn occurrence_hidden(&self, path: &[u64]) -> bool {
        self.visibility.hidden_occurrences.iter().any(|hidden| hidden == path)
    }

    /// Number of placements affected by editing this shared occurrence definition.
    pub fn occurrence_scope_count(&self, path: &[u64]) -> Result<usize> {
        let id = self.occurrence_placement(path)?.occurrence;
        self.occurrence_id_scope_count(id)
    }

    pub fn occurrence_id_scope_count(&self, id: u64) -> Result<usize> {
        if !self.doc.occurrences.iter().any(|o| o.id == id) {
            return Err(invalid("no occurrence"));
        }
        let mut stack = vec![(0, 0usize)];
        let mut visited = 0;
        let mut count = 0;
        while let Some((component, depth)) = stack.pop() {
            for o in self.doc.occurrences.iter().filter(|o| o.parent == component) {
                visited += 1;
                if depth >= 64 || visited > 100_000 {
                    return Err(invalid("occurrence scope exceeds 64 levels or 100000 placements"));
                }
                if o.id == id {
                    count += 1;
                }
                stack.push((o.component, depth + 1));
            }
        }
        Ok(count)
    }

    pub fn body_visible(&self, name: &str) -> bool {
        if self.visibility.hidden_bodies.iter().any(|n| n == name) {
            return false;
        }
        if self.visibility.hidden_occurrences.is_empty() {
            return true;
        }
        if let Ok(mut cache) = self.occurrence_visibility_cache.lock() {
            if cache.as_ref().is_none_or(|c| c.revision != self.revision || c.paths != self.visibility.hidden_occurrences) {
                let bodies = self
                    .doc
                    .body_frames(&self.model.state())
                    .into_iter()
                    .filter(|(_, _, path, _)| !self.occurrence_visible(path))
                    .map(|(name, _, _, _)| name)
                    .collect();
                *cache = Some(VisibilityCache { revision: self.revision, paths: self.visibility.hidden_occurrences.clone(), bodies });
            }
            return cache.as_ref().is_none_or(|c| !c.bodies.contains(name));
        }
        self.doc.body_frames(&self.model.state()).iter().find(|(n, _, _, _)| n == name).is_none_or(|(_, _, path, _)| self.occurrence_visible(path))
    }

    pub(crate) fn prune_occurrence_state(&mut self) {
        self.pending_moves.retain(|id, _| self.doc.occurrences.iter().any(|o| o.id == *id));
        if self.active_component != 0 && !self.doc.components.iter().any(|c| c.id == self.active_component) {
            self.active_component = 0;
            self.active_occurrence_path = None;
        }
        if self.active_occurrence_path.as_ref().is_some_and(|p| self.occurrence_placement(p).map_or(true, |p| p.component != self.active_component)) {
            self.active_occurrence_path = None;
        }
        let selection = std::mem::take(&mut self.selection);
        self.selection = selection.into_iter().filter(|s| !matches!(s, Sel::Occurrence {path} if self.occurrence_placement(path).is_err())).collect();
        let hidden = std::mem::take(&mut self.visibility.hidden_occurrences);
        self.visibility.hidden_occurrences = hidden.into_iter().filter(|p| self.occurrence_placement(p).is_ok()).collect();
    }
}
