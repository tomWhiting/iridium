//! The whole desktop session as an event-driven state machine: winit events
//! in, composed frames out.
//!
//! [`DesktopApp`] is this face's counterpart of the terminal face's `App`:
//! the kernel owns every verb, and a key reaches it through
//! [`Editor::handle_key`] after [`crate::keys`] translates it. What this face
//! keeps is exactly what the compositor's seam leaves to a face — the window
//! and surface, the scroll offset, and (eventually) a syntax resolver; the
//! built-in keyword highlighter stands in until this face runs tree-sitter.
//!
//! # Rendering is event-driven
//!
//! A frame is composed only on `RedrawRequested`, and a redraw is requested
//! only after input, resize or scale change. There is no animation loop, so
//! an idle editor costs nothing — the same discipline as the terminal face,
//! with the one visible consequence that the caret does not blink while the
//! keyboard is idle. Animating it means waking on a timer, which is a
//! deliberate cost this scaffold does not spend yet.
//!
//! # What the kernel reports comes back honestly
//!
//! The kernel consumes a bound key and reports what it could not do itself.
//! Host commands land in this face with nothing behind them yet — there is no
//! save, no prompt line, no palette painting in this slice — so instead of
//! dropping them, the command's id is written into the window title until the
//! next keystroke. A key that visibly names what it could not do is the
//! scaffold being honest; a key that silently does nothing is a dead key.
//!
//! # The clipboard is this process's
//!
//! Copy, cut and paste round-trip through a `String` on this struct, exactly
//! as the terminal face's do. The system clipboard (`arboard`, per the plan)
//! is a later slice; until then a copy here pastes here and nowhere else,
//! stated rather than hidden.
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
use std::path::PathBuf;
use std::sync::Arc;

use iridium_editor::input::SearchAction;
use iridium_editor::render::{FrameCompositor, FrameTarget, HighlightContext, HighlightSource};
use iridium_editor::theme::Color;
use iridium_editor::{ClipboardOperation, Editor, EditorKeyResult, KeyEvent, Language};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

use crate::keys;
use crate::surface::NativeSurface;
use crate::units::{scale_to_f32, u32_to_f32};

/// The window title, and the base every notice is appended to.
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
    /// The file named on the command line could not be read.
    #[error("cannot open `{}`: {error}", path.display())]
    File {
        /// The path that would not open.
        path: PathBuf,
        /// The error the read reported.
        error: io::Error,
    },
}

/// What the command line asked for.
#[derive(Debug, Default)]
pub struct Options {
    /// The file to edit, if the invocation named one.
    pub path: Option<PathBuf>,
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
}

impl Shell {
    /// Opens the window and brings up the GPU behind it.
    ///
    /// The font size is set *before* the font is loaded: `load_font` is the
    /// compositor's only path that measures character width, and it measures
    /// at the size in force when it runs.
    fn open(event_loop: &ActiveEventLoop) -> Result<Self, String> {
        let attributes = Window::default_attributes().with_title(TITLE);
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

        compositor.set_font_size(BASE_FONT_SIZE * scale_to_f32(window.scale_factor()));
        compositor.load_font(FONT.to_vec());

        Ok(Self {
            window,
            surface,
            compositor,
        })
    }
}

/// The compositor's highlight seam, answered with nothing.
///
/// Returning `None` every frame selects the compositor's built-in keyword
/// highlighter — the path a face without tree-sitter gets for free. Running
/// real tree-sitter spans through this seam, as the web face does from its
/// worker, is a later slice.
struct BuiltinHighlights;

impl HighlightSource for BuiltinHighlights {
    fn resolve<'a>(&mut self, _context: &HighlightContext<'a>) -> Option<Vec<(&'a str, Color)>> {
        None
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
    /// The modifier state winit last reported, applied to every key press.
    modifiers: ModifiersState,
    /// This process's clipboard. See the module documentation.
    clipboard: String,
    /// The notice currently shown in the window title, if any.
    notice: Option<String>,
    /// What killed the session from inside the event loop, if anything;
    /// [`crate::run`] reports it after the loop returns, because winit's
    /// callbacks have nowhere to return an error to.
    failure: Option<String>,
}

impl std::fmt::Debug for DesktopApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopApp")
            .field("scroll_y", &self.scroll_y)
            .field("notice", &self.notice)
            .field("failure", &self.failure)
            .finish_non_exhaustive()
    }
}

impl DesktopApp {
    /// Starts a session from what the command line asked for, reading the
    /// named file into the kernel.
    ///
    /// # Errors
    ///
    /// [`StartupError`] for a file that will not read as text.
    pub fn new(options: Options) -> Result<Self, StartupError> {
        let mut editor = Editor::with_defaults();

        if let Some(path) = options.path {
            let text = std::fs::read_to_string(&path).map_err(|error| StartupError::File {
                path: path.clone(),
                error,
            })?;
            editor.set_content(&text);
            // After the content: setting a language parses the document, and
            // doing it first would parse an empty one and then parse again.
            if let Some(language) = path
                .extension()
                .and_then(std::ffi::OsStr::to_str)
                .and_then(Language::from_extension)
            {
                editor.set_language(language);
            }
        }

        Ok(Self {
            editor,
            shell: None,
            scroll_y: 0.0,
            modifiers: ModifiersState::empty(),
            clipboard: String::new(),
            notice: None,
            failure: None,
        })
    }

    /// What killed the session from inside the event loop, if anything.
    #[must_use]
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    /// Handles one translated key press: kernel first, then whatever the
    /// kernel handed back, then scroll-to-caret, then a repaint.
    fn press(&mut self, event: &KeyEvent) {
        // A notice describes the key before this one.
        self.clear_notice();
        let result = self.editor.handle_key(event);
        self.consume(result);
        self.ensure_caret_visible();
        if let Some(shell) = &mut self.shell {
            // A caret that stays in its off-phase across a movement looks
            // like a caret that vanished.
            shell.compositor.reset_blink();
            shell.window.request_redraw();
        }
    }

    /// Acts on what the kernel made of a key.
    fn consume(&mut self, result: EditorKeyResult) {
        match result {
            EditorKeyResult::None => {},
            EditorKeyResult::Clipboard(operation) => self.clipboard(operation),
            EditorKeyResult::Search(action) => self.search_action(&action),
            // The count and captures are not read for the same reason the
            // terminal face leaves them: this face pushes no keymap that can
            // produce them.
            EditorKeyResult::HostCommand { command, .. } => {
                self.notice(format!(
                    "`{command}` is not wired into the desktop shell yet"
                ));
            },
        }
    }

    /// Keeps or supplies clipboard text.
    ///
    /// The delete half of a cut is the kernel's own reversible command,
    /// applied unchanged — building the deletion here would be a second
    /// implementation of one the kernel handed over.
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

    /// Acts on a search action the kernel reported.
    ///
    /// The kernel has already moved to the next or previous match by the time
    /// it reports one; the panel actions name UI this slice has not painted,
    /// and say so instead of dying silently.
    fn search_action(&mut self, action: &SearchAction) {
        match action {
            SearchAction::OpenSearch => {
                self.notice("the search panel is not built in the desktop shell yet".to_owned());
            },
            SearchAction::CloseSearch | SearchAction::NextMatch | SearchAction::PreviousMatch => {},
        }
    }

    /// Shows a notice in the window title, until the next key press.
    fn notice(&mut self, text: String) {
        if let Some(shell) = &self.shell {
            shell.window.set_title(&format!("{TITLE} — {text}"));
        }
        self.notice = Some(text);
    }

    /// Restores the plain title if a notice is showing.
    fn clear_notice(&mut self) {
        if self.notice.take().is_some()
            && let Some(shell) = &self.shell
        {
            shell.window.set_title(TITLE);
        }
    }

    /// Scrolls so the primary caret is on screen, mirroring the logic the web
    /// face kept when the compositor was extracted: the caret's document-space
    /// Y comes from the compositor, wrap-aware when the caret's line has not
    /// changed; the scroll decision stays here, on the face that owns
    /// `scroll_y`.
    fn ensure_caret_visible(&mut self) {
        let Some(shell) = &self.shell else {
            return;
        };
        let line_height = shell.compositor.line_height();
        let viewport_height = u32_to_f32(shell.surface.height());
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
    /// nothing honest to measure), and `load_font` is the compositor's only
    /// remeasuring path. The matching `Resized` event carries the new
    /// physical dimensions and follows separately.
    fn rescaled(&mut self, scale_factor: f64) {
        if let Some(shell) = &mut self.shell {
            shell
                .compositor
                .set_font_size(BASE_FONT_SIZE * scale_to_f32(scale_factor));
            shell.compositor.load_font(FONT.to_vec());
        }
        self.sync_kernel_viewport();
        if let Some(shell) = &self.shell {
            shell.window.request_redraw();
        }
    }

    /// Composes and presents one frame.
    ///
    /// A failed frame is reported on standard error and the session carries
    /// on — the web face does the same. The next event requests the next
    /// attempt; a fault that persists keeps naming itself rather than
    /// silently freezing the window.
    fn redraw(&mut self) {
        let editor = &self.editor;
        let scroll_y = self.scroll_y;
        let Some(shell) = &mut self.shell else {
            return;
        };
        let Shell {
            surface,
            compositor,
            ..
        } = shell;

        let width = surface.width();
        let height = surface.height();
        let fold_state = editor.fold_state();
        let mut highlights = BuiltinHighlights;

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
            )
        });

        if let Err(error) = outcome {
            let _ = writeln!(io::stderr(), "iridium-desktop: frame failed: {error}");
        }
    }
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
        match Shell::open(event_loop) {
            Ok(shell) => {
                shell.window.request_redraw();
                self.shell = Some(shell);
                self.sync_kernel_viewport();
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
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => self.resized(size.width, size.height),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => self.rescaled(scale_factor),
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                // Releases are not presses, and synthetic presses are focus
                // bookkeeping, not typing.
                if event.state == ElementState::Pressed
                    && !is_synthetic
                    && let Some(key) =
                        keys::translate(&event.logical_key, event.repeat, self.modifiers)
                {
                    self.press(&key);
                }
            },
            WindowEvent::Focused(false) => {
                // A chord left pending across a blur would eat the first
                // keystroke after the user came back; the kernel names this
                // path explicitly.
                self.editor.abort_pending_key_sequence();
            },
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {},
        }
    }
}
