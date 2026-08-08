//! # Iridium Editor
//!
//! The Iridium editing kernel: a text editor core with **no graphics dependency**.
//!
//! Iridium provides professional-grade text editing capabilities including:
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
//! ## Features
//!
//! - `render` *(default)* — the GPU rendering back end, built on `wgpu` and
//!   `glyphon`, targeting 120fps. This is the **only** feature that pulls a graphics
//!   stack into the dependency graph. With `default-features = false` the kernel —
//!   document, history, input, search, theme, view and the pure layout half of
//!   [`render`] (viewport, gutter, cursor, selection and minimap geometry) — builds
//!   with no GPU crate present at all, which is what lets non-GPU faces such as a
//!   terminal front end reuse it verbatim.
//! - `syntax` *(default)* — tree-sitter syntax highlighting via `iridium-syntax`.
//!   Without it, the `syntax_stubs` module supplies the minimal language surface.
//! - `web` — browser/WASM helpers. Combined with `render` it also provides the
//!   canvas-backed GPU surface.
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
pub mod brace_folds;
pub mod commands;
pub mod document;
pub mod editor;
pub mod fuzzy;
pub mod history;
pub mod input;
pub mod pattern;
pub mod render;
pub mod search;
#[cfg(feature = "syntax")]
pub mod syntax;
#[cfg(not(feature = "syntax"))]
pub mod syntax_stubs;
pub mod text;
pub mod theme;
pub mod view;
pub mod workspace;

/// Span indexing for efficient viewport-based syntax highlighting.
#[cfg(feature = "syntax")]
pub mod span_index;

// Re-exports for convenient access
pub use commands::{
    CommandArgs, CommandCategory, CommandId, CommandInvocation, CommandMeta, CommandRegistry,
    KeyBinding, KeyHint, KeyHintIndex, KeyLabelStyle, KeyPress, Keymap, KeymapError,
    KeymapResolver, KeymapStack, ModeName, ModifierPattern, ModifierState, RegistryError,
    Resolution, StrokeCapture, StrokePattern,
};
pub use document::{CursorState, Document, Position, Range, Selection};
pub use editor::{Editor, EditorConfig, EditorEvent, EditorKeyResult, EditorState, IridiumError};
pub use history::{Command, UndoTree};
pub use input::ImeHandler;
pub use input::{
    ClipboardOperation, CommandRunError, KeyCode, KeyEvent, KeyResult, KeyboardHandler, Modifiers,
};
pub use input::{MouseButton, MouseEvent, MouseEventKind, MouseHandler, MouseResult};
pub use render::{CurrentLineRenderer, CursorRenderer, SelectionRenderer};
#[cfg(feature = "syntax")]
pub use syntax::{HighlightSpan, HighlightType};
// The same two names in the parser-free build, so a consumer that only needs
// to *name* the type — `input::keyboard`'s auto-pair suppression does — writes
// one import instead of a `cfg` pair. The types themselves are genuinely
// different (the stub's `Highlighter` finds nothing, by construction); what is
// shared is the spelling.
#[cfg(not(feature = "syntax"))]
pub use syntax_stubs::{HighlightSpan, HighlightType};

// One type in both feature configurations, rather than a pair selected by
// `cfg`. The pair disagreed — see `iridium_lang` — and could not be tested,
// because no build had both in scope.
pub use iridium_lang::Language;
pub use theme::Theme;
pub use view::{DeltaTime, FrameBudget, FrameStats, FrameTimer};
