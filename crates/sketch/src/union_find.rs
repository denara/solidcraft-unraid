//! Disjoint sets over indices (`parent[i]` is `i` for a root), shared by the solver's
//! components, profile regions, the model's point groups and dimension placement's chains.

/// The root of `i`'s set, halving the path on the way. An index outside `parent` is its own
/// root.
pub(crate) fn find(parent: &mut [usize], mut i: usize) -> usize {
    while let Some(&p) = parent.get(i) {
        if p == i {
            break;
        }
        let gp = parent.get(p).copied().unwrap_or(p);
        if let Some(slot) = parent.get_mut(i) {
            *slot = gp;
        }
        i = p;
    }
    i
}

/// Join the sets of `a` and `b`.
pub(crate) fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if let Some(slot) = parent.get_mut(ra) {
        *slot = rb;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_and_halves_paths() {
        let mut p: Vec<usize> = (0..6).collect();
        union(&mut p, 0, 1);
        union(&mut p, 1, 2);
        union(&mut p, 4, 5);
        assert_eq!(find(&mut p, 0), find(&mut p, 2));
        assert_ne!(find(&mut p, 0), find(&mut p, 4));
        assert_eq!(find(&mut p, 3), 3);
        assert_eq!(find(&mut p, 99), 99);
    }
}
