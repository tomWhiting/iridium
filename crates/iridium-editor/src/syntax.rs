//! Syntax highlighting integration.
//!
//! This module bridges `iridium_syntax` with the editor's theme system,
//! providing syntax highlighting for the document.

use crate::render::RunStyle;
use crate::theme::{Color, SyntaxColors, SyntaxEmphasis, slot_of};

// Re-export core syntax types
pub use iridium_syntax::{
    HighlightSpan, HighlightType, Highlighter, Language, SyntaxError, SyntaxTree, has_grammar,
};

/// Converts a `HighlightType` to a `Color` using the theme's syntax colors.
///
/// This function maps semantic highlight types to concrete colors from the theme.
#[must_use]
pub const fn highlight_to_color(highlight: HighlightType, syntax: &SyntaxColors) -> Color {
    syntax.slot(slot_of(highlight))
}

/// Resolves a `HighlightType` to the full style a theme draws it in.
///
/// ⭐ **The one place a highlight becomes an appearance, for every face.**
/// [`highlight_to_color`] answers the colour half and this adds the face half,
/// so a terminal and a GPU surface reading one theme cannot disagree about
/// what a heading looks like. A face computing either half for itself would
/// drift the first time a category was added — which is why the desktop
/// resolver and the TUI palette both call in here rather than mapping their
/// own.
///
/// The two halves are keyed differently and deliberately so: colour resolves
/// through a many-to-one map onto [`SyntaxColors`]'s fourteen fields, while
/// emphasis is keyed by category. See [`SyntaxEmphasis`] for why collapsing
/// them would bold every keyword the moment a theme bolded headings.
///
/// A theme that says nothing about `highlight` — which is both shipped presets,
/// for every category — returns exactly [`RunStyle::plain`] of the colour
/// [`highlight_to_color`] gives. That identity is what keeps retained shaping
/// from reshaping every line; it is asserted, not assumed.
#[must_use]
pub fn highlight_to_style(
    highlight: HighlightType,
    syntax: &SyntaxColors,
    emphasis: &SyntaxEmphasis,
) -> RunStyle {
    emphasis.applied_to(
        highlight,
        RunStyle::plain(highlight_to_color(highlight, syntax)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{RunSlant, RunWeight};
    use crate::theme::{Emphasis, Theme};

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

    /// The four diff status categories borrow four slots, and the borrow is
    /// pinned here rather than described in a comment.
    ///
    /// ⚠️ **This is not a colour ruling and must not be read as one.** No
    /// `SyntaxColors` field means "added" or "removed"; added takes `string`
    /// and removed takes `keyword` because the vendored `diff` query names
    /// exactly those two as its own fallbacks, in its own text. Once a theme
    /// grows diff fields (#87) these arms should change, and this test is what
    /// makes that change deliberate rather than incidental.
    ///
    /// ⭐ Modified and moved *share* a colour on purpose — a many-to-one colour
    /// map is the design — while keeping separate categories, which is what
    /// lets a theme part them later. The assertion pair says both halves, so
    /// collapsing the categories to match the colours fails here.
    #[test]
    fn the_diff_slots_borrow_the_colours_they_were_ruled_to_borrow() {
        for colors in [SyntaxColors::dark(), SyntaxColors::light()] {
            for (diff, borrowed, slot) in [
                (HighlightType::DiffAdded, colors.string, "string"),
                (HighlightType::DiffRemoved, colors.keyword, "keyword"),
                (HighlightType::DiffModified, colors.attribute, "attribute"),
                (HighlightType::DiffMoved, colors.attribute, "attribute"),
            ] {
                assert_eq!(
                    highlight_to_color(diff, &colors),
                    borrowed,
                    "{diff:?} no longer borrows the {slot} slot"
                );
            }
        }

        assert_ne!(
            HighlightType::DiffModified,
            HighlightType::DiffMoved,
            "a renamed file and an edited one share a colour and must not \
             share a category, or no theme can ever tell them apart"
        );
    }

    /// ⭐ The identity every face and the retained shaper depend on: a theme
    /// that says nothing about emphasis produces exactly what the colour-only
    /// path produced.
    ///
    /// Both shipped presets say nothing, so this is not a hypothetical corner —
    /// it is what every frame does today. `set_rich_text_diffed` reshapes a line
    /// whose attributes differ from the last frame's, so an equal-but-rebuilt
    /// style would be a full reshape with no visible symptom to trace it by.
    #[test]
    fn an_empty_emphasis_table_is_byte_identical_to_the_colour_only_answer() {
        let quiet = SyntaxEmphasis::none();
        for theme in [Theme::dark(), Theme::light()] {
            for highlight in [
                HighlightType::Keyword,
                HighlightType::MarkupHeading,
                HighlightType::MarkupEmphasis,
                HighlightType::Comment,
                HighlightType::String,
            ] {
                assert_eq!(
                    highlight_to_style(highlight, &theme.syntax, &quiet),
                    RunStyle::plain(highlight_to_color(highlight, &theme.syntax)),
                    "{highlight:?} under {} must be byte-identical to the \
                     colour-only answer",
                    theme.name
                );
            }
        }
    }

    /// What the shipped presets actually say, pinned separately from the
    /// identity above.
    ///
    /// ⚠️ These two claims were one test and should not have been. "An empty
    /// table changes nothing" is a property of the mechanism; "the presets are
    /// empty" is a fact about two files that will stop being true the moment a
    /// preset bolds a heading — and the mechanism's invariant must survive
    /// that untouched. Merged, a preset gaining one entry would have taken the
    /// invariant's proof down with it and looked like the invariant breaking.
    #[test]
    fn the_shipped_presets_say_nothing_about_emphasis_yet() {
        for theme in [Theme::dark(), Theme::light()] {
            assert!(
                theme.emphasis.is_all_body_text(),
                "{} now emphasises something. That is a look change and needs \
                 to be a deliberate one — update this test with the ruling, \
                 and note that a weight only reaches the screen if the face \
                 the shaper resolves has that weight, which is a claim needing \
                 its own evidence",
                theme.name
            );
        }
    }

    /// ⭐ The capability the whole rich-text lane is for, end to end: a theme
    /// can now say "headings are bold" and mean only headings.
    ///
    /// Asserted against a keyword rather than against a heading alone, because
    /// "the heading got bold" was never in doubt — what was in doubt, and what
    /// the markup split and the category keying exist to fix, is that nothing
    /// *else* did. The two still share a colour, deliberately.
    #[test]
    fn a_theme_can_bold_headings_without_bolding_a_single_keyword() {
        let mut theme = Theme::dark();
        theme.emphasis = theme
            .emphasis
            .clone()
            .with(HighlightType::MarkupHeading, Emphasis::bold())
            .with(HighlightType::MarkupEmphasis, Emphasis::italic());

        let heading =
            highlight_to_style(HighlightType::MarkupHeading, &theme.syntax, &theme.emphasis);
        let keyword = highlight_to_style(HighlightType::Keyword, &theme.syntax, &theme.emphasis);
        let emphasis = highlight_to_style(
            HighlightType::MarkupEmphasis,
            &theme.syntax,
            &theme.emphasis,
        );

        assert_eq!(heading.weight, RunWeight::BOLD);
        assert_eq!(emphasis.slant, RunSlant::Italic);
        assert_eq!(
            keyword.weight,
            RunWeight::NORMAL,
            "a keyword shares the heading's colour and must not have gained \
             its weight"
        );
        assert_eq!(
            keyword.slant,
            RunSlant::Upright,
            "and nothing else leaned either"
        );
        assert_eq!(
            heading.color, keyword.color,
            "they still share a colour — that sharing was never the defect"
        );
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
