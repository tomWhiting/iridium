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
//! [`UndoNodeInfo`] rows already carry parentage, branch order, the preferred
//! redo child, ages and the current position. Jumping is
//! [`Editor::jump_to_history_node`], the kernel's own multi-edge replay that
//! emits exactly one content-changed event. What is here is a row layout, a
//! selection and a box.
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

use iridium_editor::history::{UndoNodeId, UndoNodeInfo, UndoTreeSnapshot};
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
    /// The selected node, or `None` to follow the document's current node.
    ///
    /// A node id rather than a row index: a jump reshapes nothing but a
    /// refresh re-linearizes the tree, and an index would silently point at a
    /// different state after one.
    selected: Option<u64>,
    /// The first visible row: the scroll position of the list window.
    scroll: usize,
    /// How many rows the last paint showed — the page the page keys hop.
    window: usize,
}

/// One row of the linearized tree: a node, its indent, and whether it lies on
/// the active undo/redo path.
struct TreeRow<'a> {
    /// The node this row shows.
    node: &'a UndoNodeInfo,
    /// How many branch points lie between the root and this node.
    indent: usize,
    /// Whether plain undo/redo travels through this node.
    on_active_path: bool,
}

impl HistoryPanel {
    /// A closed panel.
    #[must_use]
    pub fn new() -> Self {
        Self {
            window: super::panel::MAX_VISIBLE_ROWS,
            ..Self::default()
        }
    }

    /// Resets the panel for opening: the selection follows the current node.
    pub const fn open(&mut self) {
        self.selected = None;
        self.scroll = 0;
    }

    /// Handles one key press. Every key is consumed; see the module docs.
    pub fn handle_key(&mut self, event: &KeyEvent, editor: &Editor) -> HistoryOutcome {
        let snapshot = editor.history_snapshot();
        let rows = linearize(&snapshot);
        let page = self.window.max(1);
        match (chord(event.modifiers), event.key) {
            (Chord::Plain, KeyCode::Escape) | (Chord::CtrlAlt, KeyCode::Char('h' | 'H')) => {
                HistoryOutcome::Closed
            },
            (Chord::Plain, KeyCode::Enter) => self.jump_to_selected(&rows),
            (Chord::Plain, KeyCode::Up) => self.move_selection(&rows, -1),
            (Chord::Plain, KeyCode::Down) => self.move_selection(&rows, 1),
            (Chord::Plain, KeyCode::PageUp) => self.move_selection(&rows, -isize_of(page)),
            (Chord::Plain, KeyCode::PageDown) => self.move_selection(&rows, isize_of(page)),
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

    /// The selected row's node, resolved against the rows on screen.
    ///
    /// Falls back to the current node's row when nothing was selected yet or
    /// the selected node no longer exists.
    fn selected_row(&self, rows: &[TreeRow<'_>]) -> Option<usize> {
        let explicit = self.selected.and_then(|wanted| {
            rows.iter()
                .position(|row| row.node.id.parse::<u64>().ok() == Some(wanted))
        });
        explicit.or_else(|| rows.iter().position(|row| row.node.is_current))
    }

    /// Emits a jump to the selected node.
    fn jump_to_selected(&self, rows: &[TreeRow<'_>]) -> HistoryOutcome {
        let Some(index) = self.selected_row(rows) else {
            return HistoryOutcome::Handled;
        };
        let Some(row) = rows.get(index) else {
            return HistoryOutcome::Handled;
        };
        row.node
            .id
            .parse::<u64>()
            .map_or(HistoryOutcome::Handled, |id| {
                HistoryOutcome::Jump(UndoNodeId::from_u64(id))
            })
    }

    /// Moves the selection by `delta` rows, clamping at both ends.
    fn move_selection(&mut self, rows: &[TreeRow<'_>], delta: isize) -> HistoryOutcome {
        let Some(last) = rows.len().checked_sub(1) else {
            return HistoryOutcome::Handled;
        };
        let current = self.selected_row(rows).unwrap_or(0).min(last);
        let target = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta.unsigned_abs()).min(last)
        };
        if let Some(row) = rows.get(target) {
            self.selected = row.node.id.parse::<u64>().ok();
        }
        HistoryOutcome::Handled
    }

    /// Slides the scroll window so the selection is inside `visible` rows,
    /// and remembers `visible` as the page size. Called from the paint pass.
    fn follow_selection(&mut self, rows: &[TreeRow<'_>], visible: usize) {
        self.window = visible.max(1);
        if visible == 0 || rows.is_empty() {
            self.scroll = 0;
            return;
        }
        let selected = self.selected_row(rows).unwrap_or(0);
        if selected < self.scroll {
            self.scroll = selected;
        } else if selected >= self.scroll + visible {
            self.scroll = selected + 1 - visible;
        }
        self.scroll = self.scroll.min(rows.len().saturating_sub(visible));
    }
}

/// Linearizes the tree for display: root first, depth-first, children in
/// creation order, so time runs downward and a fork's branches sit under it.
///
/// The indent counts branch points, not depth — a two-hundred-edit straight
/// history stays flush left, and only genuine forks move text sideways.
fn linearize(snapshot: &UndoTreeSnapshot) -> Vec<TreeRow<'_>> {
    let by_id: std::collections::HashMap<&str, &UndoNodeInfo> = snapshot
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();

    let active = active_path(snapshot, &by_id);

    let mut rows = Vec::with_capacity(snapshot.nodes.len());
    let mut stack: Vec<(&str, usize)> = vec![(snapshot.info.root_id.as_str(), 0)];
    while let Some((id, indent)) = stack.pop() {
        let Some(node) = by_id.get(id) else {
            continue;
        };
        rows.push(TreeRow {
            node,
            indent,
            on_active_path: active.contains(id),
        });
        let child_indent = if node.child_ids.len() > 1 {
            indent + 1
        } else {
            indent
        };
        // Reversed so the first child is popped first and appears first.
        for child in node.child_ids.iter().rev() {
            stack.push((child.as_str(), child_indent));
        }
    }
    rows
}

/// The node ids plain undo and redo travel through: the current node, its
/// ancestors, and the preferred-child descent below it.
fn active_path<'a>(
    snapshot: &'a UndoTreeSnapshot,
    by_id: &std::collections::HashMap<&str, &'a UndoNodeInfo>,
) -> std::collections::HashSet<&'a str> {
    let mut active = std::collections::HashSet::new();
    let mut upward: Option<&str> = Some(snapshot.info.current_id.as_str());
    while let Some(id) = upward {
        if !active.insert(id) {
            break;
        }
        upward = by_id.get(id).and_then(|node| node.parent_id.as_deref());
    }
    let mut downward = by_id
        .get(snapshot.info.current_id.as_str())
        .and_then(|node| node.preferred_child_id.as_deref());
    while let Some(id) = downward {
        if !active.insert(id) {
            break;
        }
        downward = by_id
            .get(id)
            .and_then(|node| node.preferred_child_id.as_deref());
    }
    active
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
