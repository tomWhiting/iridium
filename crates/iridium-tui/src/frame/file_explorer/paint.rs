//! Drawing the file explorer, in either of its two placements.
//!
//! ```text
//!   ╭──────────────────────────────────────────────╮     > src         │ fn main() {
//!   │ > src                  tab to edit these rows │     ▾ crates      │     …
//!   │ ▾ crates                                     │     ▾ iridium-p…  │ }
//!   │   ▾ iridium-panel                            │       src         │
//!   │                                              │                   │
//!   ╰──────────────────────────────────────────────╯                   │
//!        floating: large, centred, a fixed size        a sidebar, taking columns
//! ```
//!
//! Which one is drawn is [`Placement`], and it is the *only* thing that
//! differs between them: the rows come from the same shared builder either way.
//!
//! # ⭐ Nothing here decides what a row says
//!
//! Every row, its indent, its marker, its truncation, the query field, the text
//! being edited and where the caret sits inside it — all of that is
//! [`iridium_panel::explorer`], the same code the GPU face draws. What is here
//! is the conversion from a [`PanelBody`] to cells.
//!
//! # ⚠️ The units seam: the builder budgets in characters, this spends in cells
//!
//! [`PanelFit::content_columns`] counts `char`s, because the GPU face draws on
//! a grid where every character occupies one character-width. A terminal's
//! columns are *display cells*, and a CJK name occupies two of them per
//! character. **They are the same number for every ASCII path, and this face
//! clips when they are not**: [`text::paint_text`] advances by display width
//! and refuses to write past the area, so a row too wide in cells is cut at the
//! border rather than overrunning the box. Marks are mapped from characters to
//! cells the same way, so a selection covers the glyphs it names.
//!
//! [`PanelFit::content_columns`]: iridium_panel::PanelFit::content_columns

use iridium_panel::{MarkKind, PanelBody, PanelCaret, PanelMark};
use unicode_width::UnicodeWidthChar as _;

use crate::cell::{CellBuffer, Style};
use crate::frame::CellPosition;
use crate::frame::line::LineLayout;
use crate::frame::palette::Palette;
use crate::frame::panel::{CentredBox, SidebarBox};
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
    /// A large floating box, centred both ways.
    Centred(CentredBox),
    /// A full-height band down the left edge.
    Sidebar(SidebarBox),
}

impl Placement {
    /// The area a row's runs are painted into.
    const fn content(self) -> TextArea {
        let (origin, width) = match self {
            Self::Centred(centred) => (
                centred.frame.content_origin(),
                centred.frame.content_width(),
            ),
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
            Self::Centred(centred) => centred.top + 1,
            Self::Sidebar(_) => 0,
        }
    }

    /// How many rows the furniture can hold body rows in.
    const fn capacity(self) -> usize {
        match self {
            Self::Centred(centred) => centred.interior_rows,
            Self::Sidebar(band) => band.rows,
        }
    }

    /// Draws whatever sits above the first body row.
    fn open(self, buffer: &mut CellBuffer, style: Style) {
        if let Self::Centred(centred) = self {
            centred.frame.top_border(buffer, centred.top, style);
        }
    }

    /// Blanks one row of furniture, edges included.
    fn blank_row(self, buffer: &mut CellBuffer, row: usize, style: Style) {
        match self {
            Self::Centred(centred) => centred.frame.blank_row(buffer, row, style),
            Self::Sidebar(band) => band.blank_row(buffer, row, style),
        }
    }

    /// Draws a separator across one already-blanked row.
    fn rule_row(self, buffer: &mut CellBuffer, row: usize, style: Style) {
        match self {
            Self::Centred(centred) => centred.frame.rule_row(buffer, row, style),
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
            Self::Centred(centred) => {
                let bottom = centred.top + 1 + centred.interior_rows;
                for row in after..bottom {
                    centred.frame.blank_row(buffer, row, style);
                }
                centred.frame.bottom_border(buffer, bottom, style);
            },
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
    if let Placement::Sidebar(band) = placement {
        for row in 0..band.rows {
            buffer.set_style(band.width - 1, row, styles.overlay_quiet());
        }
    }

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

    paint_marks(buffer, row, content, panel_row, styles);
}

/// Paints each mark's background over the cells its characters landed in.
///
/// A mark past the end of the text — a selected line break, a caret at the
/// end of a line — covers one blank cell per character, which is what the
/// row was padded with.
fn paint_marks(
    buffer: &mut CellBuffer,
    row: usize,
    content: TextArea,
    panel_row: &iridium_panel::PanelRow,
    styles: &Palette,
) {
    if panel_row.marks.is_empty() {
        return;
    }
    let text = panel_row.text();
    let widths: Vec<usize> = text
        .chars()
        .map(|character| character.width().unwrap_or(0))
        .collect();
    let cell_of = |column: usize| -> usize {
        let within: usize = widths.iter().take(column).sum();
        within + column.saturating_sub(widths.len())
    };
    for mark in &panel_row.marks {
        let (first, last) = (cell_of(mark.start), cell_of(mark.end));
        for cell in first..last.min(content.width) {
            let column = content.origin + cell;
            let Some(current) = buffer.get(column, row).map(crate::cell::Cell::style) else {
                continue;
            };
            buffer.set_style(column, row, marked(*mark, current, styles));
        }
    }
}

/// `style` with the background a mark of this kind paints.
const fn marked(mark: PanelMark, style: Style, styles: &Palette) -> Style {
    match mark.kind {
        MarkKind::Selection => styles.selected(style),
        MarkKind::Match => styles.search_match(style, false),
        MarkKind::CurrentMatch => styles.search_match(style, true),
        MarkKind::Caret => styles.caret(style),
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
