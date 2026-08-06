//! One visible line of a [`Tree`](crate::Tree).

/// A single visible row: what to draw, and how far in.
///
/// Rows are produced by [`Tree`](crate::Tree) and are a *projection* — a row
/// exists only while every ancestor of its node is expanded. A face should
/// treat a row index as valid until the next mutation and no longer; the
/// tree's methods all bounds-check, so a stale index is harmless rather than
/// fatal, but it may name a different node than it did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row<Id> {
    /// The node this row shows.
    pub id: Id,
    /// How many ancestors it has: `0` for a root, `1` for its children.
    ///
    /// `usize` rather than a narrower integer, and the two bytes saved are
    /// not worth what they cost. Depth is what distinguishes a descendant
    /// from a sibling — `subtree_extent` collapses a node by taking the run
    /// of following rows *deeper* than it. Under a saturating narrow depth,
    /// two nested levels at the cap compare equal, that run ends early, and
    /// a collapse leaves orphaned rows on screen. `usize` cannot reach its
    /// ceiling: every level needs a live node, so the hierarchy would have
    /// to outnumber addressable memory first.
    pub depth: usize,
    /// Whether to draw a disclosure arrow.
    ///
    /// Sourced from [`TreeSource::has_children`](crate::TreeSource::has_children)
    /// and therefore possibly optimistic; it is corrected to `false` if the
    /// node is expanded and turns out to have none.
    pub has_children: bool,
    /// Whether this node's children are currently shown.
    ///
    /// Always `false` when `has_children` is `false`.
    pub expanded: bool,
}

impl<Id> Row<Id> {
    /// Whether this row can be opened: it claims children and is closed.
    #[must_use]
    pub const fn is_openable(&self) -> bool {
        self.has_children && !self.expanded
    }

    /// Whether this row can be closed: it is open.
    #[must_use]
    pub const fn is_closable(&self) -> bool {
        self.expanded
    }
}
