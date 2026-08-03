//! Drawing the undo-tree panel.
//!
//! ```text
//!     ╭──────────────────────────────────────────╮
//!     │ o start                              5m  │
//!     │ o edit (2 branches)                  4m  │
//!     │   o edit                             4m  │
//!     │   * edit                            30s  │
//!     ╰──────────────────────────────────────────╯
//! ```
//!
//! The box is [`FloatingBox`], shared with the palette. Rows come from
//! [`linearize`](super::linearize): root at the top, time downward, forks
//! indenting their children. `*` is where the document is now; bright rows
//! are the active undo/redo path; dim rows are parked branches; ages sit
//! right-aligned.

use iridium_editor::Editor;

use super::{HistoryPanel, TreeRow, linearize};
use crate::cell::CellBuffer;
use crate::frame::line::LineLayout;
use crate::frame::palette::Palette;
use crate::frame::panel::{FloatingBox, MAX_VISIBLE_ROWS, TOP};
use crate::frame::text::{self, TextArea};

/// The cells one level of fork indentation moves a row.
const INDENT_CELLS: usize = 2;

/// The blank cells between a row's text and its age.
const AGE_GAP: usize = 2;

/// Paints the panel.
pub(super) fn paint(
    panel: &mut HistoryPanel,
    buffer: &mut CellBuffer,
    editor: &Editor,
    styles: &Palette,
) {
    let rows = buffer.height();
    let Some(panel_box) = FloatingBox::fitted(buffer.width(), rows) else {
        return;
    };
    let content = TextArea {
        origin: panel_box.content_origin(),
        width: panel_box.content_width(),
        scroll: 0,
    };

    let snapshot = editor.history_snapshot();
    let tree = linearize(&snapshot);
    let visible = tree.len().clamp(1, MAX_VISIBLE_ROWS).min(rows - TOP - 3);
    panel.follow_selection(&tree, visible.min(tree.len()));
    let selected_row = panel.selected_row(&tree);

    let base = styles.overlay();
    panel_box.top_border(buffer, TOP, base);
    for index in 0..visible {
        let row = TOP + 1 + index;
        panel_box.blank_row(buffer, row, base);
        if let Some(entry) = tree.get(panel.scroll + index) {
            let selected = selected_row == Some(panel.scroll + index);
            tree_row(buffer, row, content, entry, selected, styles);
        }
    }
    panel_box.bottom_border(buffer, TOP + 1 + visible, base);
}

/// Paints one tree row: indent, marker, label, age.
fn tree_row(
    buffer: &mut CellBuffer,
    row: usize,
    content: TextArea,
    entry: &TreeRow<'_>,
    selected: bool,
    styles: &Palette,
) {
    // The current node reads loudest, the active path normally, parked
    // branches dimmed — so the eye finds "where am I" before anything else.
    let voice = if entry.node.is_current {
        styles.overlay_toggle(true)
    } else if entry.on_active_path {
        styles.overlay()
    } else {
        styles.overlay_quiet()
    };
    let (voice, quiet) = if selected {
        (
            styles.selected(voice),
            styles.selected(styles.overlay_quiet()),
        )
    } else {
        (voice, styles.overlay_quiet())
    };
    if selected {
        for offset in 0..content.width {
            buffer.set_str(content.origin + offset, row, " ", voice);
        }
    }

    let age = age_text(entry.node.elapsed_ms);
    let age_cells = LineLayout::new(&age, 1).width();

    // The indent is capped so a pathological fork ladder cannot push every
    // label off the panel's right edge.
    let indent = (entry.indent * INDENT_CELLS).min(content.width / 2);
    let marker = if entry.node.is_current { "* " } else { "o " };
    let label = label_text(entry.node);

    let text_width = content.width.saturating_sub(age_cells + AGE_GAP);
    let text_area = TextArea {
        origin: content.origin,
        width: text_width,
        scroll: 0,
    };
    text::paint_text(buffer, row, indent, marker, voice, text_area);
    text::paint_text(buffer, row, indent + 2, &label, voice, text_area);

    if content.width > age_cells {
        buffer.set_str(content.origin + content.width - age_cells, row, &age, quiet);
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
fn age_text(elapsed_ms: u64) -> String {
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

#[cfg(test)]
mod tests {
    use super::age_text;

    #[test]
    fn ages_use_the_shortest_honest_unit() {
        assert_eq!(age_text(0), "now");
        assert_eq!(age_text(999), "now");
        assert_eq!(age_text(1_000), "1s");
        assert_eq!(age_text(59_999), "59s");
        assert_eq!(age_text(60_000), "1m");
        assert_eq!(age_text(3_599_999), "59m");
        assert_eq!(age_text(3_600_000), "1h");
        assert_eq!(age_text(86_400_000), "1d");
    }
}
