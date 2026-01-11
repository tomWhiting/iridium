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
pub use highlight::{
    CurrentLineRenderer, FoldPlaceholder, FoldPlaceholderRenderer, HighlightRect, SelectionRenderer,
};
pub use minimap::MinimapRenderer;
pub use pipeline::{DEFAULT_TEXTURE_FORMAT, GpuInfo, RenderConfig, RenderPipeline};
pub use text::{TextRenderConfig, TextRenderer};
pub use viewport::Viewport;
