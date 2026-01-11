//! Minimap renderer implementation.
//!
//! Contains the main `MinimapRenderer` struct responsible for rendering
//! the minimap and handling user interactions.

use crate::document::Document;
use crate::render::Viewport;
use crate::theme::{Color, SyntaxColors, Theme};

use super::types::{
    MinimapConfig, MinimapDimensions, MinimapDragState, MinimapLine, MinimapRect, MinimapSegment,
    ViewportIndicator,
};

/// Minimap renderer for document overview.
///
/// The minimap provides a scaled-down view of the entire document,
/// allowing users to see the overall structure and quickly navigate.
///
/// # Example
///
/// ```ignore
/// use iridium_editor::render::{MinimapRenderer, MinimapConfig};
///
/// let mut renderer = MinimapRenderer::new(MinimapConfig::default());
///
/// // Calculate dimensions
/// let dimensions = renderer.calculate_dimensions(&viewport, &document, 20.0);
///
/// // Handle a click
/// if dimensions.contains(mouse_x, mouse_y) {
///     let line = renderer.handle_click(mouse_x, mouse_y, &dimensions);
/// }
/// ```
#[derive(Debug)]
pub struct MinimapRenderer {
    /// Configuration for the minimap
    config: MinimapConfig,
    /// Current drag state
    drag_state: MinimapDragState,
    /// Cached line data for rendering
    cached_lines: Vec<MinimapLine>,
    /// First line of cached data
    cache_first_line: usize,
    /// Whether cache is valid
    cache_valid: bool,
}

impl MinimapRenderer {
    /// Creates a new minimap renderer with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(MinimapConfig::default())
    }

    /// Creates a new minimap renderer with custom configuration.
    #[must_use]
    pub const fn with_config(config: MinimapConfig) -> Self {
        Self {
            config,
            drag_state: MinimapDragState::new(),
            cached_lines: Vec::new(),
            cache_first_line: 0,
            cache_valid: false,
        }
    }

    /// Returns the current configuration.
    #[must_use]
    pub const fn config(&self) -> &MinimapConfig {
        &self.config
    }

    /// Updates the configuration.
    pub fn set_config(&mut self, config: MinimapConfig) {
        self.config = config;
        self.invalidate_cache();
    }

    /// Returns whether the minimap is enabled.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Enables or disables the minimap.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.config.enabled = enabled;
    }

    /// Invalidates the line cache.
    pub fn invalidate_cache(&mut self) {
        self.cache_valid = false;
        self.cached_lines.clear();
    }

    /// Calculates minimap dimensions for the current state.
    ///
    /// # Arguments
    ///
    /// * `viewport` - The editor viewport
    /// * `document` - The document being displayed
    /// * `editor_line_height` - Line height in the main editor
    #[must_use]
    pub fn calculate_dimensions(
        &self,
        viewport: &Viewport,
        document: &Document,
        editor_line_height: f32,
    ) -> MinimapDimensions {
        MinimapDimensions::calculate(&self.config, viewport, document, editor_line_height)
    }

    /// Calculates the viewport indicator for the current state.
    ///
    /// # Arguments
    ///
    /// * `dimensions` - Minimap dimensions
    /// * `viewport` - The editor viewport
    /// * `theme` - Current theme for colors
    #[must_use]
    pub fn calculate_viewport_indicator(
        &self,
        dimensions: &MinimapDimensions,
        viewport: &Viewport,
        theme: &Theme,
    ) -> ViewportIndicator {
        // Use selection color as base for viewport indicator
        let base_color = theme.editor.selection;
        ViewportIndicator::calculate(
            dimensions,
            viewport,
            self.config.viewport_indicator_opacity,
            base_color,
        )
    }

    /// Prepares line data for rendering.
    ///
    /// This method extracts line content and optionally applies syntax coloring.
    /// Results are cached for performance.
    ///
    /// # Arguments
    ///
    /// * `document` - The document to render
    /// * `dimensions` - Current minimap dimensions
    /// * `theme` - Current theme for colors
    /// * `syntax_spans` - Optional syntax highlighting spans per line
    #[allow(clippy::type_complexity)]
    pub fn prepare_lines(
        &mut self,
        document: &Document,
        dimensions: &MinimapDimensions,
        theme: &Theme,
        syntax_spans: Option<&[Vec<(usize, usize, Color)>]>,
    ) {
        // Check if cache is still valid
        if self.cache_valid && self.cache_first_line == dimensions.first_visible_line {
            return;
        }

        self.cached_lines.clear();
        self.cache_first_line = dimensions.first_visible_line;

        let default_color = theme.editor.foreground;
        let end_line = (dimensions.first_visible_line + dimensions.visible_line_count)
            .min(dimensions.total_lines);

        for line_num in dimensions.first_visible_line..end_line {
            let line = self.prepare_line(
                document,
                line_num,
                default_color,
                syntax_spans,
                &theme.syntax,
            );
            self.cached_lines.push(line);
        }

        self.cache_valid = true;
    }

    /// Prepares a single line for rendering.
    #[allow(clippy::type_complexity)]
    fn prepare_line(
        &self,
        document: &Document,
        line_number: usize,
        default_color: Color,
        syntax_spans: Option<&[Vec<(usize, usize, Color)>]>,
        _syntax_colors: &SyntaxColors,
    ) -> MinimapLine {
        let text = document.line(line_number).unwrap_or_default();

        if text.is_empty() {
            return MinimapLine::new(line_number, Vec::new());
        }

        // Apply syntax coloring if available and enabled
        if self.config.show_syntax_colors {
            if let Some(spans) = syntax_spans {
                if let Some(line_spans) = spans.get(line_number) {
                    let segments: Vec<MinimapSegment> = line_spans
                        .iter()
                        .filter(|(start, end, _)| *end > *start)
                        .map(|(start, end, color)| {
                            let start = (*start).min(self.config.max_chars_per_line);
                            let end = (*end).min(self.config.max_chars_per_line);
                            MinimapSegment::new(start, end, *color)
                        })
                        .collect();

                    if !segments.is_empty() {
                        return MinimapLine::new(line_number, segments);
                    }
                }
            }
        }

        // Fall back to default coloring
        MinimapLine::from_text(line_number, &text, default_color)
    }

    /// Returns the cached lines for rendering.
    #[must_use]
    pub fn lines(&self) -> &[MinimapLine] {
        &self.cached_lines
    }

    /// Generates rectangles for rendering the minimap content.
    ///
    /// This converts the cached line data into colored rectangles
    /// suitable for GPU rendering.
    ///
    /// # Arguments
    ///
    /// * `dimensions` - Minimap dimensions
    #[must_use]
    #[allow(clippy::cast_precision_loss, clippy::suboptimal_flops)]
    pub fn generate_rects(&self, dimensions: &MinimapDimensions) -> Vec<(MinimapRect, Color)> {
        let mut rects = Vec::with_capacity(self.cached_lines.len() * 2);

        for (idx, line) in self.cached_lines.iter().enumerate() {
            let y = idx as f32 * dimensions.line_height;

            for segment in &line.segments {
                let x = dimensions.x + (segment.start as f32) * dimensions.char_width;
                let width = ((segment.end - segment.start) as f32) * dimensions.char_width;

                // Clamp to minimap bounds
                let x = x.max(dimensions.x);
                let max_x = dimensions.x + dimensions.width;
                let width = width.min(max_x - x);

                if width > 0.0 {
                    rects.push((
                        MinimapRect::new(x, y, width, dimensions.line_height),
                        segment.color,
                    ));
                }
            }
        }

        rects
    }

    /// Handles a mouse click on the minimap.
    ///
    /// Returns the line number that was clicked, which can be used
    /// to scroll the main viewport.
    ///
    /// # Arguments
    ///
    /// * `x` - X coordinate relative to editor
    /// * `y` - Y coordinate relative to editor
    /// * `dimensions` - Current minimap dimensions
    /// * `viewport` - Current viewport for calculating scroll target
    #[must_use]
    pub fn handle_click(
        &mut self,
        x: f32,
        y: f32,
        dimensions: &MinimapDimensions,
        viewport: &Viewport,
    ) -> Option<usize> {
        if !dimensions.contains(x, y) {
            return None;
        }

        let clicked_line = dimensions.y_to_line(y - dimensions.y);

        // Start drag for potential drag-to-scroll
        self.drag_state
            .start(clicked_line, viewport.first_line, viewport.visible_lines);

        // Calculate target line to center the viewport
        let target = self.calculate_scroll_target(clicked_line, viewport, dimensions.total_lines);

        Some(target)
    }

    /// Handles mouse drag on the minimap.
    ///
    /// Returns the target line for scrolling during drag.
    ///
    /// # Arguments
    ///
    /// * `x` - X coordinate relative to editor
    /// * `y` - Y coordinate relative to editor
    /// * `dimensions` - Current minimap dimensions
    /// * `viewport` - Current viewport
    #[must_use]
    pub fn handle_drag(
        &mut self,
        x: f32,
        y: f32,
        dimensions: &MinimapDimensions,
        viewport: &Viewport,
    ) -> Option<usize> {
        if !self.drag_state.is_dragging {
            return None;
        }

        // Allow dragging slightly outside minimap bounds for smoother interaction
        let y = y.clamp(dimensions.y, dimensions.y + dimensions.height);
        let current_line = dimensions.y_to_line(y - dimensions.y);

        // Horizontal bounds check - end drag if too far outside
        if x < dimensions.x - 50.0 || x > dimensions.x + dimensions.width + 50.0 {
            self.drag_state.end();
            return None;
        }

        let target = self.drag_state.update(
            current_line,
            viewport.visible_lines,
            dimensions.total_lines,
        );

        Some(target)
    }

    /// Handles mouse release to end drag.
    pub fn handle_release(&mut self) {
        self.drag_state.end();
    }

    /// Returns whether a drag is in progress.
    #[must_use]
    pub const fn is_dragging(&self) -> bool {
        self.drag_state.is_dragging
    }

    /// Calculates the scroll target to center a line in the viewport.
    fn calculate_scroll_target(
        &self,
        clicked_line: usize,
        viewport: &Viewport,
        total_lines: usize,
    ) -> usize {
        // Calculate target to center the clicked line
        let half_visible = viewport.visible_lines / 2;

        if clicked_line < half_visible {
            0
        } else if clicked_line + half_visible >= total_lines {
            total_lines.saturating_sub(viewport.visible_lines)
        } else {
            clicked_line.saturating_sub(half_visible)
        }
    }

    /// Returns the minimap width for layout calculations.
    ///
    /// Returns 0.0 if minimap is disabled.
    #[must_use]
    pub fn width(&self) -> f32 {
        if self.config.enabled {
            self.config.width
        } else {
            0.0
        }
    }
}

impl Default for MinimapRenderer {
    fn default() -> Self {
        Self::new()
    }
}
