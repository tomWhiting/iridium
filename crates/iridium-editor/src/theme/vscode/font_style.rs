//! `fontStyle`, as VS Code writes it and as Iridium can wear it.
//!
//! A `tokenColors` rule's `fontStyle` is a **space-separated set**, not an
//! enum: `"italic"`, `"bold"`, `"bold italic"`, `"underline"`,
//! `"strikethrough"` and combinations of those are all valid, and so is the
//! empty string.
//!
//! # ⚠️ The empty string is the trap, and it is common
//!
//! `"fontStyle": ""` does not mean "unspecified". It means **explicitly
//! regular**, and themes use it constantly to cancel a broader rule: a theme
//! bolds all of `keyword` and then writes `"fontStyle": ""` on
//! `keyword.operator` so operators do not inherit it.
//!
//! Reading it as absent would leave the broader rule's bold in place. The two
//! cases mean opposite things, and they are kept apart **before** this function
//! is reached: the caller maps over `Option<&str>`, so a missing key never
//! calls [`parse`] at all and an empty one calls it with `""`.
//!
//! | JSON | reaches `parse` | the import does |
//! | --- | --- | --- |
//! | no `fontStyle` key | never called | leaves any earlier rule alone |
//! | `"fontStyle": ""` | `parse("")` → body weight, upright | **overwrites** any earlier rule with regular |
//! | `"fontStyle": "bold"` | `parse("bold")` → bold, upright | sets bold |
//!
//! ⚠️ Note the middle row does not answer [`Emphasis::default()`], which has
//! both fields absent and would mean "say nothing about weight or slant". It
//! answers *body weight and upright, stated* — the difference between declining
//! to speak and saying regular out loud, which is the whole of what `""` is
//! for.
//!
//! # ⛔ Two styles are recognised and deliberately dropped
//!
//! `underline` and `strikethrough` have no home in
//! [`RunStyle`](crate::render::RunStyle) and must not acquire one by accident:
//! they are **quad geometry**, a line drawn under or through a run, not a font
//! attribute cosmic-text can shape.
//!
//! They are named in the match below rather than left to the catch-all, and
//! `the_two_undrawable_styles_change_nothing` pins the drop — so a later change
//! that quietly routed `underline` onto italic, which is the tempting
//! approximation, fails instead of shipping.
//!
//! ⚠️ **Nobody is told.** A theme asking for underline imports without it and
//! without a word, which is a real if small version of the defect this whole
//! module was fixed for. It is left that way deliberately: themes have no
//! diagnostics channel at all — `iridium_config`'s `Problem` collection has
//! sections for the file, `[editor]` and `[keys]`, and none for a theme — so
//! reporting this means building that channel, which is theme work (#87) and
//! not a line to sneak in here. Named so it is a known gap rather than a
//! forgotten one.

use crate::render::{RunSlant, RunWeight};
use crate::theme::Emphasis;

/// Reads a `fontStyle` value.
///
/// The empty string is a valid, meaningful value — explicitly regular — and
/// comes back as an [`Emphasis`] that says both weight and slant are body text,
/// **not** as an empty answer. See the module note for why the distinction is
/// load-bearing.
///
/// ⭐ Every named style is stated explicitly rather than left absent. A theme
/// writing `"italic"` means *italic and body weight*, so the weight is written
/// as `Some(NORMAL)`: leaving it `None` would let a rule earlier in the file
/// keep its bold, and VS Code's own semantics replace the whole `fontStyle` of
/// the more specific rule rather than merging with it.
///
/// Unknown words are ignored rather than refused. `tokenColors` is a vendored
/// document from an ecosystem that adds vocabulary, and a theme that reaches
/// this parser has already been accepted; failing the whole import over one
/// unrecognised word would trade a complete theme for a blank one.
#[must_use]
pub fn parse(font_style: &str) -> Emphasis {
    let mut emphasis = Emphasis {
        weight: Some(RunWeight::NORMAL),
        slant: Some(RunSlant::Upright),
    };

    for word in font_style.split_whitespace() {
        // ASCII-lowercased rather than `to_lowercase`: these are fixed ASCII
        // keywords, and a locale-sensitive fold has nothing to contribute and
        // a Turkish dotless-i to get wrong.
        match word.to_ascii_lowercase().as_str() {
            "bold" => emphasis.weight = Some(RunWeight::BOLD),
            "italic" => emphasis.slant = Some(RunSlant::Italic),
            // ⛔ `underline` and `strikethrough` fall to this arm and are
            // dropped. They cannot be *named* here — clippy's
            // `match_same_arms` refuses an arm whose body matches the
            // wildcard's, and this crate takes no `#[allow]` bypasses — so the
            // distinction between "recognised but undrawable" and "unknown"
            // lives in the module note and in
            // `the_two_undrawable_styles_change_nothing`, which is a test and
            // therefore fails rather than merely reading well.
            _ => {},
        }
    }

    emphasis
}

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::render::{RunSlant, RunWeight};

    #[test]
    fn bold_is_bold_and_upright() {
        assert_eq!(parse("bold").weight, Some(RunWeight::BOLD));
        assert_eq!(parse("bold").slant, Some(RunSlant::Upright));
    }

    #[test]
    fn italic_is_italic_and_body_weight() {
        assert_eq!(parse("italic").weight, Some(RunWeight::NORMAL));
        assert_eq!(parse("italic").slant, Some(RunSlant::Italic));
    }

    /// The set is a set: order must not matter, and both spellings appear in
    /// real themes.
    #[test]
    fn both_orderings_of_bold_italic_agree() {
        assert_eq!(parse("bold italic"), parse("italic bold"));
        assert_eq!(parse("bold italic").weight, Some(RunWeight::BOLD));
        assert_eq!(parse("bold italic").slant, Some(RunSlant::Italic));
    }

    /// ⭐ The trap. `""` is explicitly regular and must state both fields, or
    /// it cannot cancel the broader rule it exists to cancel.
    #[test]
    fn the_empty_string_states_regular_rather_than_saying_nothing() {
        assert_eq!(
            parse("").weight,
            Some(RunWeight::NORMAL),
            "an empty fontStyle must say body weight out loud — saying nothing \
             leaves an earlier rule's bold in place, which is the whole reason \
             a theme writes it"
        );
        assert_eq!(parse("").slant, Some(RunSlant::Upright));
    }

    /// ⚠️ Whitespace-only is the same case as empty, and reaches it by a
    /// different route: `split_whitespace` yields nothing either way.
    #[test]
    fn whitespace_only_is_the_empty_case() {
        assert_eq!(parse("   \t "), parse(""));
    }

    /// ⭐ The drop is pinned, because the tempting approximation is to route
    /// `underline` onto italic or bold "so something shows". That would make a
    /// theme's underlined tokens lean, which the theme never asked for and
    /// nobody could trace back to here.
    #[test]
    fn the_two_undrawable_styles_change_nothing() {
        assert_eq!(parse("underline"), parse(""));
        assert_eq!(parse("strikethrough"), parse(""));
        assert_eq!(
            parse("italic underline strikethrough"),
            parse("italic"),
            "the styles that can be drawn are still drawn, and the two that \
             cannot leave no trace"
        );
    }

    /// Case is folded — themes in the wild write `Italic` and `Bold`.
    #[test]
    fn case_does_not_matter() {
        assert_eq!(parse("Bold Italic"), parse("bold italic"));
        assert_eq!(parse("ITALIC"), parse("italic"));
    }

    /// An unrecognised word must not take the theme down with it, and must not
    /// quietly alter what the recognised words said.
    #[test]
    fn an_unknown_word_is_ignored_without_disturbing_the_rest() {
        assert_eq!(parse("bold sparkly"), parse("bold"));
    }
}
