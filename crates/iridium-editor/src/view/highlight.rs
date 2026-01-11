//! Selection and line highlighting.
//!
//! Renders selection highlights and current line highlighting.

use crate::editor::{Position, Selection};

/// Configuration for highlighting.
#[derive(Debug, Clone)]
pub struct HighlightConfig {
    /// Selection highlight color (RGBA, 0.0-1.0).
    pub selection_color: [f32; 4],
    /// Current line highlight color (RGBA, 0.0-1.0).
    pub current_line_color: [f32; 4],
    /// Whether to highlight the current line.
    pub highlight_current_line: bool,
}

impl Default for HighlightConfig {
    fn default() -> Self {
        Self {
            // Semi-transparent blue selection
            selection_color: [0.25, 0.47, 0.77, 0.4],
            // Very subtle current line highlight
            current_line_color: [1.0, 1.0, 1.0, 0.05],
            highlight_current_line: true,
        }
    }
}

impl HighlightConfig {
    /// Creates a dark theme highlight configuration.
    #[must_use]
    pub fn dark_theme() -> Self {
        Self {
            selection_color: [0.25, 0.47, 0.77, 0.4],
            current_line_color: [1.0, 1.0, 1.0, 0.05],
            highlight_current_line: true,
        }
    }

    /// Creates a light theme highlight configuration.
    #[must_use]
    pub fn light_theme() -> Self {
        Self {
            selection_color: [0.18, 0.40, 0.85, 0.3],
            current_line_color: [0.0, 0.0, 0.0, 0.04],
            highlight_current_line: true,
        }
    }

    /// Sets the selection color.
    #[must_use]
    pub fn with_selection_color(mut self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.selection_color = [r, g, b, a];
        self
    }

    /// Sets the current line color.
    #[must_use]
    pub fn with_current_line_color(mut self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.current_line_color = [r, g, b, a];
        self
    }
}

/// A rectangle to be rendered as a highlight.
#[derive(Debug, Clone, Copy)]
pub struct HighlightRect {
    /// X coordinate in pixels.
    pub x: f32,
    /// Y coordinate in pixels.
    pub y: f32,
    /// Width in pixels.
    pub width: f32,
    /// Height in pixels.
    pub height: f32,
    /// Color (RGBA, 0.0-1.0).
    pub color: [f32; 4],
}

impl HighlightRect {
    /// Creates a new highlight rectangle.
    #[must_use]
    pub fn new(x: f32, y: f32, width: f32, height: f32, color: [f32; 4]) -> Self {
        Self {
            x,
            y,
            width,
            height,
            color,
        }
    }

    /// Returns true if this rectangle is visible (has positive dimensions).
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.width > 0.0 && self.height > 0.0 && self.color[3] > 0.0
    }
}

/// Generates highlight rectangles for rendering.
#[derive(Debug)]
pub struct HighlightRenderer {
    /// Configuration.
    config: HighlightConfig,
    /// Font metrics for positioning.
    char_width: f32,
    line_height: f32,
    /// Viewport dimensions.
    viewport_width: f32,
    viewport_height: f32,
}

impl HighlightRenderer {
    /// Creates a new highlight renderer.
    #[must_use]
    pub fn new(config: HighlightConfig) -> Self {
        Self {
            config,
            char_width: 8.0,
            line_height: 20.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
        }
    }

    /// Sets font metrics for positioning.
    pub fn set_font_metrics(&mut self, char_width: f32, line_height: f32) {
        self.char_width = char_width;
        self.line_height = line_height;
    }

    /// Sets viewport dimensions.
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport_width = width;
        self.viewport_height = height;
    }

    /// Returns the configuration.
    #[must_use]
    pub fn config(&self) -> &HighlightConfig {
        &self.config
    }

    /// Returns a mutable reference to the configuration.
    pub fn config_mut(&mut self) -> &mut HighlightConfig {
        &mut self.config
    }

    /// Generates the current line highlight rectangle.
    ///
    /// Returns None if current line highlighting is disabled.
    #[must_use]
    pub fn current_line_highlight(
        &self,
        cursor_line: usize,
        scroll_x: f32,
        scroll_y: f32,
    ) -> Option<HighlightRect> {
        if !self.config.highlight_current_line {
            return None;
        }

        let y = (cursor_line as f32 * self.line_height) - scroll_y;

        // Don't render if off-screen
        if y + self.line_height < 0.0 || y > self.viewport_height {
            return None;
        }

        Some(HighlightRect::new(
            -scroll_x, // Start from the left edge
            y,
            self.viewport_width + scroll_x, // Extend to right edge
            self.line_height,
            self.config.current_line_color,
        ))
    }

    /// Generates selection highlight rectangles.
    ///
    /// Returns a list of rectangles covering the selection. Multi-line selections
    /// produce multiple rectangles.
    #[must_use]
    pub fn selection_highlights(
        &self,
        selection: Option<Selection>,
        scroll_x: f32,
        scroll_y: f32,
        line_lengths: &[usize],
    ) -> Vec<HighlightRect> {
        let selection = match selection {
            Some(sel) if sel.is_selection() => sel,
            _ => return Vec::new(),
        };

        let start = selection.start();
        let end = selection.end();

        let mut rects = Vec::new();

        // Calculate visible line range
        let first_visible_line = (scroll_y / self.line_height).floor() as usize;
        let last_visible_line =
            ((scroll_y + self.viewport_height) / self.line_height).ceil() as usize;

        for line in start.line..=end.line {
            // Skip lines outside visible range
            if line < first_visible_line || line > last_visible_line {
                continue;
            }

            let line_len = line_lengths.get(line).copied().unwrap_or(0);

            // Determine start column for this line
            let start_col = if line == start.line { start.column } else { 0 };

            // Determine end column for this line
            let end_col = if line == end.line {
                end.column
            } else {
                // Include the newline character in the selection visual
                line_len + 1
            };

            // Skip if no selection on this line
            if start_col >= end_col {
                continue;
            }

            let x = (start_col as f32 * self.char_width) - scroll_x;
            let y = (line as f32 * self.line_height) - scroll_y;
            let width = ((end_col - start_col) as f32 * self.char_width).max(self.char_width);

            rects.push(HighlightRect::new(
                x,
                y,
                width,
                self.line_height,
                self.config.selection_color,
            ));
        }

        rects
    }

    /// Generates all highlight rectangles for the current frame.
    ///
    /// Returns highlights in order: current line (back), then selection (front).
    #[must_use]
    pub fn generate_highlights(
        &self,
        cursor_position: Position,
        selection: Option<Selection>,
        scroll_x: f32,
        scroll_y: f32,
        line_lengths: &[usize],
    ) -> Vec<HighlightRect> {
        let mut highlights = Vec::new();

        // Current line highlight (drawn first, behind text)
        if let Some(rect) = self.current_line_highlight(cursor_position.line, scroll_x, scroll_y) {
            highlights.push(rect);
        }

        // Selection highlights (drawn second, also behind text)
        highlights.extend(self.selection_highlights(selection, scroll_x, scroll_y, line_lengths));

        highlights
    }
}

impl Default for HighlightRenderer {
    fn default() -> Self {
        Self::new(HighlightConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_config_default() {
        let config = HighlightConfig::default();
        assert!(config.highlight_current_line);
        assert!(config.selection_color[3] > 0.0);
    }

    #[test]
    fn test_highlight_rect_is_visible() {
        let visible = HighlightRect::new(0.0, 0.0, 100.0, 20.0, [1.0, 0.0, 0.0, 1.0]);
        assert!(visible.is_visible());

        let zero_width = HighlightRect::new(0.0, 0.0, 0.0, 20.0, [1.0, 0.0, 0.0, 1.0]);
        assert!(!zero_width.is_visible());

        let transparent = HighlightRect::new(0.0, 0.0, 100.0, 20.0, [1.0, 0.0, 0.0, 0.0]);
        assert!(!transparent.is_visible());
    }

    #[test]
    fn test_highlight_renderer_current_line() {
        let mut renderer = HighlightRenderer::default();
        renderer.set_font_metrics(10.0, 20.0);
        renderer.set_viewport(800.0, 600.0);

        let rect = renderer.current_line_highlight(5, 0.0, 0.0);
        assert!(rect.is_some());

        let rect = rect.unwrap();
        assert_eq!(rect.y, 100.0); // 5 * 20
        assert_eq!(rect.height, 20.0);
    }

    #[test]
    fn test_highlight_renderer_current_line_off_screen() {
        let mut renderer = HighlightRenderer::default();
        renderer.set_font_metrics(10.0, 20.0);
        renderer.set_viewport(800.0, 600.0);

        // Line is above viewport
        let rect = renderer.current_line_highlight(0, 0.0, 100.0);
        assert!(rect.is_none());
    }

    #[test]
    fn test_highlight_renderer_selection_single_line() {
        let mut renderer = HighlightRenderer::default();
        renderer.set_font_metrics(10.0, 20.0);
        renderer.set_viewport(800.0, 600.0);

        let selection = Selection::new(Position::new(0, 5), Position::new(0, 10));
        let line_lengths = vec![20];

        let rects = renderer.selection_highlights(Some(selection), 0.0, 0.0, &line_lengths);
        assert_eq!(rects.len(), 1);

        let rect = &rects[0];
        assert_eq!(rect.x, 50.0); // 5 * 10
        assert_eq!(rect.y, 0.0);
        assert_eq!(rect.width, 50.0); // 5 chars * 10
    }

    #[test]
    fn test_highlight_renderer_selection_multi_line() {
        let mut renderer = HighlightRenderer::default();
        renderer.set_font_metrics(10.0, 20.0);
        renderer.set_viewport(800.0, 600.0);

        let selection = Selection::new(Position::new(0, 5), Position::new(2, 10));
        let line_lengths = vec![20, 15, 25];

        let rects = renderer.selection_highlights(Some(selection), 0.0, 0.0, &line_lengths);
        assert_eq!(rects.len(), 3);

        // First line: from column 5 to end (column 21 with newline)
        assert_eq!(rects[0].x, 50.0);
        assert_eq!(rects[0].y, 0.0);

        // Second line: full line
        assert_eq!(rects[1].x, 0.0);
        assert_eq!(rects[1].y, 20.0);

        // Third line: from start to column 10
        assert_eq!(rects[2].x, 0.0);
        assert_eq!(rects[2].y, 40.0);
    }

    #[test]
    fn test_highlight_renderer_no_selection() {
        let renderer = HighlightRenderer::default();
        let line_lengths = vec![20];

        // No selection
        let rects = renderer.selection_highlights(None, 0.0, 0.0, &line_lengths);
        assert!(rects.is_empty());

        // Cursor (not a selection)
        let cursor = Selection::cursor(Position::new(0, 5));
        let rects = renderer.selection_highlights(Some(cursor), 0.0, 0.0, &line_lengths);
        assert!(rects.is_empty());
    }

    #[test]
    fn test_highlight_renderer_generate_highlights() {
        let mut renderer = HighlightRenderer::default();
        renderer.set_font_metrics(10.0, 20.0);
        renderer.set_viewport(800.0, 600.0);

        let cursor = Position::new(1, 5);
        let selection = Selection::new(Position::new(0, 0), Position::new(0, 10));
        let line_lengths = vec![20, 15];

        let highlights =
            renderer.generate_highlights(cursor, Some(selection), 0.0, 0.0, &line_lengths);

        // Should have current line + selection
        assert_eq!(highlights.len(), 2);

        // First is current line
        assert_eq!(highlights[0].y, 20.0);
        // Second is selection
        assert_eq!(highlights[1].y, 0.0);
    }
}
