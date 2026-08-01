//! Painting one line's glyphs into the text area, and restyling ranges of it.
//!
//! # A line wider than the window
//!
//! The terminal face does not wrap. The kernel's fold-aware viewport yields one
//! *document* line per screen row, and soft wrapping would mean a second
//! mapping from document lines to rows — the competing layout model that
//! `docs/TERMINAL-FACE-PLAN.md` decision 1 exists to forbid. A long line is
//! therefore clipped to the window, and the window is moved by the kernel's own
//! horizontal scroll (`Viewport::scroll_offset_x`, read in cells).
//!
//! # A cut through the middle of a glyph
//!
//! Horizontal scroll and the right-hand edge can both fall inside a
//! double-width glyph. Half a glyph is a state no terminal can render — see
//! [`crate::cell`] — so a glyph that is not wholly inside the window is painted
//! as a space in its own style. It keeps its cell, its background and the
//! alignment of everything after it; only its shape is lost, and only while the
//! cut runs through it.
//!
//! A tab is a run of spaces rather than a glyph, so a cut through one paints
//! whatever part of the run is visible.

use core::ops::Range;

use unicode_segmentation::UnicodeSegmentation as _;

use super::line::PlacedCluster;
use crate::cell::{CellBuffer, Grapheme, Style, WriteOutcome};

/// The rectangle of cells that document text is painted into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextArea {
    /// The buffer column the text area starts at, just past the gutter.
    pub origin: usize,
    /// The number of cells the text area is wide.
    pub width: usize,
    /// The first display column of the line that is visible, in cells.
    pub scroll: usize,
}

impl TextArea {
    /// The display column just past the last visible one.
    const fn end(self) -> usize {
        self.scroll + self.width
    }

    /// Where a run of display columns lands in the buffer, if anywhere.
    ///
    /// Returns the buffer column of the run's first visible cell and how many
    /// of its cells are visible.
    fn clip(self, column: usize, width: usize) -> Option<(usize, usize)> {
        if width == 0 || self.width == 0 {
            return None;
        }
        let start = column.max(self.scroll);
        let end = (column + width).min(self.end());
        if start >= end {
            return None;
        }
        Some((self.origin + (start - self.scroll), end - start))
    }
}

/// Paints one cluster of a line.
pub fn paint_cluster(
    buffer: &mut CellBuffer,
    row: usize,
    cluster: &PlacedCluster,
    style: Style,
    area: TextArea,
) {
    let Some((column, visible)) = area.clip(cluster.column(), cluster.width()) else {
        return;
    };

    if cluster.is_tab() {
        fill(buffer, column, row, visible, style);
        return;
    }

    let Some(grapheme) = cluster.grapheme() else {
        return;
    };

    if visible < cluster.width() {
        // The cut runs through this glyph. Its shape cannot be shown in fewer
        // cells than it occupies, so the cells it still holds become spaces.
        fill(buffer, column, row, visible, style);
        return;
    }

    match buffer.set_grapheme(column, row, grapheme, style) {
        WriteOutcome::Written(_) => {},
        // Unreachable: the clip above proved the glyph fits inside the area,
        // and the area is inside the buffer. Blanking the cell rather than
        // leaving it is what keeps a wrong answer here from showing the
        // previous frame's glyph.
        WriteOutcome::Truncated | WriteOutcome::OutOfBounds => {
            fill(buffer, column, row, visible, style);
        },
    }
}

/// Paints plain text starting at a display column, clipped to the area.
///
/// Used for the fold placeholder, which is the face's own text rather than the
/// document's, and so has no cluster layout of its own.
pub fn paint_text(
    buffer: &mut CellBuffer,
    row: usize,
    column: usize,
    text: &str,
    style: Style,
    area: TextArea,
) {
    let mut column = column;
    for cluster in text.graphemes(true) {
        let Some(grapheme) = Grapheme::new(cluster) else {
            continue;
        };
        let width = grapheme.width();
        if let Some((buffer_column, visible)) = area.clip(column, width) {
            if visible < width {
                fill(buffer, buffer_column, row, visible, style);
            } else {
                match buffer.set_grapheme(buffer_column, row, &grapheme, style) {
                    WriteOutcome::Written(_) => {},
                    WriteOutcome::Truncated | WriteOutcome::OutOfBounds => {
                        fill(buffer, buffer_column, row, visible, style);
                    },
                }
            }
        } else if column >= area.end() {
            return;
        }
        column += width;
    }
}

/// Applies `restyle` to every cell of a display-column range that is visible.
///
/// A range that ends inside a double-width glyph restyles the whole glyph: the
/// terminal paints a glyph with one style, and the cell buffer enforces that
/// both halves agree, so there is no half to leave alone.
pub fn restyle_columns(
    buffer: &mut CellBuffer,
    row: usize,
    columns: Range<usize>,
    area: TextArea,
    restyle: impl Fn(Style) -> Style,
) {
    if columns.start >= columns.end {
        return;
    }
    let Some((column, visible)) = area.clip(columns.start, columns.end - columns.start) else {
        return;
    };
    for offset in 0..visible {
        let Some(cell) = buffer.get(column + offset, row) else {
            continue;
        };
        let style = restyle(cell.style());
        buffer.set_style(column + offset, row, style);
    }
}

/// Fills `count` cells with blanks in `style`.
fn fill(buffer: &mut CellBuffer, column: usize, row: usize, count: usize, style: Style) {
    for offset in 0..count {
        buffer.set_str(column + offset, row, " ", style);
    }
}

#[cfg(test)]
mod tests {
    use super::super::line::LineLayout;
    use super::*;
    use crate::cell::{Attributes, Cell, CellContent, Color};

    /// The area covering a whole buffer of `width` cells, unscrolled.
    const fn whole(width: usize) -> TextArea {
        TextArea {
            origin: 0,
            width,
            scroll: 0,
        }
    }

    /// The text of a row, with continuation cells shown as an empty string.
    fn row_text(buffer: &CellBuffer, row: usize) -> String {
        let mut out = String::new();
        let Some(cells) = buffer.row(row) else {
            return out;
        };
        for cell in cells {
            match cell.content() {
                CellContent::Grapheme(grapheme) => grapheme.push_to(&mut out),
                CellContent::Continuation => {},
            }
        }
        out
    }

    /// Paints every cluster of `text` into a fresh buffer of `width` cells.
    fn painted(text: &str, width: usize, area: TextArea) -> CellBuffer {
        let mut buffer = CellBuffer::new(width, 1);
        let layout = LineLayout::new(text, 4);
        for cluster in layout.clusters() {
            paint_cluster(&mut buffer, 0, cluster, Style::DEFAULT, area);
        }
        buffer
    }

    #[test]
    fn a_line_that_fits_is_painted_whole() {
        let buffer = painted("hello", 10, whole(10));
        assert_eq!(row_text(&buffer, 0), "hello     ");
    }

    #[test]
    fn a_double_width_glyph_takes_two_cells_with_a_continuation() {
        let buffer = painted("a漢b", 10, whole(10));
        assert_eq!(buffer.get(1, 0).map(Cell::columns), Some(2));
        assert!(buffer.get(2, 0).is_some_and(Cell::is_continuation));
        assert_eq!(buffer.get(3, 0).map(Cell::columns), Some(1));
    }

    #[test]
    fn a_tab_becomes_spaces_to_the_next_tab_stop() {
        let buffer = painted("a\tb", 10, whole(10));
        assert_eq!(row_text(&buffer, 0), "a   b     ");
    }

    #[test]
    fn a_line_longer_than_the_window_is_clipped_not_wrapped() {
        let buffer = painted("abcdefghij", 4, whole(4));
        assert_eq!(buffer.height(), 1);
        assert_eq!(row_text(&buffer, 0), "abcd");
    }

    #[test]
    fn horizontal_scroll_moves_the_window() {
        let area = TextArea {
            origin: 0,
            width: 4,
            scroll: 3,
        };
        let buffer = painted("abcdefghij", 4, area);
        assert_eq!(row_text(&buffer, 0), "defg");
    }

    #[test]
    fn a_glyph_cut_by_the_right_edge_becomes_a_space() {
        // `漢` starts at column 3 and would need columns 3 and 4.
        let buffer = painted("abc漢d", 4, whole(4));
        assert_eq!(row_text(&buffer, 0), "abc ");
        assert!(buffer.get(3, 0).is_some_and(|cell| !cell.is_continuation()));
    }

    #[test]
    fn a_glyph_cut_by_the_left_edge_becomes_a_space() {
        // Scrolling to column 1 cuts `漢`, which occupies columns 0 and 1.
        let area = TextArea {
            origin: 0,
            width: 4,
            scroll: 1,
        };
        let buffer = painted("漢bcd", 4, area);
        assert_eq!(row_text(&buffer, 0), " bcd");
    }

    #[test]
    fn a_cut_glyph_keeps_its_style() {
        let style = Style::DEFAULT.with_background(Color::Rgb(1, 2, 3));
        let mut buffer = CellBuffer::new(4, 1);
        let layout = LineLayout::new("abc漢", 4);
        for cluster in layout.clusters() {
            paint_cluster(&mut buffer, 0, cluster, style, whole(4));
        }
        assert_eq!(buffer.get(3, 0).map(Cell::style), Some(style));
    }

    #[test]
    fn a_tab_cut_by_the_edge_paints_the_visible_part() {
        let buffer = painted("\tx", 2, whole(2));
        assert_eq!(row_text(&buffer, 0), "  ");
    }

    #[test]
    fn the_area_origin_offsets_every_write() {
        let area = TextArea {
            origin: 3,
            width: 4,
            scroll: 0,
        };
        let buffer = painted("abcdefg", 10, area);
        assert_eq!(row_text(&buffer, 0), "   abcd   ");
    }

    #[test]
    fn restyling_covers_only_the_named_columns() {
        let mut buffer = painted("abcdef", 6, whole(6));
        restyle_columns(&mut buffer, 0, 1..3, whole(6), |style| {
            style.with_attributes(Attributes::BOLD)
        });
        let bold: Vec<bool> = (0..6)
            .map(|column| {
                buffer
                    .get(column, 0)
                    .is_some_and(|cell| cell.style().attributes.contains(Attributes::BOLD))
            })
            .collect();
        assert_eq!(bold, vec![false, true, true, false, false, false]);
    }

    #[test]
    fn restyling_a_double_width_glyph_covers_both_halves() {
        let mut buffer = painted("a漢b", 6, whole(6));
        restyle_columns(&mut buffer, 0, 1..2, whole(6), |style| {
            style.with_attributes(Attributes::BOLD)
        });
        assert!(
            buffer
                .get(2, 0)
                .is_some_and(|cell| cell.style().attributes.contains(Attributes::BOLD)),
            "the continuation half must match its glyph"
        );
    }

    #[test]
    fn restyling_off_screen_columns_does_nothing() {
        let mut buffer = painted("abcdef", 6, whole(6));
        let before = buffer.clone();
        restyle_columns(&mut buffer, 0, 10..20, whole(6), |style| {
            style.with_attributes(Attributes::BOLD)
        });
        assert_eq!(buffer, before);
    }

    #[test]
    fn plain_text_is_clipped_like_a_line() {
        let mut buffer = CellBuffer::new(6, 1);
        paint_text(&mut buffer, 0, 2, "... 5 lines", Style::DEFAULT, whole(6));
        assert_eq!(row_text(&buffer, 0), "  ... ");
    }

    #[test]
    fn plain_text_before_the_window_is_skipped() {
        let area = TextArea {
            origin: 0,
            width: 6,
            scroll: 4,
        };
        let mut buffer = CellBuffer::new(6, 1);
        paint_text(&mut buffer, 0, 0, "abcdefgh", Style::DEFAULT, area);
        assert_eq!(row_text(&buffer, 0), "efgh  ");
    }
}
