//! Bringing a session into existence: what the command line asked for, the
//! window and GPU behind it, and the first tab.
//!
//! Two halves that cannot be built together. [`DesktopApp::new`] runs before
//! winit has an event loop, so it can read a file and register commands but
//! cannot open a window; [`Shell::open`] runs from `resumed` and builds
//! everything that needs one. The split is winit's, not this face's.

use std::path::PathBuf;
use std::sync::Arc;

use iridium_config::UserConfig;
use iridium_config::theme::{ThemeChoice, ThemeError};
use iridium_editor::commands::palette::CommandMru;
use iridium_editor::render::FrameCompositor;
use iridium_editor::theme::Theme;
use iridium_editor::workspace::{FaceSetupError, Workspace};
use iridium_file::{FileError, TextFile};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::ModifiersState;
use winit::window::Window;

use super::config;
use super::files::language_of;
use super::state::{DesktopApp, DesktopDocument, UNTITLED};
use super::theme::ThemeSource;
use super::title::TITLE;
use crate::command_palette::CommandPalette;
use crate::commands;
use crate::file_tree::FileExplorer;
use crate::highlight::HighlightCache;
use crate::history_overlay::HistoryPanel;
use crate::latency::LatencyMonitor;
use crate::mouse::Pointer;
use crate::overlay::OverlayPainter;
use crate::project;
use crate::prompt::Message;
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
    /// `--theme` named a file that could not be read or parsed.
    ///
    /// Fatal, unlike a bad `config.toml`, and the asymmetry is deliberate: a
    /// configuration file is found by the editor and its absence is normal, so
    /// falling back to defaults is right. A `--theme` path was typed by the
    /// person running the command, and silently ignoring it would open a
    /// window in the wrong colours with no explanation.
    #[error(transparent)]
    Theme(#[from] ThemeError),
    /// A directory was named on the command line and could not be shown.
    ///
    /// Fatal, on the same reasoning as `--theme`: the directory was typed by
    /// whoever ran the command, and it is the *whole* of what they asked for —
    /// a session opened on a folder has no file to fall back to, so carrying on
    /// would leave an empty untitled buffer and no hint that anything failed.
    ///
    /// Note that a path which does not exist never reaches here: `is_dir` is
    /// false for it, so it is read as a file that has not been written yet,
    /// which is what it is.
    #[error("cannot show `{}`: {message}", path.display())]
    Explorer {
        /// The directory that could not be shown.
        path: PathBuf,
        /// What the directory reader said.
        message: String,
    },
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
    /// The path the invocation named, if it named one.
    ///
    /// A file to edit, or a directory to open as a project — which of the two
    /// is not decided here. [`DesktopApp::new`] asks the filesystem at the
    /// moment it opens, so the command line stays parseable without one.
    pub path: Option<PathBuf>,
    /// The theme `--theme` asked for. `None` leaves the session on the
    /// kernel's default, which is what the system-appearance default then
    /// overrides once a window exists.
    pub theme: Option<ThemeChoice>,
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
        // Ignored deliberately, and the reason is not the one above. A refused
        // *size* is a runtime value that could be wrong; `FONT` is compiled
        // into this binary, so a refusal here would mean the build itself
        // shipped bytes that are not a font, and there is no runtime response
        // to that — the window is still opening and this face has no console.
        //
        // Not left to trust: `the_embedded_font_holds_a_readable_face` below
        // asserts these exact bytes parse, and it needs no GPU, so unlike the
        // screenshot harness it actually runs in the battery.
        let _ = compositor.load_font(FONT.to_vec());

        let mut overlay = OverlayPainter::new(
            surface.device(),
            surface.queue(),
            surface.format(),
            surface.width(),
            surface.height(),
        )
        .map_err(|error| format!("cannot create the overlay painter: {error}"))?;
        // Same compiled-in `FONT`, same reasoning as the compositor above.
        let _ = overlay.set_font(font_size, FONT.to_vec());
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
        // ⚠️ **The first-run configuration file is written by
        // [`crate::run::main`], not here, and that placement is the point.**
        // This constructor runs in the test suite — `app::tests::config`
        // builds sessions with it — so a `$HOME` write here means `cargo test`
        // creating files in the home directory of whoever ran it. It did
        // exactly that once, on 9 Aug 2026, before the call was moved:
        // harmless, because the write is create-only and never overwrites, and
        // wrong regardless. Writing the file is a courtesy of the *program*,
        // so the program does it and this stays pure.

        // Read before the workspace exists, because the settings are what it
        // is constructed with. A file that is missing, unreadable or wrong
        // never fails here: it yields defaults and a list of problems, and the
        // session carries on — see `iridium_config` for why that is the rule
        // rather than a convenience.
        let user = UserConfig::read();
        // `--theme` wins over the kernel's default here, and over the system
        // appearance later: `Shell::open` only reads `Window::theme()` when
        // the command line asked for nothing, so an explicit flag is a pin
        // from the first frame rather than a value the first `ThemeChanged`
        // undoes.
        let theme = match options.theme.as_ref() {
            Some(choice) => iridium_config::theme::load(choice)?,
            None => Theme::default(),
        };
        let mut workspace = Workspace::<DesktopDocument>::new(user.editor.clone(), theme);

        // Registered on the workspace, not on one editor: it applies the
        // whole set to every tab, present and future. Registering on a
        // single `Editor` would give the first tab this face's chords and
        // every later tab none of them.
        for meta in commands::command_metas() {
            workspace.register_command(meta)?;
        }
        workspace.push_keymap(commands::keymap())?;

        // After this face's own layer, so the user's bindings sit on top of it
        // and win — which is what a user keymap is for. Before the first tab,
        // so no document is ever open under a keymap that is about to change.
        let mut problems = user.problems;
        problems.extend(config::install_user_bindings(&mut workspace, user.bindings));

        // A directory is a *project*, not a file: nothing is read as text and
        // the explorer is what the session shows. `is_dir` follows symlinks,
        // so a link to a project behaves as the project — which is what a link
        // to a project is for.
        let (content, file, project) = match options.path {
            Some(path) if path.is_dir() => (String::new(), None, Some(path)),
            Some(path) => {
                let (file, text) = TextFile::open(&path)?;
                (text, Some(file), None)
            },
            None => (String::new(), None, None),
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
        let Some(tab) = tab else {
            return Err(StartupError::NoInitialTab);
        };

        // After the content: setting a language parses the document, and
        // doing it first would parse an empty one and then parse again.
        if let (Some(language), Some(editor)) = (language, workspace.active_editor_mut()) {
            editor.set_language(language);
        }

        // Anything the configuration file said that could not be honoured goes
        // into a tab of its own — behind the file that was asked for, because
        // that file is still what the session was opened to show. The message
        // strip says the tab is there; see [`config`] for why one line is not
        // enough on its own.
        let message = (!problems.is_empty()).then(|| {
            workspace.open(
                &config::report(iridium_config::user_config_path().as_deref(), &problems),
                config::PROBLEMS_TAB,
                None,
            );
            workspace.activate(tab);
            Message::error(config::summary(&problems))
        });

        // The panel a project session exists to show, built here rather than
        // on the first frame so a directory that cannot be read says so before
        // a window opens — the same moment a file that cannot be read does.
        let explorer = match project.clone() {
            Some(path) => {
                // Through `chosen_root`, not around it: a directory named on
                // the command line was picked by hand, and that is exactly the
                // distinction that function draws — so the crawl rule stays in
                // one place rather than being restated here.
                let root = project::chosen_root(path);
                Some(
                    FileExplorer::open(root.path.clone(), root.crawl).map_err(|message| {
                        StartupError::Explorer {
                            path: root.path,
                            message,
                        }
                    })?,
                )
            },
            None => None,
        };

        Ok(Self {
            project,
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
            explorer,
            history: HistoryPanel::new(),
            history_open: false,
            prompt: None,
            message,
            latency: LatencyMonitor::from_env(),
            // A `--theme` on the command line is an explicit choice, so it
            // pins from the first frame: the window never reads the system
            // appearance over the top of it, and the first `ThemeChanged`
            // does not undo what was asked for.
            theme_source: if options.theme.is_some() {
                ThemeSource::Pinned
            } else {
                ThemeSource::System
            },
            title: String::new(),
            failure: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::FONT;

    /// ⭐ The screenshot harness asserts the same thing and **cannot be relied
    /// on for it**: it is `#[ignore]`d because it needs a real GPU, so it does
    /// not run in the battery. This one needs no device — it parses the bytes
    /// into a throwaway database — which is the only reason the claim "the
    /// embedded font loads" is checked at all rather than merely believed.
    ///
    /// What it catches: a vendor refresh, an `include_bytes!` path edited to
    /// something that is not a font, or a well-meant swap to `.woff2`, which
    /// this build cannot decode. Any of those leaves every face measuring an
    /// approximated advance width and drawing no glyphs, and nothing else
    /// would report it.
    #[test]
    fn the_embedded_font_holds_a_readable_face() {
        assert!(
            iridium_editor::render::font_data_holds_a_face(FONT),
            "the font compiled into this binary holds no face the renderer \
             can read; raw sfnt (.ttf/.otf/.ttc) is required and .woff2 is \
             not decoded"
        );
    }
}
