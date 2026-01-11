//! Viewport management for scrolling and visible region tracking.
//!
//! The viewport represents the visible region of the editor and manages
//! scrolling, including smooth momentum scrolling for trackpads.

use std::time::Instant;

use crate::editor::Position;

/// Scroll direction for scroll events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    /// Scroll up (content moves down, viewport moves up).
    Up,
    /// Scroll down (content moves up, viewport moves down).
    Down,
    /// Scroll left.
    Left,
    /// Scroll right.
    Right,
}

/// Configuration for viewport scrolling behavior.
#[derive(Debug, Clone)]
pub struct ScrollConfig {
    /// Pixels per line for discrete scroll events (mouse wheel).
    pub pixels_per_line: f32,
    /// Number of lines to scroll per wheel notch.
    pub lines_per_scroll: usize,
    /// Whether to enable smooth (momentum) scrolling.
    pub smooth_scrolling: bool,
    /// Friction coefficient for momentum decay (0.0-1.0, higher = more friction).
    pub momentum_friction: f32,
    /// Minimum velocity below which momentum stops (pixels/second).
    pub momentum_min_velocity: f32,
    /// Horizontal scroll margin (pixels from edge before horizontal scroll).
    pub horizontal_margin: f32,
    /// Vertical scroll margin (lines from edge before vertical scroll).
    pub vertical_margin_lines: usize,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            pixels_per_line: 20.0, // Will be updated with actual line height
            lines_per_scroll: 3,
            smooth_scrolling: true,
            momentum_friction: 0.92, // Smooth decay
            momentum_min_velocity: 10.0,
            horizontal_margin: 20.0,
            vertical_margin_lines: 3,
        }
    }
}

/// Momentum state for smooth scrolling.
#[derive(Debug, Clone)]
struct MomentumState {
    /// Velocity in pixels per second (x, y).
    velocity: (f32, f32),
    /// Time of last momentum update.
    last_update: Instant,
    /// Whether momentum is active.
    active: bool,
}

impl Default for MomentumState {
    fn default() -> Self {
        Self {
            velocity: (0.0, 0.0),
            last_update: Instant::now(),
            active: false,
        }
    }
}

impl MomentumState {
    fn start(&mut self, velocity_x: f32, velocity_y: f32) {
        self.velocity = (velocity_x, velocity_y);
        self.last_update = Instant::now();
        self.active = true;
    }

    fn stop(&mut self) {
        self.velocity = (0.0, 0.0);
        self.active = false;
    }

    fn update(&mut self, friction: f32, min_velocity: f32) -> Option<(f32, f32)> {
        if !self.active {
            return None;
        }

        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f32();
        self.last_update = now;

        // Apply friction based on time delta
        let decay = friction.powf(dt * 60.0); // Normalize to 60fps
        self.velocity.0 *= decay;
        self.velocity.1 *= decay;

        // Calculate displacement
        let dx = self.velocity.0 * dt;
        let dy = self.velocity.1 * dt;

        // Check if velocity is below threshold
        let speed = (self.velocity.0.powi(2) + self.velocity.1.powi(2)).sqrt();
        if speed < min_velocity {
            self.stop();
            return None;
        }

        Some((dx, dy))
    }
}

/// Viewport state for scrolling and visible region management.
///
/// The viewport tracks the visible portion of the editor content and
/// handles all scrolling operations including smooth momentum scrolling.
#[derive(Debug, Clone)]
pub struct Viewport {
    /// Horizontal scroll offset in pixels.
    pub scroll_x: f32,
    /// Vertical scroll offset in pixels.
    pub scroll_y: f32,
    /// Viewport width in pixels.
    pub width: f32,
    /// Viewport height in pixels.
    pub height: f32,
    /// Line height in pixels.
    line_height: f32,
    /// Character width in pixels (for monospace fonts).
    char_width: f32,
    /// Total content height in pixels.
    content_height: f32,
    /// Total content width in pixels.
    content_width: f32,
    /// Total number of lines in the document.
    total_lines: usize,
    /// Scroll configuration.
    config: ScrollConfig,
    /// Momentum state for smooth scrolling.
    momentum: MomentumState,
    /// Previous scroll position for change detection.
    prev_scroll_x: f32,
    prev_scroll_y: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self::new(800.0, 600.0)
    }
}

impl Viewport {
    /// Creates a new viewport with the specified dimensions.
    #[must_use]
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            scroll_x: 0.0,
            scroll_y: 0.0,
            width,
            height,
            line_height: 20.0,
            char_width: 8.0,
            content_height: 0.0,
            content_width: 0.0,
            total_lines: 1,
            config: ScrollConfig::default(),
            momentum: MomentumState::default(),
            prev_scroll_x: 0.0,
            prev_scroll_y: 0.0,
        }
    }

    /// Creates a viewport with custom scroll configuration.
    #[must_use]
    pub fn with_config(width: f32, height: f32, config: ScrollConfig) -> Self {
        let mut viewport = Self::new(width, height);
        viewport.config = config;
        viewport
    }

    /// Returns the scroll configuration.
    #[must_use]
    pub fn config(&self) -> &ScrollConfig {
        &self.config
    }

    /// Returns a mutable reference to the scroll configuration.
    pub fn config_mut(&mut self) -> &mut ScrollConfig {
        &mut self.config
    }

    /// Sets font metrics for scroll calculations.
    pub fn set_font_metrics(&mut self, char_width: f32, line_height: f32) {
        self.char_width = char_width;
        self.line_height = line_height;
        self.config.pixels_per_line = line_height;
        self.update_content_size();
    }

    /// Sets the total number of lines in the document.
    pub fn set_total_lines(&mut self, lines: usize) {
        self.total_lines = lines.max(1);
        self.update_content_size();
    }

    /// Sets the maximum content width (longest line width).
    pub fn set_content_width(&mut self, width: f32) {
        self.content_width = width;
    }

    /// Updates content size based on total lines.
    fn update_content_size(&mut self) {
        self.content_height = self.total_lines as f32 * self.line_height;
    }

    /// Resizes the viewport.
    pub fn resize(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
        // Clamp scroll position to valid range after resize
        self.clamp_scroll();
    }

    /// Returns the line height.
    #[must_use]
    pub fn line_height(&self) -> f32 {
        self.line_height
    }

    /// Returns the character width.
    #[must_use]
    pub fn char_width(&self) -> f32 {
        self.char_width
    }

    /// Returns the total number of lines.
    #[must_use]
    pub fn total_lines(&self) -> usize {
        self.total_lines
    }

    // =========================================================================
    // Scroll Position Methods
    // =========================================================================

    /// Scrolls to a specific line, centering it in the viewport if possible.
    pub fn scroll_to_line(&mut self, line: usize) {
        self.stop_momentum();
        let target_y = line as f32 * self.line_height;
        // Center the line in the viewport
        let centered_y = target_y - (self.height / 2.0) + (self.line_height / 2.0);
        self.scroll_y = centered_y;
        self.clamp_scroll();
    }

    /// Scrolls to a specific position, ensuring it's visible.
    pub fn scroll_to_position(&mut self, position: Position) {
        self.stop_momentum();

        // Vertical: center the line
        let target_y = position.line as f32 * self.line_height;
        let centered_y = target_y - (self.height / 2.0) + (self.line_height / 2.0);
        self.scroll_y = centered_y;

        // Horizontal: center the column
        let target_x = position.column as f32 * self.char_width;
        let centered_x = target_x - (self.width / 2.0) + self.char_width;
        self.scroll_x = centered_x;

        self.clamp_scroll();
    }

    /// Ensures the cursor is visible by scrolling if necessary.
    ///
    /// Uses configured margins to provide context around the cursor.
    pub fn ensure_cursor_visible(&mut self, cursor: Position) {
        self.ensure_cursor_visible_with_padding(
            cursor,
            (self.config.horizontal_margin, self.config.vertical_margin_lines as f32 * self.line_height),
        );
    }

    /// Ensures the cursor is visible with custom padding.
    pub fn ensure_cursor_visible_with_padding(
        &mut self,
        cursor: Position,
        padding: (f32, f32), // (horizontal, vertical) padding in pixels
    ) {
        let cursor_x = cursor.column as f32 * self.char_width;
        let cursor_y = cursor.line as f32 * self.line_height;

        // Horizontal scrolling
        if cursor_x < self.scroll_x + padding.0 {
            self.scroll_x = (cursor_x - padding.0).max(0.0);
        } else if cursor_x > self.scroll_x + self.width - padding.0 - self.char_width {
            self.scroll_x = cursor_x - self.width + padding.0 + self.char_width;
        }

        // Vertical scrolling
        if cursor_y < self.scroll_y + padding.1 {
            self.scroll_y = (cursor_y - padding.1).max(0.0);
        } else if cursor_y + self.line_height > self.scroll_y + self.height - padding.1 {
            self.scroll_y = cursor_y + self.line_height - self.height + padding.1;
        }

        self.clamp_scroll();
    }

    /// Scrolls by the given pixel delta.
    pub fn scroll_by(&mut self, delta_x: f32, delta_y: f32) {
        self.scroll_x += delta_x;
        self.scroll_y += delta_y;
        self.clamp_scroll();
    }

    /// Scrolls by the given number of lines.
    pub fn scroll_by_lines(&mut self, lines: i32) {
        let delta = lines as f32 * self.line_height;
        self.scroll_by(0.0, delta);
    }

    /// Scrolls by one page up.
    pub fn scroll_page_up(&mut self) {
        let page_height = self.visible_lines() as f32 * self.line_height;
        self.scroll_by(0.0, -page_height);
    }

    /// Scrolls by one page down.
    pub fn scroll_page_down(&mut self) {
        let page_height = self.visible_lines() as f32 * self.line_height;
        self.scroll_by(0.0, page_height);
    }

    /// Clamps scroll position to valid range.
    fn clamp_scroll(&mut self) {
        // Clamp horizontal
        let max_scroll_x = (self.content_width - self.width).max(0.0);
        self.scroll_x = self.scroll_x.clamp(0.0, max_scroll_x);

        // Clamp vertical
        let max_scroll_y = (self.content_height - self.height).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll_y);
    }

    // =========================================================================
    // Mouse Wheel & Trackpad Scrolling
    // =========================================================================

    /// Handles a mouse wheel scroll event.
    ///
    /// Returns true if the scroll was handled and the viewport changed.
    pub fn handle_wheel_scroll(&mut self, delta_x: f32, delta_y: f32) -> bool {
        self.stop_momentum();

        let prev_x = self.scroll_x;
        let prev_y = self.scroll_y;

        // For discrete wheel events, multiply by lines_per_scroll
        let scroll_amount = self.config.lines_per_scroll as f32 * self.line_height;
        let dx = if delta_x.abs() > 0.0 {
            delta_x.signum() * scroll_amount
        } else {
            0.0
        };
        let dy = if delta_y.abs() > 0.0 {
            delta_y.signum() * scroll_amount
        } else {
            0.0
        };

        self.scroll_by(dx, dy);

        self.scroll_x != prev_x || self.scroll_y != prev_y
    }

    /// Handles a trackpad scroll event with momentum.
    ///
    /// `delta_x` and `delta_y` are pixel deltas from the trackpad.
    /// `is_momentum` indicates if this is a momentum (inertial) event from the system.
    pub fn handle_trackpad_scroll(
        &mut self,
        delta_x: f32,
        delta_y: f32,
        is_momentum: bool,
    ) -> bool {
        let prev_x = self.scroll_x;
        let prev_y = self.scroll_y;

        if is_momentum {
            // System-provided momentum, just apply directly
            self.scroll_by(delta_x, delta_y);
        } else {
            // User gesture, apply and potentially start our own momentum
            self.scroll_by(delta_x, delta_y);

            // Track velocity for custom momentum (if system doesn't provide it)
            if self.config.smooth_scrolling {
                // Velocity is pixels per second, estimate from frame delta
                // Assuming ~60fps, multiply by 60 to get per-second rate
                let vx = delta_x * 60.0;
                let vy = delta_y * 60.0;
                self.momentum.start(vx, vy);
            }
        }

        self.scroll_x != prev_x || self.scroll_y != prev_y
    }

    /// Starts momentum scrolling with the given velocity.
    pub fn start_momentum(&mut self, velocity_x: f32, velocity_y: f32) {
        if self.config.smooth_scrolling {
            self.momentum.start(velocity_x, velocity_y);
        }
    }

    /// Stops momentum scrolling immediately.
    pub fn stop_momentum(&mut self) {
        self.momentum.stop();
    }

    /// Updates momentum scrolling state.
    ///
    /// Call this every frame to apply momentum. Returns true if still animating.
    pub fn update_momentum(&mut self) -> bool {
        if let Some((dx, dy)) = self.momentum.update(
            self.config.momentum_friction,
            self.config.momentum_min_velocity,
        ) {
            self.scroll_by(dx, dy);
            true
        } else {
            false
        }
    }

    /// Returns whether momentum scrolling is currently active.
    #[must_use]
    pub fn is_momentum_active(&self) -> bool {
        self.momentum.active
    }

    // =========================================================================
    // Visible Region Queries
    // =========================================================================

    /// Returns the first visible line index.
    #[must_use]
    pub fn first_visible_line(&self) -> usize {
        (self.scroll_y / self.line_height).floor() as usize
    }

    /// Returns the last visible line index.
    #[must_use]
    pub fn last_visible_line(&self) -> usize {
        // Calculate the y-coordinate of the bottom of the viewport
        let bottom_y = self.scroll_y + self.height;
        // Find which line contains a pixel just inside the viewport
        // Use a small epsilon to handle exact boundary cases
        let last = ((bottom_y - 0.001) / self.line_height).floor() as usize;
        last.min(self.total_lines.saturating_sub(1))
    }

    /// Returns the number of lines visible in the viewport.
    #[must_use]
    pub fn visible_lines(&self) -> usize {
        (self.height / self.line_height).ceil() as usize
    }

    /// Returns the range of visible lines (inclusive).
    #[must_use]
    pub fn visible_line_range(&self) -> (usize, usize) {
        (self.first_visible_line(), self.last_visible_line())
    }

    /// Returns true if the given line is visible in the viewport.
    #[must_use]
    pub fn is_line_visible(&self, line: usize) -> bool {
        let first = self.first_visible_line();
        let last = self.last_visible_line();
        line >= first && line <= last
    }

    /// Returns the Y coordinate for a line (in viewport space).
    #[must_use]
    pub fn line_y(&self, line: usize) -> f32 {
        (line as f32 * self.line_height) - self.scroll_y
    }

    /// Returns the line index at the given Y coordinate (in viewport space).
    #[must_use]
    pub fn line_at_y(&self, y: f32) -> usize {
        let absolute_y = y + self.scroll_y;
        if absolute_y < 0.0 {
            0
        } else {
            (absolute_y / self.line_height).floor() as usize
        }
    }

    // =========================================================================
    // Scroll Change Detection
    // =========================================================================

    /// Checks if the scroll position changed since the last call to this method.
    ///
    /// This is useful for emitting scroll events only when the position actually changes.
    pub fn take_scroll_changed(&mut self) -> bool {
        let changed = self.scroll_x != self.prev_scroll_x || self.scroll_y != self.prev_scroll_y;
        self.prev_scroll_x = self.scroll_x;
        self.prev_scroll_y = self.scroll_y;
        changed
    }

    /// Returns the scroll position change since the last check.
    #[must_use]
    pub fn scroll_delta(&self) -> (f32, f32) {
        (self.scroll_x - self.prev_scroll_x, self.scroll_y - self.prev_scroll_y)
    }

    // =========================================================================
    // Scroll Progress
    // =========================================================================

    /// Returns the vertical scroll progress (0.0 to 1.0).
    #[must_use]
    pub fn vertical_scroll_progress(&self) -> f32 {
        let max_scroll = (self.content_height - self.height).max(0.0);
        if max_scroll == 0.0 {
            0.0
        } else {
            (self.scroll_y / max_scroll).clamp(0.0, 1.0)
        }
    }

    /// Returns the horizontal scroll progress (0.0 to 1.0).
    #[must_use]
    pub fn horizontal_scroll_progress(&self) -> f32 {
        let max_scroll = (self.content_width - self.width).max(0.0);
        if max_scroll == 0.0 {
            0.0
        } else {
            (self.scroll_x / max_scroll).clamp(0.0, 1.0)
        }
    }

    /// Returns whether vertical scrolling is possible.
    #[must_use]
    pub fn can_scroll_vertically(&self) -> bool {
        self.content_height > self.height
    }

    /// Returns whether horizontal scrolling is possible.
    #[must_use]
    pub fn can_scroll_horizontally(&self) -> bool {
        self.content_width > self.width
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_viewport_new() {
        let viewport = Viewport::new(800.0, 600.0);
        assert_eq!(viewport.width, 800.0);
        assert_eq!(viewport.height, 600.0);
        assert_eq!(viewport.scroll_x, 0.0);
        assert_eq!(viewport.scroll_y, 0.0);
    }

    #[test]
    fn test_viewport_scroll_by() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_total_lines(100);
        viewport.set_content_width(1000.0);

        viewport.scroll_by(50.0, 100.0);
        assert_eq!(viewport.scroll_x, 50.0);
        assert_eq!(viewport.scroll_y, 100.0);

        // Negative scroll should clamp to 0
        viewport.scroll_by(-100.0, -200.0);
        assert_eq!(viewport.scroll_x, 0.0);
        assert_eq!(viewport.scroll_y, 0.0);
    }

    #[test]
    fn test_viewport_scroll_clamp() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(10.0, 20.0);
        viewport.set_total_lines(50); // 50 lines * 20px = 1000px content height
        viewport.set_content_width(1000.0);

        // Try to scroll beyond content
        viewport.scroll_by(0.0, 2000.0);

        // Should be clamped to max scroll (content_height - viewport_height)
        let max_scroll = (50.0 * 20.0) - 600.0;
        assert_eq!(viewport.scroll_y, max_scroll);
    }

    #[test]
    fn test_viewport_visible_lines() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(10.0, 20.0);
        viewport.set_total_lines(100);

        // 600px / 20px per line = 30 visible lines
        assert_eq!(viewport.visible_lines(), 30);

        // First and last visible (0-indexed, so lines 0-29 are visible)
        assert_eq!(viewport.first_visible_line(), 0);
        assert_eq!(viewport.last_visible_line(), 29);

        // Scroll down by 200px (10 lines)
        viewport.scroll_by(0.0, 200.0);
        assert_eq!(viewport.first_visible_line(), 10);
        assert_eq!(viewport.last_visible_line(), 39);
    }

    #[test]
    fn test_viewport_scroll_to_line() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(10.0, 20.0);
        viewport.set_total_lines(100);

        viewport.scroll_to_line(50);

        // Line 50 should be roughly centered
        let expected_y = 50.0 * 20.0 - 600.0 / 2.0 + 20.0 / 2.0;
        assert!((viewport.scroll_y - expected_y).abs() < 0.1);
    }

    #[test]
    fn test_viewport_ensure_cursor_visible() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(10.0, 20.0);
        viewport.set_total_lines(100);

        // Cursor at line 50, should scroll to make it visible
        viewport.ensure_cursor_visible(Position::new(50, 0));

        // Line 50 should now be visible
        assert!(viewport.is_line_visible(50));
    }

    #[test]
    fn test_viewport_scroll_by_lines() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(10.0, 20.0);
        viewport.set_total_lines(100);

        viewport.scroll_by_lines(5);
        assert_eq!(viewport.scroll_y, 100.0); // 5 lines * 20px

        viewport.scroll_by_lines(-3);
        assert_eq!(viewport.scroll_y, 40.0); // 2 lines * 20px
    }

    #[test]
    fn test_viewport_page_up_down() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(10.0, 20.0);
        viewport.set_total_lines(100);

        viewport.scroll_page_down();
        let page_height = 30.0 * 20.0; // 30 visible lines * 20px
        assert_eq!(viewport.scroll_y, page_height);

        viewport.scroll_page_up();
        assert_eq!(viewport.scroll_y, 0.0);
    }

    #[test]
    fn test_viewport_wheel_scroll() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(10.0, 20.0);
        viewport.set_total_lines(100);

        let changed = viewport.handle_wheel_scroll(0.0, 1.0);
        assert!(changed);

        // Default: 3 lines per scroll * 20px = 60px
        assert_eq!(viewport.scroll_y, 60.0);
    }

    #[test]
    fn test_viewport_is_line_visible() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(10.0, 20.0);
        viewport.set_total_lines(100);

        assert!(viewport.is_line_visible(0));
        assert!(viewport.is_line_visible(15));
        assert!(viewport.is_line_visible(29));
        assert!(!viewport.is_line_visible(50));

        viewport.scroll_to_line(50);
        assert!(viewport.is_line_visible(50));
    }

    #[test]
    fn test_viewport_scroll_progress() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(10.0, 20.0);
        viewport.set_total_lines(100); // 2000px total, 600px viewport

        assert_eq!(viewport.vertical_scroll_progress(), 0.0);

        let max_scroll = 2000.0 - 600.0;
        viewport.scroll_by(0.0, max_scroll / 2.0);
        assert!((viewport.vertical_scroll_progress() - 0.5).abs() < 0.01);

        viewport.scroll_by(0.0, max_scroll);
        assert!((viewport.vertical_scroll_progress() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_viewport_take_scroll_changed() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_total_lines(100);

        // Initial state, no change
        assert!(!viewport.take_scroll_changed());

        // After scrolling, should report changed
        viewport.scroll_by(0.0, 100.0);
        assert!(viewport.take_scroll_changed());

        // Second call should report no change
        assert!(!viewport.take_scroll_changed());
    }

    #[test]
    fn test_viewport_line_y() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(10.0, 20.0);
        viewport.set_total_lines(100); // Need content to scroll through

        assert_eq!(viewport.line_y(0), 0.0);
        assert_eq!(viewport.line_y(5), 100.0);

        viewport.scroll_by(0.0, 40.0);
        assert_eq!(viewport.line_y(0), -40.0);
        assert_eq!(viewport.line_y(5), 60.0);
    }

    #[test]
    fn test_viewport_line_at_y() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(10.0, 20.0);
        viewport.set_total_lines(100); // Need content to scroll through

        assert_eq!(viewport.line_at_y(0.0), 0);
        assert_eq!(viewport.line_at_y(50.0), 2);
        assert_eq!(viewport.line_at_y(100.0), 5);

        viewport.scroll_by(0.0, 40.0);
        assert_eq!(viewport.line_at_y(0.0), 2); // scroll_y=40, so y=0 means line 2
    }

    #[test]
    fn test_scroll_config_default() {
        let config = ScrollConfig::default();
        assert!(config.smooth_scrolling);
        assert_eq!(config.lines_per_scroll, 3);
    }

    #[test]
    fn test_viewport_resize() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(10.0, 20.0);
        viewport.set_total_lines(50);

        // Scroll down
        viewport.scroll_by(0.0, 500.0);

        // Resize larger
        viewport.resize(800.0, 900.0);

        // Scroll should be clamped to new max (50*20 - 900 = 100)
        assert!(viewport.scroll_y <= 100.0);
    }
}
