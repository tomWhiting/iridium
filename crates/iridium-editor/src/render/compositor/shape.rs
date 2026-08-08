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
///
/// The tab width **is** a member, as of #86. It was not, for as long as
/// nothing set it: row 11 of the map kept `Wrap`, the tab width and `Shaping`
/// out on the grounds that they sat at their cosmic-text defaults. That
/// stopped being true when `EditorConfig::tab_width` was wired through to the
/// renderer, so literal tabs measure at the configured width instead of a
/// fixed eight — and a value baked into a buffer at shaping time has to be
/// keyed, or changing it is a cache *hit* and nothing re-measures.
///
/// `Wrap` and `Shaping` are still out, on the original grounds. Soft wrap
/// remains row 11's other trigger.
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
    /// How many character advances a literal tab occupies
    /// ([`EditorConfig::tab_width`](crate::EditorConfig::tab_width), floored
    /// at 1).
    ///
    /// A key member because tab width is baked into the buffer at shaping
    /// time: without it, editing the setting would be a cache *hit*, nothing
    /// would re-measure, and the retained buffers would keep the old columns
    /// while the editing side had already moved. Content that holds no tab
    /// shapes identically either way, so the cost of keying it is one
    /// avoidable rebuild on a setting nobody changes mid-session.
    pub(super) tab_width: u16,
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
            tab_width,
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
            // Refused rather than diffed: the per-line path reuses the
            // *previous* buffer, and a buffer carries its tab width from
            // construction. Diffing text into it would leave every tab
            // measured at the old width with no line marked dirty for it.
            && tab_width == previous.tab_width
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

#[cfg(test)]
mod shape_key_tests {
    use super::ShapeKey;

    /// A key with every input at a fixed value, for tests that vary one.
    fn key() -> ShapeKey {
        ShapeKey {
            document_revision: 7,
            viewport_start: 0,
            viewport_end: 40,
            content_width: 800.0_f32.to_bits(),
            font_size: 14.0_f32.to_bits(),
            line_height_factor: 1.4_f32.to_bits(),
            font_generation: 1,
            theme_generation: 1,
            syntax_enabled: true,
            language_active: true,
            highlight_generation: 3,
            syntax_theme_generation: 1,
            fold_generation: 1,
            gutter_text_generation: 1,
            tab_width: 4,
        }
    }

    #[test]
    fn an_unchanged_key_permits_the_line_diff() {
        // The control. Without it the assertions below would pass just as well
        // against a `permits_line_diff` that always refused.
        assert!(key().permits_line_diff(&key()));
    }

    #[test]
    fn a_content_change_alone_permits_the_line_diff() {
        // The other control, and the reason the fast path exists: typing moves
        // the revision and the highlight generation and nothing else, which is
        // exactly the case per-line diffing is for.
        let mut next = key();
        next.document_revision = 8;
        next.highlight_generation = 4;
        assert!(next.permits_line_diff(&key()));
    }

    #[test]
    fn a_tab_width_change_refuses_the_line_diff() {
        // The load-bearing one. A `Buffer` carries the tab width it was
        // constructed with, and the per-line path *reuses the previous
        // buffer* — so diffing text into it would leave every tab measured at
        // the old width, with no line marked dirty to say so. The full rebuild
        // is what calls `create_buffer` again.
        //
        // ⚠️ Adding `tab_width` to the key alone does not fix the bug: the key
        // miss would route here, and this predicate would wave it through to
        // the stale buffer. This test is the difference.
        let mut next = key();
        next.tab_width = 8;
        assert!(
            !next.permits_line_diff(&key()),
            "a tab width change must force a full reshape, not a per-line diff"
        );
    }

    #[test]
    fn the_floor_keeps_the_key_and_the_renderer_agreeing() {
        // `compose` and `TextRenderer::set_tab_width` both route a requested
        // width through `usable_tab_width`, so a configured zero reaches the
        // key and the buffer as the same 1. Were only one of them to floor,
        // the key would describe a width nothing was shaped at.
        use crate::render::text::usable_tab_width;

        assert_eq!(usable_tab_width(0), 1, "zero is floored, never passed on");
        assert_eq!(usable_tab_width(1), 1);
        assert_eq!(usable_tab_width(4), 4);
        assert_eq!(usable_tab_width(u16::MAX), u16::MAX);
    }
}
