//! Text rendering with glyphon.
//!
//! This module provides GPU-accelerated text rendering using glyphon
//! for glyph rasterization and cosmic-text for text shaping.

use glyphon::{
    Attrs, Buffer, Cache, Color as GlyphonColor, Family, FontSystem, Metrics, Resolution, Shaping,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer as GlyphonTextRenderer, Viewport,
};
use wgpu::{Device, MultisampleState, Queue, TextureFormat};

use crate::editor::IridiumError;
use crate::theme::Color;

/// Default font size in pixels.
const DEFAULT_FONT_SIZE: f32 = 14.0;

/// Default line height multiplier.
const DEFAULT_LINE_HEIGHT: f32 = 1.4;

/// Configuration for text rendering.
#[derive(Debug, Clone)]
pub struct TextRenderConfig {
    /// Font size in pixels
    pub font_size: f32,
    /// Line height multiplier
    pub line_height: f32,
    /// Default font family
    pub font_family: String,
}

impl Default for TextRenderConfig {
    fn default() -> Self {
        Self {
            font_size: DEFAULT_FONT_SIZE,
            line_height: DEFAULT_LINE_HEIGHT,
            font_family: "monospace".to_string(),
        }
    }
}

/// Text renderer using glyphon for GPU text rendering.
///
/// This provides the core text rendering functionality for the editor,
/// including:
/// - Font loading and management via `FontSystem`
/// - Glyph caching via `SwashCache` and `TextAtlas`
/// - High-performance GPU rendering via `TextRenderer`
///
/// # Architecture
///
/// The renderer uses a multi-level cache:
/// 1. `FontSystem` - Loads and manages fonts
/// 2. `SwashCache` - Rasterizes glyphs on the CPU
/// 3. `TextAtlas` - GPU texture atlas for rendered glyphs
///
/// # Example
///
/// ```ignore
/// let text_renderer = TextRenderer::new(
///     &device,
///     &queue,
///     TextureFormat::Bgra8UnormSrgb,
/// )?;
///
/// // Create a text buffer and render it
/// let mut buffer = text_renderer.create_buffer(800.0);
/// text_renderer.set_text(&mut buffer, "Hello, world!");
/// ```
pub struct TextRenderer {
    /// Font system for loading and managing fonts
    font_system: FontSystem,
    /// CPU-side glyph cache using swash
    swash_cache: SwashCache,
    /// GPU glyph atlas cache
    atlas: TextAtlas,
    /// The glyphon text renderer
    renderer: GlyphonTextRenderer,
    /// glyphon viewport for resolution handling
    viewport: Viewport,
    /// Current text rendering configuration
    config: TextRenderConfig,
    /// Texture format for rendering
    texture_format: TextureFormat,
}

impl std::fmt::Debug for TextRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextRenderer")
            .field("config", &self.config)
            .field("texture_format", &self.texture_format)
            .finish_non_exhaustive()
    }
}

impl TextRenderer {
    /// Creates a new text renderer.
    ///
    /// # Arguments
    ///
    /// * `device` - The wgpu device for GPU operations
    /// * `queue` - The wgpu queue for command submission
    /// * `texture_format` - The format of the render target
    ///
    /// # Errors
    ///
    /// Returns an error if font loading fails.
    pub fn new(
        device: &Device,
        queue: &Queue,
        texture_format: TextureFormat,
    ) -> Result<Self, IridiumError> {
        Self::with_config(device, queue, texture_format, TextRenderConfig::default())
    }

    /// Creates a new text renderer with custom configuration.
    ///
    /// # Arguments
    ///
    /// * `device` - The wgpu device for GPU operations
    /// * `queue` - The wgpu queue for command submission
    /// * `texture_format` - The format of the render target
    /// * `config` - Text rendering configuration
    ///
    /// # Errors
    ///
    /// Returns an error if font loading fails.
    pub fn with_config(
        device: &Device,
        queue: &Queue,
        texture_format: TextureFormat,
        config: TextRenderConfig,
    ) -> Result<Self, IridiumError> {
        // Initialize the font system with system fonts
        let font_system = FontSystem::new();

        // Create the CPU-side glyph cache
        let swash_cache = SwashCache::new();

        // Create the GPU glyph cache (atlas)
        let cache = Cache::new(device);
        let mut atlas = TextAtlas::new(device, queue, &cache, texture_format);

        // Create the glyphon text renderer
        let renderer =
            GlyphonTextRenderer::new(&mut atlas, device, MultisampleState::default(), None);

        // Create viewport for resolution handling
        let viewport = Viewport::new(device, &cache);

        Ok(Self {
            font_system,
            swash_cache,
            atlas,
            renderer,
            viewport,
            config,
            texture_format,
        })
    }

    /// Returns the current font size in pixels.
    #[must_use]
    pub const fn font_size(&self) -> f32 {
        self.config.font_size
    }

    /// Returns the current line height in pixels.
    #[must_use]
    pub fn line_height(&self) -> f32 {
        self.config.font_size * self.config.line_height
    }

    /// Returns the actual character width by measuring a rendered character.
    ///
    /// This measures the advance width of a character using the current font,
    /// giving accurate cursor positioning regardless of resolution or font size.
    #[must_use]
    pub fn char_width(&mut self) -> f32 {
        // Create a temporary buffer to measure a character
        let metrics = Metrics::relative(self.config.font_size, self.config.line_height);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(&mut self.font_system, Some(100.0), None);

        // Use a simple character to measure - 'M' is typically the widest
        let attrs = Attrs::new().family(Family::Monospace);
        buffer.set_text(&mut self.font_system, "MM", &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);

        // Get the width from the layout
        for run in buffer.layout_runs() {
            // For monospace, each glyph should have the same advance
            // Measure "MM" and divide by 2 for more accuracy
            let mut total_width = 0.0;
            let mut glyph_count = 0;
            for glyph in run.glyphs.iter() {
                total_width += glyph.w;
                glyph_count += 1;
            }
            if glyph_count > 0 {
                return total_width / glyph_count as f32;
            }
        }

        // Fallback to approximation if measurement fails
        self.config.font_size * 0.6
    }

    /// Returns the metrics for the current font configuration.
    #[must_use]
    pub fn metrics(&self) -> Metrics {
        Metrics::relative(self.config.font_size, self.config.line_height)
    }

    /// Returns a mutable reference to the font system.
    ///
    /// This can be used to load additional fonts or query font information.
    pub const fn font_system_mut(&mut self) -> &mut FontSystem {
        &mut self.font_system
    }

    /// Creates a new text buffer for rendering.
    ///
    /// The buffer is initialized with the current font metrics and
    /// the specified width.
    ///
    /// # Arguments
    ///
    /// * `width` - The width of the text area in pixels
    pub fn create_buffer(&mut self, width: f32) -> Buffer {
        let metrics = self.metrics();
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(&mut self.font_system, Some(width), None);
        buffer
    }

    /// Sets the text content of a buffer.
    ///
    /// # Arguments
    ///
    /// * `buffer` - The buffer to update
    /// * `text` - The text content to set
    /// * `color` - The text color
    pub fn set_text(&mut self, buffer: &mut Buffer, text: &str, color: Color) {
        let attrs = Attrs::new()
            .family(Family::Monospace)
            .color(Self::to_glyphon_color(color));

        buffer.set_text(&mut self.font_system, text, &attrs, Shaping::Advanced, None);
    }

    /// Sets the text content with multiple styled spans.
    ///
    /// # Arguments
    ///
    /// * `buffer` - The buffer to update
    /// * `spans` - Iterator of (text, color) pairs
    pub fn set_rich_text<'a>(
        &mut self,
        buffer: &mut Buffer,
        spans: impl Iterator<Item = (&'a str, Color)>,
    ) {
        let rich_text: Vec<(&str, Attrs)> = spans
            .map(|(text, color)| {
                let attrs = Attrs::new()
                    .family(Family::Monospace)
                    .color(Self::to_glyphon_color(color));
                (text, attrs)
            })
            .collect();

        buffer.set_rich_text(
            &mut self.font_system,
            rich_text,
            &Attrs::new().family(Family::Monospace),
            Shaping::Advanced,
            None,
        );
    }

    /// Shapes the text in a buffer, preparing it for rendering.
    ///
    /// This must be called after modifying the buffer content and
    /// before rendering.
    pub fn shape_buffer(&mut self, buffer: &mut Buffer) {
        buffer.shape_until_scroll(&mut self.font_system, false);
    }

    /// Calculates cursor visual position accounting for line wrapping.
    ///
    /// Returns (x, y) offset from the buffer origin, where y accounts for
    /// wrapped lines that come before the cursor position.
    ///
    /// # Arguments
    ///
    /// * `buffer` - The shaped buffer containing the text
    /// * `line` - The logical line number (0-indexed)
    /// * `column` - The column within the line (0-indexed)
    /// * `char_width` - The width of a single character
    #[must_use]
    pub fn cursor_position_in_buffer(
        &self,
        buffer: &Buffer,
        line: usize,
        column: usize,
        char_width: f32,
    ) -> (f32, f32) {
        let line_height = self.line_height();
        let mut visual_line: usize = 0;
        let mut cursor_x = 0.0_f32;
        let mut cursor_y = 0.0_f32;
        let mut found = false;

        // Track columns consumed within the current logical line for wrapping
        for run in buffer.layout_runs() {
            let run_line = run.line_i;

            if run_line < line {
                // This run is before our target line, just count it
                visual_line += 1;
            } else if run_line == line && !found {
                // This run is on our target line
                // Check if cursor column falls within this run's glyph range
                let run_start_col = if run.glyphs.is_empty() {
                    0
                } else {
                    run.glyphs.first().map(|g| g.start).unwrap_or(0)
                };
                let run_end_col = if run.glyphs.is_empty() {
                    0
                } else {
                    run.glyphs.last().map(|g| g.end).unwrap_or(0)
                };

                if column <= run_end_col || run.glyphs.is_empty() {
                    // Cursor is on this visual line
                    cursor_y = visual_line as f32 * line_height;

                    // Calculate X position within this run
                    let col_in_run = column.saturating_sub(run_start_col);
                    cursor_x = col_in_run as f32 * char_width;
                    found = true;
                }
                visual_line += 1;
            } else if run_line > line && !found {
                // We've passed the target line (empty line before this run)
                cursor_y = visual_line as f32 * line_height;
                cursor_x = column as f32 * char_width;
                found = true;
            }
        }

        // Handle case where cursor is past all content (empty trailing line)
        if !found {
            cursor_y = visual_line as f32 * line_height;
            cursor_x = column as f32 * char_width;
        }

        (cursor_x, cursor_y)
    }

    /// Counts how many visual lines each logical line produces after wrapping.
    ///
    /// Returns a vector where index i contains the number of visual lines
    /// for logical line i. This accounts for line wrapping.
    #[must_use]
    pub fn visual_lines_per_logical_line(&self, buffer: &Buffer) -> Vec<usize> {
        let mut counts: Vec<usize> = Vec::new();

        for run in buffer.layout_runs() {
            let line_i = run.line_i;
            // Extend counts vector if needed
            while counts.len() <= line_i {
                counts.push(0);
            }
            counts[line_i] += 1;
        }

        // Ensure at least 1 visual line for empty lines that might not have runs
        for count in &mut counts {
            if *count == 0 {
                *count = 1;
            }
        }

        counts
    }

    /// Updates the viewport resolution.
    ///
    /// This should be called when the window size changes.
    ///
    /// # Arguments
    ///
    /// * `queue` - The wgpu queue for updates
    /// * `width` - The new width in pixels
    /// * `height` - The new height in pixels
    pub fn update_viewport(&mut self, queue: &Queue, width: u32, height: u32) {
        self.viewport.update(queue, Resolution { width, height });
    }

    /// Prepares text areas for rendering.
    ///
    /// This uploads glyph data to the GPU and prepares the render state.
    ///
    /// # Arguments
    ///
    /// * `device` - The wgpu device
    /// * `queue` - The wgpu queue
    /// * `text_areas` - The text areas to render
    ///
    /// # Errors
    ///
    /// Returns an error if GPU upload fails.
    pub fn prepare<'a>(
        &mut self,
        device: &Device,
        queue: &Queue,
        text_areas: impl IntoIterator<Item = TextArea<'a>>,
    ) -> Result<(), IridiumError> {
        self.renderer
            .prepare(
                device,
                queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                text_areas,
                &mut self.swash_cache,
            )
            .map_err(|e| IridiumError::GpuInitFailed {
                message: format!("Failed to prepare text for rendering: {e}"),
            })
    }

    /// Renders the prepared text areas.
    ///
    /// # Arguments
    ///
    /// * `pass` - The render pass to draw into
    ///
    /// # Errors
    ///
    /// Returns an error if rendering fails.
    pub fn render<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) -> Result<(), IridiumError> {
        self.renderer
            .render(&self.atlas, &self.viewport, pass)
            .map_err(|e| IridiumError::GpuInitFailed {
                message: format!("Failed to render text: {e}"),
            })
    }

    /// Trims the atlas cache, removing unused glyphs.
    ///
    /// This should be called periodically to prevent unbounded
    /// atlas growth.
    pub fn trim_cache(&mut self) {
        self.atlas.trim();
    }

    /// Loads a font from file data (T148).
    ///
    /// The font data should be the raw bytes of a TrueType (.ttf) or
    /// OpenType (.otf) font file.
    ///
    /// # Arguments
    ///
    /// * `data` - Font file data as bytes
    ///
    /// # Note
    ///
    /// The font is added to the system's font database and becomes available
    /// for use via `set_font_family()`.
    pub fn load_font(&mut self, data: Vec<u8>) {
        self.font_system.db_mut().load_font_data(data);
    }

    /// Loads a font from a file path (T148).
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the font file
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read.
    pub fn load_font_file(&mut self, path: &std::path::Path) -> Result<(), IridiumError> {
        let data = std::fs::read(path).map_err(|e| IridiumError::FontLoadFailed {
            message: format!("Failed to read font file {}: {e}", path.display()),
        })?;

        self.load_font(data);
        Ok(())
    }

    /// Returns the current configuration.
    #[must_use]
    pub const fn config(&self) -> &TextRenderConfig {
        &self.config
    }

    /// Updates the font size at runtime (T149).
    ///
    /// This invalidates all existing text buffers, which should be
    /// recreated with the new size.
    ///
    /// # Arguments
    ///
    /// * `size` - New font size in pixels
    pub fn set_font_size(&mut self, size: f32) {
        self.config.font_size = size;
        // Clear the glyph cache since glyphs will be at a different size
        self.atlas.trim();
    }

    /// Updates the line height at runtime (T149).
    ///
    /// # Arguments
    ///
    /// * `height` - New line height multiplier
    pub fn set_line_height(&mut self, height: f32) {
        self.config.line_height = height;
    }

    /// Updates the font family at runtime (T149).
    ///
    /// The family name should match a font already loaded in the system.
    /// Use `load_font` or `load_font_file` to add custom fonts first.
    ///
    /// # Arguments
    ///
    /// * `family` - Font family name (e.g., "`JetBrains` Mono")
    pub fn set_font_family(&mut self, family: impl Into<String>) {
        self.config.font_family = family.into();
        // Clear the glyph cache since we'll be using different glyphs
        self.atlas.trim();
    }

    /// Updates the configuration from a theme's typography settings (T149).
    ///
    /// This is a convenience method for updating all text rendering settings
    /// at once when the theme changes.
    ///
    /// # Arguments
    ///
    /// * `typography` - Typography settings from a theme
    pub fn apply_typography(&mut self, typography: &crate::theme::Typography) {
        self.config.font_size = typography.font_size;
        self.config.line_height = typography.line_height;
        self.config.font_family.clone_from(&typography.font_family);
        // Clear the glyph cache since settings changed
        self.atlas.trim();
    }

    /// Queries available font families in the system.
    ///
    /// Returns a list of font family names that can be used with `set_font_family`.
    pub fn available_font_families(&self) -> Vec<String> {
        self.font_system
            .db()
            .faces()
            .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect()
    }

    /// Checks if a font family is available.
    ///
    /// # Arguments
    ///
    /// * `family` - Font family name to check
    #[must_use]
    pub fn has_font_family(&self, family: &str) -> bool {
        self.font_system
            .db()
            .faces()
            .any(|face| face.families.iter().any(|(name, _)| name == family))
    }

    /// Creates a text area from a buffer for rendering.
    ///
    /// # Arguments
    ///
    /// * `buffer` - The shaped text buffer
    /// * `left` - Left position in pixels
    /// * `top` - Top position in pixels
    /// * `scale` - Scale factor (1.0 for normal size)
    /// * `bounds` - The bounds to clip text to
    /// * `default_color` - Default text color
    #[must_use]
    pub fn create_text_area(
        buffer: &Buffer,
        left: f32,
        top: f32,
        scale: f32,
        bounds: TextBounds,
        default_color: Color,
    ) -> TextArea<'_> {
        TextArea {
            buffer,
            left,
            top,
            scale,
            bounds,
            default_color: Self::to_glyphon_color(default_color),
            custom_glyphs: &[],
        }
    }

    /// Converts a theme Color to a glyphon Color.
    ///
    /// Color components are expected to be in 0.0..=1.0 range.
    /// Values are clamped and converted to u8.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn to_glyphon_color(color: Color) -> GlyphonColor {
        GlyphonColor::rgba(
            (color.r.clamp(0.0, 1.0) * 255.0) as u8,
            (color.g.clamp(0.0, 1.0) * 255.0) as u8,
            (color.b.clamp(0.0, 1.0) * 255.0) as u8,
            (color.a.clamp(0.0, 1.0) * 255.0) as u8,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::float_cmp)]
    fn text_render_config_default() {
        let config = TextRenderConfig::default();
        assert_eq!(config.font_size, DEFAULT_FONT_SIZE);
        assert_eq!(config.line_height, DEFAULT_LINE_HEIGHT);
        assert_eq!(config.font_family, "monospace");
    }

    #[test]
    fn color_conversion() {
        let color = Color::new(1.0, 0.5, 0.25, 1.0);
        let glyphon_color = TextRenderer::to_glyphon_color(color);
        // Verify color was converted (glyphon::Color doesn't expose fields directly)
        // Just verify it doesn't panic
        let _ = glyphon_color;
    }

    // Note: Actual rendering tests require GPU and are run as integration tests
}
