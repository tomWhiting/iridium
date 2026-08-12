//! The panel's state and what reaches the screen.
//!
//! [`super`] carries the argument for what the undo-tree panel *is* and why the
//! tree it shows is the kernel's; this file is the state that argument is about,
//! and the composition that turns it into rows. The keys that change it live in
//! [`super::keys`], and the table they resolve against in [`super::keymap`].

use iridium_editor::history::UndoNodeId;
use iridium_editor::history::tree_view::{TreeViewRow, TreeViewSelection, linearize};
use iridium_editor::theme::Theme;
use iridium_editor::{Editor, KeyPress, KeymapStack};

use super::keymap::default_keymap;

use crate::line::LineBuilder;
use crate::overlay::{PANEL_MAX_VISIBLE_ROWS, PanelAnchor, PanelContent, PanelFit, PanelRow};

/// The characters one level of fork indentation moves a row.
const INDENT_CHARS: usize = 2;

/// The blank characters between a row's text and its age.
const AGE_GAP: usize = 2;

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
    /// so the host recomposes it with the current marker moved.
    Jump(UndoNodeId),
}

/// The undo-tree panel.
///
/// The host owns one of these, toggles it when the kernel reports the
/// `history.togglePanel` host command, hands it every key while it is open,
/// and paints it over the composed frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryPanel {
    /// The kernel's selection model: which node is selected, and which slice
    /// of the rows the window shows.
    pub(super) selection: TreeViewSelection,
    /// The keymap the panel resolves its keys against.
    ///
    /// [`super::keymap::default_keymap`] at the base; a face pushes the user's
    /// `[keys]` bindings on top through
    /// [`set_user_keymap`](Self::set_user_keymap), which filters them to this
    /// panel's verbs.
    pub(super) keys: KeymapStack,
    /// The strokes of a multi-stroke sequence typed so far.
    ///
    /// Almost always empty: the default table is all single strokes, and only a
    /// sequence a *user* bound can leave anything here between presses.
    pub(super) pending: Vec<KeyPress>,
}

impl Default for HistoryPanel {
    /// The same panel [`new`](Self::new) builds.
    ///
    /// ⚠️ **Written out rather than derived.** A derived `Default` would give an
    /// empty [`KeymapStack`], so a panel built that way would answer no keys at
    /// all — a panel that looks constructed and is deaf.
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryPanel {
    /// A closed panel with the default bindings.
    ///
    /// ⚠️ No longer `const`: the panel now owns a keymap, and building one
    /// allocates. It is built once per session.
    #[must_use]
    pub fn new() -> Self {
        Self {
            selection: TreeViewSelection::with_window(PANEL_MAX_VISIBLE_ROWS),
            keys: KeymapStack::with_base(default_keymap()),
            pending: Vec::new(),
        }
    }

    /// Resets the panel for opening: the selection follows the current node.
    pub fn open(&mut self) {
        self.selection.reset();
        // A leader stroke pressed before the panel last closed means nothing
        // now, and leaving it would read the user's next key as its
        // continuation — the same reason the editor's resolver resets on blur.
        self.pending.clear();
    }

    /// Composes the panel for painting.
    ///
    /// `&mut self` because composition is where the scroll window follows the
    /// selection and the page size is learned, exactly as in the palette.
    pub fn content(&mut self, editor: &Editor, theme: &Theme, fit: PanelFit) -> PanelContent {
        let snapshot = editor.history_snapshot();
        let tree = linearize(&snapshot);
        let visible = tree
            .len()
            .clamp(1, PANEL_MAX_VISIBLE_ROWS)
            .min(fit.max_interior_rows);
        self.selection.follow(&tree, visible.min(tree.len()));
        let selected_row = self.selection.selected_row(&tree);
        let scroll = self.selection.scroll();

        let mut rows = Vec::with_capacity(visible);
        for index in 0..visible {
            match tree.get(scroll + index) {
                Some(entry) => {
                    let selected = selected_row == Some(scroll + index);
                    rows.push(tree_row(entry, selected, theme, fit.content_columns));
                },
                None => rows.push(PanelRow::new(Vec::new())),
            }
        }
        PanelContent {
            anchor: PanelAnchor::Top,
            content_columns: fit.content_columns,
            rows,
            caret: None,
            hovered: None,
        }
    }

    /// What a press on composed row `row` does: the row becomes the selection
    /// and the document jumps to it, which is what `Enter` does to it.
    ///
    /// This panel has no query field, so composed row `n` is list row
    /// `scroll + n` with no offset — the one panel here where those two counts
    /// coincide, and worth stating rather than leaving the reader to notice
    /// the missing `- 1`.
    pub fn click_row(&mut self, row: usize, editor: &Editor) -> HistoryOutcome {
        let snapshot = editor.history_snapshot();
        let rows = linearize(&snapshot);
        let index = self.selection.scroll().saturating_add(row);
        if !self.selection.select_row(&rows, index) {
            return HistoryOutcome::Handled;
        }
        self.selection
            .selected_node(&rows)
            .map_or(HistoryOutcome::Handled, HistoryOutcome::Jump)
    }

    /// Moves the window `delta` rows without touching the selection.
    pub const fn scroll_rows(&mut self, delta: isize) {
        self.selection.scroll_rows(delta);
    }

    /// Moves the selection by `delta` rows, clamping at both ends.
    pub(super) fn move_selection(
        &mut self,
        rows: &[TreeViewRow<'_>],
        delta: isize,
    ) -> HistoryOutcome {
        self.selection.move_by(rows, delta);
        HistoryOutcome::Handled
    }
}

/// Composes one tree row: indent, marker, label, age.
fn tree_row(entry: &TreeViewRow<'_>, selected: bool, theme: &Theme, width: usize) -> PanelRow {
    // The current node reads loudest, the active path normally, parked
    // branches dimmed — so the eye finds "where am I" before anything else.
    let voice = if entry.node.is_current {
        theme.editor.line_number_active
    } else if entry.on_active_path {
        theme.editor.foreground
    } else {
        theme.editor.line_number
    };
    let quiet = theme.editor.line_number;

    let age = age_text(entry.node.elapsed_ms);
    let age_chars = age.chars().count();
    let text_width = width.saturating_sub(age_chars + AGE_GAP);

    // The indent is capped so a pathological fork ladder cannot push every
    // label off the panel's right edge.
    let indent = (entry.indent * INDENT_CHARS).min(width / 2);
    let marker = if entry.node.is_current { "* " } else { "o " };

    let mut line = LineBuilder::new(text_width);
    line.pad_to(indent, voice);
    line.push(marker, voice);
    line.push(&label_text(entry.node), voice);
    let used = line.used();
    let mut spans = line.finish();
    if width > age_chars {
        spans.push(crate::overlay::Span::new(
            " ".repeat(width - age_chars - used),
            quiet,
        ));
        spans.push(crate::overlay::Span::new(age, quiet));
    }
    if selected {
        PanelRow::selected(spans)
    } else {
        PanelRow::new(spans)
    }
}

/// The label of one node: what it is, and whether it forks.
fn label_text(node: &iridium_editor::history::UndoNodeInfo) -> String {
    let name = match node.description.as_deref() {
        Some(description) => description,
        None if node.parent_id.is_none() => "start",
        None => "edit",
    };
    if node.child_ids.len() > 1 {
        format!("{name} ({} branches)", node.child_ids.len())
    } else {
        name.to_owned()
    }
}

/// An age in the shortest honest unit.
pub(super) fn age_text(elapsed_ms: u64) -> String {
    const SECOND: u64 = 1000;
    const MINUTE: u64 = 60 * SECOND;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    if elapsed_ms < SECOND {
        return "now".to_owned();
    }
    if elapsed_ms < MINUTE {
        return format!("{}s", elapsed_ms / SECOND);
    }
    if elapsed_ms < HOUR {
        return format!("{}m", elapsed_ms / MINUTE);
    }
    if elapsed_ms < DAY {
        return format!("{}h", elapsed_ms / HOUR);
    }
    format!("{}d", elapsed_ms / DAY)
}

/// `value` as an `isize`, saturating on a page size no window can reach.
pub(super) fn isize_of(value: usize) -> isize {
    isize::try_from(value).unwrap_or(isize::MAX)
}
