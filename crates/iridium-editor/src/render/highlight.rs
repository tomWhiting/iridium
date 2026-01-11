//! Selection and line highlighting.
//!
//! This module provides rendering primitives for selection highlights and
//! current line highlighting. These are rendered as colored rectangles
//! behind the text.

use crate::document::{Range, Selection};
use crate::theme::Color;

/// A highlight rectangle to be rendered.
#[derive(Debug, Clone)]
pub struct HighlightRect {
    /// X position in pixels (screen coordinates)
    pub x: f32,
    /// Y position in pixels (screen coordinates)
    pub y: f32,
    /// Width in pixels
    pub width: f32,
    /// Height in pixels
    pub height: f32,
    /// Fill color
    pub color: Color,
}

impl HighlightRect {
    /// Creates a new highlight rectangle.
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32, color: Color) -> Self {
        Self { x, y, width, height, color }
    }

    /// Creates a full-width line highlight.
    #[must_use]
    pub fn full_line(y: f32, width: f32, height: f32, color: Color) -> Self {
        Self { x: 0.0, y, width, height, color }
    }
}

/// Renders selection highlights.
///
/// This renderer computes the rectangles needed to highlight selected text.
/// For multi-line selections, it produces one rectangle per line.
///
/// # Example
///
/// ```ignore
/// let renderer = SelectionRenderer::new();
/// let rects = renderer.compute_selection(
///     &selection,
///     &document,
///     line_height,
///     char_width,
///     viewport_width,
///     scroll_x,
///     first_visible_line,
///     visible_lines,
///     theme.editor.selection,
/// );
/// // Draw the highlight rectangles
/// ```
#[derive(Debug, Default)]
pub struct SelectionRenderer;

impl SelectionRenderer {
    /// Creates a new selection renderer.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Computes highlight rectangles for a selection range.
    ///
    /// # Arguments
    ///
    /// * `range` - The selection range in document coordinates
    /// * `line_lengths` - Function to get the length of a line (in columns)
    /// * `line_height` - Height of a line in pixels
    /// * `char_width` - Width of a character in pixels
    /// * `viewport_width` - Width of the viewport in pixels
    /// * `scroll_x` - Horizontal scroll offset in pixels
    /// * `first_visible_line` - First visible line number
    /// * `visible_lines` - Number of visible lines
    /// * `color` - Selection highlight color
    ///
    /// Returns highlight rectangles in screen coordinates.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_selection_range<F>(
        &self,
        range: Range,
        line_lengths: F,
        line_height: f32,
        char_width: f32,
        viewport_width: f32,
        scroll_x: f32,
        first_visible_line: usize,
        visible_lines: usize,
        color: Color,
    ) -> Vec<HighlightRect>
    where
        F: Fn(usize) -> usize,
    {
        let last_visible_line = first_visible_line + visible_lines;

        // Normalize range (start before end)
        let (start, end) = if range.start <= range.end {
            (range.start, range.end)
        } else {
            (range.end, range.start)
        };

        // Clamp to visible range
        let start_line = start.line.max(first_visible_line);
        let end_line = end.line.min(last_visible_line.saturating_sub(1));

        if start_line > end_line {
            return Vec::new();
        }

        let mut rects = Vec::new();

        for line in start_line..=end_line {
            let screen_line = line - first_visible_line;
            let y = screen_line as f32 * line_height;

            // Determine start and end columns for this line
            let (col_start, col_end) = if line == start.line && line == end.line {
                // Single line selection
                (start.column, end.column)
            } else if line == start.line {
                // First line of multi-line selection
                (start.column, line_lengths(line))
            } else if line == end.line {
                // Last line of multi-line selection
                (0, end.column)
            } else {
                // Middle line - select entire line
                (0, line_lengths(line))
            };

            if col_start == col_end {
                // Empty selection on this line, skip (but show for newline)
                if line != end.line {
                    // For lines with newline selected, show a small rect
                    let x = (col_start as f32 * char_width) - scroll_x;
                    let width = char_width.min(4.0);
                    rects.push(HighlightRect::new(x, y, width, line_height, color));
                }
                continue;
            }

            let x = (col_start as f32 * char_width) - scroll_x;
            let mut width = (col_end - col_start) as f32 * char_width;

            // For non-last lines, extend selection to show newline is included
            if line != end.line && line != start.line {
                width = width.max(viewport_width - x);
            }

            rects.push(HighlightRect::new(x, y, width, line_height, color));
        }

        rects
    }

    /// Computes highlight rectangles for a Selection (anchor + head).
    #[allow(clippy::too_many_arguments)]
    pub fn compute_selection<F>(
        &self,
        selection: &Selection,
        line_lengths: F,
        line_height: f32,
        char_width: f32,
        viewport_width: f32,
        scroll_x: f32,
        first_visible_line: usize,
        visible_lines: usize,
        color: Color,
    ) -> Vec<HighlightRect>
    where
        F: Fn(usize) -> usize,
    {
        self.compute_selection_range(
            selection.range(),
            line_lengths,
            line_height,
            char_width,
            viewport_width,
            scroll_x,
            first_visible_line,
            visible_lines,
            color,
        )
    }

    /// Computes highlight rectangles for multiple selections.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_selections<'a, F>(
        &self,
        selections: impl Iterator<Item = &'a Selection>,
        line_lengths: F,
        line_height: f32,
        char_width: f32,
        viewport_width: f32,
        scroll_x: f32,
        first_visible_line: usize,
        visible_lines: usize,
        color: Color,
    ) -> Vec<HighlightRect>
    where
        F: Fn(usize) -> usize + Copy,
    {
        selections
            .flat_map(|sel| {
                self.compute_selection(
                    sel,
                    line_lengths,
                    line_height,
                    char_width,
                    viewport_width,
                    scroll_x,
                    first_visible_line,
                    visible_lines,
                    color,
                )
            })
            .collect()
    }
}

/// Renders current line highlights.
///
/// This renderer highlights the line(s) containing cursor(s).
#[derive(Debug, Default)]
pub struct CurrentLineRenderer;

impl CurrentLineRenderer {
    /// Creates a new current line renderer.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Computes highlight rectangles for cursor lines.
    ///
    /// # Arguments
    ///
    /// * `cursor_lines` - Iterator of line numbers where cursors are located
    /// * `line_height` - Height of a line in pixels
    /// * `viewport_width` - Width of the viewport in pixels
    /// * `first_visible_line` - First visible line number
    /// * `visible_lines` - Number of visible lines
    /// * `color` - Current line highlight color
    ///
    /// Returns highlight rectangles in screen coordinates.
    pub fn compute_current_lines(
        &self,
        cursor_lines: impl Iterator<Item = usize>,
        line_height: f32,
        viewport_width: f32,
        first_visible_line: usize,
        visible_lines: usize,
        color: Color,
    ) -> Vec<HighlightRect> {
        let last_visible_line = first_visible_line + visible_lines;

        // Collect and deduplicate lines (multiple cursors on same line)
        let mut lines: Vec<usize> = cursor_lines
            .filter(|&line| line >= first_visible_line && line < last_visible_line)
            .collect();
        lines.sort_unstable();
        lines.dedup();

        lines
            .into_iter()
            .map(|line| {
                let screen_line = line - first_visible_line;
                let y = screen_line as f32 * line_height;
                HighlightRect::full_line(y, viewport_width, line_height, color)
            })
            .collect()
    }

    /// Computes a single current line highlight for the primary cursor.
    pub fn compute_primary_line(
        &self,
        cursor_line: usize,
        line_height: f32,
        viewport_width: f32,
        first_visible_line: usize,
        visible_lines: usize,
        color: Color,
    ) -> Option<HighlightRect> {
        self.compute_current_lines(
            std::iter::once(cursor_line),
            line_height,
            viewport_width,
            first_visible_line,
            visible_lines,
            color,
        )
        .into_iter()
        .next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Position;

    fn line_lengths(line: usize) -> usize {
        // Simulate a document with lines of varying lengths
        match line {
            0 => 10,
            1 => 15,
            2 => 8,
            3 => 20,
            _ => 10,
        }
    }

    #[test]
    fn selection_single_line() {
        let renderer = SelectionRenderer::new();
        let range = Range::new(Position::new(0, 2), Position::new(0, 7));
        let color = Color::new(0.0, 0.0, 1.0, 0.5);

        let rects = renderer.compute_selection_range(
            range,
            line_lengths,
            20.0,  // line_height
            8.0,   // char_width
            800.0, // viewport_width
            0.0,   // scroll_x
            0,     // first_visible_line
            50,    // visible_lines
            color,
        );

        assert_eq!(rects.len(), 1);
        let rect = &rects[0];
        assert!((rect.x - 16.0).abs() < 0.001); // column 2 * 8
        assert!((rect.y - 0.0).abs() < 0.001);  // line 0 * 20
        assert!((rect.width - 40.0).abs() < 0.001); // (7-2) * 8
        assert!((rect.height - 20.0).abs() < 0.001);
    }

    #[test]
    fn selection_multi_line() {
        let renderer = SelectionRenderer::new();
        let range = Range::new(Position::new(0, 5), Position::new(2, 3));
        let color = Color::new(0.0, 0.0, 1.0, 0.5);

        let rects = renderer.compute_selection_range(
            range,
            line_lengths,
            20.0,
            8.0,
            800.0,
            0.0,
            0,
            50,
            color,
        );

        // Should have 3 rectangles (one per line)
        assert_eq!(rects.len(), 3);

        // First line: from column 5 to end of line (10)
        assert!((rects[0].x - 40.0).abs() < 0.001); // column 5 * 8
        assert!((rects[0].y - 0.0).abs() < 0.001);

        // Second line: entire line (0 to 15)
        assert!((rects[1].x - 0.0).abs() < 0.001);
        assert!((rects[1].y - 20.0).abs() < 0.001);

        // Third line: from start to column 3
        assert!((rects[2].x - 0.0).abs() < 0.001);
        assert!((rects[2].y - 40.0).abs() < 0.001);
        assert!((rects[2].width - 24.0).abs() < 0.001); // 3 * 8
    }

    #[test]
    fn selection_outside_viewport() {
        let renderer = SelectionRenderer::new();
        let range = Range::new(Position::new(0, 0), Position::new(5, 0));
        let color = Color::new(0.0, 0.0, 1.0, 0.5);

        let rects = renderer.compute_selection_range(
            range,
            line_lengths,
            20.0,
            8.0,
            800.0,
            0.0,
            10, // first_visible_line - selection is before viewport
            50,
            color,
        );

        assert!(rects.is_empty());
    }

    #[test]
    fn selection_partial_viewport() {
        let renderer = SelectionRenderer::new();
        // Selection spans lines 5-15, but viewport shows lines 10-20
        let range = Range::new(Position::new(5, 0), Position::new(15, 5));
        let color = Color::new(0.0, 0.0, 1.0, 0.5);

        let rects = renderer.compute_selection_range(
            range,
            |_| 20, // All lines have 20 chars
            20.0,
            8.0,
            800.0,
            0.0,
            10, // first_visible_line
            10, // visible_lines (10-19)
            color,
        );

        // Should show lines 10-15 (6 lines visible from selection)
        assert_eq!(rects.len(), 6);

        // First visible rect should be at screen y=0 (line 10)
        assert!((rects[0].y - 0.0).abs() < 0.001);
    }

    #[test]
    fn current_line_single() {
        let renderer = CurrentLineRenderer::new();
        let color = Color::new(0.1, 0.1, 0.1, 1.0);

        let rects = renderer.compute_current_lines(
            std::iter::once(5),
            20.0,
            800.0,
            0, // first_visible_line
            50,
            color,
        );

        assert_eq!(rects.len(), 1);
        assert!((rects[0].x - 0.0).abs() < 0.001);
        assert!((rects[0].y - 100.0).abs() < 0.001); // line 5 * 20
        assert!((rects[0].width - 800.0).abs() < 0.001);
    }

    #[test]
    fn current_line_deduplicates() {
        let renderer = CurrentLineRenderer::new();
        let color = Color::new(0.1, 0.1, 0.1, 1.0);

        // Two cursors on the same line
        let rects = renderer.compute_current_lines(
            vec![5, 5, 5].into_iter(),
            20.0,
            800.0,
            0,
            50,
            color,
        );

        // Should produce only one rectangle
        assert_eq!(rects.len(), 1);
    }

    #[test]
    fn current_line_outside_viewport() {
        let renderer = CurrentLineRenderer::new();
        let color = Color::new(0.1, 0.1, 0.1, 1.0);

        let rects = renderer.compute_current_lines(
            std::iter::once(5),
            20.0,
            800.0,
            10, // first_visible_line - cursor is before viewport
            10,
            color,
        );

        assert!(rects.is_empty());
    }

    #[test]
    fn selection_with_scroll() {
        let renderer = SelectionRenderer::new();
        let range = Range::new(Position::new(0, 5), Position::new(0, 10));
        let color = Color::new(0.0, 0.0, 1.0, 0.5);

        let rects = renderer.compute_selection_range(
            range,
            line_lengths,
            20.0,
            8.0,
            800.0,
            16.0, // scroll_x - scrolled 2 characters right
            0,
            50,
            color,
        );

        assert_eq!(rects.len(), 1);
        // X should be offset by scroll
        assert!((rects[0].x - 24.0).abs() < 0.001); // (5 * 8) - 16 scroll
    }
}
