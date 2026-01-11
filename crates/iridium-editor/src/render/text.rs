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
    pub fn font_size(&self) -> f32 {
        self.config.font_size
    }

    /// Returns the current line height in pixels.
    #[must_use]
    pub fn line_height(&self) -> f32 {
        self.config.font_size * self.config.line_height
    }

    /// Returns the metrics for the current font configuration.
    #[must_use]
    pub fn metrics(&self) -> Metrics {
        Metrics::relative(self.config.font_size, self.config.line_height)
    }

    /// Returns a mutable reference to the font system.
    ///
    /// This can be used to load additional fonts or query font information.
    pub fn font_system_mut(&mut self) -> &mut FontSystem {
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
        self.viewport.update(
            queue,
            Resolution {
                width,
                height,
            },
        );
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
    pub fn create_text_area<'a>(
        buffer: &'a Buffer,
        left: f32,
        top: f32,
        scale: f32,
        bounds: TextBounds,
        default_color: Color,
    ) -> TextArea<'a> {
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
    fn to_glyphon_color(color: Color) -> GlyphonColor {
        GlyphonColor::rgba(
            (color.r * 255.0) as u8,
            (color.g * 255.0) as u8,
            (color.b * 255.0) as u8,
            (color.a * 255.0) as u8,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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
