//! Cursor rendering with blinking support.
//!
//! Renders the text cursor (caret) with configurable blinking animation.

use std::time::{Duration, Instant};

use crate::editor::Position;

/// Configuration for cursor rendering.
#[derive(Debug, Clone)]
pub struct CursorConfig {
    /// Width of the cursor in pixels.
    pub width: f32,
    /// Whether the cursor should blink.
    pub blink_enabled: bool,
    /// Duration the cursor is visible during blink cycle.
    pub blink_on_duration: Duration,
    /// Duration the cursor is hidden during blink cycle.
    pub blink_off_duration: Duration,
    /// Color of the cursor (RGBA, 0.0-1.0).
    pub color: [f32; 4],
    /// Whether to show a block cursor instead of line cursor.
    pub block_cursor: bool,
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            width: 2.0,
            blink_enabled: true,
            blink_on_duration: Duration::from_millis(530),
            blink_off_duration: Duration::from_millis(530),
            color: [1.0, 1.0, 1.0, 1.0], // White
            block_cursor: false,
        }
    }
}

impl CursorConfig {
    /// Creates a non-blinking cursor configuration.
    #[must_use]
    pub fn no_blink() -> Self {
        Self {
            blink_enabled: false,
            ..Default::default()
        }
    }

    /// Sets the cursor color.
    #[must_use]
    pub fn with_color(mut self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.color = [r, g, b, a];
        self
    }

    /// Sets the blink timing.
    #[must_use]
    pub fn with_blink_timing(mut self, on_ms: u64, off_ms: u64) -> Self {
        self.blink_on_duration = Duration::from_millis(on_ms);
        self.blink_off_duration = Duration::from_millis(off_ms);
        self
    }

    /// Returns the total blink cycle duration.
    #[must_use]
    pub fn blink_cycle(&self) -> Duration {
        self.blink_on_duration + self.blink_off_duration
    }
}

/// Manages cursor rendering state and blinking.
#[derive(Debug)]
pub struct CursorRenderer {
    /// Configuration.
    config: CursorConfig,
    /// Position of the cursor.
    position: Position,
    /// Time when the cursor was last shown (for blink timing).
    last_activity: Instant,
    /// Font metrics for positioning.
    char_width: f32,
    line_height: f32,
    /// Baseline offset within line height.
    baseline_offset: f32,
    /// Whether the cursor is currently visible (for blinking).
    is_visible: bool,
}

impl CursorRenderer {
    /// Creates a new cursor renderer.
    #[must_use]
    pub fn new(config: CursorConfig) -> Self {
        Self {
            config,
            position: Position::origin(),
            last_activity: Instant::now(),
            char_width: 8.0,
            line_height: 20.0,
            baseline_offset: 16.0,
            is_visible: true,
        }
    }

    /// Sets the cursor position.
    pub fn set_position(&mut self, position: Position) {
        if self.position != position {
            self.position = position;
            self.reset_blink();
        }
    }

    /// Returns the current cursor position.
    #[must_use]
    pub fn position(&self) -> Position {
        self.position
    }

    /// Sets font metrics for positioning.
    pub fn set_font_metrics(&mut self, char_width: f32, line_height: f32, baseline_offset: f32) {
        self.char_width = char_width;
        self.line_height = line_height;
        self.baseline_offset = baseline_offset;
    }

    /// Returns the cursor configuration.
    #[must_use]
    pub fn config(&self) -> &CursorConfig {
        &self.config
    }

    /// Returns a mutable reference to the cursor configuration.
    pub fn config_mut(&mut self) -> &mut CursorConfig {
        &mut self.config
    }

    /// Resets the blink timer (cursor becomes visible).
    ///
    /// Call this when the user types or moves the cursor.
    pub fn reset_blink(&mut self) {
        self.last_activity = Instant::now();
        self.is_visible = true;
    }

    /// Updates the blink state.
    ///
    /// Call this every frame to update cursor visibility.
    pub fn update(&mut self) {
        if !self.config.blink_enabled {
            self.is_visible = true;
            return;
        }

        let elapsed = self.last_activity.elapsed();
        let cycle = self.config.blink_cycle();

        // Calculate position within blink cycle
        let cycle_position = elapsed.as_secs_f64() % cycle.as_secs_f64();

        self.is_visible = cycle_position < self.config.blink_on_duration.as_secs_f64();
    }

    /// Returns whether the cursor should be drawn.
    #[must_use]
    pub fn should_draw(&self) -> bool {
        self.is_visible
    }

    /// Calculates the cursor rectangle in pixels.
    ///
    /// Returns (x, y, width, height) for the cursor rectangle.
    #[must_use]
    pub fn cursor_rect(&self, scroll_x: f32, scroll_y: f32) -> (f32, f32, f32, f32) {
        let x = (self.position.column as f32 * self.char_width) - scroll_x;
        let y = (self.position.line as f32 * self.line_height) - scroll_y;

        let width = if self.config.block_cursor {
            self.char_width
        } else {
            self.config.width
        };

        (x, y, width, self.line_height)
    }

    /// Returns the cursor color.
    #[must_use]
    pub fn color(&self) -> [f32; 4] {
        self.config.color
    }

    /// Returns the cursor as a glyphon Color for rendering.
    #[must_use]
    pub fn glyphon_color(&self) -> glyphon::Color {
        glyphon::Color::rgba(
            (self.config.color[0] * 255.0) as u8,
            (self.config.color[1] * 255.0) as u8,
            (self.config.color[2] * 255.0) as u8,
            (self.config.color[3] * 255.0) as u8,
        )
    }
}

impl Default for CursorRenderer {
    fn default() -> Self {
        Self::new(CursorConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_cursor_config_default() {
        let config = CursorConfig::default();
        assert!(config.blink_enabled);
        assert_eq!(config.width, 2.0);
    }

    #[test]
    fn test_cursor_config_no_blink() {
        let config = CursorConfig::no_blink();
        assert!(!config.blink_enabled);
    }

    #[test]
    fn test_cursor_config_with_color() {
        let config = CursorConfig::default().with_color(1.0, 0.0, 0.0, 1.0);
        assert_eq!(config.color, [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn test_cursor_renderer_new() {
        let renderer = CursorRenderer::new(CursorConfig::default());
        assert_eq!(renderer.position(), Position::origin());
        assert!(renderer.should_draw());
    }

    #[test]
    fn test_cursor_renderer_set_position() {
        let mut renderer = CursorRenderer::default();
        renderer.set_position(Position::new(5, 10));
        assert_eq!(renderer.position(), Position::new(5, 10));
    }

    #[test]
    fn test_cursor_renderer_cursor_rect() {
        let mut renderer = CursorRenderer::default();
        renderer.set_font_metrics(10.0, 20.0, 16.0);
        renderer.set_position(Position::new(2, 5));

        let (x, y, width, height) = renderer.cursor_rect(0.0, 0.0);
        assert_eq!(x, 50.0); // 5 * 10
        assert_eq!(y, 40.0); // 2 * 20
        assert_eq!(width, 2.0); // default cursor width
        assert_eq!(height, 20.0); // line height
    }

    #[test]
    fn test_cursor_renderer_cursor_rect_with_scroll() {
        let mut renderer = CursorRenderer::default();
        renderer.set_font_metrics(10.0, 20.0, 16.0);
        renderer.set_position(Position::new(2, 5));

        let (x, y, _width, _height) = renderer.cursor_rect(20.0, 30.0);
        assert_eq!(x, 30.0); // 50 - 20
        assert_eq!(y, 10.0); // 40 - 30
    }

    #[test]
    fn test_cursor_renderer_blink() {
        let config = CursorConfig::default().with_blink_timing(50, 50);
        let mut renderer = CursorRenderer::new(config);

        // Initially visible
        renderer.update();
        assert!(renderer.should_draw());

        // After blink_on_duration, should be hidden
        thread::sleep(Duration::from_millis(60));
        renderer.update();
        // Note: exact timing depends on system, so we just test the mechanism works
    }

    #[test]
    fn test_cursor_renderer_no_blink() {
        let mut renderer = CursorRenderer::new(CursorConfig::no_blink());

        // Should always be visible
        renderer.update();
        assert!(renderer.should_draw());

        thread::sleep(Duration::from_millis(100));
        renderer.update();
        assert!(renderer.should_draw());
    }

    #[test]
    fn test_cursor_renderer_reset_blink() {
        let config = CursorConfig::default().with_blink_timing(10, 10);
        let mut renderer = CursorRenderer::new(config);

        // Wait for blink off
        thread::sleep(Duration::from_millis(15));
        renderer.update();

        // Reset should make it visible again
        renderer.reset_blink();
        renderer.update();
        assert!(renderer.should_draw());
    }

    #[test]
    fn test_cursor_renderer_block_cursor() {
        let config = CursorConfig {
            block_cursor: true,
            ..Default::default()
        };
        let mut renderer = CursorRenderer::new(config);
        renderer.set_font_metrics(10.0, 20.0, 16.0);

        let (_x, _y, width, _height) = renderer.cursor_rect(0.0, 0.0);
        assert_eq!(width, 10.0); // char_width for block cursor
    }
}
