//! The workspace as something [`iridium_tree::Tree`] can project.
//!
//! This is the reuse the tree crate was written generic for. With this
//! impl, a sidebar over the workspace gets expansion state, selection,
//! keyboard movement and virtualised windowing from a crate that already
//! tests all four — rather than a second, subtly different implementation
//! of the same behaviours living next to the file tree's.
//!
//! # Why `&mut self`
//!
//! [`TreeSource`] takes `&mut self` so that a source which loads lazily —
//! a directory read off disk, a remote listing — can cache what it
//! fetched. A workspace needs none of that: every answer is already in
//! memory. It is implemented anyway rather than worked around, because a
//! trait that fits every source is worth more than one that fits this one
//! exactly.

use iridium_tree::TreeSource;

use super::{Node, NodeId, Workspace};

impl TreeSource for Workspace {
    type Id = NodeId;

    /// The roots, or a group's children in display order.
    ///
    /// A tab reports no children rather than being an error: the tree asks
    /// this of any node it is told has children, and "a tab is a leaf" is
    /// the honest answer, not an exceptional one.
    fn children(&mut self, parent: Option<&Self::Id>) -> Vec<Self::Id> {
        parent.map_or_else(
            || self.roots().to_vec(),
            |id| {
                self.node(*id)
                    .map(Node::children)
                    .unwrap_or_default()
                    .to_vec()
            },
        )
    }

    /// Whether a node is a group with anything in it.
    ///
    /// An empty group answers `false`, so it draws as a leaf with no
    /// disclosure arrow. That is deliberate: an arrow that opens onto
    /// nothing invites the click again, and the tree crate's own contract
    /// permits a source to report children and then produce none precisely
    /// so this can be answered honestly instead of optimistically.
    fn has_children(&mut self, node: &Self::Id) -> bool {
        self.node(*node)
            .is_some_and(|node| !node.children().is_empty())
    }
}
