//! The gutter: line numbers and fold indicators.
//!
//! The gutter is `digits` columns of right-aligned line number, one column of
//! fold indicator, and one blank column separating it from the text. Its width
//! adapts to the document: a hundred-line file needs three columns, a
//! ten-thousand-line file needs five, and a document that grows past a power
//! of ten widens the gutter on the next frame rather than clipping its own
//! last line number.
//!
//! Both the digit count and the number's formatting come from the kernel's
//! [`GutterRenderer`], which the GPU face uses for the same job. Only the
//! placement is ours, because the kernel's is in pixels.
//!
//! The indicators are ASCII — `v` for a region that can be folded, `>` for one
//! that is folded. The obvious triangles are East Asian Ambiguous width, which
//! means a terminal configured for CJK draws them two cells wide and every
//! column after them lands one place to the right.

use iridium_editor::Editor;
use iridium_editor::render::{FoldIndicator, GutterRenderer};

use super::palette::Palette;
use crate::cell::{CellBuffer, Style};

/// The column the fold indicator occupies, counting from the gutter's start.
const FOLD_COLUMN_WIDTH: usize = 1;

/// The blank column between the gutter and the text.
const SEPARATOR_WIDTH: usize = 1;

/// The indicator for a region that can be folded.
const FOLDABLE: &str = "v";

/// The indicator for a region that is folded.
const FOLDED: &str = ">";

/// The width of the gutter for a document of `total_lines` lines.
///
/// Zero when line numbers are switched off: without them there is nothing for
/// a fold indicator to sit beside, and a one-column gutter of nothing but
/// indicators reads as an editing artefact rather than a margin.
pub fn width(total_lines: usize, show_line_numbers: bool) -> usize {
    if !show_line_numbers {
        return 0;
    }
    GutterRenderer::digit_columns(total_lines) + FOLD_COLUMN_WIDTH + SEPARATOR_WIDTH
}

/// Paints one document line's gutter entry on `row`.
///
/// `is_active` marks a line that a cursor is on — any cursor, not only the
/// primary one.
pub fn paint(
    buffer: &mut CellBuffer,
    editor: &Editor,
    line: usize,
    row: usize,
    is_active: bool,
    palette: &Palette,
    gutter_width: usize,
) {
    if gutter_width == 0 {
        return;
    }
    let total_lines = editor.state().document.line_count();
    let style = palette.gutter(is_active);
    blank_row(buffer, row, gutter_width, style);

    let digits = GutterRenderer::digit_columns(total_lines);
    let number = GutterRenderer::format_line_number(line + 1, total_lines);
    buffer.set_str(0, row, &number, style);

    if let Some(indicator) = indicator_for(editor, line) {
        let text = match indicator {
            FoldIndicator::Foldable => FOLDABLE,
            FoldIndicator::Folded => FOLDED,
        };
        buffer.set_str(digits, row, text, style);
    }
}

/// Fills a row's gutter columns with blanks in `style`.
///
/// The background matters even where there is no glyph: it is what makes the
/// gutter a margin rather than a run of stale cells from the previous frame.
pub fn blank_row(buffer: &mut CellBuffer, row: usize, gutter_width: usize, style: Style) {
    for column in 0..gutter_width.min(buffer.width()) {
        buffer.set_str(column, row, " ", style);
    }
}

/// The indicator a line should show, if any.
///
/// Both predicates are the kernel's: whether a line begins a foldable region
/// and whether that region is currently folded are fold-state questions, and a
/// face that answered them itself would disagree with the fold verbs the
/// moment either definition moved.
fn indicator_for(editor: &Editor, line: usize) -> Option<FoldIndicator> {
    if editor.is_folded(line) {
        return Some(FoldIndicator::Folded);
    }
    if editor.is_foldable(line) {
        return Some(FoldIndicator::Foldable);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gutter_widens_with_the_line_count() {
        // Two digit columns is the kernel's floor, so a nine-line and a
        // ninety-nine-line document share a width.
        assert_eq!(width(9, true), 2 + 1 + 1);
        assert_eq!(width(99, true), 2 + 1 + 1);
        assert_eq!(width(100, true), 3 + 1 + 1);
        assert_eq!(width(9_999, true), 4 + 1 + 1);
        assert_eq!(width(10_000, true), 5 + 1 + 1);
    }

    #[test]
    fn switching_line_numbers_off_removes_the_gutter_entirely() {
        assert_eq!(width(10_000, false), 0);
    }
}
