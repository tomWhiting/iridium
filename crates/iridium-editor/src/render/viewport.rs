//! Viewport and scroll management.

use serde::{Deserialize, Serialize};

use crate::document::Position;
use crate::editor::FoldState;

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
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn new(width: f32, height: f32, line_height: f32) -> Self {
        let visible_lines = (height / line_height).ceil() as usize;
        Self {
            width,
            height,
            visible_lines,
            line_height,
            ..Self::default()
        }
    }

    /// Returns true if the given line is visible.
    #[must_use]
    pub const fn contains_line(&self, line: usize) -> bool {
        line >= self.first_line && line < self.first_line + self.visible_lines
    }

    /// Scrolls to make a specific line the first visible line.
    pub const fn scroll_to_line(&mut self, line: usize) {
        self.first_line = line;
        self.scroll_offset_y = 0.0;
    }

    /// Scrolls to make a position visible.
    pub const fn scroll_to_position(&mut self, position: Position) {
        if position.line < self.first_line {
            self.scroll_to_line(position.line);
        } else if position.line >= self.first_line + self.visible_lines {
            self.scroll_to_line(position.line.saturating_sub(self.visible_lines - 1));
        }
    }

    /// Ensures the cursor is visible, scrolling if necessary.
    pub const fn ensure_cursor_visible(&mut self, cursor: Position) {
        self.scroll_to_position(cursor);
    }

    /// Updates the viewport size.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn resize(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
        self.visible_lines = (height / self.line_height).ceil() as usize;
    }

    /// Returns the last visible line number.
    #[must_use]
    pub const fn last_visible_line(&self) -> usize {
        self.first_line + self.visible_lines
    }

    // =========================================================================
    // Fold-aware viewport operations (T137)
    // =========================================================================

    /// Returns true if the given line is visible, accounting for folds.
    ///
    /// A line is visible if it's within the viewport range AND not hidden by a fold.
    #[must_use]
    pub fn is_line_visible_with_folds(&self, line: usize, fold_state: &FoldState) -> bool {
        // Hidden lines are never visible
        if fold_state.is_line_hidden(line) {
            return false;
        }

        // Convert document line to visual line
        if let Some(visual_line) = fold_state.document_to_visual_line(line) {
            visual_line >= self.first_line && visual_line < self.first_line + self.visible_lines
        } else {
            false
        }
    }

    /// Returns an iterator over visible document lines, accounting for folds.
    ///
    /// This returns the document line numbers that should be rendered,
    /// skipping lines hidden by folds.
    pub fn visible_document_lines<'a>(
        &'a self,
        fold_state: &'a FoldState,
        total_lines: usize,
    ) -> impl Iterator<Item = usize> + 'a {
        VisibleLinesIterator {
            current_visual_line: self.first_line,
            end_visual_line: self.first_line + self.visible_lines,
            fold_state,
            total_lines,
        }
    }

    /// Scrolls to make a position visible, accounting for folds.
    ///
    /// If the position is hidden by a fold, scrolls to the fold's start line.
    pub fn scroll_to_position_with_folds(&mut self, position: Position, fold_state: &FoldState) {
        let target_line = if fold_state.is_line_hidden(position.line) {
            // Find the fold that hides this line and scroll to its start
            self.find_enclosing_fold_start(position.line, fold_state)
                .unwrap_or(position.line)
        } else {
            position.line
        };

        // Convert to visual line
        if let Some(visual_line) = fold_state.document_to_visual_line(target_line) {
            if visual_line < self.first_line {
                self.first_line = visual_line;
                self.scroll_offset_y = 0.0;
            } else if visual_line >= self.first_line + self.visible_lines {
                self.first_line = visual_line.saturating_sub(self.visible_lines - 1);
                self.scroll_offset_y = 0.0;
            }
        }
    }

    /// Finds the start line of the fold that contains the given line.
    fn find_enclosing_fold_start(&self, line: usize, fold_state: &FoldState) -> Option<usize> {
        for folded_line in fold_state.folded_lines() {
            if let Some(region) = fold_state.region_at(folded_line) {
                if line > region.start_line && line <= region.end_line {
                    return Some(region.start_line);
                }
            }
        }
        None
    }

    /// Returns the document line at a given screen Y position, accounting for folds.
    ///
    /// # Arguments
    ///
    /// * `screen_y` - Y coordinate in screen pixels
    /// * `fold_state` - Current fold state
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    pub fn document_line_at_y(&self, screen_y: f32, fold_state: &FoldState) -> usize {
        let visual_offset = ((screen_y + self.scroll_offset_y) / self.line_height).floor() as usize;
        let visual_line = self.first_line + visual_offset;
        fold_state.visual_to_document_line(visual_line)
    }

    /// Returns the screen Y position for a document line, accounting for folds.
    ///
    /// Returns None if the line is hidden by a fold.
    ///
    /// # Arguments
    ///
    /// * `doc_line` - Document line number (0-indexed)
    /// * `fold_state` - Current fold state
    #[allow(clippy::cast_precision_loss)]
    pub fn screen_y_for_line(&self, doc_line: usize, fold_state: &FoldState) -> Option<f32> {
        if fold_state.is_line_hidden(doc_line) {
            return None;
        }

        let visual_line = fold_state.document_to_visual_line(doc_line)?;
        if visual_line < self.first_line || visual_line >= self.first_line + self.visible_lines {
            return None;
        }

        let screen_line = visual_line - self.first_line;
        Some(screen_line as f32 * self.line_height - self.scroll_offset_y)
    }

    /// Returns the number of visible lines accounting for folds.
    ///
    /// This may be less than `visible_lines` if the document is shorter
    /// when folds are applied.
    #[must_use]
    pub fn visible_line_count_with_folds(
        &self,
        fold_state: &FoldState,
        total_lines: usize,
    ) -> usize {
        let total_visible = fold_state.visible_line_count(total_lines);
        let available = total_visible.saturating_sub(self.first_line);
        available.min(self.visible_lines)
    }
}

/// Iterator over visible document lines, accounting for folds.
struct VisibleLinesIterator<'a> {
    current_visual_line: usize,
    end_visual_line: usize,
    fold_state: &'a FoldState,
    total_lines: usize,
}

impl<'a> Iterator for VisibleLinesIterator<'a> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_visual_line >= self.end_visual_line {
            return None;
        }

        let doc_line = self
            .fold_state
            .visual_to_document_line(self.current_visual_line);

        // Check if we've exceeded document bounds
        if doc_line >= self.total_lines {
            return None;
        }

        self.current_visual_line += 1;
        Some(doc_line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iridium_syntax::Language;

    #[test]
    fn viewport_contains_line() {
        let viewport = Viewport {
            first_line: 10,
            visible_lines: 20,
            ..Viewport::default()
        };

        assert!(!viewport.contains_line(5));
        assert!(viewport.contains_line(10));
        assert!(viewport.contains_line(20));
        assert!(!viewport.contains_line(30));
    }

    #[test]
    fn viewport_scroll_to_position() {
        let mut viewport = Viewport {
            first_line: 10,
            visible_lines: 20,
            ..Viewport::default()
        };

        // Position above viewport
        viewport.scroll_to_position(Position::new(5, 0));
        assert_eq!(viewport.first_line, 5);

        // Position below viewport
        viewport.scroll_to_position(Position::new(50, 0));
        assert_eq!(viewport.first_line, 31);
    }

    fn setup_fold_state() -> FoldState {
        let code = r#"line 0
fn foo() {
    hidden 1
    hidden 2
}
line 5"#;
        let mut state = FoldState::for_language(Language::Rust);
        state.update_regions(code);
        state
    }

    #[test]
    fn is_line_visible_with_folds() {
        let viewport = Viewport {
            first_line: 0,
            visible_lines: 10,
            ..Viewport::default()
        };
        let mut fold_state = setup_fold_state();

        // Without folds, all lines visible
        assert!(viewport.is_line_visible_with_folds(0, &fold_state));
        assert!(viewport.is_line_visible_with_folds(1, &fold_state));
        assert!(viewport.is_line_visible_with_folds(5, &fold_state));

        // With fold at line 1, lines 2-4 are hidden
        fold_state.fold_at(1);
        assert!(viewport.is_line_visible_with_folds(0, &fold_state));
        assert!(viewport.is_line_visible_with_folds(1, &fold_state)); // Fold start is visible
        assert!(!viewport.is_line_visible_with_folds(2, &fold_state)); // Hidden
        assert!(!viewport.is_line_visible_with_folds(3, &fold_state)); // Hidden
        assert!(viewport.is_line_visible_with_folds(5, &fold_state)); // After fold
    }

    #[test]
    fn visible_document_lines_iterator() {
        let viewport = Viewport {
            first_line: 0,
            visible_lines: 10,
            ..Viewport::default()
        };
        let mut fold_state = setup_fold_state();
        fold_state.fold_at(1);

        let lines: Vec<usize> = viewport.visible_document_lines(&fold_state, 6).collect();

        // Should get: 0, 1 (fold start), 5 (skipping hidden 2,3,4)
        assert_eq!(lines, vec![0, 1, 5]);
    }

    #[test]
    fn document_line_at_y_with_folds() {
        let viewport = Viewport {
            first_line: 0,
            visible_lines: 10,
            line_height: 20.0,
            scroll_offset_y: 0.0,
            ..Viewport::default()
        };
        let mut fold_state = setup_fold_state();
        fold_state.fold_at(1);

        // y=0 should be doc line 0
        assert_eq!(viewport.document_line_at_y(0.0, &fold_state), 0);

        // y=20 should be doc line 1 (fold start)
        assert_eq!(viewport.document_line_at_y(20.0, &fold_state), 1);

        // y=40 should be doc line 5 (skipping hidden lines)
        assert_eq!(viewport.document_line_at_y(40.0, &fold_state), 5);
    }

    #[test]
    fn screen_y_for_line_with_folds() {
        let viewport = Viewport {
            first_line: 0,
            visible_lines: 10,
            line_height: 20.0,
            scroll_offset_y: 0.0,
            ..Viewport::default()
        };
        let mut fold_state = setup_fold_state();
        fold_state.fold_at(1);

        // Line 0 should be at y=0
        assert_eq!(viewport.screen_y_for_line(0, &fold_state), Some(0.0));

        // Line 1 (fold start) should be at y=20
        assert_eq!(viewport.screen_y_for_line(1, &fold_state), Some(20.0));

        // Lines 2-4 are hidden, should return None
        assert_eq!(viewport.screen_y_for_line(2, &fold_state), None);
        assert_eq!(viewport.screen_y_for_line(3, &fold_state), None);
        assert_eq!(viewport.screen_y_for_line(4, &fold_state), None);

        // Line 5 should be at y=40 (visual line 2)
        assert_eq!(viewport.screen_y_for_line(5, &fold_state), Some(40.0));
    }

    #[test]
    fn visible_line_count_with_folds() {
        let viewport = Viewport {
            first_line: 0,
            visible_lines: 10,
            ..Viewport::default()
        };
        let mut fold_state = setup_fold_state();

        // Without folds, we can show up to 6 lines (total doc lines)
        assert_eq!(viewport.visible_line_count_with_folds(&fold_state, 6), 6);

        // With fold, we can show 3 visible lines (0, 1, 5)
        fold_state.fold_at(1);
        assert_eq!(viewport.visible_line_count_with_folds(&fold_state, 6), 3);
    }
}
