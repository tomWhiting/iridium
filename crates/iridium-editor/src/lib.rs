//! # Iridium Editor
//!
//! A GPU-accelerated text editor component built in Rust.
//!
//! Iridium renders at 120fps using WebGPU/wgpu, delivering the fluid, responsive
//! editing experience found in modern native editors—but portable to the browser
//! via WebAssembly.
//!
//! ## Architecture
//!
//! - `buffer` - Text buffer management using rope data structure
//! - `editor` - Core editor state and cursor management
//! - `history` - Undo/redo command system
//! - `input` - Keyboard and mouse event handling
//! - `render` - GPU rendering pipeline with wgpu and glyphon
//! - `syntax` - Syntax highlighting (tree-sitter integration)
//! - `lsp` - Language Server Protocol client

pub mod buffer;
pub mod editor;
pub mod events;
pub mod history;
pub mod input;
pub mod render;
pub mod view;

// Re-export primary types for convenient access
pub use buffer::Buffer;
pub use editor::{
    Clipboard, ControllerConfig, Cursor, Editor, EditorController, MemoryClipboard, Position,
    Selection,
};
pub use events::{
    ContentChangedEvent, CursorMovedEvent, EditorEvent, EventEmitter, ScrollChangedEvent,
    ScrollSource, SelectionChangeReason, SelectionChangedEvent,
};
pub use history::{Command, History, NodeId, NodeInfo, UndoTree};
pub use input::{
    InputAction, InputResult, Key, KeyEvent, KeyHandler, Modifiers, MouseButton, MouseEvent,
    MouseEventKind, MouseHandler,
};
pub use render::{RenderError, RenderPipeline, TextRenderer};
pub use view::{
    CursorConfig, CursorRenderer, EditorView, FrameStats, FrameTimer, GutterConfig, GutterRenderer,
    GutterWidth, HighlightConfig, HighlightRenderer, ScrollConfig, ScrollDirection, TargetFrameRate,
    ViewConfig, Viewport,
};
