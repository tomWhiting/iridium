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
//!
//! The last three sections carry the same rule for the **multi-character** rows
//! of slice S-3, which have their own fixtures and their own eliminations; see
//! their headings.
//!
//! ⭐⭐ **The last two sections are about the collapse, and since ruling B-13
//! (§10.12) the collapse asks `not_in` nothing at all.** They live here anyway,
//! and must: the rule they exercise fires only where the *insertion* wrote a
//! closer, and whether the insertion wrote one is decided by `not_in` — so a
//! live resolver is what puts each fixture on the side of the question it is
//! meant to be on. A harness with no tree would silently move every one of them
//! to the other side.
//!
//! ⚠️ **Every fixture below carries a measured label** — discriminator, control
//! or elimination — against the three reversions tabulated in
//! `multi_char_backspace_tests`' module docs. The labels are measurements, not
//! claims: three rounds of this slice failed because a verification set could
//! not tell the fix from its absence.

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

// ========== S-3: `not_in` on the multi-character rows ==========

/// A Rust source with a `/` inside a string literal, one inside a line comment
/// and one in code — each with the caret position that typing `*` would turn
/// into the `/*` opener.
///
/// Every one is followed by a space, which `autoclose_before` permits, and
/// preceded by a `/`, which is not a word character. Rust declares
/// `/*`→`" */"` with `not_in = ["string", "comment"]`.
const RUST_SLASHES: &str = concat!(
    "fn main() {\n",
    "    let s = \"a, / b\";\n",
    "    // a / b\n",
    "    let n = 1 / 2;\n",
    "}\n",
);

/// Inside the string literal: after the `/`, before the space.
const SLASH_IN_STRING: Position = Position::new(1, 17);
/// Inside the line comment: after the second `/`, before the space.
const SLASH_IN_COMMENT: Position = Position::new(2, 10);
/// In code: after the division operator, before the space.
const SLASH_IN_CODE: Position = Position::new(3, 15);

/// An editor over [`RUST_SLASHES`] as Rust, with one caret at `caret`.
fn rust_slash_editor(caret: Position) -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content(RUST_SLASHES);
    editor.set_language(Language::Rust);
    editor.set_cursor(caret);
    editor
}

/// ⚠️ The premises for the three below, in one test.
///
/// The apostrophe rule is absent from this list because it cannot apply at all:
/// it tests whether the typed character is one of `"`, `'` or `` ` ``, and `*`
/// is none of them.
#[test]
fn every_slash_caret_resolves_and_is_one_autoclose_before_permits() {
    let editor = rust_slash_editor(SLASH_IN_CODE);
    assert!(
        editor.state().syntax.tree().is_some(),
        "no tree parsed — every suppression assertion below would pass vacuously"
    );
    assert_eq!(
        scope_at(&editor, SLASH_IN_STRING),
        Some(HighlightType::String),
        "line 1 column 17 must be inside the string literal"
    );
    assert_eq!(
        scope_at(&editor, SLASH_IN_COMMENT),
        Some(HighlightType::Comment),
        "line 2 column 10 must be inside the line comment"
    );

    for (caret, expected) in [
        (SLASH_IN_STRING, "    let s = \"a, /() b\";"),
        (SLASH_IN_COMMENT, "    // a /() b"),
        (SLASH_IN_CODE, "    let n = 1 /() 2;"),
    ] {
        let mut editor = rust_slash_editor(caret);
        assert_eq!(
            typed_line(&mut editor, '(', caret),
            expected,
            "`(` must pair at {caret:?} — Rust names no `not_in` for it, so a \
             refusal here would be `autoclose_before`'s"
        );
    }
}

/// ⭐ The multi-character branch obeys `not_in` exactly as the single-character
/// one does: a `/*` typed inside a string literal is a `/` and a `*`.
#[test]
fn a_suppressed_block_comment_does_not_pair_inside_a_string() {
    let mut editor = rust_slash_editor(SLASH_IN_STRING);
    assert_eq!(
        scope_at(&editor, SLASH_IN_STRING),
        Some(HighlightType::String)
    );

    assert_eq!(
        typed_line(&mut editor, '*', SLASH_IN_STRING),
        "    let s = \"a, /* b\";",
        "no ` */` — Rust declares `/*` with not_in = [\"string\", \"comment\"]"
    );
}

/// And inside a comment, which is the other scope that row names. `/*` inside a
/// `//` comment is the case that would otherwise write a block-comment closer
/// into a line the compiler never reads.
#[test]
fn a_suppressed_block_comment_does_not_pair_inside_a_comment() {
    let mut editor = rust_slash_editor(SLASH_IN_COMMENT);
    assert_eq!(
        scope_at(&editor, SLASH_IN_COMMENT),
        Some(HighlightType::Comment)
    );

    assert_eq!(
        typed_line(&mut editor, '*', SLASH_IN_COMMENT),
        "    // a /* b",
        "no ` */`"
    );
}

/// ⭐ The control that makes the two above mean anything, and it carries two
/// eliminations at once: the identical keystroke in code both pairs — so `*`
/// *is* a trigger in Rust and the `/*` row *is* declared — and writes the
/// closer with the leading space the manifest spells.
#[test]
fn the_same_block_comment_still_pairs_in_code() {
    let mut editor = rust_slash_editor(SLASH_IN_CODE);
    assert_eq!(
        typed_line(&mut editor, '*', SLASH_IN_CODE),
        "    let n = 1 /* */ 2;",
        "in code the pair must be inserted, space and all"
    );
    assert_eq!(
        editor.state().cursor.primary.head,
        Position::new(3, 16),
        "and the caret sits between the halves, in front of the space"
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

// ========== S-3: the multi-character collapse under B-13 ==========

/// A C source carrying both halves of the collapse question: a block comment
/// with room to type `/*` inside it, where `not_in` refuses the row so the
/// editor writes **no** closer, and an ordinary code caret one line down where
/// the same keystroke pairs and the closer *is* the editor's.
///
/// Both carets are followed by a space, which `autoclose_before` permits, and
/// the apostrophe rule cannot apply to `*`. C declares `/*`→`" */"` with
/// `not_in = ["string", "comment"]`.
const C_BLOCK: &str = concat!(
    "int main() {\n",
    "    /* keep */\n",
    "    return 1 / 2;\n",
    "}\n",
);

/// Inside the block comment: after `keep`, before the space.
const C_IN_COMMENT: Position = Position::new(1, 11);
/// In code: after the division operator, before the space.
const C_IN_CODE: Position = Position::new(2, 14);

/// An editor over `text` as `language`, with one caret at `caret`, after typing
/// `keys` one character at a time through [`Editor::handle_key`].
///
/// ⭐⭐ **The keystrokes are the whole point, and under ruling B-13 (§10.12)
/// they are the only way to reach the multi-character collapse at all.** The
/// rule fires on a record the *insertion* left, so a fixture that writes a pair
/// into a document and puts a caret in the middle of it exercises the
/// fall-through and nothing else.
fn after_typing(language: Language, text: &str, caret: Position, keys: &[char]) -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content(text);
    editor.set_language(language);
    editor.set_cursor(caret);
    for &key in keys {
        editor.handle_key(&KeyEvent::simple(KeyCode::Char(key)));
    }
    editor
}

/// Presses Backspace once and returns the line `caret` was on.
fn backspaced_line(editor: &mut Editor, caret: Position) -> String {
    editor.handle_key(&KeyEvent::simple(KeyCode::Backspace));
    line_of(&editor.state().document.text(), caret)
}

/// ⚠️ **Premise, and the elimination the two below rest on** (green under all
/// three reversions, because it asserts the *insertion*). And the
/// state the collapse question needs is the interesting half.
///
/// The tree parses; the comment caret resolves to a comment and the code caret
/// to neither a comment nor a string; and **typing `*` inside the comment
/// writes no closer**, which is what makes the ` */` in front of that caret the
/// enclosing comment's rather than one the editor put there. That refusal is
/// B-7 on the *insertion* path, which B-13 leaves exactly as it was.
#[test]
fn the_block_fixture_resolves_and_the_suppressed_keystroke_writes_no_closer() {
    let editor = after_typing(Language::C, C_BLOCK, C_IN_COMMENT, &[]);
    assert!(
        editor.state().syntax.tree().is_some(),
        "no tree parsed — every assertion below would pass vacuously"
    );
    assert_eq!(
        scope_at(&editor, C_IN_COMMENT),
        Some(HighlightType::Comment),
        "line 1 column 11 must be inside the block comment"
    );
    let code = scope_at(&editor, C_IN_CODE);
    assert!(
        code.is_none_or(|h| !h.is_within("comment") && !h.is_within("string")),
        "line 2 column 14 must be neither comment nor string; got {code:?}"
    );

    let editor = after_typing(Language::C, C_BLOCK, C_IN_COMMENT, &['/', '*']);
    assert_eq!(
        line_of(&editor.state().document.text(), C_IN_COMMENT),
        "    /* keep/* */",
        "`/` is no trigger and is written plainly, and `not_in` then refuses the \
         `/*` row inside a comment — so no ` */` is written and the one in front \
         of the caret closes the enclosing comment"
    );
    assert_eq!(
        editor.state().cursor.primary.head,
        Position::new(1, 13),
        "the caret sits directly after the `*` the user typed"
    );
}

/// ⚠️ **Control** (green under all three: the approach B-13 replaces also got
/// this one right, in C). A closer the editor was forbidden to write is not the
/// collapse's to take: the ` */` in front of the caret closes the enclosing
/// comment, and taking it comments out the rest of the file.
///
/// ⭐ **Under B-13 the reason is that the insertion recorded nothing**, not that
/// a probe resolved to a comment. The keystroke that mattered has already
/// happened by the time Backspace runs, and its refusal is the evidence.
#[test]
fn a_collapse_the_insertion_never_wrote_does_not_take_the_enclosing_closer() {
    let mut editor = after_typing(Language::C, C_BLOCK, C_IN_COMMENT, &['/', '*']);
    assert_eq!(
        backspaced_line(&mut editor, C_IN_COMMENT),
        "    /* keep/ */",
        "one character, not four — the `*` the user typed goes and the enclosing \
         comment's ` */` stays"
    );
    assert_eq!(
        editor.state().cursor.primary.head,
        Position::new(1, 12),
        "and the caret is back where the `*` was typed"
    );
}

/// ⭐ **Elimination, red under R-closed — the one that makes the test above
/// mean anything.** The identical
/// language, keystroke, configuration and row, one line apart: here the editor
/// *did* write the closer, and all four characters must go.
///
/// ⚠️ At the moment of the Backspace the caret is inside the block comment the
/// insertion itself created, so every rule that asked the buffer where the caret
/// was refused here too and left an orphaned ` */`. A record does not care what
/// the caret is inside; it names what was written.
#[test]
fn the_same_collapse_in_code_still_takes_four_characters() {
    let mut editor = after_typing(Language::C, C_BLOCK, C_IN_CODE, &['*']);
    assert_eq!(
        line_of(&editor.state().document.text(), C_IN_CODE),
        "    return 1 /* */ 2;",
        "the premise: in code the row is not suppressed and the closer is written"
    );
    assert_eq!(
        backspaced_line(&mut editor, C_IN_CODE),
        "    return 1 / 2;",
        "four characters — the `*` the user typed and the ` */` the editor wrote"
    );
    assert_eq!(
        editor.state().cursor.primary.head,
        C_IN_CODE,
        "and the buffer and the caret are exactly what the keystroke found"
    );
}

/// ⚠️ **Elimination, red under R-closed.** "One character was deleted" proves
/// nothing on its own: a
/// multi-character collapse the editor *did* write still takes its whole closer
/// with the resolver live.
///
/// Rust's `r#"` row carries `not_in = ["string", "comment"]` and the caret ends
/// up inside the raw string the pair itself makes — so every rule that read the
/// scope at the caret refused this too and left the `r#` opener mangled as
/// `r##`.
#[test]
fn an_unsuppressed_multi_character_collapse_still_takes_its_closer() {
    let caret = Position::new(1, 14);
    let mut editor = after_typing(
        Language::Rust,
        "fn main() {\n    let s = r#;\n}\n",
        caret,
        &['"'],
    );
    assert!(
        editor.state().syntax.tree().is_some(),
        "the premise: there is a tree, so the resolver is live"
    );
    assert_eq!(
        line_of(&editor.state().document.text(), caret),
        "    let s = r#\"\"#;",
        "the premise: the closer was written"
    );
    assert_eq!(
        scope_at(&editor, Position::new(1, 15)),
        Some(HighlightType::String),
        "the premise: the caret resolves, and to the string the pair itself makes"
    );

    assert_eq!(
        backspaced_line(&mut editor, caret),
        "    let s = r#;",
        "the quote and the `\"#` it wrote go, and the `r#` the user typed stays"
    );
    assert_eq!(editor.state().cursor.primary.head, caret);
}

/// ⚠️ **Elimination, red under R-closed: absent means nowhere, not
/// everywhere** — the collapse's
/// half of [`a_pair_the_language_did_not_suppress_still_closes_inside_a_string`].
///
/// Python declares `"""` with `not_in = ["string"]` and nothing else, so a
/// comment does not switch that row off: the insertion writes the closer inside
/// a `#` comment and the collapse takes all four characters. Without this a rule
/// that declined for any resolved scope, rather than for a scope the row names,
/// would pass every other test here.
#[test]
fn a_row_the_language_did_not_suppress_in_this_scope_still_collapses_there() {
    let caret = Position::new(1, 6);
    let mut editor = after_typing(Language::Python, "x = 1\n# x \"\"\n", caret, &['"']);
    assert_eq!(
        scope_at(&editor, caret),
        Some(HighlightType::Comment),
        "the premise: the caret is inside the comment, and the scope resolves"
    );
    assert_eq!(
        line_of(&editor.state().document.text(), caret),
        "# x \"\"\"\"\"\"",
        "the premise: `\"\"\"` names only \"string\", so the closer was written"
    );

    assert_eq!(
        backspaced_line(&mut editor, caret),
        "# x \"\"",
        "and the collapse follows the insertion, four characters"
    );
    assert_eq!(editor.state().cursor.primary.head, caret);
}

/// ⭐⭐ **Discriminator, and the headline of ruling B-13 (§10.12).**
///
/// §10.10 traced the whole defect in Rust, and §10.11 measured that `not_in`
/// could never fix it: Rust nests block comments, so the very text the defect
/// produces leaves the outer comment unterminated, no highlight capture covers
/// it, and the resolver answers `None` at every column. Under B-6 that is
/// *unknown* and fails open, so the collapse proceeded and took four characters
/// — pinned, at the time, as a defect this slice was adding.
///
/// **A record either exists or does not, and an unparseable buffer is irrelevant
/// to it.** `not_in` refused the insertion here exactly as it did in C, so
/// nothing was recorded and one character goes.
///
/// Measured against the code this replaces: `left: "    /* keep/"`.
#[test]
fn rust_nesting_no_longer_matters_because_no_scope_is_asked() {
    const RUST_BLOCK: &str = concat!(
        "fn main() {\n",
        "    /* keep */\n",
        "    let n = 1 / 2;\n",
        "}\n",
    );

    // The same line and column as the C fixture's comment caret, re-derived
    // from this text rather than borrowed from it.
    let caret = Position::new(1, 11);

    let mut editor = after_typing(Language::Rust, RUST_BLOCK, caret, &['/', '*']);
    assert_eq!(
        line_of(&editor.state().document.text(), caret),
        "    /* keep/* */",
        "the same two keystrokes leave the same text as in C"
    );
    assert_eq!(
        scope_at(&editor, Position::new(1, 10)),
        None,
        "and this is the state no positional probe could answer in: the nested \
         `/*` leaves the outer comment unterminated, so nothing covers any \
         column of this line"
    );

    assert_eq!(
        backspaced_line(&mut editor, caret),
        "    /* keep/ */",
        "one character — the collapse asks the record, and the record is empty \
         because `not_in` refused the insertion"
    );
}

/// ⚠️ **Control, multi-line.** A block comment that opened on an earlier line
/// still suppresses the insertion at column zero of the next, so nothing is
/// recorded and Backspace takes one character.
#[test]
fn an_opener_at_the_start_of_a_line_inside_a_wrapped_comment_takes_one() {
    const WRAPPED: &str = concat!(
        "int main() {\n",
        "    /* keep\n",
        "/ */\n",
        "    return 0;\n",
        "}\n",
    );
    let caret = Position::new(2, 1);
    let mut editor = after_typing(Language::C, WRAPPED, caret, &['*']);
    assert_eq!(
        line_of(&editor.state().document.text(), caret),
        "/* */",
        "the premise: `not_in` refused the row, so no closer was written and the \
         ` */` in front of the caret is the wrapped comment's"
    );
    assert_eq!(
        backspaced_line(&mut editor, caret),
        "/ */",
        "one character, because nothing was recorded"
    );
}

/// ⚠️ **Elimination, red under R-closed**, and what keeps the test above from proving "column zero
/// always declines": the same opener at column zero, with nothing enclosing it,
/// still collapses.
#[test]
fn an_opener_at_the_start_of_a_line_in_code_still_takes_four_characters() {
    const AT_MARGIN: &str = concat!("int main() {\n", "    return 0;\n", "}\n", "/ x\n");
    let caret = Position::new(3, 1);
    let mut editor = after_typing(Language::C, AT_MARGIN, caret, &['*']);
    assert_eq!(
        line_of(&editor.state().document.text(), caret),
        "/* */ x",
        "the premise: in code the row is not suppressed and the closer is written"
    );
    assert_eq!(
        backspaced_line(&mut editor, caret),
        "/ x",
        "four characters — the closer here is the editor's own"
    );
}

// ========== B-13: the five regressions §10.12 measured ==========

/// Types `typed` at `caret` in `text` as `language`, presses Backspace once,
/// and returns the caret's line after each keystroke.
///
/// ⭐ Each of the five below is a case where the **insertion succeeded** — the
/// caret is in ordinary code — and where the approach B-13 replaces then refused
/// the collapse, because the character in front of the opener sat at the end of
/// a scope: a closing quote, a `*/`, a passed `//`. Every one is a regression
/// against the editor as it stood before that fix, reproduced from §10.12's
/// table, and every one is a **discriminator**: the measured wrong answer is
/// quoted in each test.
fn typed_then_backspaced(
    language: Language,
    text: &str,
    caret: Position,
    typed: char,
) -> (String, String) {
    let mut editor = after_typing(language, text, caret, &[typed]);
    let inserted = line_of(&editor.state().document.text(), caret);
    let collapsed = backspaced_line(&mut editor, caret);
    (inserted, collapsed)
}

/// ⭐ **Discriminator (R-open), and elimination (R-closed).** A string literal
/// ends immediately in front of the opener.
/// Measured against the code this replaces: `left: "    f(\"a\"/ */ x);"`.
#[test]
fn b13_regression_1_c_string_before_the_opener() {
    let text = "int main() {\n    f(\"a\"/ x);\n}\n";
    let (inserted, collapsed) = typed_then_backspaced(Language::C, text, Position::new(1, 10), '*');
    assert_eq!(
        inserted, "    f(\"a\"/* */ x);",
        "the closer must be written"
    );
    assert_eq!(collapsed, "    f(\"a\"/ x);");
}

/// ⭐ **Discriminator (R-open), and elimination (R-closed)**, the same shape in
/// another language.
/// Measured against the code this replaces: `left: "    foo(\"bar\"/ */ note);"`.
#[test]
fn b13_regression_2_javascript_string_before_the_opener() {
    let text = "function f() {\n    foo(\"bar\"/ note);\n}\n";
    let (inserted, collapsed) =
        typed_then_backspaced(Language::JavaScript, text, Position::new(1, 14), '*');
    assert_eq!(
        inserted, "    foo(\"bar\"/* */ note);",
        "the closer must be written"
    );
    assert_eq!(collapsed, "    foo(\"bar\"/ note);");
}

/// ⭐ **Discriminator (R-open), and elimination (R-closed).** A character
/// literal, which is a string capture too.
/// Measured against the code this replaces: `left: "    if (c == 'x'/ */ why ) {}"`.
#[test]
fn b13_regression_3_c_char_literal_before_the_opener() {
    let text = "int main() {\n    if (c == 'x'/ why ) {}\n}\n";
    let (inserted, collapsed) = typed_then_backspaced(Language::C, text, Position::new(1, 17), '*');
    assert_eq!(
        inserted, "    if (c == 'x'/* */ why ) {}",
        "the closer must be written"
    );
    assert_eq!(collapsed, "    if (c == 'x'/ why ) {}");
}

/// ⭐ **Discriminator (R-open), and elimination (R-closed).** A block comment
/// that ends immediately in front of the opener — the boundary the probe was
/// placed on.
/// Measured against the code this replaces: `left: "    /* a *// */ x;"`.
#[test]
fn b13_regression_4_c_block_comment_before_the_opener() {
    let text = "int main() {\n    /* a *// x;\n}\n";
    let (inserted, collapsed) = typed_then_backspaced(Language::C, text, Position::new(1, 12), '*');
    assert_eq!(
        inserted, "    /* a *//* */ x;",
        "the closer must be written"
    );
    assert_eq!(collapsed, "    /* a *// x;");
}

/// ⭐ **Discriminator (R-open), and elimination (R-closed)** — and the one that
/// mangles rather than orphans: the fall-through leaves `r##`.
/// Measured against the code this replaces: `left: "    let s = /* c */r##;"`.
#[test]
fn b13_regression_5_rust_block_comment_before_the_opener() {
    let text = "fn main() {\n    let s = /* c */r#;\n}\n";
    let (inserted, collapsed) =
        typed_then_backspaced(Language::Rust, text, Position::new(1, 21), '"');
    assert_eq!(
        inserted, "    let s = /* c */r#\"\"#;",
        "the closer must be written"
    );
    assert_eq!(collapsed, "    let s = /* c */r#;");
}
