//! Bringing a session into existence: what the command line asked for, the
//! window and GPU behind it, and the first tab.
//!
//! Two halves that cannot be built together. [`DesktopApp::new`] runs before
//! winit has an event loop, so it can read a file and register commands but
//! cannot open a window; [`Shell::open`] runs from `resumed` and builds
//! everything that needs one. The split is winit's, not this face's.

use std::path::PathBuf;
use std::sync::Arc;

use iridium_editor::EditorConfig;
use iridium_editor::commands::palette::CommandMru;
use iridium_editor::render::FrameCompositor;
use iridium_editor::theme::Theme;
use iridium_editor::workspace::{FaceSetupError, Workspace};
use iridium_file::{FileError, TextFile};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::ModifiersState;
use winit::window::Window;

use super::files::language_of;
use super::state::{DesktopApp, DesktopDocument, UNTITLED};
use super::title::TITLE;
use crate::command_palette::CommandPalette;
use crate::commands;
use crate::highlight::HighlightCache;
use crate::history_overlay::HistoryPanel;
use crate::latency::LatencyMonitor;
use crate::mouse::Pointer;
use crate::overlay::OverlayPainter;
use crate::search::SearchOverlay;
use crate::surface::NativeSurface;
use crate::units::scale_to_f32;

/// Base font size in logical pixels; multiplied by the window's scale factor
/// before it reaches the compositor, matching the web face's formula
/// (`14.0 * devicePixelRatio`).
pub(super) const BASE_FONT_SIZE: f32 = 14.0;

/// The editor font, embedded so the face needs no asset path to exist at
/// runtime — the same `JetBrains Mono` the web demo serves its canvas.
pub(super) static FONT: &[u8] =
    include_bytes!("../../../../examples/web/public/fonts/JetBrainsMono-Regular.ttf");

/// What went wrong before a window could open.
#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    /// The file named on the command line could not be opened.
    #[error(transparent)]
    File(#[from] FileError),
    /// A command or keymap layer this face contributes was refused.
    ///
    /// One variant rather than two because the workspace validates both by
    /// building an editor, and reports which half failed inside the error
    /// it returns.
    #[error(transparent)]
    Setup(#[from] FaceSetupError),
    /// The session could not open its first tab.
    ///
    /// Unreachable through this code path — the only refusal is a parent
    /// that is not a group, and startup passes none — but a session with
    /// no tab has nothing to type into, so it is named rather than
    /// papered over.
    #[error("could not open the initial tab")]
    NoInitialTab,
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
pub(super) struct Shell {
    /// The window, shared with the surface that holds it alive.
    pub(super) window: Arc<Window>,
    /// The swapchain on that window.
    pub(super) surface: NativeSurface,
    /// The kernel's per-frame composition, driven from `RedrawRequested`.
    pub(super) compositor: FrameCompositor,
    /// The prompt strip's painter, run as a second pass after the compositor.
    pub(super) overlay: OverlayPainter,
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
    pub(super) fn open(event_loop: &ActiveEventLoop, theme: Theme) -> Result<Self, String> {
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
        // The rejection is deliberately unhandled rather than unnoticed.
        // winit reports a positive finite scale factor, and `scale_to_f32`
        // maps anything else to zero, so this can only refuse a scale factor
        // that was already nonsense — in which case the compositor keeps its
        // default size, which is legible. There is nowhere to report it to:
        // this face writes nothing to a console by design (see `run`).
        let _ = compositor.set_font_size(font_size);
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
        let mut workspace =
            Workspace::<DesktopDocument>::new(EditorConfig::default(), Theme::default());

        // Registered on the workspace, not on one editor: it applies the
        // whole set to every tab, present and future. Registering on a
        // single `Editor` would give the first tab this face's chords and
        // every later tab none of them.
        for meta in commands::command_metas() {
            workspace.register_command(meta)?;
        }
        workspace.push_keymap(commands::keymap())?;

        let (content, file) = match options.path {
            Some(path) => {
                let (file, text) = TextFile::open(&path)?;
                (text, Some(file))
            },
            None => (String::new(), None),
        };
        let title = file
            .as_ref()
            .map_or_else(|| UNTITLED.to_owned(), TextFile::display_name);
        let language = file.as_ref().and_then(|file| language_of(file.path()));

        // A session always has a tab. An untitled empty buffer is what a
        // window with no file has always shown; it is now a *tab* showing
        // it, which is the same thing with a name.
        let tab = workspace.open_with(
            &content,
            title,
            None,
            DesktopDocument {
                scroll_y: 0.0,
                file,
                syntax: HighlightCache::new(),
            },
        );
        // Unreachable: `open_with` refuses only a parent that is not a
        // group, and this passes none. Reported rather than ignored, since
        // a session with no tab is not a session.
        if tab.is_none() {
            return Err(StartupError::NoInitialTab);
        }

        // After the content: setting a language parses the document, and
        // doing it first would parse an empty one and then parse again.
        if let (Some(language), Some(editor)) = (language, workspace.active_editor_mut()) {
            editor.set_language(language);
        }

        Ok(Self {
            workspace,
            shell: None,
            modifiers: ModifiersState::empty(),
            pointer: Pointer::new(),
            clipboard: None,
            search: SearchOverlay::new(),
            search_open: false,
            palette: CommandPalette::new(),
            palette_open: false,
            mru: CommandMru::default(),
            menu: None,
            explorer: None,
            history: HistoryPanel::new(),
            history_open: false,
            prompt: None,
            message: None,
            latency: LatencyMonitor::from_env(),
            title: String::new(),
            failure: None,
        })
    }
}
