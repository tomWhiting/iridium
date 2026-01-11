//! Type definitions for TypeScript bindings.
//!
//! This module defines all the types exposed to TypeScript consumers via napi-rs.
//! Types are designed to be easy to use from JavaScript while maintaining
//! type safety through TypeScript declarations.

use napi_derive::napi;
use serde::{Deserialize, Serialize};

use iridium_editor::editor::FoldInfo;

// ============================================================================
// Position and Range Types
// ============================================================================

/// Position in the document.
///
/// Both line and column are 0-indexed.
#[napi(object)]
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct JsPosition {
    /// Line number (0-indexed)
    pub line: u32,
    /// Column number (0-indexed)
    pub column: u32,
}

/// Range in the document.
///
/// Represents a span of text from start to end position.
#[napi(object)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JsRange {
    /// Start position
    pub start: JsPosition,
    /// End position
    pub end: JsPosition,
}

/// Selection with anchor and head.
///
/// The anchor is where the selection started, and the head is the current
/// cursor position. This allows tracking selection direction.
#[napi(object)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JsSelection {
    /// Anchor position (where selection started)
    pub anchor: JsPosition,
    /// Head position (current cursor position)
    pub head: JsPosition,
}

// ============================================================================
// Configuration Types
// ============================================================================

/// Editor configuration options.
///
/// All fields are optional - unspecified fields will use default values.
#[napi(object)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JsEditorConfig {
    /// Tab width in spaces (default: 4)
    pub tab_width: Option<u32>,
    /// Insert spaces instead of tabs (default: true)
    pub insert_spaces: Option<bool>,
    /// Auto-indent on newline (default: true)
    pub auto_indent: Option<bool>,
    /// Show line numbers (default: true)
    pub show_line_numbers: Option<bool>,
    /// Show minimap (default: true)
    pub show_minimap: Option<bool>,
    /// Cursor blink rate in ms (default: 530)
    pub cursor_blink_ms: Option<u32>,
    /// Undo grouping timeout in ms (default: 500)
    pub undo_group_timeout_ms: Option<u32>,
}

// ============================================================================
// Search Types
// ============================================================================

/// Search options.
///
/// All fields are optional - unspecified fields default to false.
#[napi(object)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JsSearchOptions {
    /// Case-sensitive matching (default: false)
    pub case_sensitive: Option<bool>,
    /// Match whole words only (default: false)
    pub whole_word: Option<bool>,
    /// Interpret query as regex (default: false)
    pub regex: Option<bool>,
}

/// Search result containing matches.
#[napi(object)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JsSearchResult {
    /// Total number of matches
    pub match_count: u32,
    /// Current match index (0-based), if any
    pub current_index: Option<u32>,
    /// All match ranges
    pub matches: Vec<JsRange>,
}

// ============================================================================
// Undo/Redo Types
// ============================================================================

/// Information about the undo tree state.
#[napi(object)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JsUndoInfo {
    /// Total number of nodes in the undo tree
    pub node_count: u32,
    /// Number of branches in the undo tree
    pub branch_count: u32,
    /// Whether undo is available
    pub can_undo: bool,
    /// Whether redo is available
    pub can_redo: bool,
    /// Current node ID in the undo tree
    pub current_id: String,
    /// Root node ID of the undo tree
    pub root_id: String,
}

// ============================================================================
// Folding Types
// ============================================================================

/// Fold information for export/import.
#[napi(object)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JsFoldInfo {
    /// Start lines of all foldable regions
    pub regions: Vec<u32>,
    /// Start lines of currently folded regions
    pub folded: Vec<u32>,
}

impl From<FoldInfo> for JsFoldInfo {
    fn from(info: FoldInfo) -> Self {
        Self {
            regions: info.regions.into_iter().map(|l| l as u32).collect(),
            folded: info.folded.into_iter().map(|l| l as u32).collect(),
        }
    }
}

impl From<JsFoldInfo> for FoldInfo {
    fn from(info: JsFoldInfo) -> Self {
        Self {
            regions: info.regions.into_iter().map(|l| l as usize).collect(),
            folded: info.folded.into_iter().map(|l| l as usize).collect(),
        }
    }
}

// ============================================================================
// Event Types
// ============================================================================

/// Event types that can be subscribed to.
///
/// Use these string constants with `editor.on(eventType, callback)`.
#[napi(string_enum)]
#[derive(Debug, Clone, Copy)]
pub enum JsEventType {
    /// Fired when document content changes
    ContentChanged,
    /// Fired when selection/cursor position changes
    SelectionChanged,
    /// Fired when scroll position changes
    ScrollChanged,
    /// Fired when theme changes
    ThemeChanged,
    /// Fired when editor gains focus
    Focus,
    /// Fired when editor loses focus
    Blur,
    /// Fired when search results update
    SearchUpdated,
    /// Fired when fold state changes
    FoldChanged,
    /// Fired when an error occurs
    Error,
}

// ============================================================================
// Error Types
// ============================================================================

/// Error information for TypeScript consumers.
#[napi(object)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JsError {
    /// Error message
    pub message: String,
    /// Error code for programmatic handling
    pub code: String,
}

// ============================================================================
// Theme Types
// ============================================================================

/// Theme information.
#[napi(object)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JsThemeInfo {
    /// Theme name
    pub name: String,
    /// Whether this is a dark theme
    pub is_dark: bool,
}

// ============================================================================
// Viewport Types
// ============================================================================

/// Viewport information.
#[napi(object)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JsViewport {
    /// First visible line (0-indexed)
    pub first_line: u32,
    /// Last visible line (0-indexed)
    pub last_line: u32,
    /// Viewport width in pixels
    pub width: f64,
    /// Viewport height in pixels
    pub height: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_position_default() {
        let pos = JsPosition::default();
        assert_eq!(pos.line, 0);
        assert_eq!(pos.column, 0);
    }

    #[test]
    fn js_range_default() {
        let range = JsRange::default();
        assert_eq!(range.start.line, 0);
        assert_eq!(range.end.line, 0);
    }

    #[test]
    fn js_fold_info_conversion() {
        let fold_info = FoldInfo {
            regions: vec![0, 10, 20],
            folded: vec![0, 20],
        };

        let js_info: JsFoldInfo = fold_info.clone().into();
        assert_eq!(js_info.regions, vec![0, 10, 20]);
        assert_eq!(js_info.folded, vec![0, 20]);

        let back: FoldInfo = js_info.into();
        assert_eq!(back.regions, fold_info.regions);
        assert_eq!(back.folded, fold_info.folded);
    }
}
