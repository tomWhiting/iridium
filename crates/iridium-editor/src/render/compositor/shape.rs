//! The retained-shaping cache: its key, the shapes it guards, and the
//! geometry a rebuild works against.

use glyphon::Buffer;

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
pub(super) struct ShapeKey {
    /// Document content (`Document::revision`, moved by every text
    /// mutation and never by cursor motion).
    pub(super) document_revision: u64,
    /// First document line included in the viewport buffer.
    pub(super) viewport_start: usize,
    /// One past the last document line included in the viewport buffer.
    pub(super) viewport_end: usize,
    /// Wrap width in pixels, as `f32::to_bits`.
    pub(super) content_width: u32,
    /// Font size in pixels, as `f32::to_bits`.
    pub(super) font_size: u32,
    /// Line height multiplier, as `f32::to_bits`.
    pub(super) line_height_factor: u32,
    /// Loaded-font-data generation
    /// ([`FrameCompositor::load_font`](super::FrameCompositor::load_font)).
    pub(super) font_generation: u64,
    /// Theme generation
    /// ([`FrameCompositor::set_theme`](super::FrameCompositor::set_theme)).
    pub(super) theme_generation: u64,
    /// Whether syntax highlighting is enabled.
    pub(super) syntax_enabled: bool,
    /// The face's [`HighlightSource::language_active`](super::HighlightSource::language_active)
    /// answer. It selects between the keyword-fallback and plain arms when the
    /// source resolves `None`, so a language set or unset at runtime reshapes
    /// even when no other input moved.
    pub(super) language_active: bool,
    /// The face's [`HighlightSource::generation`](super::HighlightSource::generation).
    pub(super) highlight_generation: u64,
    /// Capture-name map generation
    /// ([`FrameCompositor::syntax_theme_mut`](super::FrameCompositor::syntax_theme_mut)).
    pub(super) syntax_theme_generation: u64,
    /// The `FoldState::generation` — folds never move the document
    /// revision, so they need their own key member.
    pub(super) fold_generation: u64,
    /// Custom gutter text / gutter enablement generation
    /// ([`set_custom_gutter_lines`](super::FrameCompositor::set_custom_gutter_lines),
    /// [`set_gutter_enabled`](super::FrameCompositor::set_gutter_enabled)).
    pub(super) gutter_text_generation: u64,
}

impl ShapeKey {
    /// Whether a miss against `previous` may be served by per-line diffing
    /// (stage 2a): only the document content and/or the highlight answer
    /// moved, while every input the buffer's geometry, metrics and
    /// attribute construction depend on — including the viewport range,
    /// whose change re-maps every buffer line to a different document
    /// line — is unchanged.
    pub(super) const fn permits_line_diff(&self, previous: &Self) -> bool {
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
pub(super) struct RetainedShape {
    /// The inputs the buffers were built from.
    pub(super) key: ShapeKey,
    /// The main content buffer, shaped.
    pub(super) buffer: Buffer,
    /// The line-number buffer, shaped; `None` when the gutter is off.
    pub(super) gutter: Option<Buffer>,
    /// The gutter width (as `f32::to_bits`) the gutter buffer was created
    /// with. The key's `content_width` almost always pins this too, but a
    /// simultaneous surface-width and gutter-width change could cancel out
    /// in the subtraction — the gutter buffer's own width is compared
    /// directly rather than inferred.
    pub(super) gutter_width_bits: u32,
}

/// The per-frame geometry a retained-shape rebuild works against — the
/// values [`FrameCompositor::compose`](super::FrameCompositor::compose)
/// derives before consulting the key.
pub(super) struct RebuildGeometry {
    /// First document line in the viewport buffer.
    pub(super) viewport_start: usize,
    /// One past the last document line in the viewport buffer.
    pub(super) viewport_end: usize,
    /// Document byte offset where the visible content begins.
    pub(super) viewport_start_byte: usize,
    /// Wrap width in pixels for the content buffer.
    pub(super) content_width: f32,
    /// Gutter width in pixels for the gutter buffer.
    pub(super) gutter_width: f32,
    /// Left edge of the content column.
    pub(super) content_offset_x: f32,
    /// Line height in pixels.
    pub(super) line_height: f32,
    /// Total document line count.
    pub(super) line_count: usize,
}
