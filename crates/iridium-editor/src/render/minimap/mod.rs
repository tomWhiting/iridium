//! Minimap rendering for document overview.
//!
//! This module provides a scaled-down view of the entire document,
//! allowing users to quickly navigate through large files.
//!
//! # Features
//!
//! - Scaled document preview showing the entire file
//! - Viewport indicator showing the currently visible region
//! - Click-to-navigate for quick jumps
//! - Drag-to-scroll for smooth navigation
//! - Syntax coloring matching the main editor
//!
//! # Architecture
//!
//! The minimap renders lines as colored blocks rather than actual text,
//! providing a visual overview without the overhead of full text rendering.
//! Each line is represented as a series of colored segments based on
//! syntax highlighting.

mod renderer;
mod types;

#[cfg(test)]
mod tests;

pub use renderer::MinimapRenderer;
pub use types::{
    MinimapConfig, MinimapDimensions, MinimapDragState, MinimapInteraction, MinimapLine,
    MinimapPosition, MinimapRect, MinimapSegment, ViewportIndicator, DEFAULT_CHARS_PER_LINE,
    DEFAULT_MINIMAP_WIDTH, MAX_LINE_HEIGHT, MAX_MINIMAP_WIDTH, MINIMAP_CHAR_WIDTH,
    MINIMAP_LINE_HEIGHT, MIN_LINE_HEIGHT, MIN_MINIMAP_WIDTH,
};
