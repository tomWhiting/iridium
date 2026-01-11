//! GPU rendering pipeline with wgpu and glyphon.
//!
//! This module provides GPU-accelerated text rendering using wgpu for
//! the graphics backend and glyphon for text shaping and rendering.

mod error;
mod gutter;
mod pipeline;
mod text;
mod viewport;

pub use error::RenderError;
pub use gutter::{GutterConfig, GutterRenderer, GutterWidth, LineNumberEntry};
pub use pipeline::RenderPipeline;
pub use text::TextRenderer;
pub use viewport::{ScrollConfig, ScrollDirection, Viewport};
