//! Minimap type definitions.
//!
//! This module contains all the data types used for minimap rendering,
//! including configuration, dimensions, and visual elements.

use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::render::Viewport;
use crate::theme::Color;

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
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation
    )]
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
        Self {
            x,
            y,
            width,
            height,
        }
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
        Self {
            line_number,
            segments,
        }
    }

    /// Creates a minimap line from plain text with a single color.
    #[must_use]
    pub fn from_text(line_number: usize, text: &str, color: Color) -> Self {
        let len = text.chars().count();
        if len == 0 {
            return Self {
                line_number,
                segments: Vec::new(),
            };
        }

        let segments = vec![MinimapSegment::new(0, len, color)];
        Self {
            line_number,
            segments,
        }
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
    pub fn start(
        &mut self,
        line: usize,
        viewport_first_line: usize,
        viewport_visible_lines: usize,
    ) {
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
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
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
