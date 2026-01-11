//! Theme and styling configuration.

mod colors;
mod fonts;

pub use colors::{Color, EditorColors, SyntaxColors};
pub use fonts::Typography;

use serde::{Deserialize, Serialize};

/// Complete theme configuration.
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
}
