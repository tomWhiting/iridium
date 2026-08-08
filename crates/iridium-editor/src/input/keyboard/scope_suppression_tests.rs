//! The manifests' `not_in`, end to end: a pair a language switches off inside
//! a string or a comment must not auto-close there.
//!
//! These drive [`Editor::handle_key`] rather than
//! [`KeyboardHandler`](super::KeyboardHandler) directly, because the whole
//! point of the slice is the seam between them — the typing path cannot see
//! the parse tree, and the editor is where the tree lives. Everything below
//! therefore needs a real grammar, hence the `syntax` gate.
//!
//! # ⚠️ Three other rules can refuse a pair, and all three are ruled out first
//!
//! `auto_pair_edit_for` refuses to insert a pair for four separate reasons,
//! and only one of them is under test here. A caret placed carelessly is
//! refused by one of the other three, and the test then passes green against a
//! `not_in` that is never read at all. That is not hypothetical: the first
//! draft of this file did exactly that at two of its three positions.
//!
//! - **The scope did not resolve.** Under ruling B-6 an unresolved scope and a
//!   permitted one are the same answer — neither suppresses — so a resolver
//!   returning `None` for every byte looks identical to a correct one from the
//!   outside. Ruled out by
//!   [`the_fixture_resolves_to_the_three_scopes_the_tests_assume`], which names
//!   what each caret must resolve to.
//! - **`autoclose_before` (S-5)** refuses a pair in front of a character the
//!   language did not list, and Go lists only `;:.,=}])>`. Ruled out by
//!   [`every_caret_is_one_autoclose_before_permits`], which types a character Go
//!   never suppresses and requires it to pair.
//! - **The apostrophe rule** keeps a quote single directly after a word
//!   character, so `don't` stays `don't`. Every caret below is preceded by a
//!   comma or an opening bracket, and
//!   [`every_caret_is_one_the_apostrophe_rule_permits`] proves it by typing the
//!   same quote with no language set.
//!
//! What is left, when a quote does not pair here, is `not_in` and nothing else.

#![allow(clippy::expect_used, clippy::panic)]

use crate::document::{Position, Selection};
use crate::editor::Editor;
use crate::input::{KeyCode, KeyEvent};
use iridium_lang::Language;
use iridium_syntax::HighlightType;

/// A Go source with one caret position of each kind.
///
/// Each is preceded by a non-word character and followed by one
/// `autoclose_before` permits, so neither of those rules can be what refuses a
/// pair. See the module docs.
const GO: &str = "package main\n\
                  \n\
                  func main() {\n\
                  \ts := \"a, b c\"\n\
                  \t// a, b, c\n\
                  \tfmt.Println()\n\
                  }\n";

/// Inside the string literal: after the comma, before the space.
const IN_STRING: Position = Position::new(3, 9);
/// Inside the line comment: after the first comma, before the space.
const IN_COMMENT: Position = Position::new(4, 6);
/// In code, between the call's parentheses.
const IN_CODE: Position = Position::new(5, 13);

/// An editor over [`GO`] as Go, with one caret at `caret`.
fn go_editor(caret: Position) -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content(GO);
    editor.set_language(Language::Go);
    editor.set_cursor(caret);
    editor
}

/// The same text and caret with **no language**: no manifest, no tree, no rule.
fn languageless_editor(caret: Position) -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content(GO);
    editor.set_cursor(caret);
    editor
}

/// What the caret at `position` resolves to, through exactly the accessor the
/// typing path uses.
fn scope_at(editor: &Editor, position: Position) -> Option<HighlightType> {
    let state = editor.state();
    let byte = state
        .document
        .position_to_offset(position)
        .expect("the fixture's positions are inside the document");
    state.syntax.caret_scopes(&state.document).scope_at(byte)
}

/// Types `c` and returns the line the caret was on.
fn typed_line(editor: &mut Editor, c: char, caret: Position) -> String {
    editor.handle_key(&KeyEvent::simple(KeyCode::Char(c)));
    line_of(&editor.state().document.text(), caret)
}

/// The line at `caret`'s line number.
fn line_of(text: &str, caret: Position) -> String {
    text.lines()
        .nth(caret.line)
        .expect("the line survives an insertion")
        .to_owned()
}

// ========== The three premises ==========

/// ⭐ The tree parses, the resolver answers, and it answers the three
/// *different* things the tests below rely on.
#[test]
fn the_fixture_resolves_to_the_three_scopes_the_tests_assume() {
    let editor = go_editor(IN_CODE);

    assert!(
        editor.state().syntax.tree().is_some(),
        "no tree parsed — `caret_scopes` would answer nothing and every \
         suppression test below would pass vacuously"
    );

    assert_eq!(
        scope_at(&editor, IN_STRING),
        Some(HighlightType::String),
        "line 3 column 9 must be inside the string literal"
    );
    assert_eq!(
        scope_at(&editor, IN_COMMENT),
        Some(HighlightType::Comment),
        "line 4 column 6 must be inside the line comment"
    );

    let code = scope_at(&editor, IN_CODE);
    assert!(
        code.is_some_and(|h| !h.is_within("string") && !h.is_within("comment")),
        "line 5 column 13 must resolve, and to something that is neither a \
         string nor a comment; got {code:?}"
    );
}

/// ⚠️ Premise two: `autoclose_before` permits all three positions.
///
/// Proven with a character Go never suppresses. `(` carries no `not_in` at all,
/// so it must pair at all three — which it can only do if each position is one
/// `autoclose_before` permits.
#[test]
fn every_caret_is_one_autoclose_before_permits() {
    for (caret, expected) in [
        (IN_STRING, "\ts := \"a,() b c\""),
        (IN_COMMENT, "\t// a,() b, c"),
        (IN_CODE, "\tfmt.Println(())"),
    ] {
        let mut editor = go_editor(caret);
        assert_eq!(
            typed_line(&mut editor, '(', caret),
            expected,
            "`(` must pair at {caret:?} — Go names no `not_in` for it, so a \
             refusal here would be `autoclose_before`'s and the suppression \
             tests would prove nothing"
        );
    }
}

/// ⚠️ Premise three: the apostrophe rule permits all three positions.
///
/// The same quote the suppression tests use, at the same three carets, with no
/// language set. Nothing then reads a manifest — no `not_in`, no
/// `autoclose_before` — so the only rule left that could refuse is the
/// word-character one, and a pair at every position proves it does not.
#[test]
fn every_caret_is_one_the_apostrophe_rule_permits() {
    for (caret, expected) in [
        (IN_STRING, "\ts := \"a,'' b c\""),
        (IN_COMMENT, "\t// a,'' b, c"),
        (IN_CODE, "\tfmt.Println('')"),
    ] {
        let mut editor = languageless_editor(caret);
        assert!(
            !editor
                .state()
                .syntax
                .caret_scopes(&editor.state().document)
                .can_resolve(),
            "with no language there must be nothing to resolve against"
        );
        assert_eq!(
            typed_line(&mut editor, '\'', caret),
            expected,
            "with no language the quote must pair at {caret:?}"
        );
    }
}

// ========== The rule ==========

/// ⭐ The whole slice, in one assertion: a quote Go suppresses inside strings
/// stays a single quote there.
#[test]
fn a_suppressed_quote_does_not_pair_inside_a_string() {
    let mut editor = go_editor(IN_STRING);
    assert_eq!(scope_at(&editor, IN_STRING), Some(HighlightType::String));

    assert_eq!(
        typed_line(&mut editor, '\'', IN_STRING),
        "\ts := \"a,' b c\"",
        "one quote, not two — Go declares `'` with not_in = [\"comment\", \"string\"]"
    );
}

/// And the same inside a comment, which is the other half of the vocabulary.
#[test]
fn a_suppressed_quote_does_not_pair_inside_a_comment() {
    let mut editor = go_editor(IN_COMMENT);
    assert_eq!(scope_at(&editor, IN_COMMENT), Some(HighlightType::Comment));

    assert_eq!(
        typed_line(&mut editor, '\'', IN_COMMENT),
        "\t// a,' b, c",
        "one quote, not two"
    );
}

/// ⭐ The control that makes the two above mean something. The identical
/// keystroke in code pairs, so "it did not pair" is a fact about *where*, not
/// about the character or the language.
#[test]
fn the_same_quote_still_pairs_in_code() {
    let mut editor = go_editor(IN_CODE);
    assert_eq!(
        typed_line(&mut editor, '\'', IN_CODE),
        "\tfmt.Println('')",
        "in code the pair must still be inserted"
    );
}

/// ⚠️ **Absent means nowhere, not everywhere.** Go's `(` row carries no
/// `not_in`, so it pairs inside a string exactly as it does in code: the
/// suppression is per row, and a language that has not said about a pair has
/// said nothing about it.
///
/// (Shares its assertion with [`every_caret_is_one_autoclose_before_permits`]
/// and is kept separate deliberately — that one is scaffolding for the others,
/// this one is a behaviour someone could break on purpose.)
#[test]
fn a_pair_the_language_did_not_suppress_still_closes_inside_a_string() {
    let mut editor = go_editor(IN_STRING);
    assert_eq!(
        typed_line(&mut editor, '(', IN_STRING),
        "\ts := \"a,() b c\""
    );
}

/// ⭐ The case a single resolved scope per keystroke would have got wrong, and
/// the reason [`CaretScopes`](crate::editor::CaretScopes) is consulted per
/// caret rather than once.
///
/// One caret inside the string, one in code, one keystroke. A single answer for
/// both would either suppress at the code caret — fail closed, which ruling B-6
/// rejects — or insert at the string caret the pair the language forbade.
#[test]
fn two_carets_in_different_scopes_get_different_answers() {
    let mut editor = go_editor(IN_STRING);
    editor
        .state_mut()
        .cursor
        .add_cursor(Selection::collapsed(IN_CODE));
    assert_eq!(
        editor.state().cursor.all_selections().count(),
        2,
        "the fixture needs both carets"
    );

    editor.handle_key(&KeyEvent::simple(KeyCode::Char('\'')));
    let text = editor.state().document.text();
    assert_eq!(
        line_of(&text, IN_STRING),
        "\ts := \"a,' b c\"",
        "the caret inside the string must not pair"
    );
    assert_eq!(
        line_of(&text, IN_CODE),
        "\tfmt.Println('')",
        "the caret in code must pair, in the same edit"
    );
}

/// The rule is not Go-shaped. Rust declares `not_in = ["string"]` on its `"`
/// row, and a double quote typed inside a Rust string must stay single.
///
/// ⚠️ Rust is worth pinning for a second reason. It declares `not_in` on five
/// other rows and **none of them reaches `PairRules`**: `r#"`, `r##"`, `r###"`
/// and `/*` are multi-character openers (auto-pair map S-3) and `<` carries
/// `close = false`, so it is not an auto-close rule that could be suppressed.
/// Reading the manifest and expecting six suppressed pairs is the mistake this
/// test's existence answers — there is exactly one.
#[test]
fn a_second_language_suppresses_its_own_quote_inside_its_own_strings() {
    let text = "fn main() {\n    let s = \"a, b c\";\n}\n";
    // After the comma, before the space: the apostrophe rule and
    // `autoclose_before` both permit here, exactly as in the Go fixture.
    let caret = Position::new(1, 15);

    // The controls, inline: with no language the same keystroke pairs, so
    // neither of the other two refusal rules is what acts below.
    let mut plain = Editor::with_defaults();
    plain.set_content(text);
    plain.set_cursor(caret);
    assert_eq!(
        typed_line(&mut plain, '"', caret),
        "    let s = \"a,\"\" b c\";",
        "with no language the quote must pair at this caret"
    );

    let mut editor = Editor::with_defaults();
    editor.set_content(text);
    editor.set_language(Language::Rust);
    editor.set_cursor(caret);

    assert_eq!(
        scope_at(&editor, caret),
        Some(HighlightType::String),
        "the premise: the caret is inside the Rust string"
    );

    assert_eq!(
        typed_line(&mut editor, '"', caret),
        "    let s = \"a,\" b c\";",
        "one quote, not two"
    );
}

// ========== B-7: the close, and nothing else ==========

/// A selection wrapped inside a string still wraps. The user asked for it by
/// selecting, and refusing would replace the selected text with one character.
#[test]
fn a_selection_inside_a_string_still_wraps() {
    let mut editor = go_editor(IN_STRING);
    editor.set_selection(Position::new(3, 10), Position::new(3, 13));
    assert_eq!(
        typed_line(&mut editor, '\'', IN_STRING),
        "\ts := \"a, 'b c'\"",
        "a wrap is an explicit request, inside a string as anywhere else"
    );
}

/// Typing a closer that is already there steps over it, inside a string as
/// anywhere else: the question is whether the character is present, not what
/// surrounds it.
#[test]
fn skip_over_still_works_inside_a_string() {
    let mut editor = Editor::with_defaults();
    editor.set_content("package main\n\nvar s = \"it' here\"\n");
    editor.set_language(Language::Go);
    let caret = Position::new(2, 11);
    editor.set_cursor(caret);

    assert_eq!(
        scope_at(&editor, caret),
        Some(HighlightType::String),
        "the premise: the caret is inside the string"
    );

    assert_eq!(
        typed_line(&mut editor, '\'', caret),
        "var s = \"it' here\"",
        "the existing quote is stepped over, not duplicated"
    );
    assert_eq!(
        editor.state().cursor.primary.head,
        Position::new(2, 12),
        "and the caret moved past it"
    );
}

/// Backspace between the halves of a pair still collapses both inside a
/// string. The pair exists — pasted, or typed before the text around it was a
/// string — and one keystroke must still remove it.
#[test]
fn backspace_still_collapses_a_pair_inside_a_string() {
    let mut editor = Editor::with_defaults();
    editor.set_content("package main\n\nvar s = \"a '' b\"\n");
    editor.set_language(Language::Go);
    let caret = Position::new(2, 12);
    editor.set_cursor(caret);

    assert_eq!(
        scope_at(&editor, caret),
        Some(HighlightType::String),
        "the premise: the caret is between the quotes, inside the string"
    );

    editor.handle_key(&KeyEvent::simple(KeyCode::Backspace));
    assert_eq!(
        line_of(&editor.state().document.text(), caret),
        "var s = \"a  b\"",
        "both halves go, as they do outside a string"
    );
}

// ========== Fail open, and undo ==========

/// A suppressed insertion is one ordinary undoable edit, not a keystroke that
/// short-circuits into doing nothing differently.
#[test]
fn a_suppressed_insertion_is_still_one_undoable_command() {
    let mut editor = go_editor(IN_STRING);
    let before = editor.state().document.text();

    assert_eq!(
        typed_line(&mut editor, '\'', IN_STRING),
        "\ts := \"a,' b c\"",
        "the quote was typed"
    );
    assert!(editor.undo(), "there is something to undo");
    assert_eq!(
        editor.state().document.text(),
        before,
        "one undo restores the text"
    );
}
