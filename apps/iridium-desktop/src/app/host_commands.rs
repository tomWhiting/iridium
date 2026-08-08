//! Where a host command actually runs.
//!
//! The kernel resolves every chord and reports the ones it does not implement
//! rather than dropping them; [`DesktopApp::dispatch_host_command`] is the
//! single place this face turns one of those into an effect. A command
//! nothing here claims is named on the prompt strip — the key was consumed,
//! and silence would look like a dead key.

use iridium_editor::CommandId;
use iridium_editor::commands::builtin::{
    EXPLORER_TOGGLE_PANEL, HISTORY_TOGGLE_PANEL, PALETTE_OPEN, WORKSPACE_CLOSE_TAB,
};

use iridium_file::TextFile;

use std::path::Path;

use super::config;
use super::state::{DesktopApp, Flow};
use crate::commands;
use crate::file_tree::FileExplorer;
use crate::project::{self, ExplorerRoot};
use crate::prompt::Message;

impl DesktopApp {
    /// Runs one of the commands this face contributes, or says visibly that
    /// it cannot.
    ///
    /// The kernel reports a command it does not implement rather than
    /// dropping the keypress; a host command this face does not implement
    /// either surfaces on the prompt strip. Saying so is the only honest
    /// answer: the key was consumed, and silence would look like a dead key.
    pub(super) fn run_host_command(&mut self, command: &CommandId) -> Flow {
        self.dispatch_host_command(command).unwrap_or_else(|| {
            self.message = Some(Message::error(format!(
                "`{command}` is not wired into the desktop shell yet"
            )));
            Flow::Running
        })
    }

    /// Dispatches a host command this face implements, or `None` for one it
    /// does not.
    pub(super) fn dispatch_host_command(&mut self, command: &CommandId) -> Option<Flow> {
        let flow = if command == &commands::FILE_SAVE {
            self.save(false)
        } else if command == &commands::FILE_SAVE_FORCE {
            self.save(true)
        } else if command == &PALETTE_OPEN {
            self.palette_open = true;
            self.palette.open();
            Flow::Running
        } else if command == &HISTORY_TOGGLE_PANEL {
            // A toggle, exactly as the kernel names it: the panel has no
            // query to abandon, so the chord that opened it is how it is put
            // away.
            self.history_open = !self.history_open;
            if self.history_open {
                self.history.open();
            }
            Flow::Running
        } else if command == &EXPLORER_TOGGLE_PANEL {
            self.toggle_explorer()
        } else if command == &commands::COMMANDS_LIST {
            self.show_command_reference()
        } else if self.workspace.handles_command(command) {
            self.run_workspace_command(command)
        } else {
            return None;
        };
        Some(flow)
    }

    /// Opens a tab listing every command by the id a `[keys]` line names it by.
    ///
    /// The command palette searches ids but shows titles, so this is the only
    /// way to find out what to write — and a binding nobody can name is a
    /// feature nobody can use.
    ///
    /// The text is built before the tab is opened rather than while: reading
    /// the registry borrows the active editor, and opening a tab needs the
    /// workspace mutably. Building first is not a workaround for the borrow
    /// checker so much as the honest order — the reference describes the
    /// session as it was *asked about*, not as it is once a tab about it
    /// exists.
    fn show_command_reference(&mut self) -> Flow {
        let Some(editor) = self.workspace.active_editor() else {
            // Unreachable: the session always has a tab. Reported rather than
            // dropped, since the chord was consumed and silence would look
            // like a dead key.
            self.message = Some(Message::error("there is no editor to list the commands of"));
            return Flow::Running;
        };
        let text = config::command_reference(editor.commands(), editor.key_hints());
        self.workspace.open(&text, config::COMMANDS_TAB, None);
        self.after_tab_change();
        Flow::Running
    }

    /// Opens the file explorer, or closes it if it is already open.
    ///
    /// A toggle, exactly as the kernel names it. Closing **drops** the panel
    /// rather than hiding it: it owns a reader thread and an arena of every
    /// directory that was expanded, and a hidden panel would keep both for
    /// the rest of the session.
    ///
    /// Where it starts, and whether a search there may read past what is
    /// open, are [`crate::project`]'s decision — see there for why the two
    /// are separate questions. A failure to start the reader is reported on
    /// the strip rather than swallowed; the key was consumed, and silence
    /// would look like a dead key.
    fn toggle_explorer(&mut self) -> Flow {
        if let Some(explorer) = self.explorer.as_ref() {
            // ⚠️ The panel's rows can be edited as text and applied to the
            // filesystem, and closing here drops the panel outright — the
            // refusal `FileExplorer` keeps for its own escape key never gets
            // asked. So it is asked here instead: the command is refused while
            // there is unapplied work, and the panel says how to get rid of
            // it.
            if explorer.has_unapplied_edits() {
                self.message = Some(Message::error(
                    "the file explorer has unapplied edits — esc twice in it to throw them away"
                        .to_owned(),
                ));
                return Flow::Running;
            }
            self.explorer = None;
            return Flow::Running;
        }
        let root = self.explorer_root();
        match FileExplorer::open(root.path, root.crawl) {
            Ok(explorer) => self.explorer = Some(explorer),
            Err(error) => {
                self.message = Some(Message::error(format!("the file explorer: {error}")));
            },
        }
        Flow::Running
    }

    /// Where the explorer starts, with the environment read here and the
    /// choice made in [`crate::project`].
    ///
    /// The split is what makes the decision testable: the case that went
    /// wrong — a bundle launched from Finder, which inherits `/` as its
    /// working directory — cannot be reproduced from a terminal at all, so
    /// nothing that reads the environment itself could have caught it.
    ///
    /// The **file's own path** goes in, not its parent: the chooser needs to
    /// tell "a file with a real directory above it" from a bare name, and
    /// `Path::parent` of a bare name is `Some("")`, which is neither.
    fn explorer_root(&self) -> ExplorerRoot {
        let active: Option<&Path> = self
            .workspace
            .active_payload()
            .and_then(|document| document.file.as_ref())
            .map(TextFile::path);
        project::explorer_root(active, std::env::current_dir().ok(), std::env::home_dir())
    }

    /// Forwards a `workspace.*` command to the kernel's own dispatcher.
    ///
    /// This face does not decide what "next tab" means across a nested
    /// group; the workspace does, so the terminal and the browser cannot
    /// drift apart on it while both answers still look right in a workspace
    /// with no groups. What this end owns is the *consequence*: after the
    /// active tab moves, the title names a different file, the caret is in a
    /// different document, and the frame on screen describes neither.
    ///
    /// Closing is intercepted before it reaches the kernel, because the
    /// kernel has no idea which buffers have unsaved changes — see
    /// [`close_active_tab`](Self::close_active_tab).
    fn run_workspace_command(&mut self, command: &CommandId) -> Flow {
        if command == &WORKSPACE_CLOSE_TAB {
            return self.close_active_tab(false);
        }
        match self.workspace.run_command(command) {
            // Understood, and correctly did nothing: next-tab at the last
            // tab, first-tab when already there. An outcome, not a failure —
            // flashing an error at someone who reached the end of the strip
            // is how a face becomes noise.
            Ok(false) => Flow::Running,
            Ok(true) => {
                self.after_tab_change();
                Flow::Running
            },
            // Unreachable: `handles_command` gated the call. Reported rather
            // than dropped so that widening one and not the other cannot
            // become a silently inert chord.
            Err(error) => {
                self.message = Some(Message::error(format!("`{command}`: {error}")));
                Flow::Running
            },
        }
    }
}
