//! Color definitions for themes.

use serde::{Deserialize, Serialize};

/// RGBA color with components in 0-1 range.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    /// Red component (0.0-1.0)
    pub r: f32,
    /// Green component (0.0-1.0)
    pub g: f32,
    /// Blue component (0.0-1.0)
    pub b: f32,
    /// Alpha component (0.0-1.0)
    pub a: f32,
}

impl Color {
    /// Creates a new color from RGBA values (0-1 range).
    #[must_use]
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Creates a color from RGB values (0-1 range), with full opacity.
    #[must_use]
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Creates a color from a hex string (e.g., "#FF5500" or "FF5500").
    #[must_use]
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 && hex.len() != 8 {
            return None;
        }

        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        let a = if hex.len() == 8 {
            u8::from_str_radix(&hex[6..8], 16).ok()?
        } else {
            255
        };

        Some(Self {
            r: f32::from(r) / 255.0,
            g: f32::from(g) / 255.0,
            b: f32::from(b) / 255.0,
            a: f32::from(a) / 255.0,
        })
    }

    /// Converts to an array for shader uniforms.
    #[must_use]
    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::rgb(0.0, 0.0, 0.0)
    }
}

/// Editor chrome colors.
///
/// Deserialization is forgiving: fields absent from a theme JSON fall back to
/// the dark preset via [`Default`], so adding new colors never breaks
/// existing theme files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorColors {
    /// Main editor background
    pub background: Color,
    /// Default text color
    pub foreground: Color,
    /// Selection highlight when focused
    pub selection: Color,
    /// Selection highlight when unfocused
    pub selection_inactive: Color,
    /// Cursor color
    pub cursor: Color,
    /// Line number color
    pub line_number: Color,
    /// Active line number color
    pub line_number_active: Color,
    /// Current line background highlight
    pub current_line: Color,
    /// Gutter background
    pub gutter: Color,
    /// Minimap background
    pub minimap_background: Color,
    /// Search match highlight
    pub search_match: Color,
    /// Current search match highlight
    pub search_match_current: Color,
    /// Background tint for added lines in diff view
    pub diff_added_bg: Color,
    /// Background tint for deleted lines in diff view
    pub diff_deleted_bg: Color,
    /// Gutter color for added lines in diff view (+)
    pub diff_added_gutter: Color,
    /// Gutter color for deleted lines in diff view (-)
    pub diff_deleted_gutter: Color,
    /// Gutter bar color for added lines (change indicators)
    pub change_added: Color,
    /// Gutter bar color for modified lines (change indicators)
    pub change_modified: Color,
    /// Gutter bar color for deleted lines (change indicators)
    pub change_deleted: Color,
    /// Foreground color for inline blame ghost text
    pub blame_foreground: Color,
    /// Gutter marker color for error diagnostics
    pub diagnostic_error: Color,
    /// Gutter marker color for warning diagnostics
    pub diagnostic_warning: Color,
    /// Gutter marker color for info diagnostics
    pub diagnostic_info: Color,
    /// Gutter marker color for hint diagnostics
    pub diagnostic_hint: Color,
}

impl EditorColors {
    /// Creates dark theme editor colors.
    #[must_use]
    pub fn dark() -> Self {
        Self {
            background: Color::new(0.0, 0.0, 0.0, 0.0),
            foreground: Color::from_hex("d4d4d4").unwrap_or_default(),
            selection: Color::new(0.26, 0.42, 0.56, 0.5),
            selection_inactive: Color::new(0.26, 0.42, 0.56, 0.3),
            cursor: Color::from_hex("aeafad").unwrap_or_default(),
            line_number: Color::from_hex("858585").unwrap_or_default(),
            line_number_active: Color::from_hex("c6c6c6").unwrap_or_default(),
            current_line: Color::new(0.15, 0.15, 0.15, 0.3),
            gutter: Color::new(0.0, 0.0, 0.0, 0.0),
            minimap_background: Color::new(0.0, 0.0, 0.0, 0.0),
            search_match: Color::new(0.52, 0.37, 0.13, 0.6),
            search_match_current: Color::new(0.82, 0.67, 0.33, 0.8),
            diff_added_bg: Color::new(0.14, 0.40, 0.22, 0.25),
            diff_deleted_bg: Color::new(0.50, 0.15, 0.15, 0.25),
            diff_added_gutter: Color::new(0.30, 0.70, 0.40, 1.0),
            diff_deleted_gutter: Color::new(0.80, 0.30, 0.30, 1.0),
            change_added: Color::new(0.30, 0.70, 0.40, 1.0),
            change_modified: Color::new(0.80, 0.65, 0.20, 1.0),
            change_deleted: Color::new(0.80, 0.30, 0.30, 1.0),
            blame_foreground: Color::new(0.45, 0.45, 0.50, 0.6),
            diagnostic_error: Color::new(0.80, 0.30, 0.30, 1.0),
            diagnostic_warning: Color::new(0.80, 0.65, 0.20, 1.0),
            diagnostic_info: Color::new(0.30, 0.55, 0.80, 1.0),
            diagnostic_hint: Color::new(0.55, 0.55, 0.60, 1.0),
        }
    }

    /// Creates light theme editor colors.
    #[must_use]
    pub fn light() -> Self {
        Self {
            background: Color::from_hex("ffffff").unwrap_or_default(),
            foreground: Color::from_hex("333333").unwrap_or_default(),
            selection: Color::new(0.68, 0.85, 1.0, 0.5),
            selection_inactive: Color::new(0.68, 0.85, 1.0, 0.3),
            cursor: Color::from_hex("000000").unwrap_or_default(),
            line_number: Color::from_hex("999999").unwrap_or_default(),
            line_number_active: Color::from_hex("333333").unwrap_or_default(),
            current_line: Color::new(0.97, 0.97, 0.97, 1.0),
            gutter: Color::from_hex("f5f5f5").unwrap_or_default(),
            minimap_background: Color::from_hex("f5f5f5").unwrap_or_default(),
            search_match: Color::new(1.0, 0.92, 0.55, 0.6),
            search_match_current: Color::new(1.0, 0.72, 0.25, 0.8),
            diff_added_bg: Color::new(0.20, 0.60, 0.30, 0.15),
            diff_deleted_bg: Color::new(0.70, 0.20, 0.20, 0.15),
            diff_added_gutter: Color::new(0.15, 0.55, 0.25, 1.0),
            diff_deleted_gutter: Color::new(0.70, 0.20, 0.20, 1.0),
            change_added: Color::new(0.15, 0.55, 0.25, 1.0),
            change_modified: Color::new(0.70, 0.55, 0.10, 1.0),
            change_deleted: Color::new(0.70, 0.20, 0.20, 1.0),
            blame_foreground: Color::new(0.50, 0.50, 0.55, 0.5),
            diagnostic_error: Color::new(0.70, 0.20, 0.20, 1.0),
            diagnostic_warning: Color::new(0.70, 0.55, 0.10, 1.0),
            diagnostic_info: Color::new(0.20, 0.40, 0.70, 1.0),
            diagnostic_hint: Color::new(0.45, 0.45, 0.50, 1.0),
        }
    }
}

impl Default for EditorColors {
    /// The dark preset. Serves as the per-field fallback for theme JSON that
    /// omits fields (see the container-level `#[serde(default)]`).
    fn default() -> Self {
        Self::dark()
    }
}

/// Syntax highlighting colors.
///
/// Deserialization is forgiving: fields absent from a theme JSON fall back to
/// the dark preset via [`Default`], so adding new token colors never breaks
/// existing theme files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyntaxColors {
    /// Keywords (if, else, fn, etc.)
    pub keyword: Color,
    /// String literals
    pub string: Color,
    /// Numeric literals
    pub number: Color,
    /// Comments
    pub comment: Color,
    /// Function names
    pub function: Color,
    /// Variable names
    pub variable: Color,
    /// Type names
    pub type_name: Color,
    /// Operators
    pub operator: Color,
    /// Punctuation
    pub punctuation: Color,
    /// Object properties
    pub property: Color,
    /// Constants
    pub constant: Color,
    /// HTML/XML tags
    pub tag: Color,
    /// HTML/XML attributes
    pub attribute: Color,
    /// Syntax errors
    pub error: Color,
}

impl SyntaxColors {
    /// Creates dark theme syntax colors.
    #[must_use]
    pub fn dark() -> Self {
        Self {
            keyword: Color::from_hex("569cd6").unwrap_or_default(),
            string: Color::from_hex("ce9178").unwrap_or_default(),
            number: Color::from_hex("b5cea8").unwrap_or_default(),
            comment: Color::from_hex("6a9955").unwrap_or_default(),
            function: Color::from_hex("dcdcaa").unwrap_or_default(),
            variable: Color::from_hex("9cdcfe").unwrap_or_default(),
            type_name: Color::from_hex("4ec9b0").unwrap_or_default(),
            operator: Color::from_hex("d4d4d4").unwrap_or_default(),
            punctuation: Color::from_hex("d4d4d4").unwrap_or_default(),
            property: Color::from_hex("9cdcfe").unwrap_or_default(),
            constant: Color::from_hex("4fc1ff").unwrap_or_default(),
            tag: Color::from_hex("569cd6").unwrap_or_default(),
            attribute: Color::from_hex("9cdcfe").unwrap_or_default(),
            error: Color::from_hex("f44747").unwrap_or_default(),
        }
    }

    /// Creates light theme syntax colors.
    #[must_use]
    pub fn light() -> Self {
        Self {
            keyword: Color::from_hex("0000ff").unwrap_or_default(),
            string: Color::from_hex("a31515").unwrap_or_default(),
            number: Color::from_hex("098658").unwrap_or_default(),
            comment: Color::from_hex("008000").unwrap_or_default(),
            function: Color::from_hex("795e26").unwrap_or_default(),
            variable: Color::from_hex("001080").unwrap_or_default(),
            type_name: Color::from_hex("267f99").unwrap_or_default(),
            operator: Color::from_hex("000000").unwrap_or_default(),
            punctuation: Color::from_hex("000000").unwrap_or_default(),
            property: Color::from_hex("001080").unwrap_or_default(),
            constant: Color::from_hex("0070c1").unwrap_or_default(),
            tag: Color::from_hex("800000").unwrap_or_default(),
            attribute: Color::from_hex("ff0000").unwrap_or_default(),
            error: Color::from_hex("ff0000").unwrap_or_default(),
        }
    }
}

impl Default for SyntaxColors {
    /// The dark preset. Serves as the per-field fallback for theme JSON that
    /// omits fields (see the container-level `#[serde(default)]`).
    fn default() -> Self {
        Self::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_from_hex() {
        let color = Color::from_hex("#FF5500").unwrap();
        assert!((color.r - 1.0).abs() < 0.01);
        assert!((color.g - 0.333).abs() < 0.01);
        assert!((color.b - 0.0).abs() < 0.01);
    }

    #[test]
    fn color_from_hex_with_alpha() {
        let color = Color::from_hex("FF550080").unwrap();
        assert!((color.a - 0.502).abs() < 0.01);
    }

    #[test]
    fn editor_colors_parse_with_missing_fields() {
        // Theme JSON written before newer fields existed must keep parsing,
        // with absent fields falling back to the dark preset.
        let partial = r#"{
            "background": {"r": 0.1, "g": 0.1, "b": 0.1, "a": 1.0},
            "foreground": {"r": 0.9, "g": 0.9, "b": 0.9, "a": 1.0}
        }"#;
        let colors: EditorColors = serde_json::from_str(partial).unwrap();
        assert!((colors.background.r - 0.1).abs() < f32::EPSILON);
        assert_eq!(
            colors.diagnostic_error,
            EditorColors::dark().diagnostic_error
        );
        assert_eq!(colors.diff_added_bg, EditorColors::dark().diff_added_bg);
    }

    #[test]
    fn syntax_colors_parse_with_missing_fields() {
        let partial = r#"{
            "keyword": {"r": 0.3, "g": 0.6, "b": 0.8, "a": 1.0}
        }"#;
        let colors: SyntaxColors = serde_json::from_str(partial).unwrap();
        assert!((colors.keyword.r - 0.3).abs() < f32::EPSILON);
        assert_eq!(colors.string, SyntaxColors::dark().string);
    }
}
