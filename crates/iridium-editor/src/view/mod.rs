//! Editor view and rendering.
//!
//! This module provides the visual representation of the editor, including
//! text rendering, cursor display, selection highlighting, and the render loop.

mod cursor_renderer;
mod editor_view;
mod frame_timer;
mod highlight;
mod line_cache;

pub use cursor_renderer::{CursorConfig, CursorRenderer};
pub use editor_view::{EditorView, ViewConfig};
pub use frame_timer::{FrameStats, FrameTimer, TargetFrameRate};
pub use highlight::{HighlightConfig, HighlightRenderer};
pub use line_cache::LineCache;

// Re-export rendering types from render module for backwards compatibility
pub use crate::render::{GutterConfig, GutterRenderer, GutterWidth, ScrollConfig, ScrollDirection, Viewport};
