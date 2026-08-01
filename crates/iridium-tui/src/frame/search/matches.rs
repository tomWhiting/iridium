//! Where the search's matches fall on the lines a frame paints.
//!
//! The ranges are the kernel's. Nothing here searches, decides what a match is,
//! or knows which one is current — that is all
//! [`SearchState`](iridium_editor::search::SearchState), read through
//! [`Editor::search_state`]. This module only turns document positions into
//! cells and asks the palette for a background.
//!
//! # Why the matches are gathered once per frame
//!
//! A match list is unbounded: searching `e` across a hundred-thousand-line file
//! is tens of thousands of ranges, and asking "which of these touch line *n*"
//! once per painted row would be that list times the height of the screen every
//! keystroke. One pass per frame collects the few that touch the painted span,
//! and each row then scans only those.
//!
//! The pass is linear rather than a binary search deliberately. The kernel
//! produces matches in ascending start order, but nothing in its API *promises*
//! that, and a binary search that silently assumed it would drop matches rather
//! than fail — the kind of wrong answer that looks like a rendering glitch. A
//! linear filter over a list that is already in memory costs a comparison per
//! match and cannot be wrong about ordering it was never told.

use iridium_editor::{Editor, Range};

use crate::cell::CellBuffer;
use crate::frame::line::LineLayout;
use crate::frame::palette::Palette;
use crate::frame::text::{self, TextArea};

/// The search matches that touch the lines one frame paints.
#[derive(Debug, Clone, Default)]
pub(in crate::frame) struct MatchHighlights {
    /// Each match in the painted span, and whether it is the current one.
    matches: Vec<(Range, bool)>,
}

impl MatchHighlights {
    /// The matches touching `first_line..=last_line`, current one marked.
    ///
    /// An inactive search has no matches, so this is empty whenever the user is
    /// not searching and no caller needs to ask whether they are.
    pub(in crate::frame) fn collect(editor: &Editor, first_line: usize, last_line: usize) -> Self {
        let state = editor.search_state();
        let current = editor.current_match_index();
        let matches = state
            .all_matches()
            .iter()
            .enumerate()
            .filter(|(_, range)| range.start.line <= last_line && range.end.line >= first_line)
            .map(|(index, range)| (*range, current == Some(index)))
            .collect();
        Self { matches }
    }

    /// Paints every match on one document line.
    ///
    /// The current match is painted last so that it wins wherever two matches
    /// overlap — the kernel's literal search reports overlapping matches, so
    /// `aa` in `aAa` is two ranges sharing a cell, and the one the user is
    /// standing on is the one that must be visible.
    pub(in crate::frame) fn paint(
        &self,
        buffer: &mut CellBuffer,
        row: usize,
        line: MatchedLine<'_>,
        palette: &Palette,
        area: TextArea,
    ) {
        for &(range, is_current) in &self.matches {
            if !is_current {
                paint_one(buffer, row, line, range, is_current, palette, area);
            }
        }
        for &(range, is_current) in &self.matches {
            if is_current {
                paint_one(buffer, row, line, range, is_current, palette, area);
            }
        }
    }
}

/// The document line a match is being resolved against, with its cell layout.
///
/// The two travel together — a range is turned into cells *by* the layout of
/// the line it is on — so they are one parameter rather than two that a caller
/// could pair up wrongly.
#[derive(Debug, Clone, Copy)]
pub(in crate::frame) struct MatchedLine<'a> {
    /// The document line number.
    pub(in crate::frame) number: usize,
    /// That line's clusters, placed in cells.
    pub(in crate::frame) layout: &'a LineLayout,
}

/// Paints one match's contribution to one line.
///
/// A match that continues onto the next line is drawn one cell past the end of
/// this one: that cell is the line break, and a multi-line regex match covers
/// it. This is the same rule selections are painted by, for the same reason.
fn paint_one(
    buffer: &mut CellBuffer,
    row: usize,
    line: MatchedLine<'_>,
    range: Range,
    is_current: bool,
    palette: &Palette,
    area: TextArea,
) {
    if line.number < range.start.line || line.number > range.end.line {
        return;
    }
    let from = if line.number == range.start.line {
        line.layout.column_to_cell(range.start.column)
    } else {
        0
    };
    let to = if line.number == range.end.line {
        line.layout.column_to_cell(range.end.column)
    } else {
        line.layout.width() + 1
    };
    // A zero-width match — an anchor such as `^` in regex mode — covers no
    // cell. Marking one cell anyway would claim the character beside it was
    // found, which it was not.
    if from >= to {
        return;
    }
    text::restyle_columns(buffer, row, from..to, area, |style| {
        palette.search_match(style, is_current)
    });
}
