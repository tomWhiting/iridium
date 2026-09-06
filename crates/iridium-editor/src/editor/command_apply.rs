//! Common successful command effects; prepared cell edits publish one validated state.

use super::{Editor, EditorEvent};
use crate::document::{EditSpan, compute_edit_span};
use crate::history::Command;
use crate::input::keyboard::cell_input::PreparedCellEdit;

/// Legacy edits group content; explicit cell gestures isolate the whole transaction.
#[derive(Clone, Copy)]
enum HistoryRecording {
    GroupedContent,
    IsolatedTransaction,
}

impl Editor {
    /// Applies a command without touching the keyboard handler's transient
    /// state.
    ///
    /// Used by the input paths that own that state themselves: the keyboard
    /// handler (which runs its own post-dispatch bookkeeping in
    /// `note_operation`, and for the add verbs *must* keep its addition-order
    /// stack across command application) and the mouse/IME/paste/search paths
    /// (which reset the sticky column and addition-order stack explicitly before
    /// calling this).
    pub(super) fn apply_command_internal(&mut self, command: Command) {
        // Computed before the edit lands, because the span is expressed in the
        // pre-edit document's coordinates and that document is about to stop
        // existing. An error here is not fatal: leaving the edit unreported
        // makes the next sync parse the document whole, which is slower and
        // still correct.
        let span = compute_edit_span(&self.state.document, &command)
            .ok()
            .flatten();

        // Apply the command
        if let Err(e) = command.apply(&mut self.state.document, &mut self.state.cursor) {
            self.emit(&EditorEvent::Error {
                message: e.to_string(),
                code: "COMMAND_FAILED".to_string(),
            });
            return;
        }

        self.finish_command(command, span, HistoryRecording::GroupedContent);
    }

    /// No fallible operation remains: preparation belongs to this synchronous borrow.
    pub(super) fn commit_cell_edit(&mut self, prepared: PreparedCellEdit) {
        if prepared.command.is_empty() {
            return;
        }
        self.keyboard_handler.reset_vertical_state();
        self.keyboard_handler.invalidate_cursor_order();
        self.state.document = prepared.document;
        self.state.cursor = prepared.cursor;
        self.finish_command(
            prepared.command,
            prepared.span,
            HistoryRecording::IsolatedTransaction,
        );
    }

    fn finish_command(
        &mut self,
        command: Command,
        span: Option<EditSpan>,
        recording: HistoryRecording,
    ) {
        let content_changed = command.modifies_content();
        let selection_changed = command.modifies_selection();
        if let Some(span) = span {
            self.state.syntax.note_edit(&self.state.document, &span);
        }
        if content_changed {
            // The next frame reads folds and syntax; search still has old ranges.
            self.state.refresh_syntax();
        }
        match recording {
            HistoryRecording::GroupedContent if content_changed => self.state.history.push(command),
            HistoryRecording::IsolatedTransaction => self.state.history.push_isolated(command),
            HistoryRecording::GroupedContent => {},
        }
        if content_changed {
            if self.state.refresh_search() {
                self.emit_search_updated();
            }
            self.emit_content_changed();
        }
        if selection_changed || content_changed {
            self.emit_selection_changed();
        }
    }
}
