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
//! - `render` - GPU rendering pipeline with wgpu and glyphon
//! - `syntax` - Syntax highlighting (tree-sitter integration)
//! - `lsp` - Language Server Protocol client

pub mod buffer;
pub mod editor;
pub mod history;
pub mod render;

// Re-export primary types for convenient access
pub use buffer::Buffer;
pub use editor::{Cursor, Editor, Position, Selection};
pub use history::{Command, History};
pub use render::{RenderError, RenderPipeline, TextRenderer};
