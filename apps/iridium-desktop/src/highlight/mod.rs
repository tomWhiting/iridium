//! The desktop face's side of the compositor's highlight seam: the kernel's
//! tree-sitter spans, cached per parse generation, resolved per frame.
//!
//! Nothing here parses. The kernel keeps one parse tree for the document and
//! refreshes it after every content change (`EditorState::refresh_syntax`,
//! which every mutating command runs); [`HighlightCache`] reads that tree,
//! turns it into an indexed span set once per generation, and
//! [`FrameHighlights`] resolves those spans onto one frame's visible content.
//!
//! # The spans cover a window, not the document
//!
//! Deriving every span of a 10k-line file on every parse generation was the
//! parser tax (docs/design/PARSER-TAX-MAP.md): ~30ms of each keystroke spent
//! walking the whole tree for a viewport that paints ~90 lines. The windowed
//! derive that fixed it — viewport widened by [`OVERSCAN_LINES`] each way,
//! rebuild only when the parse generation moves *or* the requested viewport
//! escapes the covered window — is the kernel's
//! [`WindowedSpanCache`](iridium_editor::span_index::WindowedSpanCache),
//! shared with the terminal face (the parser-tax map's R2 ruling);
//! [`HighlightCache`] is this face's thin handle on it. Spans straddling
//! the window's edges arrive whole from the kernel
//! ([`Highlighter::spans_in_range`](iridium_editor::syntax::Highlighter::spans_in_range)'s
//! boundary contract) and the resolver clamps them to visible content
//! exactly as it always has.
//!
//! Only the resolution step is this face's own — a GPU frame wants coloured
//! text runs where the terminal's cell frame wants per-cluster styles — so
//! [`FrameHighlights`] is what this module keeps beside the handle.
//!
//! # Colours are the kernel's
//!
//! Each span's [`HighlightType`](iridium_editor::syntax::HighlightType) is
//! mapped through the kernel's own
//! [`highlight_to_color`](iridium_editor::syntax::highlight_to_color) against the theme's
//! `SyntaxColors` — the mapping the terminal face paints with. The
//! [`HighlightContext`](iridium_editor::render::HighlightContext)'s
//! capture-name string map is deliberately not
//! consulted: it exists for the web face, whose spans arrive from JavaScript
//! as capture-name strings; this face's spans are the kernel's typed spans,
//! and a string detour would add a lookup that can miss to a mapping that
//! cannot.
//!
//! # What a frame's resolution promises
//!
//! The runs cover the visible content exactly — gaps take the theme
//! foreground — and are flat: where spans nest, **the innermost span owns the
//! byte** and the span around it keeps everything the inner one does not take.
//! That collapse is the kernel's
//! [`flatten_spans`](iridium_editor::render::flatten_spans), not this face's,
//! because the web face needs the identical rule and had the identical bug.
//!
//! Span offsets are document-absolute while the content is the compositor's
//! fold-collapsed extraction, so offsets past a collapsed fold can drift by the
//! placeholder text's length; they are clamped and snapped to character
//! boundaries, never trusted. That guard is the kernel's
//! [`snap_down`](iridium_editor::render::snap_down) now, for the same reason —
//! the web resolvers did not have it and would have panicked on the first fold
//! drift over non-ASCII text.
//!
//! # Where the pieces live
//!
//! Split on the seam the module is already about — what survives between frames
//! against what is built for one:
//!
//! - `cache` — [`HighlightCache`], the handle on the kernel's windowed cache
//!   and the gates that decide whether anything is rebuilt.
//! - `resolve` — [`FrameHighlights`] and the span-to-run resolution, borrowed
//!   for exactly one `compose` call.
//! - `tests` — the suite, divided the same way: `frame` for what one frame
//!   resolves to, `window` for what the windowed derive covers.

mod cache;
mod resolve;

#[cfg(test)]
mod tests;

pub use cache::HighlightCache;
pub use resolve::FrameHighlights;

// The kernel's overscan margin, re-exported so this face's viewport docs
// (`App::viewport_window`) can name it where its slack is absorbed. One
// definition, kernel-owned — the parser-tax map's R1 ruling.
pub use iridium_editor::span_index::OVERSCAN_LINES;
