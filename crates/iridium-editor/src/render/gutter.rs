//! Line number gutter and fold indicator rendering.
//!
//! This module provides rendering primitives for the gutter area, which displays
//! line numbers, fold indicators, and other margin annotations.

use crate::editor::FoldState;
use crate::theme::Color;

/// Default character width for calculating gutter width.
const DEFAULT_CHAR_WIDTH: f32 = 8.0;

/// Minimum gutter padding (pixels) on each side of line numbers.
const GUTTER_PADDING: f32 = 8.0;

/// Minimum number of digit columns to display.
const MIN_DIGIT_COLUMNS: usize = 2;

/// Fold indicator width in pixels.
const FOLD_INDICATOR_WIDTH: f32 = 16.0;

/// A fold indicator marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldIndicator {
    /// A foldable region (can be collapsed).
    Foldable,
    /// A collapsed/folded region (can be expanded).
    Folded,
}

/// A line number entry to be rendered.
#[derive(Debug, Clone)]
pub struct LineNumberEntry {
    /// The line number to display (1-indexed for display).
    pub line_number: usize,
    /// X position in pixels (right-aligned text end position).
    pub x: f32,
    /// Y position in pixels (top of line).
    pub y: f32,
    /// Whether this is the current line (for highlighting).
    pub is_current: bool,
    /// Color for the line number.
    pub color: Color,
}

impl LineNumberEntry {
    /// Creates a new line number entry.
    #[must_use]
    pub const fn new(line_number: usize, x: f32, y: f32, is_current: bool, color: Color) -> Self {
        Self {
            line_number,
            x,
            y,
            is_current,
            color,
        }
    }
}

/// A fold indicator entry to be rendered.
#[derive(Debug, Clone)]
pub struct FoldIndicatorEntry {
    /// The fold indicator type.
    pub indicator: FoldIndicator,
    /// X position in pixels (center of indicator).
    pub x: f32,
    /// Y position in pixels (center of indicator).
    pub y: f32,
    /// Color for the indicator.
    pub color: Color,
}

impl FoldIndicatorEntry {
    /// Creates a new fold indicator entry.
    #[must_use]
    pub const fn new(indicator: FoldIndicator, x: f32, y: f32, color: Color) -> Self {
        Self {
            indicator,
            x,
            y,
            color,
        }
    }
}

/// A background rectangle for the gutter area.
#[derive(Debug, Clone)]
pub struct GutterBackground {
    /// Width of the gutter in pixels.
    pub width: f32,
    /// Height of the gutter in pixels.
    pub height: f32,
    /// Background color.
    pub color: Color,
}

impl GutterBackground {
    /// Creates a new gutter background.
    #[must_use]
    pub const fn new(width: f32, height: f32, color: Color) -> Self {
        Self {
            width,
            height,
            color,
        }
    }
}

/// Configuration for gutter rendering.
#[derive(Debug, Clone)]
pub struct GutterConfig {
    /// Character width in pixels (for width calculations).
    pub char_width: f32,
    /// Whether to show fold indicators.
    pub show_fold_indicators: bool,
    /// Padding on each side of line numbers.
    pub padding: f32,
}

impl Default for GutterConfig {
    fn default() -> Self {
        Self {
            char_width: DEFAULT_CHAR_WIDTH,
            show_fold_indicators: true,
            padding: GUTTER_PADDING,
        }
    }
}

/// Gutter renderer for line numbers and fold indicators.
///
/// This renderer computes the data needed to render line numbers in the gutter
/// area. The host application is responsible for actually rendering the text
/// and indicators.
///
/// # Width Calculation
///
/// The gutter width is calculated dynamically based on the total number of
/// lines in the document. This ensures the gutter is always wide enough to
/// display all line numbers without clipping.
///
/// # Example
///
/// ```ignore
/// let renderer = GutterRenderer::with_config(GutterConfig::default());
///
/// // Calculate gutter width based on document
/// let gutter_width = renderer.calculate_width(total_lines, char_width);
///
/// // Compute line numbers for visible region
/// let entries = renderer.compute_line_numbers(
///     first_visible_line,
///     visible_lines,
///     total_lines,
///     current_line,
///     line_height,
///     char_width,
///     theme.editor.line_number,
///     theme.editor.line_number_active,
/// );
///
/// // Render each entry
/// for entry in entries {
///     // Draw text at entry.x, entry.y with entry.color
/// }
/// ```
#[derive(Debug)]
pub struct GutterRenderer {
    /// Configuration.
    config: GutterConfig,
}

impl Default for GutterRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl GutterRenderer {
    /// Creates a new gutter renderer with default configuration.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            config: GutterConfig {
                char_width: DEFAULT_CHAR_WIDTH,
                show_fold_indicators: true,
                padding: GUTTER_PADDING,
            },
        }
    }

    /// Creates a new gutter renderer with custom configuration.
    #[must_use]
    pub const fn with_config(config: GutterConfig) -> Self {
        Self { config }
    }

    /// Returns the current configuration.
    #[must_use]
    pub const fn config(&self) -> &GutterConfig {
        &self.config
    }

    /// Returns a mutable reference to the configuration.
    pub const fn config_mut(&mut self) -> &mut GutterConfig {
        &mut self.config
    }

    /// Calculates the number of digit columns needed for line numbers.
    ///
    /// # Arguments
    ///
    /// * `total_lines` - Total number of lines in the document
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    pub fn digit_columns(total_lines: usize) -> usize {
        if total_lines == 0 {
            return MIN_DIGIT_COLUMNS;
        }

        // Calculate digits needed for the largest line number
        let digits = (total_lines as f64).log10().floor() as usize + 1;
        digits.max(MIN_DIGIT_COLUMNS)
    }

    /// Calculates the gutter width in pixels.
    ///
    /// The width includes padding and optionally space for fold indicators.
    ///
    /// # Arguments
    ///
    /// * `total_lines` - Total number of lines in the document
    /// * `char_width` - Width of a character in pixels
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn calculate_width(&self, total_lines: usize, char_width: f32) -> f32 {
        let digit_count = Self::digit_columns(total_lines);
        let number_width = digit_count as f32 * char_width;
        let fold_width = if self.config.show_fold_indicators {
            FOLD_INDICATOR_WIDTH
        } else {
            0.0
        };

        self.config.padding + number_width + self.config.padding + fold_width
    }

    /// Calculates the width of the line number area only (without fold indicators).
    ///
    /// # Arguments
    ///
    /// * `total_lines` - Total number of lines in the document
    /// * `char_width` - Width of a character in pixels
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn calculate_line_number_width(&self, total_lines: usize, char_width: f32) -> f32 {
        let digit_count = Self::digit_columns(total_lines);
        let number_width = digit_count as f32 * char_width;
        self.config.padding + number_width + self.config.padding
    }

    /// Computes gutter background rectangle.
    ///
    /// # Arguments
    ///
    /// * `total_lines` - Total number of lines in the document
    /// * `viewport_height` - Height of the viewport in pixels
    /// * `char_width` - Width of a character in pixels
    /// * `color` - Background color
    #[must_use]
    pub fn compute_background(
        &self,
        total_lines: usize,
        viewport_height: f32,
        char_width: f32,
        color: Color,
    ) -> GutterBackground {
        let width = self.calculate_width(total_lines, char_width);
        GutterBackground::new(width, viewport_height, color)
    }

    /// Computes line number entries for visible lines.
    ///
    /// Line numbers are right-aligned in the gutter area. The x coordinate
    /// returned is the right edge of where the text should be rendered.
    ///
    /// # Arguments
    ///
    /// * `first_visible_line` - First visible line number (0-indexed)
    /// * `visible_lines` - Number of visible lines
    /// * `total_lines` - Total number of lines in the document
    /// * `current_line` - Current line where cursor is (0-indexed), or None
    /// * `line_height` - Height of a line in pixels
    /// * `char_width` - Width of a character in pixels
    /// * `line_number_color` - Color for normal line numbers
    /// * `active_line_color` - Color for the current line number
    #[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
    pub fn compute_line_numbers(
        &self,
        first_visible_line: usize,
        visible_lines: usize,
        total_lines: usize,
        current_line: Option<usize>,
        line_height: f32,
        char_width: f32,
        line_number_color: Color,
        active_line_color: Color,
    ) -> Vec<LineNumberEntry> {
        let last_visible = (first_visible_line + visible_lines).min(total_lines);
        let digit_count = Self::digit_columns(total_lines);

        // Right edge of line numbers (where text ends)
        let right_x = (digit_count as f32).mul_add(char_width, self.config.padding);

        (first_visible_line..last_visible)
            .map(|line| {
                let screen_line = line - first_visible_line;
                let y = screen_line as f32 * line_height;
                let is_current = current_line == Some(line);
                let color = if is_current {
                    active_line_color
                } else {
                    line_number_color
                };

                // Line numbers are 1-indexed for display
                LineNumberEntry::new(line + 1, right_x, y, is_current, color)
            })
            .collect()
    }

    /// Computes line number entries for multiple cursors.
    ///
    /// When there are multiple cursors, all cursor lines are highlighted.
    ///
    /// # Arguments
    ///
    /// * `first_visible_line` - First visible line number (0-indexed)
    /// * `visible_lines` - Number of visible lines
    /// * `total_lines` - Total number of lines in the document
    /// * `cursor_lines` - Iterator of line numbers where cursors are located (0-indexed)
    /// * `line_height` - Height of a line in pixels
    /// * `char_width` - Width of a character in pixels
    /// * `line_number_color` - Color for normal line numbers
    /// * `active_line_color` - Color for cursor line numbers
    #[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
    pub fn compute_line_numbers_multi(
        &self,
        first_visible_line: usize,
        visible_lines: usize,
        total_lines: usize,
        cursor_lines: impl Iterator<Item = usize>,
        line_height: f32,
        char_width: f32,
        line_number_color: Color,
        active_line_color: Color,
    ) -> Vec<LineNumberEntry> {
        // Collect cursor lines into a set for O(1) lookup
        let cursor_set: std::collections::HashSet<usize> = cursor_lines.collect();

        let last_visible = (first_visible_line + visible_lines).min(total_lines);
        let digit_count = Self::digit_columns(total_lines);

        let right_x = (digit_count as f32).mul_add(char_width, self.config.padding);

        (first_visible_line..last_visible)
            .map(|line| {
                let screen_line = line - first_visible_line;
                let y = screen_line as f32 * line_height;
                let is_current = cursor_set.contains(&line);
                let color = if is_current {
                    active_line_color
                } else {
                    line_number_color
                };

                LineNumberEntry::new(line + 1, right_x, y, is_current, color)
            })
            .collect()
    }

    /// Computes fold indicator entries for visible lines.
    ///
    /// # Arguments
    ///
    /// * `fold_markers` - Iterator of (line, indicator) pairs for lines with fold markers
    /// * `first_visible_line` - First visible line number (0-indexed)
    /// * `visible_lines` - Number of visible lines
    /// * `total_lines` - Total number of lines in the document
    /// * `line_height` - Height of a line in pixels
    /// * `char_width` - Width of a character in pixels
    /// * `color` - Fold indicator color
    #[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
    pub fn compute_fold_indicators(
        &self,
        fold_markers: impl Iterator<Item = (usize, FoldIndicator)>,
        first_visible_line: usize,
        visible_lines: usize,
        total_lines: usize,
        line_height: f32,
        char_width: f32,
        color: Color,
    ) -> Vec<FoldIndicatorEntry> {
        if !self.config.show_fold_indicators {
            return Vec::new();
        }

        let last_visible = first_visible_line + visible_lines;
        let line_number_width = self.calculate_line_number_width(total_lines, char_width);

        // Center of fold indicator area
        let center_x = line_number_width + (FOLD_INDICATOR_WIDTH / 2.0);

        fold_markers
            .filter(|(line, _)| *line >= first_visible_line && *line < last_visible)
            .map(|(line, indicator)| {
                let screen_line = line - first_visible_line;
                let center_y = (screen_line as f32).mul_add(line_height, line_height / 2.0);
                FoldIndicatorEntry::new(indicator, center_x, center_y, color)
            })
            .collect()
    }

    /// Formats a line number as a right-aligned string.
    ///
    /// This is a convenience method for rendering line numbers as text.
    ///
    /// # Arguments
    ///
    /// * `line_number` - The line number to format (1-indexed)
    /// * `total_lines` - Total number of lines (for padding width calculation)
    #[must_use]
    pub fn format_line_number(line_number: usize, total_lines: usize) -> String {
        let width = Self::digit_columns(total_lines);
        format!("{line_number:>width$}")
    }

    /// Computes fold indicators from a FoldState (T134).
    ///
    /// This is a convenience method that extracts fold markers from the FoldState
    /// and computes their visual positions for rendering.
    ///
    /// # Arguments
    ///
    /// * `fold_state` - The fold state containing fold regions
    /// * `first_visible_line` - First visible line number (0-indexed)
    /// * `visible_lines` - Number of visible lines
    /// * `total_lines` - Total number of lines in the document
    /// * `line_height` - Height of a line in pixels
    /// * `char_width` - Width of a character in pixels
    /// * `color` - Fold indicator color
    #[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
    pub fn compute_fold_indicators_from_state(
        &self,
        fold_state: &FoldState,
        first_visible_line: usize,
        visible_lines: usize,
        total_lines: usize,
        line_height: f32,
        char_width: f32,
        color: Color,
    ) -> Vec<FoldIndicatorEntry> {
        if !self.config.show_fold_indicators {
            return Vec::new();
        }

        // Build fold markers from the fold state
        let fold_markers = fold_state.regions().iter().map(|region| {
            let indicator = if fold_state.is_folded(region.start_line) {
                FoldIndicator::Folded
            } else {
                FoldIndicator::Foldable
            };
            (region.start_line, indicator)
        });

        self.compute_fold_indicators(
            fold_markers,
            first_visible_line,
            visible_lines,
            total_lines,
            line_height,
            char_width,
            color,
        )
    }

    /// Computes the bounding box for a fold indicator click detection.
    ///
    /// Returns (x, y, width, height) for the clickable area around a fold indicator.
    ///
    /// # Arguments
    ///
    /// * `line` - Document line number (0-indexed)
    /// * `first_visible_line` - First visible line number (0-indexed)
    /// * `total_lines` - Total number of lines in the document
    /// * `line_height` - Height of a line in pixels
    /// * `char_width` - Width of a character in pixels
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn fold_indicator_bounds(
        &self,
        line: usize,
        first_visible_line: usize,
        total_lines: usize,
        line_height: f32,
        char_width: f32,
    ) -> (f32, f32, f32, f32) {
        let line_number_width = self.calculate_line_number_width(total_lines, char_width);
        let x = line_number_width;
        let screen_line = line.saturating_sub(first_visible_line);
        let y = screen_line as f32 * line_height;
        (x, y, FOLD_INDICATOR_WIDTH, line_height)
    }

    /// Tests if a point is within a fold indicator area.
    ///
    /// # Arguments
    ///
    /// * `point_x` - X coordinate of the point
    /// * `point_y` - Y coordinate of the point
    /// * `first_visible_line` - First visible line number (0-indexed)
    /// * `visible_lines` - Number of visible lines
    /// * `total_lines` - Total number of lines in the document
    /// * `line_height` - Height of a line in pixels
    /// * `char_width` - Width of a character in pixels
    ///
    /// Returns Some(line) if the point is within a fold indicator area, None otherwise.
    #[allow(
        clippy::too_many_arguments,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation
    )]
    pub fn hit_test_fold_indicator(
        &self,
        point_x: f32,
        point_y: f32,
        first_visible_line: usize,
        visible_lines: usize,
        total_lines: usize,
        line_height: f32,
        char_width: f32,
    ) -> Option<usize> {
        if !self.config.show_fold_indicators {
            return None;
        }

        let line_number_width = self.calculate_line_number_width(total_lines, char_width);
        let fold_x_start = line_number_width;
        let fold_x_end = fold_x_start + FOLD_INDICATOR_WIDTH;

        // Check if x is within fold indicator area
        if point_x < fold_x_start || point_x >= fold_x_end {
            return None;
        }

        // Check if y is within visible lines
        if point_y < 0.0 {
            return None;
        }

        let screen_line = (point_y / line_height) as usize;
        if screen_line >= visible_lines {
            return None;
        }

        let doc_line = first_visible_line + screen_line;
        if doc_line >= total_lines {
            return None;
        }

        Some(doc_line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digit_columns_small() {
        assert_eq!(GutterRenderer::digit_columns(0), MIN_DIGIT_COLUMNS);
        assert_eq!(GutterRenderer::digit_columns(1), MIN_DIGIT_COLUMNS);
        assert_eq!(GutterRenderer::digit_columns(9), MIN_DIGIT_COLUMNS);
        assert_eq!(GutterRenderer::digit_columns(10), MIN_DIGIT_COLUMNS);
        assert_eq!(GutterRenderer::digit_columns(99), MIN_DIGIT_COLUMNS);
    }

    #[test]
    fn digit_columns_larger() {
        assert_eq!(GutterRenderer::digit_columns(100), 3);
        assert_eq!(GutterRenderer::digit_columns(999), 3);
        assert_eq!(GutterRenderer::digit_columns(1000), 4);
        assert_eq!(GutterRenderer::digit_columns(9999), 4);
        assert_eq!(GutterRenderer::digit_columns(10_000), 5);
    }

    #[test]
    fn gutter_width_small_file() {
        let renderer = GutterRenderer::new();
        let char_width = 8.0;

        // With fold indicators: padding(8) + 2 digits(16) + padding(8) + fold(16) = 48
        let width = renderer.calculate_width(50, char_width);
        assert!((width - 48.0).abs() < 0.001);
    }

    #[test]
    fn gutter_width_large_file() {
        let renderer = GutterRenderer::new();
        let char_width = 8.0;

        // 10000 lines needs 5 digits
        // padding(8) + 5 digits(40) + padding(8) + fold(16) = 72
        let width = renderer.calculate_width(10_000, char_width);
        assert!((width - 72.0).abs() < 0.001);
    }

    #[test]
    fn gutter_width_no_fold_indicators() {
        let config = GutterConfig {
            show_fold_indicators: false,
            ..Default::default()
        };
        let renderer = GutterRenderer::with_config(config);
        let char_width = 8.0;

        // Without fold indicators: padding(8) + 2 digits(16) + padding(8) = 32
        let width = renderer.calculate_width(50, char_width);
        assert!((width - 32.0).abs() < 0.001);
    }

    #[test]
    fn line_numbers_basic() {
        let renderer = GutterRenderer::new();
        let line_number_color = Color::rgb(0.5, 0.5, 0.5);
        let active_color = Color::rgb(1.0, 1.0, 1.0);

        let entries = renderer.compute_line_numbers(
            0,       // first_visible_line
            10,      // visible_lines
            100,     // total_lines
            Some(5), // current_line
            20.0,    // line_height
            8.0,     // char_width
            line_number_color,
            active_color,
        );

        assert_eq!(entries.len(), 10);

        // Check first line
        assert_eq!(entries[0].line_number, 1);
        assert!((entries[0].y - 0.0).abs() < 0.001);
        assert!(!entries[0].is_current);

        // Check current line (line 5, which is entries[5])
        assert_eq!(entries[5].line_number, 6);
        assert!((entries[5].y - 100.0).abs() < 0.001); // line 5 * 20
        assert!(entries[5].is_current);

        // Check last line
        assert_eq!(entries[9].line_number, 10);
    }

    #[test]
    fn line_numbers_scrolled() {
        let renderer = GutterRenderer::new();
        let color = Color::rgb(0.5, 0.5, 0.5);

        let entries = renderer.compute_line_numbers(
            50,   // first_visible_line
            10,   // visible_lines
            100,  // total_lines
            None, // no current line
            20.0, 8.0, color, color,
        );

        assert_eq!(entries.len(), 10);

        // First visible line is 50, so display line number 51
        assert_eq!(entries[0].line_number, 51);
        assert!((entries[0].y - 0.0).abs() < 0.001);

        // Last visible line is 59, display line number 60
        assert_eq!(entries[9].line_number, 60);
    }

    #[test]
    fn line_numbers_past_end() {
        let renderer = GutterRenderer::new();
        let color = Color::rgb(0.5, 0.5, 0.5);

        let entries = renderer.compute_line_numbers(
            95,  // first_visible_line
            10,  // visible_lines (would go past 100)
            100, // total_lines
            None, 20.0, 8.0, color, color,
        );

        // Should only have 5 entries (lines 95-99)
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].line_number, 96);
        assert_eq!(entries[4].line_number, 100);
    }

    #[test]
    fn line_numbers_multi_cursor() {
        let renderer = GutterRenderer::new();
        let line_number_color = Color::rgb(0.5, 0.5, 0.5);
        let active_color = Color::rgb(1.0, 1.0, 1.0);

        let cursor_lines = vec![2, 5, 7];
        let entries = renderer.compute_line_numbers_multi(
            0,
            10,
            100,
            cursor_lines.into_iter(),
            20.0,
            8.0,
            line_number_color,
            active_color,
        );

        assert_eq!(entries.len(), 10);

        // Check cursor lines are marked
        assert!(entries[2].is_current); // line 2
        assert!(entries[5].is_current); // line 5
        assert!(entries[7].is_current); // line 7

        // Non-cursor lines are not marked
        assert!(!entries[0].is_current);
        assert!(!entries[1].is_current);
        assert!(!entries[3].is_current);
    }

    #[test]
    fn fold_indicators_basic() {
        let renderer = GutterRenderer::new();
        let color = Color::rgb(0.5, 0.5, 0.5);

        let fold_markers = vec![
            (2, FoldIndicator::Foldable),
            (5, FoldIndicator::Folded),
            (50, FoldIndicator::Foldable), // Outside visible range
        ];

        let entries = renderer.compute_fold_indicators(
            fold_markers.into_iter(),
            0,    // first_visible_line
            10,   // visible_lines
            100,  // total_lines
            20.0, // line_height
            8.0,  // char_width
            color,
        );

        // Only 2 markers in visible range
        assert_eq!(entries.len(), 2);

        assert_eq!(entries[0].indicator, FoldIndicator::Foldable);
        assert!((entries[0].y - 50.0).abs() < 0.001); // line 2 * 20 + 10 (center)

        assert_eq!(entries[1].indicator, FoldIndicator::Folded);
        assert!((entries[1].y - 110.0).abs() < 0.001); // line 5 * 20 + 10 (center)
    }

    #[test]
    fn fold_indicators_disabled() {
        let config = GutterConfig {
            show_fold_indicators: false,
            ..Default::default()
        };
        let renderer = GutterRenderer::with_config(config);
        let color = Color::rgb(0.5, 0.5, 0.5);

        let fold_markers = vec![(2, FoldIndicator::Foldable)];

        let entries = renderer.compute_fold_indicators(
            fold_markers.into_iter(),
            0,
            10,
            100,
            20.0,
            8.0,
            color,
        );

        // Should return empty when disabled
        assert!(entries.is_empty());
    }

    #[test]
    fn format_line_number_padding() {
        assert_eq!(GutterRenderer::format_line_number(1, 99), " 1");
        assert_eq!(GutterRenderer::format_line_number(99, 99), "99");
        assert_eq!(GutterRenderer::format_line_number(1, 100), "  1");
        assert_eq!(GutterRenderer::format_line_number(100, 100), "100");
        assert_eq!(GutterRenderer::format_line_number(1, 1000), "   1");
    }

    #[test]
    fn gutter_background() {
        let renderer = GutterRenderer::new();
        let color = Color::rgb(0.1, 0.1, 0.1);

        let bg = renderer.compute_background(100, 600.0, 8.0, color);

        assert!((bg.height - 600.0).abs() < 0.001);
        // Width for 100 lines (3 digits): 8 + 24 + 8 + 16 = 56
        assert!((bg.width - 56.0).abs() < 0.001);
    }

    #[test]
    fn x_coordinate_right_aligned() {
        let renderer = GutterRenderer::new();
        let color = Color::rgb(0.5, 0.5, 0.5);
        let char_width = 8.0;

        let entries =
            renderer.compute_line_numbers(0, 5, 100, None, 20.0, char_width, color, color);

        // For 100 lines, we need 3 digit columns
        // x should be: padding(8) + 3 * char_width(8) = 32
        for entry in entries {
            assert!((entry.x - 32.0).abs() < 0.001);
        }
    }

    #[test]
    fn hit_test_fold_indicator_basic() {
        let renderer = GutterRenderer::new();
        let char_width = 8.0;
        let line_height = 20.0;
        let total_lines = 100;

        // Line number width: 8 + 24 + 8 = 40
        // Fold indicator from x=40 to x=56

        // Hit on line 2
        let line = renderer.hit_test_fold_indicator(
            45.0,  // x in fold indicator area
            45.0,  // y (line 2: 40-60)
            0,     // first_visible_line
            10,    // visible_lines
            total_lines,
            line_height,
            char_width,
        );
        assert_eq!(line, Some(2));

        // Miss - x before fold indicator
        let line = renderer.hit_test_fold_indicator(
            30.0, 45.0, 0, 10, total_lines, line_height, char_width,
        );
        assert_eq!(line, None);

        // Miss - x after fold indicator
        let line = renderer.hit_test_fold_indicator(
            60.0, 45.0, 0, 10, total_lines, line_height, char_width,
        );
        assert_eq!(line, None);

        // Miss - y below visible area
        let line = renderer.hit_test_fold_indicator(
            45.0, 250.0, 0, 10, total_lines, line_height, char_width,
        );
        assert_eq!(line, None);
    }

    #[test]
    fn hit_test_fold_indicator_scrolled() {
        let renderer = GutterRenderer::new();

        // When scrolled to line 50, clicking on y=10 should hit line 50
        let line = renderer.hit_test_fold_indicator(
            45.0,  // x in fold indicator area
            10.0,  // y (first line in viewport)
            50,    // first_visible_line
            10,    // visible_lines
            100,
            20.0,
            8.0,
        );
        assert_eq!(line, Some(50));
    }

    #[test]
    fn fold_indicator_bounds_basic() {
        let renderer = GutterRenderer::new();
        let (x, y, width, height) = renderer.fold_indicator_bounds(5, 0, 100, 20.0, 8.0);

        // Line number width: 8 + 24 + 8 = 40
        assert!((x - 40.0).abs() < 0.001);
        assert!((y - 100.0).abs() < 0.001); // line 5 * 20
        assert!((width - 16.0).abs() < 0.001); // FOLD_INDICATOR_WIDTH
        assert!((height - 20.0).abs() < 0.001); // line_height
    }
}
