//! Syntax highlighting integration.
//!
//! This module bridges `iridium_syntax` with the editor's theme system,
//! providing syntax highlighting for the document.

use crate::theme::{Color, SyntaxColors};

// Re-export core syntax types
pub use iridium_syntax::{
    HighlightSpan, HighlightType, Highlighter, Language, SyntaxError, SyntaxTree, has_grammar,
};

/// Converts a `HighlightType` to a `Color` using the theme's syntax colors.
///
/// This function maps semantic highlight types to concrete colors from the theme.
#[must_use]
#[allow(clippy::match_same_arms)] // Different semantic types intentionally map to same color
pub const fn highlight_to_color(highlight: HighlightType, syntax: &SyntaxColors) -> Color {
    match highlight {
        // Keywords
        HighlightType::Keyword | HighlightType::KeywordControl => syntax.keyword,

        // Strings
        HighlightType::String | HighlightType::StringEscape => syntax.string,

        // Numbers and booleans
        HighlightType::Number | HighlightType::Boolean => syntax.number,

        // Comments
        HighlightType::Comment | HighlightType::CommentDoc => syntax.comment,

        // Functions
        HighlightType::Function
        | HighlightType::FunctionDefinition
        | HighlightType::FunctionMethod
        | HighlightType::FunctionSpecial => syntax.function,

        // Variables
        HighlightType::Variable
        | HighlightType::VariableParameter
        | HighlightType::VariableSpecial => syntax.variable,

        // Types
        HighlightType::Type | HighlightType::TypeBuiltin | HighlightType::TypeInterface => {
            syntax.type_name
        },

        // Operators
        HighlightType::Operator => syntax.operator,

        // Punctuation
        HighlightType::PunctuationBracket
        | HighlightType::PunctuationDelimiter
        | HighlightType::PunctuationSpecial => syntax.punctuation,

        // Properties
        HighlightType::Property => syntax.property,

        // Constants
        HighlightType::Constant => syntax.constant,

        // Rust-specific
        HighlightType::Lifetime => syntax.type_name, // Use type color for lifetimes

        // Attributes/decorators
        HighlightType::Attribute => syntax.attribute,

        // Tags (HTML/XML)
        HighlightType::Tag => syntax.tag,

        // Embedded content - use default foreground
        HighlightType::Embedded => syntax.variable,

        // Errors
        HighlightType::Error => syntax.error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_to_color_keywords() {
        let colors = SyntaxColors::dark();
        let color = highlight_to_color(HighlightType::Keyword, &colors);
        assert_eq!(color, colors.keyword);
    }
}
