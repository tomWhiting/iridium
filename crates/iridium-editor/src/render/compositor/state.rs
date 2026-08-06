//! The compositor's fields and how one is built.
//!
//! Everything the other modules in [`super`] read lives here, documented
//! once. The fields are `pub(super)` rather than private because the passes
//! that use them are siblings: the type is one unit of state whose *methods*
//! are split by when they run, not a type with a boundary through its middle.

use std::collections::HashMap;

use wgpu::{Device, Queue, TextureFormat};

use super::metrics::{MAX_SELECTION_QUADS, MAX_VIEWPORT_LINES};
use super::shape::RetainedShape;
use crate::editor::IridiumError;
use crate::render::cursor::CursorRenderer;
use crate::render::gutter::GutterRenderer;
use crate::render::quad::{Quad, QuadRenderer};
use crate::render::simple_highlight::SimpleHighlighter;
use crate::render::text::TextRenderer;
use crate::render::viewport::ViewportConfig;
use crate::theme::{Color, Theme};

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
    pub(super) text_renderer: TextRenderer,
    /// Quad renderer for background elements (gutter, selection highlights).
    pub(super) background_quad_renderer: QuadRenderer,
    /// Quad renderer for foreground elements (cursor) — separate buffer to
    /// avoid GPU conflicts: two `queue.write_buffer` calls to one buffer
    /// within a frame leave only the last write visible.
    pub(super) cursor_quad_renderer: QuadRenderer,
    /// Caret blink state, shared by every caret on purpose.
    pub(super) cursor_renderer: CursorRenderer,
    /// Gutter width and digit-column calculations.
    pub(super) gutter_renderer: GutterRenderer,
    /// Built-in keyword highlighter, the bridge for a language-active face
    /// whose [`HighlightSource`](super::HighlightSource) has no spans this
    /// frame. It never touches a document without a language — that renders
    /// plain.
    pub(super) highlighter: SimpleHighlighter,
    /// The active theme.
    pub(super) theme: Theme,
    /// Whether syntax highlighting is enabled.
    pub(super) syntax_enabled: bool,
    /// Whether the gutter (line numbers) is enabled.
    pub(super) gutter_enabled: bool,
    /// Pixels of window reserved above the document, and the Y origin every
    /// layout query measures from.
    ///
    /// Ten by default — the content's own breathing room, which is all a
    /// face with no chrome above the text needs. A face that draws a band
    /// there, such as the desktop face's tab strip, adds its height here so
    /// that painting, hit-testing, scroll-to-caret and the scroll clamp all
    /// move together. **Held once and read by all four**, because an offset
    /// applied in the painter but not in the hit test agrees with the truth
    /// exactly at the top of the document and is a row out everywhere else.
    pub(super) top_inset: f32,
    /// The X origin the gutter is drawn from, and the left edge every
    /// horizontal layout query measures from.
    ///
    /// Zero by default — a face with no chrome beside the text reserves
    /// nothing, and the content's own breathing room is the gutter's width
    /// plus [`HORIZONTAL_PADDING`](super::metrics::HORIZONTAL_PADDING), which
    /// sits to the *right* of this. A face that draws a band there, such as a
    /// sidebar or a file tree, puts its width here so that the gutter
    /// background, the line numbers, the change bars, the content column and
    /// both hit-test directions all move together. Held once and read by all
    /// of them, for the same reason [`Self::top_inset`] is.
    pub(super) left_inset: f32,
    /// Cached character width (measured from actual font metrics).
    pub(super) cached_char_width: f32,
    /// Viewport configuration for the overscan buffer.
    pub(super) viewport_config: ViewportConfig,
    /// Cached viewport dimensions to avoid redundant GPU uniform updates.
    pub(super) cached_viewport_width: u32,
    /// See [`Self::cached_viewport_width`].
    pub(super) cached_viewport_height: u32,

    // =========================================================================
    // Retained shaping: the shaped buffers survive across frames
    // =========================================================================
    /// The shaped viewport buffers from the last frame, with the exact
    /// inputs they were shaped under; `None` before the first frame. A
    /// frame whose [`ShapeKey`](super::shape::ShapeKey) matches skips content
    /// extraction, highlight resolution and all shaping.
    pub(super) retained: Option<RetainedShape>,
    /// Bumped by [`Self::load_font`]: new font data can change how
    /// `Family::Monospace` resolves, which changes every shaped glyph.
    pub(super) font_generation: u64,
    /// Bumped by [`Self::set_theme`] (and through it
    /// [`Self::set_dark_theme`]): the theme feeds text colors and the
    /// fallback highlighter's palette.
    pub(super) theme_generation: u64,
    /// Bumped by every mutable borrow of [`Self::syntax_theme_mut`]: the
    /// raw accessor defeats change tracking, so the borrow itself is the
    /// change signal (over-invalidating only when the host actually calls
    /// it, which coincides with real changes).
    pub(super) syntax_theme_generation: u64,
    /// Bumped by [`Self::set_custom_gutter_lines`] and
    /// [`Self::set_gutter_enabled`]: custom gutter text and gutter
    /// enablement change both the gutter buffer's text and the content
    /// column's width.
    pub(super) gutter_text_generation: u64,
    /// How many times compose has run the rebuild path — the observable
    /// that makes every cache claim testable without reading pixels.
    pub(super) shape_rebuilds: u64,
    /// How many buffer lines the per-line diffing rebuild path has
    /// actually reshaped, across the content and gutter buffers.
    pub(super) lines_reshaped: u64,

    // =========================================================================
    // Pre-allocated buffers for compose() — eliminates per-frame allocations
    // =========================================================================
    /// Pre-allocated buffer for the visible content string.
    /// Capacity: ~10KB (typical viewport worth of text).
    pub(super) cpu_visible_content: String,
    /// Pre-allocated buffer for document-to-visual line mapping.
    /// Capacity: document line count (grows as needed).
    pub(super) cpu_doc_to_visual: Vec<Option<usize>>,
    /// Pre-allocated buffer for visible document line indices.
    /// Capacity: [`MAX_VIEWPORT_LINES`].
    pub(super) cpu_visible_doc_lines: Vec<usize>,
    /// Pre-allocated buffer for the line numbers string.
    /// Capacity: ~2KB (typical viewport worth of line numbers).
    pub(super) cpu_line_numbers: String,
    /// Pre-allocated buffer for gutter background quads.
    /// Capacity: 1-2 quads typically.
    pub(super) cpu_gutter_quads: Vec<Quad>,
    /// Pre-allocated buffer for selection highlight quads.
    /// Capacity: [`MAX_SELECTION_QUADS`].
    pub(super) cpu_selection_quads: Vec<Quad>,
    /// Pre-allocated buffer for cursor quads.
    /// Capacity: 1-2 quads typically.
    pub(super) cpu_cursor_quads: Vec<Quad>,
    /// Pre-allocated buffer for combined background quads.
    /// Capacity: gutter + selection quads.
    pub(super) cpu_background_quads: Vec<Quad>,
    /// Pre-allocated buffer for line background quads.
    pub(super) cpu_line_bg_quads: Vec<Quad>,
    /// Pre-allocated buffer for gutter change bar quads.
    pub(super) cpu_gutter_change_quads: Vec<Quad>,
    /// Pre-allocated buffer for blame text rendering.
    pub(super) cpu_blame_content: String,

    // =========================================================================
    // Cached layout info for pixel-to-position mapping with word wrap
    // =========================================================================
    /// Cached visual line mapping built during compose.
    /// Each entry maps a visual line to (`buffer_line_index`, `start_column_in_line`).
    pub(super) cached_visual_line_map: Vec<(usize, usize)>,
    /// The `viewport_start` value when `cached_visual_line_map` was built.
    pub(super) cached_map_viewport_start: usize,
    /// Content offset X when the map was built.
    pub(super) cached_content_offset_x: f32,

    // =========================================================================
    // Cached scroll data for word-wrap-aware scrolling
    // =========================================================================
    /// Total visual lines including wrapped sub-lines, updated each frame.
    pub(super) cached_total_visual_lines: usize,
    /// Absolute cursor Y position (document space) from the last frame.
    pub(super) cached_cursor_abs_y: f32,
    /// The cursor doc line when `cached_cursor_abs_y` was computed.
    pub(super) cached_cursor_doc_line: usize,

    // =========================================================================
    // Presentation inputs pushed by the host between frames
    // =========================================================================
    /// Map of capture name → color, consumed by the face's highlight resolver
    /// through [`HighlightContext::syntax_theme`](super::HighlightContext::syntax_theme).
    pub(super) syntax_theme: HashMap<String, Color>,
    /// Per-line background colors (`doc_line` → color). Used for diff
    /// highlighting.
    pub(super) line_backgrounds: HashMap<usize, Color>,
    /// Per-line gutter change bar colors (`doc_line` → color). Used for
    /// change indicators.
    pub(super) gutter_changes: HashMap<usize, Color>,
    /// Custom gutter text lines (replaces auto line numbers when `Some`).
    pub(super) custom_gutter_lines: Option<Vec<String>>,
    /// Per-line blame text (`doc_line` → formatted blame string). Shown at
    /// the end of the cursor line.
    pub(super) blame_data: HashMap<usize, String>,
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
            top_inset: Self::DOCUMENT_TOP_PADDING,
            left_inset: 0.0,
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
}
