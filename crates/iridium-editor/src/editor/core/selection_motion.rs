//! Resolve arrow destinations at the selected structural granularity.
use super::Editor;
use crate::commands::{
    CommandInvocation,
    builtin::{
        CURSOR_CHAR_LEFT, CURSOR_CHAR_RIGHT, CURSOR_LINE_DOWN, CURSOR_LINE_UP, CURSOR_PAGE_DOWN,
        CURSOR_PAGE_UP,
    },
};
use crate::{CursorState, history::Command, input::KeyResult};
impl Editor {
    pub(super) fn preserve_structural_selection(
        &mut self,
        result: KeyResult,
        invocation: Option<&CommandInvocation>,
    ) -> KeyResult {
        if !self.keyboard_handler.preserves_vertical_selection() {
            return result;
        }
        let Some(invocation) = invocation else {
            return result;
        };
        let id = invocation.command();
        if [CURSOR_CHAR_LEFT, CURSOR_CHAR_RIGHT].contains(id)
            && self
                .state
                .cursor
                .all_selections()
                .any(|selection| !selection.is_collapsed())
            && matches!(
                &result,
                KeyResult::Command(Command::SetSelection { .. }) | KeyResult::Handled
            )
        {
            let old_state = self.state.cursor.clone();
            let next = self.state.syntax.retarget_horizontal(
                &self.state.document,
                &old_state,
                id == &CURSOR_CHAR_RIGHT,
                invocation.args().repeat_count(),
            );
            return if next == old_state {
                KeyResult::Handled
            } else {
                KeyResult::Command(Command::SetSelection {
                    old_state,
                    new_state: next,
                })
            };
        }
        if ![
            CURSOR_LINE_UP,
            CURSOR_LINE_DOWN,
            CURSOR_PAGE_UP,
            CURSOR_PAGE_DOWN,
        ]
        .contains(id)
        {
            return result;
        }
        let KeyResult::Command(Command::SetSelection {
            old_state,
            new_state,
        }) = result
        else {
            return result;
        };
        let next = if old_state
            .all_selections()
            .any(|selection| !selection.is_collapsed())
        {
            self.state.syntax.retarget_vertical(
                &self.state.document,
                &old_state,
                &new_state,
                id == &CURSOR_LINE_DOWN || id == &CURSOR_PAGE_DOWN,
            )
        } else {
            CursorState::from_selections(new_state.primary, new_state.secondary.clone())
        };
        self.keyboard_handler
            .rekey_vertical_selection(&new_state, &next);
        if next == old_state {
            KeyResult::Handled
        } else {
            KeyResult::Command(Command::SetSelection {
                old_state,
                new_state: next,
            })
        }
    }
}
