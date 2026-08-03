//! The undo-tree panel: every branch of the history, reachable.
//!
//! This is the terminal face of the kernel's `history.togglePanel` host
//! command. The kernel's undo history is a tree — an edit after an undo starts
//! a branch and the abandoned branch is kept — and until this panel existed
//! the terminal could only walk it blind, cycling branches with
//! `Ctrl+Alt+Z` / `Ctrl+Alt+Y` and trusting the statusline.
//!
//! # The tree is the kernel's and none of it is repeated here
//!
//! Everything shown comes from one call:
//! [`Editor::history_snapshot`], whose
//! [`UndoNodeInfo`](iridium_editor::history::UndoNodeInfo) rows already carry
//! parentage, branch order, the preferred redo child, ages and the current
//! position. So is the *shape* of the panel: the linearized rows and the
//! selection browsing them are
//! [`tree_view`](iridium_editor::history::tree_view), shared with every other
//! face that draws the tree. Jumping is [`Editor::jump_to_history_node`], the
//! kernel's own multi-edge replay that emits exactly one content-changed
//! event. What is here is a key map, a row painter and a box.
//!
//! # Reading the panel
//!
//! The root — the state the file was opened in — is the top row; time runs
//! downward. A run with no forks stays flush left; each branch point indents
//! its children one cell, so a straight history reads as a straight list and
//! only genuine forks grow sideways. The row the document currently sits on is
//! marked `*`; every other state is `o`. The **active path** — the states
//! plain undo and redo would travel through — is painted bright; everything
//! off it is dim. Ages are right-aligned.
//!
//! # Keys
//!
//! The panel is modal, like the palette: browsing history while stray keys
//! edited the document would grow the very tree being read.
//!
//! | Key | Action |
//! |---|---|
//! | `Up` / `Down` | move the selection, clamping at the ends |
//! | `PageUp` / `PageDown` | hop by one windowful |
//! | `Home` / `End` | the root / the newest row |
//! | `Enter` | jump the document to the selected state — the panel stays open |
//! | `Escape`, `Ctrl+Alt+H` | close |
//!
//! `Enter` deliberately keeps the panel open: hopping between two states and
//! watching the document change underneath is what a tree is *for*, and the
//! refreshed panel shows the `*` moving. `Escape` is how you put it away.

mod paint;

#[cfg(test)]
mod tests;

use iridium_editor::history::UndoNodeId;
use iridium_editor::history::tree_view::{TreeViewSelection, linearize};
use iridium_editor::{Editor, KeyCode, KeyEvent, Modifiers};

use super::palette::Palette;
use crate::cell::CellBuffer;

/// What became of a key handed to the panel.
///
/// There is no `Ignored`: the panel is modal, and a key it does not bind is
/// swallowed rather than falling through to the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryOutcome {
    /// The panel consumed the key and stays open.
    Handled,
    /// The panel was closed.
    Closed,
    /// The document should jump to this history node. The panel stays open,
    /// so the host repaints it with the current marker moved.
    Jump(UndoNodeId),
}

/// The undo-tree panel.
///
/// The host owns one of these, toggles it when the kernel reports the
/// `history.togglePanel` host command, hands it every key while it is open,
/// and paints it over the finished frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryPanel {
    /// The kernel's selection model: which node is selected, and which slice
    /// of the rows the window shows.
    selection: TreeViewSelection,
}

impl HistoryPanel {
    /// A closed panel.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            selection: TreeViewSelection::with_window(super::panel::MAX_VISIBLE_ROWS),
        }
    }

    /// Resets the panel for opening: the selection follows the current node.
    pub const fn open(&mut self) {
        self.selection.reset();
    }

    /// Handles one key press. Every key is consumed; see the module docs.
    pub fn handle_key(&mut self, event: &KeyEvent, editor: &Editor) -> HistoryOutcome {
        let snapshot = editor.history_snapshot();
        let rows = linearize(&snapshot);
        let page = isize_of(self.selection.page());
        match (chord(event.modifiers), event.key) {
            (Chord::Plain, KeyCode::Escape) | (Chord::CtrlAlt, KeyCode::Char('h' | 'H')) => {
                HistoryOutcome::Closed
            },
            (Chord::Plain, KeyCode::Enter) => self
                .selection
                .selected_node(&rows)
                .map_or(HistoryOutcome::Handled, HistoryOutcome::Jump),
            (Chord::Plain, KeyCode::Up) => self.move_selection(&rows, -1),
            (Chord::Plain, KeyCode::Down) => self.move_selection(&rows, 1),
            (Chord::Plain, KeyCode::PageUp) => self.move_selection(&rows, -page),
            (Chord::Plain, KeyCode::PageDown) => self.move_selection(&rows, page),
            (Chord::Plain, KeyCode::Home) => self.move_selection(&rows, isize::MIN),
            (Chord::Plain, KeyCode::End) => self.move_selection(&rows, isize::MAX),
            // Modal: everything else is swallowed, not passed to the document.
            _ => HistoryOutcome::Handled,
        }
    }

    /// Paints the panel over a finished frame.
    ///
    /// `&mut self` because painting is where the scroll window follows the
    /// selection and the page size is learned, exactly as in the palette.
    /// Nothing is drawn on a screen too small for an honest panel; the keys
    /// keep working regardless, so `Escape` can never be trapped.
    pub fn paint(&mut self, buffer: &mut CellBuffer, editor: &Editor, styles: &Palette) {
        paint::paint(self, buffer, editor, styles);
    }

    /// Moves the selection by `delta` rows, clamping at both ends.
    fn move_selection(
        &mut self,
        rows: &[iridium_editor::history::tree_view::TreeViewRow<'_>],
        delta: isize,
    ) -> HistoryOutcome {
        self.selection.move_by(rows, delta);
        HistoryOutcome::Handled
    }
}

/// `value` as an `isize`, saturating on a page size no terminal can reach.
fn isize_of(value: usize) -> isize {
    isize::try_from(value).unwrap_or(isize::MAX)
}

/// The modifier combinations the panel distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Chord {
    /// No modifier that changes the meaning of the key.
    Plain,
    /// Control and Alt together — the toggle chord that closes the panel.
    CtrlAlt,
    /// Anything else, which the modal panel swallows.
    Other,
}

/// The chord one modifier set names.
const fn chord(modifiers: Modifiers) -> Chord {
    match (modifiers.ctrl, modifiers.alt, modifiers.meta) {
        (false, false, false) => Chord::Plain,
        (true, true, false) => Chord::CtrlAlt,
        _ => Chord::Other,
    }
}
