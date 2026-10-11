//! Commit a command before publishing derived state or changing input policy.

use super::{Editor, EditorEvent};
use crate::document::compute_edit_span;
use crate::history::Command;

/// The caller that owns keyboard transient state decides when to invalidate it.
#[derive(Clone, Copy)]
enum InputPolicy {
    Reset,
    Preserve,
    IsolatedUndo,
}

impl Editor {
    /// Applies a command to the document (public, host-driven entry point).
    ///
    /// This is the out-of-band command surface for host code (apply a bare
    /// `Insert`/`Delete`/`Replace`, or push a `SetSelection`). Like every other
    /// successful host-driven mutation it invalidates the keyboard handler's
    /// transient state — the sticky vertical column and the multi-cursor
    /// addition-order stack — so a `SetSelection` that reconstructs a
    /// byte-identical `CursorState` cannot leave Ctrl+U/skip acting on a stale
    /// stack. The internal keyboard/mouse/IME/paste paths deliberately bypass
    /// this via [`Self::apply_command_internal`]: they manage that transient
    /// state themselves (the keyboard handler through its own post-dispatch
    /// bookkeeping, the others by resetting it explicitly before applying).
    ///
    /// Document modifications and selection changes both emit appropriate
    /// events; a rejected command emits an error and leaves document, history,
    /// selections and keyboard motion state unchanged.
    pub fn apply_command(&mut self, command: Command) {
        self.finish_change();
        self.apply_command_with_input_policy(command, InputPolicy::Reset);
    }

    /// Applies a command without touching the keyboard handler's transient
    /// state.
    ///
    /// Used by the input paths that own that state themselves: the keyboard
    /// handler (which runs its own post-dispatch bookkeeping in
    /// `note_operation`, and for the add verbs *must* keep its addition-order
    /// stack across command application) and the mouse/IME/paste/search paths
    /// (which reset the sticky column and addition-order stack explicitly before
    /// calling this).
    pub(super) fn apply_command_internal(&mut self, command: Command) -> bool {
        self.apply_command_with_input_policy(command, InputPolicy::Preserve)
    }

    pub(super) fn apply_sequence_command(&mut self, command: Command) -> bool {
        self.apply_command_with_input_policy(command, InputPolicy::IsolatedUndo)
    }

    fn apply_command_with_input_policy(&mut self, command: Command, input: InputPolicy) -> bool {
        let content_changed = command.modifies_content();
        let selection_changed = command.modifies_selection();
        let scope_before = (content_changed && self.state.search.scope_range_count().is_some())
            .then(|| self.state.document.clone());

        // Computed before the edit lands, because the span is expressed in the
        // pre-edit document's coordinates and that document is about to stop
        // existing. An error here is not fatal: leaving the edit unreported
        // makes the next sync parse the document whole, which is slower and
        // still correct.
        let span = match compute_edit_span(&self.state.document, &command) {
            Ok(span) => span,
            // Explicit full-parse fallback: sequential host edits can refer to
            // positions created by an earlier child, absent in this document.
            Err(error) => {
                tracing::debug!(%error, "edit span unavailable; successful edit will reparse fully");
                None
            },
        };

        // Apply the command
        if let Err(e) = command.apply(&mut self.state.document, &mut self.state.cursor) {
            self.emit(&EditorEvent::Error {
                message: e.to_string(),
                code: "COMMAND_FAILED".to_string(),
            });
            return false;
        }

        // Only committed host edits invalidate preferred columns and addition
        // order. A rejected edit must not change the next navigation command.
        if matches!(input, InputPolicy::Reset) {
            self.keyboard_handler.reset_vertical_state();
            self.keyboard_handler.invalidate_cursor_order();
        }

        if let Some(span) = span {
            self.state.syntax.note_edit(&self.state.document, &span);
        }

        // Push to undo history if it modifies content
        if content_changed {
            // Folds are read by the renderer on the very next frame, so they
            // are refreshed here rather than lazily. Before this the editor's
            // regions were correct only until the first keystroke.
            self.state.refresh_syntax();

            if let Some(before) = scope_before {
                self.state.search.track_scope(
                    &before,
                    std::slice::from_ref(&command),
                    &self.state.document,
                    None,
                );
            }
            if matches!(input, InputPolicy::IsolatedUndo) {
                self.state.history.finish_group();
            }
            self.state.history.push(command);
            if matches!(input, InputPolicy::IsolatedUndo) {
                self.state.history.finish_group();
            }
            self.state
                .search
                .remember_scope(self.state.history.current_node_id());

            // Recorded search match ranges point into the pre-edit document;
            // re-synchronize the active search so replace operations never
            // act on stale ranges.
            if self.state.refresh_search() {
                self.emit_search_updated();
            }

            self.emit_content_changed();
        } else if selection_changed && self.change_session.is_some() {
            self.state.history.push(command);
        }

        // Emit selection changed if needed
        if selection_changed || content_changed {
            self.emit_selection_changed();
        }
        true
    }
}
