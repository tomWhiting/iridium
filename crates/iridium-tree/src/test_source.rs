//! An in-memory [`TreeSource`] for this crate's own tests.
//!
//! Counts its own calls, because the laziness guarantee — a closed subtree
//! is never enumerated — is a claim about *calls*, and a test that only
//! inspected the resulting rows could not tell a lazy tree from an eager one
//! that hid the extra work.

use crate::TreeSource;
use std::collections::HashMap;

/// A fixed hierarchy described by parent→child edges.
///
/// Indexed by parent rather than scanned, so `children` is O(children) and
/// not O(edges). That matters here: the deep-chain test would otherwise be
/// quadratic in the depth, and a fixture slow enough to notice gets its
/// depth reduced until it stops testing the thing it was written for.
pub struct Fixture {
    /// Children per parent, with `None` keying the roots. Insertion order
    /// within a parent is preserved as sibling order.
    children: HashMap<Option<String>, Vec<String>>,
    /// How many times [`TreeSource::children`] has been called.
    pub children_calls: usize,
}

impl Fixture {
    /// Builds a fixture from `(parent, child)` pairs, where an empty parent
    /// means a root. Declaration order is preserved as sibling order.
    pub fn new(edges: &[(&str, &str)]) -> Self {
        let mut children: HashMap<Option<String>, Vec<String>> = HashMap::new();
        for (parent, child) in edges {
            let parent = if parent.is_empty() {
                None
            } else {
                Some((*parent).to_owned())
            };
            children
                .entry(parent)
                .or_default()
                .push((*child).to_owned());
        }
        Self {
            children,
            children_calls: 0,
        }
    }

    /// A single chain `n0 → n1 → … → n(depth-1)`, for depth tests.
    pub fn chain(depth: usize) -> Self {
        let mut children: HashMap<Option<String>, Vec<String>> = HashMap::new();
        children.insert(None, vec!["n0".to_owned()]);
        for level in 1..depth {
            children.insert(Some(format!("n{}", level - 1)), vec![format!("n{level}")]);
        }
        Self {
            children,
            children_calls: 0,
        }
    }

    /// The ids of the currently visible rows, for terse assertions.
    pub fn ids(tree: &crate::Tree<Self>) -> Vec<String> {
        tree.rows().iter().map(|row| row.id.clone()).collect()
    }

    /// The `(id, depth)` of each visible row.
    pub fn shape(tree: &crate::Tree<Self>) -> Vec<(String, usize)> {
        tree.rows()
            .iter()
            .map(|row| (row.id.clone(), row.depth))
            .collect()
    }
}

impl TreeSource for Fixture {
    type Id = String;

    fn children(&mut self, parent: Option<&Self::Id>) -> Vec<Self::Id> {
        self.children_calls = self.children_calls.saturating_add(1);
        self.children
            .get(&parent.cloned())
            .cloned()
            .unwrap_or_default()
    }

    fn has_children(&mut self, node: &Self::Id) -> bool {
        self.children
            .get(&Some(node.clone()))
            .is_some_and(|kids| !kids.is_empty())
    }
}

/// A source that claims every node has children and never produces any.
///
/// The contract permits an optimistic `has_children`, so the tree has to
/// cope with the promise being broken; this is that case in isolation.
pub struct Liar;

impl TreeSource for Liar {
    type Id = &'static str;

    fn children(&mut self, parent: Option<&Self::Id>) -> Vec<Self::Id> {
        if parent.is_none() {
            vec!["only"]
        } else {
            Vec::new()
        }
    }

    fn has_children(&mut self, _node: &Self::Id) -> bool {
        true
    }
}
