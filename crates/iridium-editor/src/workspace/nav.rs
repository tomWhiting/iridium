//! Moving between tabs, and choosing what to activate when one closes.
//!
//! Every movement here is over the **flattened tab order**: the tabs as
//! they appear reading the organisation top to bottom, skipping groups.
//! That is the order the user sees, so it is the order ⌘⇧[ and ⌘⇧] must
//! follow — stepping through a group's children and then continuing past
//! its end, rather than stopping at a boundary the tabs themselves do not
//! show.
//!
//! **Movement clamps, it does not wrap**, for the same reason as the tree
//! crate's: held keys repeat, and a wrapping list jumps from the last tab
//! to the first while the user is still leaning on the key, which reads as
//! the workspace having lost their place.

use std::collections::HashSet;

use super::{Node, NodeId, Workspace};

impl<T> Workspace<T> {
    /// Every tab, in the order they are displayed.
    ///
    /// Depth-first pre-order over the organisation, keeping only tabs.
    /// Allocates, and is therefore for navigation and for building a tab
    /// strip — not for a per-frame lookup.
    #[must_use]
    pub fn tabs(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        // Reversed, so popping yields the roots in display order.
        let mut stack: Vec<NodeId> = self.roots().iter().rev().copied().collect();

        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                // A cycle cannot be built through the public API; this stops
                // a hang rather than asserting one is impossible.
                continue;
            }
            match self.node(id) {
                Some(Node::Tab { .. }) => out.push(id),
                Some(Node::Group { children, .. }) => {
                    stack.extend(children.iter().rev().copied());
                },
                None => {},
            }
        }
        out
    }

    /// How many tabs are open.
    ///
    /// Tabs, not documents: two tabs onto one file count twice, because the
    /// number this answers is "how many things can I switch between".
    #[must_use]
    pub fn tab_count(&self) -> usize {
        self.tabs().len()
    }

    /// Activates the next tab in display order, returning whether it moved.
    ///
    /// With nothing active this activates the first tab, so a workspace
    /// whose active tab was just closed to nothing still responds to the
    /// first key press rather than swallowing it.
    pub fn next_tab(&mut self) -> bool {
        let tabs = self.tabs();
        let target = self
            .active()
            .and_then(|active| position_of(&tabs, active))
            .map_or_else(
                || tabs.first().copied(),
                |index| tabs.get(index.saturating_add(1)).copied(),
            );
        target.is_some_and(|id| self.activate(id))
    }

    /// Activates the previous tab in display order, returning whether it
    /// moved.
    ///
    /// With nothing active this activates the *last* tab, mirroring
    /// [`next_tab`](Workspace::next_tab): the end reached should be the one
    /// nearest the key pressed.
    pub fn previous_tab(&mut self) -> bool {
        let tabs = self.tabs();
        let target = match self.active().and_then(|active| position_of(&tabs, active)) {
            None => tabs.last().copied(),
            Some(0) => None,
            Some(index) => tabs.get(index.saturating_sub(1)).copied(),
        };
        target.is_some_and(|id| self.activate(id))
    }

    /// Activates the first tab, returning whether it moved.
    pub fn first_tab(&mut self) -> bool {
        let target = self.tabs().first().copied();
        target.is_some_and(|id| self.active() != Some(id) && self.activate(id))
    }

    /// Activates the last tab, returning whether it moved.
    pub fn last_tab(&mut self) -> bool {
        let target = self.tabs().last().copied();
        target.is_some_and(|id| self.active() != Some(id) && self.activate(id))
    }

    /// The tab to activate once `id` and its subtree are gone.
    ///
    /// **The first tab after the removed subtree, else the last one before
    /// it, else nothing.** Forward first because closing a tab and carrying
    /// on is the common case and the eye is already to the right of what
    /// vanished; backward as the fallback so closing the last tab lands
    /// somewhere rather than nowhere.
    ///
    /// Must be called *before* the removal — afterwards the position is
    /// gone, and the best a search could do is "whatever is first", which
    /// moves the user across the workspace instead of one tab over.
    #[must_use]
    pub fn neighbour_of(&self, id: NodeId) -> Option<NodeId> {
        let doomed = self.subtree_of(id);
        let tabs = self.tabs();
        let first_doomed = tabs.iter().position(|tab| doomed.contains(tab))?;

        tabs.get(first_doomed..)
            .unwrap_or_default()
            .iter()
            .find(|tab| !doomed.contains(tab))
            .or_else(|| {
                tabs.get(..first_doomed)
                    .unwrap_or_default()
                    .iter()
                    .rev()
                    .find(|tab| !doomed.contains(tab))
            })
            .copied()
    }
}

/// Where `id` sits in `tabs`, if at all.
fn position_of(tabs: &[NodeId], id: NodeId) -> Option<usize> {
    tabs.iter().position(|tab| *tab == id)
}
