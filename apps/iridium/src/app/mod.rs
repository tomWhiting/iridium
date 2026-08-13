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
//! The three modules beside this one extend the same [`App`] rather than
//! introducing another type, which is how the kernel splits its own editor:
//! [`commands`](crate::app::commands) is the verbs this face contributes and
//! their keys, `document`
//! is what it does to a file, and `view` is what reaches the screen.
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
//! The kernel's viewport is fold-aware and the terminal drives it in cell units,
//! which is decision 1 of `docs/TERMINAL-FACE-PLAN.md` and why there is no
//! second layout model here. But the only way to *write* to that viewport is
//! [`Editor::state_mut`](iridium_editor::Editor::state_mut), which eagerly
//! discards the keyboard handler's transient state: the sticky vertical column,
//! the multi-cursor addition order, and any half-typed key sequence. Calling it
//! once per keystroke would break chords outright.
//!
//! So `App::ensure_caret_visible` pays it as rarely as it can — nothing at all
//! while a key sequence is pending, and nothing while the caret's line is
//! already on screen. What remains is that scrolling past the edge of the
//! screen loses the sticky vertical column. That is a gap in the kernel's
//! surface — there is no `Editor::scroll_to` that leaves the keyboard handler
//! alone — and the alternative was a face-local scroll model, which is the
//! mistake this whole crate is written to avoid.
//!
//! # The clipboard is this process's
//!
//! [`ClipboardOperation`](iridium_editor::input::ClipboardOperation) hands the
//! host text to keep and asks for text back.
//! There is no system clipboard behind it: this crate depends on the kernel and
//! the face and nothing else, and the terminal answer — OSC 52 — is a byte
//! sequence only the driver may write, which exposes no hook for one. So a copy
//! here is readable by a paste here and by nothing else. It is stated rather
//! than hidden, because a clipboard that silently is not the system's is worse
//! than one documented not to be.

pub mod commands;
pub mod prompt;

mod config;
mod document;
mod view;

#[cfg(test)]
mod tests;

use iridium_editor::commands::builtin::{
    EXPLORER_TOGGLE_PANEL, HISTORY_TOGGLE_PANEL, PALETTE_OPEN,
};
use iridium_editor::commands::palette::CommandMru;
use iridium_editor::input::{CommandRunError, SearchAction};
use iridium_editor::{
    ClipboardOperation, CommandArgs, CommandId, Editor, EditorKeyResult, KeyEvent,
};
use iridium_tui::frame::{
    CommandPalette, ExplorerAction, FileExplorerPanel, Frame, HistoryOutcome, HistoryPanel,
    PaletteOutcome, SearchOutcome, SearchOverlay,
};
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
    /// The kernel.
    editor: Editor,
    /// The renderer, kept across frames because it caches the highlighter.
    frame: Frame,
    /// The file being edited, if the session named one.
    file: Option<TextFile>,
    /// The search panel. Kept across closes so re-opening offers the last
    /// query back, which is what [`SearchOverlay`] is built for.
    search: SearchOverlay,
    /// Whether the search panel is on screen.
    search_open: bool,
    /// The command palette panel.
    palette: CommandPalette,
    /// Whether the palette is on screen. While it is, it is modal: every key
    /// goes to it, exactly as with a prompt.
    palette_open: bool,
    /// Which commands ran recently, feeding the palette's recency ranking.
    ///
    /// Recorded only after a command actually ran — an entry for a command
    /// that errored would rank a failure as a favourite.
    mru: CommandMru,
    /// The file explorer, when it is open. `None` is the whole of "closed" —
    /// there is no second flag to disagree with it, unlike the panels above,
    /// which keep their state across closes because they have a query worth
    /// offering back. A directory listing is not: it is read again in
    /// milliseconds and a stale one would be a lie about the disk.
    explorer: Option<FileExplorerPanel>,
    /// The undo-tree panel.
    history: HistoryPanel,
    /// Whether the undo-tree panel is on screen. Modal while it is.
    history_open: bool,
    /// The question on the prompt line, if one is open.
    prompt: Option<Prompt>,
    /// What the last key had to say, shown until the next one.
    message: Option<Message>,
    /// This process's clipboard. See the module documentation.
    clipboard: String,
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
    /// `path` is used only to name the file in the message; the bindings come
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
        let mut editor = Editor::with_defaults();

        for meta in commands::command_metas() {
            editor
                .register_command(meta)
                .map_err(StartupError::Command)?;
        }
        editor
            .push_keymap(commands::keymap())
            .map_err(StartupError::Keymap)?;

        // ⭐ **After this face's layer, so the user's bindings win.** The stack
        // resolves highest layer first, and a user who rebinds a chord this
        // face also binds means the one they wrote — which is the whole point
        // of the file. See `config` for why this face reads it and does not
        // write it.
        // ⭐ Built *before* the bindings are moved into the install, and kept:
        // every panel this face owns is modal and resolves against its own
        // stack, so each needs the user's layer handed to it. The editor's copy
        // is not reachable from a panel.
        let user_keys = iridium_config::keys::keymap(user.bindings.iter().cloned());
        let problems = config::install_user_bindings(&mut editor, user.bindings);
        let config_message = config::summary(&problems, path).map(Message::error);

        if let Some(choice) = options.theme {
            editor.set_theme(theme::load(&choice).map_err(StartupError::Theme)?);
        }

        // ⭐ **The one `is_dir` that decides what the argument was** (R5). The
        // command line is a pure function over its arguments and cannot ask the
        // filesystem, so the path arrives whole and is told apart here.
        //
        // A path that does not exist is a FILE, not an empty project: "created
        // on save if it does not exist" is what the usage promises, and a typo
        // that opened a project nobody has would lose the name they typed.
        let project = options
            .path
            .as_deref()
            .filter(|path| path.is_dir())
            .map(std::path::Path::to_path_buf);

        let file = match options.path {
            // A directory names no file to read, so the buffer starts empty and
            // unnamed — saving it asks for a name, exactly as `iridium` with no
            // argument does.
            Some(_) if project.is_some() => None,
            Some(path) => {
                let (file, text) = TextFile::open(&path).map_err(StartupError::File)?;
                editor.set_content(&text);
                Some(file)
            },
            None => None,
        };

        // After the content: setting a language parses the document, and doing
        // it first would parse an empty one and then parse again.
        if let Some(language) = file
            .as_ref()
            .and_then(|file| document::language_of(file.path()))
        {
            editor.set_language(language);
        }

        if options.read_only {
            editor.state_mut().read_only = true;
        }

        let mut app = Self {
            editor,
            frame: Frame::new(),
            file,
            search: SearchOverlay::new(&user_keys),
            search_open: false,
            palette: {
                let mut palette = CommandPalette::new();
                palette.set_user_keymap(&user_keys);
                palette
            },
            palette_open: false,
            explorer: None,
            mru: CommandMru::default(),
            history: HistoryPanel::new(&user_keys),
            history_open: false,
            prompt: None,
            // A configuration problem is the first thing the session has to
            // say. Nothing else has run yet, so nothing is being overwritten.
            message: config_message,
            clipboard: String::new(),
            columns: 0,
            rows: 0,
            user_keys,
        };

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
        self.columns = columns;
        self.rows = rows;
        self.sync_viewport();
        self.ensure_caret_visible();
    }

    /// The kernel, for a caller that wants to look at what this session holds.
    #[must_use]
    pub const fn editor(&self) -> &Editor {
        &self.editor
    }

    /// The file being edited, if the session named one.
    #[must_use]
    pub const fn file(&self) -> Option<&TextFile> {
        self.file.as_ref()
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
            TerminalInput::FocusLost => {
                // A chord left pending across a blur would eat the first
                // keystroke after the user came back; the kernel names this
                // path explicitly.
                self.editor.abort_pending_key_sequence();
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
            // A release is not a press, and the kernel binds presses; feeding
            // one to the keymap would run every binding twice. A mouse event
            // is dropped because hit testing a cell grid is a kernel change the
            // plan names (decision 2) and not this face's to make: the kernel's
            // mouse handler resolves a click in pixels against a fixed
            // character width, which a terminal cannot supply, and a caret in
            // the wrong place is worse than no caret. `Unmapped` is a key the
            // kernel's `KeyCode` cannot name, and the kernel is the only place
            // a new key code may be added.
            TerminalInput::FocusGained
            | TerminalInput::Release(_)
            | TerminalInput::Mouse(_)
            | TerminalInput::Unmapped(_) => Flow::Running,
        }
    }

    /// Handles one key press.
    fn handle_key(&mut self, event: &KeyEvent) -> Flow {
        // A message describes the key before this one.
        self.message = None;

        if self.prompt.is_some() {
            return self.answer_prompt(event);
        }

        if self.palette_open {
            return self.drive_palette(event);
        }

        if self.history_open {
            return self.drive_history(event);
        }

        // Modal like the three above it, and for the stronger reason: the
        // explorer gives every printable character to its filter, so a key
        // that fell through here would be a key typed into the document while
        // the person was looking at a file list.
        if self.explorer.is_some() {
            return self.drive_explorer(event);
        }

        if self.search_open {
            match self.search.handle_key(event, &mut self.editor) {
                // The panel leaves what it does not bind to the host, so a save
                // is not dead while a search is open.
                SearchOutcome::Ignored => {},
                SearchOutcome::Handled | SearchOutcome::Replaced(_) => {
                    self.ensure_caret_visible();
                    return Flow::Running;
                },
                SearchOutcome::Closed => {
                    self.search_open = false;
                    self.sync_viewport();
                    self.ensure_caret_visible();
                    return Flow::Running;
                },
            }
        }

        let result = self.editor.handle_key(event);
        let flow = self.consume(result);
        self.ensure_caret_visible();
        flow
    }

    /// Acts on what the kernel made of a key.
    fn consume(&mut self, result: EditorKeyResult) -> Flow {
        match result {
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
        let flow = if command == &commands::FILE_SAVE {
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
            self.editor.fold_all();
            self.ensure_caret_visible();
            Flow::Running
        } else if command == &commands::FOLD_NONE {
            self.editor.unfold_all();
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

    /// Hands a key to the open undo-tree panel and acts on the outcome.
    fn drive_history(&mut self, event: &KeyEvent) -> Flow {
        match self.history.handle_key(event, &self.editor) {
            HistoryOutcome::Handled => Flow::Running,
            HistoryOutcome::Closed => {
                self.history_open = false;
                Flow::Running
            },
            HistoryOutcome::Jump(node) => {
                // The panel stays open: hopping between states and watching
                // the document change underneath is what the tree is for.
                if !self.editor.jump_to_history_node(node) {
                    self.message = Some(Message::error(
                        "that history state no longer exists".to_owned(),
                    ));
                }
                self.ensure_caret_visible();
                Flow::Running
            },
        }
    }

    /// Opens the file explorer, or closes it if it is already up.
    ///
    /// ⚠️ **Refused while the panel holds unapplied edits**, exactly as the GPU
    /// face refuses it. Closing here drops the panel outright, so the refusal
    /// `FileExplorer` keeps for its own `Escape` is never asked — it has to be
    /// asked here instead, or a directory's worth of typed renames goes with
    /// one chord and nothing says so.
    fn toggle_explorer(&mut self) -> Flow {
        let band = self.sidebar_columns();
        if let Some(explorer) = self.explorer.as_ref() {
            if explorer.has_unapplied_edits() {
                self.message = Some(Message::error(
                    "the file explorer has unapplied edits — esc twice in it to throw them away"
                        .to_owned(),
                ));
                return Flow::Running;
            }
            self.explorer = None;
            self.resync_after_band_change(band);
            return Flow::Running;
        }
        let root = self.explorer_root();
        self.open_explorer(root);
        self.resync_after_band_change(band);
        Flow::Running
    }

    /// Puts the file explorer on screen, rooted here.
    ///
    /// ⭐ **Opened as a popover, including at startup**, and that is a ruling
    /// rather than a default. The panel is modal in this face — every key goes
    /// to it, printable characters included — and a popover is what a modal
    /// picker looks like: it appears, you choose, it closes. A band down the
    /// left edge looks *persistent*, and a persistent panel that nonetheless
    /// swallows every keystroke would be furniture making a promise the input
    /// routing does not keep.
    ///
    /// ⚠️ **That changes with #113**, which is where the terminal face's
    /// modality is decided — and R6 says it must not be decided as a side
    /// effect of this task. When a non-modal keymap exists, a session started
    /// on a directory should come up as a sidebar and stay there past the first
    /// file it opens. Until then the honest placement is the one that matches
    /// how keys actually behave.
    ///
    /// The failure is named rather than swallowed: whether it came from a
    /// chord or from the command line, a panel that silently did not appear is
    /// indistinguishable from one that is broken.
    fn open_explorer(&mut self, root: iridium_panel::explorer::ExplorerRoot) {
        match FileExplorerPanel::open(root.path, root.crawl, &self.user_keys) {
            Ok(explorer) => self.explorer = Some(explorer),
            Err(error) => {
                self.message = Some(Message::error(format!("the file explorer: {error}")));
            },
        }
    }

    /// Writes the kernel's viewport again when the left band's width moved.
    ///
    /// ⚠️ **The frame's geometry and the kernel's viewport are two different
    /// things.** `Chrome` reaches the geometry every frame, so what is *drawn*
    /// follows the band immediately; the kernel's own viewport is only written
    /// by `sync_viewport`, and it is what the scroll verbs measure against. Skip
    /// this and the document draws in the columns it has while the kernel keeps
    /// scrolling against the columns it used to have.
    ///
    /// Guarded on an actual change rather than called after every explorer key,
    /// because `sync_viewport` reaches `Editor::state_mut`, which discards the
    /// keyboard handler's transient state — see this module's own note on what
    /// that costs.
    fn resync_after_band_change(&mut self, before: usize) {
        if self.sidebar_columns() != before {
            self.sync_viewport();
        }
    }

    /// Where this face opens the explorer when nobody said.
    ///
    /// ⭐ **The *choice* is [`chosen_root`] and is shared; this *guess* is this
    /// face's, and the two faces genuinely guess differently.** The GPU face
    /// must distrust its working directory — a bundled application launched
    /// from Finder inherits `/`, and rooting there once sent a search crawl
    /// across an entire disk. A terminal has the opposite property: its working
    /// directory is where a person `cd`-ed to, which is the most reliable
    /// statement of intent either face ever gets.
    ///
    /// So: the open file's folder if there is one, and otherwise the working
    /// directory, trusted. `chosen_root` then answers the only question that
    /// means the same thing in both faces — whether a search may read past it.
    fn explorer_root(&self) -> iridium_panel::explorer::ExplorerRoot {
        let from_file = self
            .file
            .as_ref()
            .map(TextFile::path)
            .and_then(|path| path.parent())
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(std::path::Path::to_path_buf);
        let directory = from_file
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        iridium_panel::explorer::chosen_root(directory)
    }

    /// Hands a key to the open explorer and acts on the outcome.
    fn drive_explorer(&mut self, event: &KeyEvent) -> Flow {
        let band = self.sidebar_columns();
        let Some(explorer) = self.explorer.as_mut() else {
            return Flow::Running;
        };
        let action = explorer.handle_key(event);
        let flow = match action {
            ExplorerAction::Handled => Flow::Running,
            ExplorerAction::Close => {
                self.explorer = None;
                Flow::Running
            },
            ExplorerAction::Open(path) => {
                self.explorer = None;
                self.request_open(path)
            },
            // Everything the panel could not do, in its own words.
            ExplorerAction::Report(message) => {
                self.message = Some(Message::error(message));
                Flow::Running
            },
        };
        // The sidebar chord reports `Handled` like any other key — the
        // placement is the panel's own state — so the band's width is what
        // says it moved.
        self.resync_after_band_change(band);
        flow
    }

    /// Hands a key to the open palette and acts on the outcome.
    fn drive_palette(&mut self, event: &KeyEvent) -> Flow {
        match self.palette.handle_key(event, &self.editor, &self.mru) {
            PaletteOutcome::Handled => Flow::Running,
            PaletteOutcome::Closed => {
                self.palette_open = false;
                Flow::Running
            },
            PaletteOutcome::Run(command) => {
                self.palette_open = false;
                self.run_palette_command(&command)
            },
        }
    }

    /// Runs a command the palette resolved: the kernel's own if it implements
    /// it, this face's if not, and an honest message when neither does.
    ///
    /// The recency list records only commands that actually dispatched, so a
    /// palette entry nothing runs cannot become a ranked favourite. Read-only
    /// refusals are not failures here: the kernel consumes them the same way a
    /// keypress does, silently leaving the document alone.
    fn run_palette_command(&mut self, command: &CommandId) -> Flow {
        match self.editor.run_command(command.as_str(), CommandArgs::NONE) {
            Ok(result) => {
                self.mru.record(command);
                let flow = self.consume(result);
                self.ensure_caret_visible();
                flow
            },
            Err(CommandRunError::Unimplemented { .. }) => {
                if let Some(flow) = self.dispatch_host_command(command) {
                    self.mru.record(command);
                    self.ensure_caret_visible();
                    return flow;
                }
                self.message = Some(Message::error(format!(
                    "`{command}` is not available in the terminal yet"
                )));
                Flow::Running
            },
        }
    }

    /// Keeps or supplies clipboard text.
    ///
    /// The delete half of a cut is the kernel's own reversible command, applied
    /// unchanged: building the deletion here would be a second implementation
    /// of one the kernel handed over.
    fn clipboard(&mut self, operation: ClipboardOperation) {
        match operation {
            ClipboardOperation::Copy(text) => self.clipboard = text,
            ClipboardOperation::Cut { text, command } => {
                self.clipboard = text;
                self.editor.apply_command(command);
            },
            ClipboardOperation::Paste => {
                // Taken and put back rather than cloned: the kernel's paste
                // borrows the text, and a clipboard large enough to matter is
                // one nobody wants copied on every press.
                let text = std::mem::take(&mut self.clipboard);
                self.editor.paste(&text);
                self.clipboard = text;
            },
        }
    }

    /// Opens or closes the search panel.
    ///
    /// Only the two panel actions need anything here: the kernel has already
    /// moved to the next or previous match by the time it reports one.
    fn search_action(&mut self, action: &SearchAction) {
        match *action {
            SearchAction::OpenSearch => {
                self.search_open = true;
                self.search.open(&mut self.editor);
                // The panel takes rows from the document, so the kernel's
                // viewport shrinks with it.
                self.sync_viewport();
            },
            SearchAction::CloseSearch => {
                if self.search_open {
                    self.search_open = false;
                    self.search.close(&mut self.editor);
                    self.sync_viewport();
                }
            },
            SearchAction::NextMatch | SearchAction::PreviousMatch => {},
        }
    }

    /// Inserts pasted text, which is one command rather than a stream of keys.
    fn paste(&mut self, text: &str) {
        if let Some(prompt) = self.prompt.as_mut() {
            prompt.paste(text);
            return;
        }
        if self.palette_open {
            // The palette is modal: a paste belongs to its query, not to the
            // document underneath it.
            self.palette.paste(text);
            return;
        }
        if self.history_open {
            // The undo-tree panel has no field for pasted text to land in,
            // and a paste reaching the document under a modal panel would
            // grow the very tree being read.
            return;
        }
        self.editor.paste(text);
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
            Answer::Pending | Answer::Cancelled => Flow::Running,
            Answer::Goto(line) => {
                self.goto(line);
                Flow::Running
            },
            Answer::SaveAs(path) => self.save_as(&path),
            Answer::Do(Deed::Quit) => Flow::Exit,
            Answer::Do(Deed::Reload) => self.reload(),
            Answer::Do(Deed::Open(path)) => self.open_file(&path),
        }
    }
}
