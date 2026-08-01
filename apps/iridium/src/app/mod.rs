//! The whole editor as a state machine: keys in, editor state and a cell
//! buffer out.
//!
//! There is no terminal here. [`App`] is driven by [`TerminalInput`] values and
//! draws into a [`Surface`](iridium_tui::cell::Surface), both of which a test
//! can construct, so everything this program decides — what a key does, when a
//! save is refused, what the statusline says — runs on the host. What is left
//! for [`run`](crate::run) is opening a terminal and blocking on it.
//!
//! The three modules beside this one extend the same [`App`] rather than
//! introducing another type, which is how the kernel splits its own editor:
//! [`commands`] is the verbs this face contributes and their keys, `document`
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
//! see [`commands`]. `App::run_host_command` is the only place they are
//! dispatched, and it is reached only because the kernel reported
//! [`EditorKeyResult::HostCommand`].
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
//! [`ClipboardOperation`] hands the host text to keep and asks for text back.
//! There is no system clipboard behind it: this crate depends on the kernel and
//! the face and nothing else, and the terminal answer — OSC 52 — is a byte
//! sequence only the driver may write, which exposes no hook for one. So a copy
//! here is readable by a paste here and by nothing else. It is stated rather
//! than hidden, because a clipboard that silently is not the system's is worse
//! than one documented not to be.

pub mod commands;
pub mod prompt;

mod document;
mod view;

#[cfg(test)]
mod tests;

use iridium_editor::input::SearchAction;
use iridium_editor::{ClipboardOperation, CommandId, Editor, EditorKeyResult, KeyEvent};
use iridium_tui::frame::{Frame, SearchOutcome, SearchOverlay};
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
        let mut editor = Editor::with_defaults();

        for meta in commands::command_metas() {
            editor.register_command(meta).map_err(StartupError::Command)?;
        }
        editor
            .push_keymap(commands::keymap())
            .map_err(StartupError::Keymap)?;

        if let Some(choice) = options.theme.as_ref() {
            editor.set_theme(theme::load(choice).map_err(StartupError::Theme)?);
        }

        let file = match options.path.as_ref() {
            Some(path) => {
                let (file, text) = TextFile::open(path).map_err(StartupError::File)?;
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
            search: SearchOverlay::new(),
            search_open: false,
            prompt: None,
            message: None,
            clipboard: String::new(),
            columns: 0,
            rows: 0,
        };

        if let Some(line) = options.line {
            app.goto(line);
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
                self.search_action(action);
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

    /// Runs one of the commands this face contributes.
    ///
    /// The kernel reports a command it does not implement rather than dropping
    /// the keypress, so an id that arrives here and is not one of ours came
    /// from a keymap layer someone else pushed. Saying so is the only honest
    /// answer: the key was consumed, and silence would look like a dead key.
    fn run_host_command(&mut self, command: &CommandId) -> Flow {
        if command == &commands::FILE_SAVE {
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
        } else {
            self.message = Some(Message::error(format!(
                "`{command}` is bound to a key but nothing runs it"
            )));
            Flow::Running
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
    fn search_action(&mut self, action: SearchAction) {
        match action {
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
        }
    }
}
