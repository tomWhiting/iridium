//! The syntax seam: what a face's highlight resolver is handed, and what it
//! owes back.

use std::collections::HashMap;

use crate::render::viewport::ViewportConfig;
use crate::theme::Color;

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
/// [`FrameCompositor::compose`](super::FrameCompositor::compose) calls this
/// once per frame, only when syntax highlighting is enabled. Returning `Some`
/// paints the given spans (slices of [`HighlightContext::content`] paired with
/// colors, in order, gaps included); returning `None` says "I have nothing
/// this frame", and what the compositor does next is [`Self::language_active`]'s
/// answer: with a language set the built-in keyword highlighter bridges the
/// span-less frame, with none the content renders plain — the same decision
/// the web face always made, split along ownership: the tree-sitter paths live
/// with the face that owns the spans, the fallback paths live with the
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
    /// host's [`set_syntax_enabled`](super::FrameCompositor::set_syntax_enabled)
    /// channel.
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
