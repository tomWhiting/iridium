//! Theme and styling configuration.
//!
//! This module provides theming support for the Iridium editor including:
//! - Runtime theme loading from JSON
//! - VS Code theme compatibility
//! - Light and dark theme presets
//! - Full color and typography customization

mod colors;
mod fonts;
mod vscode;

pub use colors::{Color, EditorColors, SyntaxColors};
pub use fonts::Typography;
pub use vscode::{VsCodeTheme, VsCodeTokenColor};

use serde::{Deserialize, Serialize};

/// Error type for theme operations.
#[derive(Debug, Clone)]
pub enum ThemeError {
    /// JSON parsing failed
    ParseError(String),
    /// Required field is missing
    MissingField(String),
    /// Invalid color format
    InvalidColor(String),
}

impl std::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParseError(msg) => write!(f, "Theme parse error: {msg}"),
            Self::MissingField(field) => write!(f, "Missing required theme field: {field}"),
            Self::InvalidColor(color) => write!(f, "Invalid color format: {color}"),
        }
    }
}

impl std::error::Error for ThemeError {}

/// Complete theme configuration.
///
/// # JSON Format
///
/// Themes can be loaded from JSON with the following structure:
///
/// ```json
/// {
///   "name": "My Theme",
///   "is_dark": true,
///   "editor": {
///     "background": "#1e1e1e",
///     "foreground": "#d4d4d4",
///     ...
///   },
///   "syntax": {
///     "keyword": "#569cd6",
///     ...
///   },
///   "typography": {
///     "font_family": "JetBrains Mono",
///     "font_size": 14.0,
///     ...
///   }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    /// Theme name
    pub name: String,
    /// Whether this is a dark theme
    pub is_dark: bool,
    /// Editor chrome colors
    pub editor: EditorColors,
    /// Syntax highlighting colors
    pub syntax: SyntaxColors,
    /// Typography settings
    pub typography: Typography,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// Creates the default dark theme.
    #[must_use]
    pub fn dark() -> Self {
        Self {
            name: "Iridium Dark".to_string(),
            is_dark: true,
            editor: EditorColors::dark(),
            syntax: SyntaxColors::dark(),
            typography: Typography::default(),
        }
    }

    /// Creates the default light theme.
    #[must_use]
    pub fn light() -> Self {
        Self {
            name: "Iridium Light".to_string(),
            is_dark: false,
            editor: EditorColors::light(),
            syntax: SyntaxColors::light(),
            typography: Typography::default(),
        }
    }

    /// Creates a theme from a JSON string (T146).
    ///
    /// This parses a JSON theme definition and returns a fully configured Theme.
    /// The JSON should match the Theme struct's serialization format.
    ///
    /// # Arguments
    ///
    /// * `json` - A JSON string containing the theme definition
    ///
    /// # Errors
    ///
    /// Returns `ThemeError::ParseError` if the JSON is invalid.
    ///
    /// # Example
    ///
    /// ```
    /// use iridium_editor::theme::Theme;
    ///
    /// let json = r#"{
    ///     "name": "Custom Dark",
    ///     "is_dark": true,
    ///     "editor": {
    ///         "background": {"r": 0.1, "g": 0.1, "b": 0.1, "a": 1.0},
    ///         "foreground": {"r": 0.9, "g": 0.9, "b": 0.9, "a": 1.0},
    ///         "selection": {"r": 0.3, "g": 0.5, "b": 0.7, "a": 0.5},
    ///         "selection_inactive": {"r": 0.3, "g": 0.5, "b": 0.7, "a": 0.3},
    ///         "cursor": {"r": 0.9, "g": 0.9, "b": 0.9, "a": 1.0},
    ///         "line_number": {"r": 0.5, "g": 0.5, "b": 0.5, "a": 1.0},
    ///         "line_number_active": {"r": 0.8, "g": 0.8, "b": 0.8, "a": 1.0},
    ///         "current_line": {"r": 0.15, "g": 0.15, "b": 0.15, "a": 1.0},
    ///         "gutter": {"r": 0.1, "g": 0.1, "b": 0.1, "a": 1.0},
    ///         "minimap_background": {"r": 0.1, "g": 0.1, "b": 0.1, "a": 1.0},
    ///         "search_match": {"r": 0.5, "g": 0.4, "b": 0.1, "a": 0.6},
    ///         "search_match_current": {"r": 0.8, "g": 0.6, "b": 0.2, "a": 0.8}
    ///     },
    ///     "syntax": {
    ///         "keyword": {"r": 0.34, "g": 0.61, "b": 0.84, "a": 1.0},
    ///         "string": {"r": 0.81, "g": 0.57, "b": 0.47, "a": 1.0},
    ///         "number": {"r": 0.71, "g": 0.81, "b": 0.66, "a": 1.0},
    ///         "comment": {"r": 0.42, "g": 0.6, "b": 0.33, "a": 1.0},
    ///         "function": {"r": 0.86, "g": 0.86, "b": 0.67, "a": 1.0},
    ///         "variable": {"r": 0.61, "g": 0.86, "b": 0.99, "a": 1.0},
    ///         "type_name": {"r": 0.31, "g": 0.79, "b": 0.69, "a": 1.0},
    ///         "operator": {"r": 0.83, "g": 0.83, "b": 0.83, "a": 1.0},
    ///         "punctuation": {"r": 0.83, "g": 0.83, "b": 0.83, "a": 1.0},
    ///         "property": {"r": 0.61, "g": 0.86, "b": 0.99, "a": 1.0},
    ///         "constant": {"r": 0.31, "g": 0.76, "b": 1.0, "a": 1.0},
    ///         "tag": {"r": 0.34, "g": 0.61, "b": 0.84, "a": 1.0},
    ///         "attribute": {"r": 0.61, "g": 0.86, "b": 0.99, "a": 1.0},
    ///         "error": {"r": 0.96, "g": 0.28, "b": 0.28, "a": 1.0}
    ///     },
    ///     "typography": {
    ///         "font_family": "JetBrains Mono",
    ///         "font_size": 14.0,
    ///         "line_height": 1.5,
    ///         "letter_spacing": 0.0
    ///     }
    /// }"#;
    ///
    /// let theme = Theme::from_json(json).unwrap();
    /// assert_eq!(theme.name, "Custom Dark");
    /// ```
    pub fn from_json(json: &str) -> Result<Self, ThemeError> {
        serde_json::from_str(json).map_err(|e| ThemeError::ParseError(e.to_string()))
    }

    /// Creates a theme from a JSON value.
    ///
    /// # Errors
    ///
    /// Returns `ThemeError::ParseError` if the JSON value doesn't match the expected format.
    pub fn from_json_value(value: serde_json::Value) -> Result<Self, ThemeError> {
        serde_json::from_value(value).map_err(|e| ThemeError::ParseError(e.to_string()))
    }

    /// Converts the theme to a JSON string.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails (should not happen for valid themes).
    pub fn to_json(&self) -> Result<String, ThemeError> {
        serde_json::to_string_pretty(self).map_err(|e| ThemeError::ParseError(e.to_string()))
    }

    /// Creates a theme from a VS Code theme definition (T151).
    ///
    /// VS Code themes use a different format with tokenColors arrays.
    /// This method converts the VS Code format to Iridium's native format.
    ///
    /// # Arguments
    ///
    /// * `vscode_theme` - A VS Code theme definition
    ///
    /// # Example
    ///
    /// ```
    /// use iridium_editor::theme::{Theme, VsCodeTheme};
    ///
    /// let vscode_json = r##"{
    ///     "name": "My VS Code Theme",
    ///     "type": "dark",
    ///     "colors": {
    ///         "editor.background": "#1e1e1e",
    ///         "editor.foreground": "#d4d4d4"
    ///     },
    ///     "tokenColors": []
    /// }"##;
    ///
    /// let vscode_theme: VsCodeTheme = serde_json::from_str(vscode_json).unwrap();
    /// let theme = Theme::from_vscode(&vscode_theme);
    /// assert_eq!(theme.name, "My VS Code Theme");
    /// assert!(theme.is_dark);
    /// ```
    #[must_use]
    pub fn from_vscode(vscode_theme: &VsCodeTheme) -> Self {
        vscode::convert_vscode_theme(vscode_theme)
    }

    /// Loads a theme from a VS Code theme JSON string (T151).
    ///
    /// This is a convenience method that parses VS Code JSON and converts it.
    ///
    /// # Errors
    ///
    /// Returns `ThemeError::ParseError` if the JSON is invalid.
    pub fn from_vscode_json(json: &str) -> Result<Self, ThemeError> {
        let vscode_theme: VsCodeTheme =
            serde_json::from_str(json).map_err(|e| ThemeError::ParseError(e.to_string()))?;
        Ok(Self::from_vscode(&vscode_theme))
    }

    /// Merges another theme's values into this one.
    ///
    /// Only non-default values from `other` are applied. This is useful
    /// for applying partial theme overrides.
    pub fn merge(&mut self, other: &Self) {
        // Merge editor colors
        self.editor = other.editor.clone();
        // Merge syntax colors
        self.syntax = other.syntax.clone();
        // Merge typography if font family is specified
        if !other.typography.font_family.is_empty() {
            self.typography = other.typography.clone();
        }
    }

    /// Returns a builder for creating custom themes.
    pub fn builder() -> ThemeBuilder {
        ThemeBuilder::new()
    }
}

/// Builder for creating custom themes.
///
/// # Example
///
/// ```
/// use iridium_editor::theme::{Theme, Color};
///
/// let theme = Theme::builder()
///     .name("Custom Theme")
///     .dark()
///     .background(Color::from_hex("#1a1a2e").unwrap())
///     .foreground(Color::from_hex("#eaeaea").unwrap())
///     .build();
///
/// assert_eq!(theme.name, "Custom Theme");
/// ```
#[must_use]
pub struct ThemeBuilder {
    theme: Theme,
}

impl ThemeBuilder {
    /// Creates a new theme builder with dark theme defaults.
    pub fn new() -> Self {
        Self {
            theme: Theme::dark(),
        }
    }

    /// Sets the theme name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.theme.name = name.into();
        self
    }

    /// Sets this as a dark theme.
    pub fn dark(mut self) -> Self {
        self.theme.is_dark = true;
        self.theme.editor = EditorColors::dark();
        self.theme.syntax = SyntaxColors::dark();
        self
    }

    /// Sets this as a light theme.
    pub fn light(mut self) -> Self {
        self.theme.is_dark = false;
        self.theme.editor = EditorColors::light();
        self.theme.syntax = SyntaxColors::light();
        self
    }

    /// Sets the background color.
    pub const fn background(mut self, color: Color) -> Self {
        self.theme.editor.background = color;
        self
    }

    /// Sets the foreground (text) color.
    pub const fn foreground(mut self, color: Color) -> Self {
        self.theme.editor.foreground = color;
        self
    }

    /// Sets the selection color.
    pub const fn selection(mut self, color: Color) -> Self {
        self.theme.editor.selection = color;
        self
    }

    /// Sets the cursor color.
    pub const fn cursor(mut self, color: Color) -> Self {
        self.theme.editor.cursor = color;
        self
    }

    /// Sets the current line highlight color.
    pub const fn current_line(mut self, color: Color) -> Self {
        self.theme.editor.current_line = color;
        self
    }

    /// Sets the keyword syntax color.
    pub const fn keyword_color(mut self, color: Color) -> Self {
        self.theme.syntax.keyword = color;
        self
    }

    /// Sets the string syntax color.
    pub const fn string_color(mut self, color: Color) -> Self {
        self.theme.syntax.string = color;
        self
    }

    /// Sets the comment syntax color.
    pub const fn comment_color(mut self, color: Color) -> Self {
        self.theme.syntax.comment = color;
        self
    }

    /// Sets the font family.
    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.theme.typography.font_family = family.into();
        self
    }

    /// Sets the font size.
    pub const fn font_size(mut self, size: f32) -> Self {
        self.theme.typography.font_size = size;
        self
    }

    /// Sets the line height multiplier.
    pub const fn line_height(mut self, height: f32) -> Self {
        self.theme.typography.line_height = height;
        self
    }

    /// Builds the final theme.
    pub fn build(self) -> Theme {
        self.theme
    }
}

impl Default for ThemeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_from_json_basic() {
        let json = serde_json::to_string(&Theme::dark()).unwrap();
        let theme = Theme::from_json(&json).unwrap();
        assert_eq!(theme.name, "Iridium Dark");
        assert!(theme.is_dark);
    }

    #[test]
    fn theme_from_json_invalid() {
        let result = Theme::from_json("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn theme_to_json() {
        let theme = Theme::dark();
        let json = theme.to_json().unwrap();
        assert!(json.contains("Iridium Dark"));
    }

    #[test]
    fn theme_builder() {
        let theme = Theme::builder()
            .name("Test Theme")
            .dark()
            .font_size(16.0)
            .build();

        assert_eq!(theme.name, "Test Theme");
        assert!(theme.is_dark);
        assert!((theme.typography.font_size - 16.0).abs() < f32::EPSILON);
    }

    #[test]
    fn theme_builder_light() {
        let theme = Theme::builder().name("Light Theme").light().build();

        assert_eq!(theme.name, "Light Theme");
        assert!(!theme.is_dark);
    }
}
