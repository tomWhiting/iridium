//! GPU rendering subsystem.
//!
//! This module handles all rendering using wgpu and glyphon.

mod cursor;
mod gutter;
mod highlight;
mod minimap;
mod pipeline;
mod text;
mod viewport;

#[cfg(feature = "web")]
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
pub use pipeline::{
    DEFAULT_TEXTURE_FORMAT, GpuInfo, MinimapRenderData, RenderConfig, RenderPipeline,
    SyntaxUniforms, ThemeUniforms,
};
pub use text::{TextRenderConfig, TextRenderer};
pub use viewport::Viewport;

#[cfg(feature = "web")]
pub use web::WebRenderConfig;

#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub use web::{performance_now, request_animation_frame, WebSurface};
