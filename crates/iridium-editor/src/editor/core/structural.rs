//! Counted structural navigation stops when a step has no target.

use crate::Editor;
use crate::history::Command;
use crate::input::AstRequest;

impl Editor {
    /// Repeats a structural motion, returning whether any selection changed.
    ///
    /// Tree boundaries stop the traversal immediately, including when no parser
    /// is available. A zero count does nothing. Hosts share this implementation.
    pub fn perform_ast_request_repeated(&mut self, request: AstRequest, count: u32) -> bool {
        if count == 0 {
            return false;
        }
        let mut next = self.state.cursor.clone();
        for _ in 0..count {
            let Some(step) =
                self.state
                    .syntax
                    .apply_ast_request(&self.state.document, &next, request)
            else {
                break;
            };
            next = step;
        }
        if next == self.state.cursor {
            return false;
        }
        // Publish only the final selection; a count is one user action.
        self.apply_command_internal(Command::SetSelection {
            old_state: self.state.cursor.clone(),
            new_state: next,
        });
        true
    }
}
