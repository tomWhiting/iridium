//! Cursor addition order and active-selection transitions.
use std::collections::HashSet;

use super::KeyboardHandler;
use crate::document::{CursorState, Selection};

#[derive(Debug, Clone, Copy)]
pub(super) struct CursorAddition {
    pub selection: Selection,
    pub previous_primary: Selection,
}

impl KeyboardHandler {
    /// Promotes an existing selection without changing the edit targets.
    pub(super) fn focus_selection(cursor: &mut CursorState, focus: Selection) {
        if let Some(index) = cursor
            .secondary
            .iter()
            .position(|selection| *selection == focus)
        {
            std::mem::swap(&mut cursor.primary, &mut cursor.secondary[index]);
            cursor.sort();
        }
    }

    /// Removes a tracked addition, restoring its previous primary if needed.
    pub(super) fn remove_added_cursor(cursor: &CursorState, entry: CursorAddition) -> CursorState {
        let mut remaining = cursor
            .all_selections()
            .copied()
            .filter(|selection| *selection != entry.selection);
        let Some(primary) = remaining.next() else {
            return cursor.clone();
        };
        let mut state = CursorState::from_selections(primary, remaining.collect());
        if cursor.primary == entry.selection {
            Self::focus_selection(&mut state, entry.previous_primary);
        }
        state
    }

    /// Discards stale addition identities after edits or unrelated motions.
    pub(super) fn sync_add_order(&mut self, cursor: &CursorState, revision: u64) {
        if self.add_order_anchor.as_ref() != Some(cursor)
            || self.add_order_revision != Some(revision)
        {
            self.cursor_add_order.clear();
            self.add_order_anchor = Some(cursor.clone());
            self.add_order_revision = Some(revision);
            return;
        }
        let selections: HashSet<_> = cursor.all_selections().copied().collect();
        self.cursor_add_order
            .retain(|entry| selections.contains(&entry.selection));
    }

    /// Tracks fresh targets, recording the active addition last. Reshaping the
    /// reference word for select-all or Ctrl+D is not itself an addition.
    pub(super) fn record_cursor_change(&mut self, old: &CursorState, new_state: &CursorState) {
        let current: HashSet<_> = new_state.all_selections().copied().collect();
        let previous: HashSet<_> = old.all_selections().copied().collect();
        self.cursor_add_order
            .retain(|entry| current.contains(&entry.selection));
        let mut previous_primary = old.primary;
        for selection in &new_state.secondary {
            if !previous.contains(selection) {
                self.cursor_add_order.push(CursorAddition {
                    selection: *selection,
                    previous_primary,
                });
                previous_primary = *selection;
            }
        }
        if new_state.primary != old.primary
            && new_state.secondary.contains(&old.primary)
            && !previous.contains(&new_state.primary)
        {
            self.cursor_add_order.push(CursorAddition {
                selection: new_state.primary,
                previous_primary,
            });
        }
        self.add_order_anchor = Some(new_state.clone());
    }
}
