//! VS Code theme compatibility layer (T151).
//!
//! This module provides conversion from VS Code theme format to Iridium's
//! native theme format. VS Code themes are widely available and this
//! compatibility layer allows users to use their favorite themes.
//!
//! # VS Code Theme Format
//!
//! VS Code themes consist of:
//! - `colors`: A map of editor UI colors (background, foreground, etc.)
//! - `tokenColors`: An array of syntax highlighting rules with scopes
//!
//! # Scope Mapping
//!
//! VS Code uses TextMate-style scopes (e.g., "keyword.control", "string.quoted").
//! This module maps these to Iridium's simpler token types.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{Color, EditorColors, SyntaxColors, Theme, Typography};

/// A VS Code theme definition.
///
/// This represents the JSON structure of a VS Code color theme file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsCodeTheme {
    /// Theme name
    #[serde(default)]
    pub name: String,

    /// Theme type: "dark", "light", or "hc" (high contrast)
    #[serde(rename = "type", default)]
    pub theme_type: String,

    /// Editor UI colors
    #[serde(default)]
    pub colors: HashMap<String, String>,

    /// Syntax highlighting token colors
    #[serde(rename = "tokenColors", default)]
    pub token_colors: Vec<VsCodeTokenColor>,
}

impl VsCodeTheme {
    /// Whether this document states anything an editor could wear.
    ///
    /// ⭐ **The line that makes the two-parser order work.** Every field above
    /// is `#[serde(default)]`, faithfully, because every field is optional in
    /// VS Code itself — which means *any* JSON object deserializes into this
    /// struct. That is harmless on its own and a defect in context:
    /// `iridium_config::theme` offers a file to the strict native parser first
    /// and this one second, precisely so a native theme with a typo is
    /// reported rather than swallowed. A second parser that accepts everything
    /// makes the first one's complaint unreachable.
    ///
    /// So the narrowest possible test: at least one of `colors` or
    /// `tokenColors`. Both kinds of real theme pass — UI-only themes carry
    /// `colors` and no `tokenColors`, `TextMate` conversions carry the reverse —
    /// while a document with neither cannot change a single pixel, so
    /// accepting it could only ever produce silence. `name` and `type` are
    /// deliberately not enough: they describe a theme without being one.
    #[must_use]
    pub fn states_any_colour(&self) -> bool {
        !self.colors.is_empty() || !self.token_colors.is_empty()
    }
}

/// A syntax highlighting rule in VS Code themes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsCodeTokenColor {
    /// Human-readable name for this rule
    #[serde(default)]
    pub name: Option<String>,

    /// Scopes this rule applies to (can be string or array)
    #[serde(default)]
    pub scope: VsCodeScope,

    /// Style settings for matching tokens
    #[serde(default)]
    pub settings: VsCodeTokenSettings,
}

/// Scope specification for a token color rule.
///
/// In VS Code themes, scope can be either a single string or an array of strings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VsCodeScope {
    /// Single scope string
    Single(String),
    /// Multiple scopes
    Multiple(Vec<String>),
    /// No scope (matches everything)
    #[default]
    None,
}

impl VsCodeScope {
    /// Returns all scopes as a slice of strings.
    fn to_vec(&self) -> Vec<&str> {
        match self {
            Self::Single(s) => vec![s.as_str()],
            Self::Multiple(v) => v.iter().map(String::as_str).collect(),
            Self::None => vec![],
        }
    }
}

/// Style settings for a token color rule.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VsCodeTokenSettings {
    /// Foreground color
    #[serde(default)]
    pub foreground: Option<String>,

    /// Background color (rarely used)
    #[serde(default)]
    pub background: Option<String>,

    /// Font style (italic, bold, underline)
    #[serde(rename = "fontStyle", default)]
    pub font_style: Option<String>,
}

/// Converts a VS Code theme to an Iridium theme.
pub fn convert_vscode_theme(vscode: &VsCodeTheme) -> Theme {
    let is_dark = vscode.theme_type.to_lowercase() != "light";

    // Start with defaults based on theme type
    let mut editor = if is_dark {
        EditorColors::dark()
    } else {
        EditorColors::light()
    };
    let mut syntax = if is_dark {
        SyntaxColors::dark()
    } else {
        SyntaxColors::light()
    };

    // Convert editor colors
    convert_editor_colors(&vscode.colors, &mut editor);

    // Convert token colors
    convert_token_colors(&vscode.token_colors, &mut syntax);

    Theme {
        name: if vscode.name.is_empty() {
            "Imported VS Code Theme".to_string()
        } else {
            vscode.name.clone()
        },
        is_dark,
        editor,
        syntax,
        typography: Typography::default(),
    }
}

/// Converts VS Code editor UI colors to Iridium editor colors.
fn convert_editor_colors(colors: &HashMap<String, String>, editor: &mut EditorColors) {
    // Map VS Code color keys to Iridium colors
    if let Some(color) = colors
        .get("editor.background")
        .and_then(|c| Color::from_hex(c))
    {
        editor.background = color;
    }
    if let Some(color) = colors
        .get("editor.foreground")
        .and_then(|c| Color::from_hex(c))
    {
        editor.foreground = color;
    }
    if let Some(color) = colors
        .get("editor.selectionBackground")
        .and_then(|c| Color::from_hex(c))
    {
        editor.selection = color;
    }
    if let Some(color) = colors
        .get("editor.inactiveSelectionBackground")
        .and_then(|c| Color::from_hex(c))
    {
        editor.selection_inactive = color;
    }
    if let Some(color) = colors
        .get("editorCursor.foreground")
        .and_then(|c| Color::from_hex(c))
    {
        editor.cursor = color;
    }
    if let Some(color) = colors
        .get("editorLineNumber.foreground")
        .and_then(|c| Color::from_hex(c))
    {
        editor.line_number = color;
    }
    if let Some(color) = colors
        .get("editorLineNumber.activeForeground")
        .and_then(|c| Color::from_hex(c))
    {
        editor.line_number_active = color;
    }
    if let Some(color) = colors
        .get("editor.lineHighlightBackground")
        .and_then(|c| Color::from_hex(c))
    {
        editor.current_line = color;
    }
    if let Some(color) = colors
        .get("editorGutter.background")
        .and_then(|c| Color::from_hex(c))
    {
        editor.gutter = color;
    }
    if let Some(color) = colors
        .get("minimap.background")
        .and_then(|c| Color::from_hex(c))
    {
        editor.minimap_background = color;
    }
    if let Some(color) = colors
        .get("editor.findMatchBackground")
        .and_then(|c| Color::from_hex(c))
    {
        editor.search_match = color;
    }
    if let Some(color) = colors
        .get("editor.findMatchHighlightBackground")
        .and_then(|c| Color::from_hex(c))
    {
        editor.search_match_current = color;
    }
}

/// Converts VS Code token colors to Iridium syntax colors.
fn convert_token_colors(token_colors: &[VsCodeTokenColor], syntax: &mut SyntaxColors) {
    for token in token_colors {
        let Some(ref foreground) = token.settings.foreground else {
            continue;
        };
        let Some(color) = Color::from_hex(foreground) else {
            continue;
        };

        // Apply color to all matching scopes
        for scope in token.scope.to_vec() {
            apply_scope_color(scope, color, syntax);
        }
    }
}

/// Applies a color to the appropriate syntax field based on `TextMate` scope.
fn apply_scope_color(scope: &str, color: Color, syntax: &mut SyntaxColors) {
    // Map TextMate scopes to Iridium token types
    // See: https://macromates.com/manual/en/language_grammars#naming_conventions

    let scope_lower = scope.to_lowercase();

    // Keywords
    if scope_lower.starts_with("keyword")
        || scope_lower.starts_with("storage.type")
        || scope_lower.starts_with("storage.modifier")
    {
        syntax.keyword = color;
        return;
    }

    // Strings
    if scope_lower.starts_with("string") {
        syntax.string = color;
        return;
    }

    // Numbers
    if scope_lower.starts_with("constant.numeric") {
        syntax.number = color;
        return;
    }

    // Comments
    if scope_lower.starts_with("comment") {
        syntax.comment = color;
        return;
    }

    // Functions
    if scope_lower.starts_with("entity.name.function")
        || scope_lower.starts_with("support.function")
        || scope_lower.starts_with("meta.function-call")
    {
        syntax.function = color;
        return;
    }

    // Variables
    if scope_lower.starts_with("variable") {
        syntax.variable = color;
        return;
    }

    // Types
    if scope_lower.starts_with("entity.name.type")
        || scope_lower.starts_with("support.type")
        || scope_lower.starts_with("entity.name.class")
        || scope_lower.starts_with("support.class")
    {
        syntax.type_name = color;
        return;
    }

    // Operators
    if scope_lower.starts_with("keyword.operator") {
        syntax.operator = color;
        return;
    }

    // Punctuation
    if scope_lower.starts_with("punctuation") {
        syntax.punctuation = color;
        return;
    }

    // Properties
    if scope_lower.starts_with("variable.other.property")
        || scope_lower.starts_with("entity.name.tag.yaml")
        || scope_lower.starts_with("support.type.property-name")
    {
        syntax.property = color;
        return;
    }

    // Constants
    if scope_lower.starts_with("constant.language")
        || scope_lower.starts_with("constant.other")
        || scope_lower.starts_with("variable.other.constant")
    {
        syntax.constant = color;
        return;
    }

    // Tags (HTML/XML)
    if scope_lower.starts_with("entity.name.tag") {
        syntax.tag = color;
        return;
    }

    // Attributes
    if scope_lower.starts_with("entity.other.attribute-name") {
        syntax.attribute = color;
        return;
    }

    // Errors
    if scope_lower.starts_with("invalid") {
        syntax.error = color;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_minimal_vscode_theme() {
        let vscode = VsCodeTheme {
            name: "Test Theme".to_string(),
            theme_type: "dark".to_string(),
            colors: HashMap::new(),
            token_colors: vec![],
        };

        let theme = convert_vscode_theme(&vscode);
        assert_eq!(theme.name, "Test Theme");
        assert!(theme.is_dark);
    }

    #[test]
    fn convert_light_theme() {
        let vscode = VsCodeTheme {
            name: "Light Test".to_string(),
            theme_type: "light".to_string(),
            colors: HashMap::new(),
            token_colors: vec![],
        };

        let theme = convert_vscode_theme(&vscode);
        assert!(!theme.is_dark);
    }

    #[test]
    fn convert_editor_background() {
        let mut colors = HashMap::new();
        colors.insert("editor.background".to_string(), "#282c34".to_string());

        let vscode = VsCodeTheme {
            name: "With BG".to_string(),
            theme_type: "dark".to_string(),
            colors,
            token_colors: vec![],
        };

        let theme = convert_vscode_theme(&vscode);
        // Verify background was converted (exact values depend on hex parsing)
        assert!(theme.editor.background.r > 0.1);
    }

    #[test]
    fn convert_token_colors_keyword() {
        let token_colors = vec![VsCodeTokenColor {
            name: Some("Keywords".to_string()),
            scope: VsCodeScope::Single("keyword".to_string()),
            settings: VsCodeTokenSettings {
                foreground: Some("#ff0000".to_string()),
                background: None,
                font_style: None,
            },
        }];

        let vscode = VsCodeTheme {
            name: "Tokens Test".to_string(),
            theme_type: "dark".to_string(),
            colors: HashMap::new(),
            token_colors,
        };

        let theme = convert_vscode_theme(&vscode);
        // Red keyword color
        assert!((theme.syntax.keyword.r - 1.0).abs() < 0.01);
        assert!(theme.syntax.keyword.g < 0.01);
        assert!(theme.syntax.keyword.b < 0.01);
    }

    #[test]
    fn convert_multiple_scopes() {
        let token_colors = vec![VsCodeTokenColor {
            name: Some("Strings".to_string()),
            scope: VsCodeScope::Multiple(vec!["string".to_string(), "string.quoted".to_string()]),
            settings: VsCodeTokenSettings {
                foreground: Some("#00ff00".to_string()),
                background: None,
                font_style: None,
            },
        }];

        let vscode = VsCodeTheme {
            name: "Multi Scope".to_string(),
            theme_type: "dark".to_string(),
            colors: HashMap::new(),
            token_colors,
        };

        let theme = convert_vscode_theme(&vscode);
        // Green string color
        assert!(theme.syntax.string.g > 0.9);
    }

    #[test]
    fn parse_vscode_json() {
        let json = r##"{
            "name": "JSON Theme",
            "type": "dark",
            "colors": {
                "editor.background": "#1e1e1e"
            },
            "tokenColors": [
                {
                    "scope": "keyword",
                    "settings": {
                        "foreground": "#569cd6"
                    }
                }
            ]
        }"##;

        let vscode: VsCodeTheme = serde_json::from_str(json).unwrap();
        assert_eq!(vscode.name, "JSON Theme");
        assert_eq!(vscode.token_colors.len(), 1);
    }
}
