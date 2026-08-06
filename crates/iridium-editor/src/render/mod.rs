//! Rendering subsystem: pure layout, plus an optional GPU back end.
//!
//! The module is split along a hard seam:
//!
//! - **Pure layout** — the `cursor`, `gutter`, `highlight`, `minimap`,
//!   `simple_highlight` and `viewport` submodules compute geometry, spans and
//!   rectangles from document state alone. They depend only on the text buffer and
//!   have no graphics API in their dependency graph, so they are always compiled and
//!   are equally usable by a GPU face, a terminal face or a test.
//! - **GPU back end** — the `compositor`, `pipeline`, `quad`, `text` and `web`
//!   submodules wrap `wgpu` and `glyphon`. They are compiled only when the `render`
//!   feature is enabled (it is on by default), and `web` additionally requires the
//!   `web` feature. The `compositor` is the face-neutral per-frame assembly every
//!   GPU face drives.
//!
//! Consumers that only need layout — a terminal front end, a headless test harness,
//! a server-side indexer — should depend on this crate with `default-features = false`
//! and pay no GPU cost at all.

mod cursor;
mod gutter;
mod highlight;
mod minimap;
mod simple_highlight;
/// Public, and it took two restatements to earn that.
///
/// This was `pub(crate)` until 2026-08-06, on the reasoning that the
/// conversions were "an internal discipline, not part of the crate's
/// published surface". That reasoning was wrong in a way only visible from
/// outside the crate: a face cannot adopt a discipline it cannot reach, so
/// both faces restated it instead.
///
/// - `apps/iridium-desktop/src/units.rs` said so outright — *"the kernel
///   solved this once ... but that module is private to the kernel, so the
///   two conversions this face needs are restated here"*.
/// - `iridium-bindings`' web face did not restate it; it fell back to the
///   bare `as usize` casts this module exists to replace.
///
/// The desktop restatement then diverged: its `pixel_to_index` saturated at
/// `u32::MAX` where this one saturates at `usize::MAX`, so the two disagreed
/// on every input at or above 2^32. Unreachable at real line counts, and
/// that is the point — a duplicate drifts silently while the drift is still
/// benign, and is only ever discovered once it is not.
///
/// Widening the module, not re-exporting item-by-item at the crate root:
/// the `render::` path states where the discipline comes from.
pub mod units;
mod viewport;

#[cfg(feature = "render")]
mod compositor;
#[cfg(feature = "render")]
mod pipeline;
#[cfg(feature = "render")]
mod quad;
#[cfg(feature = "render")]
mod rounded;
#[cfg(feature = "render")]
mod text;

#[cfg(all(feature = "render", feature = "web"))]
mod web;

pub use cursor::{BlinkState, CursorConfig, CursorRect, CursorRenderer, CursorStyle};
pub use gutter::{
    FoldIndicator, FoldIndicatorEntry, GutterBackground, GutterConfig, GutterRenderer,
    LineNumberEntry,
};
pub use highlight::{
    CurrentLineRenderer, FoldPlaceholder, FoldPlaceholderRenderer, HighlightRect,
    SearchHighlightRenderer, SelectionRenderer,
};
pub use minimap::{
    DEFAULT_CHARS_PER_LINE, DEFAULT_MINIMAP_WIDTH, MAX_LINE_HEIGHT, MAX_MINIMAP_WIDTH,
    MIN_LINE_HEIGHT, MIN_MINIMAP_WIDTH, MINIMAP_CHAR_WIDTH, MINIMAP_LINE_HEIGHT, MinimapConfig,
    MinimapDimensions, MinimapDragState, MinimapInteraction, MinimapLine, MinimapPosition,
    MinimapRect, MinimapRenderer, MinimapSegment, ViewportIndicator,
};
pub use simple_highlight::{HighlightSpan, SimpleHighlighter, SyntaxColors, TokenType};
pub use viewport::{Viewport, ViewportConfig};

#[cfg(feature = "render")]
pub use compositor::{FrameCompositor, FrameTarget, HighlightContext, HighlightSource};
#[cfg(feature = "render")]
pub use pipeline::{
    DEFAULT_TEXTURE_FORMAT, GpuInfo, MinimapRenderData, RenderConfig, RenderPipeline,
    SyntaxUniforms, ThemeUniforms,
};
#[cfg(feature = "render")]
pub use quad::{Quad, QuadRenderer};
#[cfg(feature = "render")]
pub use rounded::{RoundedQuad, RoundedQuadRenderer};
#[cfg(feature = "render")]
pub use text::{TextRenderConfig, TextRenderer};

#[cfg(all(feature = "render", feature = "web"))]
pub use web::WebRenderConfig;

#[cfg(all(feature = "render", feature = "web", target_arch = "wasm32"))]
pub use web::WebSurface;
