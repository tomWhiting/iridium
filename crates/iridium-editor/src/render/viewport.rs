//! Viewport and scroll management.

use ropey::Rope;
use serde::{Deserialize, Serialize};

use crate::document::Position;
use crate::editor::FoldState;

/// Configuration for viewport-aware rendering.
///
/// Controls how much content is pre-rendered beyond the visible viewport
/// to enable smooth scrolling without visual artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewportConfig {
    /// Overscan buffer as multiple of viewport height.
    ///
    /// A value of 2.0 means one full screen is pre-rendered above
    /// and one full screen below the visible viewport.
    ///
    /// Default: 2.0
    pub overscan_factor: f32,
}

impl Default for ViewportConfig {
    fn default() -> Self {
        Self {
            overscan_factor: 2.0,
        }
    }
}

impl ViewportConfig {
    /// Creates a new viewport config with the given overscan factor.
    #[must_use]
    pub const fn new(overscan_factor: f32) -> Self {
        Self { overscan_factor }
    }

    /// Creates a config with no overscan (visible viewport only).
    ///
    /// Useful for testing or constrained memory environments.
    #[must_use]
    pub const fn no_overscan() -> Self {
        Self {
            overscan_factor: 0.0,
        }
    }
}

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
            // `visible_lines` may be zero: chrome can consume every row a
            // terminal has, and a viewport that has not been sized yet starts
            // there. The outer `saturating_sub` never protected this inner one.
            self.scroll_to_line(
                position
                    .line
                    .saturating_sub(self.visible_lines.saturating_sub(1)),
            );
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
        fold_state
            .document_to_visual_line(line)
            .is_some_and(|visual_line| {
                visual_line >= self.first_line && visual_line < self.first_line + self.visible_lines
            })
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
            Self::find_enclosing_fold_start(position.line, fold_state).unwrap_or(position.line)
        } else {
            position.line
        };

        // Convert to visual line
        if let Some(visual_line) = fold_state.document_to_visual_line(target_line) {
            if visual_line < self.first_line {
                self.first_line = visual_line;
                self.scroll_offset_y = 0.0;
            } else if visual_line >= self.first_line + self.visible_lines {
                // See `scroll_to_position`: `visible_lines - 1` underflows on a
                // viewport with no room for text, which is a state every other
                // method here already answers rather than panics on.
                self.first_line = visual_line.saturating_sub(self.visible_lines.saturating_sub(1));
                self.scroll_offset_y = 0.0;
            }
        }
    }

    /// Finds the start line of the fold that contains the given line.
    fn find_enclosing_fold_start(line: usize, fold_state: &FoldState) -> Option<usize> {
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
        Some((screen_line as f32).mul_add(self.line_height, -self.scroll_offset_y))
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

    // =========================================================================
    // Byte range queries for viewport-aware syntax highlighting
    // =========================================================================

    /// Calculates the byte range to query for visible syntax spans.
    ///
    /// This method converts the current viewport (in visual lines) to a byte
    /// range suitable for querying the span index. It includes an overscan
    /// buffer to pre-render content for smooth scrolling.
    ///
    /// # Arguments
    ///
    /// * `rope` - Document rope for line-to-byte conversion
    /// * `fold_state` - Fold state for visual-to-document line mapping
    /// * `config` - Viewport configuration (overscan settings)
    ///
    /// # Returns
    ///
    /// A tuple `(start_byte, end_byte)` defining the byte range to query.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let (start, end) = viewport.query_byte_range(&rope, &fold_state, &config);
    /// let spans = span_index.query(start, end);
    /// ```
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    pub fn query_byte_range(
        &self,
        rope: &Rope,
        fold_state: &FoldState,
        config: &ViewportConfig,
    ) -> (usize, usize) {
        use ropey::LineType;

        let total_doc_lines = rope.len_lines(LineType::LF_CR);
        if total_doc_lines == 0 {
            return (0, 0);
        }

        // Calculate overscan in lines
        let overscan_lines =
            (self.visible_lines as f32 * config.overscan_factor / 2.0).ceil() as usize;

        // Calculate visual line range with overscan
        let first_visual_line = self.first_line.saturating_sub(overscan_lines);
        let last_visual_line = (self.first_line + self.visible_lines + overscan_lines)
            .min(fold_state.visible_line_count(total_doc_lines));

        // Convert visual lines to document lines
        let first_doc_line = fold_state.visual_to_document_line(first_visual_line);
        let last_doc_line = fold_state.visual_to_document_line(last_visual_line);

        // Clamp to document bounds
        let first_doc_line = first_doc_line.min(total_doc_lines.saturating_sub(1));
        let last_doc_line = last_doc_line.min(total_doc_lines);

        // Convert to byte offsets
        let start_byte = rope.line_to_byte_idx(first_doc_line, LineType::LF_CR);
        let end_byte = if last_doc_line >= total_doc_lines {
            rope.len()
        } else {
            rope.line_to_byte_idx(last_doc_line, LineType::LF_CR)
        };

        (start_byte, end_byte)
    }

    /// Returns the visible document line range with overscan.
    ///
    /// This is useful for debugging and testing the byte range calculation.
    ///
    /// # Arguments
    ///
    /// * `fold_state` - Fold state for visual-to-document line mapping
    /// * `config` - Viewport configuration
    /// * `total_doc_lines` - Total lines in document
    ///
    /// # Returns
    ///
    /// A tuple `(first_doc_line, last_doc_line)` inclusive range.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    pub fn visible_line_range(
        &self,
        fold_state: &FoldState,
        config: &ViewportConfig,
        total_doc_lines: usize,
    ) -> (usize, usize) {
        let overscan_lines =
            (self.visible_lines as f32 * config.overscan_factor / 2.0).ceil() as usize;

        let first_visual_line = self.first_line.saturating_sub(overscan_lines);
        let last_visual_line = (self.first_line + self.visible_lines + overscan_lines)
            .min(fold_state.visible_line_count(total_doc_lines));

        let first_doc_line = fold_state.visual_to_document_line(first_visual_line);
        let last_doc_line = fold_state.visual_to_document_line(last_visual_line);

        (
            first_doc_line.min(total_doc_lines.saturating_sub(1)),
            last_doc_line.min(total_doc_lines),
        )
    }
}

/// Iterator over visible document lines, accounting for folds.
struct VisibleLinesIterator<'a> {
    current_visual_line: usize,
    end_visual_line: usize,
    fold_state: &'a FoldState,
    total_lines: usize,
}

impl Iterator for VisibleLinesIterator<'_> {
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

    /// Parses `source` and refreshes `state`'s regions from the result.
    ///
    /// The detector borrows a tree rather than owning one, so a test that wants
    /// regions has to parse first — exactly as the editor does.
    fn update_regions_of(state: &mut FoldState, source: &str) -> bool {
        use std::borrow::Cow;

        use crate::editor::SyntaxDelta;
        #[cfg(not(feature = "syntax"))]
        use crate::syntax_stubs::SyntaxTree;
        #[cfg(feature = "syntax")]
        use iridium_syntax::SyntaxTree;

        let Some(language) = state.language() else {
            return false;
        };
        let Ok(mut tree) = SyntaxTree::new(language) else {
            return false;
        };
        let Some(parsed) = tree.parse(source) else {
            return false;
        };
        // A fresh parse of a fresh tree: nothing about the previous one is known,
        // which is exactly what `Full` says.
        state.update_regions(parsed, &SyntaxDelta::Full, || Cow::Borrowed(source))
    }
    use super::*;
    use ropey::Rope;

    // The crate re-exports `Language` from `iridium_syntax` or `syntax_stubs`
    // depending on the `syntax` feature, so these tests run in both
    // configurations rather than silently vanishing from the GPU-free build.
    use crate::Language;

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
        let code = r"line 0
fn foo() {
    hidden 1
    hidden 2
}
line 5";
        let mut state = FoldState::for_language(Language::Rust);
        update_regions_of(&mut state, code);
        state
    }

    #[test]
    #[cfg(feature = "syntax")]
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
    #[cfg(feature = "syntax")]
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
    #[cfg(feature = "syntax")]
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
    #[cfg(feature = "syntax")]
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

    // =========================================================================
    // Query byte range tests (T014)
    // =========================================================================

    #[test]
    fn query_byte_range_simple() {
        // Simple 5-line document, 10 chars per line
        let content = "0123456789\n0123456789\n0123456789\n0123456789\n0123456789\n";
        let rope = Rope::from_str(content);
        let fold_state = FoldState::default();
        let config = ViewportConfig::default();

        let viewport = Viewport {
            first_line: 0,
            visible_lines: 2,
            ..Viewport::default()
        };

        let (start, end) = viewport.query_byte_range(&rope, &fold_state, &config);

        // With 2x overscan and 2 visible lines, we query 1 line above + 2 visible + 1 line below
        // But since first_line is 0, there's nothing above
        assert_eq!(start, 0);
        // Should include some lines beyond visible (overscan)
        assert!(end > 22); // More than just 2 lines
    }

    #[test]
    fn query_byte_range_middle_viewport() {
        // 10-line document
        let content =
            "line 0\nline 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\n";
        let rope = Rope::from_str(content);
        let fold_state = FoldState::default();
        let config = ViewportConfig::default();

        let viewport = Viewport {
            first_line: 4, // Middle of document
            visible_lines: 2,
            ..Viewport::default()
        };

        let (start, end) = viewport.query_byte_range(&rope, &fold_state, &config);

        // Should start before line 4 (overscan)
        assert!(start < 28); // Line 4 starts at byte 28
        // Should end after line 5
        assert!(end > 42); // Line 6 starts at byte 42
    }

    #[test]
    fn query_byte_range_empty_document() {
        let rope = Rope::from_str("");
        let fold_state = FoldState::default();
        let config = ViewportConfig::default();

        let viewport = Viewport::default();

        let (start, end) = viewport.query_byte_range(&rope, &fold_state, &config);

        assert_eq!(start, 0);
        assert_eq!(end, 0);
    }

    #[test]
    fn query_byte_range_no_overscan() {
        let content = "line 0\nline 1\nline 2\nline 3\nline 4\n";
        let rope = Rope::from_str(content);
        let fold_state = FoldState::default();
        let config = ViewportConfig::no_overscan();

        let viewport = Viewport {
            first_line: 1,
            visible_lines: 2,
            ..Viewport::default()
        };

        let (start, end) = viewport.query_byte_range(&rope, &fold_state, &config);

        // Line 1 starts at byte 7
        assert_eq!(start, 7);
        // Line 3 starts at byte 21 (end of visible range)
        assert_eq!(end, 21);
    }

    #[test]
    #[cfg(feature = "syntax")]
    fn query_byte_range_with_folds() {
        let content = "line 0\nfn foo() {\n  hidden\n}\nline 4\n";
        let rope = Rope::from_str(content);
        let mut fold_state = FoldState::for_language(Language::Rust);
        update_regions_of(&mut fold_state, content);
        fold_state.fold_at(1); // Fold the function
        let config = ViewportConfig::no_overscan();

        let viewport = Viewport {
            first_line: 0,
            visible_lines: 3, // Can see: line 0, fn foo() { (folded), line 4
            ..Viewport::default()
        };

        let (start, end) = viewport.query_byte_range(&rope, &fold_state, &config);

        // Should start at beginning
        assert_eq!(start, 0);
        // Should include the folded content (for correct span queries)
        assert!(end > 30);
    }

    #[test]
    fn visible_line_range_basic() {
        let fold_state = FoldState::default();
        let config = ViewportConfig::default();

        let viewport = Viewport {
            first_line: 5,
            visible_lines: 10,
            ..Viewport::default()
        };

        let (first, last) = viewport.visible_line_range(&fold_state, &config, 100);

        // With 2x overscan and 10 visible lines, overscan is 10 lines
        // So range should be roughly [0, 20] (5-5=0 to 5+10+5=20)
        assert!(first < 5);
        assert!(last > 15);
    }

    /// A viewport with no room for text is a state every other method here
    /// already handles: `is_line_visible` answers "no", `visible_document_lines`
    /// yields nothing, `screen_y_for_line` returns `None`. It is reachable
    /// whenever chrome consumes every row — a terminal face with a status line
    /// and a search panel open on a short window — and the terminal face
    /// reaches it on its very first frame, before any resize has arrived.
    ///
    /// Both scroll methods guarded the *outer* subtraction with
    /// `saturating_sub` and left `visible_lines - 1` raw inside it, so both
    /// panicked here rather than declining to scroll.
    #[test]
    fn scrolling_a_viewport_with_no_visible_lines_does_not_underflow() {
        let fold_state = FoldState::default();
        let mut viewport = Viewport {
            first_line: 0,
            visible_lines: 0,
            ..Viewport::default()
        };

        viewport.scroll_to_position(Position::new(0, 0));
        assert_eq!(
            viewport.first_line, 0,
            "nothing is visible, so nothing is scrolled past"
        );

        viewport.scroll_to_position_with_folds(Position::new(7, 0), &fold_state);
        assert_eq!(
            viewport.first_line, 7,
            "the target lands at the top, which is where it will be when the \
             viewport next has a row to show it in"
        );
    }

    /// The one-row case is the boundary the fix has to keep: `visible_lines`
    /// of 1 must still put the target line at the top, not one line above it.
    #[test]
    fn scrolling_a_one_row_viewport_puts_the_target_on_that_row() {
        let fold_state = FoldState::default();
        let mut viewport = Viewport {
            first_line: 0,
            visible_lines: 1,
            ..Viewport::default()
        };

        viewport.scroll_to_position_with_folds(Position::new(4, 0), &fold_state);
        assert_eq!(viewport.first_line, 4);

        viewport.scroll_to_position(Position::new(9, 0));
        assert_eq!(viewport.first_line, 9);
    }
}
