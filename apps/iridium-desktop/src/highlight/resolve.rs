//! The resolution side of the seam: what one frame is painted from.
//!
//! [`FrameHighlights`] borrows the cache built in [`super::cache`] for exactly
//! one `compose` call, and [`rich_spans`] turns the spans it finds into the
//! coloured runs the compositor draws.

use iridium_editor::render::{
    HighlightContext, HighlightSource, RunStyle, SpanRun, flatten_spans, snap_down,
};
use iridium_editor::span_index::SpanIndex;
use iridium_editor::syntax::highlight_to_style;
use iridium_editor::theme::{SyntaxColors, SyntaxEmphasis};

/// The per-frame [`HighlightSource`]: the cached span index and the theme's
/// syntax colours, borrowed for exactly one `compose` call.
///
/// The fields are `pub(super)` rather than private because
/// [`HighlightCache::resolver`](super::HighlightCache::resolver) builds one and
/// lives in the sibling module — visible across `highlight` and nowhere else,
/// so the type is still opaque to the rest of the face.
#[derive(Debug)]
pub struct FrameHighlights<'a> {
    /// The indexed spans, absent when no language (or no working highlighter)
    /// is set.
    pub(super) index: Option<&'a SpanIndex>,
    /// The theme's syntax colours, mapped per highlight by the kernel.
    pub(super) colors: &'a SyntaxColors,
    /// The theme's emphasis table — the categories it draws in a face other
    /// than body text. Empty in both shipped presets, and empty is a complete
    /// answer rather than an unset one.
    pub(super) emphasis: &'a SyntaxEmphasis,
    /// The owning [`HighlightCache`](super::HighlightCache)'s generation at
    /// construction.
    pub(super) generation: u64,
    /// The owning cache's language answer at construction — see
    /// [`HighlightCache::language_active`](super::HighlightCache::language_active).
    pub(super) language_active: bool,
}

impl HighlightSource for FrameHighlights<'_> {
    fn resolve<'a>(&mut self, context: &HighlightContext<'a>) -> Option<Vec<(&'a str, RunStyle)>> {
        let index = self.index?;
        Some(rich_spans(index, context, self.colors, self.emphasis))
    }

    fn language_active(&self) -> bool {
        self.language_active
    }

    fn generation(&self) -> u64 {
        self.generation
    }
}

/// Resolves the indexed spans against one frame's visible content: styled
/// runs in order, gaps filled with the theme foreground, the whole content
/// covered.
///
/// ⚠️ **The weight and slant come from the theme, never from here.** Inventing
/// a rule in this face ("italicise comments") would be it deciding something
/// the theme owns, and the GPU face and the terminal face would then disagree
/// about what a comment looks like. Both call
/// [`highlight_to_style`](iridium_editor::syntax::highlight_to_style), which is
/// the single place a category becomes an appearance.
///
/// A gap between spans still takes the plain theme foreground and no emphasis:
/// it belongs to no category, so there is nothing for a theme to have an
/// opinion about.
fn rich_spans<'a>(
    index: &SpanIndex,
    context: &HighlightContext<'a>,
    colors: &SyntaxColors,
    emphasis: &SyntaxEmphasis,
) -> Vec<(&'a str, RunStyle)> {
    let visible = context.content;
    let start_byte = context.content_start_byte;
    let end_byte = start_byte.saturating_add(visible.len());

    // Snapped here rather than inside the flattening: the drift is this face's
    // (document offsets against fold-collapsed content), and the kernel's rule
    // is about which span owns a byte, not about where a byte begins. A span
    // that snapping empties is discarded by `flatten_spans` along with any the
    // window handed over degenerate.
    let spans: Vec<SpanRun<_>> = index
        .query(start_byte, end_byte)
        .map(|span| SpanRun {
            start: snap_down(visible, span.start.saturating_sub(start_byte)),
            end: snap_down(visible, span.end.saturating_sub(start_byte)),
            payload: span.highlight,
        })
        .collect();

    // Order is left as the index yields it: `flatten_spans` sorts stably and
    // resolves identical ranges in favour of the first, which is the rule
    // `Highlighter::spans_with` already applied when the spans were emitted.
    flatten_spans(spans, visible.len())
        .into_iter()
        .map(|run| {
            let style = run.payload.map_or_else(
                || RunStyle::plain(context.foreground),
                |highlight| highlight_to_style(highlight, colors, emphasis),
            );
            (&visible[run.start..run.end], style)
        })
        .collect()
}
