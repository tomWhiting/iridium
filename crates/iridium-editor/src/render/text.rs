//! Text rendering with glyphon.
//!
//! This module provides GPU-accelerated text rendering using glyphon
//! for glyph rasterization and cosmic-text for text shaping.

use glyphon::cosmic_text::{BidiParagraphs, LineEnding, LineIter};
use glyphon::{
    Attrs, AttrsList, Buffer, BufferLine, Cache, Color as GlyphonColor, Family, FontSystem,
    Metrics, Resolution, Shaping, SwashCache, TextArea, TextAtlas, TextBounds,
    TextRenderer as GlyphonTextRenderer, Viewport,
};
use wgpu::{Device, MultisampleState, Queue, TextureFormat};

use super::units::index_to_f32;
use crate::editor::IridiumError;
use crate::theme::Color;

/// Default font size in pixels.
const DEFAULT_FONT_SIZE: f32 = 14.0;

/// Default line height multiplier.
const DEFAULT_LINE_HEIGHT: f32 = 1.4;

/// The largest font size [`TextRenderer::set_font_size`] will accept, in
/// pixels.
///
/// Not a rendering limit — a display could ask for more and glyphon would
/// draw it. It is an upper bound chosen so that `font_size * line_height`
/// stays finite whatever the two factors are, which is the property
/// [`TextRenderer::line_height`] promises its callers. Two separately finite
/// factors can still multiply to infinity, so bounding only below is not
/// enough.
const MAX_FONT_SIZE: f32 = 1_024.0;

/// The smallest font size [`TextRenderer::set_font_size`] will accept, in
/// pixels.
///
/// One physical pixel. Bounding *below* by a positive number rather than
/// just excluding zero is load-bearing and was not obvious: `f32`
/// multiplication underflows, so two separately-positive factors can produce
/// a zero product. The smallest positive `f32` times [`f32::MIN_POSITIVE`]
/// is exactly `0.0` — which is the degenerate line height this whole guard
/// exists to make unreachable. A floor of one pixel against a floor of
/// [`MIN_LINE_HEIGHT_MULTIPLIER`] keeps the product at or above `0.1`, so
/// underflow is impossible rather than merely unlikely.
const MIN_FONT_SIZE: f32 = 1.0;

/// The largest line height multiplier
/// [`TextRenderer::set_line_height`] will accept.
///
/// Deliberately **not** the `MAX_LINE_HEIGHT` re-exported from
/// [`crate::render`], which is the minimap's row height in pixels. These are
/// different quantities that would share a name, and this module's is a
/// unitless multiplier.
const MAX_LINE_HEIGHT_MULTIPLIER: f32 = 64.0;

/// The smallest line height multiplier
/// [`TextRenderer::set_line_height`] will accept.
///
/// A tenth: a line box a tenth the height of its glyphs is already far past
/// unusable, so nothing legitimate lives below it, and it is the other half
/// of the underflow floor described on [`MIN_FONT_SIZE`].
const MIN_LINE_HEIGHT_MULTIPLIER: f32 = 0.1;

/// Whether `size` is a font size [`TextRenderer::set_font_size`] will accept.
///
/// A free function rather than an inline condition so it can be tested
/// without a GPU: constructing a [`TextRenderer`] needs a device, and a
/// predicate that can only be exercised through one is a predicate that goes
/// untested on every machine without an adapter.
///
/// The `is_finite` check is not redundant against the range comparisons.
/// `NaN` compares `false` to everything, so `NaN >= MIN` already fails —
/// but only by accident of IEEE semantics, and a later rewrite to
/// `!(size < MIN)` would flip that silently. It is stated so the intent
/// survives a refactor.
const fn font_size_is_usable(size: f32) -> bool {
    size.is_finite() && size >= MIN_FONT_SIZE && size <= MAX_FONT_SIZE
}

/// Whether `height` is a multiplier [`TextRenderer::set_line_height`] will
/// accept. Separated for the same reason as [`font_size_is_usable`].
const fn line_height_is_usable(height: f32) -> bool {
    height.is_finite()
        && height >= MIN_LINE_HEIGHT_MULTIPLIER
        && height <= MAX_LINE_HEIGHT_MULTIPLIER
}

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
/// let mut buffer = text_renderer.create_buffer(Some(800.0));
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
    /// Cached character width to avoid per-frame allocation and measurement.
    /// Invalidated when font size or family changes.
    cached_char_width: Option<f32>,
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
            cached_char_width: None, // Computed lazily on first call to char_width()
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
    ///
    /// # Performance
    ///
    /// The result is cached to avoid per-frame buffer allocations and text shaping.
    /// The cache is invalidated when font size or family changes.
    #[must_use]
    pub fn char_width(&mut self) -> f32 {
        // Return cached value if available
        if let Some(width) = self.cached_char_width {
            return width;
        }

        // Compute and cache the char width
        let width = self.measure_char_width();
        self.cached_char_width = Some(width);
        width
    }

    /// Internal method to measure character width. Called once and cached.
    fn measure_char_width(&mut self) -> f32 {
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
            let total_width: f32 = run.glyphs.iter().map(|glyph| glyph.w).sum();
            if !run.glyphs.is_empty() {
                return total_width / index_to_f32(run.glyphs.len());
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
    /// the specified width. Pass `None` for unconstrained width (no wrapping).
    ///
    /// # Arguments
    ///
    /// * `width` - The width of the text area in pixels, or `None` for no width constraint
    pub fn create_buffer(&mut self, width: Option<f32>) -> Buffer {
        let metrics = self.metrics();
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(&mut self.font_system, width, None);
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

    /// Sets styled spans on a buffer by diffing per line, preserving the
    /// shape caches of lines that did not change.
    ///
    /// [`Buffer::set_rich_text`] unconditionally resets every line's shape
    /// and layout caches, so retained shaping cannot go through it. This is
    /// the same construction routed through [`BufferLine::set_text`], which
    /// diffs text, ending and attribute list and resets only lines that
    /// actually differ — a one-character edit reshapes one line. It must
    /// stay byte-for-byte faithful to what `set_rich_text` builds, or the
    /// first diffed frame after a full rebuild would spuriously reshape
    /// everything: the same [`BidiParagraphs`] line split over the
    /// concatenated span text, `LineEnding::default()` on every line, a
    /// per-line [`AttrsList`] whose defaults carry no color with spans
    /// added only where they differ from those defaults, and one empty
    /// line when the text is empty.
    ///
    /// Returns how many lines were actually reshaped (differing lines plus
    /// lines appended to grow the buffer). Callers must still run
    /// [`Self::shape_buffer`] afterwards; unchanged lines answer it from
    /// their caches.
    pub fn set_rich_text_diffed<'a>(
        buffer: &mut Buffer,
        spans: impl Iterator<Item = (&'a str, Color)>,
    ) -> usize {
        let default_attrs = Attrs::new().family(Family::Monospace);

        // Concatenate the spans into one string with byte ranges, exactly as
        // `Buffer::set_rich_text` does before splitting into lines.
        let mut string = String::new();
        let mut span_ranges: Vec<(Color, std::ops::Range<usize>)> = Vec::new();
        for (text, color) in spans {
            let start = string.len();
            string.push_str(text);
            span_ranges.push((color, start..string.len()));
        }

        let string_start = string.as_ptr() as usize;
        let mut span_idx = 0_usize;
        let mut line_count = 0_usize;
        let mut reshaped = 0_usize;

        for line in BidiParagraphs::new(&string) {
            let line_start = line.as_ptr() as usize - string_start;
            let line_range = line_start..line_start + line.len();

            let mut attrs_list = AttrsList::new(&default_attrs);
            while let Some((color, span_range)) = span_ranges.get(span_idx) {
                // start..end is the intersection of this line and this span.
                let start = line_range.start.max(span_range.start);
                let end = line_range.end.min(span_range.end);
                if start < end {
                    let attrs = default_attrs.clone().color(Self::to_glyphon_color(*color));
                    // Only add attrs if they don't match the defaults — the
                    // rule `set_rich_text` applies.
                    if attrs != attrs_list.defaults() {
                        attrs_list
                            .add_span(start - line_range.start..end - line_range.start, &attrs);
                    }
                }
                // A span ending inside this line is followed by another span
                // on the same line; a span reaching the line's end carries
                // into the next line, whose empty intersection advances it.
                if span_range.end < line_range.end {
                    span_idx += 1;
                } else {
                    break;
                }
            }

            reshaped += usize::from(Self::write_line(
                buffer,
                line_count,
                line,
                LineEnding::default(),
                attrs_list,
            ));
            line_count += 1;
        }

        // Empty text still owns one empty line, as in `set_rich_text`.
        if line_count == 0 {
            reshaped += usize::from(Self::write_line(
                buffer,
                0,
                "",
                LineEnding::default(),
                AttrsList::new(&default_attrs),
            ));
            line_count = 1;
        }

        buffer.lines.truncate(line_count);
        reshaped
    }

    /// Sets uniformly colored text on a buffer by diffing per line — the
    /// [`Self::set_text`] counterpart of [`Self::set_rich_text_diffed`].
    ///
    /// Mirrors [`Buffer::set_text`]'s construction exactly: [`LineIter`]
    /// line splitting with each line's real ending, the given attributes
    /// (family and color) as every line's [`AttrsList`] defaults with no
    /// spans, and a trailing empty `LineEnding::None` line whenever the
    /// last split line ends with a terminator (or the text is empty).
    ///
    /// Returns how many lines were actually reshaped. Callers must still
    /// run [`Self::shape_buffer`] afterwards.
    pub fn set_text_diffed(buffer: &mut Buffer, text: &str, color: Color) -> usize {
        let attrs = Attrs::new()
            .family(Family::Monospace)
            .color(Self::to_glyphon_color(color));

        let mut line_count = 0_usize;
        let mut reshaped = 0_usize;
        let mut last_ending = LineEnding::default();
        for (range, ending) in LineIter::new(text) {
            reshaped += usize::from(Self::write_line(
                buffer,
                line_count,
                &text[range],
                ending,
                AttrsList::new(&attrs),
            ));
            line_count += 1;
            last_ending = ending;
        }

        // Ensure there is an ending line with no line ending, as in
        // `Buffer::set_text` (empty text hits this too: no lines were
        // produced and the default ending above is not `None`).
        if last_ending != LineEnding::None {
            reshaped += usize::from(Self::write_line(
                buffer,
                line_count,
                "",
                LineEnding::None,
                AttrsList::new(&attrs),
            ));
            line_count += 1;
        }

        buffer.lines.truncate(line_count);
        reshaped
    }

    /// Writes one line into `buffer.lines[index]` through the diffing
    /// [`BufferLine::set_text`], appending a fresh line when the buffer is
    /// shorter. Returns whether the line's shape cache was reset.
    fn write_line(
        buffer: &mut Buffer,
        index: usize,
        text: &str,
        ending: LineEnding,
        attrs_list: AttrsList,
    ) -> bool {
        if let Some(line) = buffer.lines.get_mut(index) {
            line.set_text(text, ending, attrs_list)
        } else {
            buffer
                .lines
                .push(BufferLine::new(text, ending, attrs_list, Shaping::Advanced));
            true
        }
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
                    run.glyphs.first().map_or(0, |g| g.start)
                };
                let run_end_col = if run.glyphs.is_empty() {
                    0
                } else {
                    run.glyphs.last().map_or(0, |g| g.end)
                };

                if column <= run_end_col || run.glyphs.is_empty() {
                    // Cursor is on this visual line
                    cursor_y = index_to_f32(visual_line) * line_height;

                    // Calculate X position within this run
                    let col_in_run = column.saturating_sub(run_start_col);
                    cursor_x = index_to_f32(col_in_run) * char_width;
                    found = true;
                }
                visual_line += 1;
            } else if run_line > line && !found {
                // We've passed the target line (empty line before this run)
                cursor_y = index_to_f32(visual_line) * line_height;
                cursor_x = index_to_f32(column) * char_width;
                found = true;
            }
        }

        // Handle case where cursor is past all content (empty trailing line)
        if !found {
            cursor_y = index_to_f32(visual_line) * line_height;
            cursor_x = index_to_f32(column) * char_width;
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
    /// recreated with the new size. Also invalidates the cached char width.
    ///
    /// Rejects a size outside `[MIN_FONT_SIZE, MAX_FONT_SIZE]` or a
    /// non-finite one, keeping the previous size and returning `false`.
    ///
    /// This is the guard for the whole estate, and it is here rather than in
    /// a face because of where the bad values come from: a font size is
    /// derived from a display scale factor, and both faces get that from
    /// outside. The web face multiplies a base size by `devicePixelRatio`,
    /// which is `0` in some headless and synthetic environments and
    /// `undefined` before layout; the desktop face multiplies by winit's
    /// scale factor. **A face-side guard protects only that face**, and the
    /// web face's has been one `??`-for-`||` refactor away from deleting
    /// itself, since `??` admits `0` and `NaN` where `||` does not.
    ///
    /// The consequence of accepting one is not a bad-looking frame. A zero
    /// font size makes [`Self::line_height`] zero, and viewport arithmetic
    /// divides by it — `scroll_y / 0.0` is `+∞`, which converts to a
    /// `usize::MAX` first-line index. See `render::units::pixel_to_index`,
    /// where that case is the one input the reachability argument does not
    /// otherwise cover.
    ///
    /// Bounded at both ends, and both ends are load-bearing. Above, because
    /// two separately-finite factors can multiply to infinity. Below by a
    /// *positive* floor rather than by zero, because two separately-positive
    /// factors can multiply to zero — `f32` underflows. Excluding zero alone
    /// would have left the degenerate line height reachable through the back
    /// door.
    pub fn set_font_size(&mut self, size: f32) -> bool {
        if !font_size_is_usable(size) {
            return false;
        }
        self.config.font_size = size;
        // Invalidate cached char width since it depends on font size
        self.cached_char_width = None;
        // Clear the glyph cache since glyphs will be at a different size
        self.atlas.trim();
        true
    }

    /// Updates the line height multiplier at runtime (T149).
    ///
    /// Rejects a multiplier outside
    /// `[MIN_LINE_HEIGHT_MULTIPLIER, MAX_LINE_HEIGHT_MULTIPLIER]` or a
    /// non-finite one, keeping the previous value and returning `false`.
    /// Guarded for the same reason as [`Self::set_font_size`], and it has to
    /// be: the quantity that matters downstream is the *product* of the two,
    /// so guarding only one of them leaves the degenerate case reachable
    /// through the other.
    ///
    /// # Arguments
    ///
    /// * `height` - New line height multiplier
    pub const fn set_line_height(&mut self, height: f32) -> bool {
        if !line_height_is_usable(height) {
            return false;
        }
        self.config.line_height = height;
        true
    }

    /// Updates the font family at runtime (T149).
    ///
    /// The family name should match a font already loaded in the system.
    /// Use `load_font` or `load_font_file` to add custom fonts first.
    /// Also invalidates the cached char width.
    ///
    /// # Arguments
    ///
    /// * `family` - Font family name (e.g., "`JetBrains` Mono")
    pub fn set_font_family(&mut self, family: impl Into<String>) {
        self.config.font_family = family.into();
        // Invalidate cached char width since it depends on font family
        self.cached_char_width = None;
        // Clear the glyph cache since we'll be using different glyphs
        self.atlas.trim();
    }

    /// Updates the configuration from a theme's typography settings (T149).
    ///
    /// This is a convenience method for updating all text rendering settings
    /// at once when the theme changes. Also invalidates the cached char width.
    ///
    /// # Arguments
    ///
    /// * `typography` - Typography settings from a theme
    pub fn apply_typography(&mut self, typography: &crate::theme::Typography) {
        self.config.font_size = typography.font_size;
        self.config.line_height = typography.line_height;
        self.config.font_family.clone_from(&typography.font_family);
        // Invalidate cached char width since font settings changed
        self.cached_char_width = None;
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

    /// The values a hostile or broken host can put into a scale factor.
    const DEGENERATE: [f32; 8] = [
        0.0,
        -0.0,
        -1.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MAX,
        f32::MIN,
    ];

    #[test]
    fn every_degenerate_font_size_is_refused() {
        for size in DEGENERATE {
            assert!(
                !font_size_is_usable(size),
                "{size} must not be accepted as a font size"
            );
        }
    }

    #[test]
    fn every_degenerate_line_height_is_refused() {
        for height in DEGENERATE {
            assert!(
                !line_height_is_usable(height),
                "{height} must not be accepted as a line height multiplier"
            );
        }
    }

    #[test]
    fn the_defaults_pass_their_own_guards() {
        // Otherwise a fresh renderer would be holding values it would refuse
        // to be set to, which is the sort of inconsistency that only shows up
        // the first time someone round-trips the config.
        assert!(font_size_is_usable(DEFAULT_FONT_SIZE));
        assert!(line_height_is_usable(DEFAULT_LINE_HEIGHT));
    }

    #[test]
    fn both_bounds_are_inclusive_and_the_next_float_beyond_each_is_not() {
        for (accepted, rejected) in [
            (MIN_FONT_SIZE, f32::from_bits(MIN_FONT_SIZE.to_bits() - 1)),
            (MAX_FONT_SIZE, f32::from_bits(MAX_FONT_SIZE.to_bits() + 1)),
        ] {
            assert!(font_size_is_usable(accepted), "{accepted} is the bound");
            assert!(
                !font_size_is_usable(rejected),
                "{rejected} is one float past the bound"
            );
        }
        for (accepted, rejected) in [
            (
                MIN_LINE_HEIGHT_MULTIPLIER,
                f32::from_bits(MIN_LINE_HEIGHT_MULTIPLIER.to_bits() - 1),
            ),
            (
                MAX_LINE_HEIGHT_MULTIPLIER,
                f32::from_bits(MAX_LINE_HEIGHT_MULTIPLIER.to_bits() + 1),
            ),
        ] {
            assert!(line_height_is_usable(accepted), "{accepted} is the bound");
            assert!(
                !line_height_is_usable(rejected),
                "{rejected} is one float past the bound"
            );
        }
    }

    /// The reason the lower bounds are positive numbers rather than zero.
    ///
    /// Both of these are finite and strictly positive, so a guard that only
    /// excluded zero would accept them — and their product is exactly `0.0`,
    /// reintroducing the degenerate line height through the back door.
    #[test]
    // Exact comparison against zero is the assertion, not an approximation
    // of one: the claim is that the product is *precisely* `0.0`, which is
    // what makes the division downstream infinite.
    #[allow(clippy::float_cmp)]
    fn two_positive_values_that_multiply_to_zero_are_both_refused() {
        // `f32::MIN_POSITIVE` is positive by definition; only the subnormal
        // needs stating.
        let smallest = f32::from_bits(1);
        assert!(smallest > 0.0 && smallest.is_finite());
        assert_eq!(
            smallest * f32::MIN_POSITIVE,
            0.0,
            "the premise of this test is that f32 multiplication underflows"
        );

        assert!(!font_size_is_usable(smallest));
        assert!(!line_height_is_usable(f32::MIN_POSITIVE));
        assert!(!font_size_is_usable(f32::MIN_POSITIVE));
        assert!(!line_height_is_usable(smallest));
    }

    /// The property every caller downstream actually depends on: viewport
    /// arithmetic divides by `line_height()`, so it must never be zero,
    /// negative, or infinite. This is the assertion that would have caught
    /// the `usize::MAX` first-line index at its source.
    ///
    /// Sweeps the extremes of both accepted ranges together, which is where
    /// overflow and underflow live — the interior cannot fail if the corners
    /// do not.
    #[test]
    fn any_accepted_pair_yields_a_finite_positive_line_height() {
        let sizes = [MIN_FONT_SIZE, 8.0, 14.0, MAX_FONT_SIZE];
        let multipliers = [
            MIN_LINE_HEIGHT_MULTIPLIER,
            1.0,
            DEFAULT_LINE_HEIGHT,
            MAX_LINE_HEIGHT_MULTIPLIER,
        ];
        for size in sizes {
            for multiplier in multipliers {
                assert!(
                    font_size_is_usable(size) && line_height_is_usable(multiplier),
                    "the fixture must only hold values the guards accept"
                );
                let line_height = size * multiplier;
                assert!(
                    line_height.is_finite(),
                    "{size} * {multiplier} overflowed to {line_height}"
                );
                assert!(
                    line_height > 0.0,
                    "{size} * {multiplier} underflowed to {line_height}"
                );
            }
        }
    }

    // Note: Actual rendering tests require GPU and are run as integration tests
}
