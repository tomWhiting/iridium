//! Main editor view integrating all rendering components.
//!
//! The EditorView combines text rendering, cursor, highlights, and the render
//! loop into a complete visual representation of the editor.

use crate::editor::EditorController;
use crate::render::{GutterConfig, GutterRenderer, RenderError, TextRenderer, Viewport};

use super::cursor_renderer::{CursorConfig, CursorRenderer};
use super::frame_timer::{FrameStats, FrameTimer, TargetFrameRate};
use super::highlight::{HighlightConfig, HighlightRect, HighlightRenderer};
use super::line_cache::LineCache;

/// Configuration for the editor view.
#[derive(Debug, Clone)]
pub struct ViewConfig {
    /// Target frame rate.
    pub frame_rate: TargetFrameRate,
    /// Cursor configuration.
    pub cursor: CursorConfig,
    /// Highlight configuration.
    pub highlight: HighlightConfig,
    /// Gutter configuration.
    pub gutter: GutterConfig,
    /// Left padding in pixels.
    pub padding_left: f32,
    /// Top padding in pixels.
    pub padding_top: f32,
    /// Right padding in pixels.
    pub padding_right: f32,
    /// Bottom padding in pixels.
    pub padding_bottom: f32,
    /// Background color (RGBA, 0.0-1.0).
    pub background_color: [f32; 4],
    /// Text color (RGBA, 0.0-1.0).
    pub text_color: [f32; 4],
}

impl Default for ViewConfig {
    fn default() -> Self {
        Self {
            frame_rate: TargetFrameRate::Fps120,
            cursor: CursorConfig::default(),
            highlight: HighlightConfig::default(),
            gutter: GutterConfig::default(),
            padding_left: 4.0,
            padding_top: 4.0,
            padding_right: 4.0,
            padding_bottom: 4.0,
            background_color: [0.12, 0.12, 0.12, 1.0], // Dark gray
            text_color: [0.87, 0.87, 0.87, 1.0],       // Light gray
        }
    }
}

impl ViewConfig {
    /// Creates a dark theme configuration.
    #[must_use]
    pub fn dark_theme() -> Self {
        Self {
            highlight: HighlightConfig::dark_theme(),
            gutter: GutterConfig::dark_theme(),
            background_color: [0.12, 0.12, 0.12, 1.0],
            text_color: [0.87, 0.87, 0.87, 1.0],
            ..Default::default()
        }
    }

    /// Creates a light theme configuration.
    #[must_use]
    pub fn light_theme() -> Self {
        Self {
            highlight: HighlightConfig::light_theme(),
            gutter: GutterConfig::light_theme(),
            background_color: [0.98, 0.98, 0.98, 1.0],
            text_color: [0.13, 0.13, 0.13, 1.0],
            cursor: CursorConfig::default().with_color(0.0, 0.0, 0.0, 1.0),
            ..Default::default()
        }
    }

    /// Returns the total horizontal padding.
    #[must_use]
    pub fn horizontal_padding(&self) -> f32 {
        self.padding_left + self.padding_right
    }

    /// Returns the total vertical padding.
    #[must_use]
    pub fn vertical_padding(&self) -> f32 {
        self.padding_top + self.padding_bottom
    }
}

/// The complete editor view.
///
/// Integrates the editor controller with rendering components to provide
/// a full visual editing experience at 120fps.
pub struct EditorView {
    /// The editor controller.
    controller: EditorController,
    /// Cursor renderer.
    cursor_renderer: CursorRenderer,
    /// Highlight renderer.
    highlight_renderer: HighlightRenderer,
    /// Gutter renderer.
    gutter_renderer: GutterRenderer,
    /// Frame timer.
    frame_timer: FrameTimer,
    /// Viewport state.
    viewport: Viewport,
    /// View configuration.
    config: ViewConfig,
    /// Font metrics.
    char_width: f32,
    line_height: f32,
    /// Line length cache for large file optimization.
    line_cache: LineCache,
    /// Whether the content has changed requiring cache invalidation.
    cache_dirty: bool,
}

impl EditorView {
    /// Creates a new editor view.
    #[must_use]
    pub fn new(width: f32, height: f32) -> Self {
        Self::with_config(width, height, ViewConfig::default())
    }

    /// Creates a new editor view with configuration.
    #[must_use]
    pub fn with_config(width: f32, height: f32, config: ViewConfig) -> Self {
        let cursor_renderer = CursorRenderer::new(config.cursor.clone());
        let highlight_renderer = HighlightRenderer::new(config.highlight.clone());
        let gutter_renderer = GutterRenderer::new(config.gutter.clone());
        let frame_timer = FrameTimer::new(config.frame_rate);

        Self {
            controller: EditorController::new(),
            cursor_renderer,
            highlight_renderer,
            gutter_renderer,
            frame_timer,
            viewport: Viewport::new(width, height),
            config,
            char_width: 8.0,
            line_height: 20.0,
            line_cache: LineCache::new(),
            cache_dirty: true,
        }
    }

    /// Creates an editor view with initial content.
    #[must_use]
    pub fn with_content(width: f32, height: f32, content: &str) -> Self {
        let mut view = Self::new(width, height);
        view.controller = EditorController::with_content(content);
        view.cache_dirty = true;
        view
    }

    /// Returns a reference to the editor controller.
    #[must_use]
    pub fn controller(&self) -> &EditorController {
        &self.controller
    }

    /// Returns a mutable reference to the editor controller.
    pub fn controller_mut(&mut self) -> &mut EditorController {
        self.cache_dirty = true;
        &mut self.controller
    }

    /// Returns the view configuration.
    #[must_use]
    pub fn config(&self) -> &ViewConfig {
        &self.config
    }

    /// Returns the viewport.
    #[must_use]
    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    /// Returns a mutable reference to the viewport.
    pub fn viewport_mut(&mut self) -> &mut Viewport {
        &mut self.viewport
    }

    /// Returns the frame timer.
    #[must_use]
    pub fn frame_timer(&self) -> &FrameTimer {
        &self.frame_timer
    }

    /// Returns frame statistics.
    #[must_use]
    pub fn frame_stats(&self) -> FrameStats {
        self.frame_timer.stats()
    }

    /// Sets font metrics.
    pub fn set_font_metrics(&mut self, char_width: f32, line_height: f32) {
        self.char_width = char_width;
        self.line_height = line_height;
        self.cursor_renderer
            .set_font_metrics(char_width, line_height, line_height * 0.8);
        self.highlight_renderer
            .set_font_metrics(char_width, line_height);
        self.gutter_renderer
            .set_font_metrics(char_width, line_height);
        self.viewport.set_font_metrics(char_width, line_height);
        self.controller_mut()
            .mouse_handler_mut()
            .set_font_metrics(char_width, line_height);
    }

    /// Resizes the view.
    pub fn resize(&mut self, width: f32, height: f32) {
        self.viewport.resize(width, height);
        self.highlight_renderer.set_viewport(width, height);
        let page_lines = self.viewport.visible_lines();
        self.controller_mut().set_page_lines(page_lines);
    }

    /// Updates the view state.
    ///
    /// Call this every frame to update animations and ensure cursor visibility.
    pub fn update(&mut self) {
        // Update frame timer
        self.frame_timer.begin_frame();

        // Update cursor blink
        self.cursor_renderer.update();

        // Sync cursor position
        let cursor_pos = self.controller.editor().cursor_position();
        self.cursor_renderer.set_position(cursor_pos);

        // Update viewport with current line count
        let line_count = self.controller.editor().line_count();
        self.viewport.set_total_lines(line_count);
        self.gutter_renderer.set_total_lines(line_count);

        // Ensure cursor is visible
        self.viewport.ensure_cursor_visible(cursor_pos);

        // Update momentum scrolling if active
        self.viewport.update_momentum();

        // Update mouse handler scroll
        self.controller
            .mouse_handler_mut()
            .set_scroll(self.viewport.scroll_x, self.viewport.scroll_y);
        self.controller.mouse_handler_mut().set_padding(
            self.config.padding_left + self.gutter_renderer.total_width(),
            self.config.padding_top,
        );

        // Update line cache if dirty (invalidated) or visible range changed
        if self.cache_dirty {
            self.line_cache.invalidate();
            self.cache_dirty = false;
        }

        // Always update the visible range cache
        self.update_line_cache();
    }

    /// Updates the line cache for the visible line range.
    ///
    /// Uses windowed caching - only caches visible lines plus a buffer zone.
    /// This is efficient for large files (100k+ lines).
    fn update_line_cache(&mut self) {
        let (first, last) = self.viewport.visible_line_range();
        let buffer = self.controller.editor().buffer();
        self.line_cache.update_visible(buffer, first, last);
    }

    /// Returns whether the cursor should be drawn.
    #[must_use]
    pub fn should_draw_cursor(&self) -> bool {
        self.cursor_renderer.should_draw()
    }

    /// Returns the cursor rectangle.
    #[must_use]
    pub fn cursor_rect(&self) -> (f32, f32, f32, f32) {
        let (x, y, w, h) = self
            .cursor_renderer
            .cursor_rect(self.viewport.scroll_x, self.viewport.scroll_y);
        (
            x + self.config.padding_left,
            y + self.config.padding_top,
            w,
            h,
        )
    }

    /// Returns the cursor color.
    #[must_use]
    pub fn cursor_color(&self) -> [f32; 4] {
        self.cursor_renderer.color()
    }

    /// Generates highlight rectangles for the current frame.
    #[must_use]
    pub fn highlight_rects(&self) -> Vec<HighlightRect> {
        let cursor_pos = self.controller.editor().cursor_position();
        let selection = self.controller.editor().selection();

        // Get line lengths from cache for highlight rendering
        let line_lengths = self.line_cache.all_lengths();

        let mut rects = self.highlight_renderer.generate_highlights(
            cursor_pos,
            selection,
            self.viewport.scroll_x,
            self.viewport.scroll_y,
            &line_lengths,
        );

        // Adjust for padding
        for rect in &mut rects {
            rect.x += self.config.padding_left;
            rect.y += self.config.padding_top;
        }

        rects
    }

    /// Returns the background color as a wgpu::Color.
    #[must_use]
    pub fn background_wgpu_color(&self) -> wgpu::Color {
        wgpu::Color {
            r: self.config.background_color[0] as f64,
            g: self.config.background_color[1] as f64,
            b: self.config.background_color[2] as f64,
            a: self.config.background_color[3] as f64,
        }
    }

    /// Returns the text color as a glyphon Color.
    #[must_use]
    pub fn text_glyphon_color(&self) -> glyphon::Color {
        glyphon::Color::rgba(
            (self.config.text_color[0] * 255.0) as u8,
            (self.config.text_color[1] * 255.0) as u8,
            (self.config.text_color[2] * 255.0) as u8,
            (self.config.text_color[3] * 255.0) as u8,
        )
    }

    /// Returns the visible text content for rendering.
    ///
    /// This method implements viewport culling by returning only the lines
    /// that are currently visible in the viewport, improving performance
    /// for large files.
    #[must_use]
    pub fn visible_text(&self) -> String {
        let (first_line, last_line) = self.viewport.visible_line_range();
        let buffer = self.controller.editor().buffer();
        let total_lines = buffer.len_lines();

        // Clamp to valid line range
        let first = first_line.min(total_lines.saturating_sub(1));
        let last = last_line.min(total_lines.saturating_sub(1));

        // Build visible text from individual lines
        let mut result = String::new();
        for line_idx in first..=last {
            let line = buffer.line(line_idx);
            result.push_str(&line.to_string());
        }

        result
    }

    /// Returns the range of visible lines.
    #[must_use]
    pub fn visible_line_range(&self) -> (usize, usize) {
        self.viewport.visible_line_range()
    }

    /// Prepares the text renderer with current content.
    ///
    /// # Errors
    ///
    /// Returns an error if text preparation fails.
    pub fn prepare_text(
        &self,
        text_renderer: &mut TextRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(), RenderError> {
        let text = self.visible_text();
        text_renderer.set_text(&text);
        text_renderer.shape();

        // Calculate position values for viewport-culled text rendering
        // Account for gutter width and padding
        let gutter_width = self.gutter_renderer.total_width();
        let left = self.config.padding_left + gutter_width - self.viewport.scroll_x;

        // Since visible_text() returns only visible lines starting at first_visible_line,
        // we need to position the text where that first line would appear.
        // The top position is: padding + (first_line * line_height) - scroll_y
        // But since scroll_y ≈ first_line * line_height, this simplifies to:
        // padding + (scroll_y - floor(scroll_y / line_height) * line_height) - scroll_y
        // Which is: padding - (scroll_y mod line_height), giving subpixel scrolling
        let first_line = self.viewport.first_visible_line();
        let first_line_y = first_line as f32 * self.line_height;
        let top = self.config.padding_top + first_line_y - self.viewport.scroll_y;

        let color = self.text_glyphon_color();

        text_renderer.prepare_at(device, queue, left, top, color)?;
        Ok(())
    }

    /// Returns the gutter renderer.
    #[must_use]
    pub fn gutter_renderer(&self) -> &GutterRenderer {
        &self.gutter_renderer
    }

    /// Returns the total gutter width.
    #[must_use]
    pub fn gutter_width(&self) -> f32 {
        self.gutter_renderer.total_width()
    }

    /// Resets the cursor blink timer.
    pub fn reset_cursor_blink(&mut self) {
        self.cursor_renderer.reset_blink();
    }

    /// Marks the content as changed (invalidates caches).
    pub fn invalidate(&mut self) {
        self.cache_dirty = true;
        self.reset_cursor_blink();
    }

    /// Scrolls the view by the given pixel deltas.
    pub fn scroll(&mut self, delta_x: f32, delta_y: f32) {
        self.viewport.scroll_by(delta_x, delta_y);
    }
}

impl std::fmt::Debug for EditorView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditorView")
            .field("viewport", &self.viewport)
            .field("config", &self.config)
            .field("char_width", &self.char_width)
            .field("line_height", &self.line_height)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Position;

    #[test]
    fn test_view_config_default() {
        let config = ViewConfig::default();
        assert_eq!(config.frame_rate, TargetFrameRate::Fps120);
    }

    #[test]
    fn test_viewport_new() {
        let viewport = Viewport::new(800.0, 600.0);
        assert_eq!(viewport.width, 800.0);
        assert_eq!(viewport.height, 600.0);
        assert_eq!(viewport.scroll_x, 0.0);
        assert_eq!(viewport.scroll_y, 0.0);
    }

    #[test]
    fn test_viewport_scroll() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_total_lines(100);
        viewport.set_content_width(1000.0);
        viewport.scroll_by(100.0, 50.0);
        assert_eq!(viewport.scroll_x, 100.0);
        assert_eq!(viewport.scroll_y, 50.0);

        // Can't scroll negative
        viewport.scroll_by(-200.0, -100.0);
        assert_eq!(viewport.scroll_x, 0.0);
        assert_eq!(viewport.scroll_y, 0.0);
    }

    #[test]
    fn test_viewport_visible_lines() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(10.0, 20.0);
        viewport.set_total_lines(100);
        // 600px / 20px per line = 30 visible lines
        assert_eq!(viewport.visible_lines(), 30);
        assert_eq!(viewport.first_visible_line(), 0);
        // Lines 0-29 are visible (30 lines total)
        assert_eq!(viewport.last_visible_line(), 29);
    }

    #[test]
    fn test_viewport_ensure_cursor_visible() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(10.0, 20.0);
        viewport.set_total_lines(100);

        // Cursor at (0, 0) - should stay at scroll (0, 0)
        viewport.ensure_cursor_visible(Position::new(0, 0));
        assert_eq!(viewport.scroll_y, 0.0);

        // Cursor at line 50 - should scroll down
        viewport.ensure_cursor_visible(Position::new(50, 0));
        assert!(viewport.scroll_y > 0.0);
    }

    #[test]
    fn test_editor_view_new() {
        let view = EditorView::new(800.0, 600.0);
        assert_eq!(view.viewport().width, 800.0);
        assert_eq!(view.viewport().height, 600.0);
    }

    #[test]
    fn test_editor_view_with_content() {
        let view = EditorView::with_content(800.0, 600.0, "Hello, world!");
        assert_eq!(view.controller().editor().content(), "Hello, world!");
    }

    #[test]
    fn test_editor_view_resize() {
        let mut view = EditorView::new(800.0, 600.0);
        view.resize(1024.0, 768.0);
        assert_eq!(view.viewport().width, 1024.0);
        assert_eq!(view.viewport().height, 768.0);
    }

    #[test]
    fn test_editor_view_cursor_rect() {
        let mut view = EditorView::new(800.0, 600.0);
        view.set_font_metrics(10.0, 20.0);
        view.controller_mut()
            .editor_mut()
            .move_cursor_to(Position::new(2, 5));
        view.update();

        let (x, y, _w, h) = view.cursor_rect();
        // Position accounts for padding, but cursor_rect uses viewport's scroll position
        assert_eq!(x, 50.0 + view.config.padding_left); // 5 * 10 + padding
        assert_eq!(y, 40.0 + view.config.padding_top); // 2 * 20 + padding
        assert_eq!(h, 20.0);
    }

    #[test]
    fn test_editor_view_highlight_rects() {
        let mut view = EditorView::with_content(800.0, 600.0, "Hello\nWorld");
        view.set_font_metrics(10.0, 20.0);
        view.update();

        // No selection, should just have current line highlight
        let rects = view.highlight_rects();
        assert_eq!(rects.len(), 1); // Current line

        // Add selection
        view.controller_mut()
            .editor_mut()
            .move_cursor_to(Position::new(0, 0));
        view.controller_mut()
            .editor_mut()
            .extend_selection_to(Position::new(0, 5));
        view.update();

        let rects = view.highlight_rects();
        assert_eq!(rects.len(), 2); // Current line + selection
    }

    #[test]
    fn test_editor_view_visible_line_range() {
        let mut view = EditorView::with_content(
            800.0,
            200.0,
            "Line1\nLine2\nLine3\nLine4\nLine5\nLine6\nLine7\nLine8\nLine9\nLine10",
        );
        view.set_font_metrics(10.0, 20.0);
        view.update();

        let (first, last) = view.visible_line_range();
        assert_eq!(first, 0);
        assert!(last <= 10);
    }
}
