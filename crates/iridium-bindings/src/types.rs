//! Type definitions for TypeScript bindings.

use napi_derive::napi;
use serde::{Deserialize, Serialize};

/// Position in the document.
#[napi(object)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsPosition {
    /// Line number (0-indexed)
    pub line: u32,
    /// Column number (0-indexed)
    pub column: u32,
}

/// Range in the document.
#[napi(object)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsRange {
    /// Start position
    pub start: JsPosition,
    /// End position
    pub end: JsPosition,
}

/// Selection with anchor and head.
#[napi(object)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsSelection {
    /// Anchor position
    pub anchor: JsPosition,
    /// Head position
    pub head: JsPosition,
}

/// Editor configuration options.
#[napi(object)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsEditorConfig {
    /// Tab width in spaces
    pub tab_width: Option<u32>,
    /// Insert spaces instead of tabs
    pub insert_spaces: Option<bool>,
    /// Auto-indent on newline
    pub auto_indent: Option<bool>,
    /// Show line numbers
    pub show_line_numbers: Option<bool>,
    /// Show minimap
    pub show_minimap: Option<bool>,
    /// Cursor blink rate in ms
    pub cursor_blink_ms: Option<u32>,
    /// Undo grouping timeout in ms
    pub undo_group_timeout_ms: Option<u32>,
}

/// Search options.
#[napi(object)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsSearchOptions {
    /// Case-sensitive matching
    pub case_sensitive: Option<bool>,
    /// Match whole words only
    pub whole_word: Option<bool>,
    /// Interpret query as regex
    pub regex: Option<bool>,
}
