//! The generalised skip-over — the auto-pair map's slice S-3, step 4.
//!
//! The rule, exactly as `docs/design/AUTO-PAIR-MAP.md` §10.4 states it:
//!
//! > Typing `c` steps over the character at the caret when `c` equals that
//! > character **and** the text before the caret, plus `c`, ends a closer this
//! > language declares.
//!
//! A single-character closer satisfies that with an empty prefix, so what was
//! already here is unchanged *by construction* rather than by care —
//! [`an_existing_single_character_skip_over_still_works`] is the proof, and it
//! passes before the change as well as after. `"#` satisfies it in two
//! keystrokes, which is the case §10.4 was written for.
//!
//! ⭐ **Ruling B-10 as §10.9 reverses it narrows "declares" once more: a closer
//! joins the set only if it is reachable from the caret its own insertion
//! leaves.** `" */"` is not — `/*| */` puts a space at the caret — so it
//! contributes nothing, `/` is not a trigger in any language, and the
//! keystroke [`a_slash_in_front_of_a_slash_is_written_and_never_swallowed`]
//! reproduces is written instead of swallowed. `"#` stays, because `r#"|"#` has
//! that closer's own first character at the caret.
//!
//! # ⚠️ What can refuse a skip, and how each is ruled out
//!
//! Skip-over is structurally exempt from three of the five rules that can
//! refuse a *pair*: the branch returns before `autoclose_before`, the
//! apostrophe rule and `not_in` are ever reached. That is ruling B-7. What is
//! left, and where each is eliminated:
//!
//! 1. **`c` is not a trigger.** `#` is what step 4 adds, and without it the
//!    rule is unreachable however correct the matching is. Ruled out by
//!    [`the_trigger_set_grows_only_to_a_reachable_closers_final_character`],
//!    which also asserts the other direction for `/`.
//! 2. **The language declares no such closer.** Ruled out by
//!    [`the_same_keystroke_in_a_language_without_the_closer_writes_it`], which
//!    holds buffer, caret and keystroke fixed and varies only the manifest.
//! 3. **`autoclose_before`.** Not consulted here, and
//!    [`skip_over_is_not_gated_by_autoclose_before`] shows the exemption doing
//!    real work rather than nominal work: the character at the `"|#` caret is a
//!    `#`, which Rust's `;:.,=}])>` does not list, so a rule that *were*
//!    consulted would refuse this very fixture.
//! 4. **The apostrophe rule.** It tests `is_quote`, and neither `#` nor `/` is
//!    a quote. Where a fixture types a quote the rule is live and would refuse
//!    — `char_before` is `c`, a word character — which is exactly why "the text
//!    is byte-for-byte unchanged" is unambiguous here: every refusal, and the
//!    insertion path too, would have lengthened the line.
//! 5. **`not_in`.** `press` passes `CaretScopes::none()`, whose `scope_at`
//!    answers `None` for every byte, so the suppression branch cannot be
//!    entered on this harness at all — see `multi_char_pair_tests`, which
//!    proves that premise and whose helpers this file shares. It does not gate
//!    this branch in any case.
//!
//! ⚠️ Do not give this file an `Editor`-level fixture: it would hand the carets
//! a real parse tree and premise 5 would stop being true for every test here.

#![allow(clippy::expect_used, clippy::panic)]

use iridium_lang::Language;

use super::behavior_tests::{ch, doc_in, press};
use super::multi_char_pair_tests::{BLOCK_COMMENT_LANGUAGES, rules, typed_at};
use super::tests::{cursors_at, heads};
use super::{EditorConfig, KeyboardHandler};

/// A Rust raw string with its two-character closer already in place, and the
/// two carets §10.4 walks the user through: column 14 is `r#"abc|"#`, column 15
/// is `r#"abc"|#`.
const RAW_STRING: &str = "let s = r#\"abc\"#;";

// ========== The premises ==========

/// ⚠️ Premise one, and the only place step 4's *reachability* change is
/// directly observable.
///
/// The set grows by **the final character of every closer that can be stepped
/// over**, and by nothing else. Three distinctions, each load-bearing:
///
/// - the *final* character, not every character — the `/*` rows close with
///   `" */"`, so "every character" would make a space an auto-pair keystroke in
///   seven languages;
/// - only a closer a user can reach from the caret its own insertion leaves —
///   `" */"` cannot be (⭐ ruling B-10 as §10.9 reverses it), so it contributes
///   nothing and `/` is a trigger nowhere;
/// - only from the language that declares it.
///
/// ⚠️ The `/` assertions are the whole of the second, and they must stay
/// *negative for every language*: this is a `Language::all()` sweep and not a
/// list, because a closer that cannot be stepped over must not enter the set in
/// the language that declares it either — which is precisely where the swallowed
/// keystroke lived.
#[test]
fn the_trigger_set_grows_only_to_a_reachable_closers_final_character() {
    // Rust's closers are `}`, `"#`, `"##`, `"###`, `]`, `)`, `"` and `" */"`.
    // Only the raw-string three are steppable, and they end in `#`.
    assert!(
        rules(Language::Rust).is_trigger('#'),
        "`\"#` is Rust's raw-string closer and `r#\"|\"#` puts its first \
         character at the caret, so `#` must reach auto-pair handling"
    );

    // The raw-string rows are Rust's alone; nothing else in the tree declares a
    // closer ending in `#`.
    for language in [
        Language::C,
        Language::Cpp,
        Language::Go,
        Language::JavaScript,
        Language::TypeScript,
        Language::Tsx,
        Language::Python,
        Language::Json,
        Language::Yaml,
        Language::Markdown,
    ] {
        assert!(
            !rules(language).is_trigger('#'),
            "{language:?} declares no closer ending in `#`, so `#` must stay an \
             ordinary character there"
        );
    }

    // ⭐ The reversal. The seven `/*` languages are in this sweep, and they are
    // the ones it is about: a `/` typed anywhere must be written.
    for language in Language::all() {
        assert!(
            !rules(*language).is_trigger('/'),
            "{language:?}: `\" */\"` cannot be stepped over from `/*| */`, so it \
             contributes nothing and `/` must stay an ordinary character"
        );
    }

    // ⚠️ The guard that must survive every future widening of this set. The
    // space is unreachable for the same reason `/` now is, and would be the
    // first character `\" */\"` put in the set under a whole-delimiter rule.
    for language in Language::all() {
        assert!(
            !rules(*language).is_trigger(' '),
            "a space must never be a trigger, and {language:?} is where the \
             `\" */\"` closer would first put one"
        );
    }

    // ⚠️ The *opener* half is untouched: `/*` could not pair at all without it,
    // and B-10's reversal is the closer-derived half alone.
    for language in BLOCK_COMMENT_LANGUAGES {
        assert!(
            rules(language).is_trigger('*'),
            "{language:?} declares `/*`, so `*` must still reach auto-pair \
             handling"
        );
    }
}

/// ⭐ Premise two. Buffer, caret and keystroke are held fixed; only the
/// manifest varies, and each expected value is byte-identical to what the
/// *unfixed* editor produces for the same fixture.
///
/// Every language below declares the same `autoclose_before` set as the one it
/// stands in for, so nothing but the missing closer can account for the
/// difference.
///
/// ⚠️ `#` is the only character this premise still has to eliminate for.
/// Under §10.9 no language steps over a `/` at all, so the `/` fixtures moved
/// to [`no_language_steps_over_a_block_comment_closers_final_slash`], where
/// they say something stronger: the `/` is written in the languages that
/// declare the row as well as in the ones that do not.
#[test]
fn the_same_keystroke_in_a_language_without_the_closer_writes_it() {
    for (language, text, column, key, expected) in [
        // No `"#` closer: the `#` is written, which is the bug §10.4 names.
        (Language::C, RAW_STRING, 15, '#', "let s = r#\"abc\"##;"),
        (
            Language::Python,
            RAW_STRING,
            15,
            '#',
            "let s = r#\"abc\"##;",
        ),
    ] {
        let (after, caret) = typed_at(Some(language), text, column, key);
        assert_eq!(
            after, expected,
            "{language:?} declares no closer this keystroke could end, so \
             {key:?} at column {column} of {text:?} must be written"
        );
        assert_eq!(caret, vec![(0, column + 1)]);
    }
}

/// ⚠️ Premise three, shown to be doing real work rather than nominal work.
///
/// The skip-over branch returns before `permits_close_before` is reached, and
/// this fixture is one where that matters: the character at the caret is a `#`,
/// which Rust's `;:.,=}])>` does not list. The second assertion proves it by
/// typing a character that *is* gated at the very same caret and watching it
/// refuse to pair.
#[test]
fn skip_over_is_not_gated_by_autoclose_before() {
    let (after, caret) = typed_at(Some(Language::Rust), RAW_STRING, 15, '#');
    assert_eq!(
        after, RAW_STRING,
        "the skip happens in front of an unlisted `#`"
    );
    assert_eq!(caret, vec![(0, 16)]);

    let (paired, _) = typed_at(Some(Language::Rust), RAW_STRING, 15, '(');
    assert_eq!(
        paired, "let s = r#\"abc\"(#;",
        "`(` at the same caret must not pair, which is what proves `#` is a \
         character `autoclose_before` refuses to close in front of"
    );
}

// ========== The rule ==========

/// ⚠️ The by-construction half of §10.4: a single-character closer ends itself
/// with an empty prefix, so every skip that worked before must still work.
///
/// Not a red proof and labelled so nobody reads it as one — it passes against
/// the unfixed editor too. It is the regression guard the generalisation needs,
/// across a bracket, all three quotes' languages, and a document with no
/// language at all (`PairRules::ALL`).
#[test]
fn an_existing_single_character_skip_over_still_works() {
    for (language, text, column, key) in [
        (Some(Language::Rust), "f()", 2, ')'),
        (Some(Language::Rust), "let s = \"\";", 9, '"'),
        (Some(Language::Python), "print(\"\")", 7, '"'),
        (Some(Language::Python), "print('')", 7, '\''),
        (Some(Language::Json), "{\"a\": []}", 7, ']'),
        (None, "[]", 1, ']'),
    ] {
        let (after, caret) = typed_at(language, text, column, key);
        assert_eq!(
            after, text,
            "typing {key:?} at column {column} of {text:?} must step over it"
        );
        assert_eq!(caret, vec![(0, column + 1)]);
    }
}

/// ⭐ The case §10.4 exists for. `"#` is not a single-character closer, so
/// nothing stepped over the `#` and the user got `r#"abc"#|#`.
///
/// The observable is that the text is **byte-for-byte unchanged**: every one of
/// the five refusals, and the insertion path too, would have lengthened the
/// line.
#[test]
fn a_hash_completing_a_raw_string_closer_steps_over_it() {
    let (after, caret) = typed_at(Some(Language::Rust), RAW_STRING, 15, '#');
    assert_eq!(
        after, RAW_STRING,
        "`let s = r#\"abc\"` plus `#` ends Rust's `\"#`, so the `#` already \
         there is stepped over rather than duplicated"
    );
    assert_eq!(caret, vec![(0, 16)]);
}

/// The two keystrokes that close a raw string, in the order a user types them:
/// the quote steps over one character by the old rule, the hash by the new one.
#[test]
fn the_two_keystrokes_that_close_a_raw_string_leave_the_text_alone() {
    let mut doc = doc_in(Language::Rust, RAW_STRING);
    let mut cursor = cursors_at(&[(0, 14)]);
    let mut handler = KeyboardHandler::new();
    let config = EditorConfig::default();

    press(&mut handler, &ch('"'), &mut doc, &mut cursor, &config);
    assert_eq!(doc.text(), RAW_STRING, "the quote steps over the quote");
    assert_eq!(heads(&cursor), vec![(0, 15)]);

    press(&mut handler, &ch('#'), &mut doc, &mut cursor, &config);
    assert_eq!(doc.text(), RAW_STRING, "and the hash steps over the hash");
    assert_eq!(heads(&cursor), vec![(0, 16)]);
}

/// ⚠️ The behaviour the generalisation must **not** break. A whole-closer skip
/// would jump all three quotes on the first keystroke and the user's next two
/// would open a new pair; muscle memory types three.
///
/// All three steps are asserted, because a rule that jumped two would still
/// look right at the first.
#[test]
fn a_triple_quote_steps_over_exactly_one_quote_at_a_time() {
    const TRIPLE: &str = "print(\"\"\"abc\"\"\")";
    let mut doc = doc_in(Language::Python, TRIPLE);
    let mut cursor = cursors_at(&[(0, 12)]);
    let mut handler = KeyboardHandler::new();
    let config = EditorConfig::default();

    for column in [13, 14, 15] {
        press(&mut handler, &ch('"'), &mut doc, &mut cursor, &config);
        assert_eq!(doc.text(), TRIPLE, "no quote may be written or removed");
        assert_eq!(
            heads(&cursor),
            vec![(0, column)],
            "each keystroke steps exactly one quote"
        );
    }
}

/// ⭐ **The reversal, on the fixture that used to assert the opposite.** `/* x
/// *|/` typing `/` once stepped over the closer's final slash; under §10.9 it
/// writes it, in the seven languages that declare `" */"` and in the ones that
/// do not alike.
///
/// ⚠️ The benefit that was traded away is worth naming, because it is nothing:
/// a user closing this comment types `*` and then `/` at `/* x |*/`… which they
/// never do, because the editor already wrote the closer. The only way to reach
/// the old skip was to retype a closer that was already there.
#[test]
fn no_language_steps_over_a_block_comment_closers_final_slash() {
    for language in BLOCK_COMMENT_LANGUAGES {
        let (after, caret) = typed_at(Some(language), "/* x */", 6, '/');
        assert_eq!(
            after, "/* x *//",
            "{language:?} declares `\" */\"`, and it is still not steppable"
        );
        assert_eq!(caret, vec![(0, 7)]);
    }
    for language in [Language::Python, Language::Json] {
        let (after, caret) = typed_at(Some(language), "/* x */", 6, '/');
        assert_eq!(
            after, "/* x *//",
            "{language:?} declares no `/*` row at all, and answers identically"
        );
        assert_eq!(caret, vec![(0, 7)]);
    }
}

/// The exact case §10.4 names as the one that must **not** fire: a bare `#`
/// before a `#[derive]`. The character at the caret matches what was typed, but
/// the text before the caret is empty and `#` alone ends no declared closer.
///
/// ⚠️ Not a red proof — the unfixed editor writes the same `#` because `#` is
/// not a trigger there at all. It is the guard on widening the trigger set.
#[test]
fn a_hash_that_ends_no_closer_is_written() {
    let (after, caret) = typed_at(Some(Language::Rust), "#[derive(Debug)]", 0, '#');
    assert_eq!(
        after, "##[derive(Debug)]",
        "nothing precedes the caret, so `#` ends no closer and is written"
    );
    assert_eq!(caret, vec![(0, 1)]);
}

/// The same guard for the character step 4 briefly added and §10.9 took back. A
/// line comment and a path separator both put a `/` directly after a `/`, and
/// neither may skip.
///
/// ⚠️ Not a red proof, for the same reason as the fixture above: `/` reaches no
/// closer these fixtures could end, and since §10.9 it reaches auto-pair
/// handling in no language at all.
#[test]
fn a_slash_that_ends_no_closer_is_written() {
    for (text, column, expected) in [
        ("//", 0, "///"),
        ("//", 1, "///"),
        ("let u = \"http://x\";", 15, "let u = \"http:///x\";"),
        ("let n = a;", 9, "let n = a/;"),
    ] {
        let (after, caret) = typed_at(Some(Language::Rust), text, column, '/');
        assert_eq!(
            after, expected,
            "typing `/` at column {column} of {text:?} ends no closer"
        );
        assert_eq!(caret, vec![(0, column + 1)]);
    }
}

/// ⚠️ §10.4's accepted false positive, pinned as a decision rather than left to
/// be discovered. Any `"` followed by a `#` ends Rust's `"#`, whether or not a
/// raw string is what is actually there.
///
/// It is accepted because the only way to reach it is to type a `#` that is
/// already present — the character is not lost, the caret simply moves past it.
#[test]
fn a_hash_after_any_quote_steps_over_one_that_is_already_there() {
    let (after, caret) = typed_at(Some(Language::Rust), "let s = \"#;", 9, '#');
    assert_eq!(
        after, "let s = \"#;",
        "`let s = \"` plus `#` ends `\"#`, and this rule does not ask whether a \
         raw string opened it"
    );
    assert_eq!(caret, vec![(0, 10)]);
}

/// ⭐ **The unreachability that decides B-10**, and the reason `/` is not a
/// trigger: `" */"` cannot be stepped over from the caret its own insertion
/// leaves. `/*| */` typing `*` finds a **space** at the caret, not a `*`, so
/// the rule's first condition fails before the closer is ever considered.
/// Reaching the `*` means typing the space first, which nobody does.
///
/// That is what makes the generalised skip-over unreachable in the case these
/// rows exist to serve and reachable only in false-positive ones — so it buys
/// nothing and costs a swallowed keystroke, which is §10.9's argument in full.
///
/// Pinned so that a later attempt to "fix" it is a deliberate change to a
/// tested behaviour rather than a quiet one.
#[test]
fn the_block_comment_closer_is_not_reachable_from_the_caret_it_leaves() {
    for language in BLOCK_COMMENT_LANGUAGES {
        let (after, caret) = typed_at(Some(language), "fn f() {/* */}", 10, '*');
        assert_eq!(
            after, "fn f() {/** */}",
            "{language:?}: the character at the caret is a space, so the `*` is \
             written"
        );
        assert_eq!(caret, vec![(0, 11)]);
    }
}

/// ⭐ **B-10 reversed — §10.9's red proof.** `" */"` contributes nothing to the
/// skip-over trigger set, so a `/` typed in front of a `/` is written.
///
/// The buffer is ordinary source: the string contains `a */b`, the text before
/// the caret ends with ` *`, and the character at the caret is a `/`. Under the
/// unreversed rule the keystroke is swallowed and the text is unchanged.
///
/// ⚠️ Being inside a string does not rescue it, which is why this needs no
/// parse tree: skip-over is exempt from `not_in` (ruling B-7), so a real tree
/// would answer identically. **Silently discarding a keystroke is a worse
/// failure than writing an unwanted character** — the correction for the second
/// is Backspace and for the first is to wonder whether the keyboard is broken.
#[test]
fn a_slash_in_front_of_a_slash_is_written_and_never_swallowed() {
    for language in BLOCK_COMMENT_LANGUAGES {
        let (after, caret) = typed_at(Some(language), "const s = \"a */b\";", 14, '/');
        assert_eq!(
            after, "const s = \"a *//b\";",
            "{language:?}: `\" */\"` is unreachable from the caret its own \
             insertion leaves, so `/` is not a trigger and this `/` is typed"
        );
        assert_eq!(caret, vec![(0, 15)]);
    }
}

/// Two carets, one keystroke, and the match is against each caret's own
/// preceding text rather than the document's.
#[test]
fn two_carets_decide_their_own_skips_in_one_edit() {
    let mut doc = doc_in(Language::Rust, "let a = r#\"x\"#;\nlet b = #;");
    let mut cursor = cursors_at(&[(0, 13), (1, 8)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('#'),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(
        doc.text(),
        "let a = r#\"x\"#;\nlet b = ##;",
        "the first caret steps over its `#`; the second is preceded by a space, \
         ends no closer, and writes one"
    );
    assert_eq!(heads(&cursor), vec![(0, 14), (1, 9)]);
}

/// ⚠️ **A caret whose column and byte offset differ.** `é` is two bytes and one
/// character, so the closer's `#` sits at character 13 and byte 14.
///
/// The skip-over match cuts the line at the caret and asks whether that prefix
/// plus the typed character ends a declared closer; a cut made with a column
/// used as a byte index would slice `é` in half — a panic — or, one character
/// further on, silently compare the wrong text. Every other fixture in this file
/// is ASCII, where the two indices coincide and neither mistake is observable.
#[test]
fn a_multi_byte_character_before_the_opener_does_not_shift_the_skip() {
    let (after, caret) = typed_at(Some(Language::Rust), "let é = r#\"a\"#;", 13, '#');
    assert_eq!(
        after, "let é = r#\"a\"#;",
        "the `#` already there is stepped over, not duplicated"
    );
    assert_eq!(caret, vec![(0, 14)], "and the caret is a character index");
}
