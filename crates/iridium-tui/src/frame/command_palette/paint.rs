//! Drawing the command palette's floating panel.
//!
//! ```text
//!     ╭──────────────────────────────────────────╮
//!     │ > dup                                    │
//!     │ Duplicate Line Down       Shift+Alt+Down │
//!     │ Duplicate Line Up           Shift+Alt+Up │
//!     ╰──────────────────────────────────────────╯
//! ```
//!
//! The box itself — placement, rounded corners, minimum honest size — is
//! [`FloatingBox`], shared with the undo-tree panel; see
//! [`panel`](crate::frame::panel) for why the corners are round and what that
//! costs on an ambiguous-width terminal.
//!
//! # What a row shows
//!
//! The command's title; the key that runs it, right-aligned and dropped before
//! it would collide with the title; and, when the query matched something other
//! than the title — an alias, the id, the category, the description — that
//! matched text as a dim annotation after the title, because highlighting match
//! positions inside a string that is not shown would underline nothing.
//!
//! Match positions are highlighted with the search-match background, on
//! exactly the characters the kernel reported. The kernel reports *character*
//! positions into [`PaletteEntry::matched_text`]; cells are found by mapping
//! each position through the same [`LineLayout`] that painted the text, so a
//! double-width or combining title highlights the right glyphs.

use iridium_editor::commands::palette::{self, CommandMru, MatchField, PaletteEntry};
use iridium_editor::{Editor, KeyLabelStyle};

use super::CommandPalette;
use crate::cell::{Cell, CellBuffer};
use crate::frame::CellPosition;
use crate::frame::field::scroll_for;
use crate::frame::line::LineLayout;
use crate::frame::palette::Palette;
use crate::frame::panel::{FloatingBox, MAX_VISIBLE_ROWS, TOP};
use crate::frame::text::{self, TextArea};

/// The query prompt, drawn before the field.
const PROMPT: &str = "> ";

/// The fewest cells the title keeps before the key hint is dropped.
const TITLE_FLOOR: usize = 8;

/// The blank cells between the title (or annotation) and the key hint.
const HINT_GAP: usize = 2;

/// The blank cells between the title and a non-title match annotation.
const ANNOTATION_GAP: usize = 2;

/// Paints the panel and returns where the query caret landed.
pub(super) fn paint(
    panel: &mut CommandPalette,
    buffer: &mut CellBuffer,
    editor: &Editor,
    mru: &CommandMru,
    styles: &Palette,
) -> Option<CellPosition> {
    let rows = buffer.height();
    let panel_box = FloatingBox::fitted(buffer.width(), rows)?;
    let content = TextArea {
        origin: panel_box.content_origin(),
        width: panel_box.content_width(),
        scroll: 0,
    };

    let results = palette::search_text(editor.commands(), mru, panel.query(), None);
    // An empty list still gets one row, to say so.
    let list_rows = results.len().max(1);
    let visible = list_rows.min(MAX_VISIBLE_ROWS).min(rows - TOP - 3);
    panel.follow_selection(results.len(), visible.min(results.len()));

    let base = styles.overlay();
    panel_box.top_border(buffer, TOP, base);
    let caret = input_row(panel, buffer, TOP + 1, panel_box, content, styles);
    for index in 0..visible {
        let row = TOP + 2 + index;
        panel_box.blank_row(buffer, row, base);
        match results.get(panel.scroll() + index) {
            Some(entry) => {
                let selected = panel.scroll() + index == panel.clamped_selection(results.len());
                result_row(buffer, row, content, entry, selected, editor, styles);
            },
            None => {
                text::paint_text(
                    buffer,
                    row,
                    0,
                    "No matching commands",
                    styles.overlay_quiet(),
                    content,
                );
            },
        }
    }
    panel_box.bottom_border(buffer, TOP + 2 + visible, base);
    caret
}

/// Paints the query row and returns where its caret landed.
fn input_row(
    panel: &CommandPalette,
    buffer: &mut CellBuffer,
    row: usize,
    panel_box: FloatingBox,
    content: TextArea,
    styles: &Palette,
) -> Option<CellPosition> {
    panel_box.blank_row(buffer, row, styles.overlay());
    buffer.set_str(content.origin, row, PROMPT, styles.overlay_quiet());

    let prompt_cells = LineLayout::new(PROMPT, 1).width();
    let field_width = content.width.saturating_sub(prompt_cells);
    if field_width == 0 {
        return None;
    }
    let caret = panel.field().caret_cell();
    let area = TextArea {
        origin: content.origin + prompt_cells,
        width: field_width,
        scroll: scroll_for(caret, field_width),
    };
    panel.field().paint(buffer, row, area, styles.overlay());
    Some(CellPosition {
        column: area.origin + (caret - area.scroll),
        row,
    })
}

/// Paints one result: title, match highlighting, annotation, key hint.
fn result_row(
    buffer: &mut CellBuffer,
    row: usize,
    content: TextArea,
    entry: &PaletteEntry<'_>,
    selected: bool,
    editor: &Editor,
    styles: &Palette,
) {
    let (base, quiet) = if selected {
        (
            styles.selected(styles.overlay()),
            styles.selected(styles.overlay_quiet()),
        )
    } else {
        (styles.overlay(), styles.overlay_quiet())
    };
    if selected {
        for offset in 0..content.width {
            buffer.set_str(content.origin + offset, row, " ", base);
        }
    }

    let hint: Option<&str> = editor
        .key_hints()
        .primary_hint(entry.id().as_str())
        .map(|hint| hint.label(KeyLabelStyle::Portable));
    let hint_cells = hint.map_or(0, |label| LineLayout::new(label, 1).width());
    let hint_fits = hint_cells > 0 && content.width >= TITLE_FLOOR + HINT_GAP + hint_cells;
    let text_width = if hint_fits {
        content.width - HINT_GAP - hint_cells
    } else {
        content.width
    };
    let text_area = TextArea {
        origin: content.origin,
        width: text_width,
        scroll: 0,
    };

    text::paint_text(buffer, row, 0, entry.title(), base, text_area);
    let title_cells = LineLayout::new(entry.title(), 1).width();

    match entry.matched_field() {
        None => {},
        Some(MatchField::Title) => {
            highlight_matches(
                buffer,
                row,
                entry.title(),
                entry.matches(),
                0,
                text_area,
                styles,
            );
        },
        Some(_) => {
            // The match landed in text the title does not show; show that text
            // so the highlight has something true to sit on.
            let start = title_cells + ANNOTATION_GAP;
            text::paint_text(buffer, row, start, entry.matched_text(), quiet, text_area);
            highlight_matches(
                buffer,
                row,
                entry.matched_text(),
                entry.matches(),
                start,
                text_area,
                styles,
            );
        },
    }

    if hint_fits {
        if let Some(label) = hint {
            buffer.set_str(
                content.origin + content.width - hint_cells,
                row,
                label,
                quiet,
            );
        }
    }
}

/// Restyles the cells of the matched characters with the match background.
///
/// `text` is what was painted starting `offset` cells into `area`, and
/// `positions` are the kernel's character positions into it. Every position is
/// mapped through the text's own layout, so combining sequences highlight
/// their one cell and double-width glyphs highlight both halves —
/// [`CellBuffer::set_style`] covers the pair.
fn highlight_matches(
    buffer: &mut CellBuffer,
    row: usize,
    text: &str,
    positions: &[u32],
    offset: usize,
    area: TextArea,
    styles: &Palette,
) {
    if positions.is_empty() {
        return;
    }
    let layout = LineLayout::new(text, 1);
    let clusters = layout.clusters();
    let bytes: Vec<usize> = text.char_indices().map(|(byte, _)| byte).collect();
    for &position in positions {
        let Some(&byte) = bytes.get(position as usize) else {
            continue;
        };
        // The last cluster whose start is at or before the character's byte is
        // the cluster that contains it.
        let index = clusters
            .partition_point(|cluster| cluster.byte_index() <= byte)
            .checked_sub(1);
        let Some(cluster) = index.and_then(|index| clusters.get(index)) else {
            continue;
        };
        let column = offset + cluster.column();
        if column >= area.width {
            continue;
        }
        let cell = area.origin + column;
        let Some(existing) = buffer.get(cell, row).map(Cell::style) else {
            continue;
        };
        buffer.set_style(cell, row, styles.search_match(existing, false));
    }
}
