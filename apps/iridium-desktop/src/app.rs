//! The whole desktop session as an event-driven state machine: winit events
//! in, composed frames out.
//!
//! [`DesktopApp`] is this face's counterpart of the terminal face's `App`:
//! the kernel owns every verb, and a key reaches it through
//! [`Editor::handle_key`] after [`crate::keys`] translates it. The verbs this
//! face *adds* — save, and save-anyway — are registered as host commands and
//! bound in a keymap layer ([`crate::commands`]), exactly the kernel-first
//! dispatch pattern the terminal face established: the kernel resolves the
//! chord, reports [`EditorKeyResult::HostCommand`], and `dispatch_host_command`
//! here is the only place a host verb runs. A host command nothing here
//! implements is reported on the prompt strip rather than silently dropped —
//! a key that visibly names what it could not do is the face being honest; a
//! key that silently does nothing is a dead key.
//!
//! # Rendering is event-driven
//!
//! A frame is composed only on `RedrawRequested`, and a redraw is requested
//! only after input, resize or scale change — mouse effects included, and
//! only actual effects: a cursor merely gliding across the window repaints
//! nothing. There is no animation loop, so an idle editor costs nothing —
//! the same discipline as the terminal face, with the one visible consequence
//! that the caret does not blink while the keyboard is idle.
//!
//! # The mouse drives the kernel's machinery
//!
//! Clicks, drags and multi-clicks go through the kernel's `MouseHandler`
//! with positions resolved by the compositor's hit test — see
//! [`crate::mouse`] for the synthesized-grid seam between the two. The wheel
//! writes `scroll_y` directly under the same clamp every other scroll write
//! uses, because the scroll offset is the face's to own.
//!
//! # The clipboard is the system's
//!
//! Copy, cut and paste round-trip through the macOS pasteboard via `arboard`.
//! A pasteboard that cannot be reached is reported on the prompt strip, never
//! swallowed — and the delete half of a cut applies only after the pasteboard
//! holds the text, because a cut whose copy failed would destroy the only
//! copy.
//!
//! # Saving is real, and shared
//!
//! The file layer is `iridium-file` — the identical atomic-save, staleness-by
//! -byte-comparison code the terminal face runs. A save is refused when the
//! file changed on disk (the forced chord overwrites), an unnamed buffer asks
//! for a name on the prompt strip, and closing the window over unsaved
//! changes asks first. Dirtiness is content-based: the document is compared
//! against the bytes the file held when it and this program last agreed, so a
//! document undone back to its saved state is clean.
//!
//! # Scroll is owned here, in physical pixels
//!
//! Like the web face, this face owns `scroll_y` and the compositor answers
//! layout questions about it: `max_scroll_y` bounds it and `cursor_anchor_y`
//! drives scroll-to-caret, both wrap-aware from the last composed frame. The
//! kernel's own viewport is still synced on resize — not for painting, which
//! ignores it, but because `cursor.pageUp`/`pageDown` take their hop from
//! `viewport.visible_lines`, and a page key must hop what is actually on
//! screen. That sync goes through `state_mut`, which discards sticky columns
//! and pending chords; it runs only on resize and scale change, where the
//! terminal face pays the same price for the same reason.

use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use iridium_editor::commands::builtin::{HISTORY_TOGGLE_PANEL, PALETTE_OPEN};
use iridium_editor::commands::palette::CommandMru;
use iridium_editor::input::{CommandRunError, SearchAction};
use iridium_editor::render::{FrameCompositor, FrameTarget};
use iridium_editor::theme::Theme;
use iridium_editor::{
    ClipboardOperation, CommandArgs, CommandId, Editor, EditorKeyResult, KeyCode, KeyEvent,
    KeymapError, Language, MouseResult, Position, RegistryError,
};
use iridium_file::{FileError, TextFile};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

use crate::command_palette::{CommandPalette, PaletteOutcome};
use crate::commands;
use crate::context_menu::{ContextMenu, MenuOutcome};
use crate::highlight::HighlightCache;
use crate::history_overlay::{HistoryOutcome, HistoryPanel};
use crate::keys;
use crate::latency::LatencyMonitor;
use crate::mouse::{self, Grid, Pointer};
use crate::overlay::{OverlayPainter, PanelContent, PanelGeometry, StripContent};
use crate::prompt::{Answer, Deed, Message, Prompt};
use crate::search::{SearchOutcome, SearchOverlay};
use crate::surface::NativeSurface;
use crate::units::{index_to_f32, pixel_from_f64, pixel_to_index, scale_to_f32, u32_to_f32};

/// The window title before a session has a file to name.
const TITLE: &str = "iridium";

/// Base font size in logical pixels; multiplied by the window's scale factor
/// before it reaches the compositor, matching the web face's formula
/// (`14.0 * devicePixelRatio`).
const BASE_FONT_SIZE: f32 = 14.0;

/// The vertical padding scroll-to-caret keeps between the caret and the
/// viewport edge, matching the compositor's own content padding.
const SCROLL_PADDING: f32 = 10.0;

/// The editor font, embedded so the face needs no asset path to exist at
/// runtime — the same `JetBrains Mono` the web demo serves its canvas.
static FONT: &[u8] = include_bytes!("../../../examples/web/public/fonts/JetBrainsMono-Regular.ttf");

/// What went wrong before a window could open.
#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    /// The file named on the command line could not be opened.
    #[error(transparent)]
    File(#[from] FileError),
    /// A command this face contributes could not be registered.
    #[error(transparent)]
    Command(#[from] RegistryError),
    /// The keymap layer this face pushes was refused.
    #[error(transparent)]
    Keymap(#[from] KeymapError),
}

/// What the command line asked for.
#[derive(Debug, Default)]
pub struct Options {
    /// The file to edit, if the invocation named one.
    pub path: Option<PathBuf>,
}

/// Whether the session carries on after an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Wait for the next event.
    Running,
    /// Leave.
    Exit,
}

/// The window-bound half of the session: everything that cannot exist until
/// winit hands over an active event loop, created in `resumed` and owned as
/// one unit so it is either all there or not at all.
struct Shell {
    /// The window, shared with the surface that holds it alive.
    window: Arc<Window>,
    /// The swapchain on that window.
    surface: NativeSurface,
    /// The kernel's per-frame composition, driven from `RedrawRequested`.
    compositor: FrameCompositor,
    /// The prompt strip's painter, run as a second pass after the compositor.
    overlay: OverlayPainter,
}

impl Shell {
    /// Opens the window and brings up the GPU behind it.
    ///
    /// The font size is set *before* the font is loaded: `load_font` is the
    /// compositor's only path that measures character width, and it measures
    /// at the size in force when it runs. The overlay's painter follows the
    /// same rule.
    ///
    /// `theme` is the editor's own: the compositor must never sit frozen on
    /// a preset the editor does not hold, or its clear color and chrome
    /// colors drift from everything the resolver paints.
    fn open(event_loop: &ActiveEventLoop, theme: Theme) -> Result<Self, String> {
        let attributes = Window::default_attributes().with_title(TITLE);
        // Without this, macOS composes ⌥+letter into a character — ⌥F arrives
        // as `ƒ` with the `alt` bit already spent — and every ⌥ chord the
        // keymap binds is unreachable. `Both` covers the left and right keys,
        // because a chord must not depend on which ⌥ the hand reached. The
        // accented characters ⌥ would otherwise compose are the stated price,
        // and they remain reachable through the system character viewer.
        #[cfg(target_os = "macos")]
        let attributes = {
            use winit::platform::macos::{OptionAsAlt, WindowAttributesExtMacOS as _};
            attributes.with_option_as_alt(OptionAsAlt::Both)
        };
        let window = event_loop
            .create_window(attributes)
            .map_err(|error| format!("cannot open a window: {error}"))?;
        let window = Arc::new(window);

        let size = window.inner_size();
        let surface = NativeSurface::from_window(Arc::clone(&window), size.width, size.height)
            .map_err(|error| format!("cannot bring up the GPU: {error}"))?;

        let mut compositor = FrameCompositor::new(
            surface.device(),
            surface.queue(),
            surface.format(),
            surface.width(),
            surface.height(),
        )
        .map_err(|error| format!("cannot create the compositor: {error}"))?;

        compositor.set_theme(theme);
        let font_size = BASE_FONT_SIZE * scale_to_f32(window.scale_factor());
        compositor.set_font_size(font_size);
        compositor.load_font(FONT.to_vec());

        let mut overlay = OverlayPainter::new(
            surface.device(),
            surface.queue(),
            surface.format(),
            surface.width(),
            surface.height(),
        )
        .map_err(|error| format!("cannot create the overlay painter: {error}"))?;
        overlay.set_font(font_size, FONT.to_vec());
        overlay.set_scale(scale_to_f32(window.scale_factor()));

        Ok(Self {
            window,
            surface,
            compositor,
            overlay,
        })
    }
}

/// One editing session in one window.
pub struct DesktopApp {
    /// The kernel.
    editor: Editor,
    /// The window-bound state, absent until the event loop resumes.
    shell: Option<Shell>,
    /// The face's vertical scroll offset, in physical pixels.
    scroll_y: f32,
    /// The modifier state winit last reported, applied to every key press
    /// and mouse press.
    modifiers: ModifiersState,
    /// The kernel's mouse machinery plus the pointer state winit reports
    /// piecemeal. See [`crate::mouse`].
    pointer: Pointer,
    /// The system clipboard, opened lazily so a pasteboard that cannot be
    /// reached fails on the keystroke that needed it — visibly — rather than
    /// at startup.
    clipboard: Option<arboard::Clipboard>,
    /// The file being edited, if the session named one.
    file: Option<TextFile>,
    /// The kernel's tree-sitter spans, cached per parse generation and
    /// resolved through the compositor's highlight seam each frame. See
    /// [`crate::highlight`].
    syntax: HighlightCache,
    /// The search panel. Kept across closes so re-opening offers the last
    /// query back, which is what [`SearchOverlay`] is built for.
    search: SearchOverlay,
    /// Whether the search panel is on screen. It is not modal: keys it does
    /// not bind stay the host's, so a save chord works mid-search.
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
    /// The right-click context menu, while one is up. Modal while it is:
    /// it holds key focus, and the click that leaves it is spent leaving it.
    menu: Option<ContextMenu>,
    /// The undo-tree panel.
    history: HistoryPanel,
    /// Whether the undo-tree panel is on screen. Modal while it is.
    history_open: bool,
    /// The question on the prompt strip, if one is open. Modal while it is.
    prompt: Option<Prompt>,
    /// What the last input had to say, shown on the strip until the next key.
    message: Option<Message>,
    /// Keydown-to-present measurement, armed by `IRIDIUM_LATENCY`; one
    /// branch per event when it is not. See [`crate::latency`].
    latency: LatencyMonitor,
    /// The window title as last set, so an unchanged title costs nothing.
    title: String,
    /// What killed the session from inside the event loop, if anything;
    /// [`crate::run`] reports it after the loop returns, because winit's
    /// callbacks have nowhere to return an error to.
    failure: Option<String>,
}

impl std::fmt::Debug for DesktopApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopApp")
            .field("scroll_y", &self.scroll_y)
            .field("prompt", &self.prompt)
            .field("message", &self.message)
            .field("failure", &self.failure)
            .finish_non_exhaustive()
    }
}

impl DesktopApp {
    /// Starts a session from what the command line asked for: registers this
    /// face's commands, pushes its keymap layer, and reads the named file
    /// into the kernel.
    ///
    /// # Errors
    ///
    /// [`StartupError`] for a file that will not open as text and for the two
    /// registration failures — which cannot happen for the tables in
    /// [`crate::commands`], a unit test there proves it against a real
    /// registry, and are reported rather than ignored so they cannot become
    /// silent if those tables change.
    pub fn new(options: Options) -> Result<Self, StartupError> {
        let mut editor = Editor::with_defaults();

        for meta in commands::command_metas() {
            editor.register_command(meta)?;
        }
        editor.push_keymap(commands::keymap())?;

        let file = match options.path {
            Some(path) => {
                let (file, text) = TextFile::open(&path)?;
                editor.set_content(&text);
                Some(file)
            },
            None => None,
        };

        // After the content: setting a language parses the document, and
        // doing it first would parse an empty one and then parse again.
        if let Some(language) = file.as_ref().and_then(|file| language_of(file.path())) {
            editor.set_language(language);
        }

        Ok(Self {
            editor,
            shell: None,
            scroll_y: 0.0,
            modifiers: ModifiersState::empty(),
            pointer: Pointer::new(),
            clipboard: None,
            file,
            syntax: HighlightCache::new(),
            search: SearchOverlay::new(),
            search_open: false,
            palette: CommandPalette::new(),
            palette_open: false,
            mru: CommandMru::default(),
            menu: None,
            history: HistoryPanel::new(),
            history_open: false,
            prompt: None,
            message: None,
            latency: LatencyMonitor::from_env(),
            title: String::new(),
            failure: None,
        })
    }

    /// What killed the session from inside the event loop, if anything.
    #[must_use]
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    /// Writes the latency summary to standard error, when there is one to
    /// write.
    ///
    /// Called by [`crate::run`] after the event loop returns cleanly — the
    /// summary belongs to the whole session, so it prints once, at the end.
    /// A disarmed monitor prints nothing at all; an armed one that never
    /// recorded a sample says so rather than inventing a distribution.
    pub fn report_latency(&self) {
        if !self.latency.is_armed() {
            return;
        }
        let line = self.latency.summary().map_or_else(
            || "latency: no keydown->present samples were recorded".to_owned(),
            |summary| summary.to_string(),
        );
        let _ = writeln!(io::stderr(), "{line}");
    }

    /// Whether the document differs from what is on disk.
    ///
    /// Compared against the bytes the file held when it and this program last
    /// agreed, not against a revision counter: a document undone back to its
    /// saved state is clean, and a counter would call it modified for the
    /// rest of the session. The comparison is a rope against a `&str`, which
    /// returns on a length mismatch without reading a byte.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        let saved = self
            .file
            .as_ref()
            .and_then(TextFile::saved_text)
            .unwrap_or("");
        *self.editor.state().document.rope() != saved
    }

    /// Handles one translated key press, in the terminal face's order: the
    /// open prompt if there is one, then the modal palette, then the modal
    /// undo-tree panel, then the search panel (which leaves unbound keys to
    /// the kernel), then the kernel — then scroll-to-caret and a repaint.
    fn press(&mut self, event: &KeyEvent) -> Flow {
        // A message describes the key before this one.
        self.message = None;

        let flow = if self.prompt.is_some() {
            self.prompt_key(event)
        } else if self.menu.is_some() {
            self.drive_menu(event)
        } else if self.palette_open {
            self.drive_palette(event)
        } else if self.history_open {
            self.drive_history(event)
        } else {
            self.search_or_document_key(event)
        };

        self.refresh_title();
        if let Some(shell) = &mut self.shell {
            // A caret that stays in its off-phase across a movement looks
            // like a caret that vanished.
            shell.compositor.reset_blink();
            shell.window.request_redraw();
        }
        flow
    }

    /// Hands a key to the open search panel, and to the kernel for anything
    /// the panel does not bind.
    ///
    /// The panel leaves what it does not bind to the host, so a save chord is
    /// not dead while a search is open — the exact rule the terminal face
    /// applies.
    fn search_or_document_key(&mut self, event: &KeyEvent) -> Flow {
        if self.search_open {
            match self.search.handle_key(event, &mut self.editor) {
                SearchOutcome::Ignored => {},
                SearchOutcome::Handled | SearchOutcome::Replaced(_) => {
                    self.ensure_caret_visible();
                    return Flow::Running;
                },
                SearchOutcome::Closed => {
                    self.search_open = false;
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

    /// Hands a key to the open palette and acts on the outcome.
    ///
    /// One chord is intercepted before the palette sees it: the paste chord,
    /// which pastes the system clipboard into the query — the palette is
    /// modal, so a paste belongs to its field, not to the document under it.
    fn drive_palette(&mut self, event: &KeyEvent) -> Flow {
        if is_paste_chord(event) {
            match self.clipboard_text() {
                Ok(text) => self.palette.paste(&text),
                Err(message) => self.message = Some(message),
            }
            return Flow::Running;
        }
        match self.palette.handle_key(event, &self.editor, &self.mru) {
            PaletteOutcome::Handled => Flow::Running,
            PaletteOutcome::Closed => {
                self.palette_open = false;
                Flow::Running
            },
            PaletteOutcome::Run(command) => {
                self.palette_open = false;
                self.run_chosen_command(&command)
            },
        }
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

    /// Runs a command a panel resolved — the palette's `Enter`, or the
    /// context menu's row: the kernel's own if it implements it, this face's
    /// if not, and an honest message when neither does.
    ///
    /// There is deliberately one of these. A menu verb reaches the document
    /// through the same kernel-first, command-sourced, undoable path a chord
    /// and a palette entry do; a second dispatch would be a second set of
    /// semantics to keep in step.
    ///
    /// The recency list records only commands that actually dispatched, so an
    /// entry nothing runs cannot become a ranked favourite — and a verb run
    /// from the menu trains the same ranking a palette run does.
    fn run_chosen_command(&mut self, command: &CommandId) -> Flow {
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
                    "`{command}` is not available in the desktop yet"
                )));
                Flow::Running
            },
        }
    }

    /// Hands a key to the open prompt and acts on the answer.
    ///
    /// One chord is intercepted before the prompt sees it: the paste chord
    /// (`Ctrl+V`/`⌘V`), which pastes the system clipboard into the prompt's
    /// field — typing a long path by hand is the alternative. Everything
    /// else is the prompt's, and the prompt swallows what it does not name.
    fn prompt_key(&mut self, event: &KeyEvent) -> Flow {
        if is_paste_chord(event) {
            match self.clipboard_text() {
                Ok(text) => {
                    if let Some(prompt) = self.prompt.as_mut() {
                        prompt.paste(&text);
                    }
                },
                // Shown once the prompt closes; a clipboard failure must not
                // tear down the question being asked.
                Err(message) => self.message = Some(message),
            }
            return Flow::Running;
        }

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
            Answer::SaveAs(path) => self.save_as(&path),
            Answer::Do(Deed::Quit) => Flow::Exit,
        }
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
            // The count and captures are not read for the same reason the
            // terminal face leaves them: nothing in this face's keymap can
            // produce them.
            EditorKeyResult::HostCommand { command, .. } => self.run_host_command(&command),
        }
    }

    /// Runs one of the commands this face contributes, or says visibly that
    /// it cannot.
    ///
    /// The kernel reports a command it does not implement rather than
    /// dropping the keypress; a host command this face does not implement
    /// either surfaces on the prompt strip. Saying so is the only honest
    /// answer: the key was consumed, and silence would look like a dead key.
    fn run_host_command(&mut self, command: &CommandId) -> Flow {
        self.dispatch_host_command(command).unwrap_or_else(|| {
            self.message = Some(Message::error(format!(
                "`{command}` is not wired into the desktop shell yet"
            )));
            Flow::Running
        })
    }

    /// Dispatches a host command this face implements, or `None` for one it
    /// does not.
    fn dispatch_host_command(&mut self, command: &CommandId) -> Option<Flow> {
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
        } else {
            return None;
        };
        Some(flow)
    }

    /// Writes the document to its file.
    ///
    /// An unnamed buffer is not a dead end: it asks for a name on the prompt
    /// strip. A file that changed on disk refuses the unforced save and names
    /// the chord that overwrites, because a refusal that does not say the way
    /// out is a dead end with better manners.
    fn save(&mut self, force: bool) -> Flow {
        if self.editor.state().read_only {
            // Nothing could have changed, so a save here can only write a
            // stale buffer over a file that something else may have moved on.
            self.message = Some(Message::error("the document is read-only"));
            return Flow::Running;
        }
        if self.file.is_none() {
            self.prompt = Some(Prompt::save_as());
            return Flow::Running;
        }

        let text = self.editor.content();
        // Unreachable: the check above returned. Reported rather than ignored
        // so that a later edit cannot make it silent.
        let Some(file) = self.file.as_mut() else {
            self.message = Some(Message::error("there is no file to write"));
            return Flow::Running;
        };
        let outcome = file.save(&text, force).map(|()| file.display_name());
        self.message = Some(match outcome {
            Ok(name) => Message::notice(format!("wrote {name}")),
            Err(error @ FileError::Changed { .. }) => {
                Message::error(format!("{error} — Ctrl+Alt+S (⌘⌥S) saves anyway"))
            },
            Err(error) => Message::error(error.to_string()),
        });
        Flow::Running
    }

    /// Writes an unnamed buffer to a path for the first time.
    ///
    /// The save is not forced, and a file that appeared at this path while
    /// the name was being typed is reported rather than overwritten. The
    /// buffer takes the name only when the write succeeded; a failed save-as
    /// leaves it unnamed, which is what it still is.
    fn save_as(&mut self, path: &Path) -> Flow {
        let mut file = TextFile::new(path);
        let text = self.editor.content();
        match file.save(&text, false) {
            Ok(()) => {
                self.message = Some(Message::notice(format!("wrote {}", file.display_name())));
                self.file = Some(file);
                // The name is what says what the document is written in, and
                // until now there was no name.
                if let Some(language) = language_of(path) {
                    self.editor.set_language(language);
                }
            },
            Err(error) => self.message = Some(Message::error(error.to_string())),
        }
        Flow::Running
    }

    /// Leaves, asking first when there are unsaved changes.
    fn request_quit(&mut self) -> Flow {
        if self.is_dirty() {
            self.prompt = Some(Prompt::confirm(
                "Unsaved changes. Quit without saving? (y/n)",
                Deed::Quit,
            ));
            if let Some(shell) = &self.shell {
                shell.window.request_redraw();
            }
            return Flow::Running;
        }
        Flow::Exit
    }

    /// Keeps or supplies clipboard text, on the system pasteboard.
    ///
    /// The delete half of a cut is the kernel's own reversible command,
    /// applied unchanged — and applied only once the pasteboard holds the
    /// text, because a cut whose copy failed would destroy the only copy.
    fn clipboard(&mut self, operation: ClipboardOperation) {
        match operation {
            ClipboardOperation::Copy(text) => {
                if let Err(error) = self.clipboard_store(text) {
                    self.message = Some(Message::error(error));
                }
            },
            ClipboardOperation::Cut { text, command } => match self.clipboard_store(text) {
                Ok(()) => self.editor.apply_command(command),
                Err(error) => {
                    self.message = Some(Message::error(format!(
                        "{error} — the cut left the document untouched"
                    )));
                },
            },
            ClipboardOperation::Paste => match self.clipboard_text() {
                Ok(text) => self.editor.paste(&text),
                Err(message) => self.message = Some(message),
            },
        }
    }

    /// The system clipboard, opened on first use and kept.
    ///
    /// # Errors
    ///
    /// A user-visible sentence when the pasteboard cannot be reached.
    fn clipboard_handle(&mut self) -> Result<&mut arboard::Clipboard, String> {
        if self.clipboard.is_none() {
            match arboard::Clipboard::new() {
                Ok(clipboard) => self.clipboard = Some(clipboard),
                Err(error) => {
                    return Err(format!("cannot reach the system clipboard: {error}"));
                },
            }
        }
        self.clipboard
            .as_mut()
            .ok_or_else(|| "cannot reach the system clipboard".to_owned())
    }

    /// Puts text on the system pasteboard.
    fn clipboard_store(&mut self, text: String) -> Result<(), String> {
        let clipboard = self.clipboard_handle()?;
        clipboard
            .set_text(text)
            .map_err(|error| format!("cannot write to the clipboard: {error}"))
    }

    /// Reads text from the system pasteboard.
    ///
    /// # Errors
    ///
    /// A ready-to-show [`Message`]: a pasteboard holding no text is a notice
    /// — it is an answer, not a failure — and anything else is an error.
    fn clipboard_text(&mut self) -> Result<String, Message> {
        let clipboard = self.clipboard_handle().map_err(Message::error)?;
        match clipboard.get_text() {
            Ok(text) => Ok(text),
            Err(arboard::Error::ContentNotAvailable) => {
                Err(Message::notice("the clipboard holds no text"))
            },
            Err(error) => Err(Message::error(format!(
                "cannot read the clipboard: {error}"
            ))),
        }
    }

    /// Opens or closes the search panel.
    ///
    /// Only the two panel actions need anything here: the kernel has already
    /// moved to the next or previous match by the time it reports one.
    fn search_action(&mut self, action: &SearchAction) {
        match action {
            SearchAction::OpenSearch => {
                self.search_open = true;
                self.search.open(&mut self.editor);
            },
            SearchAction::CloseSearch => {
                if self.search_open {
                    self.search_open = false;
                    self.search.close(&mut self.editor);
                }
            },
            SearchAction::NextMatch | SearchAction::PreviousMatch => {},
        }
    }

    /// Sets the window title to the document's name and dirty state, when it
    /// changed.
    fn refresh_title(&mut self) {
        let name = self.file.as_ref().map(TextFile::display_name);
        let title = title_for(name.as_deref(), self.is_dirty());
        if title != self.title {
            if let Some(shell) = &self.shell {
                shell.window.set_title(&title);
            }
            self.title = title;
        }
    }

    // =========================================================================
    // Mouse
    // =========================================================================

    /// Records where the cursor is and extends a drag if one is under way.
    fn pointer_moved(&mut self, x: f32, y: f32) {
        self.pointer.set_position(x, y);
        if self.menu.is_some() {
            // The one overlay this face steers with the pointer; the document
            // underneath is not being selected while a menu is up.
            self.hover_menu();
            return;
        }
        if !self.pointer.is_dragging() || self.prompt.is_some() {
            return;
        }
        let Some((grid, line, column)) = self.hit_test() else {
            return;
        };
        let modifiers = keys::kernel_modifiers(self.modifiers);
        let result = {
            let state = self.editor.state();
            self.pointer.drag(
                line,
                column,
                modifiers,
                grid,
                &state.document,
                &state.cursor,
            )
        };
        self.apply_mouse(result);
    }

    /// Handles the primary button going down.
    ///
    /// Modal like the keyboard, and in the same order: while a prompt is open
    /// a click answers nothing and edits nothing, so it is swallowed; an open
    /// context menu owns the click ([`Self::menu_click`]); an open modal panel
    /// spends it being dismissed ([`Self::dismiss_modal_panel`]); only then
    /// does the document see it. The wheel stays live throughout — reading the
    /// document can inform the answer.
    fn pointer_pressed(&mut self) -> Flow {
        if self.prompt.is_some() {
            return Flow::Running;
        }
        if self.menu.is_some() {
            return self.menu_click();
        }
        if self.dismiss_modal_panel() {
            return Flow::Running;
        }
        self.message = None;
        let Some((grid, line, column)) = self.hit_test() else {
            return Flow::Running;
        };
        let modifiers = keys::kernel_modifiers(self.modifiers);
        let result = {
            let state = self.editor.state();
            self.pointer.press(
                line,
                column,
                modifiers,
                grid,
                &state.document,
                &state.cursor,
            )
        };
        self.apply_mouse(result);
        Flow::Running
    }

    /// Spends a press on the modal panel that is open, reporting whether it
    /// was spent.
    ///
    /// The click-through fix (D-4): before this, a click while the palette or
    /// the undo tree was up fell straight through to the document and moved
    /// the caret under a panel the user was still reading. A press outside the
    /// panel now dismisses it, macOS-style, and a press *on* it is swallowed
    /// and leaves it up — those panels are keyboard-driven, so there is
    /// nothing inside one for a click to do, but dismissing on a click that
    /// landed on the panel itself would be a trap. Either way the document
    /// never sees the click.
    ///
    /// The search panel is deliberately not included: it is not modal — keys
    /// it does not bind stay the host's — so clicking into the document while
    /// it is up is the shipped, wanted behaviour.
    fn dismiss_modal_panel(&mut self) -> bool {
        if !(self.palette_open || self.history_open) {
            return false;
        }
        if !self.pointer_is_on_a_panel() {
            self.palette_open = false;
            self.history_open = false;
            // Only a dismissal changed the frame; a press on the panel itself
            // leaves the screen exactly as it was.
            self.request_redraw();
        }
        true
    }

    /// Whether the pointer is on any panel the last frame actually painted.
    fn pointer_is_on_a_panel(&self) -> bool {
        let Some(shell) = &self.shell else {
            return false;
        };
        let (x, y) = self.pointer.position();
        shell
            .overlay
            .painted_panels()
            .iter()
            .flatten()
            .any(|geometry| geometry.contains(x, y))
    }

    /// Handles the secondary button going down: the context menu.
    ///
    /// Only the secondary button opens it (D-3); Ctrl+click keeps the
    /// add-cursor meaning it shipped with. A press while a prompt or a modal
    /// panel is up is spent on that panel and opens nothing — modal means
    /// modal.
    fn secondary_pressed(&mut self) {
        if self.prompt.is_some() || self.palette_open || self.history_open {
            return;
        }
        // A message describes the input before this one.
        self.message = None;
        self.place_caret_under_pointer();
        let (x, y) = self.pointer.position();
        self.menu = Some(ContextMenu::open(&self.editor, x, y));
        if let Some(shell) = &mut self.shell {
            shell.compositor.reset_blink();
            shell.window.request_redraw();
        }
    }

    /// Moves the caret to the cell the pointer is over, unless the pointer is
    /// inside a selection.
    ///
    /// D-6, the macOS rule: right-pressing inside a selection leaves it alone
    /// — it is what the verbs are about to act on — and right-pressing outside
    /// one moves the caret to the clicked cell, so Cut and Copy act where the
    /// user pointed rather than where the caret happened to be.
    fn place_caret_under_pointer(&mut self) {
        let Some((_, line, column)) = self.hit_test() else {
            return;
        };
        let position = Position::new(line, column);
        let state = self.editor.state();
        if mouse::selection_covers(&state.cursor, position) {
            return;
        }
        let result = mouse::caret_at(position, &state.document, &state.cursor);
        self.apply_mouse(result);
    }

    /// Hands a key to the open context menu and acts on the outcome.
    fn drive_menu(&mut self, event: &KeyEvent) -> Flow {
        let Some(menu) = self.menu.as_mut() else {
            return Flow::Running;
        };
        match menu.handle_key(event) {
            MenuOutcome::Handled => Flow::Running,
            MenuOutcome::Closed => {
                self.menu = None;
                Flow::Running
            },
            MenuOutcome::Run(command) => {
                self.menu = None;
                self.run_chosen_command(&command)
            },
        }
    }

    /// Spends a press on the open context menu: a row runs, the padding does
    /// nothing, and anything outside dismisses.
    fn menu_click(&mut self) -> Flow {
        let (x, y) = self.pointer.position();
        let row = self
            .painted_menu()
            .filter(|(_, geometry)| geometry.contains(x, y))
            .map(|(menu, geometry)| geometry.row_at(x, y, menu.rows()));
        match row {
            None => {
                self.menu = None;
                self.request_redraw();
                Flow::Running
            },
            // On the menu, but on its padding: nothing runs, nothing changes.
            Some(None) => Flow::Running,
            Some(Some(row)) => match self.menu.as_mut().and_then(|menu| menu.activate(row)) {
                Some(command) => {
                    self.menu = None;
                    let flow = self.run_chosen_command(&command);
                    self.request_redraw();
                    flow
                },
                // A separator or a greyed verb: the menu stays up rather than
                // vanishing on a click that meant nothing.
                None => Flow::Running,
            },
        }
    }

    /// Asks for the next frame, when there is a window to ask.
    fn request_redraw(&self) {
        if let Some(shell) = &self.shell {
            shell.window.request_redraw();
        }
    }

    /// Highlights the menu row the pointer is over, repainting only when the
    /// highlight actually moved.
    fn hover_menu(&mut self) {
        let (x, y) = self.pointer.position();
        let Some(row) = self
            .painted_menu()
            .filter(|(_, geometry)| geometry.contains(x, y))
            .and_then(|(menu, geometry)| geometry.row_at(x, y, menu.rows()))
        else {
            return;
        };
        if self.menu.as_mut().is_some_and(|menu| menu.hover(row)) {
            self.request_redraw();
        }
    }

    /// The open menu as the last frame painted it.
    ///
    /// The menu is composed last of all the panels ([`Self::panel_contents`]),
    /// so it is the last placement the painter recorded. `None` when it is
    /// closed, when the window could not hold it, or before the first frame
    /// that showed it — a press that cannot be resolved against painted chrome
    /// is honestly not on the menu.
    fn painted_menu(&self) -> Option<(&ContextMenu, PanelGeometry)> {
        let menu = self.menu.as_ref()?;
        let geometry = self
            .shell
            .as_ref()?
            .overlay
            .painted_panels()
            .last()
            .copied()
            .flatten()?;
        Some((menu, geometry))
    }

    /// Handles the window losing focus.
    fn blurred(&mut self) {
        // A chord left pending across a blur would eat the first keystroke
        // after the user came back; the kernel names this path explicitly.
        self.editor.abort_pending_key_sequence();
        // A menu is aimed at a click that is no longer being made.
        self.menu = None;
    }

    /// Handles the primary button going up, ending any drag.
    fn pointer_released(&mut self) {
        let Some((grid, ..)) = self.hit_test() else {
            return;
        };
        let result = {
            let state = self.editor.state();
            self.pointer.release(grid, &state.document, &state.cursor)
        };
        self.apply_mouse(result);
    }

    /// Resolves the pointer's position to a document cell through the
    /// compositor — the honest, wrap- and fold-aware mapping.
    fn hit_test(&self) -> Option<(Grid, usize, usize)> {
        let shell = self.shell.as_ref()?;
        let (x, y) = self.pointer.position();
        let (line, column) = shell.compositor.pixel_to_position(
            &self.editor,
            self.editor.fold_state(),
            self.scroll_y,
            x,
            y,
        );
        let grid = Grid {
            line_height: shell.compositor.line_height(),
            width: u32_to_f32(shell.surface.width()),
            height: u32_to_f32(shell.surface.height()),
        };
        Some((grid, line, column))
    }

    /// Acts on what the kernel's mouse machinery decided, and repaints only
    /// when something changed.
    fn apply_mouse(&mut self, result: MouseResult) {
        match result {
            MouseResult::Command(command) => {
                self.editor.apply_command(command);
                if let Some(shell) = &mut self.shell {
                    shell.compositor.reset_blink();
                    shell.window.request_redraw();
                }
            },
            MouseResult::Scroll { delta_y, .. } => self.scroll_by(delta_y),
            // Unreachable through the synthesized grid (see `crate::mouse`),
            // handled honestly in case the seam widens: the kernel named an
            // effect and the face must not drop it.
            MouseResult::ToggleFold { line } => {
                self.editor.toggle_fold_at(line);
                if let Some(shell) = &self.shell {
                    shell.window.request_redraw();
                }
            },
            MouseResult::ScrollToLine { target_line } => {
                let line_height = self
                    .shell
                    .as_ref()
                    .map_or(0.0, |shell| shell.compositor.line_height());
                self.scroll_y = index_to_f32(target_line) * line_height;
                self.clamp_scroll();
                if let Some(shell) = &self.shell {
                    shell.window.request_redraw();
                }
            },
            MouseResult::Handled | MouseResult::Ignored => {},
        }
    }

    /// Handles a wheel or trackpad scroll, in the kernel's sign convention.
    fn wheel(&mut self, delta: &MouseScrollDelta) {
        let Some(shell) = &self.shell else {
            return;
        };
        let (_, delta_y) = mouse::wheel_delta(delta, shell.compositor.line_height());
        self.scroll_by(delta_y);
    }

    /// Moves the scroll offset under the usual clamp, repainting only when it
    /// actually moved.
    fn scroll_by(&mut self, delta_y: f32) {
        let before = self.scroll_y;
        self.scroll_y += delta_y;
        self.clamp_scroll();
        if (self.scroll_y - before).abs() > f32::EPSILON
            && let Some(shell) = &self.shell
        {
            shell.window.request_redraw();
        }
    }

    // =========================================================================
    // Scroll and viewport
    // =========================================================================

    /// Scrolls so the primary caret is on screen, mirroring the logic the web
    /// face kept when the compositor was extracted: the caret's document-space
    /// Y comes from the compositor, wrap-aware when the caret's line has not
    /// changed; the scroll decision stays here, on the face that owns
    /// `scroll_y`.
    ///
    /// While the search panel is up, the rows it covers at the bottom do not
    /// count as visible — the terminal face shrinks its viewport for the same
    /// reason, so a match is never scrolled to a row the panel that found it
    /// is covering.
    fn ensure_caret_visible(&mut self) {
        let search_open = self.search_open;
        let Some(shell) = &self.shell else {
            return;
        };
        let line_height = shell.compositor.line_height();
        let bottom_inset = if search_open {
            shell.overlay.panel_height(2)
        } else {
            0.0
        };
        let viewport_height = (u32_to_f32(shell.surface.height()) - bottom_inset).max(line_height);
        let caret_y = shell
            .compositor
            .cursor_anchor_y(&self.editor, self.editor.fold_state());

        if caret_y < self.scroll_y + SCROLL_PADDING {
            // Caret above the viewport: bring its line to the top.
            self.scroll_y = (caret_y - SCROLL_PADDING).max(0.0);
        } else if caret_y + line_height > self.scroll_y + viewport_height - SCROLL_PADDING {
            // Caret below the viewport: bring its line to the bottom.
            self.scroll_y = caret_y + line_height - viewport_height + SCROLL_PADDING;
        }
        self.clamp_scroll();
    }

    /// Clamps the scroll offset to what the document can honestly show —
    /// the same clamp the web face applies on every scroll write.
    fn clamp_scroll(&mut self) {
        let Some(shell) = &self.shell else {
            return;
        };
        let max = shell.compositor.max_scroll_y(
            &self.editor,
            self.editor.fold_state(),
            u32_to_f32(shell.surface.height()),
        );
        self.scroll_y = self.scroll_y.clamp(0.0, max);
    }

    /// The document lines (half-open) the next frame will show — the window
    /// the highlight cache scopes its span derive to.
    ///
    /// Fold-aware through the kernel's visual-line mapping, and derived from
    /// the same inputs that drive the compose: the face-owned `scroll_y`, the
    /// surface height and the compositor's line height. Wrap-unaware by
    /// construction — the wrap map belongs to the frame being composed — so
    /// with wrapped lines above the viewport the estimate names later lines
    /// than are truly first on screen; the cache's overscan margin
    /// (`highlight::OVERSCAN_LINES` each way) is what absorbs that slack,
    /// alongside the row of partial-line slack added here. `None` without a
    /// shell: a headless session has no viewport to scope to.
    fn viewport_window(&self) -> Option<std::ops::Range<usize>> {
        let shell = self.shell.as_ref()?;
        let line_height = shell.compositor.line_height();
        if line_height <= 0.0 {
            return None;
        }
        let first_visual = pixel_to_index(self.scroll_y / line_height);
        let rows =
            pixel_to_index(u32_to_f32(shell.surface.height()) / line_height).saturating_add(2);
        let first_line = self.editor.visual_to_document_line(first_visual);
        let last_line = self
            .editor
            .visual_to_document_line(first_visual.saturating_add(rows));
        Some(first_line..last_line.saturating_add(1))
    }

    /// Pushes the surface's geometry into the kernel's viewport.
    ///
    /// Painting never reads that viewport — the compositor is driven by
    /// `scroll_y` — but `cursor.pageUp`/`pageDown` hop by its
    /// `visible_lines`, so it must describe the real window. The `state_mut`
    /// this costs discards sticky columns and any pending chord; it is paid
    /// only on resize and scale change, exactly where the terminal face pays
    /// it.
    fn sync_kernel_viewport(&mut self) {
        let Some(shell) = &self.shell else {
            return;
        };
        let line_height = shell.compositor.line_height();
        let width = u32_to_f32(shell.surface.width());
        let height = u32_to_f32(shell.surface.height());
        let state = self.editor.state_mut();
        state.viewport.line_height = line_height;
        state.viewport.resize(width, height);
    }

    /// Handles a new surface size, in physical pixels.
    fn resized(&mut self, width: u32, height: u32) {
        // Zero means minimized, not small; keep the last honest size.
        if width == 0 || height == 0 {
            return;
        }
        if let Some(shell) = &mut self.shell {
            shell.surface.resize(width, height);
            shell
                .compositor
                .resize(shell.surface.queue(), width, height);
            shell.overlay.resize(shell.surface.queue(), width, height);
        }
        self.sync_kernel_viewport();
        self.clamp_scroll();
        if let Some(shell) = &self.shell {
            shell.window.request_redraw();
        }
    }

    /// Handles a scale factor change: refits the font, physical pixels
    /// throughout.
    ///
    /// The font is *reloaded*, not just resized: `set_font_size` deliberately
    /// does not remeasure character width (before any font is loaded there is
    /// nothing honest to measure), and `load_font` is the only remeasuring
    /// path — for the overlay's painter as for the compositor. The matching
    /// `Resized` event carries the new physical dimensions and follows
    /// separately.
    fn rescaled(&mut self, scale_factor: f64) {
        if let Some(shell) = &mut self.shell {
            let font_size = BASE_FONT_SIZE * scale_to_f32(scale_factor);
            shell.compositor.set_font_size(font_size);
            shell.compositor.load_font(FONT.to_vec());
            shell.overlay.set_font(font_size, FONT.to_vec());
            shell.overlay.set_scale(scale_to_f32(scale_factor));
        }
        self.sync_kernel_viewport();
        if let Some(shell) = &self.shell {
            shell.window.request_redraw();
        }
    }

    // =========================================================================
    // Painting
    // =========================================================================

    /// What the prompt strip should show this frame, if anything.
    ///
    /// The open prompt wins over a message — it is the question being asked —
    /// and a frame with neither skips the overlay pass entirely.
    fn strip_content(&self) -> Option<StripContent> {
        if let Some(prompt) = &self.prompt {
            return Some(StripContent {
                text: prompt.line(),
                caret_column: prompt.caret_column(),
                is_error: false,
            });
        }
        self.message.as_ref().map(|message| StripContent {
            text: message.text().to_owned(),
            caret_column: None,
            is_error: message.is_error(),
        })
    }

    /// Composes and presents one frame: the document through the compositor,
    /// then — when a prompt, a message or any panel is up — the overlays as a
    /// second pass on the same texture view. See [`crate::overlay`] for the
    /// pass structure.
    ///
    /// The panels are composed here, against the window's current grid, so
    /// every frame shows the selection window and the match counts as they
    /// are *now* — the undo tree's `*` moves the frame after a jump.
    ///
    /// A failed frame is reported on standard error and the session carries
    /// on — the web face does the same. The next event requests the next
    /// attempt; a fault that persists keeps naming itself rather than
    /// silently freezing the window.
    fn redraw(&mut self) {
        // Costs a generation comparison when nothing changed; the spans are
        // rebuilt — over the frame's viewport plus overscan, never the whole
        // document — only when the kernel actually reparsed or the viewport
        // escaped the covered window.
        match self.viewport_window() {
            Some(window) => self.syntax.refresh_windowed(&self.editor, window),
            None => self.syntax.refresh(&self.editor),
        }
        let strip = self.strip_content();
        let panels = self.panel_contents();
        let editor = &self.editor;
        let scroll_y = self.scroll_y;
        let Some(shell) = &mut self.shell else {
            return;
        };
        let Shell {
            surface,
            compositor,
            overlay,
            ..
        } = shell;

        let width = surface.width();
        let height = surface.height();
        let fold_state = editor.fold_state();
        let theme = &editor.state().theme;
        let mut highlights = self.syntax.resolver(&theme.syntax);
        let panel_refs: Vec<&PanelContent> = panels.iter().collect();

        let outcome = surface.render_frame(|view, device, queue| {
            compositor.compose(
                editor,
                fold_state,
                scroll_y,
                &mut highlights,
                FrameTarget {
                    view,
                    device,
                    queue,
                    width,
                    height,
                },
            )?;
            if strip.is_some() || !panel_refs.is_empty() {
                overlay.paint(
                    FrameTarget {
                        view,
                        device,
                        queue,
                        width,
                        height,
                    },
                    strip.as_ref(),
                    &panel_refs,
                    theme,
                )?;
            }
            Ok(())
        });

        match outcome {
            Ok(()) => {
                // The frame is submitted and presented; the pending keydown,
                // if any, is answered. A failed frame keeps it pending — the
                // keystroke's effect is still unpresented, so the eventual
                // sample honestly spans the failed attempt.
                if self.latency.is_armed()
                    && let Some(milliseconds) = self.latency.frame_presented(Instant::now())
                {
                    let _ = writeln!(
                        io::stderr(),
                        "latency: keydown->present {milliseconds:.2}ms"
                    );
                }
            },
            Err(error) => {
                let _ = writeln!(io::stderr(), "iridium-desktop: frame failed: {error}");
            },
        }
    }

    /// Composes every open panel for this frame, bottom-most first so the
    /// palette — the most modal thing on screen — paints on top.
    ///
    /// A window too small for an honest panel composes none; the panels' keys
    /// keep working regardless, so `Escape` is never trapped behind a resize.
    fn panel_contents(&mut self) -> Vec<PanelContent> {
        let Some(shell) = &mut self.shell else {
            return Vec::new();
        };
        let Some(fit) = shell
            .overlay
            .panel_fit(shell.surface.width(), shell.surface.height())
        else {
            return Vec::new();
        };
        let theme = &self.editor.state().theme;
        let mut panels = Vec::new();
        if self.search_open {
            panels.push(self.search.content(&self.editor, theme, fit));
        }
        if self.history_open {
            panels.push(self.history.content(&self.editor, theme, fit));
        }
        if self.palette_open {
            panels.push(self.palette.content(&self.editor, &self.mru, theme, fit));
        }
        // Last, and so on top and last in the painter's placement record —
        // which is how `painted_menu` finds it again for the pointer.
        if let Some(menu) = &self.menu {
            panels.push(menu.content(theme, fit));
        }
        panels
    }
}

/// The window title for a document's name and dirty state, in the terminal
/// face's statusline vocabulary: `[No Name]` for an unnamed buffer, ` [+]`
/// for unsaved changes.
fn title_for(name: Option<&str>, dirty: bool) -> String {
    let mut title = String::from(name.unwrap_or("[No Name]"));
    if dirty {
        title.push_str(" [+]");
    }
    title.push_str(" — ");
    title.push_str(TITLE);
    title
}

/// Whether a key event is the paste chord, in either its mac or its portable
/// spelling.
///
/// Consulted only while a prompt is open, where the keymap cannot run: the
/// prompt is modal, and this is the one chord it forwards to the field.
const fn is_paste_chord(event: &KeyEvent) -> bool {
    matches!(event.key, KeyCode::Char('v' | 'V'))
        && (event.modifiers.ctrl || event.modifiers.meta)
        && !event.modifiers.alt
}

/// The language a file name implies, if the kernel knows one for it.
///
/// A name with no extension, or one no grammar claims, is not an error: the
/// document is then edited without highlighting and without folds, which is
/// what a plain text file is.
fn language_of(path: &Path) -> Option<Language> {
    Language::from_extension(path.extension().and_then(std::ffi::OsStr::to_str)?)
}

impl ApplicationHandler for DesktopApp {
    /// Creates the window and everything behind it, once.
    ///
    /// winit delivers `resumed` before any window event and may deliver it
    /// again on some platforms; the shell is created on the first and kept on
    /// the rest. A failure here is recorded and the loop ended — there is no
    /// session without a window, and no return channel but the field.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.shell.is_some() {
            return;
        }
        match Shell::open(event_loop, self.editor.state().theme.clone()) {
            Ok(shell) => {
                shell.window.request_redraw();
                self.shell = Some(shell);
                self.sync_kernel_viewport();
                self.refresh_title();
            },
            Err(message) => {
                self.failure = Some(message);
                event_loop.exit();
            },
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // One window per session; anything else is not ours.
        if self
            .shell
            .as_ref()
            .is_none_or(|shell| shell.window.id() != window_id)
        {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                if self.request_quit() == Flow::Exit {
                    event_loop.exit();
                }
            },
            WindowEvent::Resized(size) => self.resized(size.width, size.height),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => self.rescaled(scale_factor),
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                // The latency clock starts at receipt, before translation —
                // the user's finger does not care where the time went. When
                // the flag is off this is the apparatus's whole cost: one
                // branch, no clock read.
                let received = if self.latency.is_armed() {
                    Some(Instant::now())
                } else {
                    None
                };
                // Releases are not presses, and synthetic presses are focus
                // bookkeeping, not typing.
                if event.state == ElementState::Pressed
                    && !is_synthetic
                    && let Some(key) =
                        keys::translate(&event.logical_key, event.repeat, self.modifiers)
                {
                    // Recorded only for a dispatched key: `press` requests a
                    // redraw for every one, so a pending timestamp always has
                    // a frame coming. A dropped key's timestamp dies here —
                    // see `crate::latency` for the whole policy.
                    if let Some(at) = received {
                        self.latency.key_dispatched(at);
                    }
                    if self.press(&key) == Flow::Exit {
                        event_loop.exit();
                    }
                }
            },
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer_moved(pixel_from_f64(position.x), pixel_from_f64(position.y));
            },
            WindowEvent::MouseInput { state, button, .. } => {
                match (button, state) {
                    (winit::event::MouseButton::Left, ElementState::Pressed) => {
                        if self.pointer_pressed() == Flow::Exit {
                            event_loop.exit();
                        }
                    },
                    (winit::event::MouseButton::Left, ElementState::Released) => {
                        self.pointer_released();
                    },
                    // The secondary button opens the context menu, and only
                    // the secondary button does (D-3): Ctrl+click keeps the
                    // add-cursor meaning it has shipped with.
                    (winit::event::MouseButton::Right, ElementState::Pressed) => {
                        self.secondary_pressed();
                    },
                    _ => {},
                }
            },
            WindowEvent::MouseWheel { delta, .. } => self.wheel(&delta),
            WindowEvent::Focused(false) => self.blurred(),
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {},
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use iridium_editor::{KeyCode, KeyEvent, Modifiers};
    use iridium_file::test_support::TempDir;

    use super::{DesktopApp, Flow, Options, title_for};
    use crate::prompt::Prompt;

    /// A key press under the given modifiers.
    fn chord(key: KeyCode, modifiers: Modifiers) -> KeyEvent {
        KeyEvent {
            key,
            modifiers,
            is_repeat: false,
        }
    }

    /// A plain key press.
    fn press(key: KeyCode) -> KeyEvent {
        chord(key, Modifiers::none())
    }

    /// The `Ctrl+S` save chord.
    fn ctrl_s() -> KeyEvent {
        chord(KeyCode::Char('s'), Modifiers::ctrl())
    }

    /// Opens a session on a file holding `text`.
    fn open(directory: &TempDir, name: &str, text: &str) -> (DesktopApp, PathBuf) {
        let path = directory.path().join(name);
        std::fs::write(&path, text).expect("the fixture file was written");
        let app = DesktopApp::new(Options {
            path: Some(path.clone()),
        })
        .expect("the session opened");
        (app, path)
    }

    /// Types text into the app one plain key press at a time.
    fn type_into(app: &mut DesktopApp, text: &str) {
        for character in text.chars() {
            assert_eq!(app.press(&press(KeyCode::Char(character))), Flow::Running);
        }
    }

    #[test]
    fn the_save_chord_writes_the_file_and_cleans_the_buffer() {
        let directory = TempDir::new("desktop-save");
        let (mut app, path) = open(&directory, "a.txt", "one");
        type_into(&mut app, "x");
        assert!(app.is_dirty());

        assert_eq!(app.press(&ctrl_s()), Flow::Running);
        assert!(!app.is_dirty(), "a saved buffer is clean");
        assert!(
            std::fs::read_to_string(&path)
                .expect("the file reads back")
                .contains('x'),
            "the edit reached the disk"
        );
        let message = app.message.as_ref().expect("the save reported itself");
        assert!(message.text().contains("wrote"), "{}", message.text());
        assert!(!message.is_error());
    }

    #[test]
    fn the_mac_save_chord_reaches_the_same_verb() {
        let directory = TempDir::new("desktop-save-meta");
        let (mut app, path) = open(&directory, "a.txt", "one");
        type_into(&mut app, "y");

        let meta = Modifiers {
            meta: true,
            ..Modifiers::none()
        };
        assert_eq!(app.press(&chord(KeyCode::Char('s'), meta)), Flow::Running);
        assert!(!app.is_dirty());
        assert!(
            std::fs::read_to_string(&path)
                .expect("the file reads back")
                .contains('y')
        );
    }

    #[test]
    fn a_stale_file_refuses_the_save_and_the_forced_chord_overwrites() {
        let directory = TempDir::new("desktop-stale");
        let (mut app, path) = open(&directory, "a.txt", "mine");
        type_into(&mut app, "!");

        // Something else rewrites the file underneath the session.
        std::fs::write(&path, "theirs").expect("the external write landed");

        assert_eq!(app.press(&ctrl_s()), Flow::Running);
        let message = app.message.as_ref().expect("the refusal was reported");
        assert!(message.is_error());
        assert!(
            message.text().contains("changed on disk"),
            "{}",
            message.text()
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("the file reads back"),
            "theirs",
            "the refused save wrote anyway"
        );

        let force = Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::none()
        };
        assert_eq!(app.press(&chord(KeyCode::Char('s'), force)), Flow::Running);
        assert!(
            std::fs::read_to_string(&path)
                .expect("the file reads back")
                .contains('!'),
            "the forced save overwrote"
        );
        assert!(!app.is_dirty());
    }

    #[test]
    fn an_unnamed_buffer_asks_for_a_name_and_saves_under_it() {
        let directory = TempDir::new("desktop-save-as");
        let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
        type_into(&mut app, "hi");

        assert_eq!(app.press(&ctrl_s()), Flow::Running);
        assert!(
            matches!(app.prompt, Some(Prompt::SaveAs(_))),
            "an unnamed buffer asks for a name"
        );

        // The prompt is modal: the save chord must not save mid-question.
        assert_eq!(app.press(&ctrl_s()), Flow::Running);
        assert!(app.prompt.is_some(), "the chord was swallowed");

        let target = directory.path().join("notes.txt");
        type_into(&mut app, &target.to_string_lossy());
        assert_eq!(app.press(&press(KeyCode::Enter)), Flow::Running);

        assert!(app.prompt.is_none());
        assert_eq!(
            std::fs::read_to_string(&target).expect("the new file reads back"),
            "hi"
        );
        assert!(!app.is_dirty());
        assert_eq!(
            app.file.as_ref().map(super::TextFile::display_name),
            Some("notes.txt".to_owned())
        );
    }

    #[test]
    fn closing_over_unsaved_changes_asks_first() {
        let directory = TempDir::new("desktop-quit");
        let (mut app, _path) = open(&directory, "a.txt", "one");
        type_into(&mut app, "x");

        assert_eq!(app.request_quit(), Flow::Running);
        assert!(app.prompt.is_some(), "a dirty session asks before quitting");

        // "No" keeps the session.
        assert_eq!(app.press(&press(KeyCode::Char('n'))), Flow::Running);
        assert!(app.prompt.is_none());

        // Asked again, "yes" leaves.
        assert_eq!(app.request_quit(), Flow::Running);
        assert_eq!(app.press(&press(KeyCode::Char('y'))), Flow::Exit);
    }

    #[test]
    fn closing_a_clean_session_just_leaves() {
        let directory = TempDir::new("desktop-quit-clean");
        let (mut app, _path) = open(&directory, "a.txt", "one");
        assert_eq!(app.request_quit(), Flow::Exit);
        assert!(app.prompt.is_none());
    }

    #[test]
    fn a_read_only_document_refuses_to_save() {
        let directory = TempDir::new("desktop-read-only");
        let (mut app, path) = open(&directory, "a.txt", "one");
        app.editor.state_mut().read_only = true;

        assert_eq!(app.press(&ctrl_s()), Flow::Running);
        let message = app.message.as_ref().expect("the refusal was reported");
        assert!(message.is_error());
        assert!(message.text().contains("read-only"), "{}", message.text());
        assert_eq!(
            std::fs::read_to_string(&path).expect("the file reads back"),
            "one"
        );
    }

    /// A `Ctrl+Alt` chord.
    fn ctrl_alt(key: KeyCode) -> KeyEvent {
        chord(
            key,
            Modifiers {
                ctrl: true,
                alt: true,
                ..Modifiers::none()
            },
        )
    }

    /// A bare ⌘ chord.
    fn meta(key: KeyCode) -> KeyEvent {
        chord(
            key,
            Modifiers {
                meta: true,
                ..Modifiers::none()
            },
        )
    }

    #[test]
    fn the_palette_is_modal_and_escape_gives_the_document_back() {
        let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
        assert_eq!(
            app.press(&chord(KeyCode::Char('k'), Modifiers::ctrl())),
            Flow::Running
        );
        assert!(app.palette_open, "Ctrl+K opens the palette");

        // Modal: typing goes to the query, not the document.
        assert_eq!(app.press(&press(KeyCode::Char('x'))), Flow::Running);
        assert_eq!(app.editor.content(), "", "the keystroke fed the query");
        assert_eq!(app.palette.query(), "x");

        assert_eq!(app.press(&press(KeyCode::Escape)), Flow::Running);
        assert!(!app.palette_open);
        assert_eq!(app.press(&press(KeyCode::Char('y'))), Flow::Running);
        assert_eq!(app.editor.content(), "y", "the document is back");
    }

    #[test]
    fn the_mac_spellings_reach_the_same_overlays() {
        let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
        assert_eq!(app.press(&meta(KeyCode::Char('k'))), Flow::Running);
        assert!(app.palette_open, "⌘K opens the palette");
        assert_eq!(app.press(&meta(KeyCode::Char('k'))), Flow::Running);
        assert!(!app.palette_open, "⌘K closes it again");

        let meta_alt = Modifiers {
            meta: true,
            alt: true,
            ..Modifiers::none()
        };
        assert_eq!(
            app.press(&chord(KeyCode::Char('h'), meta_alt)),
            Flow::Running
        );
        assert!(app.history_open, "⌘⌥H toggles the undo tree");

        let mut second = DesktopApp::new(Options { path: None }).expect("a session opened");
        assert_eq!(second.press(&meta(KeyCode::Char('f'))), Flow::Running);
        assert!(second.search_open, "⌘F opens the search panel");
    }

    #[test]
    fn a_kernel_command_runs_from_the_palette_and_is_remembered() {
        let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
        type_into(&mut app, "hello");
        assert_eq!(
            app.press(&chord(KeyCode::Char('k'), Modifiers::ctrl())),
            Flow::Running
        );
        type_into(&mut app, "select all");
        assert_eq!(app.press(&press(KeyCode::Enter)), Flow::Running);
        assert!(!app.palette_open, "running a command closes the palette");

        // The whole document is selected, so typing replaces it.
        type_into(&mut app, "z");
        assert_eq!(app.editor.content(), "z");
        assert!(
            !app.mru.is_empty(),
            "the dispatched command was recorded for recency"
        );
    }

    #[test]
    fn a_face_command_runs_from_the_palette() {
        let directory = TempDir::new("desktop-palette-save");
        let (mut app, path) = open(&directory, "a.txt", "one");
        type_into(&mut app, "x");
        assert_eq!(
            app.press(&chord(KeyCode::Char('k'), Modifiers::ctrl())),
            Flow::Running
        );
        type_into(&mut app, "save");
        assert_eq!(app.press(&press(KeyCode::Enter)), Flow::Running);
        assert!(
            std::fs::read_to_string(&path)
                .expect("the file reads back")
                .contains('x'),
            "the palette's Save reached the same save path as the chord"
        );
        assert!(!app.is_dirty());
    }

    #[test]
    fn reopening_the_palette_clears_the_query() {
        let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
        assert_eq!(app.press(&meta(KeyCode::Char('k'))), Flow::Running);
        type_into(&mut app, "fold");
        assert_eq!(app.palette.query(), "fold");
        assert_eq!(app.press(&press(KeyCode::Escape)), Flow::Running);
        assert_eq!(app.press(&meta(KeyCode::Char('k'))), Flow::Running);
        assert_eq!(app.palette.query(), "", "a palette opens aimed at nothing");
    }

    #[test]
    fn a_right_press_opens_the_context_menu_at_the_pointer() {
        let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
        type_into(&mut app, "hello");
        app.pointer.set_position(120.0, 240.0);
        app.secondary_pressed();

        let menu = app.menu.as_ref().expect("the right button opens the menu");
        assert_eq!(
            menu.anchor(),
            (120.0, 240.0),
            "the menu hangs from the click"
        );
        assert_eq!(
            app.editor.content(),
            "hello",
            "opening the menu edits nothing"
        );
    }

    #[test]
    fn a_right_press_while_a_modal_panel_is_open_opens_no_menu() {
        // Modal means modal: the press is spent on the panel that is up.
        let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
        assert_eq!(app.press(&meta(KeyCode::Char('k'))), Flow::Running);
        app.pointer.set_position(120.0, 240.0);
        app.secondary_pressed();
        assert!(app.menu.is_none(), "no menu opens over the palette");
        assert!(app.palette_open, "and the palette stays up");
    }

    #[test]
    fn the_context_menu_is_modal_and_escape_gives_the_document_back() {
        let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
        app.pointer.set_position(10.0, 10.0);
        app.secondary_pressed();
        assert!(app.menu.is_some());

        assert_eq!(app.press(&press(KeyCode::Char('x'))), Flow::Running);
        assert_eq!(
            app.editor.content(),
            "",
            "the keystroke was swallowed by the menu"
        );

        assert_eq!(app.press(&press(KeyCode::Escape)), Flow::Running);
        assert!(app.menu.is_none(), "Escape closes the menu");
        assert_eq!(app.press(&press(KeyCode::Char('y'))), Flow::Running);
        assert_eq!(app.editor.content(), "y", "the document is back");
    }

    #[test]
    fn enter_runs_the_highlighted_menu_verb_through_the_kernel() {
        let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
        type_into(&mut app, "hello");
        app.pointer.set_position(10.0, 10.0);
        app.secondary_pressed();

        // Down to Select All: Cut, Copy, Paste, ——, Select All.
        for _ in 0..3 {
            assert_eq!(app.press(&press(KeyCode::Down)), Flow::Running);
        }
        assert_eq!(app.press(&press(KeyCode::Enter)), Flow::Running);
        assert!(app.menu.is_none(), "running a verb closes the menu");
        assert!(
            !app.mru.is_empty(),
            "a menu run trains the same recency list a palette run does"
        );

        // The whole document is selected, so typing replaces it.
        type_into(&mut app, "z");
        assert_eq!(app.editor.content(), "z");
    }

    #[test]
    fn the_menus_palette_row_opens_the_palette() {
        let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
        app.pointer.set_position(10.0, 10.0);
        app.secondary_pressed();
        assert_eq!(app.press(&press(KeyCode::End)), Flow::Running);
        assert_eq!(app.press(&press(KeyCode::Enter)), Flow::Running);
        assert!(app.menu.is_none());
        assert!(
            app.palette_open,
            "the host command behind the row reached `dispatch_host_command`"
        );
    }

    #[test]
    fn losing_focus_closes_the_menu() {
        let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
        app.pointer.set_position(10.0, 10.0);
        app.secondary_pressed();
        assert!(app.menu.is_some());
        app.blurred();
        assert!(
            app.menu.is_none(),
            "a menu does not outlive the window's focus"
        );
    }

    #[test]
    fn a_click_outside_the_open_menu_dismisses_it_and_never_reaches_the_document() {
        let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
        type_into(&mut app, "hello");
        app.pointer.set_position(10.0, 10.0);
        app.secondary_pressed();
        assert!(app.menu.is_some());

        app.pointer.set_position(900.0, 900.0);
        assert_eq!(app.pointer_pressed(), Flow::Running);
        assert!(app.menu.is_none(), "the click outside dismissed the menu");
        assert_eq!(
            app.editor.content(),
            "hello",
            "the dismissing click never reached the document"
        );
    }

    #[test]
    fn a_click_outside_the_palette_dismisses_it_and_never_reaches_the_document() {
        // D-4: a modal panel swallows the click that dismisses it. Before the
        // context-menu slice the press fell straight through to the document
        // while the palette stayed on screen.
        let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
        type_into(&mut app, "hello");
        assert_eq!(app.press(&meta(KeyCode::Char('k'))), Flow::Running);
        assert!(app.palette_open, "⌘K opens the palette");

        app.pointer.set_position(12.0, 34.0);
        app.pointer_pressed();

        assert!(
            !app.palette_open,
            "a click outside the palette dismisses it"
        );
        assert_eq!(
            app.editor.content(),
            "hello",
            "the dismissing click never reached the document"
        );
    }

    #[test]
    fn a_click_outside_the_undo_tree_dismisses_it_too() {
        let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
        assert_eq!(app.press(&ctrl_alt(KeyCode::Char('h'))), Flow::Running);
        assert!(app.history_open, "Ctrl+Alt+H opens the undo tree");

        app.pointer.set_position(12.0, 34.0);
        app.pointer_pressed();

        assert!(!app.history_open, "a click outside the panel dismisses it");
    }

    #[test]
    fn a_click_while_the_search_panel_is_open_still_reaches_the_document() {
        // The search panel is deliberately *not* modal — keys it does not bind
        // stay the host's — so D-4 does not touch it: a click while it is open
        // belongs to the document, and the panel stays up.
        let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
        assert_eq!(
            app.press(&chord(KeyCode::Char('f'), Modifiers::ctrl())),
            Flow::Running
        );
        assert!(app.search_open);

        app.pointer.set_position(12.0, 34.0);
        app.pointer_pressed();

        assert!(app.search_open, "a click does not dismiss the search panel");
    }

    #[test]
    fn the_history_panel_toggles_and_is_modal() {
        let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
        type_into(&mut app, "a");
        assert_eq!(app.press(&ctrl_alt(KeyCode::Char('h'))), Flow::Running);
        assert!(app.history_open, "Ctrl+Alt+H opens the undo tree");

        // Modal: typing must not reach the document while history is open.
        assert_eq!(app.press(&press(KeyCode::Char('x'))), Flow::Running);
        assert_eq!(app.editor.content(), "a");

        assert_eq!(app.press(&ctrl_alt(KeyCode::Char('h'))), Flow::Running);
        assert!(!app.history_open, "the toggle chord closes it");
    }

    #[test]
    fn jumping_from_the_history_panel_walks_states_and_keeps_the_panel_open() {
        let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
        // One node per edit, so the rows are predictable.
        app.editor.state_mut().history.set_group_timeout_ms(0);
        type_into(&mut app, "a");
        type_into(&mut app, "b");
        assert_eq!(app.editor.content(), "ab");

        assert_eq!(app.press(&ctrl_alt(KeyCode::Char('h'))), Flow::Running);
        assert_eq!(app.press(&press(KeyCode::Up)), Flow::Running);
        assert_eq!(app.press(&press(KeyCode::Enter)), Flow::Running);
        assert_eq!(app.editor.content(), "a", "one row up is one edit back");
        assert!(
            app.history_open,
            "the panel stays open so the * can be watched moving"
        );
        assert!(app.message.is_none(), "a good jump reports nothing");
    }

    #[test]
    fn the_search_panel_opens_finds_and_closes_without_disturbing_the_document() {
        let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
        type_into(&mut app, "alpha beta alpha");
        assert_eq!(
            app.press(&chord(KeyCode::Char('f'), Modifiers::ctrl())),
            Flow::Running
        );
        assert!(app.search_open, "Ctrl+F opens the search panel");

        type_into(&mut app, "alpha");
        assert_eq!(app.editor.search_match_count(), 2, "typing searches live");
        assert_eq!(
            app.editor.content(),
            "alpha beta alpha",
            "the query went to the field, not the document"
        );

        assert_eq!(app.press(&press(KeyCode::Escape)), Flow::Running);
        assert!(!app.search_open);
        assert_eq!(
            app.editor.search_match_count(),
            0,
            "closing the panel closes the kernel's search"
        );
        assert_eq!(app.editor.content(), "alpha beta alpha");
    }

    #[test]
    fn a_save_chord_still_works_while_the_search_panel_is_open() {
        let directory = TempDir::new("desktop-search-save");
        let (mut app, path) = open(&directory, "a.txt", "one");
        type_into(&mut app, "!");
        assert_eq!(
            app.press(&chord(KeyCode::Char('f'), Modifiers::ctrl())),
            Flow::Running
        );
        assert_eq!(app.press(&ctrl_s()), Flow::Running);
        assert!(
            std::fs::read_to_string(&path)
                .expect("the file reads back")
                .contains('!'),
            "the panel left the save chord to the host"
        );
        assert!(app.search_open, "saving did not close the search");
    }

    #[test]
    fn the_language_is_read_from_the_extension_as_every_face_reads_it() {
        use std::path::Path;

        use iridium_editor::Language;

        use super::language_of;

        assert_eq!(language_of(Path::new("main.rs")), Some(Language::Rust));
        assert_eq!(language_of(Path::new("a/b/mod.RS")), Some(Language::Rust));
        assert_eq!(
            language_of(Path::new("index.ts")),
            Some(Language::TypeScript)
        );
        assert_eq!(language_of(Path::new("notes.md")), Some(Language::Markdown));
        assert_eq!(
            language_of(Path::new("unclaimed.xyz")),
            None,
            "an extension no grammar claims is plain text"
        );
        assert_eq!(
            language_of(Path::new("Makefile")),
            None,
            "a name without an extension is plain text"
        );
    }

    #[test]
    fn opening_a_rust_file_primes_the_highlight_cache() {
        let directory = TempDir::new("desktop-syntax");
        let (mut app, _path) = open(&directory, "a.rs", "fn main() {}\n");
        assert_eq!(
            app.editor.language(),
            Some(iridium_editor::Language::Rust),
            "the session set the language from the name"
        );
        app.syntax.refresh(&app.editor);
        assert_eq!(
            app.syntax.rebuilds(),
            1,
            "the parsed document yields spans on the first refresh"
        );
        app.syntax.refresh(&app.editor);
        assert_eq!(app.syntax.rebuilds(), 1, "an idle frame rebuilds nothing");

        type_into(&mut app, "x");
        app.syntax.refresh(&app.editor);
        assert_eq!(
            app.syntax.rebuilds(),
            2,
            "a keystroke's reparse rebuilds once"
        );
    }

    #[test]
    fn the_title_names_the_document_and_its_dirty_state() {
        assert_eq!(title_for(None, false), "[No Name] — iridium");
        assert_eq!(title_for(None, true), "[No Name] [+] — iridium");
        assert_eq!(title_for(Some("a.rs"), false), "a.rs — iridium");
        assert_eq!(title_for(Some("a.rs"), true), "a.rs [+] — iridium");
    }

    #[test]
    fn a_dirty_buffer_becomes_clean_again_when_undone_to_its_saved_state() {
        // Content-based dirtiness, not a revision counter: undoing the only
        // edit makes the document what the disk holds, so it is clean.
        let directory = TempDir::new("desktop-dirty-undo");
        let (mut app, _path) = open(&directory, "a.txt", "one");
        assert!(!app.is_dirty());
        type_into(&mut app, "x");
        assert!(app.is_dirty());
        assert_eq!(
            app.press(&chord(KeyCode::Char('z'), Modifiers::ctrl())),
            Flow::Running
        );
        assert!(!app.is_dirty(), "an undone edit leaves a clean buffer");
    }
}
