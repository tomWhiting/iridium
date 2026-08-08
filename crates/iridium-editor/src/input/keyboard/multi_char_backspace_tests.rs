//! Backspace immediately after a multi-character auto-pair — the auto-pair
//! map's slice S-3, step 5, under ruling B-8 (§10.8) as ruling B-13 (§10.12)
//! re-founds it.
//!
//! One keystroke undoes one keystroke: the closer the editor wrote goes, and
//! the single character the user typed to trigger it goes with it, so `r#"|"#`
//! leaves `r#` — exactly the buffer the quote was typed into.
//!
//! # ⭐⭐ Every fixture here types the pair, and that is ruling B-13
//!
//! The multi-character collapse is gated on [`AutoPairInsertion`], the record
//! the insertion leaves behind, and **not** on anything the buffer can be asked
//! afterwards. So a fixture that writes `r#""#` into a document and puts a caret
//! in the middle of it is not testing this rule at all — it is testing the
//! fall-through, which is a different and equally required thing. The two are
//! separated here by construction:
//!
//! - [`typed_then_backspaced`] presses the trigger and then Backspace through
//!   **one handler**, which is the only way to reach the rule;
//! - [`backspaced`] presses Backspace alone into a buffer nobody typed, which
//!   is the only way to reach the fall-through.
//!
//! ⚠️ **Three rounds of positional probes were each refuted because the
//! verification set could not tell the fix from its absence**, so every fixture
//! here and in `scope_suppression_tests` carries a label, and the labels were
//! **measured against three reversions rather than asserted**:
//!
//! | reversion | what it is | how many of these tests go red |
//! | --- | --- | --- |
//! | **R-open** | the code B-13 replaces — the collapse fires wherever the buffer shows a declared row, gated by `not_in` in front of the opener | **8** |
//! | **R-stale** | `AutoPairInsertion::describes` returns `true`: a record is kept but never validated | **3** |
//! | **R-closed** | `describes` returns `false`: the multi-character collapse never fires at all | **15** |
//! | **R-id** | the `self.document == document.id()` term is deleted from `describes` | **1** |
//! | **R-eager** | `self.auto_pair = None` added to `KeyboardHandler::reset_vertical_state`, beside the sticky columns | **1** |
//!
//! ⚠️ **R-id and R-eager were added after the B-13 adversary showed that
//! R-stale cannot substitute for either.** R-stale disables all three of
//! `describes`' keys at once, so it proves that *some* validation exists and
//! says nothing about which term does the work — and measured per-term, the
//! document-identity term had **no** test that could tell it from its absence.
//! R-eager exists because every other record test asserts a *refusal*, and a
//! refusal test cannot catch a guard that became too eager. Each of the two
//! fails exactly one test, which is the point: they are the only reversions
//! their test can see, and their test is the only one that can see them.
//!
//! - **Discriminator** — red under R-open or R-stale. These are the tests that
//!   can tell this fix from its absence, and each quotes the wrong answer it was
//!   measured producing.
//! - **Elimination** — red under R-closed. These stop "one character was
//!   deleted" from proving nothing, by pinning that the collapse still fires
//!   where it must.
//! - **Control** — green under all three. These pin what the fix may not break
//!   (B-8's left bound, the single-character rule, undoability) and are labelled
//!   so nobody reads one as evidence for the gate.
//!
//! # ⚠️ Rule L applies differently on this path
//!
//! Backspace reaches `backspace_edits_with_pairs` through one branch on
//! `config.auto_pairs` and nothing else. There is no trigger gate, no
//! `autoclose_before`, no apostrophe rule and — since B-13 — no `not_in` on this
//! path at all. Of the five reasons a pair may not appear, only **"the language
//! declares no such row"** and **"the editor never wrote it"** are live here, and
//! both are ruled out by their own fixtures:
//! [`the_same_pair_in_a_language_without_the_row_collapses_only_the_quotes`]
//! holds buffer, caret and keystroke fixed and varies only the manifest, and
//! [`auto_pairs_off_deletes_exactly_one_character`] rules out the remaining
//! alternative — that some wider delete rule, and not the auto-pair one,
//! produced the collapse.
//!
//! ⚠️ **The prefix stays, always.** `r#"|"#` leaves the `r#` the user typed and
//! takes only the quote that triggered the insertion and the `"#` it wrote, and
//! by the same rule `print(f"|")` leaves the `f`. That is ruling B-8, and
//! [`an_empty_prefixed_string_keeps_the_prefix_the_user_typed`] and
//! [`backspace_restores_the_buffer_the_keystroke_found`] pin it.
//!
//! ⭐ Tidiness loses on purpose: `r#` alone is not valid Rust, so a whole-pair
//! collapse would leave a cleaner buffer. But nothing can know how much of a
//! prefix the user wanted gone, and `print(f|)` is *exactly what the user had*.
//! One more Backspace is cheap; a re-typed character the editor ate is not.
//!
//! [`AutoPairInsertion`]: super::AutoPairInsertion

#![allow(clippy::expect_used, clippy::panic)]

use iridium_lang::Language;

use super::auto_pair_record::AutoPairInsertion;
use super::behavior_tests::{ch, doc_in, press};
use super::multi_char_pair_tests::BLOCK_COMMENT_LANGUAGES;
use super::tests::{cursors_at, heads};
use super::{
    Command, Document, EditorConfig, KeyCode, KeyEvent, KeyboardHandler, Modifiers, Position,
    Selection,
};
use crate::editor::Editor;

const BACKSPACE: KeyEvent = KeyEvent::new(KeyCode::Backspace, Modifiers::none());

/// Backspaces once at `column` on line 0 of `text` — tagged with `language`
/// when one is given — and returns the resulting text with every caret.
///
/// ⚠️ **No keystroke produced this buffer**, so no record describes it: this
/// reaches the fall-through, never the multi-character collapse. Use
/// [`typed_then_backspaced`] for that.
fn backspaced_with(
    language: Option<Language>,
    text: &str,
    column: usize,
    config: &EditorConfig,
) -> (String, Vec<(usize, usize)>) {
    let mut doc = language.map_or_else(|| Document::new(text), |lang| doc_in(lang, text));
    let mut cursor = cursors_at(&[(0, column)]);
    let mut handler = KeyboardHandler::new();
    press(&mut handler, &BACKSPACE, &mut doc, &mut cursor, config);
    (doc.text(), heads(&cursor))
}

/// [`backspaced_with`] in a language, with the default configuration.
fn backspaced(language: Language, text: &str, column: usize) -> (String, Vec<(usize, usize)>) {
    backspaced_with(Some(language), text, column, &EditorConfig::default())
}

/// Types `typed` at `column` on line 0 of `text`, then presses Backspace —
/// **through one handler**, which is what carries the insertion record from the
/// first keystroke to the second.
///
/// Returns the text after the insertion, the text after the Backspace, and the
/// carets after the Backspace. The first is asserted by every caller: without it
/// a fixture where the editor wrote no closer at all would report the right
/// final text for the wrong reason.
fn typed_then_backspaced(
    language: Language,
    text: &str,
    column: usize,
    typed: char,
) -> (String, String, Vec<(usize, usize)>) {
    let mut doc = doc_in(language, text);
    let mut cursor = cursors_at(&[(0, column)]);
    let mut handler = KeyboardHandler::new();
    let config = EditorConfig::default();
    press(&mut handler, &ch(typed), &mut doc, &mut cursor, &config);
    let inserted = doc.text();
    press(&mut handler, &BACKSPACE, &mut doc, &mut cursor, &config);
    (inserted, doc.text(), heads(&cursor))
}

// ========== The premises ==========

/// ⭐ **Elimination** (of a manifest explanation, so green under all three
/// reversions). The one manifest refusal that is live on this path:
/// buffer, caret and keystroke are held fixed, and only the manifest varies.
///
/// Every language below declares `"`→`"` exactly as the one it stands in for
/// does, so the single-character collapse fires identically.
#[test]
fn the_same_pair_in_a_language_without_the_row_collapses_only_the_quotes() {
    for (language, text, column, expected, caret) in [
        (Language::C, "let s = r#\"\"#;", 11, "let s = r##;", 10),
        (Language::Python, "let s = r#\"\"#;", 11, "let s = r##;", 10),
        (
            Language::Rust,
            "print(\"\"\"\"\"\")",
            9,
            "print(\"\"\"\")",
            8,
        ),
        (
            Language::Json,
            "print(\"\"\"\"\"\")",
            9,
            "print(\"\"\"\")",
            8,
        ),
    ] {
        let (after, heads) = backspaced(language, text, column);
        assert_eq!(
            after, expected,
            "{language:?} declares no multi-character row this caret sits \
             inside, so only the single-character rule may fire"
        );
        assert_eq!(heads, vec![(0, caret)]);
    }
}

/// ⚠️ **Elimination** (green under all three). With auto-pairs off the same
/// caret deletes exactly one
/// character, so every collapse below is the auto-pair rule's and not some
/// wider delete.
#[test]
fn auto_pairs_off_deletes_exactly_one_character() {
    let config = EditorConfig {
        auto_pairs: false,
        ..EditorConfig::default()
    };
    let (after, caret) = backspaced_with(Some(Language::Rust), "let s = r#\"\"#;", 11, &config);
    assert_eq!(
        after, "let s = r#\"#;",
        "one character, and it is the quote"
    );
    assert_eq!(caret, vec![(0, 10)]);
}

// ========== B-13: the record is what fires the collapse ==========

/// ⭐⭐ **Discriminator, and B-13's headline.** Rust nests block comments, so
/// `/* keep/* */` leaves the outer comment unterminated: no capture covers any
/// column of that line and every positional probe answers *unknown*. §10.11
/// pinned that as unfixable and it was — by any question put to the buffer.
///
/// Nobody typed this `/*` through this editor, so no record names this caret and
/// the multi-character rule has nothing to say. The four-character answer would
/// take the enclosing comment's ` */` and comment out the rest of the file.
///
/// Measured against the code this replaces: `left: "/* keep/"`.
#[test]
fn b13_rust_nested_block_comment_takes_one_character() {
    let (after, caret) = backspaced(Language::Rust, "/* keep/* */", 9);
    assert_eq!(after, "/* keep/ */");
    assert_eq!(caret, vec![(0, 8)]);
}

/// ⭐ **Discriminator.** The same shape in C, which does not nest — so the
/// approach this replaces could resolve the scope here and did fix this one.
/// It must stay fixed for a reason that no longer mentions scopes.
#[test]
fn b13_c_nested_block_comment_takes_one_character() {
    let (after, caret) = backspaced(Language::C, "/* keep/* */", 9);
    assert_eq!(after, "/* keep/ */");
    assert_eq!(caret, vec![(0, 8)]);
}

/// ⭐ **Discriminator.** A raw-string pair nobody typed — opened from a file, or
/// pasted — is not the multi-character rule's to collapse, and falls through to
/// the single-character answer.
///
/// ⚠️ The fall-through mangles the opener into `r##`, and that is the accepted
/// cost of the asymmetry: the single-character collapse keeps its buffer-based
/// rule because over-deleting one character is a nuisance, while the
/// multi-character one cannot, because over-deleting four is data loss. Measured
/// against the code this replaces: `left: "let s = r#;"`.
#[test]
fn b13_a_pair_the_editor_did_not_write_falls_through() {
    let (after, caret) = backspaced(Language::Rust, "let s = r#\"\"#;", 11);
    assert_eq!(after, "let s = r##;");
    assert_eq!(caret, vec![(0, 10)]);
}

/// ⭐ **Discriminator for the revision half of the record.** One more keystroke
/// between the insertion and the Backspace, and the record no longer describes
/// the buffer: the collapse falls through and one character goes.
///
/// The caret is back where the insertion left it — the `a` was typed and then
/// deleted — so cursor-state identity alone would revive the record. The content
/// revision is what refuses it.
#[test]
fn b13_an_edit_between_the_keystrokes_invalidates_the_record() {
    let mut doc = doc_in(Language::Rust, "let s = r#;");
    let mut cursor = cursors_at(&[(0, 10)]);
    let mut handler = KeyboardHandler::new();
    let config = EditorConfig::default();

    press(&mut handler, &ch('"'), &mut doc, &mut cursor, &config);
    assert_eq!(
        doc.text(),
        "let s = r#\"\"#;",
        "the premise: the pair is there"
    );
    press(&mut handler, &ch('a'), &mut doc, &mut cursor, &config);
    press(&mut handler, &BACKSPACE, &mut doc, &mut cursor, &config);
    assert_eq!(
        doc.text(),
        "let s = r#\"\"#;",
        "the `a` goes and nothing else — the caret is back between the halves \
         but the buffer is a revision on from the one the record was taken at"
    );
    assert_eq!(heads(&cursor), vec![(0, 11)]);

    press(&mut handler, &BACKSPACE, &mut doc, &mut cursor, &config);
    assert_eq!(
        doc.text(),
        "let s = r##;",
        "and the next Backspace gets the single-character answer, not the \
         four-character one"
    );
}

/// ⭐ **Discriminator for the cursor-state half of the record.** A second caret
/// added after the insertion changes the cursor state, so the record stops
/// describing it and the first caret loses its collapse too.
///
/// One keystroke wrote one command across the carets it had; a different set of
/// carets is a different edit's state, and the record is one keystroke's.
#[test]
fn b13_a_cursor_added_after_the_insertion_invalidates_the_record() {
    let mut doc = doc_in(Language::Rust, "let s = r#;\nx");
    let mut cursor = cursors_at(&[(0, 10)]);
    let mut handler = KeyboardHandler::new();
    let config = EditorConfig::default();

    press(&mut handler, &ch('"'), &mut doc, &mut cursor, &config);
    assert_eq!(
        doc.text(),
        "let s = r#\"\"#;\nx",
        "the premise: the pair is there"
    );

    let mut with_extra = cursors_at(&[(0, 11), (1, 1)]);
    press(&mut handler, &BACKSPACE, &mut doc, &mut with_extra, &config);
    assert_eq!(
        doc.text(),
        "let s = r##;\n",
        "the recorded caret is still at (0, 11), but the cursor state is not \
         the one the record was taken at, so nothing collapses by record"
    );
}

// ========== The rule ==========

/// ⚠️ **Control.** Nothing about the single-character collapse changes: it keeps
/// its buffer-based rule deliberately, and every pair that collapsed before must
/// still collapse whoever wrote it.
///
/// ⭐ **This is where the asymmetry lives, and it is not an oversight.**
/// Over-deleting one character is a nuisance and the user's remedy is to type it
/// again; over-deleting four is data loss. Only the destructive rule was given a
/// record to answer to.
///
/// ⭐ The Python fixtures are the load-bearing ones: `print("|")` sits one
/// character away from `print(f"|")`, and a multi-character rule that matched on
/// the quote alone would eat the `(`.
#[test]
fn an_existing_single_character_pair_still_collapses() {
    for (language, text, column, expected, caret) in [
        (Some(Language::Rust), "f()", 2, "f", 1),
        (Some(Language::Rust), "let s = \"\";", 9, "let s = ;", 8),
        (Some(Language::Python), "print(\"\")", 7, "print()", 6),
        (Some(Language::Python), "print('')", 7, "print()", 6),
        (Some(Language::Json), "{\"a\": []}", 7, "{\"a\": }", 6),
        (None, "[]", 1, "", 0),
    ] {
        let (after, heads) = backspaced_with(language, text, column, &EditorConfig::default());
        assert_eq!(
            after, expected,
            "backspace at column {column} of {text:?} must collapse the pair"
        );
        assert_eq!(heads, vec![(0, caret)]);
    }
}

/// ⭐ **Elimination, red under R-closed.** Rust's three raw-string widths, each giving back the
/// buffer the quote was typed into. The single-character collapse takes the two
/// quotes and leaves `r##`, `r####`, `r######` behind — a mangled opener.
///
/// ⭐ **This is also where the closer's length is observable**, and it is the
/// only thing about the row the delete range reads: at `r##"|"##` a rule taking
/// `"#` instead of `"##` leaves `let s = r###;`, one `#` too many.
#[test]
fn a_raw_string_pair_gives_back_the_opener_the_user_typed() {
    for (before, column, typed) in [
        ("let s = r#;", 10, "let s = r#\"\"#;"),
        ("let s = r##;", 11, "let s = r##\"\"##;"),
        ("let s = r###;", 12, "let s = r###\"\"###;"),
    ] {
        let (inserted, after, heads) = typed_then_backspaced(Language::Rust, before, column, '"');
        assert_eq!(inserted, typed, "the premise: the closer was written");
        assert_eq!(
            after, before,
            "the quote and the closer it wrote go, and the prefix stays"
        );
        assert_eq!(heads, vec![(0, column)]);
    }
}

/// ⭐ **Elimination, red under R-closed.** The `/*` seven. The closer is `" */"` with its leading
/// space, and all three characters of it go — the `/` the user typed before the
/// `*` does not.
///
/// The single-character rule cannot collapse this at all — `*` opens no
/// single-character pair — so without the record's answer it deletes the `*` and
/// leaves ` */` orphaned.
#[test]
fn a_block_comment_pair_takes_the_closers_leading_space_and_leaves_the_slash() {
    for language in BLOCK_COMMENT_LANGUAGES {
        let (inserted, after, caret) = typed_then_backspaced(language, "fn f() {/}", 9, '*');
        assert_eq!(
            inserted, "fn f() {/* */}",
            "{language:?}: the premise: the closer was written"
        );
        assert_eq!(
            after, "fn f() {/}",
            "{language:?}: the space belongs to the closer and goes with it; the \
             `/` was in the buffer before the `*` was typed and stays"
        );
        assert_eq!(caret, vec![(0, 9)]);
    }
}

/// ⭐ **Elimination, red under R-closed.** Python's two triple-quote rows. The third quote and the
/// three it wrote go, and the two that were already there stay — `""|` is
/// exactly the buffer that third quote was typed into.
#[test]
fn a_triple_quoted_pair_leaves_the_two_quotes_it_was_typed_after() {
    for (before, typed, inserted_text) in [
        ("print(\"\")", '"', "print(\"\"\"\"\"\")"),
        ("print('')", '\'', "print('''''')"),
    ] {
        let (inserted, after, caret) = typed_then_backspaced(Language::Python, before, 8, typed);
        assert_eq!(inserted, inserted_text, "the premise, from {before:?}");
        assert_eq!(after, before, "from {before:?}");
        assert_eq!(caret, vec![(0, 8)]);
    }
}

/// ⚠️ **Control** (green under all three: `b"` and `rb"` both close with one
/// character, so the single-character answer agrees). At `print(rb|)` both `b"` and `rb"` match the text
/// before the caret; under §10.7 step 5 as written the longer one won and took
/// the `r` and the `b` with it, leaving `print()`.
///
/// Under B-8 the question does not arise: whichever row matched, one character
/// goes to the left, and it is the quote the user typed.
#[test]
fn which_row_matched_cannot_change_how_much_is_deleted() {
    let (inserted, after, caret) = typed_then_backspaced(Language::Python, "print(rb)", 8, '"');
    assert_eq!(
        inserted, "print(rb\"\")",
        "the premise: the closer was written"
    );
    assert_eq!(
        after, "print(rb)",
        "`rb\"` is the longer match and `b\"` the shorter, and neither may take \
         a character the user typed"
    );
    assert_eq!(caret, vec![(0, 8)]);
}

/// ⚠️ **Control for B-8's left bound.** `f"` is the opener the manifest
/// declares, but the `f` was in the buffer before the quote was typed, so one
/// backspace takes the quote and the closer it wrote and nothing else.
///
/// Consistent with `r#"|"#`, which leaves the `r#` for the same reason. Someone
/// who wants the `f` gone presses Backspace again.
#[test]
fn an_empty_prefixed_string_keeps_the_prefix_the_user_typed() {
    for (prefix, quote) in [("f", '"'), ("b", '\''), ("rb", '"'), ("t", '\'')] {
        let before = format!("print({prefix})");
        let column = "print(".len() + prefix.len();
        let (inserted, after, caret) =
            typed_then_backspaced(Language::Python, &before, column, quote);
        assert_eq!(
            inserted,
            format!("print({prefix}{quote}{quote})"),
            "the premise, from {before:?}"
        );
        assert_eq!(after, before, "from {before:?}");
        assert_eq!(caret, vec![(0, column)]);
    }
}

/// ⭐ **B-8's red proof, and a control now.** Backspace may delete the closer
/// the editor wrote and the one character the user typed to trigger it, and
/// nothing else — never a prefix that was already in the buffer.
///
/// ⚠️ The single-character collapse would give the same answer for all three,
/// and that is the property being asserted: **the multi-character rule may
/// never delete more than it does on the left.** Which branch produced the
/// answer is not the observable and is not asserted; the buffer is.
#[test]
fn backspace_restores_the_buffer_the_keystroke_found() {
    for (before, column, quote) in [
        // The identifier `verb` ends in `rb`, so Python's `rb"` row matches the
        // text before the caret. Two characters of a variable name.
        ("print(verb)", 10, '"'),
        // `it` ends in `t`, and `t'` is a declared row.
        ("it", 2, '\''),
        // Inside a comment. This harness resolves no scope, so `not_in` cannot
        // fire and the `rb"` row is the one that matched — which makes the `rb`
        // exactly the prefix a whole-pair collapse would have eaten.
        ("# verb", 6, '"'),
    ] {
        let (_, after, heads) = typed_then_backspaced(Language::Python, before, column, quote);
        assert_eq!(
            after, before,
            "backspace after typing {quote:?} at column {column} of {before:?} \
             must restore the buffer the keystroke found, not eat the prefix in \
             front of it"
        );
        assert_eq!(heads, vec![(0, column)]);
    }
}

/// ⚠️ **Control.** A rule that asked only what the character before the caret
/// was would find `"""` in front of this caret, match Python's triple-quote row
/// and take four characters, leaving `x = `.
///
/// Nothing typed this, so there is no record and the multi-character rule has
/// nothing to say; the single-character collapse takes the two quotes it is sure
/// of.
#[test]
fn a_quote_with_no_declared_opener_behind_it_takes_only_its_own_partner() {
    let (after, caret) = backspaced(Language::Python, "x = \"\"\"\"", 5);
    assert_eq!(
        after, "x = \"\"",
        "two quotes, not four: no multi-character opener sits behind this caret"
    );
    assert_eq!(caret, vec![(0, 4)]);
}

/// ⚠️ **Control.** A pair with something in it is not a pair this editor just
/// wrote: backspace deletes one character, exactly as it always has.
///
/// Without this a rule that collapsed `r#"` wherever it found one would pass
/// every other test in the file.
#[test]
fn a_non_empty_multi_character_pair_deletes_one_character() {
    for (language, text, column, expected, caret) in [
        (
            Language::Rust,
            "let s = r#\"a\"#;",
            11,
            "let s = r#a\"#;",
            10,
        ),
        (Language::Rust, "fn f() {/*x */}", 10, "fn f() {/x */}", 9),
        (
            Language::Python,
            "print(\"\"\"a\"\"\")",
            9,
            "print(\"\"a\"\"\")",
            8,
        ),
    ] {
        let (after, heads) = backspaced(language, text, column);
        assert_eq!(
            after, expected,
            "the pair at column {column} of {text:?} is not empty"
        );
        assert_eq!(heads, vec![(0, caret)]);
    }
}

/// ⚠️ **Control.** The written closer is matched line-locally, so a pair whose
/// halves sit on different lines never collapses — the same discipline
/// `char_at` and `char_before` already keep.
#[test]
fn a_pair_split_across_lines_never_collapses() {
    let (after, caret) = backspaced(Language::Rust, "fn f() {/*\n */}", 10);
    assert_eq!(
        after, "fn f() {/\n */}",
        "the closer is on the next line and is not part of this caret's pair"
    );
    assert_eq!(caret, vec![(0, 9)]);
}

/// ⭐ **Elimination, red under R-closed; multi-cursor.** One keystroke, two carets, and the record
/// covers both — one with a multi-character closer and one with none at all.
/// Each caret gets its own answer from the same record.
#[test]
fn two_carets_collapse_their_own_pairs_in_one_edit() {
    let mut doc = doc_in(Language::Rust, "let a = r#;\nlet b = ;");
    let mut cursor = cursors_at(&[(0, 10), (1, 8)]);
    let mut handler = KeyboardHandler::new();
    let config = EditorConfig::default();

    press(&mut handler, &ch('"'), &mut doc, &mut cursor, &config);
    assert_eq!(
        doc.text(),
        "let a = r#\"\"#;\nlet b = \"\";",
        "the premise: the first caret got the `r#\"` row's closer and the \
         second the plain quote's"
    );

    press(&mut handler, &BACKSPACE, &mut doc, &mut cursor, &config);
    assert_eq!(
        doc.text(),
        "let a = r#;\nlet b = ;",
        "the first caret takes a quote and the `\"#` the record names, the \
         second a quote and the `\"` the single-character rule finds"
    );
    assert_eq!(heads(&cursor), vec![(0, 10), (1, 8)]);
}

/// ⭐ **Elimination, red under R-closed.** A multi-character collapse is one reversible command, like
/// every other edit on this path — text and every cursor restored together.
///
/// ⭐ Both closer widths, because the inverse has to restore what the forward
/// edit took and only the second of these takes four characters: `"#` is two
/// characters against a one-character trigger, and `" */"` is three. A command
/// whose inverse re-inserted a fixed width, or the trigger alone, would pass the
/// first fixture and fail the second.
#[test]
fn a_multi_character_collapse_is_one_undoable_command() {
    for (language, before, column, typed, paired) in [
        (Language::Rust, "let s = r#;", 10, '"', "let s = r#\"\"#;"),
        (Language::Rust, "fn f() {/}", 9, '*', "fn f() {/* */}"),
    ] {
        let mut doc = doc_in(language, before);
        let mut cursor = cursors_at(&[(0, column)]);
        let mut handler = KeyboardHandler::new();
        let config = EditorConfig::default();

        press(&mut handler, &ch(typed), &mut doc, &mut cursor, &config);
        assert_eq!(doc.text(), paired, "the premise, from {before:?}");
        let paired_cursor = cursor.clone();

        let cmd = press(&mut handler, &BACKSPACE, &mut doc, &mut cursor, &config)
            .expect("a pair collapse must produce a command");
        assert_eq!(doc.text(), before, "the collapse of {paired:?}");

        cmd.inverse()
            .apply(&mut doc, &mut cursor)
            .expect("inverse must apply");
        assert_eq!(doc.text(), paired, "one inverse restores {paired:?}");
        assert_eq!(cursor, paired_cursor, "and every cursor with it");
    }
}

/// ⚠️ **Elimination, red under R-closed: a caret whose column and byte offset
/// differ.** `é` is two bytes
/// and one character, so the caret is at character 11 and byte 12, and the three
/// characters this deletes span four bytes' worth of the line's own indexing.
///
/// The delete range is built in characters — one to the left, `"#`'s two to the
/// right — while the line is matched by slicing a `str`. A column used as a byte
/// index would cut `é` in half and panic; a closer measured in bytes would take
/// the wrong count. Every other fixture in this file is ASCII, where the two
/// coincide and neither mistake can be seen.
#[test]
fn a_multi_byte_character_before_the_opener_does_not_shift_the_collapse() {
    let (inserted, after, caret) = typed_then_backspaced(Language::Rust, "let é = r#;", 10, '"');
    assert_eq!(
        inserted, "let é = r#\"\"#;",
        "the premise: the closer was written"
    );
    assert_eq!(
        after, "let é = r#;",
        "the quote and the `\"#` it wrote go, and `let é = r#` stays"
    );
    assert_eq!(caret, vec![(0, 10)], "and the caret is a character index");
}

// ========== B-13: history is not a re-authorisation ==========

/// ⭐ **Discriminator for the revision half of the record, through history.**
/// Undo and redo restore the pair and the exact caret the insertion left, and
/// the collapse still does not fire.
///
/// ⚠️ **Pinned as specified rather than as pleasant.** A redo that puts the text
/// and the carets back is, byte for byte, the state the record described — and
/// undo/redo bump the content revision even when they restore identical
/// content, which is what refuses it. Ruling B-13 names undo and redo among the
/// things that leave no match, and on a destructive path the conservative answer
/// is the specified one: one more Backspace recovers the rest, and nothing
/// recovers text the editor ate.
///
/// Measured against the code this replaces: `left: "let s = r#;"`.
#[test]
fn b13_redo_that_restores_the_pair_does_not_restore_the_record() {
    let mut editor = Editor::with_defaults();
    editor.set_content("let s = r#;");
    editor.set_language(Language::Rust);
    editor.set_cursor(Position::new(0, 10));

    editor.handle_key(&KeyEvent::simple(KeyCode::Char('"')));
    assert_eq!(
        editor.state().document.text(),
        "let s = r#\"\"#;",
        "the premise: the closer was written"
    );

    assert!(editor.undo(), "the insertion must be undoable");
    assert_eq!(editor.state().document.text(), "let s = r#;");
    assert!(editor.redo(), "and redoable");
    assert_eq!(
        editor.state().document.text(),
        "let s = r#\"\"#;",
        "the premise: redo restored the pair"
    );
    assert_eq!(
        editor.state().cursor.primary.head,
        Position::new(0, 11),
        "and the exact caret the insertion left, so cursor identity alone would \
         revive the record"
    );

    editor.handle_key(&KeyEvent::simple(KeyCode::Backspace));
    assert_eq!(
        editor.state().document.text(),
        "let s = r##;",
        "the single-character answer: the revision is not the one the record was \
         taken at"
    );
}

/// **Control** under all three reversions, and pinned for the opposite reason
/// to every other record test here: it asserts the record is *kept*.
///
/// A pure motion round trip that lands back on the recorded state revives the
/// record, and `auto_pair_record`'s module docs argue at length that this is
/// correct — the record claims *the editor wrote this closer at this caret*,
/// and a document with the same identity and revision is the same bytes, so
/// the claim still holds.
///
/// ⚠️ **Every other record test asserts a refusal, and refusal tests cannot
/// catch a guard that became too eager.** Adding a `sticky_dirty`-style reset
/// to the motion path "for safety" would flip this sequence from `let s = r#;`
/// to `let s = r##;` with the whole suite still green. This is the test that
/// goes red instead. Found by the B-13 adversary as an unpinned known case —
/// §10.8's own rule, applied to a deliberate behaviour rather than to a false
/// positive.
#[test]
fn b13_a_motion_round_trip_back_to_the_recorded_state_revives() {
    let mut doc = doc_in(Language::Rust, "let s = r#;");
    let mut cursor = cursors_at(&[(0, 10)]);
    let mut handler = KeyboardHandler::new();
    let config = EditorConfig::default();

    press(&mut handler, &ch('"'), &mut doc, &mut cursor, &config);
    assert_eq!(
        doc.text(),
        "let s = r#\"\"#;",
        "the premise: the pair is there"
    );
    assert_eq!(heads(&cursor), vec![(0, 11)], "and the caret is inside it");

    let revision_after_insertion = doc.revision();
    for key in [KeyCode::Right, KeyCode::Right, KeyCode::Left, KeyCode::Left] {
        press(
            &mut handler,
            &KeyEvent::simple(key),
            &mut doc,
            &mut cursor,
            &config,
        );
    }
    assert_eq!(
        heads(&cursor),
        vec![(0, 11)],
        "the premise of the round trip: the caret is back where it started"
    );
    assert_eq!(
        doc.revision(),
        revision_after_insertion,
        "and motion moved no text, so the revision is untouched — both keys \
         the record validates against still agree"
    );

    press(&mut handler, &BACKSPACE, &mut doc, &mut cursor, &config);
    assert_eq!(
        doc.text(),
        "let s = r#;",
        "the record survived the round trip, so the collapse still fires and \
         gives back exactly the buffer the quote was typed into"
    );
}

/// **Discriminator** for the one term of [`AutoPairInsertion::describes`] that
/// no keystroke can reach, asserted directly against the predicate.
///
/// ⭐⭐ **This test exists because deleting the term it pins leaves every
/// end-to-end test in this file green.** Measured by the B-13 adversary: the
/// only document swap a handler can see is `EditorState::set_content`, which
/// builds the replacement with `Document::continuing_from` and therefore bumps
/// the revision — so the revision term has always already refused, and the
/// identity term is never the reason for a `false`.
///
/// That is precisely the shape that refuted rounds 2 through 4 of this ruling:
/// **a term the examined set agrees with its own absence on.** The answer is
/// not to delete it — a future path that installs a document without bumping
/// the revision would revive a record across files — but a guard justified by
/// a mechanism nothing exercises is a claim, and a claim gets pinned or
/// rewritten. Here it is pinned, at the only level where the divergent case
/// exists: two documents, identical content, identical revision, different ids.
#[test]
fn b13_the_document_identity_term_refuses_a_different_file_at_one_revision() {
    let mut doc_a = Document::new("let s = r#;");
    let mut doc_b = Document::new("let s = r#;");
    assert_ne!(
        doc_a.id(),
        doc_b.id(),
        "the premise: two fresh documents are two different files"
    );
    assert_eq!(
        doc_a.revision(),
        doc_b.revision(),
        "the premise the revision term cannot cover: they agree on revision, \
         which is the whole reason this term is not redundant"
    );

    let cursor = cursors_at(&[(0, 10)]);
    let command = Command::Insert {
        position: Position::new(0, 10),
        text: "\"\"#".to_string(),
    };
    let landed = [Selection::collapsed(Position::new(0, 11))];
    let closers = [Some("\"#")];
    let record = AutoPairInsertion::capture(&doc_a, &cursor, &command, &landed, &closers)
        .expect("the insertion writes a multi-character closer, so it is recorded");

    // The identical command against both, so revision and cursor state agree
    // afterwards and the document id is the ONLY term left to disagree.
    let mut state_a = cursor.clone();
    command
        .apply(&mut doc_a, &mut state_a)
        .expect("the command applies to the document it was captured against");
    let mut state_b = cursor;
    command
        .apply(&mut doc_b, &mut state_b)
        .expect("and to the identical one");

    assert_eq!(
        doc_a.revision(),
        doc_b.revision(),
        "the premise again, after the edit: the two are still at one revision"
    );
    assert_eq!(state_a, state_b, "and at one cursor state");

    assert!(
        record.describes(&doc_a, &state_a),
        "the positive control — without this the refusal below proves nothing, \
         because a predicate that always returns false would also pass it"
    );
    assert!(
        !record.describes(&doc_b, &state_b),
        "and the term under test: a different file at the same revision and the \
         same cursor state is refused, on document identity alone"
    );
}
