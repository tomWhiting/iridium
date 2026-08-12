//! The floating panel box the palette and the undo tree share: geometry,
//! rounded borders, blanked rows.
//!
//! # The corners are round on purpose
//!
//! Boxes are drawn with the arc corners `╭ ╮ ╰ ╯` rather than the right-angle
//! `┌ ┐ └ ┘`. All the light box-drawing characters share one caveat: they are
//! East Asian *ambiguous* width, so a terminal configured to render ambiguous
//! characters double-wide will misalign the border. That terminal is already
//! living with every TUI border it meets misaligning the same way, the panel's
//! *content* stays aligned because the buffer's own width model laid it out,
//! and the alternative — an ASCII `+--+` box — was rejected as answering a
//! cosmetic risk with a permanent cost.
//!
//! # One box, two panels
//!
//! The command palette and the undo-tree panel are the same piece of
//! furniture: horizontally centred, one row down from the top, at most
//! [`MAX_WIDTH`] cells wide, declining to draw at all on a screen too small
//! for an honest panel. Sharing the box here is what keeps them from
//! drifting apart the first time one of them is adjusted.

use crate::cell::{CellBuffer, Style};

/// The widest a floating panel gets, borders included.
pub(in crate::frame) const MAX_WIDTH: usize = 64;

/// The narrowest exterior worth drawing. Below this a panel shows nothing
/// honestly — a list four cells wide answers no question — so nothing is
/// drawn at all.
const MIN_WIDTH: usize = 20;

/// The row a floating panel starts on: one below the top edge, so it reads
/// as floating rather than as a title bar.
pub(in crate::frame) const TOP: usize = 1;

/// The most content rows a floating panel shows at once.
pub(in crate::frame) const MAX_VISIBLE_ROWS: usize = 12;

/// A floating panel's horizontal placement, fitted to a screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::frame) struct FloatingBox {
    /// The column of the left border.
    pub(in crate::frame) left: usize,
    /// The exterior width, borders included.
    pub(in crate::frame) width: usize,
}

impl FloatingBox {
    /// Fits a box to a screen, or `None` when the screen cannot show an
    /// honest panel: too narrow for [`MIN_WIDTH`], or too short for the
    /// borders and one content row below [`TOP`].
    pub(in crate::frame) fn fitted(columns: usize, rows: usize) -> Option<Self> {
        let width = columns.min(MAX_WIDTH);
        if width < MIN_WIDTH || rows < TOP + 3 {
            return None;
        }
        Some(Self {
            left: (columns - width) / 2,
            width,
        })
    }

    /// The column content starts at: inside the border, past one pad cell.
    pub(in crate::frame) const fn content_origin(&self) -> usize {
        self.left + 2
    }

    /// The cells available to content between the pads.
    pub(in crate::frame) const fn content_width(&self) -> usize {
        self.width - 4
    }

    /// Paints the top border: `╭───╮`.
    pub(in crate::frame) fn top_border(&self, buffer: &mut CellBuffer, row: usize, style: Style) {
        self.border(buffer, row, '╭', '╮', style);
    }

    /// Paints the bottom border: `╰───╯`.
    pub(in crate::frame) fn bottom_border(
        &self,
        buffer: &mut CellBuffer,
        row: usize,
        style: Style,
    ) {
        self.border(buffer, row, '╰', '╯', style);
    }

    /// Blanks one interior row and draws its side borders.
    pub(in crate::frame) fn blank_row(&self, buffer: &mut CellBuffer, row: usize, style: Style) {
        for offset in 0..self.width {
            buffer.set_str(self.left + offset, row, " ", style);
        }
        buffer.set_str(self.left, row, "│", style);
        buffer.set_str(self.left + self.width - 1, row, "│", style);
    }

    /// Draws a hairline rule across one interior row, joining both sides.
    ///
    /// The shared builders in [`iridium_panel`] emit a separator row to say
    /// "a group ends here" and say nothing about what a separator looks like,
    /// because that is furniture and furniture is a face's. Here it is `├──┤`,
    /// which is the same light box-drawing weight as the surrounding border —
    /// so it reads as part of the panel rather than as content that happens to
    /// be dashes.
    ///
    /// The row must already be blanked; this writes over it.
    pub(in crate::frame) fn rule_row(&self, buffer: &mut CellBuffer, row: usize, style: Style) {
        self.border(buffer, row, '├', '┤', style);
    }

    /// Paints one horizontal border row.
    fn border(&self, buffer: &mut CellBuffer, row: usize, first: char, last: char, style: Style) {
        let mut line = String::with_capacity(self.width * 3);
        line.push(first);
        for _ in 0..self.width.saturating_sub(2) {
            line.push('─');
        }
        line.push(last);
        buffer.set_str(self.left, row, &line, style);
    }
}

/// The widest a sidebar gets. Wide enough for a nested path, narrow enough
/// that the document keeps most of the screen.
const SIDEBAR_WIDTH: usize = 32;

/// A full-height band down the left edge, holding a panel that takes its
/// columns **from** the document rather than floating over it (R4).
///
/// # ⭐ Why this is not a [`FloatingBox`] with different numbers
///
/// A floating box is furniture with an outside: four borders, because it sits
/// *on* something and has to say where it ends. A band has no outside on three
/// of its four edges — it runs into the screen's own left, top and bottom — so
/// it is drawn with **one vertical rule on its right**, which is the only edge
/// that separates it from anything.
///
/// That also answers the corner question this face would otherwise inherit: a
/// panel flush at column zero has no free corners to round, so there is nothing
/// here for a rounded corner to be right or wrong about. The rule is the same
/// light box-drawing weight as [`FloatingBox`]'s borders, so the two read as
/// one furniture set.
///
/// # It stops above the statusline
///
/// The statusline describes the *document* — its name, whether it is dirty —
/// and it keeps the full width. A band that covered its first thirty columns
/// would be a panel reporting on a file it does not contain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::frame) struct SidebarBox {
    /// The exterior width, the rule included. This is the number the document
    /// gives up, and the one that reaches `Chrome::sidebar_columns`.
    pub(in crate::frame) width: usize,
    /// How many rows it spans, counting from row zero.
    pub(in crate::frame) rows: usize,
}

impl SidebarBox {
    /// Fits a band `rows` tall to a screen, or `None` when it cannot afford one.
    ///
    /// ⚠️ **`rows` is what the band may span, not the screen's height** — it is
    /// [`document_rows`](crate::frame::document_rows), the same number the
    /// document gets, so the band cannot cover the statusline or a search panel
    /// that already took its rows from the same place.
    ///
    /// Two refusals, both the same rule [`FloatingBox::fitted`] carries: a band
    /// narrower than [`MIN_WIDTH`] shows no file name honestly, and one with
    /// fewer than two rows shows a query field and nothing to query.
    /// ⚠️ **A refusal here is not a failure** — the caller keeps its popover,
    /// which fits screens a band cannot.
    ///
    /// The band never takes more than half the screen, whatever
    /// [`SIDEBAR_WIDTH`] says: a file tree that leaves the document narrower
    /// than itself has inverted which one is the point.
    pub(in crate::frame) const fn fitted(columns: usize, rows: usize) -> Option<Self> {
        let width = if SIDEBAR_WIDTH < columns / 2 {
            SIDEBAR_WIDTH
        } else {
            columns / 2
        };
        if width < MIN_WIDTH || rows < 2 {
            return None;
        }
        Some(Self { width, rows })
    }

    /// The column content starts at: one pad cell in from the screen's edge.
    ///
    /// A constant and not a method, because unlike a floating box a band has
    /// nowhere else to begin — it is flush at column zero by definition, so
    /// there is no instance for this to depend on.
    pub(in crate::frame) const CONTENT_ORIGIN: usize = 1;

    /// The cells available to content, between the pad and the rule.
    pub(in crate::frame) const fn content_width(self) -> usize {
        self.width - 2
    }

    /// Blanks one row of the band and draws its right-hand rule.
    pub(in crate::frame) fn blank_row(&self, buffer: &mut CellBuffer, row: usize, style: Style) {
        for column in 0..self.width {
            buffer.set_str(column, row, " ", style);
        }
        buffer.set_str(self.width - 1, row, "│", style);
    }

    /// Draws a hairline rule across one row, joining the band's own edge.
    ///
    /// `├` on the right rather than `┤`: the band's rule is its *right* edge,
    /// so a separator meets it from the inside. The left end is `─` and not a
    /// corner, because the screen's edge is not the panel's.
    pub(in crate::frame) fn rule_row(&self, buffer: &mut CellBuffer, row: usize, style: Style) {
        let mut line = String::with_capacity(self.width * 3);
        for _ in 0..self.width.saturating_sub(1) {
            line.push('─');
        }
        line.push('┤');
        buffer.set_str(0, row, &line, style);
    }
}
