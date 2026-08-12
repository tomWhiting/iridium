//! Drawing the file explorer, in either of its two placements.
//!
//! ```text
//!     ╭──────────────────────────────────────────╮   > src         │ fn main() {
//!     │ > src              tab to edit these rows │   ▾ crates      │     …
//!     │ ▾ crates                                 │   ▾ iridium-p…  │ }
//!     │   ▾ iridium-panel                        │     src         │
//!     ╰──────────────────────────────────────────╯                 │
//!            a popover, floating over the text          a sidebar, taking columns
//! ```
//!
//! The box is [`FloatingBox`], shared with the palette and the undo tree; see
//! [`panel`](crate::frame::panel) for why its corners are round, and for why
//! the band has none to round. Which one is drawn is [`Placement`], and it is
//! the *only* thing that differs between them: the rows themselves come from
//! the same shared builder either way.
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

use crate::cell::{CellBuffer, Style};
use crate::frame::CellPosition;
use crate::frame::line::LineLayout;
use crate::frame::palette::Palette;
use crate::frame::panel::{FloatingBox, SidebarBox, TOP};
use crate::frame::text::{self, TextArea};

/// The furniture a body is painted into: a box that floats, or a band that
/// takes columns from the document.
///
/// ⭐ **One enum rather than two paint functions**, because everything between
/// the first row and the last is identical — the rows, their runs, their
/// widths and their caret are the shared panel's, and the only thing a
/// placement decides is what surrounds them. Two functions would be two places
/// for a row-painting rule to be fixed in.
#[derive(Debug, Clone, Copy)]
pub(super) enum Placement {
    /// A floating box, centred, one row down from the top.
    Popover(FloatingBox),
    /// A full-height band down the left edge.
    Sidebar(SidebarBox),
}

impl Placement {
    /// The area a row's runs are painted into.
    const fn content(self) -> TextArea {
        let (origin, width) = match self {
            Self::Popover(panel_box) => (panel_box.content_origin(), panel_box.content_width()),
            Self::Sidebar(band) => (SidebarBox::CONTENT_ORIGIN, band.content_width()),
        };
        TextArea {
            origin,
            width,
            scroll: 0,
        }
    }

    /// The screen row the first body row lands on.
    ///
    /// A band starts at the top of the screen because it has no border above
    /// it; a box starts below the one it drew.
    const fn first_row(self) -> usize {
        match self {
            Self::Popover(_) => TOP + 1,
            Self::Sidebar(_) => 0,
        }
    }

    /// How many rows the furniture can hold body rows in.
    const fn capacity(self) -> usize {
        match self {
            // A box grows to its content, so the only bound is the body.
            Self::Popover(_) => usize::MAX,
            Self::Sidebar(band) => band.rows,
        }
    }

    /// Draws whatever sits above the first body row.
    fn open(self, buffer: &mut CellBuffer, style: Style) {
        if let Self::Popover(panel_box) = self {
            panel_box.top_border(buffer, TOP, style);
        }
    }

    /// Blanks one row of furniture, edges included.
    fn blank_row(self, buffer: &mut CellBuffer, row: usize, style: Style) {
        match self {
            Self::Popover(panel_box) => panel_box.blank_row(buffer, row, style),
            Self::Sidebar(band) => band.blank_row(buffer, row, style),
        }
    }

    /// Draws a separator across one already-blanked row.
    fn rule_row(self, buffer: &mut CellBuffer, row: usize, style: Style) {
        match self {
            Self::Popover(panel_box) => panel_box.rule_row(buffer, row, style),
            Self::Sidebar(band) => band.rule_row(buffer, row, style),
        }
    }

    /// Draws whatever sits below the last body row, and fills any reserved
    /// space the body did not use.
    ///
    /// ⚠️ **The band's leftover rows are written, not skipped.** The document
    /// is laid out to start *after* the band, so nothing else ever paints those
    /// cells: skip them and the band's rule stops where the list stops, leaving
    /// a full-height column that looks like a short box with the screen's
    /// background below it. The columns were taken either way.
    fn close(self, buffer: &mut CellBuffer, after: usize, style: Style) {
        match self {
            Self::Popover(panel_box) => panel_box.bottom_border(buffer, after, style),
            Self::Sidebar(band) => {
                for row in after..band.rows {
                    band.blank_row(buffer, row, style);
                }
            },
        }
    }
}

/// Paints `body` into its placement and reports where its caret landed.
///
/// `None` when the caret fell outside the rows that were drawn — the same
/// answer the palette gives, and for the same reason: a caret drawn where its
/// row is not is worse than no caret.
pub(super) fn paint(
    body: &PanelBody,
    placement: Placement,
    buffer: &mut CellBuffer,
    styles: &Palette,
) -> Option<CellPosition> {
    let content = placement.content();
    let first = placement.first_row();
    let drawn = body.rows.len().min(placement.capacity());

    let base = styles.overlay();
    placement.open(buffer, base);
    for (index, row) in body.rows.iter().take(drawn).enumerate() {
        let screen_row = first + index;
        placement.blank_row(buffer, screen_row, base);
        if row.separator {
            // A separator is furniture, and furniture is this face's. The
            // shared builder says "a rule goes here" and says nothing about
            // what a rule looks like.
            placement.rule_row(buffer, screen_row, base);
            continue;
        }
        paint_row(buffer, screen_row, content, row, styles);
    }
    placement.close(buffer, first + drawn, base);

    caret_position(body.caret, placement, drawn)
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
    placement: Placement,
    drawn_rows: usize,
) -> Option<CellPosition> {
    let caret = caret?;
    let content = placement.content();
    if caret.row >= drawn_rows || caret.column >= content.width {
        return None;
    }
    Some(CellPosition {
        column: content.origin + caret.column,
        row: placement.first_row() + caret.row,
    })
}
