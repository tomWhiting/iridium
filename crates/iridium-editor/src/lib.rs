//! # Iridium Editor
//!
//! A GPU-accelerated text editor core built with Rust and wgpu.
//!
//! Iridium provides professional-grade text editing capabilities including:
//! - 120fps rendering performance
//! - Sub-8ms input latency
//! - Multi-cursor editing
//! - Tree-structured undo/redo history
//! - Syntax highlighting via tree-sitter
//! - Code folding
//! - Minimap navigation
//! - Theming support
//!
//! ## Architecture
//!
//! The editor follows a command-sourced architecture where all mutations
//! go through reversible [`Command`] objects, enabling the undo tree.
//!
//! ## Example
//!
//! ```ignore
//! use iridium_editor::{Editor, EditorConfig};
//!
//! let config = EditorConfig::default();
//! let editor = Editor::new(config)?;
//!
//! editor.set_content("Hello, world!");
//! editor.insert(" Iridium");
//! ```

#![doc(html_root_url = "https://docs.rs/iridium-editor/0.1.0")]

// Module declarations only - no implementation here
pub mod document;
pub mod editor;
pub mod history;
pub mod input;
pub mod render;
pub mod search;
#[cfg(feature = "syntax")]
pub mod syntax;
#[cfg(not(feature = "syntax"))]
pub mod syntax_stubs;
pub mod theme;
pub mod view;

/// Span indexing for efficient viewport-based syntax highlighting.
#[cfg(feature = "syntax")]
pub mod span_index;

// Re-exports for convenient access
pub use document::{CursorState, Document, Position, Range, Selection};
pub use editor::{Editor, EditorConfig, EditorEvent, EditorState, IridiumError};
pub use history::{Command, UndoTree};
pub use input::ImeHandler;
pub use input::{ClipboardOperation, KeyCode, KeyEvent, KeyResult, KeyboardHandler, Modifiers};
pub use input::{MouseButton, MouseEvent, MouseEventKind, MouseHandler, MouseResult};
pub use render::{CurrentLineRenderer, CursorRenderer, SelectionRenderer};
#[cfg(feature = "syntax")]
pub use syntax::{DocumentHighlighter, HighlightSpan, HighlightType, Language};
#[cfg(not(feature = "syntax"))]
pub use syntax_stubs::Language;
pub use theme::Theme;
pub use view::{DeltaTime, FrameBudget, FrameStats, FrameTimer};
