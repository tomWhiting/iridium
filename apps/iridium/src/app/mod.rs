//! The whole editor as a state machine: keys in, editor state and a cell
//! buffer out.
//!
//! There is no terminal here. [`App`] is driven by
//! [`TerminalInput`](iridium_tui::input::TerminalInput) values and
//! draws into a [`Surface`](iridium_tui::cell::Surface), both of which a test
//! can construct, so everything this program decides — what a key does, when a
//! save is refused, what the statusline says — runs on the host. What is left
//! for [`run`](crate::run) is opening a terminal and blocking on it.
//!
//! The shared workspace owns editors and their file/search/render payloads.
//! `session` constructs documents, `tabs` guards their lifecycle, `document`
//! handles save/reload, `panels` routes modal input, and `view` paints the screen.
//!
//! # The kernel owns every verb
//!
//! Nothing here edits a document, moves a caret, folds a region or searches.
//! Editing reaches the kernel through
//! [`Editor::handle_key`](iridium_editor::Editor::handle_key), through
//! [`Editor::apply_command`](iridium_editor::Editor::apply_command) for the
//! clipboard command the kernel hands back, or through a kernel method named by
//! a binding. The verbs this face *adds* — save, reload, quit, go-to-line, the
//! fold commands — are registered as host commands and bound in a keymap layer;
//! see [`commands`](crate::app::commands). `App::run_host_command` is the only
//! place they are
//! dispatched, and it is reached only because the kernel reported
//! [`EditorKeyResult::HostCommand`](iridium_editor::EditorKeyResult::HostCommand).
//!
//! # Scrolling costs more than it should
//!
//! The kernel's fold-aware viewport uses cell units in the terminal. Its
//! dedicated geometry/scroll API preserves keyboard intentions during host
//! resizes and caret following. Raw `Editor::state_mut` remains a cursor or
//! document mutation boundary and deliberately clears those intentions.
//!
//! # Clipboard ownership
//!
//! Library sessions start with an internal clipboard. The binary selects a
//! native system provider locally, or terminal copy transport for SSH sessions.
//! Clipboard failures are visible and cuts commit only after delivery. Terminal
//! bytes are written by the driver before the next input is accepted.

pub mod commands;
pub mod prompt;

mod binding_reference;
mod clipboard;
mod completion;
mod config;
mod config_actions;
mod document;
mod drag_scroll;
#[cfg(test)]
mod drag_scroll_tests;
mod ex;
mod input;
mod modal;
#[cfg(test)]
mod modal_tests;
mod mouse;
#[cfg(test)]
mod mouse_tests;
#[cfg(test)]
mod navigation_tests;
mod panels;
mod project_search;
mod query;
#[cfg(test)]
mod selection_motion_tests;
mod session;
mod sidebar;
mod sidebar_resize;
#[cfg(test)]
mod sidebar_resize_tests;
mod similar;
mod structural;
mod tab_strip;
mod tabs;
mod view;
mod workbench;
#[cfg(test)]
mod workbench_tests;
mod wrapping;
#[cfg(test)]
mod wrapping_tests;

#[cfg(test)]
mod selection_tests;
#[cfg(test)]
mod tab_tests;
#[cfg(test)]
mod tests;

use iridium_editor::commands::builtin::{
    EXPLORER_TOGGLE_PANEL, HISTORY_TOGGLE_PANEL, PALETTE_OPEN,
};
use iridium_editor::commands::palette::CommandMru;
use iridium_editor::input::SearchAction;
use iridium_editor::{CommandId, Editor, EditorKeyResult, KeyEvent};
use iridium_tui::frame::{CommandPalette, FileExplorerPanel, HistoryPanel};
use iridium_tui::input::TerminalInput;

pub use self::document::StartupError;
use self::prompt::{Answer, Deed, Message, Prompt};
use crate::cli::Options;
use crate::file::TextFile;
use crate::theme;

/// Whether the editor carries on after an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Wait for the next event.
    Running,
    /// Leave.
    Exit,
}

/// One editing session.
pub struct App {
    /// The shared owner of each document and its terminal state.
    workspace: iridium_editor::workspace::Workspace<session::DocumentSession>,
    /// The explicit directory passed at startup, kept across tab changes.
    project: Option<std::path::PathBuf>,
    /// Whether every document in this invocation is read-only.
    access: session::Access,
    /// Whether the search panel is on screen.
    search_open: bool,
    /// The command palette panel.
    palette: CommandPalette,
    /// Whether the palette is on screen. While it is, it is modal: every key
    /// goes to it, exactly as with a prompt.
    palette_open: bool,
    startup_mode: modal::StartupMode,
    mouse_anchor: Option<iridium_editor::Position>,
    mouse_scroll: Option<drag_scroll::DragScroll>,
    pending_write_quit: Option<iridium_editor::workspace::NodeId>,
    /// Which commands ran recently, feeding the palette's recency ranking.
    ///
    /// Recorded only after a command actually ran — an entry for a command
    /// that errored would rank a failure as a favourite.
    mru: CommandMru,
    /// The file browser, either a focused picker or persistent sidebar.
    explorer: Option<FileExplorerPanel>,
    focus: sidebar::Focus,
    sidebar_drag: Option<(u16, usize)>,
    config_path: Option<std::path::PathBuf>,
    /// Navigation retained across closes; contents are read afresh.
    explorer_bookmark: Option<iridium_tui::frame::FileExplorerBookmark>,
    /// The undo-tree panel.
    history: HistoryPanel,
    /// Whether the undo-tree panel is on screen. Modal while it is.
    history_open: bool,
    /// The question on the prompt line, if one is open.
    prompt: Option<Prompt>,
    query: Option<query::QuerySession>,
    similar: Option<similar::Preview>,
    completion: Option<completion::Menu>,
    project_search: Option<project_search::panel::Panel>,
    /// What the last key had to say, shown until the next one.
    message: Option<Message>,
    /// This process's clipboard. See the module documentation.
    clipboard: String,
    clipboard_backend: crate::clipboard::Backend,
    pending_clipboard: Option<clipboard::PendingClipboard>,
    /// The width of the terminal, in cells.
    columns: usize,
    /// The height of the terminal, in cells.
    rows: usize,
    /// The user's `[keys]` layer, kept so panels built later can be given it.
    ///
    /// The file explorer is constructed on demand rather than at startup, so
    /// there has to be somewhere for its bindings to wait.
    user_keys: iridium_editor::Keymap,
}

impl App {
    /// Starts a session from what the command line asked for.
    ///
    /// # Errors
    ///
    /// [`StartupError`] for a theme that will not load and a file that will not
    /// open. The two registration failures cannot happen for the tables in
    /// [`commands`] — a unit test there proves it against a real registry — and
    /// are reported rather than ignored so that they cannot become silent if
    /// those tables change.
    pub fn new(options: Options) -> Result<Self, StartupError> {
        // Bound rather than inlined: `with_config` borrows the path, and a
        // temporary built inside the call would not outlive it.
        let path = iridium_config::user_config_path();
        Self::with_config(options, iridium_config::UserConfig::read(), path.as_deref())
    }

    /// Starts a session from `options` and an *already-read* configuration.
    ///
    /// ⭐ **The seam exists so the wiring can be tested.** [`Self::new`] reads
    /// the file this process's environment names, which a test cannot choose
    /// without setting environment variables shared with every other test in
    /// the binary. A test that could only assert against a bare [`Editor`]
    /// would go on passing if this constructor stopped installing the user's
    /// bindings altogether — which is precisely the wiring worth guarding.
    ///
    /// `path` names the file for diagnostics and subsequent reloads; bindings come
    /// from `user`, already read from wherever it was.
    ///
    /// # Errors
    ///
    /// As [`Self::new`].
    pub(crate) fn with_config(
        options: Options,
        user: iridium_config::UserConfig,
        path: Option<&std::path::Path>,
    ) -> Result<Self, StartupError> {
        let theme = options.theme.map_or_else(
            || Ok(iridium_editor::Theme::default()),
            |choice| theme::load(&choice).map_err(StartupError::Theme),
        )?;
        let mut workspace = iridium_editor::workspace::Workspace::new(user.editor, theme);
        for meta in commands::command_metas() {
            workspace
                .register_command(meta)
                .map_err(StartupError::Setup)?;
        }
        workspace
            .push_keymap(commands::keymap())
            .map_err(StartupError::Setup)?;
        workspace
            .push_keymap(iridium_editor::commands::structural_keymap::structural_keymap())
            .map_err(StartupError::Setup)?;
        if !options.non_modal {
            workspace
                .push_keymap(modal::keymap())
                .map_err(StartupError::Setup)?;
        }
        workspace
            .push_keymap(workbench::keymap().map_err(|error| {
                StartupError::Setup(iridium_editor::workspace::FaceSetupError::Keymap(error))
            })?)
            .map_err(StartupError::Setup)?;
        let user_keys = iridium_config::keys::keymap(user.bindings.iter().cloned());
        let mut problems = user.problems;
        problems.extend(config::install_user_bindings(&mut workspace, user.bindings));
        let config_message = config::summary(&problems, path).map(Message::error);
        let project = options
            .path
            .as_deref()
            .filter(|path| path.is_dir())
            .map(std::path::Path::to_path_buf);

        let mut app = Self {
            workspace,
            project: project.clone(),
            access: if options.read_only {
                session::Access::ReadOnly
            } else {
                session::Access::ReadWrite
            },
            search_open: false,
            palette: {
                let mut palette = CommandPalette::new();
                palette.set_user_keymap(&user_keys);
                palette
            },
            palette_open: false,
            startup_mode: if options.non_modal {
                modal::StartupMode::Insert
            } else {
                modal::StartupMode::Structural
            },
            mouse_anchor: None,
            mouse_scroll: None,
            pending_write_quit: None,
            explorer: None,
            focus: sidebar::Focus::Document,
            sidebar_drag: None,
            config_path: path.map(std::path::Path::to_path_buf),
            explorer_bookmark: None,
            mru: CommandMru::default(),
            history: HistoryPanel::new(&user_keys),
            history_open: false,
            prompt: None,
            query: None,
            similar: None,
            completion: None,
            project_search: None,
            // A configuration problem is the first thing the session has to
            // say. Nothing else has run yet, so nothing is being overwritten.
            message: config_message,
            clipboard: String::new(),
            clipboard_backend: crate::clipboard::Backend::Internal,
            pending_clipboard: None,
            columns: 0,
            rows: 0,
            user_keys,
        };

        match options.path.filter(|_| project.is_none()) {
            Some(path) => {
                app.add_file(&path)?;
            },
            None => {
                app.add_scratch()?;
            },
        }
        if let Some(line) = options.line {
            app.goto(line);
        }
        // ⚠️ **After the line, not before.** Opening the tree makes the panel
        // modal, and `goto` moves the document's caret — running it second
        // would leave the caret where the panel's own key handling had not put
        // it. The order is what makes `iridium +40 ~/project` land on line 40
        // of the empty buffer behind the tree rather than nowhere.
        if let Some(directory) = project {
            app.open_explorer(iridium_panel::explorer::chosen_root(directory));
            if let Some(panel) = app.explorer.as_mut() {
                panel.set_sidebar(true);
            }
        }
        Ok(app)
    }

    /// Tells the editor how big the terminal is, in cells.
    ///
    /// Call this once the terminal is open and on every resize. It is what puts
    /// the screen's geometry into the kernel's viewport; without it the kernel
    /// scrolls against a viewport still measured in pixels and disagrees with
    /// what is on screen.
    pub fn resize(&mut self, columns: usize, rows: usize) {
        self.stop_mouse_drag();
        self.columns = columns;
        self.rows = rows;
        self.sync_viewport();
        self.ensure_caret_visible();
    }

    /// The kernel, for a caller that wants to look at what this session holds.
    #[must_use]
    pub fn editor(&self) -> Option<&Editor> {
        self.workspace.active_editor()
    }

    /// The file being edited, if the session named one.
    #[must_use]
    pub fn file(&self) -> Option<&TextFile> {
        self.workspace
            .active_payload()
            .and_then(|session| session.file.as_ref())
    }

    /// What the last input had to say, if anything.
    #[must_use]
    pub const fn message(&self) -> Option<&Message> {
        self.message.as_ref()
    }

    /// The question on the prompt line, if one is open.
    #[must_use]
    pub const fn prompt(&self) -> Option<&Prompt> {
        self.prompt.as_ref()
    }

    /// Whether the search panel is on screen.
    #[must_use]
    pub const fn is_searching(&self) -> bool {
        self.search_open
    }

    /// Whether the file explorer is on screen.
    #[must_use]
    pub const fn is_explorer_open(&self) -> bool {
        self.explorer.is_some()
    }

    /// Whether the command palette is on screen.
    #[must_use]
    pub const fn is_palette_open(&self) -> bool {
        self.palette_open
    }

    /// Whether the undo-tree panel is on screen.
    #[must_use]
    pub const fn is_history_open(&self) -> bool {
        self.history_open
    }

    /// Handles one thing the user did.
    pub fn handle_input(&mut self, input: &TerminalInput) -> Flow {
        match input {
            TerminalInput::Key(event) => self.handle_key(event),
            TerminalInput::Paste(text) => {
                self.paste(text);
                Flow::Running
            },
            TerminalInput::Mouse(event) => {
                self.completion = None;
                self.handle_mouse(*event)
            },
            TerminalInput::FocusLost => {
                self.stop_mouse_drag();
                // A chord left pending across a blur would eat the first
                // keystroke after the user came back; the kernel names this
                // path explicitly.
                if let Some(editor) = self.workspace.active_editor_mut() {
                    editor.abort_pending_key_sequence();
                }
                Flow::Running
            },
            // The driver turns a resize into `DriverEvent::Resize` before this
            // is reached, so this arm is for a caller driving `App` directly.
            TerminalInput::Resize { rows, cols } => {
                let columns = usize::try_from(*cols).unwrap_or(usize::MAX);
                let rows = usize::try_from(*rows).unwrap_or(usize::MAX);
                self.resize(columns, rows);
                Flow::Running
            },
            // Key releases and focus gain do not dispatch editing commands.
            TerminalInput::FocusGained | TerminalInput::Release(_) | TerminalInput::Unmapped(_) => {
                Flow::Running
            },
        }
    }

    /// Acts on what the kernel made of a key.
    fn consume(&mut self, result: EditorKeyResult) -> Flow {
        match result {
            EditorKeyResult::Error(message) => {
                self.message = Some(Message::error(message));
                Flow::Running
            },
            EditorKeyResult::None => Flow::Running,
            EditorKeyResult::Clipboard(operation) => {
                self.clipboard(operation);
                Flow::Running
            },
            EditorKeyResult::Search(action) => {
                self.search_action(&action);
                Flow::Running
            },
            // The count and captures a key sequence carried are not read: a
            // count is typed as a numeric prefix, and in a non-modal keymap
            // digits are text, so nothing in this face can produce one. A
            // command here that used a count would be describing a keymap this
            // face does not have.
            EditorKeyResult::HostCommand { command, .. } => self.run_host_command(&command),
        }
    }

    /// Runs one of the commands this face contributes, from a keypress.
    ///
    /// The kernel reports a command it does not implement rather than dropping
    /// the keypress, so an id that arrives here and is not one of ours came
    /// from a keymap layer someone else pushed. Saying so is the only honest
    /// answer: the key was consumed, and silence would look like a dead key.
    fn run_host_command(&mut self, command: &CommandId) -> Flow {
        self.dispatch_host_command(command).unwrap_or_else(|| {
            self.message = Some(Message::error(format!(
                "`{command}` is bound to a key but nothing runs it"
            )));
            Flow::Running
        })
    }

    /// Dispatches a host command this face implements, or `None` for one it
    /// does not — the caller knows whether it arrived by key or by palette,
    /// and the honest message differs.
    fn dispatch_host_command(&mut self, command: &CommandId) -> Option<Flow> {
        if let Some(flow) = self.workbench_command(command) {
            return Some(flow);
        }
        if let Some(flow) = self.wrap_command(command) {
            return Some(flow);
        }
        if command == &commands::PROJECT_SEARCH {
            return Some(self.open_project_search(""));
        }
        if command == &commands::COMPLETE_WORD {
            return Some(self.open_completion());
        }
        if command == &commands::FIND_SIMILAR {
            return Some(self.open_similar());
        }
        if command == &commands::QUERY_JSON {
            return Some(self.open_query(""));
        }
        if let Some(flow) = self.ex_command(command) {
            return Some(flow);
        }
        if self.workspace.handles_command(command) {
            return Some(self.workspace_command(command));
        }
        let flow = if command == &commands::STRUCTURAL_ENTER {
            self.enter_structural_navigation(
                iridium_editor::commands::structural_keymap::STRUCTURAL,
            )
        } else if command == &commands::STRUCTURAL_VERB_ENTER {
            self.enter_structural_navigation(
                iridium_editor::commands::structural_keymap::STRUCTURAL_VERB,
            )
        } else if command == &commands::FILE_NEW {
            self.new_scratch()
        } else if command == &commands::FILE_SAVE {
            self.save(false)
        } else if command == &commands::FILE_SAVE_FORCE {
            self.save(true)
        } else if command == &commands::FILE_RELOAD {
            self.request_reload()
        } else if command == &commands::APP_QUIT {
            self.request_quit()
        } else if command == &commands::GOTO_LINE {
            self.prompt = Some(Prompt::goto_line());
            Flow::Running
        } else if command == &commands::FOLD_TOGGLE {
            self.toggle_fold()
        } else if command == &commands::FOLD_ALL {
            if let Some(editor) = self.workspace.active_editor_mut() {
                editor.fold_all();
            }
            self.fold_carets_to_headers();
            self.ensure_caret_visible();
            Flow::Running
        } else if command == &commands::FOLD_NONE {
            if let Some(editor) = self.workspace.active_editor_mut() {
                editor.unfold_all();
            }
            self.ensure_caret_visible();
            Flow::Running
        } else if command == &PALETTE_OPEN {
            self.palette_open = true;
            self.palette.open();
            Flow::Running
        } else if command == &EXPLORER_TOGGLE_PANEL {
            self.toggle_explorer()
        } else if command == &HISTORY_TOGGLE_PANEL {
            // A toggle, exactly as the kernel names it: the panel has no query
            // to abandon, so the chord that opened it is how it is put away.
            self.history_open = !self.history_open;
            if self.history_open {
                self.history.open();
            }
            Flow::Running
        } else {
            return None;
        };
        Some(flow)
    }

    /// Opens or closes the search panel.
    ///
    /// Only the two panel actions need anything here: the kernel has already
    /// moved to the next or previous match by the time it reports one.
    fn search_action(&mut self, action: &SearchAction) {
        let Some((editor, session)) = self.workspace.active_editor_and_payload_mut() else {
            return;
        };
        match *action {
            SearchAction::OpenSearch => {
                self.search_open = true;
                session.search.open(editor);
                // The panel takes rows from the document, so the kernel's
                // viewport shrinks with it.
                self.sync_viewport();
            },
            SearchAction::CloseSearch => {
                if self.search_open {
                    self.search_open = false;
                    session.search.close(editor);
                    self.sync_viewport();
                }
            },
            SearchAction::NextMatch
            | SearchAction::PreviousMatch
            | SearchAction::SelectAllMatches
            | SearchAction::OpenInSelection
            | SearchAction::ClearScope => {},
        }
    }

    /// Inserts pasted text, which is one command rather than a stream of keys.
    fn paste(&mut self, text: &str) {
        if self.similar.is_some() {
            return;
        }
        if self.project_search_open() {
            if let Some(panel) = self.project_search.as_mut() {
                panel.paste(text);
            }
            return;
        }
        if let Some(prompt) = self.prompt.as_mut() {
            prompt.paste(text);
            self.refresh_query();
            return;
        }
        if self.palette_open {
            // The palette is modal: a paste belongs to its query, not to the
            // document underneath it.
            self.palette.paste(text);
            self.start_palette_query();
            return;
        }
        if self.history_open {
            // The undo-tree panel has no field for pasted text to land in,
            // and a paste reaching the document under a modal panel would
            // grow the very tree being read.
            return;
        }
        if let Some(explorer) = self
            .explorer
            .as_mut()
            .filter(|_| self.focus == crate::app::sidebar::Focus::Explorer)
        {
            explorer.paste(text);
            return;
        }
        if let Some((editor, session)) = self.workspace.active_editor_and_payload_mut() {
            if self.search_open {
                session.search.paste(text, editor);
            } else {
                let result = editor.paste_with_result(text);
                self.consume(result);
            }
        }
        self.ensure_caret_visible();
    }

    /// Hands a key to the open prompt and acts on the answer.
    fn answer_prompt(&mut self, event: &KeyEvent) -> Flow {
        let Some(prompt) = self.prompt.as_mut() else {
            return Flow::Running;
        };
        let answer = prompt.answer(event);
        if matches!(answer, Answer::Pending) {
            return Flow::Running;
        }
        self.prompt = None;
        match answer {
            Answer::Pending | Answer::Cancelled | Answer::Query => {
                self.pending_write_quit = None;
                Flow::Running
            },
            Answer::Goto(line) => {
                self.goto(line);
                Flow::Running
            },
            Answer::SaveAs(path) => self.complete_write_as(&path),
            Answer::Do(Deed::Quit) => Flow::Exit,
            Answer::Do(Deed::Reload) => self.reload(),
            Answer::Do(Deed::CloseTab(tab)) => self.close_tab(tab),
        }
    }
}
