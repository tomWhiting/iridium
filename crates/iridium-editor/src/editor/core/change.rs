//! A change records resolved input and owns one explicit undo group.
use crate::commands::builtin::EDIT_INSERT_CHARACTER;
use crate::commands::{CommandInvocation, ModeName};
use crate::input::{ClipboardOperation, KeyResult};
use crate::{Editor, EditorEvent, EditorKeyResult, KeyboardHandler};

pub(super) struct ChangeSession {
    input_mode: ModeName,
    return_mode: Option<ModeName>,
    recipe: Vec<CommandInvocation>,
}

impl Editor {
    /// Completes replacement input as one undoable, repeatable change.
    /// Hosts call this when focus moves to another document or workflow.
    pub fn finish_change(&mut self) {
        if let Some(session) = self.change_session.take() {
            self.state.history.finish_group();
            self.last_command_sequence = session.recipe;
            self.keyboard_handler.set_mode(session.return_mode);
        }
    }

    /// Executes a resolved key binding, optionally recording subsequent input.
    ///
    /// # Errors
    /// Invalid sequences or recording without an input mode/content edit.
    pub fn run_input_sequence(
        &mut self,
        steps: &[CommandInvocation],
        enters_mode: Option<ModeName>,
        record_input: bool,
    ) -> Result<(), String> {
        if record_input && enters_mode.is_none() {
            return Err("input recording requires an input mode".into());
        }
        self.finish_change();
        let return_mode = self.mode().cloned();
        self.stage_command_sequence(steps, record_input)?;
        if let Some(mode) = enters_mode {
            if record_input {
                if !self.state.history.begin_group() {
                    return Err("could not begin change undo group".into());
                }
                self.change_session = Some(ChangeSession {
                    input_mode: mode.clone(),
                    return_mode,
                    recipe: steps.to_vec(),
                });
            }
            self.keyboard_handler.set_mode(Some(mode));
        }
        Ok(())
    }

    pub(super) fn consume_recorded_input(
        &mut self,
        result: KeyResult,
        invocation: Option<CommandInvocation>,
        checkpoint: Option<KeyboardHandler>,
    ) -> EditorKeyResult {
        let result = self.preserve_structural_selection(result, invocation.as_ref());
        if self
            .change_session
            .as_ref()
            .is_some_and(|session| self.mode() != Some(&session.input_mode))
        {
            let requested_mode = self.mode().cloned();
            self.finish_change();
            self.keyboard_handler.set_mode(requested_mode);
        }
        if self.change_session.is_none() {
            return self.consume_key_result(result);
        }
        match result {
            KeyResult::Command(command) => {
                let Some(invocation) = invocation else {
                    self.finish_change();
                    return self.consume_key_result(KeyResult::Command(command));
                };
                if self.state.read_only && command.modifies_content() {
                    return EditorKeyResult::None;
                }
                let recipe = match self.recorded_recipe(invocation) {
                    Ok(recipe) => recipe,
                    Err(message) => {
                        if let Some(handler) = checkpoint {
                            self.keyboard_handler = handler;
                        }
                        self.emit(&EditorEvent::Error {
                            message: message.clone(),
                            code: "CHANGE_RECORDING_FAILED".into(),
                        });
                        return EditorKeyResult::Error(message);
                    },
                };
                if self.apply_command_internal(command) {
                    if let Some(session) = self.change_session.as_mut() {
                        session.recipe = recipe;
                    }
                } else {
                    if let Some(handler) = checkpoint {
                        self.keyboard_handler = handler;
                    }
                    return EditorKeyResult::Error("change input failed; no edit applied".into());
                }
                EditorKeyResult::None
            },
            KeyResult::Error(_)
            | KeyResult::Handled
            | KeyResult::Ignored
            | KeyResult::Clipboard(ClipboardOperation::Copy(_) | ClipboardOperation::Paste) => {
                self.consume_key_result(result)
            },
            other => {
                self.finish_change();
                self.consume_key_result(other)
            },
        }
    }

    fn recorded_recipe(
        &self,
        invocation: CommandInvocation,
    ) -> Result<Vec<CommandInvocation>, String> {
        let mut recipe = self
            .change_session
            .as_ref()
            .map_or_else(Vec::new, |session| session.recipe.clone());
        if invocation.command() == &EDIT_INSERT_CHARACTER {
            if let (Some(text), Some(last)) = (invocation.args().text(), recipe.last_mut()) {
                if last.command() == &EDIT_INSERT_CHARACTER && last.args().text().is_some() {
                    last.append_text(text);
                    super::sequence::validate_recipe(&recipe)?;
                    return Ok(recipe);
                }
            }
        }
        recipe.push(invocation);
        super::sequence::validate_recipe(&recipe)?;
        Ok(recipe)
    }
}
