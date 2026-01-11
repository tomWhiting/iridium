//! Typography settings for themes.

use serde::{Deserialize, Serialize};

/// Typography settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Typography {
    /// Font family name
    pub font_family: String,
    /// Font size in pixels
    pub font_size: f32,
    /// Line height as a multiplier
    pub line_height: f32,
    /// Letter spacing in pixels
    pub letter_spacing: f32,
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            font_family: "JetBrains Mono, Fira Code, Menlo, Monaco, monospace".to_string(),
            font_size: 14.0,
            line_height: 1.5,
            letter_spacing: 0.0,
        }
    }
}

impl Typography {
    /// Returns the computed line height in pixels.
    #[must_use]
    pub fn line_height_px(&self) -> f32 {
        self.font_size * self.line_height
    }
}
