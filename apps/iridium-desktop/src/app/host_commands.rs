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
    CONFIG_RELOAD, EXPLORER_TOGGLE_PANEL, EXPLORER_TOGGLE_SIDEBAR, HISTORY_TOGGLE_PANEL,
    PALETTE_OPEN, VIEW_TOGGLE_THEME, WORKSPACE_CLOSE_TAB,
};
use iridium_editor::workspace::Node;
use iridium_file::TextFile;

use std::path::{Path, PathBuf};

use super::config;
use super::state::{DesktopApp, ExplorerFocus, ExplorerPlacement, Flow};
use crate::commands;
use crate::dialog::{self, Chosen, Want};
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
        } else if command == &commands::FILE_SAVE_AS {
            self.save_as_prompt()
        } else if command == &commands::FILE_NEW {
            self.new_file()
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
        } else if command == &EXPLORER_TOGGLE_SIDEBAR {
            self.toggle_sidebar()
        } else if command == &VIEW_TOGGLE_THEME {
            self.toggle_theme()
        } else if command == &commands::FILE_OPEN {
            self.open_file_from_chooser()
        } else if command == &commands::PROJECT_OPEN {
            self.open_project_from_chooser()
        } else if command == &commands::PROJECT_SET {
            self.set_project_to_explorer_root()
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
        self.user_keys = iridium_config::keys::keymap(user.bindings.iter().cloned());
        // ⭐ Replace, never add — the same rule the workspace layer follows, and
        // for the same reason: a binding deleted from the file must stop firing.
        // `set_user_keymap` rebuilds the panel's whole stack, so an open panel
        // picks up the reload without being reopened.
        if let Some(explorer) = self.explorer.as_mut() {
            explorer.set_user_keymap(&self.user_keys);
        }
        // The palette resolves its own keys too (#117), and unlike the explorer
        // it always exists — so there is no `if` here, and forgetting this line
        // would leave the panel on the bindings the session started with while
        // every other layer had reloaded.
        self.palette.set_user_keymap(&self.user_keys);
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
        if let Err(error) = crate::commands::create_config_if_absent(&path) {
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

    /// Where a chooser opens, and where the explorer would root: one answer,
    /// asked once.
    ///
    /// Reused rather than re-derived, and that is the point of it being a
    /// method rather than a line inside each caller. "Where is this session"
    /// already has an answer — the project when one is set, the active file's
    /// directory otherwise, the working directory or home below that — and it
    /// is tested. A chooser computing its own would be a second answer to the
    /// same question, free to disagree the moment either one moved.
    fn chooser_start(&self) -> PathBuf {
        self.explorer_root().path
    }

    /// Puts the reason on the strip, or says nothing, for the two outcomes of
    /// a chooser that produced no path.
    ///
    /// ⭐ **The two are not one case, and this is the whole reason
    /// [`Chosen`] has three variants instead of being an `Option<PathBuf>`.**
    /// Cancelling is a decision, and a face that announced it back would nag
    /// whoever simply changed their mind. Having no chooser at all is a
    /// missing feature, and a build that showed nothing for it would leave
    /// somebody pressing `⌘O` at a window that never answers. One channel
    /// that could only carry the first would be a defect.
    fn report_no_choice(&mut self, outcome: &Chosen) {
        if let Chosen::Unsupported(reason) = outcome {
            self.message = Some(Message::error(*reason));
        }
    }

    /// Picks a file with the system chooser and opens it in a new tab.
    ///
    /// ⚠️ The chooser is **application-modal**: it does not return until the
    /// panel closes, and nothing this face schedules ticks meanwhile — the
    /// explorer's background reader poll included. That is correct for a
    /// picker, and it is why this runs from the command dispatcher and never
    /// from a paint path.
    fn open_file_from_chooser(&mut self) -> Flow {
        let start = self.chooser_start();
        match dialog::choose(Want::File, Some(&start)) {
            Chosen::Path(path) => self.open_file(&path),
            outcome => self.report_no_choice(&outcome),
        }
        Flow::Running
    }

    /// Picks a folder with the system chooser and makes it this session's
    /// project.
    fn open_project_from_chooser(&mut self) -> Flow {
        // Asked **before** the panel goes up rather than after. Adopting a
        // project rebuilds the explorer, and a rebuild throws away rows that
        // have been edited and not applied — the same loss
        // [`toggle_explorer`](Self::toggle_explorer) refuses, for the same
        // reason. Refusing after the choice was made would waste the choice
        // as well.
        if self
            .explorer
            .as_ref()
            .is_some_and(FileExplorer::has_unapplied_edits)
        {
            self.message = Some(Message::error(
                "the file explorer has unapplied edits — esc twice in it to throw them away",
            ));
            return Flow::Running;
        }
        let start = self.chooser_start();
        match dialog::choose(Want::Directory, Some(&start)) {
            Chosen::Path(path) => self.adopt_project(path),
            outcome => self.report_no_choice(&outcome),
        }
        Flow::Running
    }

    /// Makes `path` this session's project, moving an open explorer with it.
    ///
    /// The panel is **rebuilt** rather than left where it was. From here on
    /// [`explorer_root`](Self::explorer_root) answers with the project, so a
    /// panel still rooted elsewhere would be a panel disagreeing with the
    /// session about where the work is — and the disagreement would surface
    /// only on the next toggle, which is the worst moment to discover it.
    ///
    /// Whether a search there may read past what is open is
    /// [`crate::project`]'s decision, taken by `chosen_root`, not restated
    /// here.
    fn adopt_project(&mut self, path: PathBuf) {
        let root = project::chosen_root(path);
        self.project = Some(root.path.clone());
        if self.explorer.is_none() {
            return;
        }
        match FileExplorer::open(root.path, root.crawl) {
            Ok(mut explorer) => {
                explorer.set_user_keymap(&self.user_keys);
                self.explorer = Some(explorer);
            },
            // Dropped rather than left on the old root, and the failure is
            // named. A panel still showing the previous project after the
            // session moved is the quieter of the two wrongs and much the
            // harder one to notice.
            Err(error) => {
                self.explorer = None;
                self.message = Some(Message::error(format!("the file explorer: {error}")));
            },
        }
    }

    /// Pins this session's project to the folder the explorer is showing.
    ///
    /// The answer to the question `⌘↓` leaves open: walking into a folder
    /// re-roots the *panel*, and the panel is dropped when it closes, so the
    /// walk is forgotten on the next toggle —
    /// `the_project_root_survives_closing_and_reopening_the_panel` asserts
    /// that deliberately. This writes the walk down instead of changing what
    /// `⌘↓` means, so both behaviours are kept rather than one traded away.
    ///
    /// Nothing is rebuilt and nothing is thrown away: the panel is *already*
    /// rooted where this pins, so unapplied edits in it survive untouched.
    /// That is why this refuses nothing where
    /// [`open_project_from_chooser`](Self::open_project_from_chooser) must.
    fn set_project_to_explorer_root(&mut self) -> Flow {
        let Some(explorer) = self.explorer.as_ref() else {
            self.message = Some(Message::error(
                "no file explorer is open — ⌘⌥E opens one, and this pins the folder it shows",
            ));
            return Flow::Running;
        };
        let root = explorer.root_path();
        // `root_path` answers with an empty path for a tree whose root it
        // cannot read back. Pinning that would set the session's project to
        // nothing at all and report success for having done it.
        if root.as_os_str().is_empty() {
            self.message = Some(Message::error(
                "the file explorer cannot say which folder it is showing",
            ));
            return Flow::Running;
        }
        self.message = Some(Message::notice(format!(
            "project set to {}",
            root.display()
        )));
        self.project = Some(root);
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
            // The band goes back to the document the moment the panel that
            // reserved it does.
            self.sync_left_inset();
            return Flow::Running;
        }
        let root = self.explorer_root();
        match FileExplorer::open(root.path, root.crawl) {
            Ok(mut explorer) => {
                explorer.set_user_keymap(&self.user_keys);
                self.explorer = Some(explorer);
                // A panel the user just asked for takes the keys, whichever
                // placement it lands in.
                self.explorer_focus = ExplorerFocus::Panel;
                self.sync_left_inset();
            },
            Err(error) => {
                self.message = Some(Message::error(format!("the file explorer: {error}")));
            },
        }
        Flow::Running
    }

    /// Puts the explorer up as a sidebar, or puts it away.
    ///
    /// ⭐ **An on/off switch, and the second press is the whole point.** This
    /// read as "move to the other placement" first, so `⌘B` twice left the
    /// explorer floating in the middle of the window rather than gone — Tom,
    /// 12 Aug 2026: *"when you command-B a second time, it just alternates
    /// between that and the central version … that's an issue"*. A key that
    /// names a **state** is one a hand can predict; a key that names a
    /// **transition** has to be counted.
    ///
    /// So there are three cases, and only the first is new: a sidebar on screen
    /// goes away, a floating panel becomes a sidebar, and nothing at all
    /// becomes a sidebar. The last is deliberate — someone pressing the sidebar
    /// chord wants a sidebar, and answering "no file explorer is open" to that
    /// is a refusal on a technicality.
    ///
    /// ⚠️ **The placement is only given back once the panel has actually
    /// gone.** Closing is refused while the oil buffer holds unapplied edits,
    /// and a placement moved ahead of a refused close would leave a sidebar
    /// drawn on screen while every measure downstream had been told it was a
    /// popover.
    pub(super) fn toggle_sidebar(&mut self) -> Flow {
        if self.explorer.is_some() && self.explorer_placement == ExplorerPlacement::Sidebar {
            let flow = self.toggle_explorer();
            if self.explorer.is_none() {
                // Back to the floating panel `⌘⌥E` opens, which is what keeps
                // both placements reachable from two keys instead of three:
                // this one is the sidebar's switch, that one is the panel's.
                self.explorer_placement = ExplorerPlacement::Popover;
            }
            return flow;
        }
        self.explorer_placement = ExplorerPlacement::Sidebar;
        // A panel the user just asked for takes the keys, and keeps them until
        // `Escape` hands them back.
        self.explorer_focus = ExplorerFocus::Panel;
        if self.explorer.is_none() {
            return self.toggle_explorer();
        }
        self.sync_left_inset();
        Flow::Running
    }

    /// Gives the document back the keys, and takes the panel away when the
    /// panel was a popover.
    ///
    /// ⚠️ **The one place the two placements behave differently on `Escape`.**
    /// A popover is opened, used and dismissed; a sidebar is kept open beside
    /// the code, and dismissing it on the key that means "give me the document
    /// back" would make it a panel to reopen after every glance.
    pub(super) fn leave_explorer(&mut self) {
        match self.explorer_placement {
            ExplorerPlacement::Popover => {
                self.explorer = None;
                self.sync_left_inset();
            },
            ExplorerPlacement::Sidebar => self.explorer_focus = ExplorerFocus::Document,
        }
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
    pub(super) fn explorer_root(&self) -> ExplorerRoot {
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
