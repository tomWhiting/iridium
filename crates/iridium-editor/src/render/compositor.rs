//! Face-neutral frame composition: the one place editor state becomes pixels.
//!
//! Every GPU face used to be at risk of the same fork: the browser bindings
//! grew a complete per-frame assembly — visible-content extraction, wrap
//! readback, gutter text, selection and caret quads, the render pass itself —
//! and a native shell would have had to rewrite all of it against the same
//! renderers. [`FrameCompositor`] is that assembly moved to the kernel, so a
//! face owns only what is genuinely its own: a surface, an input source, and
//! (optionally) a syntax resolver.
//!
//! # What a face keeps
//!
//! Three things deliberately stay on the face side of the seam:
//!
//! - **The surface.** [`FrameCompositor::compose`] takes a bare
//!   [`wgpu::TextureView`] plus device, queue and pixel dimensions
//!   ([`FrameTarget`]); acquiring and presenting the frame is the face's
//!   business, because that is where canvas and native window genuinely
//!   differ.
//! - **The scroll offset.** `scroll_y` is an input parameter. The face decides
//!   where the viewport is; the compositor only answers what that looks like
//!   ([`FrameCompositor::max_scroll_y`] and
//!   [`FrameCompositor::cursor_anchor_y`] exist so the face can decide well).
//! - **Syntax spans.** A face with a real highlighter (the web face runs
//!   tree-sitter in a worker) supplies its spans through [`HighlightSource`].
//!   When the face reports a language but has no spans this frame, the
//!   built-in keyword highlighter bridges until they arrive; when it reports
//!   no language, there is nothing to bridge to and the content renders in
//!   the plain theme foreground.
//!
//! # The caches are an interface, not a data structure
//!
//! Wrapping currently lives inside cosmic-text: the compositor hands the text
//! buffer a width and reads the resulting layout runs back into
//! `cached_visual_line_map`. Hit-testing ([`FrameCompositor::pixel_to_position`]
//! and its inverse) and scroll clamping run *between* frames, off that
//! readback. They are exposed as methods rather than fields so the planned
//! kernel-side wrap model (`docs/SOFT-WRAP-DESIGN.md`) can replace the
//! implementation without touching any face.

use std::collections::HashMap;
use std::fmt::Write as _;

use glyphon::{Buffer, TextArea, TextBounds};
use web_time::Instant;
use wgpu::{
    CommandEncoderDescriptor, Device, LoadOp, Operations, Queue, RenderPassColorAttachment,
    RenderPassDescriptor, StoreOp, TextureFormat, TextureView,
};

use super::cursor::CursorRenderer;
use super::gutter::GutterRenderer;
use super::quad::{Quad, QuadRenderer};
use super::simple_highlight::SimpleHighlighter;
use super::text::TextRenderer;
use super::units::{dimension_to_bound, index_to_f32, pixel_to_bound, pixel_to_index, u32_to_f32};
use super::viewport::ViewportConfig;
use crate::editor::{Editor, FoldState, IridiumError};
use crate::theme::{Color, Theme};

/// Maximum expected lines in viewport (for pre-allocation sizing).
/// Typical: 50 lines visible + 20 overscan = 70 lines.
const MAX_VIEWPORT_LINES: usize = 128;

/// Maximum expected selection quads per frame.
/// Typical: 1 quad per selected line, rarely more than viewport.
const MAX_SELECTION_QUADS: usize = 128;

/// The wgpu objects one frame is composed onto.
///
/// A frame needs exactly these five values from a surface, whatever the
/// surface is — a browser canvas today, a native window tomorrow. Passing
/// them as plain values (rather than as a surface trait) is deliberate: a
/// generic surface abstraction here would buy nothing and would fight
/// `wasm-bindgen` on the web face.
#[derive(Clone, Copy)]
pub struct FrameTarget<'a> {
    /// The texture view the render pass draws into.
    pub view: &'a TextureView,
    /// The device that records the frame's command encoder.
    pub device: &'a Device,
    /// The queue the frame's commands and buffer writes are submitted to.
    pub queue: &'a Queue,
    /// Surface width in physical pixels.
    pub width: u32,
    /// Surface height in physical pixels.
    pub height: u32,
}

impl std::fmt::Debug for FrameTarget<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameTarget")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

/// Everything a face's highlight resolver may need for one frame.
///
/// The visible content string is built by the compositor (it owns folding and
/// viewport virtualization), but the *colors* may belong to the face — the web
/// face resolves them from tree-sitter spans it received asynchronously. This
/// context hands the resolver the content and the layout facts it needs to
/// query by byte range, plus the face-provided theme map so span colors and
/// fallback foreground come from one source.
#[derive(Debug)]
pub struct HighlightContext<'a> {
    /// The visible content exactly as it will be shaped, folds already
    /// collapsed to their `" ... }"` placeholder form.
    pub content: &'a str,
    /// Document byte offset where `content` begins, for mapping absolute span
    /// offsets into `content`-relative slices.
    pub content_start_byte: usize,
    /// Width in pixels available to the content column — the wrap width.
    pub content_width: f32,
    /// Line height in pixels.
    pub line_height: f32,
    /// Overscan configuration for viewport-scoped span queries.
    pub viewport_config: &'a ViewportConfig,
    /// Face-provided capture-name → color map (hierarchical lookup is the
    /// face's business; the map is exposed raw).
    pub syntax_theme: &'a HashMap<String, Color>,
    /// The theme's default foreground, for gaps between highlighted spans.
    pub foreground: Color,
}

/// The seam through which a face supplies resolved syntax colors.
///
/// [`FrameCompositor::compose`] calls this once per frame, only when syntax
/// highlighting is enabled. Returning `Some` paints the given spans (slices
/// of [`HighlightContext::content`] paired with colors, in order, gaps
/// included); returning `None` says "I have nothing this frame", and what
/// the compositor does next is [`Self::language_active`]'s answer: with a
/// language set the built-in keyword highlighter bridges the span-less
/// frame, with none the content renders plain — the same decision the web
/// face always made, split along ownership: the tree-sitter paths live with
/// the face that owns the spans, the fallback paths live with the
/// compositor that owns the highlighter.
pub trait HighlightSource {
    /// Resolves this frame's highlight spans against the visible content, or
    /// `None` when it has nothing this frame — see [`Self::language_active`]
    /// for what a `None` renders as.
    fn resolve<'a>(&mut self, context: &HighlightContext<'a>) -> Option<Vec<(&'a str, Color)>>;

    /// Whether a language is set for the document this source highlights.
    ///
    /// This is what splits the two meanings of a `None` from
    /// [`Self::resolve`]. With a language set, `None` is a bridge — spans
    /// are owed but not available this frame — and the compositor colors
    /// with its built-in keyword fallback until they arrive. With no
    /// language set there is nothing to bridge to, and the content renders
    /// in the plain theme foreground: a file without a grammar must never
    /// wear another language's keyword colors.
    ///
    /// The compositor keys its retained shapes on this value directly, so a
    /// face whose language is set or cleared at runtime need not move
    /// [`Self::generation`] for this flip alone. A face whose language
    /// knowledge lives outside the kernel answers for its own notion of
    /// "set" — the web face's grammar belongs to its host's worker, and it
    /// answers `true`, leaving "this document has no language" to the
    /// host's [`FrameCompositor::set_syntax_enabled`] channel.
    fn language_active(&self) -> bool;

    /// The face's highlight generation: a value that MUST change whenever a
    /// subsequent [`Self::resolve`] could return different runs for an
    /// identical context.
    ///
    /// The compositor retains its shaped buffers across frames and skips
    /// `resolve` entirely while this value (and every other shaping input)
    /// is unchanged — a face that mutates its spans, replaces them, clears
    /// them, or changes anything else its resolution reads (span sets,
    /// span-shifting after edits, resolver-side theme colors) without
    /// moving this shows stale colors. That is the contract, including
    /// span-*clearing* paths: removing a language or emptying the span set
    /// changes the answer just as surely as new spans do, and must move
    /// the generation. A source whose answer can never change (such as a
    /// constant fallback) returns a constant.
    fn generation(&self) -> u64;
}

/// Per-frame layout values shared by the quad-building passes.
///
/// These are all derived at the top of [`FrameCompositor::compose`] and read
/// everywhere below it; carrying them as one value keeps the pass methods'
/// signatures honest about what they consume.
#[derive(Debug, Clone, Copy)]
struct FrameMetrics {
    /// Line height in pixels.
    line_height: f32,
    /// Character advance width in pixels.
    char_width: f32,
    /// The editor's fixed content padding (10px).
    padding: f32,
    /// Left edge of the content column: gutter width plus padding.
    content_offset_x: f32,
    /// Pixel offset of the viewport buffer's first line in document space.
    virtual_scroll_offset: f32,
    /// First document line included in the viewport buffer.
    viewport_start: usize,
    /// Visual index (fold-aware) of `viewport_start`.
    viewport_start_visual: usize,
    /// The face's vertical scroll offset in pixels.
    scroll_y: f32,
    /// Surface width in pixels.
    surface_width: f32,
    /// Surface height in pixels.
    surface_height: f32,
}

/// Every input the shaped viewport buffers depend on, as one comparable
/// value — the retained-shaping cache key.
///
/// The full input inventory lives in `docs/design/RETAINED-SHAPING-MAP.md`
/// §2; every row is a field here or is subsumed by one (the viewport range
/// subsumes the highlight resolver's own viewport dependence; the content
/// width folds in surface width, gutter enablement, custom gutter text
/// width, character width and digit rollover). `f32` inputs are keyed by
/// bit pattern — bit-equality is the honest cache question ("did the input
/// change"), not approximate equality. The face's scroll remainder within
/// one line is deliberately absent: it is applied at the text area's top
/// edge, so sub-line scrolls are cache hits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShapeKey {
    /// Document content (`Document::revision`, moved by every text
    /// mutation and never by cursor motion).
    document_revision: u64,
    /// First document line included in the viewport buffer.
    viewport_start: usize,
    /// One past the last document line included in the viewport buffer.
    viewport_end: usize,
    /// Wrap width in pixels, as `f32::to_bits`.
    content_width: u32,
    /// Font size in pixels, as `f32::to_bits`.
    font_size: u32,
    /// Line height multiplier, as `f32::to_bits`.
    line_height_factor: u32,
    /// Loaded-font-data generation ([`FrameCompositor::load_font`]).
    font_generation: u64,
    /// Theme generation ([`FrameCompositor::set_theme`]).
    theme_generation: u64,
    /// Whether syntax highlighting is enabled.
    syntax_enabled: bool,
    /// The face's [`HighlightSource::language_active`] answer. It selects
    /// between the keyword-fallback and plain arms when the source resolves
    /// `None`, so a language set or unset at runtime reshapes even when no
    /// other input moved.
    language_active: bool,
    /// The face's [`HighlightSource::generation`].
    highlight_generation: u64,
    /// Capture-name map generation ([`FrameCompositor::syntax_theme_mut`]).
    syntax_theme_generation: u64,
    /// The [`FoldState::generation`] — folds never move the document
    /// revision, so they need their own key member.
    fold_generation: u64,
    /// Custom gutter text / gutter enablement generation
    /// ([`FrameCompositor::set_custom_gutter_lines`],
    /// [`FrameCompositor::set_gutter_enabled`]).
    gutter_text_generation: u64,
}

impl ShapeKey {
    /// Whether a miss against `previous` may be served by per-line diffing
    /// (stage 2a): only the document content and/or the highlight answer
    /// moved, while every input the buffer's geometry, metrics and
    /// attribute construction depend on — including the viewport range,
    /// whose change re-maps every buffer line to a different document
    /// line — is unchanged.
    const fn permits_line_diff(&self, previous: &Self) -> bool {
        // Destructured so a new key member cannot be forgotten here: adding
        // a field forces this pattern to name it.
        let Self {
            document_revision: _,
            highlight_generation: _,
            viewport_start,
            viewport_end,
            content_width,
            font_size,
            line_height_factor,
            font_generation,
            theme_generation,
            syntax_enabled,
            language_active,
            syntax_theme_generation,
            fold_generation,
            gutter_text_generation,
        } = *self;
        viewport_start == previous.viewport_start
            && viewport_end == previous.viewport_end
            && content_width == previous.content_width
            && font_size == previous.font_size
            && line_height_factor == previous.line_height_factor
            && font_generation == previous.font_generation
            && theme_generation == previous.theme_generation
            && syntax_enabled == previous.syntax_enabled
            && language_active == previous.language_active
            && syntax_theme_generation == previous.syntax_theme_generation
            && fold_generation == previous.fold_generation
            && gutter_text_generation == previous.gutter_text_generation
    }
}

/// The shaped buffers from the last composed frame, with the exact inputs
/// they were shaped under.
struct RetainedShape {
    /// The inputs the buffers were built from.
    key: ShapeKey,
    /// The main content buffer, shaped.
    buffer: Buffer,
    /// The line-number buffer, shaped; `None` when the gutter is off.
    gutter: Option<Buffer>,
    /// The gutter width (as `f32::to_bits`) the gutter buffer was created
    /// with. The key's `content_width` almost always pins this too, but a
    /// simultaneous surface-width and gutter-width change could cancel out
    /// in the subtraction — the gutter buffer's own width is compared
    /// directly rather than inferred.
    gutter_width_bits: u32,
}

/// The per-frame geometry a retained-shape rebuild works against — the
/// values [`FrameCompositor::compose`] derives before consulting the key.
struct RebuildGeometry {
    /// First document line in the viewport buffer.
    viewport_start: usize,
    /// One past the last document line in the viewport buffer.
    viewport_end: usize,
    /// Document byte offset where the visible content begins.
    viewport_start_byte: usize,
    /// Wrap width in pixels for the content buffer.
    content_width: f32,
    /// Gutter width in pixels for the gutter buffer.
    gutter_width: f32,
    /// Left edge of the content column.
    content_offset_x: f32,
    /// Line height in pixels.
    line_height: f32,
    /// Total document line count.
    line_count: usize,
}

/// Composes editor state into frames, and answers layout questions between
/// them.
///
/// Owns the renderers, the pre-allocated per-frame buffers, the layout caches
/// hit-testing depends on, and the presentation inputs (theme, per-line
/// backgrounds, gutter change bars, custom gutter text, blame text) a host
/// pushes in between frames. One instance belongs to one surface: the
/// renderers' viewport uniforms track that surface's dimensions.
pub struct FrameCompositor {
    /// Text renderer shared by content, gutter and blame buffers.
    text_renderer: TextRenderer,
    /// Quad renderer for background elements (gutter, selection highlights).
    background_quad_renderer: QuadRenderer,
    /// Quad renderer for foreground elements (cursor) — separate buffer to
    /// avoid GPU conflicts: two `queue.write_buffer` calls to one buffer
    /// within a frame leave only the last write visible.
    cursor_quad_renderer: QuadRenderer,
    /// Caret blink state, shared by every caret on purpose.
    cursor_renderer: CursorRenderer,
    /// Gutter width and digit-column calculations.
    gutter_renderer: GutterRenderer,
    /// Built-in keyword highlighter, the bridge for a language-active face
    /// whose [`HighlightSource`] has no spans this frame. It never touches a
    /// document without a language — that renders plain.
    highlighter: SimpleHighlighter,
    /// The active theme.
    theme: Theme,
    /// Whether syntax highlighting is enabled.
    syntax_enabled: bool,
    /// Whether the gutter (line numbers) is enabled.
    gutter_enabled: bool,
    /// Cached character width (measured from actual font metrics).
    cached_char_width: f32,
    /// Viewport configuration for the overscan buffer.
    viewport_config: ViewportConfig,
    /// Cached viewport dimensions to avoid redundant GPU uniform updates.
    cached_viewport_width: u32,
    /// See [`Self::cached_viewport_width`].
    cached_viewport_height: u32,

    // =========================================================================
    // Retained shaping: the shaped buffers survive across frames
    // =========================================================================
    /// The shaped viewport buffers from the last frame, with the exact
    /// inputs they were shaped under; `None` before the first frame. A
    /// frame whose [`ShapeKey`] matches skips content extraction, highlight
    /// resolution and all shaping.
    retained: Option<RetainedShape>,
    /// Bumped by [`Self::load_font`]: new font data can change how
    /// `Family::Monospace` resolves, which changes every shaped glyph.
    font_generation: u64,
    /// Bumped by [`Self::set_theme`] (and through it
    /// [`Self::set_dark_theme`]): the theme feeds text colors and the
    /// fallback highlighter's palette.
    theme_generation: u64,
    /// Bumped by every mutable borrow of [`Self::syntax_theme_mut`]: the
    /// raw accessor defeats change tracking, so the borrow itself is the
    /// change signal (over-invalidating only when the host actually calls
    /// it, which coincides with real changes).
    syntax_theme_generation: u64,
    /// Bumped by [`Self::set_custom_gutter_lines`] and
    /// [`Self::set_gutter_enabled`]: custom gutter text and gutter
    /// enablement change both the gutter buffer's text and the content
    /// column's width.
    gutter_text_generation: u64,
    /// How many times compose has run the rebuild path — the observable
    /// that makes every cache claim testable without reading pixels.
    shape_rebuilds: u64,
    /// How many buffer lines the per-line diffing rebuild path has
    /// actually reshaped, across the content and gutter buffers.
    lines_reshaped: u64,

    // =========================================================================
    // Pre-allocated buffers for compose() — eliminates per-frame allocations
    // =========================================================================
    /// Pre-allocated buffer for the visible content string.
    /// Capacity: ~10KB (typical viewport worth of text).
    cpu_visible_content: String,
    /// Pre-allocated buffer for document-to-visual line mapping.
    /// Capacity: document line count (grows as needed).
    cpu_doc_to_visual: Vec<Option<usize>>,
    /// Pre-allocated buffer for visible document line indices.
    /// Capacity: [`MAX_VIEWPORT_LINES`].
    cpu_visible_doc_lines: Vec<usize>,
    /// Pre-allocated buffer for the line numbers string.
    /// Capacity: ~2KB (typical viewport worth of line numbers).
    cpu_line_numbers: String,
    /// Pre-allocated buffer for gutter background quads.
    /// Capacity: 1-2 quads typically.
    cpu_gutter_quads: Vec<Quad>,
    /// Pre-allocated buffer for selection highlight quads.
    /// Capacity: [`MAX_SELECTION_QUADS`].
    cpu_selection_quads: Vec<Quad>,
    /// Pre-allocated buffer for cursor quads.
    /// Capacity: 1-2 quads typically.
    cpu_cursor_quads: Vec<Quad>,
    /// Pre-allocated buffer for combined background quads.
    /// Capacity: gutter + selection quads.
    cpu_background_quads: Vec<Quad>,
    /// Pre-allocated buffer for line background quads.
    cpu_line_bg_quads: Vec<Quad>,
    /// Pre-allocated buffer for gutter change bar quads.
    cpu_gutter_change_quads: Vec<Quad>,
    /// Pre-allocated buffer for blame text rendering.
    cpu_blame_content: String,

    // =========================================================================
    // Cached layout info for pixel-to-position mapping with word wrap
    // =========================================================================
    /// Cached visual line mapping built during compose.
    /// Each entry maps a visual line to (`buffer_line_index`, `start_column_in_line`).
    cached_visual_line_map: Vec<(usize, usize)>,
    /// The `viewport_start` value when `cached_visual_line_map` was built.
    cached_map_viewport_start: usize,
    /// Content offset X when the map was built.
    cached_content_offset_x: f32,

    // =========================================================================
    // Cached scroll data for word-wrap-aware scrolling
    // =========================================================================
    /// Total visual lines including wrapped sub-lines, updated each frame.
    cached_total_visual_lines: usize,
    /// Absolute cursor Y position (document space) from the last frame.
    cached_cursor_abs_y: f32,
    /// The cursor doc line when `cached_cursor_abs_y` was computed.
    cached_cursor_doc_line: usize,

    // =========================================================================
    // Presentation inputs pushed by the host between frames
    // =========================================================================
    /// Map of capture name → color, consumed by the face's highlight resolver
    /// through [`HighlightContext::syntax_theme`].
    syntax_theme: HashMap<String, Color>,
    /// Per-line background colors (`doc_line` → color). Used for diff
    /// highlighting.
    line_backgrounds: HashMap<usize, Color>,
    /// Per-line gutter change bar colors (`doc_line` → color). Used for
    /// change indicators.
    gutter_changes: HashMap<usize, Color>,
    /// Custom gutter text lines (replaces auto line numbers when `Some`).
    custom_gutter_lines: Option<Vec<String>>,
    /// Per-line blame text (`doc_line` → formatted blame string). Shown at
    /// the end of the cursor line.
    blame_data: HashMap<usize, String>,
}

impl std::fmt::Debug for FrameCompositor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameCompositor")
            .field("syntax_enabled", &self.syntax_enabled)
            .field("gutter_enabled", &self.gutter_enabled)
            .field("cached_viewport_width", &self.cached_viewport_width)
            .field("cached_viewport_height", &self.cached_viewport_height)
            .finish_non_exhaustive()
    }
}

impl FrameCompositor {
    /// Creates a compositor for a surface with the given format and initial
    /// pixel dimensions.
    ///
    /// The dimensions seed both the quad renderers' viewport uniforms and the
    /// dirty check that keeps [`Self::compose`] from re-uploading them every
    /// frame.
    ///
    /// # Errors
    ///
    /// Returns an error if the text renderer cannot be created.
    pub fn new(
        device: &Device,
        queue: &Queue,
        format: TextureFormat,
        width: u32,
        height: u32,
    ) -> Result<Self, IridiumError> {
        let mut text_renderer = TextRenderer::new(device, queue, format)?;
        // Seed the glyph renderer's viewport uniform too: glyphon's starts
        // at 0×0, which culls every glyph in `prepare`. The faces masked
        // this by calling `resize` on their initial surface configure;
        // a headless consumer composing at the creation dimensions never
        // resizes and rendered no text at all.
        text_renderer.update_viewport(queue, width, height);

        // Separate quad renderers with separate vertex buffers: multiple
        // queue.write_buffer calls to the same buffer within a frame cause
        // corruption, so background and cursor quads never share one.
        let mut background_quad_renderer = QuadRenderer::new(device, format);
        background_quad_renderer.update_viewport(queue, width, height);
        let mut cursor_quad_renderer = QuadRenderer::new(device, format);
        cursor_quad_renderer.update_viewport(queue, width, height);

        Ok(Self {
            text_renderer,
            background_quad_renderer,
            cursor_quad_renderer,
            cursor_renderer: CursorRenderer::default(),
            gutter_renderer: GutterRenderer::new(),
            highlighter: SimpleHighlighter::new(),
            theme: Theme::dark(),
            syntax_enabled: true,
            gutter_enabled: true,
            cached_char_width: 14.0 * 0.6, // Default until a font is loaded
            viewport_config: ViewportConfig::default(),
            cached_viewport_width: width,
            cached_viewport_height: height,
            // Retained shaping: nothing is retained before the first frame
            retained: None,
            font_generation: 0,
            theme_generation: 0,
            syntax_theme_generation: 0,
            gutter_text_generation: 0,
            shape_rebuilds: 0,
            lines_reshaped: 0,
            // Pre-allocated buffers to avoid per-frame allocations (120fps target)
            cpu_visible_content: String::with_capacity(10 * 1024), // 10KB typical viewport
            cpu_doc_to_visual: Vec::with_capacity(1024),           // 1K lines initial
            cpu_visible_doc_lines: Vec::with_capacity(MAX_VIEWPORT_LINES),
            cpu_line_numbers: String::with_capacity(2 * 1024), // 2KB line numbers
            cpu_gutter_quads: Vec::with_capacity(2),
            cpu_selection_quads: Vec::with_capacity(MAX_SELECTION_QUADS),
            cpu_cursor_quads: Vec::with_capacity(2),
            cpu_background_quads: Vec::with_capacity(MAX_SELECTION_QUADS + 2),
            cpu_line_bg_quads: Vec::with_capacity(MAX_VIEWPORT_LINES),
            cpu_gutter_change_quads: Vec::with_capacity(MAX_VIEWPORT_LINES),
            cpu_blame_content: String::with_capacity(256),
            // Cached layout info for pixel-to-position mapping with word wrap
            cached_visual_line_map: Vec::with_capacity(MAX_VIEWPORT_LINES),
            cached_map_viewport_start: 0,
            cached_content_offset_x: 0.0,
            // Cached scroll data for word-wrap-aware scrolling
            cached_total_visual_lines: 0,
            cached_cursor_abs_y: 0.0,
            cached_cursor_doc_line: 0,
            // Presentation inputs
            syntax_theme: HashMap::new(),
            line_backgrounds: HashMap::new(),
            gutter_changes: HashMap::new(),
            custom_gutter_lines: None,
            blame_data: HashMap::new(),
        })
    }

    /// Composes one frame from editor state onto the target.
    ///
    /// This is the whole per-frame pipeline: viewport virtualization and
    /// fold-aware content extraction, shaping (which is where wrapping
    /// happens), the wrap readback that feeds hit-testing, gutter text,
    /// background/selection/caret quads, and the render pass itself.
    /// `scroll_y` is the face's vertical offset in physical pixels;
    /// `highlights` is consulted only when syntax highlighting is enabled.
    ///
    /// The shaped buffers are retained across frames behind a [`ShapeKey`]
    /// of every input they depend on: a frame whose key matches the last
    /// one skips content extraction, highlight resolution and all shaping,
    /// and pays only what a frame legitimately owes — cursor walk, quads,
    /// glyph preparation and the render pass. Retention is an optimization
    /// and nothing else: hit or miss, the composed output is identical.
    ///
    /// # Errors
    ///
    /// Returns an error when glyph preparation or text rendering fails.
    pub fn compose(
        &mut self,
        editor: &Editor,
        fold_state: &FoldState,
        scroll_y: f32,
        highlights: &mut dyn HighlightSource,
        target: FrameTarget<'_>,
    ) -> Result<(), IridiumError> {
        let FrameTarget {
            view,
            device,
            queue,
            width,
            height,
        } = target;

        // PERF: Only update viewport uniforms when dimensions actually change
        // Avoids 3 GPU buffer writes per frame when dimensions are unchanged
        self.sync_viewport_uniforms(queue, width, height);

        // Get the content to render, handling folded lines
        let doc = &editor.state().document;
        let line_count = doc.line_count();

        // Calculate visible line range for viewport virtualization
        let line_height = self.text_renderer.line_height();
        let surface_height = u32_to_f32(height);
        let first_visible_line = pixel_to_index((scroll_y / line_height).floor());
        let visible_line_count = pixel_to_index((surface_height / line_height).ceil()) + 2;
        let overscan = 10; // Extra lines above/below for smooth scrolling
        let viewport_start = first_visible_line.saturating_sub(overscan);
        let viewport_end = (first_visible_line + visible_line_count + overscan).min(line_count);

        // Calculate the byte offset where visible_content starts in the full
        // document. This is needed to correctly map span byte offsets to
        // visible_content positions.
        let viewport_start_byte = doc.line_to_byte_offset(viewport_start).unwrap_or(0);

        // Calculate layout dimensions
        let char_width = self.cached_char_width;
        let padding = 10.0_f32;

        // Calculate gutter width (based on digit count or custom text width,
        // before shaping)
        let gutter_width = self.frame_gutter_width(line_count);
        let content_offset_x = gutter_width + padding;
        let content_width = u32_to_f32(width) - content_offset_x;

        // The retained-shaping gate: every input the shaped buffers depend
        // on, compared against the last frame's. On a hit the content
        // extraction, highlight resolution, both shaping passes, the wrap
        // readback and the gutter text are all skipped — the retained
        // buffers and the between-frames caches (`cpu_visible_content`,
        // `cpu_visible_doc_lines`, `cpu_doc_to_visual`,
        // `cached_visual_line_map`, `cpu_line_numbers`,
        // `cached_total_visual_lines`) are deterministic functions of the
        // same key and already hold this frame's values.
        let key = ShapeKey {
            document_revision: doc.revision(),
            viewport_start,
            viewport_end,
            content_width: content_width.to_bits(),
            font_size: self.text_renderer.font_size().to_bits(),
            line_height_factor: self.text_renderer.config().line_height.to_bits(),
            font_generation: self.font_generation,
            theme_generation: self.theme_generation,
            syntax_enabled: self.syntax_enabled,
            language_active: highlights.language_active(),
            highlight_generation: highlights.generation(),
            syntax_theme_generation: self.syntax_theme_generation,
            fold_generation: fold_state.generation(),
            gutter_text_generation: self.gutter_text_generation,
        };

        let retained = match self.retained.take() {
            Some(shape) if shape.key == key => shape,
            previous => self.rebuild_retained(
                previous,
                key,
                editor,
                fold_state,
                highlights,
                &RebuildGeometry {
                    viewport_start,
                    viewport_end,
                    viewport_start_byte,
                    content_width,
                    gutter_width,
                    content_offset_x,
                    line_height,
                    line_count,
                },
            ),
        };
        let buffer = &retained.buffer;
        let gutter_buffer = retained.gutter.as_ref();

        // Calculate cursor position (accounting for gutter offset, folding,
        // scroll, and line wrapping)
        let cursor_pos = editor.cursor();
        let visual_cursor_line = self
            .cpu_doc_to_visual
            .get(cursor_pos.line)
            .and_then(|v| *v)
            .unwrap_or(0);

        // Calculate virtual scroll offset for viewport virtualization
        // This must match the offset used for text rendering (uses viewport_start directly)
        let virtual_scroll_offset = index_to_f32(viewport_start) * line_height;

        // Calculate cursor's line index within the viewport buffer
        // The buffer only contains lines from viewport_start, so we need relative positioning
        let viewport_start_visual = self
            .cpu_doc_to_visual
            .get(viewport_start)
            .and_then(|v| *v)
            .unwrap_or(0);
        let cursor_line_in_buffer = visual_cursor_line.saturating_sub(viewport_start_visual);

        // Use buffer layout to get accurate position with line wrapping.
        //
        // Only the vertical position is wanted here. Scrolling follows the
        // primary caret alone, and inline blame sits on its line; the caret
        // *quads* — the primary's included — are positioned in the pass further
        // down, which walks every selection.
        let (_, wrap_y) = self.text_renderer.cursor_position_in_buffer(
            buffer,
            cursor_line_in_buffer,
            cursor_pos.column,
            char_width,
        );
        // Absolute cursor Y in document space (for cursor_anchor_y)
        let cursor_abs_y = padding + wrap_y + virtual_scroll_offset;
        // Viewport-relative cursor Y for rendering
        let cursor_y = cursor_abs_y - scroll_y;

        // Cache cursor position for cursor_anchor_y
        self.cached_cursor_abs_y = cursor_abs_y;
        self.cached_cursor_doc_line = cursor_pos.line;

        // Update cursor blink state
        self.cursor_renderer.update(Instant::now());

        let metrics = FrameMetrics {
            line_height,
            char_width,
            padding,
            content_offset_x,
            virtual_scroll_offset,
            viewport_start,
            viewport_start_visual,
            scroll_y,
            surface_width: u32_to_f32(width),
            surface_height,
        };

        // PERF: Reuse pre-allocated quad buffers
        // Create gutter background quad
        self.cpu_gutter_quads.clear();
        if self.gutter_enabled {
            let gutter_bg_color = self.theme.editor.gutter;
            self.cpu_gutter_quads.push(Quad::new(
                0.0,
                0.0,
                gutter_width,
                metrics.surface_height,
                gutter_bg_color,
            ));
        }

        self.build_line_background_quads(&metrics);
        self.build_gutter_change_quads(&metrics);
        self.build_selection_quads(editor, buffer, &metrics);
        self.build_cursor_quads(editor, buffer, &metrics);

        // Text areas share the virtualized scroll position: the buffer starts
        // at viewport_start, so only the remainder of the scroll applies.
        let adjusted_scroll_y = scroll_y - virtual_scroll_offset;
        let foreground = self.theme.editor.foreground;

        // Main content text area
        let main_text_area = TextRenderer::create_text_area(
            buffer,
            content_offset_x,
            padding - adjusted_scroll_y,
            1.0,
            TextBounds {
                left: 0,
                top: 0,
                right: dimension_to_bound(width),
                bottom: dimension_to_bound(height),
            },
            foreground,
        );

        // Build blame buffer if blame data exists for the cursor line.
        // This creates a third TextArea rendered as ghost text after the line content.
        let blame_fg = self.theme.editor.blame_foreground;
        let blame_buffer = if self.blame_data.is_empty() {
            None
        } else {
            let cursor_doc_line = editor.cursor().line;
            if let Some(blame_text) = self.blame_data.get(&cursor_doc_line) {
                self.cpu_blame_content.clear();
                // Pad with spaces to separate from line content
                self.cpu_blame_content.push_str("  ");
                self.cpu_blame_content.push_str(blame_text);
                let mut blame_buf = self.text_renderer.create_buffer(None);
                self.text_renderer
                    .set_text(&mut blame_buf, &self.cpu_blame_content, blame_fg);
                self.text_renderer.shape_buffer(&mut blame_buf);
                Some(blame_buf)
            } else {
                None
            }
        };

        // Calculate blame text area position: after the end of the cursor line content
        let blame_left = if blame_buffer.is_some() {
            let cursor_doc_line = editor.cursor().line;
            let line_len = editor
                .state()
                .document
                .line(cursor_doc_line)
                .map_or(0, |l| l.chars().count());
            let line_end_x = index_to_f32(line_len) * char_width;
            content_offset_x + line_end_x
        } else {
            0.0
        };

        // Prepare text for rendering - use Vec for dynamic text area count
        let line_number_color = self.theme.editor.line_number;

        // Build text areas list dynamically based on what's enabled
        let mut text_areas: Vec<TextArea> = Vec::with_capacity(3);
        text_areas.push(main_text_area);

        if let Some(gutter_buf) = gutter_buffer {
            text_areas.push(TextRenderer::create_text_area(
                gutter_buf,
                8.0, // Small padding from left edge
                padding - adjusted_scroll_y,
                1.0,
                TextBounds {
                    left: 0,
                    top: 0,
                    right: pixel_to_bound(gutter_width),
                    bottom: dimension_to_bound(height),
                },
                line_number_color,
            ));
        }

        if let Some(blame_buf) = &blame_buffer {
            text_areas.push(TextRenderer::create_text_area(
                blame_buf,
                blame_left,
                cursor_y,
                1.0,
                TextBounds {
                    left: pixel_to_bound(blame_left),
                    top: 0,
                    right: dimension_to_bound(width),
                    bottom: dimension_to_bound(height),
                },
                blame_fg,
            ));
        }

        self.text_renderer.prepare(device, queue, text_areas)?;

        // Get background color from theme
        let bg = self.theme.editor.background;
        let clear_color = wgpu::Color {
            r: f64::from(bg.r),
            g: f64::from(bg.g),
            b: f64::from(bg.b),
            a: f64::from(bg.a),
        };

        // PERF: Batch all background quads using pre-allocated buffer.
        // Render order (back to front): gutter bg → line backgrounds → gutter change bars → selection
        self.cpu_background_quads.clear();
        self.cpu_background_quads
            .extend(self.cpu_gutter_quads.iter().copied());
        self.cpu_background_quads
            .extend(self.cpu_line_bg_quads.iter().copied());
        self.cpu_background_quads
            .extend(self.cpu_gutter_change_quads.iter().copied());
        self.cpu_background_quads
            .extend(self.cpu_selection_quads.iter().copied());

        self.submit_pass(view, device, queue, clear_color)?;

        // Trim glyph cache periodically
        self.text_renderer.trim_cache();

        // Keep the shaped buffers for the next frame. A frame that failed
        // above simply drops them — the next frame rebuilds, which costs a
        // miss and nothing else.
        self.retained = Some(retained);

        Ok(())
    }

    /// Re-uploads the renderers' viewport uniforms when the surface size
    /// changed since the last frame; a no-op otherwise.
    fn sync_viewport_uniforms(&mut self, queue: &Queue, width: u32, height: u32) {
        if width != self.cached_viewport_width || height != self.cached_viewport_height {
            self.cached_viewport_width = width;
            self.cached_viewport_height = height;
            self.text_renderer.update_viewport(queue, width, height);
            self.background_quad_renderer
                .update_viewport(queue, width, height);
            self.cursor_quad_renderer
                .update_viewport(queue, width, height);
        }
    }

    /// Rebuilds the visible-content string and the doc↔visual line maps for
    /// the given viewport, honoring folds. Returns the total visible-line
    /// count (each visible document line counted once, wrapping not yet
    /// known).
    fn build_visible_content(
        &mut self,
        editor: &Editor,
        fold_state: &FoldState,
        viewport_start: usize,
        viewport_end: usize,
    ) -> usize {
        let doc = &editor.state().document;
        let line_count = doc.line_count();

        // Build visible content, only including lines in viewport
        // Line numbers are built AFTER shaping to account for line wrapping
        // PERF: Reuse pre-allocated buffers to avoid per-frame allocations
        self.cpu_visible_content.clear();
        self.cpu_visible_doc_lines.clear();

        // Ensure doc_to_visual has capacity for all lines (grows if needed, never shrinks)
        if self.cpu_doc_to_visual.capacity() < line_count {
            self.cpu_doc_to_visual
                .reserve(line_count - self.cpu_doc_to_visual.capacity());
        }
        self.cpu_doc_to_visual.clear();

        let mut visual_line = 0;

        // Pre-fill doc_to_visual for lines before viewport
        for doc_line in 0..viewport_start {
            if fold_state.is_line_hidden(doc_line) {
                self.cpu_doc_to_visual.push(None);
            } else {
                self.cpu_doc_to_visual.push(Some(visual_line));
                visual_line += 1;
            }
        }

        // Only process lines in viewport range
        for doc_line in viewport_start..viewport_end {
            if fold_state.is_line_hidden(doc_line) {
                self.cpu_doc_to_visual.push(None);
                continue;
            }

            // Add newline separator (except for first visible line in our buffer)
            if !self.cpu_visible_doc_lines.is_empty() {
                self.cpu_visible_content.push('\n');
            }

            // Add line content
            if let Some(line_text) = doc.line(doc_line) {
                self.cpu_visible_content.push_str(&line_text);
            }

            // If this line is folded, also append the closing brace from the fold end
            if fold_state.is_folded(doc_line) {
                if let Some(region) = fold_state.region_at(doc_line) {
                    // Get the end line and find the closing brace
                    if let Some(end_line_text) = doc.line(region.end_line) {
                        let trimmed = end_line_text.trim();
                        // Append the closing portion (usually just "}")
                        if !trimmed.is_empty() {
                            self.cpu_visible_content.push_str(" ... ");
                            self.cpu_visible_content.push_str(trimmed);
                        }
                    }
                }
            }

            self.cpu_visible_doc_lines.push(doc_line);
            self.cpu_doc_to_visual.push(Some(visual_line));
            visual_line += 1;
        }

        // Fill remaining doc_to_visual for lines after viewport
        for doc_line in viewport_end..line_count {
            if fold_state.is_line_hidden(doc_line) {
                self.cpu_doc_to_visual.push(None);
            } else {
                self.cpu_doc_to_visual.push(Some(visual_line));
                visual_line += 1;
            }
        }

        visual_line
    }

    /// The gutter width for the frame being composed: measured from the
    /// longest custom gutter line when custom text is active, otherwise from
    /// the document's digit count.
    ///
    /// Distinct from [`Self::gutter_width`], which is the between-frames
    /// answer hit-testing uses and which — exactly as before the extraction —
    /// does not consult custom gutter text.
    fn frame_gutter_width(&self, line_count: usize) -> f32 {
        if !self.gutter_enabled {
            return 0.0;
        }
        let char_width = self.cached_char_width;
        self.custom_gutter_lines.as_ref().map_or_else(
            || self.gutter_renderer.calculate_width(line_count, char_width),
            |custom_lines| {
                // Width from longest custom gutter line
                let max_chars = custom_lines.iter().map(String::len).max().unwrap_or(1);
                // Same padding formula as GutterRenderer::calculate_width
                let number_width = index_to_f32(max_chars) * char_width;
                let side_padding = char_width * 2.0;
                number_width + side_padding
            },
        )
    }

    /// Fills the content buffer's text and colors: the face's resolved spans
    /// when it has them, the built-in keyword bridge when a language is set
    /// but its spans are not available this frame, plain foreground when no
    /// language is set or syntax highlighting is off.
    fn fill_content_buffer(
        &mut self,
        buffer: &mut Buffer,
        highlights: &mut dyn HighlightSource,
        content_start_byte: usize,
        content_width: f32,
        line_height: f32,
    ) {
        // Set the text with syntax highlighting if enabled
        let foreground = self.theme.editor.foreground;
        if self.syntax_enabled {
            let context = HighlightContext {
                content: &self.cpu_visible_content,
                content_start_byte,
                content_width,
                line_height,
                viewport_config: &self.viewport_config,
                syntax_theme: &self.syntax_theme,
                foreground,
            };
            if let Some(rich_spans) = highlights.resolve(&context) {
                self.text_renderer
                    .set_rich_text(buffer, rich_spans.into_iter());
            } else if highlights.language_active() {
                // The bridge: a language is set but its spans are not here
                // this frame — the keyword highlighter colors until they are.
                // PERF: Pass iterator directly instead of collecting into Vec
                let spans = self.highlighter.highlight_flat(&self.cpu_visible_content);
                self.text_renderer.set_rich_text(
                    buffer,
                    spans.iter().map(|span| (span.text.as_str(), span.color)),
                );
            } else {
                // No language set: nothing to bridge to, plain foreground.
                self.text_renderer
                    .set_text(buffer, &self.cpu_visible_content, foreground);
            }
        } else {
            // Plain text
            self.text_renderer
                .set_text(buffer, &self.cpu_visible_content, foreground);
        }
    }

    /// The stage-2a counterpart of [`Self::fill_content_buffer`]: the same
    /// four-way span resolution, routed through the per-line diffing
    /// setters so lines whose text and colors did not change keep their
    /// shape caches. Returns how many lines were actually reshaped.
    fn fill_content_buffer_diffed(
        &self,
        buffer: &mut Buffer,
        highlights: &mut dyn HighlightSource,
        content_start_byte: usize,
        content_width: f32,
        line_height: f32,
    ) -> usize {
        let foreground = self.theme.editor.foreground;
        if self.syntax_enabled {
            let context = HighlightContext {
                content: &self.cpu_visible_content,
                content_start_byte,
                content_width,
                line_height,
                viewport_config: &self.viewport_config,
                syntax_theme: &self.syntax_theme,
                foreground,
            };
            if let Some(rich_spans) = highlights.resolve(&context) {
                TextRenderer::set_rich_text_diffed(buffer, rich_spans.into_iter())
            } else if highlights.language_active() {
                let spans = self.highlighter.highlight_flat(&self.cpu_visible_content);
                TextRenderer::set_rich_text_diffed(
                    buffer,
                    spans.iter().map(|span| (span.text.as_str(), span.color)),
                )
            } else {
                TextRenderer::set_text_diffed(buffer, &self.cpu_visible_content, foreground)
            }
        } else {
            TextRenderer::set_text_diffed(buffer, &self.cpu_visible_content, foreground)
        }
    }

    /// Rebuilds the retained shaped buffers for a frame whose [`ShapeKey`]
    /// missed.
    ///
    /// When only the document content and/or the highlight answer moved
    /// against the previous frame's key ([`ShapeKey::permits_line_diff`]),
    /// the previous buffers are reclaimed and refilled through the per-line
    /// diffing setters — a one-character keystroke reshapes one line. Any
    /// other miss (viewport shift, resize, font, theme, folds, gutter) runs
    /// the full path: fresh buffers, full fill, full shape. Both arms end
    /// identically — shape, wrap readback, gutter text — so hit, diffed
    /// miss and full miss all leave the same state behind.
    fn rebuild_retained(
        &mut self,
        previous: Option<RetainedShape>,
        key: ShapeKey,
        editor: &Editor,
        fold_state: &FoldState,
        highlights: &mut dyn HighlightSource,
        g: &RebuildGeometry,
    ) -> RetainedShape {
        self.shape_rebuilds = self.shape_rebuilds.wrapping_add(1);

        let doc_visual_lines =
            self.build_visible_content(editor, fold_state, g.viewport_start, g.viewport_end);

        let reclaimed = match previous {
            Some(prev) if key.permits_line_diff(&prev.key) => Some(prev),
            _ => None,
        };

        let (mut buffer, previous_gutter, previous_gutter_width_bits) =
            if let Some(prev) = reclaimed {
                let mut buffer = prev.buffer;
                let reshaped = self.fill_content_buffer_diffed(
                    &mut buffer,
                    highlights,
                    g.viewport_start_byte,
                    g.content_width,
                    g.line_height,
                );
                self.lines_reshaped = self
                    .lines_reshaped
                    .wrapping_add(u64::try_from(reshaped).unwrap_or(u64::MAX));
                (buffer, prev.gutter, Some(prev.gutter_width_bits))
            } else {
                let mut buffer = self.text_renderer.create_buffer(Some(g.content_width));
                self.fill_content_buffer(
                    &mut buffer,
                    highlights,
                    g.viewport_start_byte,
                    g.content_width,
                    g.line_height,
                );
                (buffer, None, None)
            };
        self.text_renderer.shape_buffer(&mut buffer);

        self.rebuild_visual_line_map(
            &buffer,
            g.viewport_start,
            g.content_offset_x,
            doc_visual_lines,
        );

        // NOW build line numbers with proper spacing for wrapped lines
        self.build_line_numbers(&buffer, g.line_count);

        // Create/refresh the gutter buffer AFTER we know the wrapping
        let gutter_width_bits = g.gutter_width.to_bits();
        let gutter = if self.gutter_enabled {
            let line_number_color = self.theme.editor.line_number;
            // Reclaim the previous gutter buffer only when its width is
            // bit-identical — see [`RetainedShape::gutter_width_bits`].
            let reusable =
                previous_gutter.filter(|_| previous_gutter_width_bits == Some(gutter_width_bits));
            let mut gutter_buf = if let Some(mut existing) = reusable {
                let reshaped = TextRenderer::set_text_diffed(
                    &mut existing,
                    &self.cpu_line_numbers,
                    line_number_color,
                );
                self.lines_reshaped = self
                    .lines_reshaped
                    .wrapping_add(u64::try_from(reshaped).unwrap_or(u64::MAX));
                existing
            } else {
                let mut fresh = self.text_renderer.create_buffer(Some(g.gutter_width));
                self.text_renderer
                    .set_text(&mut fresh, &self.cpu_line_numbers, line_number_color);
                fresh
            };
            self.text_renderer.shape_buffer(&mut gutter_buf);
            Some(gutter_buf)
        } else {
            None
        };

        RetainedShape {
            key,
            buffer,
            gutter,
            gutter_width_bits,
        }
    }

    /// Reads the shaped buffer's layout runs back into the visual line map
    /// and derives the wrap-aware total visual line count.
    fn rebuild_visual_line_map(
        &mut self,
        buffer: &Buffer,
        viewport_start: usize,
        content_offset_x: f32,
        doc_visual_lines: usize,
    ) {
        // Build cached visual line map from layout_runs() for pixel_to_position.
        // Each entry maps a visual line (within the buffer) to (buffer_line_index, run_start_column).
        // This allows pixel_to_position to correctly resolve clicks on wrapped lines.
        self.cached_visual_line_map.clear();
        self.cached_map_viewport_start = viewport_start;
        self.cached_content_offset_x = content_offset_x;
        for run in buffer.layout_runs() {
            let run_start_col = if run.glyphs.is_empty() {
                0
            } else {
                run.glyphs.first().map_or(0, |g| g.start)
            };
            self.cached_visual_line_map
                .push((run.line_i, run_start_col));
        }

        // Cache total visual lines for max_scroll_y.
        // doc_visual_lines (from the doc_to_visual pass) counts all visible doc lines as 1 each.
        // The actual buffer visual lines (from layout_runs) may be more due to wrapping.
        // Extra wrapped lines = buffer_visual_lines - buffer_logical_lines.
        let buffer_visual_lines = self.cached_visual_line_map.len();
        let buffer_logical_lines = self.cpu_visible_doc_lines.len();
        let extra_wrap_lines = buffer_visual_lines.saturating_sub(buffer_logical_lines);
        self.cached_total_visual_lines = doc_visual_lines + extra_wrap_lines;
    }

    /// Rebuilds the gutter text with wrap-aware blank continuation rows:
    /// custom gutter lines when set, right-aligned line numbers otherwise.
    fn build_line_numbers(&mut self, buffer: &Buffer, line_count: usize) {
        // PERF: Reuse pre-allocated string buffer, use write!() to avoid format!() allocation
        let visual_lines_per_line = self.text_renderer.visual_lines_per_logical_line(buffer);
        self.cpu_line_numbers.clear();

        if let Some(custom_lines) = &self.custom_gutter_lines {
            // Custom gutter text: use provided strings instead of auto line numbers.
            // This is used for diff views with dual line numbers + markers.
            for (i, &doc_line) in self.cpu_visible_doc_lines.iter().enumerate() {
                if i > 0 {
                    self.cpu_line_numbers.push('\n');
                }

                // Use custom text for this doc_line, or empty string if out of range
                let text = custom_lines.get(doc_line).map_or("", |s| s.as_str());
                self.cpu_line_numbers.push_str(text);

                // Add blank lines for wrapped visual lines
                let wrap_count = visual_lines_per_line.get(i).copied().unwrap_or(1);
                let text_len = text.len();
                for _ in 1..wrap_count {
                    self.cpu_line_numbers.push('\n');
                    for _ in 0..text_len {
                        self.cpu_line_numbers.push(' ');
                    }
                }
            }

            if self.cpu_visible_doc_lines.is_empty() {
                self.cpu_line_numbers.push(' ');
            }
        } else {
            // Standard auto line numbers
            let digit_width = GutterRenderer::digit_columns(line_count);

            for (i, &doc_line) in self.cpu_visible_doc_lines.iter().enumerate() {
                // Add newline separator (except for first visible line)
                if i > 0 {
                    self.cpu_line_numbers.push('\n');
                }

                // Add line number (1-indexed) directly into buffer without allocation
                // write!() into String never fails, so we can ignore the Result
                let _ = write!(
                    self.cpu_line_numbers,
                    "{:>width$}",
                    doc_line + 1,
                    width = digit_width,
                );

                // Add blank lines for wrapped visual lines (continuation lines)
                let wrap_count = visual_lines_per_line.get(i).copied().unwrap_or(1);
                for _ in 1..wrap_count {
                    self.cpu_line_numbers.push('\n');
                    // Add blank spacing to maintain alignment
                    for _ in 0..digit_width {
                        self.cpu_line_numbers.push(' ');
                    }
                }
            }

            // Handle empty document
            if line_count == 0 {
                self.cpu_line_numbers.push_str(" 1");
            }
        }
    }

    /// Emits line background quads for diff highlighting.
    ///
    /// These render behind selection highlights so diffs are visible even
    /// when selected.
    fn build_line_background_quads(&mut self, m: &FrameMetrics) {
        self.cpu_line_bg_quads.clear();
        if self.line_backgrounds.is_empty() {
            return;
        }
        for (vi, &doc_line) in self.cpu_visible_doc_lines.iter().enumerate() {
            if let Some(&bg_color) = self.line_backgrounds.get(&doc_line) {
                // Walk the visual line map to find which visual rows correspond
                // to this logical line (accounts for wrapping).
                let buffer_line = self
                    .cpu_doc_to_visual
                    .get(doc_line)
                    .and_then(|v| *v)
                    .unwrap_or(0)
                    .saturating_sub(
                        self.cpu_doc_to_visual
                            .get(m.viewport_start)
                            .and_then(|v| *v)
                            .unwrap_or(0),
                    );

                let mut emitted = false;
                for (vline_idx, &(buf_idx, _)) in self.cached_visual_line_map.iter().enumerate() {
                    if buf_idx != buffer_line {
                        continue;
                    }
                    let row_y = index_to_f32(vline_idx) * m.line_height;
                    let y = m.padding + row_y + m.virtual_scroll_offset - m.scroll_y;
                    if y + m.line_height > 0.0 && y < m.surface_height {
                        self.cpu_line_bg_quads.push(Quad::new(
                            m.content_offset_x,
                            y,
                            m.surface_width - m.content_offset_x,
                            m.line_height,
                            bg_color,
                        ));
                    }
                    emitted = true;
                }

                // Fallback: if visual line map wasn't built yet, use simple position
                if !emitted {
                    let row_y = index_to_f32(vi) * m.line_height;
                    let y = m.padding + row_y + m.virtual_scroll_offset - m.scroll_y;
                    if y + m.line_height > 0.0 && y < m.surface_height {
                        self.cpu_line_bg_quads.push(Quad::new(
                            m.content_offset_x,
                            y,
                            m.surface_width - m.content_offset_x,
                            m.line_height,
                            bg_color,
                        ));
                    }
                }
            }
        }
    }

    /// Emits gutter change indicator quads (thin colored bars at the left
    /// edge).
    fn build_gutter_change_quads(&mut self, m: &FrameMetrics) {
        self.cpu_gutter_change_quads.clear();
        if self.gutter_changes.is_empty() || !self.gutter_enabled {
            return;
        }
        for (vi, &doc_line) in self.cpu_visible_doc_lines.iter().enumerate() {
            if let Some(&bar_color) = self.gutter_changes.get(&doc_line) {
                let buffer_line = self
                    .cpu_doc_to_visual
                    .get(doc_line)
                    .and_then(|v| *v)
                    .unwrap_or(0)
                    .saturating_sub(
                        self.cpu_doc_to_visual
                            .get(m.viewport_start)
                            .and_then(|v| *v)
                            .unwrap_or(0),
                    );

                let mut emitted = false;
                for (vline_idx, &(buf_idx, _)) in self.cached_visual_line_map.iter().enumerate() {
                    if buf_idx != buffer_line {
                        continue;
                    }
                    let row_y = index_to_f32(vline_idx) * m.line_height;
                    let y = m.padding + row_y + m.virtual_scroll_offset - m.scroll_y;
                    if y + m.line_height > 0.0 && y < m.surface_height {
                        // 3px wide bar at left gutter edge
                        self.cpu_gutter_change_quads.push(Quad::new(
                            2.0,
                            y,
                            3.0,
                            m.line_height,
                            bar_color,
                        ));
                    }
                    emitted = true;
                }

                if !emitted {
                    let row_y = index_to_f32(vi) * m.line_height;
                    let y = m.padding + row_y + m.virtual_scroll_offset - m.scroll_y;
                    if y + m.line_height > 0.0 && y < m.surface_height {
                        self.cpu_gutter_change_quads.push(Quad::new(
                            2.0,
                            y,
                            3.0,
                            m.line_height,
                            bar_color,
                        ));
                    }
                }
            }
        }
    }

    /// Emits selection highlight quads (rendered before text).
    ///
    /// Uses the visual line map for wrap-aware positioning so that selection
    /// highlights align with text even when lines wrap.
    ///
    /// Every cursor's selection is drawn, not only the primary's. Drawing
    /// just the primary is what made multi-cursor invisible in the web face:
    /// the kernel held N selections and the screen showed one, so a command
    /// that worked perfectly looked like a command that did nothing.
    fn build_selection_quads(&mut self, editor: &Editor, buffer: &Buffer, m: &FrameMetrics) {
        self.cpu_selection_quads.clear();
        let selection_color = self.theme.editor.selection;
        for selection in editor.state().cursor.all_selections() {
            if selection.is_collapsed() {
                continue;
            }
            let sel_start = selection.start();
            let sel_end = selection.end();

            for doc_line in sel_start.line..=sel_end.line {
                // Skip hidden/folded lines
                let Some(vis_line) = self.cpu_doc_to_visual.get(doc_line).and_then(|v| *v) else {
                    continue;
                };

                let line_content = editor.state().document.line(doc_line);
                let line_len = line_content.map_or(0, |l| l.chars().count());

                // Determine selection columns for this line
                let sel_col_start = if doc_line == sel_start.line {
                    sel_start.column
                } else {
                    0
                };
                let sel_col_end = if doc_line == sel_end.line {
                    sel_end.column
                } else {
                    line_len
                };

                // Extra width for newline visualization on non-final lines
                let wants_newline_extra = doc_line != sel_end.line && sel_col_end == line_len;
                let newline_extra = if wants_newline_extra {
                    m.char_width * 0.5
                } else {
                    0.0
                };

                if sel_col_start >= sel_col_end && !wants_newline_extra {
                    continue;
                }

                // Get buffer-relative line index for this doc line
                let buffer_line = vis_line.saturating_sub(m.viewport_start_visual);

                // Walk the visual line map to find which visual rows this buffer line
                // spans and emit a quad for each wrapped segment that overlaps the selection.
                let mut handled = false;
                for (vi, &(buf_idx, run_start_col)) in
                    self.cached_visual_line_map.iter().enumerate()
                {
                    if buf_idx != buffer_line {
                        continue;
                    }

                    // Determine the end column of this visual segment
                    let run_end_col = self
                        .cached_visual_line_map
                        .get(vi + 1)
                        .filter(|(next_buf, _)| *next_buf == buffer_line)
                        .map_or(line_len, |(_, next_start)| *next_start);

                    // Intersect this segment with the selection range
                    let overlap_start = sel_col_start.max(run_start_col);
                    let overlap_end = sel_col_end.min(run_end_col);

                    // Extra width only applies on the last segment of the line
                    let seg_extra = if run_end_col >= line_len {
                        newline_extra
                    } else {
                        0.0
                    };

                    if overlap_start < overlap_end
                        || (overlap_start == overlap_end && seg_extra > 0.0)
                    {
                        let seg_x = index_to_f32(overlap_start - run_start_col) * m.char_width;
                        let x = m.content_offset_x + seg_x;
                        let row_y = index_to_f32(vi) * m.line_height;
                        let y = m.padding + row_y + m.virtual_scroll_offset - m.scroll_y;
                        let seg_width = index_to_f32(overlap_end - overlap_start) * m.char_width;
                        let width = seg_width + seg_extra;

                        if y + m.line_height > 0.0 && y < m.surface_height {
                            self.cpu_selection_quads.push(Quad::new(
                                x,
                                y,
                                width,
                                m.line_height,
                                selection_color,
                            ));
                        }
                        handled = true;
                    }
                }

                // Fallback for lines not in the visual map (shouldn't happen, but safe)
                if !handled {
                    let (sx, sy) = self.text_renderer.cursor_position_in_buffer(
                        buffer,
                        buffer_line,
                        sel_col_start,
                        m.char_width,
                    );
                    let x = m.content_offset_x + sx;
                    let y = m.padding + sy + m.virtual_scroll_offset - m.scroll_y;
                    let sel_width = index_to_f32(sel_col_end - sel_col_start) * m.char_width;
                    let width = sel_width + newline_extra;

                    if y + m.line_height > 0.0 && y < m.surface_height {
                        self.cpu_selection_quads.push(Quad::new(
                            x,
                            y,
                            width,
                            m.line_height,
                            selection_color,
                        ));
                    }
                }
            }
        }
    }

    /// Emits a cursor quad (2px wide line) for **every** caret.
    ///
    /// The primary's vertical position is computed in [`Self::compose`] and
    /// kept there, because scrolling must keep following the primary alone.
    /// The others are derived here the same way.
    ///
    /// Blink is shared on purpose: carets blinking out of phase read as a
    /// rendering fault rather than as one multi-cursor edit.
    fn build_cursor_quads(&mut self, editor: &Editor, buffer: &Buffer, m: &FrameMetrics) {
        let cursor_color = self.theme.editor.cursor;
        self.cpu_cursor_quads.clear();
        if !self.cursor_renderer.is_visible() {
            return;
        }
        for selection in editor.state().cursor.all_selections() {
            let head = selection.head;
            let head_visual_line = self
                .cpu_doc_to_visual
                .get(head.line)
                .and_then(|v| *v)
                .unwrap_or(0);
            let head_line_in_buffer = head_visual_line.saturating_sub(m.viewport_start_visual);
            let (head_x, head_y) = self.text_renderer.cursor_position_in_buffer(
                buffer,
                head_line_in_buffer,
                head.column,
                m.char_width,
            );
            let x = m.content_offset_x + head_x;
            let y = m.padding + head_y + m.virtual_scroll_offset - m.scroll_y;
            // Off-screen carets are skipped, exactly as selection quads are.
            if y + m.line_height > 0.0 && y < m.surface_height {
                self.cpu_cursor_quads
                    .push(Quad::new(x, y, 2.0, m.line_height, cursor_color));
            }
        }
    }

    /// Records and submits the frame's render pass: background quads, then
    /// text, then cursor quads on top.
    ///
    /// The two quad renderers are separate on purpose — see the field
    /// documentation on [`Self::cursor_quad_renderer`].
    fn submit_pass(
        &mut self,
        view: &TextureView,
        device: &Device,
        queue: &Queue,
        clear_color: wgpu::Color,
    ) -> Result<(), IridiumError> {
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("Iridium Frame Encoder"),
        });

        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Iridium Render Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(clear_color),
                        store: StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Render gutter background and selection highlights (behind text)
            self.background_quad_renderer
                .render(&mut pass, queue, &self.cpu_background_quads);

            // Render text (main content and gutter line numbers)
            self.text_renderer.render(&mut pass)?;

            // Render cursor on top - separate vertex buffer avoids the GPU
            // buffer overwrite issue that caused selection blinking
            self.cursor_quad_renderer
                .render(&mut pass, queue, &self.cpu_cursor_quads);
        }

        queue.submit(std::iter::once(encoder.finish()));
        Ok(())
    }

    // =========================================================================
    // Between-frames layout queries (the cache consumers' interface)
    // =========================================================================

    /// The maximum vertical scroll offset in pixels.
    ///
    /// Uses the wrap-aware visual line total from the last composed frame;
    /// before the first frame it falls back to the fold-aware count, which
    /// knows nothing of wrapping — exactly the pre-extraction behavior.
    pub fn max_scroll_y(
        &self,
        editor: &Editor,
        fold_state: &FoldState,
        viewport_height: f32,
    ) -> f32 {
        let line_height = self.text_renderer.line_height();
        let total_visual = if self.cached_total_visual_lines > 0 {
            self.cached_total_visual_lines
        } else {
            // Before first render, fall back to fold-aware count (no wrapping info)
            fold_state.visible_line_count(editor.state().document.line_count())
        };
        let content_height = index_to_f32(total_visual) * line_height;
        (content_height - viewport_height + 20.0).max(0.0) // 20px padding
    }

    /// The primary caret's absolute Y position in document space, for
    /// scroll-to-caret decisions.
    ///
    /// Uses the cached wrap-aware position from the last frame when the
    /// caret is still on the same line (the common case: typing on one
    /// line); falls back to fold-only estimation when it has moved.
    pub fn cursor_anchor_y(&self, editor: &Editor, fold_state: &FoldState) -> f32 {
        let line_height = self.text_renderer.line_height();
        let padding = 10.0;
        let cursor_line = editor.cursor().line;
        if cursor_line == self.cached_cursor_doc_line && self.cached_cursor_abs_y > 0.0 {
            // Cursor on the same line as last render — use cached position
            // (accounts for wrapping within this line)
            self.cached_cursor_abs_y
        } else {
            // Cursor moved to a different line — approximate using fold mapping
            let visual_line = fold_state.document_to_visual_line(cursor_line).unwrap_or(0);
            let row_y = index_to_f32(visual_line) * line_height;
            padding + row_y
        }
    }

    /// Converts pixel coordinates to a (line, column) document position,
    /// accounting for folded lines, scroll, and line wrapping.
    ///
    /// Runs on the input hot path (every mouse move), off the visual line map
    /// cached by the last [`Self::compose`]. Before the first frame it falls
    /// back to a fold-only calculation with no wrap awareness — exactly the
    /// pre-extraction behavior, rounding included.
    pub fn pixel_to_position(
        &self,
        editor: &Editor,
        fold_state: &FoldState,
        scroll_y: f32,
        x: f32,
        y: f32,
    ) -> (usize, usize) {
        let line_height = self.text_renderer.line_height();
        let char_width = self.cached_char_width;
        let padding = 10.0_f32;

        let doc = &editor.state().document;
        let line_count = doc.line_count();

        if self.cached_visual_line_map.is_empty() {
            // Fallback: no cached map (before first render), use simple calculation
            let visual_line = pixel_to_index(((y + scroll_y - padding) / line_height).max(0.0));
            let doc_line = fold_state
                .visual_to_document_line(visual_line)
                .min(line_count.saturating_sub(1));

            let offset_x = self.gutter_width(line_count) + padding;
            let line_len = doc.line(doc_line).map_or(0, |l| l.chars().count());
            let column = pixel_to_index(((x - offset_x) / char_width + 0.5).max(0.0));
            let clamped_column = column.min(line_len);

            return (doc_line, clamped_column);
        }

        // Calculate the text area top offset (must match compose positioning)
        let virtual_scroll_offset = index_to_f32(self.cached_map_viewport_start) * line_height;
        let adjusted_scroll_y = scroll_y - virtual_scroll_offset;
        let text_area_top = padding - adjusted_scroll_y;

        // Calculate which visual line in the buffer was clicked
        let y_in_buffer = y - text_area_top;
        let visual_line_in_buffer = pixel_to_index((y_in_buffer / line_height).floor().max(0.0));

        // Use the cached visual line map to resolve the click position
        // Clamp to last visual line in the map
        let clamped_visual =
            visual_line_in_buffer.min(self.cached_visual_line_map.len().saturating_sub(1));
        let (buffer_line_idx, run_start_col) = self.cached_visual_line_map[clamped_visual];

        // Map buffer line index to document line
        let doc_line = self
            .cpu_visible_doc_lines
            .get(buffer_line_idx)
            .copied()
            .unwrap_or(0)
            .min(line_count.saturating_sub(1));

        // Calculate column within this wrap segment
        let col_in_run =
            pixel_to_index(((x - self.cached_content_offset_x) / char_width + 0.5).max(0.0));
        let column = run_start_col + col_in_run;

        // Clamp to actual line length
        let line_len = doc.line(doc_line).map_or(0, |l| l.chars().count());
        let clamped_column = column.min(line_len);

        (doc_line, clamped_column)
    }

    /// Converts a document (line, column) to pixel coordinates — the inverse
    /// of [`Self::pixel_to_position`], off the same cached visual line map.
    ///
    /// Returns `None` when the line is hidden by a fold or outside the
    /// composed viewport.
    pub fn position_to_pixel(
        &self,
        editor: &Editor,
        fold_state: &FoldState,
        scroll_y: f32,
        doc_line: usize,
        column: usize,
    ) -> Option<(f32, f32)> {
        let line_height = self.text_renderer.line_height();
        let char_width = self.cached_char_width;
        let padding = 10.0_f32;

        // Check if line is folded (hidden)
        if fold_state.is_line_hidden(doc_line) {
            return None;
        }

        if self.cached_visual_line_map.is_empty() {
            // Fallback: before first render, use simple calculation (no word wrap)
            let visual_line = fold_state.document_to_visual_line(doc_line)?;
            let offset_x = self.gutter_width(editor.state().document.line_count()) + padding;
            let column_x = index_to_f32(column) * char_width;
            let x = offset_x + column_x;
            let row_y = index_to_f32(visual_line) * line_height;
            let y = padding + row_y - scroll_y;
            return Some((x, y));
        }

        // Find the buffer line index for this document line
        let buf_idx = self
            .cpu_visible_doc_lines
            .iter()
            .position(|&dl| dl == doc_line)?;

        // Find the visual line that contains this column
        // (handles word wrap — a single document line may span multiple visual lines)
        let mut target_visual = None;
        let mut col_in_segment = column;

        for (visual_idx, &(bi, run_start)) in self.cached_visual_line_map.iter().enumerate() {
            if bi == buf_idx {
                // Check if this is the last segment for this buffer line
                let next_run_start = self
                    .cached_visual_line_map
                    .get(visual_idx + 1)
                    .filter(|&&(next_bi, _)| next_bi == buf_idx)
                    .map(|&(_, start)| start);

                if let Some(next_start) = next_run_start {
                    if column >= run_start && column < next_start {
                        target_visual = Some(visual_idx);
                        col_in_segment = column - run_start;
                        break;
                    }
                } else if column >= run_start {
                    // Last (or only) segment — column must be here
                    target_visual = Some(visual_idx);
                    col_in_segment = column - run_start;
                    break;
                }
            }
        }

        // A miss here means the document line is not in the current viewport.
        let visual_idx = target_visual?;
        let virtual_scroll_offset = index_to_f32(self.cached_map_viewport_start) * line_height;
        let seg_x = index_to_f32(col_in_segment) * char_width;
        let x = self.cached_content_offset_x + seg_x;
        let row_y = index_to_f32(visual_idx) * line_height;
        let y = padding + row_y + virtual_scroll_offset - scroll_y;
        Some((x, y))
    }

    /// The gutter width used for between-frames layout queries: digit-count
    /// based, zero when the gutter is disabled.
    ///
    /// Deliberately does not consult custom gutter text — the pre-extraction
    /// hit-testing path never did, and pixel-identity keeps that as it is.
    pub fn gutter_width(&self, line_count: usize) -> f32 {
        if self.gutter_enabled {
            self.gutter_renderer
                .calculate_width(line_count, self.cached_char_width)
        } else {
            0.0
        }
    }

    /// The line height in pixels, derived from the font configuration.
    pub fn line_height(&self) -> f32 {
        self.text_renderer.line_height()
    }

    /// The measured character advance width in pixels.
    pub const fn char_width(&self) -> f32 {
        self.cached_char_width
    }

    /// The wrap readback from the last frame: one entry per visual line,
    /// mapping to (`buffer_line_index`, `start_column_in_line`). Empty before
    /// the first frame.
    pub fn visual_line_map(&self) -> &[(usize, usize)] {
        &self.cached_visual_line_map
    }

    /// The first document line included when the visual line map was built.
    pub const fn map_viewport_start(&self) -> usize {
        self.cached_map_viewport_start
    }

    /// The content column's left edge when the visual line map was built.
    pub const fn content_offset_x(&self) -> f32 {
        self.cached_content_offset_x
    }

    /// Total visual lines including wrapped sub-lines, from the last frame.
    pub const fn total_visual_lines(&self) -> usize {
        self.cached_total_visual_lines
    }

    /// The primary caret's absolute Y (document space) from the last frame.
    pub const fn cursor_abs_y(&self) -> f32 {
        self.cached_cursor_abs_y
    }

    /// The caret's document line when [`Self::cursor_abs_y`] was computed.
    pub const fn cursor_doc_line(&self) -> usize {
        self.cached_cursor_doc_line
    }

    /// The document lines present in the last composed viewport, in order.
    pub fn visible_doc_lines(&self) -> &[usize] {
        &self.cpu_visible_doc_lines
    }

    /// How many times [`Self::compose`] has run the retained-shape rebuild
    /// path since this compositor was created.
    ///
    /// The observable behind every retention claim: a frame whose shaping
    /// inputs are unchanged must not move it, and a frame whose inputs
    /// changed must. Exists so the cache is testable without reading
    /// pixels — the exact pattern of the desktop face's
    /// `HighlightCache::rebuilds`.
    #[must_use]
    pub const fn shape_rebuilds(&self) -> u64 {
        self.shape_rebuilds
    }

    /// How many buffer lines the per-line diffing rebuild path has
    /// actually reshaped, across the content and gutter buffers, since
    /// this compositor was created.
    ///
    /// Full rebuilds do not move it — it counts only the diffed path's
    /// work, so "a one-character keystroke reshapes one line" is a
    /// testable claim rather than an asserted one.
    #[must_use]
    pub const fn lines_reshaped(&self) -> u64 {
        self.lines_reshaped
    }

    // =========================================================================
    // Host-facing state: fonts, theme, toggles, presentation inputs
    // =========================================================================

    /// Loads a font from raw TTF/OTF data and remeasures the character
    /// width, which every layout answer above depends on.
    pub fn load_font(&mut self, data: Vec<u8>) {
        self.text_renderer.load_font(data);
        // Measure actual character width from the loaded font
        self.cached_char_width = self.text_renderer.char_width();
        // New font data can change how Family::Monospace resolves — every
        // retained shape is stale.
        self.font_generation = self.font_generation.wrapping_add(1);
    }

    /// Sets the font size in pixels (already scaled for DPI by the face).
    ///
    /// Deliberately does not remeasure [`Self::char_width`]: before a font is
    /// loaded there is nothing honest to measure, and the cached width keeps
    /// its unloaded-font default until [`Self::load_font`] measures the real
    /// glyphs.
    pub fn set_font_size(&mut self, size: f32) {
        self.text_renderer.set_font_size(size);
    }

    /// Updates the renderers' viewport uniforms for a resized surface.
    ///
    /// Also updates the dimension cache so [`Self::compose`] does not
    /// redundantly re-upload the uniforms on its next frame.
    pub fn resize(&mut self, queue: &Queue, width: u32, height: u32) {
        self.text_renderer.update_viewport(queue, width, height);
        self.background_quad_renderer
            .update_viewport(queue, width, height);
        self.cursor_quad_renderer
            .update_viewport(queue, width, height);
        // Update cache to prevent redundant updates in compose
        self.cached_viewport_width = width;
        self.cached_viewport_height = height;
    }

    /// Restarts the caret blink cycle so the caret is visible immediately.
    ///
    /// Every input path that moves a caret calls this: a caret that stays in
    /// its off-phase across a movement looks like a caret that vanished.
    pub fn reset_blink(&mut self) {
        self.cursor_renderer.reset_blink();
    }

    /// Switches between the built-in dark and light themes, keeping the
    /// fallback highlighter's palette in step.
    pub fn set_dark_theme(&mut self, dark: bool) {
        self.set_theme(if dark { Theme::dark() } else { Theme::light() });
    }

    /// Replaces the active theme wholesale, keeping the fallback
    /// highlighter's palette in step — the path for a face whose editor
    /// holds a theme that is not one of the built-in presets.
    pub fn set_theme(&mut self, theme: Theme) {
        // The keyword bridge's palette is its own type, keyed to darkness
        // rather than to the theme's syntax colors.
        if theme.is_dark {
            self.highlighter.set_dark_theme();
        } else {
            self.highlighter.set_light_theme();
        }
        self.theme = theme;
        // Text colors and the fallback highlighter's palette both feed the
        // shaped buffers; a set_theme that skipped this bump would serve
        // stale-colored retained frames.
        self.theme_generation = self.theme_generation.wrapping_add(1);
    }

    /// The active theme, for hosts that derive colors from it (e.g. gutter
    /// change-bar kinds).
    pub const fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Enables or disables syntax highlighting.
    pub const fn set_syntax_enabled(&mut self, enabled: bool) {
        self.syntax_enabled = enabled;
    }

    /// Whether syntax highlighting is enabled.
    pub const fn syntax_enabled(&self) -> bool {
        self.syntax_enabled
    }

    /// Enables or disables the gutter (line numbers).
    ///
    /// Bumps the gutter generation unconditionally: toggling changes both
    /// the gutter buffer's existence and the content column's width, and a
    /// redundant call merely costs one rebuilt frame.
    pub const fn set_gutter_enabled(&mut self, enabled: bool) {
        self.gutter_enabled = enabled;
        self.gutter_text_generation = self.gutter_text_generation.wrapping_add(1);
    }

    /// Whether the gutter is enabled.
    pub const fn gutter_enabled(&self) -> bool {
        self.gutter_enabled
    }

    /// The capture-name → color map the face's highlight resolver reads,
    /// mutable so the host can replace it in place.
    ///
    /// Exposed as the raw map — rather than a setter taking a finished map —
    /// so a host that fails partway through parsing its theme leaves exactly
    /// the partial state it always did.
    ///
    /// Every mutable borrow bumps the syntax-theme generation: the raw
    /// accessor defeats change tracking, so the borrow itself is the
    /// change signal. It over-invalidates only when the host actually
    /// calls this, which coincides with real theme changes.
    pub const fn syntax_theme_mut(&mut self) -> &mut HashMap<String, Color> {
        self.syntax_theme_generation = self.syntax_theme_generation.wrapping_add(1);
        &mut self.syntax_theme
    }

    /// Per-line background colors (`doc_line` → color) for diff
    /// highlighting, mutable for in-place host updates.
    pub const fn line_backgrounds_mut(&mut self) -> &mut HashMap<usize, Color> {
        &mut self.line_backgrounds
    }

    /// Per-line gutter change bar colors (`doc_line` → color), mutable for
    /// in-place host updates.
    pub const fn gutter_changes_mut(&mut self) -> &mut HashMap<usize, Color> {
        &mut self.gutter_changes
    }

    /// Replaces the custom gutter text: one string per document line, or
    /// `None` to restore automatic line numbers.
    ///
    /// Bumps the gutter generation unconditionally — custom gutter text
    /// changes both the gutter buffer's text and (through its measured
    /// width) the content column — which also makes this call the honest
    /// way for a host to force a full rebuild of the retained shapes.
    pub fn set_custom_gutter_lines(&mut self, lines: Option<Vec<String>>) {
        self.custom_gutter_lines = lines;
        self.gutter_text_generation = self.gutter_text_generation.wrapping_add(1);
    }

    /// Per-line blame text (`doc_line` → formatted string), mutable for
    /// in-place host updates. Only the cursor line's entry is rendered.
    pub const fn blame_data_mut(&mut self) -> &mut HashMap<usize, String> {
        &mut self.blame_data
    }
}
