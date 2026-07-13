//! Editor configuration options.

use serde::{Deserialize, Serialize};

use crate::render::{DEFAULT_MINIMAP_WIDTH, MinimapPosition};

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

    /// Auto-close bracket and quote pairs (default: true)
    ///
    /// When enabled, typing `(`, `[`, `{`, `"`, `'`, or `` ` `` inserts the
    /// matching closer, typing a closer skips over an existing one, backspace
    /// between an empty pair removes both halves, and typing an opener with a
    /// selection wraps the selection.
    ///
    /// Defaults to `true` when absent from a serialized configuration, so
    /// configurations written before this option existed keep parsing.
    #[serde(default = "default_auto_pairs")]
    pub auto_pairs: bool,

    /// Fallback line-comment token for toggle-line-comment (default: None)
    ///
    /// Used by Ctrl+/ (and Shift+Alt+A's line fallback) when the document's
    /// language is unknown or has no comment syntax of its own. Ignored when
    /// the language supplies comment tokens. When neither the language nor
    /// this option provides a token, toggling comments is a no-op.
    ///
    /// Defaults to `None` when absent from a serialized configuration, so
    /// configurations written before this option existed keep parsing.
    #[serde(default)]
    pub line_comment_token: Option<String>,

    /// Show line numbers (default: true)
    pub show_line_numbers: bool,

    /// Show minimap (default: true)
    pub show_minimap: bool,

    /// Minimap width in pixels (default: 120.0)
    pub minimap_width: f32,

    /// Minimap position relative to editor (default: Right)
    #[serde(default)]
    pub minimap_position: MinimapPosition,

    /// Whether to show syntax colors in minimap (default: true)
    pub minimap_show_syntax_colors: bool,

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

/// Serde default for [`EditorConfig::auto_pairs`]: configurations serialized
/// before the option existed behave as if auto-pairing were enabled.
const fn default_auto_pairs() -> bool {
    true
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            tab_width: 4,
            insert_spaces: true,
            auto_indent: true,
            auto_pairs: true,
            line_comment_token: None,
            show_line_numbers: true,
            show_minimap: true,
            minimap_width: DEFAULT_MINIMAP_WIDTH,
            minimap_position: MinimapPosition::Right,
            minimap_show_syntax_colors: true,
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
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn default_config() {
        let config = EditorConfig::default();
        assert_eq!(config.tab_width, 4);
        assert!(config.insert_spaces);
        assert!(config.show_line_numbers);
        assert!(config.auto_pairs);
    }

    /// A configuration serialized before `auto_pairs` existed must still
    /// deserialize, with auto-pairing enabled by default.
    #[test]
    fn config_without_auto_pairs_field_deserializes_with_default() {
        let old_config = EditorConfig {
            auto_pairs: false, // Value that must NOT survive removal below.
            ..EditorConfig::default()
        };
        let json = serde_json::to_string(&old_config).expect("config must serialize");

        let mut value: serde_json::Value =
            serde_json::from_str(&json).expect("config JSON must parse");
        value
            .as_object_mut()
            .expect("config must serialize to an object")
            .remove("auto_pairs");
        let stripped = serde_json::to_string(&value).expect("stripped config must serialize");

        let parsed: EditorConfig =
            serde_json::from_str(&stripped).expect("old config without auto_pairs must parse");
        assert!(parsed.auto_pairs, "auto_pairs must default to true");
    }

    /// An explicitly serialized `auto_pairs` value round-trips.
    #[test]
    fn auto_pairs_round_trips() {
        let config = EditorConfig {
            auto_pairs: false,
            ..EditorConfig::default()
        };
        let json = serde_json::to_string(&config).expect("config must serialize");
        let parsed: EditorConfig = serde_json::from_str(&json).expect("config must parse");
        assert!(!parsed.auto_pairs);
    }

    /// A configuration serialized before `line_comment_token` existed must
    /// still deserialize, with no fallback token.
    #[test]
    fn config_without_line_comment_token_field_deserializes_with_default() {
        let config = EditorConfig {
            line_comment_token: Some("//".to_string()), // Must NOT survive removal below.
            ..EditorConfig::default()
        };
        let json = serde_json::to_string(&config).expect("config must serialize");

        let mut value: serde_json::Value =
            serde_json::from_str(&json).expect("config JSON must parse");
        value
            .as_object_mut()
            .expect("config must serialize to an object")
            .remove("line_comment_token");
        let stripped = serde_json::to_string(&value).expect("stripped config must serialize");

        let parsed: EditorConfig = serde_json::from_str(&stripped)
            .expect("old config without line_comment_token must parse");
        assert_eq!(parsed.line_comment_token, None);
    }

    /// An explicitly serialized `line_comment_token` value round-trips.
    #[test]
    fn line_comment_token_round_trips() {
        let config = EditorConfig {
            line_comment_token: Some("#".to_string()),
            ..EditorConfig::default()
        };
        let json = serde_json::to_string(&config).expect("config must serialize");
        let parsed: EditorConfig = serde_json::from_str(&json).expect("config must parse");
        assert_eq!(parsed.line_comment_token.as_deref(), Some("#"));
    }

    #[test]
    fn minimal_config() {
        let config = EditorConfig::minimal();
        assert!(!config.show_line_numbers);
        assert!(!config.show_minimap);
    }
}
