//! Minimap rendering for document overview.
//!
//! This module provides a scaled-down view of the entire document,
//! allowing users to quickly navigate through large files.
//!
//! # Features
//!
//! - Scaled document preview showing the entire file
//! - Viewport indicator showing the currently visible region
//! - Click-to-navigate for quick jumps
//! - Drag-to-scroll for smooth navigation
//! - Syntax coloring matching the main editor
//!
//! # Architecture
//!
//! The minimap renders lines as colored blocks rather than actual text,
//! providing a visual overview without the overhead of full text rendering.
//! Each line is represented as a series of colored segments based on
//! syntax highlighting.

use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::render::Viewport;
use crate::theme::{Color, SyntaxColors, Theme};

/// Default width of the minimap in pixels.
pub const DEFAULT_MINIMAP_WIDTH: f32 = 120.0;

/// Minimum width of the minimap.
pub const MIN_MINIMAP_WIDTH: f32 = 50.0;

/// Maximum width of the minimap.
pub const MAX_MINIMAP_WIDTH: f32 = 250.0;

/// Default character width in minimap (pixels per character).
pub const MINIMAP_CHAR_WIDTH: f32 = 1.5;

/// Default line height in minimap (pixels per line).
pub const MINIMAP_LINE_HEIGHT: f32 = 2.0;

/// Minimum line height in minimap.
pub const MIN_LINE_HEIGHT: f32 = 1.0;

/// Maximum line height in minimap.
pub const MAX_LINE_HEIGHT: f32 = 4.0;

/// Default number of characters to render per line.
pub const DEFAULT_CHARS_PER_LINE: usize = 80;

/// Configuration for minimap rendering.
#[derive(Debug, Clone)]
pub struct MinimapConfig {
    /// Width of the minimap in pixels
    pub width: f32,
    /// Height of each line in the minimap
    pub line_height: f32,
    /// Width of each character in the minimap
    pub char_width: f32,
    /// Maximum characters to render per line
    pub max_chars_per_line: usize,
    /// Whether the minimap is enabled
    pub enabled: bool,
    /// Position of the minimap (left or right side)
    pub position: MinimapPosition,
    /// Opacity of the viewport indicator (0.0-1.0)
    pub viewport_indicator_opacity: f32,
    /// Whether to show syntax coloring
    pub show_syntax_colors: bool,
}

impl Default for MinimapConfig {
    fn default() -> Self {
        Self {
            width: DEFAULT_MINIMAP_WIDTH,
            line_height: MINIMAP_LINE_HEIGHT,
            char_width: MINIMAP_CHAR_WIDTH,
            max_chars_per_line: DEFAULT_CHARS_PER_LINE,
            enabled: true,
            position: MinimapPosition::Right,
            viewport_indicator_opacity: 0.15,
            show_syntax_colors: true,
        }
    }
}

/// Position of the minimap relative to the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MinimapPosition {
    /// Minimap on the left side
    Left,
    /// Minimap on the right side (default)
    #[default]
    Right,
}

/// Computed dimensions for minimap rendering.
#[derive(Debug, Clone, Copy)]
pub struct MinimapDimensions {
    /// X position of the minimap
    pub x: f32,
    /// Y position of the minimap
    pub y: f32,
    /// Width of the minimap
    pub width: f32,
    /// Height of the minimap
    pub height: f32,
    /// Height of each line in pixels
    pub line_height: f32,
    /// Width of each character in pixels
    pub char_width: f32,
    /// Total document lines
    pub total_lines: usize,
    /// Scale factor (minimap line height / editor line height)
    pub scale: f32,
    /// First line to render (for very large files)
    pub first_visible_line: usize,
    /// Number of lines that fit in the minimap
    pub visible_line_count: usize,
}

impl MinimapDimensions {
    /// Calculates minimap dimensions based on config and viewport.
    ///
    /// # Arguments
    ///
    /// * `config` - Minimap configuration
    /// * `viewport` - The editor viewport
    /// * `document` - The document being displayed
    /// * `editor_line_height` - Line height in the main editor
    #[must_use]
    #[allow(clippy::cast_precision_loss, clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    pub fn calculate(
        config: &MinimapConfig,
        viewport: &Viewport,
        document: &Document,
        editor_line_height: f32,
    ) -> Self {
        let total_lines = document.line_count();

        // Calculate scale factor
        let scale = config.line_height / editor_line_height;

        // Calculate how many lines fit in the minimap height
        let visible_line_count = (viewport.height / config.line_height).floor() as usize;

        // Determine first visible line for scrolling very large files
        // For most files, we show the entire document
        // For very large files, we scroll the minimap to keep current view visible
        let first_visible_line = if total_lines <= visible_line_count {
            0
        } else {
            // Center the current viewport in the minimap
            let center_line = viewport.first_line + viewport.visible_lines / 2;
            let half_visible = visible_line_count / 2;

            if center_line < half_visible {
                0
            } else if center_line + half_visible >= total_lines {
                total_lines.saturating_sub(visible_line_count)
            } else {
                center_line.saturating_sub(half_visible)
            }
        };

        // Calculate X position based on minimap position setting
        let x = match config.position {
            MinimapPosition::Left => 0.0,
            MinimapPosition::Right => viewport.width - config.width,
        };

        Self {
            x,
            y: 0.0,
            width: config.width,
            height: viewport.height,
            line_height: config.line_height,
            char_width: config.char_width,
            total_lines,
            scale,
            first_visible_line,
            visible_line_count,
        }
    }

    /// Converts a Y coordinate in the minimap to a document line number.
    ///
    /// # Arguments
    ///
    /// * `y` - Y coordinate relative to minimap top
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn y_to_line(&self, y: f32) -> usize {
        let line_offset = (y / self.line_height).floor() as usize;
        (self.first_visible_line + line_offset).min(self.total_lines.saturating_sub(1))
    }

    /// Converts a document line number to a Y coordinate in the minimap.
    ///
    /// # Arguments
    ///
    /// * `line` - Document line number
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn line_to_y(&self, line: usize) -> f32 {
        let relative_line = line.saturating_sub(self.first_visible_line);
        relative_line as f32 * self.line_height
    }

    /// Checks if a point is within the minimap bounds.
    ///
    /// # Arguments
    ///
    /// * `x` - X coordinate relative to editor
    /// * `y` - Y coordinate relative to editor
    #[must_use]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    /// Returns the minimap bounds as a rect.
    #[must_use]
    pub const fn bounds(&self) -> MinimapRect {
        MinimapRect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

/// A rectangle for minimap rendering.
#[derive(Debug, Clone, Copy)]
pub struct MinimapRect {
    /// X position
    pub x: f32,
    /// Y position
    pub y: f32,
    /// Width
    pub width: f32,
    /// Height
    pub height: f32,
}

impl MinimapRect {
    /// Creates a new minimap rect.
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
}

/// A colored segment in a minimap line.
#[derive(Debug, Clone, Copy)]
pub struct MinimapSegment {
    /// Start column (0-indexed)
    pub start: usize,
    /// End column (exclusive)
    pub end: usize,
    /// Color of this segment
    pub color: Color,
}

impl MinimapSegment {
    /// Creates a new minimap segment.
    #[must_use]
    pub const fn new(start: usize, end: usize, color: Color) -> Self {
        Self { start, end, color }
    }
}

/// A rendered line in the minimap.
#[derive(Debug, Clone)]
pub struct MinimapLine {
    /// Line number in the document
    pub line_number: usize,
    /// Colored segments in this line
    pub segments: Vec<MinimapSegment>,
}

impl MinimapLine {
    /// Creates a new minimap line with segments.
    #[must_use]
    pub const fn new(line_number: usize, segments: Vec<MinimapSegment>) -> Self {
        Self { line_number, segments }
    }

    /// Creates a minimap line from plain text with a single color.
    #[must_use]
    pub fn from_text(line_number: usize, text: &str, color: Color) -> Self {
        let len = text.chars().count();
        if len == 0 {
            return Self { line_number, segments: Vec::new() };
        }

        let segments = vec![MinimapSegment::new(0, len, color)];
        Self { line_number, segments }
    }
}

/// The viewport indicator showing the current visible region.
#[derive(Debug, Clone, Copy)]
pub struct ViewportIndicator {
    /// Y position of the indicator
    pub y: f32,
    /// Height of the indicator
    pub height: f32,
    /// X position (same as minimap)
    pub x: f32,
    /// Width (same as minimap)
    pub width: f32,
    /// Background color with opacity
    pub color: Color,
}

impl ViewportIndicator {
    /// Calculates the viewport indicator position and size.
    ///
    /// # Arguments
    ///
    /// * `dimensions` - Minimap dimensions
    /// * `viewport` - Current editor viewport
    /// * `opacity` - Opacity of the indicator (0.0-1.0)
    /// * `base_color` - Base color for the indicator
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn calculate(
        dimensions: &MinimapDimensions,
        viewport: &Viewport,
        opacity: f32,
        base_color: Color,
    ) -> Self {
        let y = dimensions.line_to_y(viewport.first_line);
        let height = (viewport.visible_lines as f32) * dimensions.line_height;

        // Clamp height to not exceed minimap bounds
        let height = height.min(dimensions.height - y);

        let color = Color::new(base_color.r, base_color.g, base_color.b, opacity);

        Self {
            y,
            height,
            x: dimensions.x,
            width: dimensions.width,
            color,
        }
    }

    /// Returns the indicator as a rect.
    #[must_use]
    pub const fn rect(&self) -> MinimapRect {
        MinimapRect::new(self.x, self.y, self.width, self.height)
    }
}

/// Result of a minimap interaction.
#[derive(Debug, Clone, Copy)]
pub enum MinimapInteraction {
    /// No interaction
    None,
    /// Clicked at a specific line
    Click {
        /// The line that was clicked
        line: usize,
    },
    /// Dragging to a specific line
    Drag {
        /// The line being dragged to
        line: usize,
    },
    /// Hovering over a specific line
    Hover {
        /// The line being hovered over
        line: usize,
    },
}

/// State for minimap drag operations.
#[derive(Debug, Clone, Copy, Default)]
pub struct MinimapDragState {
    /// Whether a drag is in progress
    pub is_dragging: bool,
    /// Initial line when drag started
    pub start_line: Option<usize>,
    /// Offset from the click position to viewport center
    pub viewport_offset: f32,
}

impl MinimapDragState {
    /// Creates a new drag state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            is_dragging: false,
            start_line: None,
            viewport_offset: 0.0,
        }
    }

    /// Starts a drag operation.
    ///
    /// # Arguments
    ///
    /// * `line` - The line where drag started
    /// * `viewport_first_line` - The current first visible line
    /// * `viewport_visible_lines` - Number of visible lines
    #[allow(clippy::cast_precision_loss)]
    pub fn start(&mut self, line: usize, viewport_first_line: usize, viewport_visible_lines: usize) {
        self.is_dragging = true;
        self.start_line = Some(line);

        // Calculate offset from click to viewport center
        let viewport_center = viewport_first_line + viewport_visible_lines / 2;
        self.viewport_offset = line as f32 - viewport_center as f32;
    }

    /// Ends a drag operation.
    pub fn end(&mut self) {
        self.is_dragging = false;
        self.start_line = None;
        self.viewport_offset = 0.0;
    }

    /// Updates during a drag and returns the target line for the viewport.
    ///
    /// # Arguments
    ///
    /// * `current_line` - The line under the cursor
    /// * `viewport_visible_lines` - Number of visible lines in viewport
    /// * `total_lines` - Total lines in document
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
    pub fn update(
        &self,
        current_line: usize,
        viewport_visible_lines: usize,
        total_lines: usize,
    ) -> usize {
        // Calculate target line accounting for the initial offset
        let target_center = current_line as f32 - self.viewport_offset;
        let target_first = target_center - (viewport_visible_lines / 2) as f32;

        let target_first = target_first.max(0.0) as usize;
        let max_first = total_lines.saturating_sub(viewport_visible_lines);

        target_first.min(max_first)
    }
}

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
        self.drag_state.start(clicked_line, viewport.first_line, viewport.visible_lines);

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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_document() -> Document {
        let content = (0..100)
            .map(|i| format!("Line {} with some content here\n", i))
            .collect::<String>();
        Document::new(&content)
    }

    fn create_test_viewport() -> Viewport {
        Viewport::new(800.0, 600.0, 20.0)
    }

    #[test]
    fn minimap_config_default() {
        let config = MinimapConfig::default();
        assert!((config.width - DEFAULT_MINIMAP_WIDTH).abs() < f32::EPSILON);
        assert!(config.enabled);
        assert_eq!(config.position, MinimapPosition::Right);
    }

    #[test]
    fn dimensions_calculate() {
        let config = MinimapConfig::default();
        let viewport = create_test_viewport();
        let document = create_test_document();

        let dims = MinimapDimensions::calculate(&config, &viewport, &document, 20.0);

        assert!((dims.width - DEFAULT_MINIMAP_WIDTH).abs() < f32::EPSILON);
        assert_eq!(dims.total_lines, 101); // 100 lines + trailing newline handling
        assert!(dims.line_height > 0.0);
    }

    #[test]
    fn dimensions_y_to_line() {
        let config = MinimapConfig::default();
        let viewport = create_test_viewport();
        let document = create_test_document();

        let dims = MinimapDimensions::calculate(&config, &viewport, &document, 20.0);

        assert_eq!(dims.y_to_line(0.0), dims.first_visible_line);
        assert_eq!(dims.y_to_line(dims.line_height * 5.0), dims.first_visible_line + 5);
    }

    #[test]
    fn dimensions_line_to_y() {
        let config = MinimapConfig::default();
        let viewport = create_test_viewport();
        let document = create_test_document();

        let dims = MinimapDimensions::calculate(&config, &viewport, &document, 20.0);

        let y = dims.line_to_y(dims.first_visible_line + 10);
        let expected = 10.0 * dims.line_height;
        assert!((y - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn dimensions_contains() {
        let config = MinimapConfig::default();
        let viewport = create_test_viewport();
        let document = create_test_document();

        let dims = MinimapDimensions::calculate(&config, &viewport, &document, 20.0);

        // Inside bounds
        assert!(dims.contains(dims.x + 10.0, dims.y + 10.0));

        // Outside bounds
        assert!(!dims.contains(0.0, 0.0)); // Too far left
        assert!(!dims.contains(dims.x + dims.width + 10.0, 50.0)); // Too far right
    }

    #[test]
    fn viewport_indicator_calculate() {
        let config = MinimapConfig::default();
        let viewport = create_test_viewport();
        let document = create_test_document();

        let dims = MinimapDimensions::calculate(&config, &viewport, &document, 20.0);
        let indicator = ViewportIndicator::calculate(&dims, &viewport, 0.15, Color::rgb(1.0, 1.0, 1.0));

        assert!(indicator.height > 0.0);
        assert!((indicator.color.a - 0.15).abs() < 0.01);
    }

    #[test]
    fn minimap_line_from_text() {
        let line = MinimapLine::from_text(0, "Hello World", Color::rgb(1.0, 1.0, 1.0));

        assert_eq!(line.line_number, 0);
        assert_eq!(line.segments.len(), 1);
        assert_eq!(line.segments[0].start, 0);
        assert_eq!(line.segments[0].end, 11);
    }

    #[test]
    fn minimap_line_empty() {
        let line = MinimapLine::from_text(5, "", Color::rgb(1.0, 1.0, 1.0));

        assert_eq!(line.line_number, 5);
        assert!(line.segments.is_empty());
    }

    #[test]
    fn renderer_new() {
        let renderer = MinimapRenderer::new();
        assert!(renderer.is_enabled());
        assert!(!renderer.is_dragging());
    }

    #[test]
    fn renderer_set_enabled() {
        let mut renderer = MinimapRenderer::new();

        renderer.set_enabled(false);
        assert!(!renderer.is_enabled());
        assert!((renderer.width() - 0.0).abs() < f32::EPSILON);

        renderer.set_enabled(true);
        assert!(renderer.is_enabled());
        assert!(renderer.width() > 0.0);
    }

    #[test]
    fn renderer_prepare_lines() {
        let mut renderer = MinimapRenderer::new();
        let document = create_test_document();
        let viewport = create_test_viewport();
        let theme = Theme::dark();

        let dims = renderer.calculate_dimensions(&viewport, &document, 20.0);
        renderer.prepare_lines(&document, &dims, &theme, None);

        assert!(!renderer.lines().is_empty());
    }

    #[test]
    fn renderer_generate_rects() {
        let mut renderer = MinimapRenderer::new();
        let document = create_test_document();
        let viewport = create_test_viewport();
        let theme = Theme::dark();

        let dims = renderer.calculate_dimensions(&viewport, &document, 20.0);
        renderer.prepare_lines(&document, &dims, &theme, None);

        let rects = renderer.generate_rects(&dims);
        assert!(!rects.is_empty());

        // Verify rects are within bounds
        for (rect, _color) in &rects {
            assert!(rect.x >= dims.x);
            assert!(rect.x + rect.width <= dims.x + dims.width + 1.0);
            assert!(rect.height > 0.0);
        }
    }

    #[test]
    fn renderer_handle_click() {
        let mut renderer = MinimapRenderer::new();
        let document = create_test_document();
        let viewport = create_test_viewport();

        let dims = renderer.calculate_dimensions(&viewport, &document, 20.0);

        // Click in the middle of the minimap
        let click_x = dims.x + dims.width / 2.0;
        let click_y = dims.height / 2.0;

        let result = renderer.handle_click(click_x, click_y, &dims, &viewport);
        assert!(result.is_some());

        // Should start dragging
        assert!(renderer.is_dragging());
    }

    #[test]
    fn renderer_handle_click_outside() {
        let mut renderer = MinimapRenderer::new();
        let document = create_test_document();
        let viewport = create_test_viewport();

        let dims = renderer.calculate_dimensions(&viewport, &document, 20.0);

        // Click outside the minimap
        let result = renderer.handle_click(0.0, 50.0, &dims, &viewport);
        assert!(result.is_none());
        assert!(!renderer.is_dragging());
    }

    #[test]
    fn drag_state_start_and_end() {
        let mut state = MinimapDragState::new();

        assert!(!state.is_dragging);

        state.start(50, 10, 30);
        assert!(state.is_dragging);
        assert_eq!(state.start_line, Some(50));

        state.end();
        assert!(!state.is_dragging);
        assert!(state.start_line.is_none());
    }

    #[test]
    fn drag_state_update() {
        let mut state = MinimapDragState::new();
        state.start(50, 10, 30);

        let target = state.update(60, 30, 100);

        // Target should be somewhere near the dragged line
        assert!(target <= 100);
    }

    #[test]
    fn renderer_cache_invalidation() {
        let mut renderer = MinimapRenderer::new();
        let document = create_test_document();
        let viewport = create_test_viewport();
        let theme = Theme::dark();

        let dims = renderer.calculate_dimensions(&viewport, &document, 20.0);
        renderer.prepare_lines(&document, &dims, &theme, None);

        let line_count = renderer.lines().len();
        assert!(line_count > 0);

        // Invalidate and check
        renderer.invalidate_cache();
        assert!(renderer.lines().is_empty());
    }

    #[test]
    fn minimap_position_left() {
        let config = MinimapConfig {
            position: MinimapPosition::Left,
            ..Default::default()
        };
        let viewport = create_test_viewport();
        let document = create_test_document();

        let dims = MinimapDimensions::calculate(&config, &viewport, &document, 20.0);

        assert!((dims.x - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn minimap_position_right() {
        let config = MinimapConfig {
            position: MinimapPosition::Right,
            ..Default::default()
        };
        let viewport = create_test_viewport();
        let document = create_test_document();

        let dims = MinimapDimensions::calculate(&config, &viewport, &document, 20.0);

        let expected_x = viewport.width - config.width;
        assert!((dims.x - expected_x).abs() < f32::EPSILON);
    }

    #[test]
    fn minimap_segment() {
        let segment = MinimapSegment::new(5, 10, Color::rgb(1.0, 0.0, 0.0));
        assert_eq!(segment.start, 5);
        assert_eq!(segment.end, 10);
    }

    #[test]
    fn minimap_rect() {
        let rect = MinimapRect::new(10.0, 20.0, 100.0, 50.0);
        assert!((rect.x - 10.0).abs() < f32::EPSILON);
        assert!((rect.y - 20.0).abs() < f32::EPSILON);
        assert!((rect.width - 100.0).abs() < f32::EPSILON);
        assert!((rect.height - 50.0).abs() < f32::EPSILON);
    }
}
