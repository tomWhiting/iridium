//! Where a host command actually runs.
//!
//! The kernel resolves every chord and reports the ones it does not implement
//! rather than dropping them; [`DesktopApp::dispatch_host_command`] is the
//! single place this face turns one of those into an effect. A command
//! nothing here claims is named on the prompt strip — the key was consumed,
//! and silence would look like a dead key.

use iridium_config::UserConfig;
use iridium_editor::CommandId;
use iridium_editor::commands::builtin::{
    CONFIG_RELOAD, EXPLORER_TOGGLE_PANEL, HISTORY_TOGGLE_PANEL, PALETTE_OPEN, VIEW_TOGGLE_THEME,
    WORKSPACE_CLOSE_TAB,
};
use iridium_editor::workspace::Node;
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
        } else if command == &VIEW_TOGGLE_THEME {
            self.toggle_theme()
        } else if command == &commands::COMMANDS_LIST {
            self.show_command_reference()
        } else if command == &commands::CONFIG_EDIT {
            self.edit_config()
        } else if command == &CONFIG_RELOAD {
            self.reload_config()
        } else if self.workspace.handles_command(command) {
            self.run_workspace_command(command)
        } else {
            return None;
        };
        Some(flow)
    }

    /// Re-reads the user's configuration file and applies it to this session.
    ///
    /// The whole of what the file can say, in the order it has to be applied:
    /// the `[editor]` settings onto every open tab, then the `[keys]` bindings
    /// as a *replacement* for the `user` layer rather than a second copy of it,
    /// then the report of anything refused.
    ///
    /// ⚠️ **Nothing here can fail the session.** A file that has been deleted,
    /// made unreadable, or filled with nonsense since startup yields defaults
    /// and a list of problems, exactly as it does at startup — the same rule,
    /// for the same reason: this file is *found* by the editor, and a reload
    /// that could leave the session unusable would be a worse thing to have
    /// bound to a key than no reload at all.
    ///
    /// The theme is deliberately not touched. It is not in the file — `--theme`
    /// and the system appearance are the only two things that choose one — so
    /// re-applying anything here would be inventing a value the user never
    /// wrote. See [`super::theme::ThemeSource`].
    fn reload_config(&mut self) -> Flow {
        let path = iridium_config::user_config_path();
        self.apply_config(&UserConfig::load(path.as_deref()), path.as_deref())
    }

    /// Applies an already-read configuration to this session.
    ///
    /// Split from [`reload_config`](Self::reload_config) on the discipline
    /// [`crate::project`] states and for the same reason: the environment is
    /// read at the edge and the decision is made here, so what the reload
    /// *does* is testable without a process whose `$XDG_CONFIG_HOME` points at
    /// a fixture — and without two tests running at once fighting over one
    /// environment variable.
    ///
    /// `path` is only ever named in the report, so a session that has nowhere
    /// to look still reloads: it reads nothing, applies the defaults, and says
    /// so.
    pub(super) fn apply_config(&mut self, user: &UserConfig, path: Option<&Path>) -> Flow {
        self.workspace.set_config(user.editor.clone());

        let mut problems = user.problems.clone();
        problems.extend(config::replace_user_bindings(
            &mut self.workspace,
            user.bindings.clone(),
        ));

        // Built before the tab is touched, for the reason `show_command_reference`
        // builds first: the report describes the file as it was *read*, and
        // borrowing the workspace mutably to show it must not come first.
        let text = config::report(path, &problems);
        self.show_config_report(&text);
        self.message = Some(if problems.is_empty() {
            Message::notice(config::reloaded())
        } else {
            Message::error(config::summary(&problems))
        });
        Flow::Running
    }

    /// Puts `text` in the configuration tab, reusing the one already open.
    ///
    /// ⭐ **Reused rather than reopened, and this is the whole reason the
    /// method exists.** A reload is normally the *second* time this tab has
    /// been written, and opening another would leave the first one on the strip
    /// still describing a file that has since been fixed — two tabs with the
    /// same name disagreeing about the same file, and no way to tell which is
    /// current. One tab, always describing the last read.
    fn show_config_report(&mut self, text: &str) {
        let existing = self
            .workspace
            .tabs()
            .into_iter()
            .find(|id| {
                self.workspace
                    .node(*id)
                    .is_some_and(|node| node.label() == config::PROBLEMS_TAB)
            })
            .and_then(|id| self.workspace.node(id).and_then(Node::document));

        if let Some(document) = existing {
            if let Some(editor) = self.workspace.editor_mut(document) {
                editor.set_content(text);
            }
            return;
        }
        // Opened behind whatever is in front: a reload is asked for from
        // somewhere, and stealing focus away from it would lose the place of
        // whoever pressed the key.
        self.workspace.open(text, config::PROBLEMS_TAB, None);
    }

    /// Opens the user's `config.toml` as an ordinary tab.
    ///
    /// An **ordinary** tab, deliberately: it goes through [`Self::open_file`],
    /// so it has a real [`TextFile`](iridium_file::TextFile) behind it, `⌘S`
    /// writes it, and the file-changed-on-disk refusal applies exactly as it
    /// would to any other file. The `config.toml` *report* tab beside it is a
    /// different thing entirely — that one is generated text describing the
    /// last read, and saving it would write a list of complaints over the file
    /// they are about.
    ///
    /// The file is created first if it is not there. That is the same
    /// courtesy startup does, repeated here because this is the other way in
    /// and a command called "Edit Configuration" that opened an empty untitled
    /// buffer would be a command that lied about what it edits.
    fn edit_config(&mut self) -> Flow {
        let Some(path) = iridium_config::user_config_path() else {
            // No `$HOME` and no `$XDG_CONFIG_HOME`: there is nowhere for the
            // file to be, so there is nothing to open and nothing to create.
            self.message = Some(Message::error(
                "no home directory, so there is nowhere for a configuration file",
            ));
            return Flow::Running;
        };
        if let Err(error) = iridium_config::create_if_absent(&path) {
            // Reported, unlike at startup. There it is a courtesy nobody
            // asked for; here somebody pressed a key and is owed an answer.
            self.message = Some(Message::error(format!(
                "cannot create {}: {error}",
                path.display()
            )));
            return Flow::Running;
        }
        self.open_file(&path);
        Flow::Running
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

    /// Swaps between the light and dark presets, repainting everything.
    ///
    /// ⚠️ **Three places hold a theme, and all three have to move together.**
    /// The workspace's setter reaches every open editor including background
    /// tabs, so documents opened later inherit the new one; the compositor
    /// keeps its own copy because the clear colour and the retained shaped
    /// buffers are built from it; and the overlay reads the theme it is handed
    /// at paint time, so it needs nothing. Setting only the workspace leaves
    /// the window's clear colour frozen on the old preset — a dark page behind
    /// light text — which is the shape of the bug 6e22cbe fixed once already.
    ///
    /// Before the window exists there is no compositor to update, and that is
    /// not a failure: `Shell::open` takes the workspace's theme as its
    /// argument, so a toggle pressed against a headless session is picked up
    /// when the window opens.
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
        // A directory named on the command line outranks every guess below it,
        // for the whole session. It is the one root nobody has to infer: the
        // person who started the process said it out loud.
        if let Some(project) = self.project.clone() {
            return project::chosen_root(project);
        }
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
