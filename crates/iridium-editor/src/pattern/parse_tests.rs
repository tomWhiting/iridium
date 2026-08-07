//! What the query field's text is read as.

use super::{Pattern, REGEX_SIGIL};

#[test]
fn plain_text_is_fuzzy() {
    assert!(matches!(Pattern::parse("panel"), Pattern::Fuzzy(_)));
}

#[test]
fn the_sigil_makes_the_rest_a_regular_expression() {
    assert!(matches!(Pattern::parse("/pan.l"), Pattern::Regex(_)));
}

#[test]
fn nothing_typed_is_not_a_filter() {
    assert!(matches!(Pattern::parse(""), Pattern::Unfiltered));
    assert!(matches!(Pattern::parse("   "), Pattern::Unfiltered));
}

#[test]
fn the_sigil_alone_is_not_a_filter_either() {
    // **The one that would have been a real bug.** An empty regular
    // expression matches every string, so a bare `/` compiled and applied
    // would put every row on screen — and, in a panel that reads the disk to
    // fill its rows, would send it reading. Pressing the key that means "a
    // pattern is coming" must not do anything until the pattern arrives.
    let parsed = Pattern::parse("/");
    assert!(matches!(parsed, Pattern::Unfiltered));
    assert!(!parsed.is_narrowing());
    assert!(!parsed.can_match());
}

#[test]
fn a_half_typed_pattern_narrows_without_reading_anything() {
    // `/[` is two keystrokes into `/[a-z]`. It must not put the tree back on
    // screen — that flash reads as the filter giving up — and it must not
    // send anyone reading the disk on behalf of a pattern that cannot match.
    let parsed = Pattern::parse("/[");
    assert!(parsed.is_narrowing(), "the rows still come from the filter");
    assert!(!parsed.can_match(), "but nothing is worth reading for it");
    assert!(
        parsed.invalid_reason().is_some(),
        "and a face can say which of the two it is looking at"
    );
}

#[test]
fn an_invalid_pattern_carries_one_line_a_panel_can_draw() {
    let Some(reason) = Pattern::parse("/[").invalid_reason().map(str::to_owned) else {
        panic!("an unclosed character class does not compile");
    };
    assert!(
        !reason.contains('\n'),
        "a panel row is one line: {reason:?}"
    );
    assert!(
        !reason.is_empty() && !reason.contains("regex parse error"),
        "the reason, not the heading and the caret diagram: {reason:?}"
    );
}

#[test]
fn every_state_answers_both_questions_the_same_way_twice() {
    // The two flags are separate on purpose, and this is the table that says
    // how they differ. Conflating them is what sends a panel crawling a
    // filesystem for a pattern that cannot match anything.
    for (text, narrowing, matching) in [
        ("", false, false),
        ("/", false, false),
        ("panel", true, true),
        ("/pan.l", true, true),
        ("/[", true, false),
    ] {
        let parsed = Pattern::parse(text);
        assert_eq!(parsed.is_narrowing(), narrowing, "narrowing for {text:?}");
        assert_eq!(parsed.can_match(), matching, "matching for {text:?}");
    }
}

#[test]
fn a_pattern_that_compiles_has_nothing_to_complain_about() {
    assert_eq!(Pattern::parse("/pan.l").invalid_reason(), None);
    assert_eq!(Pattern::parse("panel").invalid_reason(), None);
    assert_eq!(Pattern::parse("").invalid_reason(), None);
}

#[test]
fn the_sigil_is_the_first_character_and_only_the_first() {
    // A `/` inside the pattern is a path separator being matched, not a
    // second sigil. `src/.*` is the obvious thing to type and it must not be
    // read as a fuzzy query.
    assert!(matches!(Pattern::parse("/src/.*"), Pattern::Regex(_)));
    // And one in the middle of plain text stays fuzzy.
    assert!(matches!(Pattern::parse("src/panel"), Pattern::Fuzzy(_)));
}

#[test]
fn the_sigil_constant_is_what_the_parser_actually_looks_for() {
    // Pins the constant to the behaviour, so that changing one and not the
    // other cannot leave the documentation describing a key that does
    // nothing.
    let text = format!("{REGEX_SIGIL}pan.l");
    assert!(matches!(Pattern::parse(&text), Pattern::Regex(_)));
}
