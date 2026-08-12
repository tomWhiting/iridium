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

        // ----- Markup -----
        //
        // ⭐ **The six that already reached the screen keep the exact colour
        // they had before markup was split off the code slots.** Giving markup
        // its own categories is what lets a theme style a heading without
        // touching every `fn` in every language; it is emphatically not a
        // change to what markdown looks like today, and
        // `the_markup_slots_render_exactly_as_they_did_before_they_had_slots`
        // is what stops one arriving by accident.
        HighlightType::MarkupHeading => syntax.keyword,
        HighlightType::MarkupLink => syntax.function,
        HighlightType::MarkupUrl => syntax.string,
        HighlightType::MarkupList
        | HighlightType::MarkupPunctuation
        | HighlightType::MarkupFence => syntax.punctuation,

        // The four inline categories had no mapping at all before the split —
        // `emphasis.markup` and its siblings live in `markdown-inline`, which
        // has no grammar linked, so nothing has ever asked for their colour.
        // They are ruled here rather than left to fall through, because a
        // category with no colour is the same silent-unstyled defect #70
        // cleared and there is no reason to reintroduce it and wait.
        //
        // Emphasis and strong emphasis take `operator`. Neither wants a colour
        // of its own — emphasised prose is body text wearing weight or slant,
        // which is what `render::RunStyle` carries — and `operator` is the
        // slot that is plain page ink in both shipped presets. That agreement
        // is a fact about today's themes rather than about the categories, so
        // `markup_emphasis_borrows_a_slot_that_is_still_plain_ink` fails the
        // moment a theme parts the two and markup needs its own field.
        //
        // Strikethrough takes `comment`: withdrawn text is recessive, and
        // `comment` is the recessive slot every theme already has. An inline
        // code span takes `string` — it is a literal, and reads as one.
        HighlightType::MarkupEmphasis | HighlightType::MarkupStrong => syntax.operator,
        HighlightType::MarkupStrikethrough => syntax.comment,
        HighlightType::MarkupCode => syntax.string,

        // Errors
        HighlightType::Error => syntax.error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    #[test]
    fn highlight_to_color_keywords() {
        let colors = SyntaxColors::dark();
        let color = highlight_to_color(HighlightType::Keyword, &colors);
        assert_eq!(color, colors.keyword);
    }

    /// ⭐ Splitting markup off the code categories must be invisible.
    ///
    /// The six markup captures that reach the screen today did so by *being*
    /// a code category — a heading was a `Keyword`, a link a `Function`, a
    /// list marker a `PunctuationDelimiter` reached through the prefix
    /// fallback. Giving each its own category is what makes markup styleable
    /// on its own; if it also silently repainted markdown, the taxonomy change
    /// and a colour change would have landed as one commit and neither could
    /// be reviewed.
    ///
    /// Compared against the code category rather than against a hard-coded
    /// colour, so this stays true under any theme and keeps saying the thing
    /// it means: *these render as they did*.
    #[test]
    fn the_markup_slots_render_exactly_as_they_did_before_they_had_slots() {
        for colors in [SyntaxColors::dark(), SyntaxColors::light()] {
            for (markup, code, capture) in [
                (
                    HighlightType::MarkupHeading,
                    HighlightType::Keyword,
                    "title.markup",
                ),
                (
                    HighlightType::MarkupLink,
                    HighlightType::Function,
                    "link_text.markup",
                ),
                (
                    HighlightType::MarkupUrl,
                    HighlightType::String,
                    "link_uri.markup",
                ),
                (
                    HighlightType::MarkupList,
                    HighlightType::PunctuationDelimiter,
                    "punctuation.list_marker.markup",
                ),
                (
                    HighlightType::MarkupPunctuation,
                    HighlightType::PunctuationDelimiter,
                    "punctuation.markup",
                ),
                (
                    HighlightType::MarkupFence,
                    HighlightType::PunctuationDelimiter,
                    "punctuation.embedded.markup",
                ),
            ] {
                assert_eq!(
                    highlight_to_color(markup, &colors),
                    highlight_to_color(code, &colors),
                    "@{capture} used to render as {code:?} and must still — \
                     the split is a taxonomy change, not a repaint"
                );
            }
        }
    }

    /// The tripwire under the emphasis ruling, kept as a test because a
    /// comment claiming it would go stale silently.
    ///
    /// `MarkupEmphasis` and `MarkupStrong` borrow `operator` on the grounds
    /// that emphasised prose is body text wearing weight or slant, and that
    /// `operator` is plain page ink in both shipped presets. The second half is
    /// a fact about the themes, not about the categories — so the moment a
    /// theme parts `operator` from the page ink, that borrowing is wrong and
    /// markup needs a colour field of its own. This fails there, naming why,
    /// rather than letting bold markdown drift to whatever colour operators
    /// happen to have become.
    #[test]
    fn markup_emphasis_borrows_a_slot_that_is_still_plain_ink() {
        for theme in [Theme::dark(), Theme::light()] {
            assert_eq!(
                highlight_to_color(HighlightType::MarkupEmphasis, &theme.syntax),
                theme.editor.foreground,
                "emphasised markup borrows `operator` only while `operator` is \
                 the page ink. It no longer is, so `SyntaxColors` needs its own \
                 markup fields now — see the note on the markup arms in \
                 `highlight_to_color`"
            );
        }
    }
}
