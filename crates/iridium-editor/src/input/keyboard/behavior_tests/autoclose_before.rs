//! S-5: the characters a language will close a pair in front of.
//!
//! Split out of a 1,690-line `behavior_tests.rs` for #92, on the rule the
//! file already carried. The harness and the shared fixtures are in
//! [`super`].

use super::*;

// ========== S-5: `autoclose_before` ==========

/// Types `typed` into `text` at `column` on line 0 in `language`, and returns
/// the document it landed in.
///
/// Returns the `Document` rather than its text because a `String` cloned out of
/// a local that is about to be dropped is a redundant clone, and clippy is
/// right about that.
fn typed_into(language: Language, text: &str, column: usize, typed: char) -> Document {
    let mut doc = doc_in(language, text);
    let mut cursor = cursors_at(&[(0, column)]);
    let mut handler = KeyboardHandler::new();
    press(
        &mut handler,
        &ch(typed),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );
    doc
}

/// ⭐ The subtraction S-5 exists for. Before this, typing `(` in front of an
/// identifier left a `)` stranded in the middle of it.
#[test]
fn a_pair_does_not_close_in_front_of_a_character_the_language_did_not_list() {
    assert_eq!(
        typed_into(Language::Rust, "identifier", 0, '(')
            .text()
            .as_str(),
        "(identifier",
        "rust lists `;:.,=}}])>` and not a letter, so nothing should close here"
    );
    assert_eq!(
        typed_into(Language::Rust, "identifier", 0, '"')
            .text()
            .as_str(),
        "\"identifier",
        "the rule is about the character after the caret, not about brackets"
    );
}

/// The other side of it: a listed character permits the close, so the feature
/// is not satisfied by an implementation that simply stopped pairing.
#[test]
fn a_pair_closes_in_front_of_a_character_the_language_does_list() {
    for (text, expected) in [(";", "();"), ("}", "()}"), (")", "())")] {
        assert_eq!(
            typed_into(Language::Rust, text, 0, '(').text().as_str(),
            expected,
            "rust lists {text:?} in autoclose_before"
        );
    }
}

/// ⚠️ The assumption, pinned so that changing it is a decision. Neither end of
/// line nor whitespace appears in any vendored set, and read literally that
/// would mean never closing a bracket at the end of a line.
#[test]
fn end_of_line_and_whitespace_permit_the_close() {
    assert_eq!(
        typed_into(Language::Rust, "", 0, '(').text().as_str(),
        "()",
        "end of line"
    );
    assert_eq!(
        typed_into(Language::Rust, "fn f", 4, '(').text().as_str(),
        "fn f()",
        "end of a non-empty line"
    );
    assert_eq!(
        typed_into(Language::Rust, " x", 0, '(').text().as_str(),
        "() x",
        "a space displaces nothing"
    );
    assert_eq!(
        typed_into(Language::Rust, "\tx", 0, '(').text().as_str(),
        "()\tx",
        "and neither does a tab"
    );
    // Column 0 of a line with a line below it: `char_at` stops at the line's
    // own end, so the next line's first character is not what is examined.
    assert_eq!(
        typed_into(Language::Rust, "\nx", 0, '(').text().as_str(),
        "()\nx",
        "a line ending is the end of the line, not the character after it"
    );
}

/// ⭐ The sets differ per language, and this is the test that proves they are
/// read from the manifest rather than fixed. A semicolon is in every C-like
/// set and in none of the JSON-family ones; a comma is the reverse.
#[test]
fn each_language_uses_its_own_autoclose_before_set() {
    for (language, text, typed, expected) in [
        (Language::Rust, ";", '(', "();"),
        (Language::Json, ";", '{', "{;"),
        (Language::Json, ",", '{', "{},"),
        (Language::Rust, ",", '(', "(),"),
        (Language::Yaml, "]", '[', "[]]"),
        (Language::Yaml, ";", '[', "[;"),
        // `go.mod` lists exactly `)`, and declares only the one pair.
        (Language::GoMod, ")", '(', "())"),
        (Language::GoMod, "]", '(', "(]"),
    ] {
        assert_eq!(
            typed_into(language, text, 0, typed).text().as_str(),
            expected,
            "{} typing {typed:?} in front of {text:?}",
            language.id()
        );
    }
}

/// ⭐ An absent `autoclose_before` is not an empty one. `gitcommit` declares
/// six brackets and no `autoclose_before` at all — it has not said, so it
/// closes in front of anything, exactly as every language did before S-5.
#[test]
fn a_language_that_lists_nothing_closes_in_front_of_everything() {
    assert_eq!(
        typed_into(Language::GitCommit, "identifier", 0, '(')
            .text()
            .as_str(),
        "()identifier",
        "gitcommit declares brackets and no autoclose_before"
    );
    // The same silence, reached three other ways: a language with no brackets
    // key, and a document with no language at all.
    let mut doc = Document::new("identifier");
    let mut cursor = cursors_at(&[(0, 0)]);
    let mut handler = KeyboardHandler::new();
    press(
        &mut handler,
        &ch('('),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );
    assert_eq!(doc.text(), "()identifier", "a document with no language");
}

/// Wrapping a selection is not gated: the user asked for it by selecting, and
/// the character after the selection is an accident of where it ended.
#[test]
fn wrapping_a_selection_ignores_autoclose_before() {
    let mut doc = doc_in(Language::Rust, "foobar");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 3))]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('('),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(
        doc.text(),
        "(foo)bar",
        "`b` is not in rust's set, and a wrap must not become a typed-over selection"
    );
}

/// Skipping over an existing closer is not gated either. Not discriminating on
/// its own — every closer this editor knows is in the C-like set — but it
/// pins the shape: the skip-over branch asks what is *at* the caret, never
/// what follows it.
#[test]
fn skipping_over_a_closer_ignores_autoclose_before() {
    assert_eq!(
        typed_into(Language::Rust, ")identifier", 0, ')')
            .text()
            .as_str(),
        ")identifier",
        "the closer is stepped over, not re-typed"
    );
}

#[test]
fn auto_pairs_disabled_types_plain_characters() {
    let mut doc = Document::new("");
    let mut cursor = cursors_at(&[(0, 0)]);
    let mut handler = KeyboardHandler::new();
    let config = EditorConfig {
        auto_pairs: false,
        ..EditorConfig::default()
    };

    press(&mut handler, &ch('('), &mut doc, &mut cursor, &config);

    assert_eq!(doc.text(), "(");
    assert_eq!(heads(&cursor), vec![(0, 1)]);
}

#[test]
fn auto_pairs_disabled_backspace_deletes_single_half() {
    let mut doc = Document::new("()");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();
    let config = EditorConfig {
        auto_pairs: false,
        ..EditorConfig::default()
    };

    press(&mut handler, &BACKSPACE, &mut doc, &mut cursor, &config);

    assert_eq!(doc.text(), ")");
    assert_eq!(heads(&cursor), vec![(0, 0)]);
}

#[test]
fn auto_pair_multi_cursor_pairs_on_same_line() {
    let mut doc = Document::new("ab");
    let mut cursor = cursors_at(&[(0, 1), (0, 2)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('('),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "a()b()");
    assert_eq!(heads(&cursor), vec![(0, 2), (0, 5)]);
}

#[test]
fn auto_pair_multi_cursor_mixed_skip_and_plain_insert() {
    let mut doc = Document::new("()\nx");
    let mut cursor = cursors_at(&[(0, 1), (1, 1)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch(')'),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    // The first cursor skips the existing closer; the second inserts one.
    assert_eq!(doc.text(), "()\nx)");
    assert_eq!(heads(&cursor), vec![(0, 2), (1, 2)]);
}

#[test]
fn auto_pair_multi_cursor_wrap_and_pair_insert() {
    let mut doc = Document::new("abc\nd");
    let mut cursor = cursors_with(&[
        Selection::new(Position::new(0, 0), Position::new(0, 3)),
        Selection::collapsed(Position::new(1, 1)),
    ]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('{'),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    // The selection is wrapped; the collapsed cursor gets a fresh pair.
    assert_eq!(doc.text(), "{abc}\nd{}");
    assert_eq!(
        selections(&cursor),
        vec![((0, 1), (0, 4)), ((1, 2), (1, 2))]
    );
}

#[test]
fn undo_auto_pair_insert_restores_text_and_cursors() {
    let mut doc = Document::new("ab");
    let original = cursors_at(&[(0, 1), (0, 2)]);
    let mut cursor = original.clone();
    let mut handler = KeyboardHandler::new();

    let cmd = press(
        &mut handler,
        &ch('('),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    )
    .expect("pair insertion must produce a command");

    cmd.inverse()
        .apply(&mut doc, &mut cursor)
        .expect("inverse must apply");
    assert_eq!(doc.text(), "ab");
    assert_eq!(cursor, original);
}
