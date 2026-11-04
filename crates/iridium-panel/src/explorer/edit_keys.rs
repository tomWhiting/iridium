//! What the explorer does with a key while its rows are being edited, a
//! refusal is showing, or a confirmation is.
//!
//! # The rows: every key goes to the kernel editor
//!
//! The session's [`Editor`](iridium_editor::Editor) resolves the key against
//! the kernel's keymap, the face's layer, [`super::keymap::edit_layer`] and the
//! user's `[keys]`, in that order, and runs it. The panel acts only on what
//! comes back:
//!
//! | default key | command | what the panel does |
//! | --- | --- | --- |
//! | `Ctrl+S`, `⌘S` | `explorer.edit.askApply` | shows the plan. **It does not apply**; `y` does |
//! | `Escape` | `explorer.edit.leave` | drops extra carets, else stops editing, asking about unapplied work |
//! | `Ctrl+Alt+E`, `⌘⌥E` | `explorer.togglePanel` | closes the panel once nothing is unapplied |
//! | `Ctrl+Alt+B`, `⌘B` | `explorer.toggleSidebar` | asks the face to change placement |
//!
//! A clipboard or search request is handed to the face as an
//! [`ExplorerOutcome`]; any other host command is swallowed, because the panel
//! is modal.
//!
//! # The other two screens resolve against the panel's own keymap
//!
//! A refusal takes one key and a confirmation two. Both replace the rows
//! wholesale, so a key that edited the text from behind one of them would
//! change something nobody can see.

use std::collections::BTreeSet;
use std::path::Path;

use iridium_editor::commands::builtin::{
    EXPLORER_EDIT_ASK_APPLY, EXPLORER_EDIT_LEAVE, EXPLORER_TOGGLE_PANEL, EXPLORER_TOGGLE_SIDEBAR,
};
use iridium_editor::{ClipboardOperation, CommandId, EditorKeyResult, KeyEvent};

use super::apply;
use super::mode::{Confirmation, Leaving};
use super::panel::{ExplorerOutcome, FileExplorer};
use super::plan::{Operation, Plan};
use super::resolve::Resolved;
use super::verb::Verb;

/// The verb a resolution names, if it named one this panel owns.
fn verb_of(resolved: &Resolved) -> Option<Verb> {
    match resolved {
        Resolved::Command(id) => Verb::from_id(id),
        _ => None,
    }
}

impl FileExplorer {
    /// Acts on a key resolved while a refusal or a confirmation is showing.
    pub(super) fn dispatch_edit(
        &mut self,
        resolved: &Resolved,
        event: &KeyEvent,
    ) -> ExplorerOutcome {
        if self.mode.plan().is_some() {
            return self.confirming(resolved, event);
        }
        if !self.mode.refusals().is_empty() {
            return self.refused(resolved);
        }
        ExplorerOutcome::Handled
    }

    /// A key while the rows are on screen: the editor's, then whatever it
    /// asked the panel or the face to do.
    pub(super) fn edit_key(&mut self, event: &KeyEvent) -> ExplorerOutcome {
        let Some(editing) = self.mode.editing_mut() else {
            return ExplorerOutcome::Handled;
        };
        let result = editing.editor.handle_key(event);
        editing.sync();
        // Every key but the one that leaves answers the discard question with
        // "still working". Keyed on the command, so it survives rebinding.
        let leaving = matches!(
            &result,
            EditorKeyResult::HostCommand { command, .. } if command == &EXPLORER_EDIT_LEAVE
        );
        if !leaving {
            self.mode.disarm_discard();
        }
        self.after_editor(result, event.is_repeat)
    }

    /// Pastes `text` into the rows being edited, at every caret.
    pub(super) fn paste_into_buffer(&mut self, text: &str) {
        if let Some(editing) = self.mode.editing_mut() {
            editing.editor.paste(text);
            editing.sync();
        }
    }

    /// What the panel or the face does about what the editor handed back.
    fn after_editor(&mut self, result: EditorKeyResult, is_repeat: bool) -> ExplorerOutcome {
        match result {
            EditorKeyResult::None => ExplorerOutcome::Handled,
            EditorKeyResult::Clipboard(ClipboardOperation::Copy(text)) => {
                ExplorerOutcome::Copy(text)
            },
            EditorKeyResult::Clipboard(ClipboardOperation::Cut { text, command }) => {
                if let Some(editing) = self.mode.editing_mut() {
                    editing.editor.apply_command(command);
                    editing.sync();
                }
                ExplorerOutcome::Copy(text)
            },
            EditorKeyResult::Clipboard(ClipboardOperation::Paste) => ExplorerOutcome::Paste,
            EditorKeyResult::Search(action) => ExplorerOutcome::Search(action),
            EditorKeyResult::HostCommand { command, .. } => self.edit_command(&command, is_repeat),
            EditorKeyResult::Error(message) => ExplorerOutcome::Failed(message),
        }
    }

    /// One of the panel's commands, run from the editing screen.
    fn edit_command(&mut self, command: &CommandId, is_repeat: bool) -> ExplorerOutcome {
        if command == &EXPLORER_EDIT_ASK_APPLY {
            return self.ask_to_apply(is_repeat);
        }
        if command == &EXPLORER_EDIT_LEAVE {
            return self.leave_edit();
        }
        if command == &EXPLORER_TOGGLE_PANEL {
            return self.close_from_edit();
        }
        if command == &EXPLORER_TOGGLE_SIDEBAR {
            return ExplorerOutcome::ToggleSidebar;
        }
        // Modal: a document's host command — the palette, a tab — is
        // swallowed rather than run against a document nobody is looking at.
        ExplorerOutcome::Handled
    }

    /// A key while the refusals are on screen.
    ///
    /// Its one binding goes back to the text with the caret on the first
    /// refused line, which is where the fixing starts.
    fn refused(&mut self, resolved: &Resolved) -> ExplorerOutcome {
        if verb_of(resolved) == Some(Verb::RefusedDismiss) {
            let first = self.mode.editing().and_then(|editing| {
                let rows = editing.listing.rows();
                editing
                    .refusals
                    .iter()
                    .find_map(|refusal| rows.lines.get(refusal.row).copied().flatten())
            });
            if let (Some(line), Some(editing)) = (first, self.mode.editing_mut()) {
                editing.place_caret(line);
            }
            self.mode.dismiss_refusals();
        }
        ExplorerOutcome::Handled
    }

    /// A key while the confirmation is on screen. Applying is **inert under
    /// key repeat**, so a held key cannot answer it.
    fn confirming(&mut self, resolved: &Resolved, event: &KeyEvent) -> ExplorerOutcome {
        match verb_of(resolved) {
            Some(Verb::ConfirmApply) if !event.is_repeat => self.apply_confirmed(),
            Some(Verb::ConfirmCancel) => {
                self.mode.back_to_edit();
                ExplorerOutcome::Handled
            },
            _ => ExplorerOutcome::Handled,
        }
    }

    /// `Escape`: extra carets go first, then the session — asking about
    /// unapplied work on the first press and throwing it away on the second.
    fn leave_edit(&mut self) -> ExplorerOutcome {
        if self
            .mode
            .editing_mut()
            .is_some_and(super::session::Editing::collapse_carets)
        {
            return ExplorerOutcome::Handled;
        }
        match self.mode.leave() {
            Leaving::Left => ExplorerOutcome::Handled,
            Leaving::Unsaved => {
                let armed = self
                    .mode
                    .editing_mut()
                    .is_some_and(super::session::Editing::arm_discard);
                if armed {
                    self.mode.discard();
                    return ExplorerOutcome::Handled;
                }
                ExplorerOutcome::Failed(self.discard_message("again"))
            },
        }
    }

    /// The toggle chord, which closes the panel — once nothing is unapplied.
    ///
    /// Refused rather than routed through the two-press discard: closing the
    /// panel and discarding work are two decisions.
    fn close_from_edit(&mut self) -> ExplorerOutcome {
        match self.mode.leave() {
            Leaving::Left => ExplorerOutcome::Closed,
            Leaving::Unsaved => ExplorerOutcome::Failed(self.unapplied_edits_message()),
        }
    }

    /// `Ctrl+S`: work out what the text says, and show it.
    ///
    /// ⛔ **This does not touch the disk.** It reaches the confirmation and
    /// stops there; `y` on that screen is what applies.
    fn ask_to_apply(&mut self, is_repeat: bool) -> ExplorerOutcome {
        if is_repeat {
            return ExplorerOutcome::Handled;
        }
        match self.mode.request_confirm() {
            Confirmation::Ready | Confirmation::Refused | Confirmation::NotEditing => {
                ExplorerOutcome::Handled
            },
            Confirmation::NothingToDo => {
                ExplorerOutcome::Failed("nothing to apply — the rows are as they were".to_owned())
            },
        }
    }

    /// `y`: carry the confirmed plan out.
    ///
    /// ⚠️ **Runs on the frame thread**, so an apply never interleaves with a
    /// poll redrawing the rows under it. The panel returns to browsing either
    /// way: on failure the plan describes a filesystem that has partly moved,
    /// and offering to run it again would repeat what already happened.
    fn apply_confirmed(&mut self) -> ExplorerOutcome {
        let Some(plan) = self.mode.plan().cloned() else {
            return ExplorerOutcome::Handled;
        };
        let outcome = apply::apply(&plan);
        self.mode.applied();
        self.reload_touched(&plan);

        match outcome {
            Ok(_) => ExplorerOutcome::Handled,
            Err(failure) => ExplorerOutcome::Failed(failure.describe()),
        }
    }

    /// Re-reads every directory the plan touched.
    ///
    /// ⚠️ **Not optional.** A second session over stale rows would carry paths
    /// that have moved or gone. The listings arrive on the worker, which wakes
    /// the face through [`FileExplorer::set_wake`] so they are drawn at once.
    fn reload_touched(&mut self, plan: &Plan) {
        let mut touched = BTreeSet::new();
        let mut note = |path: &Path| {
            if let Some(directory) = path.parent() {
                touched.insert(directory.to_path_buf());
            }
        };
        for operation in &plan.operations {
            match operation {
                Operation::Create { path, .. } | Operation::Delete { path } => note(path),
                Operation::Rename { from, to } => {
                    note(from);
                    note(to);
                },
            }
        }

        // A walk of what has been read, rather than a path lookup: a node is
        // identified by where it sits in this arena, never by a path string.
        let mut pending = vec![self.files.root()];
        let mut stale = Vec::new();
        while let Some(node) = pending.pop() {
            pending.extend_from_slice(self.files.listed_children(node));
            if self
                .files
                .info(node)
                .is_some_and(|info| touched.contains(info.path))
            {
                stale.push(node);
            }
        }
        for node in stale {
            self.files.reload(node);
        }
    }
}
