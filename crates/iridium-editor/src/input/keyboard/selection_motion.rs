//! Structural vertical movement starts at each object's leading edge.
use super::KeyboardHandler;
use crate::commands::structural_keymap::{STRUCTURAL, STRUCTURAL_VERB};
use crate::{CursorState, Position, Selection};
impl KeyboardHandler {
    pub(crate) fn preserves_vertical_selection(&self) -> bool {
        matches!(self.mode(), Some(mode) if mode == &STRUCTURAL || mode == &STRUCTURAL_VERB)
    }
    pub(super) fn vertical_origin(&self, selection: &Selection, extend: bool) -> Position {
        if !extend && self.preserves_vertical_selection() {
            selection.start()
        } else {
            selection.head
        }
    }
    pub(super) fn has_sticky_columns(&self, cursor: &CursorState, revision: u64) -> bool {
        !self.sticky_dirty
            && self.sticky_state.as_ref() == Some(cursor)
            && self.sticky_revision == Some(revision)
            && self
                .preferred_columns
                .as_ref()
                .is_some_and(|columns| columns.len() == cursor.cursor_count())
    }
    pub(crate) fn rekey_vertical_selection(&mut self, before: &CursorState, after: &CursorState) {
        if self.sticky_state.as_ref() == Some(before) {
            if before.cursor_count() == after.cursor_count() {
                self.sticky_state = Some(after.clone());
            } else {
                self.reset_vertical_state();
            }
        }
    }
}
/// Keep duplicate targets until the editor resolves their syntax selections.
pub(super) fn map_targets(
    cursor: &CursorState,
    extend: bool,
    preserve: bool,
    mut motion: impl FnMut(usize, &Selection) -> Position,
) -> CursorState {
    let mut result = cursor.clone();
    for (index, selection) in result.all_selections_mut().enumerate() {
        let head = motion(index, selection);
        *selection = Selection::new(if extend { selection.anchor } else { head }, head);
    }
    if preserve {
        result
    } else {
        CursorState::from_selections(result.primary, result.secondary)
    }
}
