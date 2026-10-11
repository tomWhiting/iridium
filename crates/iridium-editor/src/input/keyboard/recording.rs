//! Capture the resolved command identity, not the key that happened to name it.
use crate::commands::builtin::EDIT_INSERT_CHARACTER;
use crate::commands::{CommandArgs, CommandInvocation};
use crate::{KeyCode, KeyEvent};

pub(super) fn invocation_for_event(
    invocation: &CommandInvocation,
    event: &KeyEvent,
) -> CommandInvocation {
    if invocation.command() == &EDIT_INSERT_CHARACTER {
        if let KeyCode::Char(character) = event.key {
            return CommandInvocation::new(
                EDIT_INSERT_CHARACTER,
                CommandArgs::with_text(character.to_string()),
            );
        }
    }
    invocation.clone()
}

impl super::KeyboardHandler {
    /// Takes the last key's resolved invocation. Pending/cancelled keys have none.
    pub const fn take_input_invocation(&mut self) -> Option<CommandInvocation> {
        self.last_invocation.take()
    }
}
