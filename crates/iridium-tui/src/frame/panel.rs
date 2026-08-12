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
