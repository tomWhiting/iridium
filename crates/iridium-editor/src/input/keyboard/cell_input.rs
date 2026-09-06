//! Cell dispatch reuses resolved command IDs and preflights reversible edits.

use crate::commands::CommandArgs;
use crate::document::{CursorState, Document};
use crate::editor::{CaretScopes, CellInputError, CellInputOptions, EditorConfig, FoldState};

use super::actions::{CommandContext, action_for};
pub use super::cell_edits::{
    PreparedCellEdit, canonical_end, prepare_command, prepare_paste, prepare_result,
    validate_cursor, validate_range,
};
use super::dispatch::ResolvedKey;
use super::{CommandRunError, KeyEvent, KeyResult, KeyboardHandler};

/// Borrowed state for one operation. No geometry is retained for later mutation.
pub struct CellInputContext<'a> {
    pub document: &'a Document,
    pub cursor: &'a CursorState,
    pub config: &'a EditorConfig,
    pub scopes: &'a CaretScopes<'a>,
    pub folds: &'a FoldState,
    pub options: CellInputOptions,
    pub read_only: bool,
}

impl KeyboardHandler {
    pub(crate) fn handle_cell_key(
        &mut self,
        event: &KeyEvent,
        cell: &CellInputContext<'_>,
    ) -> Result<KeyResult, CellInputError> {
        validate_cursor(cell.document, cell.cursor)?;
        match self.resolve_key(event) {
            ResolvedKey::Action(action, args) => {
                let context = CommandContext {
                    event: Some(event),
                    args,
                    document: cell.document,
                    cursor: cell.cursor,
                    config: cell.config,
                    scopes: cell.scopes,
                };
                self.dispatch_cell_action(action, &context, cell)
            },
            ResolvedKey::Result(result) => Ok(result),
        }
    }

    pub(crate) fn run_cell_command(
        &mut self,
        id: &str,
        args: CommandArgs,
        cell: &CellInputContext<'_>,
    ) -> Result<KeyResult, CellInputError> {
        validate_cursor(cell.document, cell.cursor)?;
        let action =
            action_for(id).ok_or_else(|| CommandRunError::Unimplemented { id: id.to_owned() })?;
        let context = CommandContext {
            event: None,
            args,
            document: cell.document,
            cursor: cell.cursor,
            config: cell.config,
            scopes: cell.scopes,
        };
        self.dispatch_cell_action(action, &context, cell)
    }

    pub(crate) fn reset_cell_state(&mut self) {
        self.cell_navigation = None;
        self.cell_geometry_dirty = false;
    }

    /// Invalidates without dropping allocations, preserving the const fold API.
    pub(crate) const fn invalidate_cell_geometry(&mut self) {
        self.cell_geometry_dirty = true;
    }
}
