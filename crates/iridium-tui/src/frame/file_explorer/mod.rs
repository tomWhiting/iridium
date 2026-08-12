//! The file explorer: a floating panel showing a directory tree, with the
//! oil buffer behind `Tab`.
//!
//! This is the terminal face of the same panel the GPU face draws, reached by
//! the same command id. Until this module existed the terminal had no file
//! tree at all.
//!
//! # ⭐ How little is here is the point
//!
//! The tree, the selection, the query, the crawl, the scroll window, the oil
//! buffer, every key's meaning, the rename planning and the refusals are all
//! [`iridium_panel::explorer::FileExplorer`] — the same code, not a second
//! implementation that agrees today. This module holds a poll, a paint and the
//! translation of one enum, and that is the entire terminal face of the file
//! explorer.
//!
//! Compare what it replaced: the GPU face's copy was seven modules and about
//! 2,600 lines. Writing that again here would have meant the two editors could
//! disagree about what a rename *does*, and the disagreement would be found by
//! the person using them rather than by a test, because nothing compiles both
//! faces' copies at once.
//!
//! # The panel is modal, exactly as the palette is
//!
//! Every key is consumed while it is open. The explorer's own key tables
//! decide what each one means — including the printable characters, which go
//! to the filter — so there is nothing here to keep in step with the GPU face.
//!
//! # ⚠️ One outcome this face cannot yet honour, and it says so
//!
//! [`ExplorerOutcome::ToggleSidebar`] asks for the panel to become a
//! full-height column down the left edge, taking columns *from* the document.
//! The terminal face has no such placement yet — it is #112d step 5, where
//! `FrameLayout` learns to give up columns.
//!
//! **It is reported rather than swallowed.** [`ExplorerAction::Report`]
//! carries a sentence for the statusline, because a key that silently does
//! nothing is indistinguishable from a key that is broken, and the person
//! pressing it has no way to tell which. A channel that can only carry success
//! is a defect.

mod paint;

#[cfg(test)]
mod tests;

use iridium_editor::KeyEvent;
use iridium_editor::theme::Theme;
use iridium_panel::PanelFit;
use iridium_panel::explorer::{ExplorerOutcome, FileExplorer};

use super::CellPosition;
use super::palette::Palette;
use super::panel::{FloatingBox, TOP};
use crate::cell::CellBuffer;

/// What the host should do after handing the explorer a key.
///
/// A narrower vocabulary than [`ExplorerOutcome`] on purpose: the shared panel
/// reports what *it* did, and this is what a terminal host can act on. The
/// mapping is [`ExplorerAction::from_outcome`], which is the one place a new
/// shared outcome has to be answered for in this face — so adding one over
/// there fails the build here rather than being quietly dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplorerAction {
    /// The key was consumed and the panel stays open.
    Handled,
    /// The panel should close.
    Close,
    /// The panel should close and this path should be opened.
    Open(std::path::PathBuf),
    /// Something failed, or was asked for that this face cannot do. The string
    /// is for the statusline, already a whole sentence.
    Report(String),
}

impl ExplorerAction {
    /// Translates a shared outcome into what this face does about it.
    fn from_outcome(outcome: ExplorerOutcome) -> Self {
        match outcome {
            ExplorerOutcome::Handled => Self::Handled,
            // Dismissed and Closed differ only in whether the panel asked to
            // go or was told to; a face with one placement has nothing to do
            // differently about that, and pretending otherwise would be a
            // distinction with no behaviour behind it.
            ExplorerOutcome::Dismissed | ExplorerOutcome::Closed => Self::Close,
            ExplorerOutcome::Open(path) => Self::Open(path),
            ExplorerOutcome::Failed(message) => Self::Report(message),
            ExplorerOutcome::ToggleSidebar => {
                Self::Report("the sidebar placement is not in the terminal face yet".to_owned())
            },
        }
    }
}

/// The terminal face's file explorer panel.
#[derive(Debug)]
pub struct FileExplorerPanel {
    /// The shared panel, which owns every piece of state there is.
    explorer: FileExplorer,
}

impl FileExplorerPanel {
    /// Opens the panel on `root`, crawling past it or not.
    ///
    /// # Errors
    ///
    /// Returns the shared panel's own message when the directory cannot be
    /// read — already a sentence fit for the statusline.
    pub fn open(root: std::path::PathBuf, crawl: bool) -> Result<Self, String> {
        FileExplorer::open(root, crawl).map(|explorer| Self { explorer })
    }

    /// Collects any directory listing that has arrived since the last frame.
    ///
    /// ⚠️ **The host must call this once per frame while the panel is open.**
    /// The filesystem source reads on a worker thread, so a listing lands some
    /// frames after it was wanted and nothing wakes the host when it does.
    /// Skipping it corrupts nothing; it leaves the panel showing a directory
    /// that is permanently loading, which is the failure this sentence exists
    /// to make findable.
    ///
    /// `true` means rows changed and a repaint is owed.
    pub fn poll(&mut self) -> bool {
        self.explorer.poll()
    }

    /// Whether a directory read is still outstanding, and so whether another
    /// frame is owed even if no key arrives.
    pub fn is_waiting(&self) -> bool {
        self.explorer.is_waiting()
    }

    /// Whether the panel's rows have been edited without being applied.
    ///
    /// ⚠️ **A host that closes the panel outright must ask this first.** The
    /// shared panel keeps a two-press refusal for its own `Escape`, and a host
    /// that drops the panel by some other route never gets to hear it — a
    /// directory's worth of typed renames would go with one chord and nothing
    /// would say so.
    pub fn has_unapplied_edits(&self) -> bool {
        self.explorer.has_unapplied_edits()
    }

    /// Hands one key to the panel.
    pub fn handle_key(&mut self, event: &KeyEvent) -> ExplorerAction {
        ExplorerAction::from_outcome(self.explorer.handle_key(event))
    }

    /// Composes and paints the panel, reporting where the caret landed.
    ///
    /// ⭐ **The fit is [`PanelFit::popover`] rather than a ceiling written
    /// here**, and that is deliberate: the browse ceiling is a property of the
    /// panel — a list you read *down* wants more rows than a list you *query* —
    /// and it was ruled once, in `iridium-panel`, for every face. A twelve
    /// written into this file would have been this face quietly disagreeing
    /// with the other one about how tall a file tree should be.
    pub fn paint(
        &mut self,
        buffer: &mut CellBuffer,
        theme: &Theme,
        styles: &Palette,
    ) -> Option<CellPosition> {
        let panel_box = FloatingBox::fitted(buffer.width(), buffer.height())?;
        let interior = interior_rows(buffer.height())?;
        let body = self.explorer.body(
            theme,
            PanelFit::popover(panel_box.content_width(), interior),
        );
        paint::paint(&body, buffer, styles)
    }
}

/// How many interior rows a floating box has on a screen this tall.
///
/// The box costs the row above it ([`TOP`]) and its two borders. `None` when
/// that leaves nothing — the same screen [`FloatingBox::fitted`] declines, so
/// the two answers cannot disagree about whether a panel is drawable.
fn interior_rows(screen_rows: usize) -> Option<usize> {
    screen_rows.checked_sub(TOP + 2).filter(|rows| *rows > 0)
}
