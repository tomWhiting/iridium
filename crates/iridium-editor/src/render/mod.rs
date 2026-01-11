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

pub use cursor::{BlinkState, CursorConfig, CursorRect, CursorRenderer, CursorStyle};
pub use gutter::{
    FoldIndicator, FoldIndicatorEntry, GutterBackground, GutterConfig, GutterRenderer,
    LineNumberEntry,
};
pub use highlight::{CurrentLineRenderer, HighlightRect, SelectionRenderer};
pub use minimap::{
    MinimapConfig, MinimapDimensions, MinimapDragState, MinimapInteraction, MinimapLine,
    MinimapPosition, MinimapRect, MinimapRenderer, MinimapSegment, ViewportIndicator,
    DEFAULT_CHARS_PER_LINE, DEFAULT_MINIMAP_WIDTH, MAX_LINE_HEIGHT, MAX_MINIMAP_WIDTH,
    MINIMAP_CHAR_WIDTH, MINIMAP_LINE_HEIGHT, MIN_LINE_HEIGHT, MIN_MINIMAP_WIDTH,
};
pub use pipeline::{
    GpuInfo, MinimapRenderData, RenderConfig, RenderPipeline, SyntaxUniforms, ThemeUniforms,
    DEFAULT_TEXTURE_FORMAT,
};
pub use text::{TextRenderConfig, TextRenderer};
pub use viewport::Viewport;
