//! The retained-shaping proof harness for [`FrameCompositor::compose`].
//!
//! Two obligations, from `docs/design/RETAINED-SHAPING-MAP.md` §5:
//!
//! - **The staleness matrix.** For every input of the shape key (§2 of the
//!   map): mutating it makes the next compose a rebuild, observable through
//!   `shape_rebuilds()`; and the inputs deliberately *outside* the key — an
//!   identical frame, a sub-line scroll, blink phase, the per-frame
//!   presentation maps — produce hits.
//! - **Pixel identity, hot vs cold.** The cache may only change *when* work
//!   happens, never *what* is produced: a hit frame is byte-identical to
//!   the cold frame before it, and every warm-after-invalidation frame is
//!   byte-identical to a fresh compositor composing the same state cold.
//!   Determinism is arranged, not hoped for — the face font is loaded from
//!   bytes, the metrics are fixed, and the blink is pinned by
//!   `reset_blink()` immediately before every compared compose.
//!
//! Headless by construction, exactly like `benches/compose_frame.rs`: no
//! surface anywhere, an offscreen texture per test, and a missing GPU
//! adapter fails the test loudly rather than passing over work that did
//! not happen.
//!
//! # Where the rows live
//!
//! Split by **which input of the shape key the row mutates**, because that is
//! the axis a defect appears along: a compositor that stopped keying the fold
//! generation breaks the fold rows and nothing else.
//!
//! - `harness` — the fixtures, and the single path every comparand is built
//!   by. Nothing here is a test.
//! - `hits` — the inputs deliberately *outside* the key.
//! - `typing` — the miss side, stage 2a: the edit path.
//! - `layout` — scroll, target size, font face, font size.
//! - `theme` — the palette the text and the fallback highlighter draw from.
//! - `syntax` — the language answer, the spans, and their generation.
//! - `gutter` — folds, custom gutter lines, and the digit rollover.

#[path = "../support/mod.rs"]
mod support;

mod harness;

mod document_identity;
mod gutter;
mod hits;
mod layout;
mod syntax;
mod theme;
mod typing;
