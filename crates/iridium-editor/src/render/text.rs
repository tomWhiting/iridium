//! Text rendering with glyphon.
//!
//! This module provides GPU-accelerated text rendering using the glyphon
//! library, which handles font loading, text shaping, and glyph rasterization.

use super::error::RenderError;
use glyphon::{
    Attrs, Buffer as GlyphonBuffer, Cache, Color, Family, FontSystem, Metrics, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer as GlyphonTextRenderer, Viewport,
};
use tracing::{debug, info};

/// Configuration for text rendering.
#[derive(Debug, Clone)]
pub struct TextConfig {
    /// Default font size in pixels.
    pub font_size: f32,
    /// Line height multiplier.
    pub line_height: f32,
    /// Default font family.
    pub font_family: String,
}

impl Default for TextConfig {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            line_height: 1.4,
            font_family: "monospace".to_string(),
        }
    }
}

/// Text renderer using glyphon for GPU-accelerated text.
///
/// The TextRenderer manages font loading, text layout, and glyph atlas
/// management for efficient text rendering.
pub struct TextRenderer {
    /// The font system containing loaded fonts.
    font_system: FontSystem,
    /// The swash cache for glyph rasterization.
    swash_cache: SwashCache,
    /// The glyphon cache for shared resources.
    #[allow(dead_code)]
    cache: Cache,
    /// The text atlas for storing glyph textures.
    atlas: TextAtlas,
    /// The viewport for clipping and resolution.
    viewport: Viewport,
    /// The glyphon text renderer.
    renderer: GlyphonTextRenderer,
    /// Text buffer for layout.
    text_buffer: GlyphonBuffer,
    /// Current configuration.
    config: TextConfig,
    /// Atlas texture format.
    format: wgpu::TextureFormat,
    /// Current dimensions.
    dimensions: (u32, u32),
}

impl TextRenderer {
    /// Creates a new text renderer.
    ///
    /// # Arguments
    ///
    /// * `device` - The wgpu device for GPU resources
    /// * `queue` - The wgpu queue for uploading data
    /// * `format` - The texture format for rendering
    /// * `width` - Viewport width
    /// * `height` - Viewport height
    ///
    /// # Errors
    ///
    /// Returns an error if font system or atlas initialization fails.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Result<Self, RenderError> {
        Self::with_config(device, queue, format, width, height, TextConfig::default())
    }

    /// Creates a new text renderer with custom configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if initialization fails.
    pub fn with_config(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        config: TextConfig,
    ) -> Result<Self, RenderError> {
        info!("Initializing text renderer: font_size={}", config.font_size);

        // Initialize font system with system fonts
        let mut font_system = FontSystem::new();
        debug!("Font system initialized with system fonts");

        // Create swash cache for glyph rasterization
        let swash_cache = SwashCache::new();

        // Create glyphon cache for shared resources
        let cache = Cache::new(device);

        // Create text atlas
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        debug!("Text atlas created");

        // Create viewport for clipping and resolution
        let mut viewport = Viewport::new(device, &cache);
        viewport.update(queue, glyphon::Resolution { width, height });

        // Create the glyphon text renderer
        let renderer =
            GlyphonTextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);

        // Create text buffer for layout
        let metrics = Metrics::new(config.font_size, config.font_size * config.line_height);
        let mut text_buffer = GlyphonBuffer::new(&mut font_system, metrics);
        text_buffer.set_size(&mut font_system, Some(width as f32), Some(height as f32));

        Ok(Self {
            font_system,
            swash_cache,
            cache,
            atlas,
            viewport,
            renderer,
            text_buffer,
            config,
            format,
            dimensions: (width, height),
        })
    }

    /// Returns a reference to the font system.
    #[must_use]
    pub fn font_system(&self) -> &FontSystem {
        &self.font_system
    }

    /// Returns a mutable reference to the font system.
    pub fn font_system_mut(&mut self) -> &mut FontSystem {
        &mut self.font_system
    }

    /// Returns the current configuration.
    #[must_use]
    pub fn config(&self) -> &TextConfig {
        &self.config
    }

    /// Returns the current dimensions.
    #[must_use]
    pub fn dimensions(&self) -> (u32, u32) {
        self.dimensions
    }

    /// Resizes the viewport.
    pub fn resize(&mut self, queue: &wgpu::Queue, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        debug!("Resizing text viewport to {}x{}", width, height);
        self.dimensions = (width, height);
        self.viewport
            .update(queue, glyphon::Resolution { width, height });
        self.text_buffer.set_size(
            &mut self.font_system,
            Some(width as f32),
            Some(height as f32),
        );
    }

    /// Sets the text content to render.
    ///
    /// This updates the internal text buffer with new content for layout.
    pub fn set_text(&mut self, text: &str) {
        self.text_buffer.set_text(
            &mut self.font_system,
            text,
            &Attrs::new().family(Family::Monospace),
            Shaping::Advanced,
            None,
        );
    }

    /// Sets text with custom attributes.
    pub fn set_text_with_attrs(&mut self, text: &str, attrs: &Attrs) {
        self.text_buffer
            .set_text(&mut self.font_system, text, attrs, Shaping::Advanced, None);
    }

    /// Updates the font size.
    pub fn set_font_size(&mut self, size: f32) {
        self.config.font_size = size;
        let metrics = Metrics::new(size, size * self.config.line_height);
        self.text_buffer.set_metrics(&mut self.font_system, metrics);
    }

    /// Updates the line height multiplier.
    pub fn set_line_height(&mut self, line_height: f32) {
        self.config.line_height = line_height;
        let metrics = Metrics::new(self.config.font_size, self.config.font_size * line_height);
        self.text_buffer.set_metrics(&mut self.font_system, metrics);
    }

    /// Performs text layout and shaping.
    ///
    /// Call this after setting text content and before rendering.
    pub fn shape(&mut self) {
        self.text_buffer
            .shape_until_scroll(&mut self.font_system, false);
    }

    /// Creates a TextArea for rendering at the given position.
    pub fn create_text_area(&self, x: f32, y: f32, color: Color) -> TextArea<'_> {
        TextArea {
            buffer: &self.text_buffer,
            left: x,
            top: y,
            scale: 1.0,
            bounds: TextBounds {
                left: 0,
                top: 0,
                right: self.dimensions.0 as i32,
                bottom: self.dimensions.1 as i32,
            },
            default_color: color,
            custom_glyphs: &[],
        }
    }

    /// Prepares the renderer for a frame.
    ///
    /// This uploads glyph data to the GPU. Call before render().
    ///
    /// # Errors
    ///
    /// Returns an error if GPU upload fails.
    pub fn prepare<'a>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        text_areas: impl IntoIterator<Item = TextArea<'a>>,
    ) -> Result<(), RenderError> {
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
            .map_err(|e| RenderError::TextLayoutFailed {
                message: format!("Failed to prepare text: {e:?}"),
            })?;

        Ok(())
    }

    /// Prepares the renderer with a text area at the given position.
    ///
    /// This is a convenience method that creates the text area and prepares
    /// in one call, avoiding borrow conflicts.
    ///
    /// # Errors
    ///
    /// Returns an error if GPU upload fails.
    pub fn prepare_at(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        x: f32,
        y: f32,
        color: Color,
    ) -> Result<(), RenderError> {
        let text_area = TextArea {
            buffer: &self.text_buffer,
            left: x,
            top: y,
            scale: 1.0,
            bounds: TextBounds {
                left: 0,
                top: 0,
                right: self.dimensions.0 as i32,
                bottom: self.dimensions.1 as i32,
            },
            default_color: color,
            custom_glyphs: &[],
        };

        self.renderer
            .prepare(
                device,
                queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                [text_area],
                &mut self.swash_cache,
            )
            .map_err(|e| RenderError::TextLayoutFailed {
                message: format!("Failed to prepare text: {e:?}"),
            })?;

        Ok(())
    }

    /// Renders text to the given render pass.
    ///
    /// Call this after prepare(). Renders all text areas that were prepared.
    ///
    /// # Errors
    ///
    /// Returns an error if rendering fails.
    pub fn render<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) -> Result<(), RenderError> {
        self.renderer
            .render(&self.atlas, &self.viewport, pass)
            .map_err(|e| RenderError::TextLayoutFailed {
                message: format!("Failed to render text: {e:?}"),
            })?;

        Ok(())
    }

    /// Trims the atlas cache.
    ///
    /// Call periodically to reclaim unused atlas space.
    pub fn trim_atlas(&mut self) {
        self.atlas.trim();
    }

    /// Returns metrics about the text layout.
    #[must_use]
    pub fn layout_metrics(&self) -> LayoutMetrics {
        let runs = self.text_buffer.layout_runs();
        let line_count = runs.count();

        LayoutMetrics {
            line_count,
            font_size: self.config.font_size,
            line_height: self.config.font_size * self.config.line_height,
        }
    }
}

/// Metrics about the current text layout.
#[derive(Debug, Clone, Copy)]
pub struct LayoutMetrics {
    /// Number of lines in the layout.
    pub line_count: usize,
    /// The font size in pixels.
    pub font_size: f32,
    /// The line height in pixels.
    pub line_height: f32,
}

impl std::fmt::Debug for TextRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextRenderer")
            .field("config", &self.config)
            .field("format", &self.format)
            .field("dimensions", &self.dimensions)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_config_default() {
        let config = TextConfig::default();
        assert_eq!(config.font_size, 14.0);
        assert_eq!(config.line_height, 1.4);
    }

    #[test]
    fn test_layout_metrics() {
        let metrics = LayoutMetrics {
            line_count: 10,
            font_size: 14.0,
            line_height: 19.6,
        };
        assert_eq!(metrics.line_count, 10);
        assert_eq!(metrics.font_size, 14.0);
    }
}
