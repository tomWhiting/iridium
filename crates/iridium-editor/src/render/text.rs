//! Text rendering with glyphon.
//!
//! This module provides GPU-accelerated text rendering using glyphon
//! for glyph rasterization and cosmic-text for text shaping.

use glyphon::cosmic_text::{BidiParagraphs, LineEnding, LineIter};
use glyphon::{
    Attrs, AttrsList, Buffer, BufferLine, Cache, Color as GlyphonColor, Family, FontSystem,
    Metrics, Resolution, Shaping, Style, SwashCache, TextArea, TextAtlas, TextBounds,
    TextRenderer as GlyphonTextRenderer, Viewport, Weight, fontdb,
};
use wgpu::{Device, MultisampleState, Queue, TextureFormat};

use super::style::{RunSlant, RunStyle};
use super::units::index_to_f32;
use crate::editor::IridiumError;
use crate::theme::Color;

/// Default font size in pixels.
const DEFAULT_FONT_SIZE: f32 = 14.0;

/// Default line height multiplier.
const DEFAULT_LINE_HEIGHT: f32 = 1.4;

/// Whether these bytes hold at least one font face this build can read.
///
/// Answers the same question [`TextRenderer::load_font`] answers, without a
/// GPU, a device or a renderer — it loads into a throwaway database. That is
/// the whole point: the renderer's own answer is only reachable from a test
/// that can build a `wgpu` device, and those tests are `#[ignore]`d because
/// they need real hardware. ⭐ A font constant proven loadable only by a test
/// that never runs in the battery is not proven loadable at all.
///
/// Raw sfnt only — `.ttf`, `.otf`, `.ttc`. Compressed web formats such as
/// `.woff2` are **not** decoded and return `false` here.
#[must_use]
pub fn font_data_holds_a_face(data: &[u8]) -> bool {
    let mut database = fontdb::Database::new();
    database.load_font_data(data.to_vec());
    !database.is_empty()
}

/// Advance width to assume, as a fraction of the font size, when the real one
/// cannot be measured.
///
/// Two callers, and they are not the same situation. One is a shaped run that
/// produced no glyphs; the other is a font database with no faces in it at
/// all, which is every web build before its embedder supplies a face. `0.6` is
/// the usual advance ratio of a monospace face at a given size — near enough
/// that a first frame drawn before the font arrives has plausible geometry
/// rather than collapsing to zero-width columns.
const FALLBACK_CHAR_WIDTH_RATIO: f32 = 0.6;

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

/// The tab width a buffer gets when nothing has said otherwise.
///
/// The same number cosmic-text defaults to, restated here so the value this
/// crate ships is visible in this crate rather than inherited silently from a
/// dependency's internals.
pub const DEFAULT_TAB_WIDTH: u16 = 8;

/// `requested` as a width a buffer can actually use: zero becomes 1.
///
/// The single definition of that rule, because it has to hold in two places —
/// the renderer, which shapes the tabs, and the compositor, which puts the
/// same number in its cache key. Two copies could drift, and the failure would
/// be a cache key describing a width nothing was shaped at.
///
/// Zero is floored rather than passed through because cosmic-text ignores a
/// zero and keeps whatever width the buffer already had. Since the editing
/// side floors at 1 too (`input::keyboard::behaviors::tab_width`, so a corrupt
/// file cannot produce an empty indent level), passing zero through would mean
/// indenting by one column and rendering at eight — a fresh copy of the split
/// this whole field exists to close.
#[must_use]
pub const fn usable_tab_width(requested: u16) -> u16 {
    if requested == 0 { 1 } else { requested }
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
    /// How many character advances a literal tab occupies.
    ///
    /// Mirrors [`EditorConfig::tab_width`](crate::EditorConfig::tab_width) so
    /// one setting has one meaning: before this existed, `tab_width` governed
    /// indent and unindent while a tab on screen always measured eight, and a
    /// file full of tabs ignored the configuration entirely.
    ///
    /// Never zero. The editing side floors at 1
    /// (`input::keyboard::behaviors::tab_width`) so a corrupt file cannot
    /// produce an empty indent level, while cosmic-text *ignores* a zero and
    /// keeps whatever the buffer had. Left alone, `tab_width = 0` would
    /// therefore indent by one column and render at eight — a fresh copy of
    /// the divergence this field closes. Both sides floor at 1 so they agree.
    pub tab_width: u16,
}

impl Default for TextRenderConfig {
    fn default() -> Self {
        Self {
            font_size: DEFAULT_FONT_SIZE,
            line_height: DEFAULT_LINE_HEIGHT,
            font_family: "monospace".to_string(),
            tab_width: DEFAULT_TAB_WIDTH,
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
        // ⛔ SHAPING WITH AN EMPTY DATABASE PANICS. cosmic-text's shaper ends
        // at `no default font found`, and there is no fallback face to reach
        // for — a `wasm32` target has no system fonts at all, so this is the
        // *normal* state of a web build until the embedder supplies one.
        //
        // ⭐ The approximation at the bottom of this function was written for
        // "measurement fails" and could never run in this case: the panic
        // happens inside `set_text`, several lines above it. A fallback the
        // real failure mode cannot reach is not a fallback. This guard is what
        // gives it its population back.
        //
        // Deliberately not an error. A width is wanted by layout on every
        // frame, and a renderer with no font has nothing to draw anyway — the
        // honest signal belongs at `load_font`, which now reports, rather than
        // in a geometry query that cannot do anything with it.
        if self.font_system.db().is_empty() {
            return self.config.font_size * FALLBACK_CHAR_WIDTH_RATIO;
        }

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
        self.config.font_size * FALLBACK_CHAR_WIDTH_RATIO
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
        // The one place a rendered buffer is born, which is why the tab width
        // is applied here rather than at each call site: a buffer that missed
        // it would measure tabs at cosmic-text's default and nothing would say
        // so — the glyphs simply land in the wrong columns.
        buffer.set_tab_width(&mut self.font_system, self.config.tab_width);
        buffer
    }

    /// How many character advances a literal tab currently occupies.
    #[must_use]
    pub const fn tab_width(&self) -> u16 {
        self.config.tab_width
    }

    /// Sets the tab width for buffers created from here on, flooring a zero
    /// at 1.
    ///
    /// Zero is floored rather than passed through because cosmic-text *ignores*
    /// a zero and keeps whatever the buffer already had, which would leave
    /// rendering at eight columns while the editing side indented by one — see
    /// [`TextRenderConfig::tab_width`].
    ///
    /// This does not touch buffers that already exist: a `Buffer` carries the
    /// width it was constructed with. Re-measuring live text is the retained
    /// cache's job, and `ShapeKey::tab_width` is what makes it happen.
    pub const fn set_tab_width(&mut self, tab_width: u16) {
        self.config.tab_width = usable_tab_width(tab_width);
    }

    /// Sets the text content of a buffer.
    ///
    /// # Arguments
    ///
    /// * `buffer` - The buffer to update
    /// * `text` - The text content to set
    /// * `color` - The text color
    pub fn set_text(&mut self, buffer: &mut Buffer, text: &str, color: Color) {
        // Through the same construction the rich path uses, with the style
        // left plain. `Weight(400)` and `Style::Normal` are precisely what
        // `Attrs::new()` already defaults to, so this is byte-identical to
        // naming family and colour alone — and it stays identical if the
        // shared construction ever grows a field.
        let attrs = Self::span_attrs(RunStyle::plain(color));

        buffer.set_text(&mut self.font_system, text, &attrs, Shaping::Advanced, None);
    }

    /// The attributes every span in this crate is drawn with, from one place.
    ///
    /// ⚠️ **Both rich setters must build attributes identically**, and this is
    /// how that is guaranteed rather than asserted:
    /// [`Self::set_rich_text_diffed`] reshapes a line only when its attributes
    /// differ from the previous frame's, so two constructions that drifted by
    /// a single field would make the first diffed frame after any full rebuild
    /// reshape every line — a performance cliff whose only symptom is that the
    /// editor got slower.
    ///
    /// The family stays monospace for every run. Weight and slant are the
    /// variations a face may ask for; a *different family* is not, because the
    /// column geometry this crate lays out assumes one advance width and a
    /// proportional run would silently break the caret.
    fn span_attrs(style: RunStyle) -> Attrs<'static> {
        Attrs::new()
            .family(Family::Monospace)
            .color(Self::to_glyphon_color(style.color))
            .weight(Weight(style.weight.0))
            .style(match style.slant {
                RunSlant::Upright => Style::Normal,
                RunSlant::Italic => Style::Italic,
            })
    }

    /// The attributes a line's [`AttrsList`] defaults to: monospace, and
    /// nothing said about colour, weight or slant.
    ///
    /// Kept beside [`Self::span_attrs`] because the two are read together —
    /// a span is only added to a line when it differs from these.
    const fn default_attrs() -> Attrs<'static> {
        Attrs::new().family(Family::Monospace)
    }

    /// Sets the text content with multiple styled spans.
    ///
    /// # Arguments
    ///
    /// * `buffer` - The buffer to update
    /// * `spans` - Iterator of (text, style) pairs
    pub fn set_rich_text<'a>(
        &mut self,
        buffer: &mut Buffer,
        spans: impl Iterator<Item = (&'a str, RunStyle)>,
    ) {
        let rich_text: Vec<(&str, Attrs)> = spans
            .map(|(text, style)| (text, Self::span_attrs(style)))
            .collect();

        buffer.set_rich_text(
            &mut self.font_system,
            rich_text,
            &Self::default_attrs(),
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
        spans: impl Iterator<Item = (&'a str, RunStyle)>,
    ) -> usize {
        let default_attrs = Self::default_attrs();

        // Concatenate the spans into one string with byte ranges, exactly as
        // `Buffer::set_rich_text` does before splitting into lines.
        let mut string = String::new();
        let mut span_ranges: Vec<(RunStyle, std::ops::Range<usize>)> = Vec::new();
        for (text, style) in spans {
            let start = string.len();
            string.push_str(text);
            span_ranges.push((style, start..string.len()));
        }

        let string_start = string.as_ptr() as usize;
        let mut span_idx = 0_usize;
        let mut line_count = 0_usize;
        let mut reshaped = 0_usize;

        for line in BidiParagraphs::new(&string) {
            let line_start = line.as_ptr() as usize - string_start;
            let line_range = line_start..line_start + line.len();

            let mut attrs_list = AttrsList::new(&default_attrs);
            while let Some((style, span_range)) = span_ranges.get(span_idx) {
                // start..end is the intersection of this line and this span.
                let start = line_range.start.max(span_range.start);
                let end = line_range.end.min(span_range.end);
                if start < end {
                    let attrs = Self::span_attrs(*style);
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
    ///
    /// # Returns
    ///
    /// `true` when at least one face was added, `false` when the bytes held
    /// none the parser could read.
    ///
    /// ⛔ **`fontdb::load_font_data` returns `()` and reports nothing.** It
    /// takes raw sfnt — `.ttf`, `.otf`, `.ttc` — and *silently adds no faces*
    /// for anything else, `woff2` most of all, which is the format a web page
    /// actually serves. So the count is taken here, by measuring the database
    /// either side of the call, because there is no other way to find out.
    ///
    /// ⭐ The failure this makes visible was total and silent. Handing the web
    /// face a `woff2` added nothing, and the very next line measured a
    /// character — which shaped against an empty database and panicked "no
    /// default font found" inside cosmic-text. On wasm a panic is an
    /// `unreachable` trap, and a trap does **not** reject the awaited promise:
    /// the caller's `await` never settled, never threw, and the page sat there
    /// looking like it was still loading. A silent load failure two lines
    /// upstream became an editor that could not be distinguished from a slow
    /// one.
    #[must_use = "a font that failed to load leaves the renderer with nothing to shape"]
    pub fn load_font(&mut self, data: Vec<u8>) -> bool {
        let before = self.font_system.db().len();
        self.font_system.db_mut().load_font_data(data);
        let added = self.font_system.db().len() > before;
        if added {
            // A new face can change what `Family::Monospace` resolves to, and
            // the cached width belongs to whatever was resolvable before.
            self.cached_char_width = None;
        }
        added
    }

    /// Loads a font from a file path (T148).
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the font file
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, or if it could be read and
    /// held no face the parser recognised — a readable file that contributes
    /// no font is a failure to load, and reporting only the first of those two
    /// would leave the more confusing one silent.
    pub fn load_font_file(&mut self, path: &std::path::Path) -> Result<(), IridiumError> {
        let data = std::fs::read(path).map_err(|e| IridiumError::FontLoadFailed {
            message: format!("Failed to read font file {}: {e}", path.display()),
        })?;

        if self.load_font(data) {
            return Ok(());
        }
        Err(IridiumError::FontLoadFailed {
            message: format!(
                "{} contains no font face this build can read; raw sfnt \
                 (.ttf/.otf/.ttc) is required and compressed web formats such \
                 as .woff2 are not decoded",
                path.display()
            ),
        })
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
    use crate::render::style::RunWeight;

    #[test]
    #[allow(clippy::float_cmp)]
    fn text_render_config_default() {
        let config = TextRenderConfig::default();
        assert_eq!(config.font_size, DEFAULT_FONT_SIZE);
        assert_eq!(config.line_height, DEFAULT_LINE_HEIGHT);
        assert_eq!(config.font_family, "monospace");
    }

    /// ⭐ The identity the retained-shaping cache rests on.
    ///
    /// Before styles existed, a span's attributes were monospace plus a
    /// colour. `set_rich_text_diffed` reshapes a line only when its
    /// attributes differ from the previous frame's, so if a *plain* run now
    /// produced anything else — a weight, a slant, anything — the first
    /// diffed frame after every full rebuild would reshape every line in the
    /// viewport, and the only symptom would be that the editor got slower.
    ///
    /// Asserted against a hand-built `Attrs` rather than against
    /// `span_attrs`'s own output, because comparing a function to itself
    /// proves nothing about what it produces.
    #[test]
    fn a_plain_run_still_asks_for_exactly_monospace_and_a_colour() {
        let colour = Color::new(0.2, 0.4, 0.6, 1.0);
        let before = Attrs::new()
            .family(Family::Monospace)
            .color(TextRenderer::to_glyphon_color(colour));

        assert_eq!(
            TextRenderer::span_attrs(RunStyle::plain(colour)),
            before,
            "a plain run's attributes drifted from the colour-only ones"
        );
    }

    /// And the other half: a style that asks for something must actually
    /// reach the attributes. A `RunStyle` the renderer silently dropped
    /// would be a decorative API — the whole defect this type exists to
    /// avoid — and it would look exactly like the assertion above passing.
    #[test]
    fn weight_and_slant_reach_the_attributes() {
        let colour = Color::new(0.2, 0.4, 0.6, 1.0);
        let plain = TextRenderer::span_attrs(RunStyle::plain(colour));

        let bold = TextRenderer::span_attrs(RunStyle::plain(colour).bold());
        assert_ne!(bold, plain, "bold was dropped on the way to the shaper");
        assert_eq!(bold.weight, Weight(RunWeight::BOLD.0));

        let italic = TextRenderer::span_attrs(RunStyle::plain(colour).italic());
        assert_ne!(italic, plain, "italic was dropped on the way to the shaper");
        assert_eq!(italic.style, Style::Italic);

        // And the two are independent: a bold run must not also lean.
        assert_eq!(bold.style, Style::Normal);
        assert_eq!(italic.weight, Weight(RunWeight::NORMAL.0));
    }

    /// An arbitrary weight survives the trip rather than being rounded to
    /// one of the two named ones.
    #[test]
    fn a_weight_between_the_named_ones_reaches_the_shaper_intact() {
        let colour = Color::new(0.2, 0.4, 0.6, 1.0);
        let attrs = TextRenderer::span_attrs(RunStyle::plain(colour).with_weight(RunWeight(600)));
        assert_eq!(attrs.weight, Weight(600));
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

    /// The bytes a web page actually serves are `woff2`, and this build cannot
    /// read them.
    ///
    /// ⭐ The defect this pins was total and silent, and it cost a consumer a
    /// day. `fontdb::load_font_data` returns `()`: handed a `woff2` it adds no
    /// face and says nothing, so the load "succeeded". The next line measured a
    /// character, which shaped against an empty database and panicked inside
    /// cosmic-text with "no default font found" — and on wasm a panic is an
    /// `unreachable` trap, which does **not** reject the promise the caller is
    /// awaiting. The `await` never settled and never threw.
    ///
    /// ⚠️ The header alone is what makes this a `woff2` rather than a `ttf`:
    /// `wOF2` where sfnt would carry `0x00010000` or `OTTO`. The rest is
    /// plausible padding, so this exercises the parser's verdict rather than a
    /// length check.
    #[test]
    fn woff2_bytes_hold_no_face_this_build_can_read() {
        let mut woff2 = b"wOF2".to_vec();
        woff2.extend_from_slice(&[0x00; 64]);
        assert!(
            !font_data_holds_a_face(&woff2),
            "woff2 must be reported as unreadable rather than silently loading \
             nothing"
        );
    }

    /// The other half of the claim: the check is not simply always `false`.
    ///
    /// ⭐ Without this, the test above would pass against a function that
    /// rejected everything, including the fonts the faces actually ship —
    /// which would turn every build into the failure it is meant to catch.
    #[test]
    fn a_real_sfnt_face_is_recognised() {
        static FONT: &[u8] =
            include_bytes!("../../../../examples/web/public/fonts/JetBrainsMono-Regular.ttf");
        assert!(
            font_data_holds_a_face(FONT),
            "the vendored JetBrains Mono .ttf must be recognised"
        );
    }

    /// Empty and truncated input is a verdict, not a crash.
    #[test]
    fn nothing_and_almost_nothing_hold_no_face() {
        assert!(!font_data_holds_a_face(&[]));
        assert!(!font_data_holds_a_face(&[0x00, 0x01]));
    }

    /// ⛔ **The reason [`TextRenderer::measure_char_width`]'s empty-database
    /// guard exists**, pinned directly rather than asserted about.
    ///
    /// This is the discrimination proof for the whole fix: it reproduces the
    /// exact failure — shaping with no faces available — in isolation, with no
    /// GPU and no renderer, and shows that cosmic-text's answer is a **panic**
    /// rather than an empty layout. That is what made the fallback below the
    /// shaping call unreachable, and on wasm it is what became a trap that
    /// never rejected the caller's promise.
    ///
    /// If a future cosmic-text returns an empty run instead of panicking, this
    /// test fails — and that is the correct outcome, because the guard's
    /// justification would have expired and the comment above it would have
    /// become false. A guard whose reason has quietly stopped being true is
    /// exactly the kind of thing nobody re-checks.
    #[test]
    #[should_panic(expected = "no default font found")]
    fn shaping_with_an_empty_font_database_panics() {
        let mut font_system =
            FontSystem::new_with_locale_and_db("en-US".to_owned(), fontdb::Database::new());
        let metrics = Metrics::relative(DEFAULT_FONT_SIZE, DEFAULT_LINE_HEIGHT);
        let mut buffer = Buffer::new(&mut font_system, metrics);
        let attrs = Attrs::new().family(Family::Monospace);
        buffer.set_text(&mut font_system, "MM", &attrs, Shaping::Advanced, None);
    }

    // Note: Actual rendering tests require GPU and are run as integration tests
}
