//! The session's fields, and the questions asked of them from everywhere.
//!
//! The fields are `pub(super)` rather than private because the handlers that
//! read them are siblings: [`DesktopApp`] is one unit of session state whose
//! *methods* split by which input they answer, not a type with a boundary
//! through its middle.
//!
//! Note what is deliberately absent: whole-`self` accessors. `active_editor_mut`
//! is reached through the `workspace` *field* at every call site, because a
//! method on [`DesktopApp`] borrows all of `self` and would break
//! `self.search.handle_key(event, editor)` — that call is legal only because
//! `search` and `workspace` are disjoint fields.

use std::io::{self, Write as _};

#[cfg(test)]
use iridium_editor::Editor;
use iridium_editor::commands::palette::CommandMru;
use iridium_editor::workspace::{DocumentId, Workspace};
use iridium_file::TextFile;
use winit::keyboard::ModifiersState;

use super::startup::Shell;
use super::theme::ThemeSource;
use crate::command_palette::CommandPalette;
use crate::context_menu::ContextMenu;
use crate::file_tree::FileExplorer;
use crate::highlight::HighlightCache;
use crate::history_overlay::HistoryPanel;
use crate::latency::LatencyMonitor;
use crate::mouse::Pointer;
use crate::prompt::{Message, Prompt};
use crate::search::SearchOverlay;

/// The tab label for a buffer that has no file yet.
pub(super) const UNTITLED: &str = "untitled";

/// Whether the session carries on after an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Wait for the next event.
    Running,
    /// Leave.
    Exit,
}

/// What this face keeps per open document.
///
/// Three fields, none of which duplicate anything the kernel holds: the
/// scroll offset is in physical pixels and the kernel's is in lines, the
/// file is this face's business because the kernel does no I/O, and the
/// highlight cache is keyed to the compositor's seam.
///
/// It rides in the [`Workspace`] rather than in a map this face keeps,
/// because the rule for when it goes away — *when the last tab onto its
/// document closes* — is already implemented and tested there. Two tabs
/// onto one file share a buffer and an undo history, and must therefore
/// share exactly one of these.
#[derive(Debug, Default)]
pub(super) struct DesktopDocument {
    /// The face's vertical scroll offset, in physical pixels.
    pub(super) scroll_y: f32,
    /// The file this document came from, if it has one yet.
    pub(super) file: Option<TextFile>,
    /// The kernel's tree-sitter spans, cached per parse generation and
    /// resolved through the compositor's highlight seam each frame. See
    /// [`crate::highlight`].
    pub(super) syntax: HighlightCache,
}

/// One editing session in one window.
pub struct DesktopApp {
    /// Every open document, its organisation, and this face's per-document
    /// state.
    ///
    /// **Never empty while a session is running.** `new` opens either the
    /// named file or an untitled buffer, and closing the last tab opens a
    /// fresh untitled one — a window with no tab has nothing to type into
    /// and no honest thing to draw. Nothing here relies on that invariant
    /// being *provable*: every read still handles the empty case by
    /// returning early rather than by asserting it away.
    pub(super) workspace: Workspace<DesktopDocument>,
    /// The window-bound state, absent until the event loop resumes.
    pub(super) shell: Option<Shell>,
    /// The modifier state winit last reported, applied to every key press
    /// and mouse press.
    pub(super) modifiers: ModifiersState,
    /// The kernel's mouse machinery plus the pointer state winit reports
    /// piecemeal. See [`crate::mouse`].
    pub(super) pointer: Pointer,
    /// The system clipboard, opened lazily so a pasteboard that cannot be
    /// reached fails on the keystroke that needed it — visibly — rather than
    /// at startup.
    pub(super) clipboard: Option<arboard::Clipboard>,
    /// The search panel. Kept across closes so re-opening offers the last
    /// query back, which is what [`SearchOverlay`] is built for.
    pub(super) search: SearchOverlay,
    /// Whether the search panel is on screen. It is not modal: keys it does
    /// not bind stay the host's, so a save chord works mid-search.
    pub(super) search_open: bool,
    /// The command palette panel.
    pub(super) palette: CommandPalette,
    /// Whether the palette is on screen. While it is, it is modal: every key
    /// goes to it, exactly as with a prompt.
    pub(super) palette_open: bool,
    /// Which commands ran recently, feeding the palette's recency ranking.
    ///
    /// Recorded only after a command actually ran — an entry for a command
    /// that errored would rank a failure as a favourite.
    pub(super) mru: CommandMru,
    /// The right-click context menu, while one is up. Modal while it is:
    /// it holds key focus, and the click that leaves it is spent leaving it.
    pub(super) menu: Option<ContextMenu>,
    /// The file explorer, while one is open. Modal while it is.
    ///
    /// `Option` rather than a panel plus a flag, because unlike the other
    /// panels this one owns a reader thread: closing it must *drop* it, not
    /// hide it, or an editor that had the explorer open once keeps a thread
    /// and an arena of every directory ever expanded for the rest of the
    /// session.
    pub(super) explorer: Option<FileExplorer>,
    /// The undo-tree panel.
    pub(super) history: HistoryPanel,
    /// Whether the undo-tree panel is on screen. Modal while it is.
    pub(super) history_open: bool,
    /// The question on the prompt strip, if one is open. Modal while it is.
    pub(super) prompt: Option<Prompt>,
    /// What the last input had to say, shown on the strip until the next key.
    pub(super) message: Option<Message>,
    /// Keydown-to-present measurement, armed by `IRIDIUM_LATENCY`; one
    /// branch per event when it is not. See [`crate::latency`].
    pub(super) latency: LatencyMonitor,
    /// Who chose the theme, and so whether the system appearance still
    /// overrides it. See [`ThemeSource`].
    ///
    /// Pinned by `--theme` at startup and by the manual toggle, and never
    /// unpinned: a pin lasts until the window closes. There is nowhere to
    /// persist it to and nothing to clear it with — the estate has no
    /// preferences store, and inventing one is not a light theme's decision
    /// to make (D-7).
    ///
    /// The alternative to a pin is worse than it looks. Without one, the next
    /// `ThemeChanged` — which arrives on a schedule the user did not choose,
    /// when macOS crosses sunset under "Auto" — silently undoes a deliberate
    /// choice made ten seconds earlier.
    pub(super) theme_source: ThemeSource,
    /// The window title as last set, so an unchanged title costs nothing.
    pub(super) title: String,
    /// What killed the session from inside the event loop, if anything;
    /// [`crate::run`] reports it after the loop returns, because winit's
    /// callbacks have nowhere to return an error to.
    pub(super) failure: Option<String>,
}

impl std::fmt::Debug for DesktopApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopApp")
            .field(
                "scroll_y",
                &self.workspace.active_payload().map(|it| it.scroll_y),
            )
            .field("tabs", &self.workspace.tab_count())
            .field("prompt", &self.prompt)
            .field("message", &self.message)
            .field("failure", &self.failure)
            .finish_non_exhaustive()
    }
}

impl DesktopApp {
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
    ///
    /// Answers `false` when no tab is open: there is no document, so nothing
    /// can differ from anything. That is the honest answer for a state the
    /// session should never be in, and a truthful `false` here is what stops
    /// a spurious "unsaved changes" prompt on the way out.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.workspace
            .active_document()
            .is_some_and(|document| self.document_is_dirty(document))
    }

    /// Whether one document differs from what is on disk.
    ///
    /// The same comparison [`Self::is_dirty`] makes, asked of a named buffer
    /// rather than of whichever one is in front — the tab strip marks every
    /// tab, not just the active one, and marking them by a different test
    /// than the one that guards closing would show a dot on a tab that closes
    /// without a question, or no dot on one that stops to ask.
    pub(super) fn document_is_dirty(&self, document: DocumentId) -> bool {
        let Some(payload) = self.workspace.payload(document) else {
            return false;
        };
        let Some(editor) = self.workspace.editor(document) else {
            return false;
        };
        let saved = payload
            .file
            .as_ref()
            .and_then(TextFile::saved_text)
            .unwrap_or("");
        *editor.state().document.rope() != saved
    }

    /// Asks for the next frame, when there is a window to ask.
    pub(super) fn request_redraw(&self) {
        if let Some(shell) = &self.shell {
            shell.window.request_redraw();
        }
    }
}

/// Whole-`self` accessors for the tests, and **only** for the tests.
///
/// The production code deliberately has none of these, for the reason given
/// at the top of this module. A test is straight-line code that never holds
/// two borrows at once, so it pays nothing for the convenience; the
/// `cfg(test)` gate is what stops the convenience leaking back into the paths
/// that cannot afford it.
#[cfg(test)]
impl DesktopApp {
    /// The active tab's editor. Panics if no tab is open — every test opens
    /// one, and a test that silently asserted nothing would be worse.
    pub(super) fn test_editor(&self) -> &Editor {
        self.workspace.active_editor().expect("a tab is open")
    }

    /// The active tab's editor, mutably.
    pub(super) fn test_editor_mut(&mut self) -> &mut Editor {
        self.workspace.active_editor_mut().expect("a tab is open")
    }

    /// The active tab's face state.
    pub(super) fn test_document(&self) -> &DesktopDocument {
        self.workspace.active_payload().expect("a tab is open")
    }

    /// The active tab's face state, mutably.
    pub(super) fn test_document_mut(&mut self) -> &mut DesktopDocument {
        self.workspace.active_payload_mut().expect("a tab is open")
    }

    /// Rebuilds the active tab's highlight cache, as a frame would.
    pub(super) fn test_refresh_syntax(&mut self) {
        let (editor, document) = self
            .workspace
            .active_editor_and_payload_mut()
            .expect("a tab is open");
        document.syntax.refresh(editor);
    }
}
