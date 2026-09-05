//! Opt-in cell input on the existing editor, command registry and undo tree.

use thiserror::Error;

use super::{Editor, EditorKeyResult, IridiumError};
use crate::cell_layout::{
    Affinity, CellColumn, CellLayoutError, CellRowMap, CellWrapParameters, ScreenRow,
};
use crate::commands::CommandArgs;
use crate::document::{CursorState, Selection};
use crate::history::Command;
use crate::input::keyboard::cell_input::{CellInputContext, prepare_result, validate_cursor};
use crate::input::{CommandRunError, HistoryRequest, KeyEvent, KeyResult, SearchAction};

/// Explicit geometry for one cell input operation; the host owns focus and send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellInputOptions {
    /// Text width and tab stops, identical to the face's prepared layout.
    pub wrap: CellWrapParameters,
    /// Actual visible text rows. Zero-height page motions do not move.
    pub visible_rows: usize,
}

/// A rejected cell operation has not changed the live document or history.
#[derive(Debug, Error)]
pub enum CellInputError {
    /// Invalid coordinates or cell geometry.
    #[error(transparent)]
    Layout(#[from] CellLayoutError),
    /// The named command belongs to the host, as with legacy direct dispatch.
    #[error(transparent)]
    Command(#[from] CommandRunError),
    /// A prepared reversible edit could not be applied.
    #[error(transparent)]
    Edit(#[from] IridiumError),
}

impl Editor {
    /// Resolves a key through the current keymap with cell-aware editor actions.
    ///
    /// No send key is built in. Host commands, chords and repeat suppression use
    /// the same resolver as `handle_key`. Grapheme-interior selections fail
    /// before a command changes the document or history.
    pub fn handle_cell_key(
        &mut self,
        event: &KeyEvent,
        options: CellInputOptions,
    ) -> Result<EditorKeyResult, CellInputError> {
        let scopes = self.state.syntax.caret_scopes(&self.state.document);
        let context = CellInputContext {
            document: &self.state.document,
            cursor: &self.state.cursor,
            config: &self.state.config,
            scopes: &scopes,
            folds: &self.state.fold_state,
            options,
            read_only: self.state.read_only,
        };
        let result = self.keyboard_handler.handle_cell_key(event, &context)?;
        self.consume_cell_result(result)
    }

    /// Runs a resolved command ID with the same cell semantics as a key binding.
    pub fn run_cell_command(
        &mut self,
        id: &str,
        args: CommandArgs,
        options: CellInputOptions,
    ) -> Result<EditorKeyResult, CellInputError> {
        let scopes = self.state.syntax.caret_scopes(&self.state.document);
        let context = CellInputContext {
            document: &self.state.document,
            cursor: &self.state.cursor,
            config: &self.state.config,
            scopes: &scopes,
            folds: &self.state.fold_state,
            options,
            read_only: self.state.read_only,
        };
        let result = self.keyboard_handler.run_cell_command(id, args, &context)?;
        self.consume_cell_result(result)
    }

    /// Pastes the original text once through the reversible multi-cursor builder.
    ///
    /// Paste does not resolve keys or produce host submit commands. Normalizing
    /// the resulting selections is part of the same command and undo entry.
    pub fn paste_cells(&mut self, text: &str) -> Result<(), CellInputError> {
        validate_cursor(&self.state.document, &self.state.cursor)?;
        if self.state.read_only || text.is_empty() {
            return Ok(());
        }
        let result =
            self.keyboard_handler
                .handle_paste(text, &self.state.document, &self.state.cursor);
        let result = prepare_result(result, &self.state.document, &self.state.cursor)?;
        self.keyboard_handler.reset_vertical_state();
        self.keyboard_handler.invalidate_cursor_order();
        self.consume_key_result(result);
        Ok(())
    }

    /// Moves the primary selection using fresh current geometry, never a saved hit.
    ///
    /// `row` is a document screen row; the face adds its scroll offset before
    /// calling. Extending keeps the primary anchor and drops secondary cursors.
    pub fn set_cell_pointer(
        &mut self,
        row: ScreenRow,
        column: CellColumn,
        extend: bool,
        options: CellInputOptions,
    ) -> Result<(), CellInputError> {
        validate_cursor(&self.state.document, &self.state.cursor)?;
        let hit = CellRowMap::prepare(&self.state.document, &self.state.fold_state, options.wrap)?
            .position_at(row, column)?;
        let old_state = self.state.cursor.clone();
        let selection = if extend {
            Selection::new(old_state.primary.anchor, hit.position)
        } else {
            Selection::collapsed(hit.position)
        };
        let new_state = CursorState::new(selection);
        self.keyboard_handler.reset_vertical_state();
        self.keyboard_handler.invalidate_cursor_order();
        self.apply_command_internal(Command::SetSelection {
            old_state,
            new_state,
        });
        self.keyboard_handler
            .remember_cell_pointer(&self.state, options, hit.affinity);
        Ok(())
    }

    /// Current primary soft-break affinity, or Downstream after invalidation.
    #[must_use]
    pub fn cell_affinity(&self, options: CellInputOptions) -> Affinity {
        self.keyboard_handler.cell_affinity(&self.state, options)
    }

    /// Current affinity for the cursor at `index` in `all_selections` order.
    /// None names a nonexistent cursor; stale geometry yields Downstream for
    /// an existing cursor, exactly as the primary-only accessor does.
    #[must_use]
    pub fn cell_cursor_affinity(
        &self,
        index: usize,
        options: CellInputOptions,
    ) -> Option<Affinity> {
        self.keyboard_handler
            .cell_cursor_affinity(&self.state, options, index)
    }

    fn consume_cell_result(
        &mut self,
        result: KeyResult,
    ) -> Result<EditorKeyResult, CellInputError> {
        match result {
            // Replaying content also moves the undo tree. Refuse it before
            // either changes; selecting the next/previous redo branch remains
            // available because it changes neither content nor the cursor.
            KeyResult::History(
                HistoryRequest::Undo | HistoryRequest::Redo | HistoryRequest::RedoBranch(_),
            ) if self.state.read_only => Ok(EditorKeyResult::None),
            KeyResult::Ast(request) => {
                let next = self.state.syntax.apply_ast_request(
                    &self.state.document,
                    &self.state.cursor,
                    request,
                );
                if let Some(new_state) = next {
                    let result = KeyResult::Command(Command::SetSelection {
                        old_state: self.state.cursor.clone(),
                        new_state,
                    });
                    let result = prepare_result(result, &self.state.document, &self.state.cursor)?;
                    Ok(self.consume_key_result(result))
                } else {
                    Ok(EditorKeyResult::None)
                }
            },
            KeyResult::Search(action @ (SearchAction::NextMatch | SearchAction::PreviousMatch)) => {
                self.cell_search(action)
            },
            other => Ok(self.consume_key_result(other)),
        }
    }

    fn cell_search(&mut self, action: SearchAction) -> Result<EditorKeyResult, CellInputError> {
        let mut search = self.state.search.clone();
        match action {
            SearchAction::NextMatch => {
                search.next_match();
            },
            SearchAction::PreviousMatch => {
                search.previous_match();
            },
            SearchAction::OpenSearch | SearchAction::CloseSearch => {
                return Ok(self.consume_key_result(KeyResult::Search(action)));
            },
        }
        let command = search.current_range().map(|range| {
            KeyResult::Command(Command::SetSelection {
                old_state: self.state.cursor.clone(),
                new_state: CursorState::at(range.start),
            })
        });
        let prepared = command
            .map(|result| prepare_result(result, &self.state.document, &self.state.cursor))
            .transpose()?;
        self.state.search = search;
        self.keyboard_handler.reset_vertical_state();
        self.keyboard_handler.invalidate_cursor_order();
        if let Some(result) = prepared {
            self.consume_key_result(result);
        }
        self.emit_search_updated();
        Ok(EditorKeyResult::None)
    }
}
