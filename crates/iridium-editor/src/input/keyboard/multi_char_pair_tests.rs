//! Multi-character auto-pair openers — `"""`, `f"`, `r#"`, `/*` — end to end
//! through [`KeyboardHandler`](super::KeyboardHandler).
//!
//! The auto-pair map's slice S-3, steps 2 and 3: the manifest reaches
//! [`PairRules`], the trigger set grows to every multi-character opener's final
//! character, and the insertion branch matches the longest declared opener
//! against the text before the caret. The generalised skip-over (§10.4) and the
//! backspace collapse (§10.7 step 5) are not here, and are not asserted.
//!
//! # ⚠️ Five rules can refuse a pair, and each is ruled out first
//!
//! Four of the five would make a Python test pass for the wrong reason. Each is
//! eliminated by a test asserting the *premise* rather than the conclusion:
//!
//! 1. **`c` is not a trigger.** `handle_char_input` never enters auto-pair
//!    handling unless [`PairRules::is_trigger`] says so, and `*` is in no pair.
//!    Ruled out by [`the_trigger_set_is_the_languages_and_grows_only_where_the_manifest_says`].
//! 2. **No pair is declared.** Ruled out by
//!    [`the_same_keystroke_in_a_language_without_the_row_gets_the_single_character_answer`],
//!    which holds buffer, caret and keystroke fixed and varies only the
//!    manifest.
//! 3. **`autoclose_before`** refuses in front of a character the language did
//!    not list. Every insertion fixture places a `;`, `)` or `}` at the caret —
//!    characters these languages *list*, so the permission comes through
//!    `allowed.contains(c)` and not through the end-of-line and whitespace
//!    inference S-5 records as an assumption. Ruled out by
//!    [`every_insertion_fixture_caret_is_one_autoclose_before_permits`].
//! 4. **The apostrophe rule** keeps a quote single directly after a word
//!    character. Ruled out by
//!    [`every_quote_fixture_caret_is_one_the_apostrophe_rule_permits`] — which
//!    also proves the *inverse* for `print(f)`, where the rule is live and the
//!    multi-character match must beat it.
//! 5. **`not_in`** suppresses a pair inside a string or a comment. Ruled out by
//!    harness rather than by fixture: `press` passes `CaretScopes::none()`,
//!    whose `scope_at` answers `None` for every byte, so the suppression branch
//!    cannot be entered at all here. Ruled out by
//!    [`the_harness_cannot_resolve_a_scope_so_not_in_never_fires`]. The rule's
//!    own tests need a real parse tree and live in `scope_suppression_tests`.
//!
//! ⚠️ Do not add an `Editor`-level fixture here: it would give the carets a
//! real tree, and premise 5 would stop being true for every test in the file.

#![allow(clippy::expect_used, clippy::panic)]

use iridium_lang::Language;

use super::behavior_tests::{ch, doc_in, press};
use super::behaviors::PairRules;
use super::tests::{cursors_at, heads};
use super::{Document, EditorConfig, KeyboardHandler};
use crate::editor::CaretScopes;

/// Types `key` at `column` on line 0 of `text` — tagged with `language` when
/// one is given, and with no language at all otherwise — and returns the
/// resulting document text with every caret.
pub(super) fn typed_at(
    language: Option<Language>,
    text: &str,
    column: usize,
    key: char,
) -> (String, Vec<(usize, usize)>) {
    let mut doc = language.map_or_else(|| Document::new(text), |lang| doc_in(lang, text));
    let mut cursor = cursors_at(&[(0, column)]);
    let mut handler = KeyboardHandler::new();
    press(
        &mut handler,
        &ch(key),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );
    (doc.text(), heads(&cursor))
}

/// [`typed_at`] with a language, keeping only the text.
fn text_after(language: Language, text: &str, column: usize, key: char) -> String {
    typed_at(Some(language), text, column, key).0
}

/// The rules a document in `language` gets, through the typing path's accessor.
pub(super) fn rules(language: Language) -> PairRules {
    PairRules::for_document(&doc_in(language, ""))
}

/// The seven `/*`→`" */"` languages, shared with the step 4 and 5 test modules.
pub(super) const BLOCK_COMMENT_LANGUAGES: [Language; 7] = [
    Language::Rust,
    Language::C,
    Language::Cpp,
    Language::Go,
    Language::JavaScript,
    Language::TypeScript,
    Language::Tsx,
];

// ========== The five premises ==========

/// ⚠️ Premise three: `autoclose_before` permits every insertion caret below.
///
/// Proven with `(`, which every one of these languages declares, which carries
/// no `not_in`, which no manifest opener ends with — so S-3 cannot touch it —
/// and which the apostrophe rule structurally ignores. A full pair at each
/// caret leaves `autoclose_before` as the only rule that could have been what
/// permitted it.
///
/// ⚠️ The skip-over and backspace carets of §10.4 and §10.7 step 5 are
/// deliberately absent: the character at those carets is a quote or a `#`,
/// neither of which is in `;:.,=}])>`, so `(` would correctly *not* pair there.
/// Those paths are exempt from `autoclose_before` structurally, and adding them
/// here would make this test red for an honest reason.
#[test]
fn every_insertion_fixture_caret_is_one_autoclose_before_permits() {
    for (language, text, column, expected) in [
        (Language::Python, "print(\"\")", 8, "print(\"\"())"),
        (Language::Python, "print(f)", 7, "print(f())"),
        (Language::Python, "print(verb)", 10, "print(verb())"),
        (Language::Rust, "let s = r#;", 10, "let s = r#();"),
        (Language::Rust, "let s = r##;", 11, "let s = r##();"),
        (Language::Rust, "let s = r###;", 12, "let s = r###();"),
        (Language::Rust, "let n = a;", 9, "let n = a();"),
        (Language::Rust, "{/}", 2, "{/()}"),
    ] {
        assert_eq!(
            text_after(language, text, column, '('),
            expected,
            "`(` must pair at column {column} of {text:?} in {language:?} — a \
             refusal here would be `autoclose_before`'s, and every insertion \
             test below would prove nothing"
        );
    }
}

/// ⚠️ Premise four, both halves.
///
/// The same buffers, carets and keystrokes with **no language set**:
/// [`PairRules::for_document`] then returns `PairRules::ALL`, so there is no
/// manifest, no multi-character row, no `autoclose_before` and no `not_in`, and
/// the word-character rule is the only one left that can refuse.
///
/// The first group must pair, which proves the rule does not fire there. The
/// second must stay single, which proves it *does* — and that is §10.2's
/// premise: at `print(f|)` the multi-character match has to beat a rule that is
/// live and refusing, not merely one that was never engaged.
#[test]
fn every_quote_fixture_caret_is_one_the_apostrophe_rule_permits() {
    for (text, column, expected) in [
        ("print(\"\")", 8, "print(\"\"\"\")"),
        ("let s = r#;", 10, "let s = r#\"\";"),
        ("let s = r##;", 11, "let s = r##\"\";"),
        ("let s = r###;", 12, "let s = r###\"\";"),
    ] {
        assert_eq!(
            typed_at(None, text, column, '"').0,
            expected,
            "with no language the quote must pair at column {column} of {text:?}"
        );
    }

    for (text, column, expected) in [
        ("print(f)", 7, "print(f\")"),
        ("print(verb)", 10, "print(verb\")"),
    ] {
        assert_eq!(
            typed_at(None, text, column, '"').0,
            expected,
            "the character before column {column} of {text:?} is a word \
             character, so the apostrophe rule must keep the quote single"
        );
    }

    // For the `/*` fixture the rule cannot apply at all: it tests `is_quote`,
    // and with no language `*` does not even reach auto-pair handling.
    assert!(
        !PairRules::ALL.is_trigger('*'),
        "with no language `*` must not be a trigger, so no rule can refuse it"
    );
}

/// ⚠️ Premise five: this harness cannot resolve a scope, so `not_in` is
/// unreachable for every character in every language here.
///
/// The inverse of `scope_suppression_tests`'
/// `the_fixture_resolves_to_the_three_scopes_the_tests_assume`. Both halves
/// matter: the first says the resolver has nothing to answer with, the second
/// shows behaviourally that a pair Rust *does* suppress inside strings still
/// closes on this harness. Without the second, someone giving these tests a
/// real tree would silently change what all of them mean.
#[test]
fn the_harness_cannot_resolve_a_scope_so_not_in_never_fires() {
    assert!(
        !CaretScopes::none().can_resolve(),
        "`press` passes `CaretScopes::none()`; if that could resolve, the \
         suppression branch would be live and these fixtures would be wrong"
    );

    // Rust declares `"` with not_in = ["string"], and this caret is inside a
    // string literal in every sense but the parser's.
    let (text, caret) = typed_at(Some(Language::Rust), "let s = \"a, b\";", 11, '"');
    assert_eq!(
        text, "let s = \"a,\"\" b\";",
        "no tree means no scope means no suppression — B-6's fail-open direction"
    );
    assert_eq!(caret, vec![(0, 12)]);
}

/// ⚠️ Premise one, and the only place S-3's *reachability* change is directly
/// observable. `/*` ends in `*`, which is in no pair, so without this the `/*`
/// rows could never fire however correct the matching is.
///
/// Both halves are required. Without the negative half an implementation that
/// simply added `*` to the six-slot pair table would pass, and every language
/// would start pairing stars.
#[test]
fn the_trigger_set_is_the_languages_and_grows_only_where_the_manifest_says() {
    for language in BLOCK_COMMENT_LANGUAGES {
        assert!(
            rules(language).is_trigger('*'),
            "{language:?} declares `/*`, so `*` must reach auto-pair handling"
        );
    }
    assert!(
        rules(Language::Python).is_trigger('"'),
        "the characters that were already triggers must still be"
    );
    assert!(rules(Language::Python).is_trigger('\''));

    for language in [
        Language::Python,
        Language::Json,
        Language::Yaml,
        Language::Css,
        Language::Markdown,
    ] {
        assert!(
            !rules(language).is_trigger('*'),
            "{language:?} declares no `/*` row, so `*` must stay an ordinary \
             character there"
        );
    }

    // ⚠️ The `/*` closer is `" */"`, with a leading space. Only the *opener's*
    // final character joins the set; a rule reading whole delimiters would put
    // a space in it and make every space typed in seven languages an auto-pair
    // keystroke.
    for language in Language::all() {
        assert!(
            !rules(*language).is_trigger(' '),
            "a space must never be a trigger, and {language:?} is where it \
             would first show"
        );
    }
}

/// ⭐ Premise two, and the strongest elimination in the file. Buffer, caret and
/// keystroke are held fixed; only the manifest varies.
///
/// Every language paired below declares `"`→`"` and the same
/// `autoclose_before` set as the one it is standing in for, so nothing but the
/// missing multi-character row can account for the difference. Each expected
/// value is also byte-identical to what the *unfixed* editor produces for the
/// same fixture, which is the point.
#[test]
fn the_same_keystroke_in_a_language_without_the_row_gets_the_single_character_answer() {
    for (language, text, column, key, expected) in [
        (Language::Rust, "print(\"\")", 8, '"', "print(\"\"\"\")"),
        (Language::Rust, "print(f)", 7, '"', "print(f\")"),
        (Language::Rust, "print(verb)", 10, '"', "print(verb\")"),
        (Language::C, "let s = r#;", 10, '"', "let s = r#\"\";"),
        (Language::C, "let s = r##;", 11, '"', "let s = r##\"\";"),
        (Language::Python, "{/}", 2, '*', "{/*}"),
    ] {
        assert_eq!(
            text_after(language, text, column, key),
            expected,
            "{language:?} declares no row for this opener, so typing {key:?} at \
             column {column} of {text:?} must get the single-character answer"
        );
    }
}

// ========== The rule ==========

/// ⭐ Python's two triple-quote rows: three in, three out, in one keystroke.
/// Both are here because each reaches the branch through its own character.
#[test]
fn both_triple_quote_rows_close_with_three_not_one() {
    for (buffer, quote, expected) in [
        ("print(\"\")", '"', "print(\"\"\"\"\"\")"),
        ("print('')", '\'', "print('''''')"),
    ] {
        let (text, caret) = typed_at(Some(Language::Python), buffer, 8, quote);
        assert_eq!(
            text, expected,
            "the closer is three quotes, not the single-character row's one"
        );
        assert_eq!(caret, vec![(0, 9)], "the caret lands between the halves");
    }
}

/// ⭐ §10.2, and the biggest user-visible payload of the slice. `f` is a word
/// character, so the apostrophe rule refuses this quote today; the
/// multi-character match runs *before* that rule, which is what makes Python's
/// twelve prefix rows anything other than dead data.
#[test]
fn a_prefix_row_beats_the_apostrophe_rule() {
    let (text, caret) = typed_at(Some(Language::Python), "print(f)", 7, '"');
    assert_eq!(
        text, "print(f\"\")",
        "`f\"` is a declared opener, and the rule that keeps `don't` single \
         must not reach it"
    );
    assert_eq!(caret, vec![(0, 8)]);
}

/// All twelve of them, both quotes each. `rb` also exercises the two-character
/// prefix, where `b"` matches as well and the longer row is the right one.
#[test]
fn every_python_prefix_row_closes_its_quote() {
    for prefix in ["f", "b", "u", "r", "rb", "t"] {
        for quote in ['"', '\''] {
            let text = format!("print({prefix})");
            let column = "print(".len() + prefix.len();
            let expected = format!("print({prefix}{quote}{quote})");
            assert_eq!(
                text_after(Language::Python, &text, column, quote),
                expected,
                "`{prefix}{quote}` is a declared opener"
            );
        }
    }
}

/// ⭐ Rust's three raw-string widths, separated by the `ends_with` rule alone
/// and with no special casing: `"…r##"` ends with `r##` and not with `r#`, so
/// the narrower row cannot match the wider caret.
///
/// ⚠️ This proves the *matching* semantics, not the length tie-break. No
/// vendored fixture makes longest-match observable through the text, because
/// the only real contest in the data — Python's `b"` against `rb"` — declares
/// the same closer on both rows.
#[test]
fn the_three_raw_string_widths_separate_by_the_text_before_the_caret() {
    for (text, column, expected, caret) in [
        ("let s = r#;", 10, "let s = r#\"\"#;", 11),
        ("let s = r##;", 11, "let s = r##\"\"##;", 12),
        ("let s = r###;", 12, "let s = r###\"\"###;", 13),
    ] {
        let (after, heads) = typed_at(Some(Language::Rust), text, column, '"');
        assert_eq!(
            after, expected,
            "typing `\"` at column {column} of {text:?}"
        );
        assert_eq!(heads, vec![(0, caret)]);
    }
}

/// ⚠️ **A caret whose column and byte offset differ.** `é` is two bytes and one
/// character, so the opener begins at character 8 and byte 9, and the insertion
/// lands at character 10 and byte 11.
///
/// Nothing in this path may read a column as a byte index or a length in bytes
/// as a length in characters. Both would still produce the right answer for
/// every ASCII fixture in this file — and this is a codebase that has shipped
/// that class of defect before, so the fixture is cheap insurance rather than
/// decoration. A `str` index off a character boundary panics, so a wrong reading
/// fails loudly here rather than quietly.
#[test]
fn a_multi_byte_character_before_the_opener_does_not_shift_the_insertion() {
    let (after, heads) = typed_at(Some(Language::Rust), "let é = r#;", 10, '"');
    assert_eq!(
        after, "let é = r#\"\"#;",
        "the `r#\"` row matches on characters, and `é` is one of them"
    );
    assert_eq!(heads, vec![(0, 11)], "the caret is a character index");
}

/// ⭐ The `/*` seven, and the closer's leading space kept exactly as the
/// manifests spell it — `/*| */`, not `/*|*/`.
#[test]
fn the_block_comment_pairs_and_keeps_the_closers_leading_space() {
    for language in BLOCK_COMMENT_LANGUAGES {
        let (text, caret) = typed_at(Some(language), "{/}", 2, '*');
        assert_eq!(
            text, "{/* */}",
            "{language:?} declares `/*`→`\" */\"`, and nothing may trim that space"
        );
        assert_eq!(caret, vec![(0, 3)]);
    }
}

/// The other side of widening the trigger set: a `*` completing no opener is
/// still an ordinary character, in the very languages that gained the trigger.
#[test]
fn a_star_completing_no_opener_is_still_just_a_star() {
    for language in BLOCK_COMMENT_LANGUAGES {
        let (text, caret) = typed_at(Some(language), "let n = a;", 9, '*');
        assert_eq!(
            text, "let n = a*;",
            "{language:?}: the text before the caret ends with `a`, not `/`"
        );
        assert_eq!(caret, vec![(0, 10)]);
    }
}

/// ⚠️ §10.3's accepted false match, pinned as a decision rather than left to be
/// discovered. `verb` ends in `b`, so Python's `b"` row matches and the quote
/// now pairs where the apostrophe rule used to refuse it. The closer is the
/// same `"` the single-character row would have written, so the only change is
/// that a pair appears — which is exactly what §10.2 asked for.
#[test]
fn a_quote_after_an_identifier_ending_in_a_prefix_letter_now_pairs() {
    let (text, caret) = typed_at(Some(Language::Python), "print(verb)", 10, '"');
    assert_eq!(text, "print(verb\"\")");
    assert_eq!(caret, vec![(0, 11)]);
}

/// The multi-character branch is subject to `autoclose_before` exactly as the
/// single-character one is.
///
/// ⚠️ Not part of the slice's red proof, and labelled so nobody reads it as
/// one: the unfixed editor produces the same `{/*x}` because `*` never reaches
/// auto-pair handling at all. It is red against an S-3 that matched the opener
/// and forgot the gate, which is what it is for — hence the second assertion,
/// which changes only the character at the caret and gets the pair.
#[test]
fn a_multi_character_opener_does_not_close_in_front_of_an_unlisted_character() {
    let (text, caret) = typed_at(Some(Language::Rust), "{/x}", 2, '*');
    assert_eq!(
        text, "{/*x}",
        "`x` is not in Rust's `;:.,=}}])>`, so nothing may be closed in front of it"
    );
    assert_eq!(caret, vec![(0, 3)]);

    assert_eq!(
        text_after(Language::Rust, "{/}", 2, '*'),
        "{/* */}",
        "the same caret with a listed character closes, so the refusal above is \
         `autoclose_before`'s and not the matcher failing to find `/*`"
    );
}

/// Two carets, one keystroke, and only one of them completes an opener. The
/// match is against each caret's own preceding text, not the document's.
#[test]
fn two_carets_get_their_own_answers_in_one_edit() {
    let mut doc = doc_in(Language::Rust, "let a = r#;\nlet b = ;");
    let mut cursor = cursors_at(&[(0, 10), (1, 8)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('"'),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(
        doc.text(),
        "let a = r#\"\"#;\nlet b = \"\";",
        "the first caret completes `r#\"`, the second only the plain quote"
    );
    assert_eq!(heads(&cursor), vec![(0, 11), (1, 9)]);
}

/// A multi-character pair is one reversible command, like every other edit on
/// this path — not a keystroke that writes text some other way.
#[test]
fn a_multi_character_pair_is_one_undoable_command() {
    let mut doc = doc_in(Language::Rust, "let s = r#;");
    let original = cursors_at(&[(0, 10)]);
    let mut cursor = original.clone();
    let mut handler = KeyboardHandler::new();

    let cmd = press(
        &mut handler,
        &ch('"'),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    )
    .expect("a multi-character pair must produce a command");
    assert_eq!(doc.text(), "let s = r#\"\"#;");

    cmd.inverse()
        .apply(&mut doc, &mut cursor)
        .expect("inverse must apply");
    assert_eq!(doc.text(), "let s = r#;");
    assert_eq!(cursor, original);
}

/// ⭐ The three keystrokes §10.4 protects, in the order a Python user types
/// them: `"` opens a pair, `"` steps over the closer, `"` completes the triple.
#[test]
fn three_quotes_in_a_row_open_a_triple_quoted_string() {
    let mut doc = doc_in(Language::Python, "print()");
    let mut cursor = cursors_at(&[(0, 6)]);
    let mut handler = KeyboardHandler::new();
    let config = EditorConfig::default();

    press(&mut handler, &ch('"'), &mut doc, &mut cursor, &config);
    assert_eq!(doc.text(), "print(\"\")", "the first quote opens a pair");
    assert_eq!(heads(&cursor), vec![(0, 7)]);

    press(&mut handler, &ch('"'), &mut doc, &mut cursor, &config);
    assert_eq!(
        doc.text(),
        "print(\"\")",
        "the second steps over the closer rather than inserting"
    );
    assert_eq!(heads(&cursor), vec![(0, 8)]);

    press(&mut handler, &ch('"'), &mut doc, &mut cursor, &config);
    assert_eq!(
        doc.text(),
        "print(\"\"\"\"\"\")",
        "and the third completes `\"\"\"` and writes its closer"
    );
    assert_eq!(heads(&cursor), vec![(0, 9)]);
}
