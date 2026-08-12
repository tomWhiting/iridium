//! The cache side of the seam: what survives between frames.
//!
//! [`HighlightCache`] is this face's handle on the kernel's
//! [`WindowedSpanCache`] — the gates that decide whether a parse generation or
//! a viewport move costs a span rebuild. Nothing here runs per frame; see
//! [`super::resolve`] for the half that does.

use std::ops::Range;

use iridium_editor::Editor;
use iridium_editor::span_index::WindowedSpanCache;
use iridium_editor::theme::Theme;

use super::resolve::FrameHighlights;

/// The cached parse-derived state frames are resolved from: this face's
/// handle on the kernel's [`WindowedSpanCache`].
///
/// One per session, owned by the app beside the editor.
/// [`Self::refresh_windowed`] runs before every frame and costs a generation
/// and window comparison when nothing changed; the span rebuild — over the
/// covered window only, never the whole document — happens only when the
/// kernel actually reparsed or the viewport escaped the cover. The gates,
/// the window arithmetic and the derive all live in the kernel; what this
/// type adds is [`Self::resolver`], the compositor-facing seam.
#[derive(Debug, Default)]
pub struct HighlightCache {
    /// The hoisted cache: spans, covered window, generation gates.
    spans: WindowedSpanCache,
}

impl HighlightCache {
    /// An empty cache: no language, no spans, resolvers that answer `None`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            spans: WindowedSpanCache::new(),
        }
    }

    /// Brings the cache up to date with the kernel's tree over the whole
    /// document — [`Self::refresh_windowed`] with a window covering every
    /// line.
    ///
    /// The convenience form for callers without a viewport (headless states,
    /// tests): its derive is O(document), which is exactly the cost the
    /// windowed form exists to avoid, so the per-frame path must pass its
    /// real viewport instead.
    pub fn refresh(&mut self, editor: &Editor) {
        self.spans.refresh(editor);
    }

    /// Brings the cache up to date with the kernel's tree, reusing everything
    /// it can, deriving spans for `viewport_lines` (document lines, half-open)
    /// widened by [`OVERSCAN_LINES`](super::OVERSCAN_LINES) each way.
    ///
    /// The rebuild gates and their meaning for this face: with no language
    /// set the cache empties and reports the language inactive — the
    /// compositor renders the plain foreground. A language whose bundled
    /// highlight query does not compile empties the cache but keeps the
    /// language active — it degrades to the compositor's keyword bridge.
    /// Otherwise the spans are rebuilt only when the parse count or document
    /// revision moved *or* the requested viewport escaped the covered
    /// window; see [`WindowedSpanCache::refresh_windowed`].
    pub fn refresh_windowed(&mut self, editor: &Editor, viewport_lines: Range<usize>) {
        self.spans.refresh_windowed(editor, viewport_lines);
    }

    /// How many times the spans have been rebuilt since the cache was
    /// created.
    ///
    /// Exists so the generation gate is testable from outside: refreshing
    /// without an intervening edit must leave this unchanged.
    #[must_use]
    pub const fn rebuilds(&self) -> u64 {
        self.spans.rebuilds()
    }

    /// The generation the per-frame resolver reports to the compositor —
    /// see [`WindowedSpanCache::generation`] for what moves it.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.spans.generation()
    }

    /// Whether the kernel had a language set at the last [`Self::refresh`]
    /// — the answer the resolver carries as
    /// [`HighlightSource::language_active`](iridium_editor::render::HighlightSource::language_active).
    #[must_use]
    pub const fn language_active(&self) -> bool {
        self.spans.language_active()
    }

    /// One frame's resolver over the cached spans, coloured from `colors`.
    ///
    /// Borrows the cache immutably, so it can be handed to
    /// [`FrameCompositor::compose`](iridium_editor::render::FrameCompositor)
    /// alongside mutable borrows of the surface and compositor.
    ///
    /// The resolver's generation is this cache's. The **theme** is a second
    /// input to the resolution that the generation does not cover: today
    /// the desktop face sets its theme once at startup and never again, so
    /// it cannot change under a retained frame — if runtime theme
    /// switching ever lands, the switch must move this generation too, or
    /// retained frames keep the old palette.
    ///
    /// Takes the whole theme rather than its `syntax` field alone, because
    /// resolution now reads two parts of it — the colours and the emphasis
    /// table — and passing them separately is one more thing eleven call
    /// sites could get out of step.
    #[must_use]
    pub fn resolver<'a>(&'a self, theme: &'a Theme) -> FrameHighlights<'a> {
        FrameHighlights {
            index: self.spans.index(),
            colors: &theme.syntax,
            emphasis: &theme.emphasis,
            generation: self.spans.generation(),
            language_active: self.spans.language_active(),
        }
    }
}
