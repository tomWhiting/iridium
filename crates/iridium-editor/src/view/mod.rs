//! Editor view and rendering.
//!
//! This module provides the visual representation of the editor, including
//! text rendering, cursor display, selection highlighting, and the render loop.

mod cursor_renderer;
mod editor_view;
mod frame_timer;
mod gutter;
mod highlight;
mod line_cache;
mod viewport;

pub use cursor_renderer::{CursorConfig, CursorRenderer};
pub use editor_view::{EditorView, ViewConfig};
pub use frame_timer::{FrameStats, FrameTimer, TargetFrameRate};
pub use gutter::{GutterConfig, GutterRenderer, GutterWidth};
pub use highlight::{HighlightConfig, HighlightRenderer};
pub use line_cache::LineCache;
pub use viewport::{ScrollConfig, ScrollDirection, Viewport};
