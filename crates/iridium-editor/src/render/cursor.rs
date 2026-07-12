//! Cursor rendering with blinking caret support.
//!
//! This module handles rendering of the text cursor (caret) with configurable
//! blinking behavior and styling.

use std::time::Duration;

use web_time::Instant;

use crate::document::Position;
use crate::theme::Color;

/// Cursor blink state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlinkState {
    /// Cursor is visible
    Visible,
    /// Cursor is hidden (blink off phase)
    Hidden,
}

/// Cursor visual style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorStyle {
    /// Vertical line (I-beam)
    #[default]
    Line,
    /// Filled block
    Block,
    /// Underline
    Underline,
}

/// Configuration for cursor rendering.
#[derive(Debug, Clone)]
pub struct CursorConfig {
    /// Cursor style (line, block, or underline)
    pub style: CursorStyle,
    /// Cursor width for line style (pixels)
    pub line_width: f32,
    /// Blink interval (time for one on/off cycle)
    pub blink_interval: Duration,
    /// Whether blinking is enabled
    pub blink_enabled: bool,
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            style: CursorStyle::Line,
            line_width: 2.0,
            blink_interval: Duration::from_millis(1000),
            blink_enabled: true,
        }
    }
}

/// A single cursor to be rendered.
#[derive(Debug, Clone)]
pub struct CursorRect {
    /// X position in pixels
    pub x: f32,
    /// Y position in pixels
    pub y: f32,
    /// Width in pixels
    pub width: f32,
    /// Height in pixels
    pub height: f32,
    /// Cursor color
    pub color: Color,
}

impl CursorRect {
    /// Creates a new cursor rectangle.
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32, color: Color) -> Self {
        Self {
            x,
            y,
            width,
            height,
            color,
        }
    }
}

/// Cursor renderer with blink timing.
///
/// This renderer tracks cursor blink state based on time and provides
/// cursor rectangles for rendering. The host application is responsible
/// for actually drawing the rectangles (e.g., as GPU quads).
///
/// # Blink Behavior
///
/// - Cursor starts visible when created or reset
/// - Blinks at configured interval (default 500ms on, 500ms off)
/// - Blink resets (becomes visible) when cursor moves or text changes
/// - Blinking can be disabled for continuous visibility
///
/// # Example
///
/// ```ignore
/// let mut cursor_renderer = CursorRenderer::new(CursorConfig::default());
///
/// // On each frame, update and check visibility
/// if cursor_renderer.update(Instant::now()).is_visible() {
///     let rects = cursor_renderer.compute_cursors(
///         &cursor_state,
///         line_height,
///         char_width,
///         scroll_offset,
///         &theme.editor.cursor,
///     );
///     // Draw the cursor rectangles
/// }
/// ```
#[derive(Debug)]
pub struct CursorRenderer {
    /// Configuration
    config: CursorConfig,
    /// Time when blink cycle started
    blink_start: Instant,
    /// Current blink state
    blink_state: BlinkState,
    /// Whether cursor has focus (affects blinking)
    has_focus: bool,
}

impl Default for CursorRenderer {
    fn default() -> Self {
        Self::new(CursorConfig::default())
    }
}

impl CursorRenderer {
    /// Creates a new cursor renderer with the given configuration.
    #[must_use]
    pub fn new(config: CursorConfig) -> Self {
        Self {
            config,
            blink_start: Instant::now(),
            blink_state: BlinkState::Visible,
            has_focus: true,
        }
    }

    /// Updates the blink state based on current time.
    ///
    /// Returns the current blink state for convenience.
    pub fn update(&mut self, now: Instant) -> BlinkState {
        if !self.config.blink_enabled || !self.has_focus {
            self.blink_state = BlinkState::Visible;
            return self.blink_state;
        }

        let elapsed = now.duration_since(self.blink_start);
        let half_interval = self.config.blink_interval / 2;

        // Determine which phase of the blink cycle we're in
        let cycle_position = elapsed.as_millis() % self.config.blink_interval.as_millis();

        self.blink_state = if cycle_position < half_interval.as_millis() {
            BlinkState::Visible
        } else {
            BlinkState::Hidden
        };

        self.blink_state
    }

    /// Returns the current blink state.
    #[must_use]
    pub const fn blink_state(&self) -> BlinkState {
        self.blink_state
    }

    /// Returns true if the cursor should be visible.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.blink_state == BlinkState::Visible
    }

    /// Resets the blink cycle, making cursor immediately visible.
    ///
    /// Call this when the cursor moves or text changes to ensure
    /// the cursor is visible after edits.
    pub fn reset_blink(&mut self) {
        self.blink_start = Instant::now();
        self.blink_state = BlinkState::Visible;
    }

    /// Sets whether the editor has focus.
    ///
    /// When unfocused, the cursor doesn't blink and may be rendered differently.
    pub fn set_focus(&mut self, has_focus: bool) {
        self.has_focus = has_focus;
        if has_focus {
            self.reset_blink();
        }
    }

    /// Returns whether the editor has focus.
    #[must_use]
    pub const fn has_focus(&self) -> bool {
        self.has_focus
    }

    /// Sets the cursor style.
    pub const fn set_style(&mut self, style: CursorStyle) {
        self.config.style = style;
    }

    /// Returns the cursor style.
    #[must_use]
    pub const fn style(&self) -> CursorStyle {
        self.config.style
    }

    /// Enables or disables cursor blinking.
    pub const fn set_blink_enabled(&mut self, enabled: bool) {
        self.config.blink_enabled = enabled;
        if !enabled {
            self.blink_state = BlinkState::Visible;
        }
    }

    /// Returns whether blinking is enabled.
    #[must_use]
    pub const fn blink_enabled(&self) -> bool {
        self.config.blink_enabled
    }

    /// Computes cursor rectangles for rendering.
    ///
    /// # Arguments
    ///
    /// * `positions` - Iterator of cursor positions (document coordinates)
    /// * `line_height` - Height of a line in pixels
    /// * `char_width` - Width of a character in pixels (monospace assumed)
    /// * `scroll_x` - Horizontal scroll offset in pixels
    /// * `scroll_y` - Vertical scroll offset in pixels
    /// * `first_visible_line` - First visible line number
    /// * `visible_lines` - Number of visible lines
    /// * `color` - Cursor color
    ///
    /// Returns cursor rectangles in screen coordinates.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_cursors<'a>(
        &self,
        positions: impl Iterator<Item = &'a Position>,
        line_height: f32,
        char_width: f32,
        scroll_x: f32,
        first_visible_line: usize,
        visible_lines: usize,
        color: Color,
    ) -> Vec<CursorRect> {
        let last_visible_line = first_visible_line + visible_lines;

        positions
            .filter(|pos| pos.line >= first_visible_line && pos.line < last_visible_line)
            .map(|pos| {
                let screen_line = pos.line - first_visible_line;
                let x = (pos.column as f32).mul_add(char_width, -scroll_x);
                let y = screen_line as f32 * line_height;

                let (width, height) = match self.config.style {
                    CursorStyle::Line => (self.config.line_width, line_height),
                    CursorStyle::Block => (char_width, line_height),
                    CursorStyle::Underline => (char_width, 2.0),
                };

                let y = match self.config.style {
                    CursorStyle::Underline => y + line_height - 2.0,
                    _ => y,
                };

                CursorRect::new(x, y, width, height, color)
            })
            .collect()
    }

    /// Computes a single cursor rectangle for the primary cursor.
    ///
    /// This is a convenience method for single-cursor scenarios.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_primary_cursor(
        &self,
        position: Position,
        line_height: f32,
        char_width: f32,
        scroll_x: f32,
        first_visible_line: usize,
        visible_lines: usize,
        color: Color,
    ) -> Option<CursorRect> {
        self.compute_cursors(
            std::iter::once(&position),
            line_height,
            char_width,
            scroll_x,
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

    #[test]
    fn cursor_default_config() {
        let config = CursorConfig::default();
        assert_eq!(config.style, CursorStyle::Line);
        assert!(config.blink_enabled);
    }

    #[test]
    fn cursor_starts_visible() {
        let renderer = CursorRenderer::default();
        assert!(renderer.is_visible());
    }

    #[test]
    fn cursor_blink_cycle() {
        let config = CursorConfig {
            blink_interval: Duration::from_millis(100),
            blink_enabled: true,
            ..Default::default()
        };
        let mut renderer = CursorRenderer::new(config);
        let start = Instant::now();

        // Initially visible
        renderer.update(start);
        assert!(renderer.is_visible());

        // After 25ms, still visible (in first half)
        renderer.update(start + Duration::from_millis(25));
        assert!(renderer.is_visible());

        // After 75ms, hidden (in second half)
        renderer.update(start + Duration::from_millis(75));
        assert!(!renderer.is_visible());

        // After 125ms, visible again (new cycle)
        renderer.update(start + Duration::from_millis(125));
        assert!(renderer.is_visible());
    }

    #[test]
    fn cursor_reset_blink() {
        let config = CursorConfig {
            blink_interval: Duration::from_millis(100),
            blink_enabled: true,
            ..Default::default()
        };
        let mut renderer = CursorRenderer::new(config);
        let start = Instant::now();

        // Move into hidden phase
        renderer.update(start + Duration::from_millis(75));
        assert!(!renderer.is_visible());

        // Reset blink
        renderer.reset_blink();
        assert!(renderer.is_visible());
    }

    #[test]
    fn cursor_no_blink_without_focus() {
        let mut renderer = CursorRenderer::default();
        renderer.set_focus(false);

        // Even in what would be hidden phase, stays visible without focus
        let start = renderer.blink_start;
        renderer.update(start + Duration::from_millis(750));
        assert!(renderer.is_visible());
    }

    #[test]
    fn cursor_rect_computation() {
        let renderer = CursorRenderer::default();
        let positions = [Position::new(5, 10)];
        let line_height = 20.0;
        let char_width = 8.0;
        let color = Color::rgb(1.0, 1.0, 1.0);

        let rects = renderer.compute_cursors(
            positions.iter(),
            line_height,
            char_width,
            0.0, // scroll_x
            0,   // first_visible_line
            50,  // visible_lines
            color,
        );

        assert_eq!(rects.len(), 1);
        let rect = &rects[0];
        assert!((rect.x - 80.0).abs() < 0.001); // column 10 * 8 char_width
        assert!((rect.y - 100.0).abs() < 0.001); // line 5 * 20 line_height
    }

    #[test]
    fn cursor_filters_invisible_lines() {
        let renderer = CursorRenderer::default();
        let positions = vec![
            Position::new(0, 0),   // Before viewport
            Position::new(10, 0),  // In viewport
            Position::new(100, 0), // After viewport
        ];
        let line_height = 20.0;
        let char_width = 8.0;
        let color = Color::rgb(1.0, 1.0, 1.0);

        let rects = renderer.compute_cursors(
            positions.iter(),
            line_height,
            char_width,
            0.0,
            5,  // first_visible_line
            20, // visible_lines (5-24)
            color,
        );

        // Only position at line 10 is in viewport (5..25)
        assert_eq!(rects.len(), 1);
    }

    #[test]
    fn cursor_style_block() {
        let config = CursorConfig {
            style: CursorStyle::Block,
            ..Default::default()
        };
        let renderer = CursorRenderer::new(config);
        let positions = [Position::new(0, 0)];
        let line_height = 20.0;
        let char_width = 8.0;
        let color = Color::rgb(1.0, 1.0, 1.0);

        let rects =
            renderer.compute_cursors(positions.iter(), line_height, char_width, 0.0, 0, 50, color);

        assert_eq!(rects.len(), 1);
        // Block cursor should have char_width width
        assert!((rects[0].width - char_width).abs() < 0.001);
    }
}
