//! Editor configuration options.

use serde::{Deserialize, Serialize};

/// Editor configuration options.
///
/// All options have sensible defaults. Use [`EditorConfig::default()`] for
/// standard settings.
///
/// # Example
///
/// ```
/// use iridium_editor::EditorConfig;
///
/// let config = EditorConfig {
///     tab_width: 2,
///     insert_spaces: true,
///     ..EditorConfig::default()
/// };
/// ```
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorConfig {
    /// Tab width in spaces (default: 4)
    pub tab_width: usize,

    /// Insert spaces instead of tabs (default: true)
    pub insert_spaces: bool,

    /// Auto-indent on newline (default: true)
    pub auto_indent: bool,

    /// Show line numbers (default: true)
    pub show_line_numbers: bool,

    /// Show minimap (default: true)
    pub show_minimap: bool,

    /// Cursor blink rate in milliseconds (0 = no blink, default: 500)
    pub cursor_blink_ms: u32,

    /// Undo grouping timeout in milliseconds (default: 500)
    ///
    /// Edits made within this time window are grouped into a single undo operation.
    pub undo_group_timeout_ms: u64,

    /// Scroll past end (default: false)
    ///
    /// When true, allows scrolling past the last line.
    pub scroll_past_end: bool,

    /// Word wrap (default: false)
    pub word_wrap: bool,

    /// Highlight current line (default: true)
    pub highlight_current_line: bool,

    /// Show whitespace characters (default: false)
    pub show_whitespace: bool,

    /// Font size in pixels (default: 14.0)
    pub font_size: f32,

    /// Line height multiplier (default: 1.5)
    pub line_height: f32,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            tab_width: 4,
            insert_spaces: true,
            auto_indent: true,
            show_line_numbers: true,
            show_minimap: true,
            cursor_blink_ms: 500,
            undo_group_timeout_ms: 500,
            scroll_past_end: false,
            word_wrap: false,
            highlight_current_line: true,
            show_whitespace: false,
            font_size: 14.0,
            line_height: 1.5,
        }
    }
}

impl EditorConfig {
    /// Creates a minimal configuration suitable for embedded use.
    #[must_use]
    pub fn minimal() -> Self {
        Self {
            show_line_numbers: false,
            show_minimap: false,
            highlight_current_line: false,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = EditorConfig::default();
        assert_eq!(config.tab_width, 4);
        assert!(config.insert_spaces);
        assert!(config.show_line_numbers);
    }

    #[test]
    fn minimal_config() {
        let config = EditorConfig::minimal();
        assert!(!config.show_line_numbers);
        assert!(!config.show_minimap);
    }
}
