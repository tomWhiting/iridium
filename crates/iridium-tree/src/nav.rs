//! Keyboard movement over the projection.
//!
//! These are the four arrow keys and the two ends, as every tree view has
//! agreed to behave since the Finder: down and up walk *visible* rows and
//! therefore step in and out of open subtrees for free; right opens or
//! descends; left closes or ascends.
//!
//! **Movement clamps, it does not wrap.** Held keys repeat, and a wrapping
//! list jumps from the last row to the first while the user is still leaning
//! on the key — which reads as the view having lost their place. Every
//! method here returns whether the selection actually moved, so a face can
//! distinguish "at the end" from "did something".

use crate::{Tree, TreeSource};

#[cfg(test)]
mod nav_tests;

impl<S: TreeSource> Tree<S> {
    /// Moves the selection to the next visible row.
    ///
    /// With nothing selected this selects the first row, so a fresh tree
    /// responds to the first key press rather than swallowing it.
    pub fn move_down(&mut self) -> bool {
        if self.is_empty() {
            return false;
        }
        let next = match self.selected() {
            None => 0,
            Some(index) if index.saturating_add(1) < self.len() => index.saturating_add(1),
            Some(_) => return false,
        };
        self.select(next)
    }

    /// Moves the selection to the previous visible row.
    ///
    /// With nothing selected this selects the last row, mirroring
    /// [`move_down`](Tree::move_down): pressing up in an untouched list
    /// should reach the end nearest the key.
    pub fn move_up(&mut self) -> bool {
        if self.is_empty() {
            return false;
        }
        let previous = match self.selected() {
            None => self.len().saturating_sub(1),
            Some(0) => return false,
            Some(index) => index.saturating_sub(1),
        };
        self.select(previous)
    }

    /// Selects the first visible row.
    pub fn move_to_first(&mut self) -> bool {
        if self.is_empty() || self.selected() == Some(0) {
            return false;
        }
        self.select(0)
    }

    /// Selects the last visible row.
    pub fn move_to_last(&mut self) -> bool {
        let last = self.len().saturating_sub(1);
        if self.is_empty() || self.selected() == Some(last) {
            return false;
        }
        self.select(last)
    }

    /// The right-arrow verb: open a closed node, or step into an open one.
    ///
    /// Three cases, in the order a user expects them:
    ///
    /// 1. closed and openable ⇒ open it, selection stays put so the newly
    ///    revealed children appear *below* the thing that revealed them;
    /// 2. already open ⇒ move to its first child;
    /// 3. a leaf ⇒ nothing, and `false` so a face can decline to scroll.
    ///
    /// Deliberately does **not** fall through to the next visible row on a
    /// leaf. That would make right-arrow a second down-arrow at the bottom
    /// of every subtree, which silently walks the selection out of the
    /// branch the user was reading.
    pub fn move_right(&mut self, source: &mut S) -> bool {
        let Some(index) = self.selected() else {
            return false;
        };
        let Some(row) = self.row(index) else {
            return false;
        };

        if row.is_openable() {
            return self.expand(source, index);
        }
        if row.expanded {
            // The first child is always the next row: the projection is
            // depth-first pre-order, and an expanded row has at least one
            // child by construction.
            return self.select(index.saturating_add(1));
        }
        false
    }

    /// The left-arrow verb: close an open node, or step out to its parent.
    ///
    /// 1. open ⇒ close it;
    /// 2. closed or a leaf ⇒ select its parent;
    /// 3. a closed root ⇒ nothing.
    ///
    /// This is what makes left-arrow a reliable way out of a deep subtree:
    /// repeated presses alternate closing and ascending until it reaches a
    /// root, and then stops rather than wrapping.
    pub fn move_left(&mut self) -> bool {
        let Some(index) = self.selected() else {
            return false;
        };
        let Some(row) = self.row(index) else {
            return false;
        };

        if row.expanded {
            return self.collapse(index);
        }
        // Resolved before selecting: `parent_of` borrows immutably and
        // `select` mutably, so the two cannot be chained.
        let Some(parent) = self.parent_of(index) else {
            return false;
        };
        self.select(parent)
    }

    /// Closes the selected node's subtree and selects it, from anywhere
    /// inside that subtree.
    ///
    /// The verb for "I am lost, take me back up" — unlike
    /// [`move_left`](Tree::move_left) it does not first close the row the
    /// cursor is on, so a selected file inside three open directories lands
    /// on its own directory, closed, in one press.
    pub fn collapse_parent(&mut self) -> bool {
        let Some(index) = self.selected() else {
            return false;
        };
        let Some(parent) = self.parent_of(index) else {
            return false;
        };
        self.select(parent) && self.collapse(parent)
    }

    /// The row index of `index`'s parent, if it has one.
    ///
    /// Found by walking back to the nearest shallower row, which is the
    /// parent exactly because the projection is depth-first pre-order. Cost
    /// is the size of the preceding sibling subtrees, so it is bounded by
    /// what is on screen above the row rather than by the whole tree.
    #[must_use]
    pub fn parent_of(&self, index: usize) -> Option<usize> {
        let depth = self.row(index)?.depth;
        if depth == 0 {
            return None;
        }
        self.rows()
            .get(..index)?
            .iter()
            .rposition(|row| row.depth < depth)
    }
}
