//! Stage the entire route, then publish document, syntax and history together.

use crate::document::{CursorState, Document, compute_edit_span};
use crate::editor::ast::PreparedSyntaxEdit;
use crate::editor::{Editor, EditorEvent, IridiumError, SyntaxState};
use crate::history::{Command, NavigationRequest};

impl Editor {
    pub(in crate::editor) fn replay_history(&mut self, request: NavigationRequest) -> bool {
        self.finish_change();
        let scope_before = self
            .state
            .search
            .scope_range_count()
            .map(|_| self.state.document.clone());
        let state = &mut self.state;
        let result = state.history.navigate(request, |commands| {
            stage_replay(
                commands,
                &mut state.document,
                &mut state.cursor,
                &mut state.syntax,
            )
        });
        match result {
            Ok(Some(commands)) => {
                if let Some(before) = scope_before {
                    self.state.search.track_scope(
                        &before,
                        &commands,
                        &self.state.document,
                        Some(self.state.history.current_node_id()),
                    );
                }
                if let Some(last) = commands.last() {
                    self.finish_history_replay(last);
                }
                true
            },
            Ok(None) => false,
            Err(error) => {
                let code = match request {
                    NavigationRequest::Undo => "UNDO_FAILED",
                    NavigationRequest::Redo => "REDO_FAILED",
                    NavigationRequest::Branch(_) => "REDO_BRANCH_FAILED",
                    NavigationRequest::Jump(_) => "HISTORY_JUMP_FAILED",
                };
                self.emit(&EditorEvent::Error {
                    message: error.to_string(),
                    code: code.to_owned(),
                });
                false
            },
        }
    }
}

fn stage_replay(
    commands: &[Command],
    document: &mut Document,
    cursor: &mut CursorState,
    syntax: &mut SyntaxState,
) -> Result<(), IridiumError> {
    if commands.is_empty() {
        return Ok(());
    }
    let original_revision = document.revision();
    let mut staged_document = document.clone();
    let mut staged_cursor = cursor.clone();
    let mut edits = Some(Vec::new());
    for command in commands {
        let revision = staged_document.revision();
        let span = match compute_edit_span(&staged_document, command) {
            Ok(span) => span,
            Err(error) => {
                tracing::debug!(%error, "history edit span unavailable; successful replay will reparse fully");
                None
            },
        };
        command.apply(&mut staged_document, &mut staged_cursor)?;
        if staged_document.revision() != revision {
            let prepared = span.and_then(|span| PreparedSyntaxEdit::new(&staged_document, &span));
            match (edits.as_mut(), prepared) {
                (Some(edits), Some(edit)) => edits.push(edit),
                // One missing edit invalidates the whole incremental chain:
                // a later known edit must not hide the revision gap.
                _ => edits = None,
            }
        }
    }

    // Nothing above touched live state. All fallible work has now succeeded.
    *document = staged_document;
    *cursor = staged_cursor;
    if document.revision() != original_revision {
        if let Some(edits) = edits.filter(|_| syntax.tracks_revision(original_revision)) {
            for edit in edits {
                syntax.note_prepared_edit(&edit);
            }
        } else {
            syntax.invalidate();
        }
    }
    Ok(())
}
