//! Auto-closing pairs, and the pair set being the language's own.
//!
//! Split out of a 1,690-line `behavior_tests.rs` for #92, on the rule the
//! file already carried. The harness and the shared fixtures are in
//! [`super`].

use super::*;

// ========== Auto-pairs ==========

#[test]
fn typing_opener_inserts_pair_with_caret_between() {
    let mut doc = Document::new("");
    let mut cursor = cursors_at(&[(0, 0)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('('),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "()");
    assert_eq!(heads(&cursor), vec![(0, 1)]);
}

#[test]
fn typing_closer_skips_over_existing_closer() {
    let mut doc = Document::new("()");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    let cmd = press(
        &mut handler,
        &ch(')'),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert!(cmd.is_some(), "skip-over must move the caret");
    assert_eq!(doc.text(), "()", "skip-over must not modify content");
    assert_eq!(heads(&cursor), vec![(0, 2)]);
}

#[test]
fn backspace_between_pair_deletes_both_halves() {
    let mut doc = Document::new("a()b");
    let mut cursor = cursors_at(&[(0, 2)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &BACKSPACE,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "ab");
    assert_eq!(heads(&cursor), vec![(0, 1)]);
}

#[test]
fn backspace_between_quote_pair_deletes_both_halves() {
    let mut doc = Document::new("\"\"");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &BACKSPACE,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "");
    assert_eq!(heads(&cursor), vec![(0, 0)]);
}

#[test]
fn backspace_between_non_empty_pair_deletes_single_character() {
    let mut doc = Document::new("(a)");
    let mut cursor = cursors_at(&[(0, 2)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &BACKSPACE,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "()");
    assert_eq!(heads(&cursor), vec![(0, 1)]);
}

#[test]
fn typing_opener_wraps_selection() {
    let mut doc = Document::new("abc");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 3))]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('('),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "(abc)");
    assert_eq!(selections(&cursor), vec![((0, 1), (0, 4))]);
}

#[test]
fn wrapping_backward_selection_preserves_orientation() {
    let mut doc = Document::new("abc");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 3), Position::new(0, 0))]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('['),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "[abc]");
    assert_eq!(selections(&cursor), vec![((0, 4), (0, 1))]);
}

#[test]
fn typing_closing_bracket_over_selection_replaces_it() {
    let mut doc = Document::new("abc");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 3))]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch(')'),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), ")");
    assert_eq!(heads(&cursor), vec![(0, 1)]);
}

#[test]
fn quote_directly_after_word_character_stays_single() {
    let mut doc = Document::new("don");
    let mut cursor = cursors_at(&[(0, 3)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('\''),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "don'");
    assert_eq!(heads(&cursor), vec![(0, 4)]);
}

#[test]
fn quote_after_non_word_character_pairs() {
    let mut doc = Document::new("a ");
    let mut cursor = cursors_at(&[(0, 2)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('"'),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "a \"\"");
    assert_eq!(heads(&cursor), vec![(0, 3)]);
}

#[test]
fn bracket_after_word_character_still_pairs() {
    let mut doc = Document::new("foo");
    let mut cursor = cursors_at(&[(0, 3)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('('),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    // The word-character suppression applies to quotes only.
    assert_eq!(doc.text(), "foo()");
    assert_eq!(heads(&cursor), vec![(0, 4)]);
}

// ========== The pair set is the language's ==========

/// ⭐ The one that costs a Rust user something every day. Rust's manifest
/// declares no `'` pair, because a lifetime is not a character literal.
#[test]
fn rust_does_not_pair_the_lifetime_quote() {
    let mut doc = doc_in(Language::Rust, "fn f<");
    let mut cursor = cursors_at(&[(0, 5)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('\''),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    // The apostrophe guard cannot catch this: `<` is not a word character.
    assert_eq!(
        doc.text(),
        "fn f<'",
        "a lifetime quote must not close itself"
    );
    assert_eq!(heads(&cursor), vec![(0, 6)]);
}

/// The pair Rust *does* declare still works — the fix subtracts, it does not
/// switch auto-pairing off for the language.
#[test]
fn rust_still_pairs_the_brackets_it_declares() {
    let mut doc = doc_in(Language::Rust, "fn f");
    let mut cursor = cursors_at(&[(0, 4)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('('),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "fn f()");
    assert_eq!(heads(&cursor), vec![(0, 5)]);
}

/// A closer the language never inserts is a character the user typed, so it
/// is written rather than stepped over.
#[test]
fn rust_writes_a_second_quote_instead_of_skipping_one() {
    let mut doc = doc_in(Language::Rust, "'a'");
    let mut cursor = cursors_at(&[(0, 2)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('\''),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "'a''", "skipping here would drop the keystroke");
    assert_eq!(heads(&cursor), vec![(0, 3)]);
}

/// Backspace uses the same set: a pair the language never inserted is not
/// collapsed by one keystroke.
#[test]
fn rust_backspace_does_not_eat_both_halves_of_a_char_literal() {
    let mut doc = doc_in(Language::Rust, "''");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &BACKSPACE,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "'");
    assert_eq!(heads(&cursor), vec![(0, 0)]);
}

/// Every subtraction the vendored manifests ask for, in one table. Each entry
/// is a character the editor paired in every language before this, and that
/// this language declares it does not pair.
#[test]
fn each_language_drops_the_pairs_its_manifest_does_not_declare() {
    for (language, typed) in [
        (Language::Rust, '\''),
        (Language::Rust, '`'),
        (Language::Json, '\''),
        (Language::Json, '`'),
        (Language::Yaml, '('),
        (Language::Yaml, '`'),
        (Language::GoMod, '{'),
        (Language::GoMod, '"'),
        (Language::GoWork, '['),
        (Language::Markdown, '\''),
        (Language::Markdown, '"'),
        (Language::Markdown, '`'),
        (Language::Diff, '('),
    ] {
        let mut doc = doc_in(language, "");
        let mut cursor = cursors_at(&[(0, 0)]);
        let mut handler = KeyboardHandler::new();

        press(
            &mut handler,
            &ch(typed),
            &mut doc,
            &mut cursor,
            &EditorConfig::default(),
        );

        assert_eq!(
            doc.text(),
            typed.to_string(),
            "{} pairs {typed:?} and its manifest does not declare it",
            language.id()
        );
    }
}

/// A language that has *not said* keeps every pair. `awl` carries no
/// `brackets` key at all, and neither does a document with no language —
/// three distinct silences, one answer.
#[test]
fn a_language_that_declares_no_brackets_keeps_every_pair() {
    for text in ["awl", ""] {
        let mut doc = Document::new("");
        if !text.is_empty() {
            doc.set_language(Some(text.to_owned()));
        }
        let mut cursor = cursors_at(&[(0, 0)]);
        let mut handler = KeyboardHandler::new();

        press(
            &mut handler,
            &ch('\''),
            &mut doc,
            &mut cursor,
            &EditorConfig::default(),
        );

        assert_eq!(
            doc.text(),
            "''",
            "a language that has not declared brackets must keep the defaults"
        );
    }
}

/// The pairs each manifest *does* declare still work, so the subtraction
/// cannot be passed by an implementation that simply stops pairing.
#[test]
fn each_language_keeps_the_pairs_its_manifest_does_declare() {
    for (language, typed, expected) in [
        (Language::Rust, '"', "\"\""),
        (Language::Json, '{', "{}"),
        (Language::Yaml, '\'', "''"),
        (Language::GoMod, '(', "()"),
        (Language::Markdown, '[', "[]"),
        (Language::Python, '\'', "''"),
        (Language::TypeScript, '`', "``"),
    ] {
        let mut doc = doc_in(language, "");
        let mut cursor = cursors_at(&[(0, 0)]);
        let mut handler = KeyboardHandler::new();

        press(
            &mut handler,
            &ch(typed),
            &mut doc,
            &mut cursor,
            &EditorConfig::default(),
        );

        assert_eq!(
            doc.text(),
            expected,
            "{} declares {typed:?} and must still pair it",
            language.id()
        );
    }
}
