//! Cell actions wrap the existing action table, never physical key bindings.

use crate::document::{CursorState, Selection};
use crate::editor::CellInputError;

use super::actions::{CommandContext, KeyboardAction};
use super::cell_input::CellInputContext;
use super::cell_navigation::CellMotion;
use super::{KeyResult, KeyboardHandler, backspace_pairs, cell_edits, editing};

use KeyboardAction as Action;

impl KeyboardHandler {
    pub(super) fn dispatch_cell_action(
        &mut self,
        action: KeyboardAction,
        context: &CommandContext<'_>,
        cell: &CellInputContext<'_>,
    ) -> Result<KeyResult, CellInputError> {
        let previous_pair = self.auto_pair.clone();
        let result = self
            .run_cell_action(action, context, cell)
            .and_then(|result| cell_edits::prepare_result(result, cell.document, cell.cursor));
        match result {
            Ok(result) => {
                // A command that cannot land must not claim it wrote a closer.
                if cell.read_only {
                    self.auto_pair = previous_pair;
                }
                let navigation = self.cell_navigation.take();
                self.note_operation(Some(action), &result);
                if CellMotion::for_action(action).is_some()
                    || matches!(
                        action,
                        Action::ClipboardCopy | Action::NoOp | Action::SearchOpen
                    )
                {
                    self.cell_navigation = navigation;
                }
                Ok(result)
            },
            Err(error) => {
                self.auto_pair = previous_pair;
                self.reset_cell_state();
                Err(error)
            },
        }
    }

    fn run_cell_action(
        &mut self,
        action: KeyboardAction,
        context: &CommandContext<'_>,
        cell: &CellInputContext<'_>,
    ) -> Result<KeyResult, CellInputError> {
        if let Some(motion) = CellMotion::for_action(action) {
            return self.cell_motion(motion, context.args.repeat_count(), cell);
        }
        let horizontal = match action {
            Action::CharLeft => Some((false, false, false)),
            Action::CharLeftSelect => Some((false, false, true)),
            Action::CharRight => Some((true, false, false)),
            Action::CharRightSelect => Some((true, false, true)),
            Action::WordLeft => Some((false, true, false)),
            Action::WordLeftSelect => Some((false, true, true)),
            Action::WordRight => Some((true, true, false)),
            Action::WordRightSelect => Some((true, true, true)),
            _ => None,
        };
        if let Some((forward, word, extend)) = horizontal {
            return Self::cell_horizontal(context, forward, word, extend);
        }
        let mut edits = match action {
            Action::DeleteBackward if context.config.auto_pairs => {
                backspace_pairs::backspace_edits_with_pairs(
                    context.document,
                    context.cursor,
                    self.auto_pair.as_ref(),
                )
            },
            Action::DeleteBackward => {
                editing::backspace_edits(context.document, context.cursor, false)
            },
            Action::DeleteWordBackward => {
                editing::backspace_edits(context.document, context.cursor, true)
            },
            Action::DeleteForward => {
                editing::delete_forward_edits(context.document, context.cursor, false)
            },
            Action::DeleteWordForward => {
                editing::delete_forward_edits(context.document, context.cursor, true)
            },
            Action::DeleteToLineStart => editing::delete_to_line_start_edits(context.cursor),
            Action::DeleteToLineEnd => {
                editing::delete_to_line_end_edits(context.document, context.cursor)
            },
            _ => return Ok(self.run_action(action, context)),
        };
        cell_edits::expand_edits(context.document, &mut edits)?;
        Ok(
            editing::build_multi_cursor_command(context.document, context.cursor, edits)
                .map_or(KeyResult::Handled, KeyResult::Command),
        )
    }

    fn cell_horizontal(
        context: &CommandContext<'_>,
        forward: bool,
        word: bool,
        extend: bool,
    ) -> Result<KeyResult, CellInputError> {
        let move_selection = |selection: Selection| -> Result<Selection, CellInputError> {
            let mut head = selection.head;
            for _ in 0..context.args.repeat_count() {
                let next = cell_edits::step(context.document, head, forward, word)?;
                if next == head {
                    break;
                }
                head = next;
            }
            Ok(if extend {
                Selection::new(selection.anchor, head)
            } else {
                Selection::collapsed(head)
            })
        };
        let mut state = CursorState::new(move_selection(context.cursor.primary)?);
        // Primary already moved above; preserve secondary sorting/merge rules.
        for selection in &context.cursor.secondary {
            state.add_cursor(move_selection(*selection)?);
        }
        Ok(Self::create_selection_command(context.cursor, &state))
    }
}
