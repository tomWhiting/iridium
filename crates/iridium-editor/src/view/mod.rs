//! Editor view and rendering.
//!
//! This module provides the visual representation of the editor, including
//! text rendering, cursor display, selection highlighting, and the render loop.

mod cursor_renderer;
mod editor_view;
mod frame_timer;
mod highlight;

pub use cursor_renderer::{CursorConfig, CursorRenderer};
pub use editor_view::{EditorView, ViewConfig, Viewport};
pub use frame_timer::{FrameStats, FrameTimer, TargetFrameRate};
pub use highlight::{HighlightConfig, HighlightRenderer};
