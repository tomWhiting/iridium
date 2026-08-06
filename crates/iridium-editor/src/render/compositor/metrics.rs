//! Layout constants, and the per-frame values derived from them.

/// Maximum expected lines in viewport (for pre-allocation sizing).
/// Typical: 50 lines visible + 20 overscan = 70 lines.
pub(super) const MAX_VIEWPORT_LINES: usize = 128;

/// Maximum expected selection quads per frame.
/// Typical: 1 quad per selected line, rarely more than viewport.
pub(super) const MAX_SELECTION_QUADS: usize = 128;

/// The content column's inset from the gutter, in pixels.
pub(super) const HORIZONTAL_PADDING: f32 = 10.0;

/// The line numbers' own breathing room inside the gutter, measured from the
/// gutter's left edge rather than the window's.
pub(super) const GUTTER_TEXT_PADDING: f32 = 8.0;

/// How far a change indicator bar sits from the gutter's left edge.
pub(super) const CHANGE_BAR_INSET: f32 = 2.0;

/// How wide a change indicator bar is drawn.
pub(super) const CHANGE_BAR_WIDTH: f32 = 3.0;

/// Pixels of empty window left below the last line when scrolled to the end,
/// so it does not sit flush against the edge.
pub(super) const BOTTOM_SLACK: f32 = 10.0;

/// Per-frame layout values shared by the quad-building passes.
///
/// These are all derived at the top of
/// [`FrameCompositor::compose`](super::FrameCompositor::compose) and read
/// everywhere below it; carrying them as one value keeps the pass methods'
/// signatures honest about what they consume.
#[derive(Debug, Clone, Copy)]
pub(super) struct FrameMetrics {
    /// Line height in pixels.
    pub(super) line_height: f32,
    /// Character advance width in pixels.
    pub(super) char_width: f32,
    /// The Y origin the document is drawn from:
    /// [`FrameCompositor::top_inset`](super::FrameCompositor::top_inset).
    pub(super) top_inset: f32,
    /// Left edge of the content column: the reserved inset, the gutter's
    /// width, then padding.
    pub(super) content_offset_x: f32,
    /// The X origin the gutter is drawn from:
    /// [`FrameCompositor::left_inset`](super::FrameCompositor::left_inset).
    pub(super) left_inset: f32,
    /// Pixel offset of the viewport buffer's first line in document space.
    pub(super) virtual_scroll_offset: f32,
    /// First document line included in the viewport buffer.
    pub(super) viewport_start: usize,
    /// Visual index (fold-aware) of `viewport_start`.
    pub(super) viewport_start_visual: usize,
    /// The face's vertical scroll offset in pixels.
    pub(super) scroll_y: f32,
    /// Surface width in pixels.
    pub(super) surface_width: f32,
    /// Surface height in pixels.
    pub(super) surface_height: f32,
}
