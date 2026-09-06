//! Typed host replacement of one cell-safe range, with exact reversible selection.

use super::{CellInputError, Editor};
use crate::document::{CursorState, Range};
use crate::history::Command;
use crate::input::keyboard::cell_input::{
    canonical_end, prepare_command, validate_cursor, validate_range,
};

/// Where a successful host replacement leaves the complete cursor state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellReplacementCursor {
    /// Collapse to the first complete grapheme boundary at or after inserted text.
    /// Extra cursors are removed; a new CRLF pair is treated as one line ending.
    EndOfReplacement,
    /// Restore exactly these post-edit selections, including direction and extras.
    /// Every endpoint must already be a valid complete-grapheme boundary.
    Exact(CursorState),
}

impl Editor {
    /// Replace one explicit range without resetting the editor or its undo tree.
    ///
    /// The text is preserved byte-for-byte. Validation and reversible command
    /// preparation finish before live state, history, transient input or events
    /// change. A successful nonempty transaction is isolated from adjacent typing
    /// without changing the configured grouping timeout or deleting redo branches.
    /// Identical text and cursor is a no-op; cursor-only changes are undoable.
    ///
    /// # Errors
    /// Returns a typed error for read-only state, invalid/inverted/interior ranges,
    /// invalid current or exact final selections, or failed command preparation.
    pub fn replace_cell_range(
        &mut self,
        range: Range,
        replacement: &str,
        cursor: CellReplacementCursor,
    ) -> Result<(), CellInputError> {
        if self.state.read_only {
            return Err(CellInputError::ReadOnly);
        }
        validate_cursor(&self.state.document, &self.state.cursor)?;
        validate_range(&self.state.document, range)?;
        let start = self
            .state
            .document
            .position_to_offset(range.start)
            .ok_or(CellInputError::InvalidRange { range })?;
        let end = start
            .checked_add(replacement.len())
            .ok_or(CellInputError::InvalidOffset { offset: start })?;
        let old_text = self.state.document.slice(range);
        let command = if old_text == replacement {
            Command::Compound {
                commands: Vec::new(),
            }
        } else {
            Command::Replace {
                range,
                old_text,
                new_text: replacement.to_owned(),
            }
        };
        let prepared = prepare_command(command, &self.state.document, &self.state.cursor)?;
        let new_state = match cursor {
            CellReplacementCursor::EndOfReplacement => {
                CursorState::at(canonical_end(&prepared.document, end)?)
            },
            CellReplacementCursor::Exact(state) => state,
        };
        let prepared = prepared.select(new_state)?;
        self.commit_cell_edit(prepared);
        Ok(())
    }
}
