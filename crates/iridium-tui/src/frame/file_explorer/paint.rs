//! Drawing the file explorer's floating panel.
//!
//! ```text
//!     ╭──────────────────────────────────────────╮
//!     │ > src              tab to edit these rows │
//!     │ ▾ crates                                 │
//!     │   ▾ iridium-panel                        │
//!     │     src                                  │
//!     ╰──────────────────────────────────────────╯
//! ```
//!
//! The box is [`FloatingBox`], shared with the palette and the undo tree; see
//! [`panel`](crate::frame::panel) for why the corners are round.
//!
//! # ⭐ Nothing here decides what a row says
//!
//! Every row, its indent, its disclosure marker, its truncation, the query
//! field and where the caret sits inside it — all of that is
//! [`iridium_panel::explorer`], the same code the GPU face draws. What is here
//! is the conversion from a [`PanelBody`] to cells, and it is the whole of this
//! face's contribution to the panel.
//!
//! That is the payoff of the hoist rather than an accident of it: a rename
//! refused here is refused there, for the same reason and with the same
//! sentence, because it is one implementation and not two that agree today.
//!
//! # ⚠️ The units seam: the builder budgets in characters, this spends in cells
//!
//! [`PanelFit::content_columns`] counts `char`s, because the GPU face draws on
//! a grid where every character occupies one character-width and a shared crate
//! has no business knowing what a terminal thinks a column is. A terminal's
//! columns are *display cells*, and a CJK name occupies two of them per
//! character.
//!
//! **They are the same number for every ASCII path, and this face clips when
//! they are not.** [`text::paint_text`] advances by display width and refuses
//! to write past the area, so a row composed to fit in characters but too wide
//! in cells is cut at the border — visibly, on its right edge — rather than
//! overrunning the box. That is the contract [`PanelRow`] already states, and
//! it is the honest answer available: teaching the shared crate about display
//! width would apply a *terminal's* width model to the GPU face, where it is
//! simply false.
//!
//! What it costs, stated plainly so nobody has to rediscover it: a directory of
//! wide-character names loses a character or two of its longest rows at the
//! panel edge, and the right-aligned hint on the query row is the first thing
//! to go. Nothing is corrupted and nothing panics.

use iridium_panel::{PanelBody, PanelCaret};

use crate::cell::CellBuffer;
use crate::frame::CellPosition;
use crate::frame::line::LineLayout;
use crate::frame::palette::Palette;
use crate::frame::panel::{FloatingBox, TOP};
use crate::frame::text::{self, TextArea};

/// Paints `body` into a floating box and reports where its caret landed.
///
/// `None` when the screen cannot hold an honest panel, or when the caret fell
/// outside the rows that fit — the same answer the palette gives, and for the
/// same reason: a caret drawn where its row is not is worse than no caret.
pub(super) fn paint(
    body: &PanelBody,
    buffer: &mut CellBuffer,
    styles: &Palette,
) -> Option<CellPosition> {
    let panel_box = FloatingBox::fitted(buffer.width(), buffer.height())?;
    let content = TextArea {
        origin: panel_box.content_origin(),
        width: panel_box.content_width(),
        scroll: 0,
    };

    let base = styles.overlay();
    panel_box.top_border(buffer, TOP, base);
    for (index, row) in body.rows.iter().enumerate() {
        let screen_row = TOP + 1 + index;
        panel_box.blank_row(buffer, screen_row, base);
        if row.separator {
            // A separator is furniture, and furniture is this face's. The
            // shared builder says "a rule goes here" and says nothing about
            // what a rule looks like.
            panel_box.rule_row(buffer, screen_row, base);
            continue;
        }
        paint_row(buffer, screen_row, content, row, styles);
    }
    panel_box.bottom_border(buffer, TOP + 1 + body.rows.len(), base);

    caret_position(body.caret, &panel_box, body.rows.len())
}

/// Paints one row's runs, left to right, each in its own colour.
///
/// The selected row takes the selection background across the full content
/// width first, so the tint reaches the blank cells past the text rather than
/// stopping where the name does — a half-tinted row reads as a rendering fault
/// rather than as a selection.
fn paint_row(
    buffer: &mut CellBuffer,
    row: usize,
    content: TextArea,
    panel_row: &iridium_panel::PanelRow,
    styles: &Palette,
) {
    if panel_row.selected {
        let selected = styles.selected(styles.overlay());
        for offset in 0..content.width {
            buffer.set_str(content.origin + offset, row, " ", selected);
        }
    }

    let mut column = 0;
    for span in &panel_row.spans {
        let style = styles.overlay_span(span.color);
        let style = if panel_row.selected {
            styles.selected(style)
        } else {
            style
        };
        text::paint_text(buffer, row, column, &span.text, style, content);
        // ⚠️ Advanced by DISPLAY WIDTH, not by character count. The builder
        // counted characters to lay the row out; the next run has to start
        // where the last one actually ended on this grid, or a row with one
        // wide glyph in it puts every following run one cell to the left.
        // A tab cannot reach here — these are composed runs, not document
        // text — so a tab width of one is the identity and not a choice.
        column += LineLayout::new(&span.text, 1).width();
        if column >= content.width {
            break;
        }
    }
}

/// Where the caret goes on screen, or `None` when its row was not drawn.
fn caret_position(
    caret: Option<PanelCaret>,
    panel_box: &FloatingBox,
    drawn_rows: usize,
) -> Option<CellPosition> {
    let caret = caret?;
    if caret.row >= drawn_rows || caret.column >= panel_box.content_width() {
        return None;
    }
    Some(CellPosition {
        column: panel_box.content_origin() + caret.column,
        row: TOP + 1 + caret.row,
    })
}
