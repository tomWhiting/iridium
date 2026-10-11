//! Command composition stages every step before publishing one undoable edit.
use crate::commands::CommandInvocation;
use crate::document::{CursorState, Document};
use crate::editor::ast::SyntaxState;
use crate::history::Command;
use crate::input::{AstRequest, KeyResult};
use crate::{Editor, KeyboardHandler};

const MAX_STEPS: usize = 128;
const MAX_COUNT: u32 = 10_000;

#[cfg(test)]
mod tests;

struct Staged {
    document: Document,
    cursor: CursorState,
    syntax: SyntaxState,
    keyboard: KeyboardHandler,
    commands: Vec<Command>,
}

impl Editor {
    /// Repeats the last successful composed edit at the current selections.
    ///
    /// # Errors
    /// No recorded composition, excessive count, or any failed replay step.
    /// A failed replay leaves both the editor and its repeat recipe unchanged.
    pub fn repeat_command_sequence(&mut self, count: u32) -> Result<(), String> {
        self.finish_change();
        if self.last_command_sequence.is_empty() {
            return Err("no composed edit to repeat".into());
        }
        let repeats = usize::try_from(count).map_err(|error| error.to_string())?;
        if repeats == 0 || repeats > MAX_STEPS / self.last_command_sequence.len() {
            return Err(format!(
                "repetition must fit within {MAX_STEPS} command steps"
            ));
        }
        let recipe = self.last_command_sequence.clone();
        let steps: Vec<_> = (0..repeats).flat_map(|_| recipe.iter().cloned()).collect();
        self.run_command_sequence(&steps)?;
        self.last_command_sequence = recipe;
        Ok(())
    }

    /// Runs stable command IDs as a single transaction and undo unit.
    ///
    /// Counts apply to their own invocation. Syntax boundaries, missing parsers,
    /// read-only edits and unsupported host/clipboard/search/history operations
    /// reject the entire sequence. No intermediate selection or event is exposed.
    /// Limited to 128 commands and 10,000 total requested repetitions.
    ///
    /// # Errors
    /// Names the failing step and command. Text, selections, history and input
    /// state remain unchanged on failure. Hosts display the returned diagnostic.
    pub fn run_command_sequence(&mut self, steps: &[CommandInvocation]) -> Result<(), String> {
        self.finish_change();
        self.stage_command_sequence(steps, false)
    }

    pub(super) fn stage_command_sequence(
        &mut self,
        steps: &[CommandInvocation],
        require_edit: bool,
    ) -> Result<(), String> {
        validate_recipe(steps)?;
        let mut stage = Staged {
            document: self.state.document.clone(),
            cursor: self.state.cursor.clone(),
            syntax: self.state.syntax.fork()?,
            keyboard: self.keyboard_handler.clone(),
            commands: Vec::new(),
        };
        for (index, step) in steps.iter().enumerate() {
            stage
                .step(self, step)
                .map_err(|message| format!("step {} ({}): {message}", index + 1, step.command()))?;
        }
        let edited = stage.commands.iter().any(Command::modifies_content);
        if require_edit && !edited {
            return Err("a recorded change must begin with an edit".into());
        }
        if !self.apply_sequence_command(Command::Compound {
            commands: stage.commands,
        }) {
            return Err("command sequence commit failed; no edits applied".into());
        }
        if edited {
            self.last_command_sequence = steps.to_vec();
        }
        self.keyboard_handler = stage.keyboard;
        self.state.syntax.commit_expansion(stage.syntax);
        Ok(())
    }
}

impl Staged {
    fn step(&mut self, editor: &Editor, step: &CommandInvocation) -> Result<(), String> {
        if step.command() == &crate::commands::builtin::EDIT_INSERT_CHARACTER {
            if let Some(text) = step.args().text() {
                for character in (0..step.args().repeat_count()).flat_map(|_| text.chars()) {
                    self.command_step(
                        editor,
                        &CommandInvocation::new(
                            step.command().clone(),
                            crate::commands::CommandArgs::with_captures(vec![character]),
                        ),
                    )?;
                }
                return Ok(());
            }
        }
        self.command_step(editor, step)
    }

    fn command_step(&mut self, editor: &Editor, step: &CommandInvocation) -> Result<(), String> {
        // Revision mismatch after a staged edit triggers a full reparse before
        // any subsequent command consults syntax scopes or node positions.
        self.syntax.sync(&self.document);
        let scopes = self.syntax.caret_scopes(&self.document);
        let result = self
            .keyboard
            .run_command(
                step.command().as_str(),
                step.args().clone(),
                &self.document,
                &self.cursor,
                &editor.state.history,
                &editor.state.config,
                &scopes,
            )
            .map_err(|error| error.to_string())?;
        let command = match result {
            KeyResult::Error(error) => return Err(error),
            KeyResult::Command(command) => command,
            KeyResult::Ast(request) => self.select(request, 1)?,
            KeyResult::AstRepeated { request, count } => self.select(request, count)?,
            _ => return Err("command requires external state or produced no operation".into()),
        };
        if editor.state.read_only && command.modifies_content() {
            return Err("document is read-only".into());
        }
        command
            .apply(&mut self.document, &mut self.cursor)
            .map_err(|error| error.to_string())?;
        if command.modifies_content() {
            self.syntax.invalidate();
        }
        self.commands.push(command);
        Ok(())
    }

    fn select(&mut self, request: AstRequest, count: u32) -> Result<Command, String> {
        let mut next = self.cursor.clone();
        for _ in 0..count {
            next = self
                .syntax
                .apply_ast_request(&self.document, &next, request)
                .ok_or_else(|| "no syntax target (boundary or unavailable parser)".to_owned())?;
        }
        Ok(Command::SetSelection {
            old_state: self.cursor.clone(),
            new_state: next,
        })
    }
}

pub(super) fn validate_recipe(steps: &[CommandInvocation]) -> Result<(), String> {
    if steps.is_empty() || steps.len() > MAX_STEPS {
        return Err(format!("a command sequence needs 1..={MAX_STEPS} steps"));
    }
    if steps.iter().any(|step| step.args().repeat_count() == 0) {
        return Err("command sequence counts must be positive".into());
    }
    let count = steps.iter().try_fold(0_u32, |total, step| {
        let cost = if step.command() == &crate::commands::builtin::EDIT_INSERT_CHARACTER {
            step.args().text().map_or(Some(1), |text| {
                text.chars()
                    .try_fold(0_u32, |count, _| count.checked_add(1))
            })
        } else {
            Some(1)
        }?;
        total.checked_add(cost.checked_mul(step.args().repeat_count())?)
    });
    if count.is_none_or(|count| count > MAX_COUNT) {
        return Err(format!("command sequence exceeds {MAX_COUNT} repetitions"));
    }
    let text_size = steps.iter().try_fold(0_usize, |total, step| {
        total.checked_add(step.args().text().map_or(0, str::len))
    });
    if text_size.is_none_or(|size| size > 1_048_576) {
        return Err("recorded text exceeds 1 MiB".into());
    }
    Ok(())
}
