//! Main editor view integrating all rendering components.
//!
//! The EditorView combines text rendering, cursor, highlights, and the render
//! loop into a complete visual representation of the editor.

use crate::editor::{navigation, EditorController, Position};
use crate::render::{RenderError, TextRenderer};

use super::cursor_renderer::{CursorConfig, CursorRenderer};
use super::frame_timer::{FrameStats, FrameTimer, TargetFrameRate};
use super::highlight::{HighlightConfig, HighlightRect, HighlightRenderer};

/// Configuration for the editor view.
#[derive(Debug, Clone)]
pub struct ViewConfig {
    /// Target frame rate.
    pub frame_rate: TargetFrameRate,
    /// Cursor configuration.
    pub cursor: CursorConfig,
    /// Highlight configuration.
    pub highlight: HighlightConfig,
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

/// Viewport state for scrolling.
#[derive(Debug, Clone, Default)]
pub struct Viewport {
    /// Scroll offset X.
    pub scroll_x: f32,
    /// Scroll offset Y.
    pub scroll_y: f32,
    /// Viewport width.
    pub width: f32,
    /// Viewport height.
    pub height: f32,
}

impl Viewport {
    /// Creates a new viewport.
    #[must_use]
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            scroll_x: 0.0,
            scroll_y: 0.0,
            width,
            height,
        }
    }

    /// Ensures the cursor is visible by scrolling if necessary.
    pub fn ensure_cursor_visible(
        &mut self,
        cursor: Position,
        char_width: f32,
        line_height: f32,
        padding: (f32, f32), // (horizontal, vertical) padding for visibility
    ) {
        let cursor_x = cursor.column as f32 * char_width;
        let cursor_y = cursor.line as f32 * line_height;

        // Horizontal scrolling
        if cursor_x < self.scroll_x + padding.0 {
            self.scroll_x = (cursor_x - padding.0).max(0.0);
        } else if cursor_x > self.scroll_x + self.width - padding.0 {
            self.scroll_x = cursor_x - self.width + padding.0;
        }

        // Vertical scrolling
        if cursor_y < self.scroll_y + padding.1 {
            self.scroll_y = (cursor_y - padding.1).max(0.0);
        } else if cursor_y + line_height > self.scroll_y + self.height - padding.1 {
            self.scroll_y = cursor_y + line_height - self.height + padding.1;
        }
    }

    /// Scrolls by the given delta.
    pub fn scroll_by(&mut self, delta_x: f32, delta_y: f32) {
        self.scroll_x = (self.scroll_x + delta_x).max(0.0);
        self.scroll_y = (self.scroll_y + delta_y).max(0.0);
    }

    /// Returns the first visible line.
    #[must_use]
    pub fn first_visible_line(&self, line_height: f32) -> usize {
        (self.scroll_y / line_height).floor() as usize
    }

    /// Returns the last visible line.
    #[must_use]
    pub fn last_visible_line(&self, line_height: f32) -> usize {
        ((self.scroll_y + self.height) / line_height).ceil() as usize
    }

    /// Returns the number of lines visible in the viewport.
    #[must_use]
    pub fn visible_lines(&self, line_height: f32) -> usize {
        (self.height / line_height).ceil() as usize
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
    /// Frame timer.
    frame_timer: FrameTimer,
    /// Viewport state.
    viewport: Viewport,
    /// View configuration.
    config: ViewConfig,
    /// Font metrics.
    char_width: f32,
    line_height: f32,
    /// Cached line lengths for highlight rendering.
    line_lengths_cache: Vec<usize>,
    /// Whether the cache needs updating.
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
        let frame_timer = FrameTimer::new(config.frame_rate);

        Self {
            controller: EditorController::new(),
            cursor_renderer,
            highlight_renderer,
            frame_timer,
            viewport: Viewport::new(width, height),
            config,
            char_width: 8.0,
            line_height: 20.0,
            line_lengths_cache: Vec::new(),
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
        self.controller_mut()
            .mouse_handler_mut()
            .set_font_metrics(char_width, line_height);
    }

    /// Resizes the view.
    pub fn resize(&mut self, width: f32, height: f32) {
        self.viewport.width = width;
        self.viewport.height = height;
        self.highlight_renderer.set_viewport(width, height);
        let page_lines = (height / self.line_height) as usize;
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

        // Ensure cursor is visible
        self.viewport.ensure_cursor_visible(
            cursor_pos,
            self.char_width,
            self.line_height,
            (self.char_width * 2.0, self.line_height),
        );

        // Update mouse handler scroll
        self.controller
            .mouse_handler_mut()
            .set_scroll(self.viewport.scroll_x, self.viewport.scroll_y);
        self.controller
            .mouse_handler_mut()
            .set_padding(self.config.padding_left, self.config.padding_top);

        // Update line lengths cache if dirty
        if self.cache_dirty {
            self.update_line_lengths_cache();
            self.cache_dirty = false;
        }
    }

    /// Updates the line lengths cache.
    fn update_line_lengths_cache(&mut self) {
        let buffer = self.controller.editor().buffer();
        self.line_lengths_cache.clear();

        for line_idx in 0..buffer.len_lines() {
            let len = navigation::line_length(buffer, line_idx);
            self.line_lengths_cache.push(len);
        }
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

        let mut rects = self.highlight_renderer.generate_highlights(
            cursor_pos,
            selection,
            self.viewport.scroll_x,
            self.viewport.scroll_y,
            &self.line_lengths_cache,
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
    #[must_use]
    pub fn visible_text(&self) -> String {
        // For now, return all content. In a real implementation, this would
        // return only visible lines for performance.
        self.controller.editor().content()
    }

    /// Returns the range of visible lines.
    #[must_use]
    pub fn visible_line_range(&self) -> (usize, usize) {
        let first = self.viewport.first_visible_line(self.line_height);
        let last = self
            .viewport
            .last_visible_line(self.line_height)
            .min(self.controller.editor().line_count().saturating_sub(1));
        (first, last)
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

        // Calculate position values before creating the text area
        let left = self.config.padding_left - self.viewport.scroll_x;
        let top = self.config.padding_top - self.viewport.scroll_y;
        let color = self.text_glyphon_color();

        text_renderer.prepare_at(device, queue, left, top, color)?;
        Ok(())
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
        let viewport = Viewport::new(800.0, 600.0);
        assert_eq!(viewport.visible_lines(20.0), 30);
        assert_eq!(viewport.first_visible_line(20.0), 0);
        assert_eq!(viewport.last_visible_line(20.0), 30);
    }

    #[test]
    fn test_viewport_ensure_cursor_visible() {
        let mut viewport = Viewport::new(800.0, 600.0);

        // Cursor at (0, 0) - should stay at scroll (0, 0)
        viewport.ensure_cursor_visible(Position::new(0, 0), 10.0, 20.0, (20.0, 20.0));
        assert_eq!(viewport.scroll_y, 0.0);

        // Cursor at line 50 - should scroll down
        viewport.ensure_cursor_visible(Position::new(50, 0), 10.0, 20.0, (20.0, 20.0));
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
