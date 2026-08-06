//! Expansion state, the row projection derived from it, and the selection.

use crate::{Row, TreeSource};
use core::fmt;
use std::collections::HashSet;

#[cfg(test)]
mod tree_tests;

/// A hierarchical view: what is open, what is therefore visible, and where
/// the selection sits.
///
/// See the [crate documentation](crate) for the model. In short: `expanded`
/// is authoritative and `rows` is derived from it, so collapsing a node
/// preserves the state of everything beneath it.
pub struct Tree<S: TreeSource> {
    /// The visible rows in depth-first pre-order. Derived; never edited
    /// except through the methods below, which keep it and `expanded`
    /// consistent.
    rows: Vec<Row<S::Id>>,
    /// Every node the user has opened, including ones not currently visible
    /// because an ancestor is closed.
    ///
    /// Ids are **not** pruned when a node disappears from the hierarchy. A
    /// directory that is deleted and recreated should reopen the way the
    /// user left it, and the cost is one id per node ever expanded — bounded
    /// by what a person has actually clicked, not by the size of the tree.
    expanded: HashSet<S::Id>,
    /// The selected row, as an index into `rows`.
    ///
    /// An index rather than an id so that keyboard movement is O(1); every
    /// mutation below is responsible for moving it to stay on the same node,
    /// or onto the collapsed ancestor when its node stops being visible.
    selected: Option<usize>,
}

impl<S: TreeSource> Tree<S> {
    /// Builds a tree showing `source`'s roots, all closed.
    pub fn new(source: &mut S) -> Self {
        let mut expanded = HashSet::new();
        let rows = Self::build(&mut expanded, source, None, 0);
        Self {
            rows,
            expanded,
            selected: None,
        }
    }

    /// The visible rows, in the order they should be drawn.
    #[must_use]
    pub fn rows(&self) -> &[Row<S::Id>] {
        &self.rows
    }

    /// How many rows are visible.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether nothing is visible — an empty hierarchy, not a closed one.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The row at `index`, or `None` if it is past the end.
    #[must_use]
    pub fn row(&self, index: usize) -> Option<&Row<S::Id>> {
        self.rows.get(index)
    }

    /// The `count` rows starting at `first`, clamped to what exists.
    ///
    /// This is the virtualisation entry point: a face renders
    /// `window(first_visible, visible_count)` and never touches the rest, so
    /// the cost of a frame is the height of the viewport rather than the
    /// size of the tree. Both arguments are clamped, so a viewport that has
    /// scrolled past the end yields an empty slice instead of an error.
    #[must_use]
    pub fn window(&self, first: usize, count: usize) -> &[Row<S::Id>] {
        let start = first.min(self.rows.len());
        let end = start.saturating_add(count).min(self.rows.len());
        &self.rows[start..end]
    }

    /// The index of the row showing `id`, if it is visible.
    ///
    /// Linear in the number of visible rows. Intended for restoring a
    /// selection or revealing a node, not for a per-frame lookup.
    #[must_use]
    pub fn index_of(&self, id: &S::Id) -> Option<usize> {
        self.rows.iter().position(|row| &row.id == id)
    }

    /// Whether `id` is open, whether or not it is currently visible.
    #[must_use]
    pub fn is_expanded(&self, id: &S::Id) -> bool {
        self.expanded.contains(id)
    }

    /// The selected row index, if any.
    #[must_use]
    pub const fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// The selected node, if any.
    #[must_use]
    pub fn selected_id(&self) -> Option<&S::Id> {
        self.selected
            .and_then(|index| self.rows.get(index))
            .map(|row| &row.id)
    }

    /// Selects `index`, returning whether it named a visible row.
    ///
    /// A rejected index leaves the previous selection alone rather than
    /// clearing it: a click that raced a collapse should not also lose the
    /// user's place.
    pub fn select(&mut self, index: usize) -> bool {
        if index < self.rows.len() {
            self.selected = Some(index);
            return true;
        }
        false
    }

    /// Drops the selection.
    pub const fn clear_selection(&mut self) {
        self.selected = None;
    }

    /// Opens the node at `index`, returning whether anything opened.
    ///
    /// Returns `false` for an out-of-range index, a row already open, and a
    /// row that claimed children but produced none — in that last case the
    /// row is corrected to a leaf, so the disclosure arrow disappears rather
    /// than inviting the click again.
    pub fn expand(&mut self, source: &mut S, index: usize) -> bool {
        let Some(row) = self.rows.get(index) else {
            return false;
        };
        if row.expanded || !row.has_children {
            return false;
        }

        let id = row.id.clone();
        let depth = row.depth.saturating_add(1);
        self.expanded.insert(id.clone());
        let children = Self::build(&mut self.expanded, source, Some(&id), depth);

        if children.is_empty() {
            // The source promised children and delivered none. Permitted by
            // the contract, so correct the row rather than treating it as an
            // error, and drop the expansion so state stays truthful.
            self.expanded.remove(&id);
            if let Some(row) = self.rows.get_mut(index) {
                row.has_children = false;
                row.expanded = false;
            }
            return false;
        }

        let inserted = children.len();
        let at = index.saturating_add(1);
        self.rows.splice(at..at, children);
        if let Some(row) = self.rows.get_mut(index) {
            row.expanded = true;
        }
        // Rows at or before `index` did not move; everything after shifted
        // down by exactly the number inserted.
        if let Some(selected) = self.selected {
            if selected > index {
                self.selected = Some(selected.saturating_add(inserted));
            }
        }
        true
    }

    /// Closes the node at `index`, returning whether anything closed.
    ///
    /// The descendants' own expansion state is kept, so re-opening restores
    /// the subtree exactly as it was.
    ///
    /// If the selection was inside the subtree being hidden it moves to
    /// `index` — the node just collapsed — because that is where the user's
    /// attention is and it is the only row guaranteed to still exist.
    pub fn collapse(&mut self, index: usize) -> bool {
        let Some(row) = self.rows.get(index) else {
            return false;
        };
        if !row.expanded {
            return false;
        }

        let id = row.id.clone();
        let extent = self.subtree_extent(index);
        self.expanded.remove(&id);
        let end = index.saturating_add(1).saturating_add(extent);
        self.rows.drain(index + 1..end);
        if let Some(row) = self.rows.get_mut(index) {
            row.expanded = false;
        }

        if let Some(selected) = self.selected {
            if selected > index {
                self.selected = if selected < end {
                    Some(index)
                } else {
                    Some(selected.saturating_sub(extent))
                };
            }
        }
        true
    }

    /// Opens a closed row or closes an open one.
    ///
    /// Returns whether the row changed. A leaf never changes.
    pub fn toggle(&mut self, source: &mut S, index: usize) -> bool {
        match self.rows.get(index) {
            Some(row) if row.expanded => self.collapse(index),
            Some(_) => self.expand(source, index),
            None => false,
        }
    }

    /// Closes every open node, keeping only the roots visible.
    ///
    /// Forgets expansion state entirely, unlike [`collapse`](Tree::collapse)
    /// — this is the "start again" verb, and preserving state would make it
    /// indistinguishable from collapsing the roots one at a time.
    pub fn collapse_all(&mut self, source: &mut S) {
        let selected_id = self.selected_id().cloned();
        self.expanded.clear();
        self.rows = Self::build(&mut self.expanded, source, None, 0);
        self.restore_selection(selected_id.as_ref());
    }

    /// Rebuilds the projection from the source, keeping expansion and, where
    /// possible, the selection.
    ///
    /// This is how a lazily-loading source announces that it has more: read
    /// the directory off-thread, store it, then call `refresh`. It is also
    /// the correct response to anything that changed the hierarchy
    /// underneath the tree.
    ///
    /// Cost is the number of *visible* rows, not the size of the hierarchy,
    /// because closed subtrees are still never enumerated. The selection
    /// follows its node; if that node is gone, the selection is dropped
    /// rather than left pointing at whatever moved into its index.
    pub fn refresh(&mut self, source: &mut S) {
        let selected_id = self.selected_id().cloned();
        self.rows = Self::build(&mut self.expanded, source, None, 0);
        self.restore_selection(selected_id.as_ref());
    }

    /// How many rows immediately after `index` are its descendants.
    ///
    /// Descendants are exactly the following run of rows deeper than
    /// `index`, which holds because the projection is depth-first pre-order.
    fn subtree_extent(&self, index: usize) -> usize {
        let Some(depth) = self.rows.get(index).map(|row| row.depth) else {
            return 0;
        };
        self.rows
            .get(index.saturating_add(1)..)
            .unwrap_or_default()
            .iter()
            .take_while(|row| row.depth > depth)
            .count()
    }

    /// Puts the selection back on `id`, or drops it if that row is gone.
    fn restore_selection(&mut self, id: Option<&S::Id>) {
        self.selected = id.and_then(|id| self.index_of(id));
    }

    /// Materialises the visible rows under `parent`, honouring `expanded`.
    ///
    /// Iterative rather than recursive, deliberately: depth here is the
    /// source's to choose, and a hierarchy deep enough to overflow the stack
    /// is precisely the input a model that promises not to panic has to
    /// survive. The explicit stack holds `(id, depth)` and is popped in
    /// pre-order, children pushed reversed so siblings emerge in source
    /// order.
    fn build(
        expanded: &mut HashSet<S::Id>,
        source: &mut S,
        parent: Option<&S::Id>,
        base_depth: usize,
    ) -> Vec<Row<S::Id>> {
        let mut out = Vec::new();
        let mut stack: Vec<(S::Id, usize)> = source
            .children(parent)
            .into_iter()
            .rev()
            .map(|id| (id, base_depth))
            .collect();

        while let Some((id, depth)) = stack.pop() {
            let claims_children = source.has_children(&id);
            let is_open = claims_children && expanded.contains(&id);
            let children = if is_open {
                source.children(Some(&id))
            } else {
                Vec::new()
            };
            // A node that is open and empty is a leaf whatever `has_children`
            // said; drop the expansion so the state does not claim an open
            // node with nothing in it.
            let opened = is_open && !children.is_empty();
            if is_open && children.is_empty() {
                expanded.remove(&id);
            }

            out.push(Row {
                id,
                depth,
                has_children: if is_open { opened } else { claims_children },
                expanded: opened,
            });

            if opened {
                let child_depth = depth.saturating_add(1);
                stack.extend(children.into_iter().rev().map(|id| (id, child_depth)));
            }
        }
        out
    }
}

impl<S: TreeSource> fmt::Debug for Tree<S>
where
    S::Id: fmt::Debug,
{
    /// Written by hand because deriving would demand `S: Debug`, and the
    /// source is not part of the tree's state.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Tree")
            .field("rows", &self.rows)
            .field("expanded", &self.expanded)
            .field("selected", &self.selected)
            .finish()
    }
}
