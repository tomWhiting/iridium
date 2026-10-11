//! Explicit selection replacement and visibility without changing edit targets.
use super::{CursorState, Editor, Position, Selection};

impl Editor {
    /// The syntax kind of the exact primary selection, if it matches a node
    /// in the current parse tree. Partial, collapsed and parser-free selections
    /// return `None`. Reading this never triggers parsing.
    #[must_use]
    #[cfg(feature = "syntax")]
    pub fn selected_syntax_kind(&self) -> Option<&'static str> {
        self.state
            .syntax
            .selection_kind(&self.state.document, self.state.cursor.primary)
    }

    /// Parser-free builds have no syntax selection to describe.
    #[must_use]
    #[cfg(not(feature = "syntax"))]
    pub const fn selected_syntax_kind(&self) -> Option<&'static str> {
        None
    }

    /// Sets a single collapsed cursor, clamped to the document.
    /// Explicit selection changes reset keyboard navigation state.
    pub fn set_cursor(&mut self, position: Position) {
        self.set_selections(CursorState::at(position));
    }

    /// Replaces all cursors with one selection, clamping both endpoints.
    pub fn set_selection(&mut self, anchor: Position, head: Position) {
        self.set_selections(CursorState::new(Selection::new(anchor, head)));
    }

    /// Replaces all selections, clamping endpoints and merging overlapping targets.
    /// Preserves the supplied primary selection and resets transient keyboard state.
    /// Like `set_cursor`, this does not edit text or add an undo entry.
    pub fn set_selections(&mut self, mut cursors: CursorState) {
        self.finish_change();
        for selection in cursors.all_selections_mut() {
            selection.anchor = self.state.document.clamp_position(selection.anchor);
            selection.head = self.state.document.clamp_position(selection.head);
        }
        self.keyboard_handler.reset_vertical_state();
        self.keyboard_handler.invalidate_cursor_order();
        self.state.cursor = CursorState::from_selections(cursors.primary, cursors.secondary);
        self.emit_selection_changed();
    }

    /// Opens folds hiding selection heads without moving or collapsing selections.
    /// One fold update handles all heads; keyboard intentions remain untouched.
    pub fn reveal_selection_heads(&mut self) -> bool {
        if self.state.fold_state.folded_count() == 0 {
            return false;
        }
        let lines: Vec<_> = self
            .state
            .cursor
            .all_selections()
            .filter(|selection| self.state.fold_state.is_line_hidden(selection.head.line))
            .map(|selection| selection.head.line)
            .collect();
        let changed = self.state.fold_state.unfold_containing_lines(&lines);
        if changed {
            self.emit_fold_changed();
        }
        changed
    }
}

#[cfg(test)]
mod tests;
