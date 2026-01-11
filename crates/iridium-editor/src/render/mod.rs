//! GPU rendering pipeline with wgpu and glyphon.
//!
//! This module provides GPU-accelerated text rendering using wgpu for
//! the graphics backend and glyphon for text shaping and rendering.

mod error;
mod pipeline;
mod text;

pub use error::RenderError;
pub use pipeline::RenderPipeline;
pub use text::TextRenderer;
