//! Viewport and scroll management.

use serde::{Deserialize, Serialize};

use crate::document::Position;

/// The visible region of the document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    /// First visible line (0-indexed)
    pub first_line: usize,
    /// Scroll offset within first line (pixels)
    pub scroll_offset_y: f32,
    /// Horizontal scroll offset (pixels)
    pub scroll_offset_x: f32,
    /// Viewport width in pixels
    pub width: f32,
    /// Viewport height in pixels
    pub height: f32,
    /// Number of visible lines (computed)
    pub visible_lines: usize,
    /// Line height in pixels
    pub line_height: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            first_line: 0,
            scroll_offset_y: 0.0,
            scroll_offset_x: 0.0,
            width: 800.0,
            height: 600.0,
            visible_lines: 40,
            line_height: 20.0,
        }
    }
}

impl Viewport {
    /// Creates a new viewport with the given dimensions.
    #[must_use]
    pub fn new(width: f32, height: f32, line_height: f32) -> Self {
        let visible_lines = (height / line_height).ceil() as usize;
        Self { width, height, line_height, visible_lines, ..Self::default() }
    }

    /// Returns true if the given line is visible.
    #[must_use]
    pub fn contains_line(&self, line: usize) -> bool {
        line >= self.first_line && line < self.first_line + self.visible_lines
    }

    /// Scrolls to make a specific line the first visible line.
    pub fn scroll_to_line(&mut self, line: usize) {
        self.first_line = line;
        self.scroll_offset_y = 0.0;
    }

    /// Scrolls to make a position visible.
    pub fn scroll_to_position(&mut self, position: Position) {
        if position.line < self.first_line {
            self.scroll_to_line(position.line);
        } else if position.line >= self.first_line + self.visible_lines {
            self.scroll_to_line(position.line.saturating_sub(self.visible_lines - 1));
        }
    }

    /// Ensures the cursor is visible, scrolling if necessary.
    pub fn ensure_cursor_visible(&mut self, cursor: Position) {
        self.scroll_to_position(cursor);
    }

    /// Updates the viewport size.
    pub fn resize(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
        self.visible_lines = (height / self.line_height).ceil() as usize;
    }

    /// Returns the last visible line number.
    #[must_use]
    pub fn last_visible_line(&self) -> usize {
        self.first_line + self.visible_lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_contains_line() {
        let viewport = Viewport { first_line: 10, visible_lines: 20, ..Viewport::default() };

        assert!(!viewport.contains_line(5));
        assert!(viewport.contains_line(10));
        assert!(viewport.contains_line(20));
        assert!(!viewport.contains_line(30));
    }

    #[test]
    fn viewport_scroll_to_position() {
        let mut viewport = Viewport { first_line: 10, visible_lines: 20, ..Viewport::default() };

        // Position above viewport
        viewport.scroll_to_position(Position::new(5, 0));
        assert_eq!(viewport.first_line, 5);

        // Position below viewport
        viewport.scroll_to_position(Position::new(50, 0));
        assert_eq!(viewport.first_line, 31);
    }
}
