//! The oracle for replacing the hard-coded comment table with manifest data.
//!
//! [`super::comments::language_tokens`] is a `match` over thirteen languages
//! written by hand. The same facts are in the vendored `config.toml` beside
//! each language's queries, and the plan is to delete the match and read the
//! manifest instead. This module is what makes that safe: it asserts the two
//! *already agree*, before either is touched, so the deletion is provably not a
//! behaviour change rather than apparently not one.
//!
//! It lives here rather than in `iridium-lang` because the hard-coded table is
//! here, and an oracle has to see both halves.
//!
//! # The one difference, and why it is not a bug
//!
//! Zed's `json/config.toml` carries `line_comments = ["// "]`. Iridium's table
//! gives JSON nothing, on the grounds that the JSON specification has no
//! comments. Both are defensible and the manifest's answer is the one that will
//! win, so the difference is asserted explicitly, in both directions, rather
//! than tolerated by a loose comparison — if a refresh ever makes the two
//! agree, this fails and someone gets to notice.
//!
//! # What this does not cover
//!
//! Only languages that are `Language` variants. The manifests describe eight
//! more, and the registry step is where those start to matter.

#![allow(clippy::expect_used)]

use iridium_lang::Language;
use iridium_lang::manifest::Manifest;

use super::comments::language_tokens;

/// The vendored manifest for a language, which every variant has.
fn manifest(language: Language) -> &'static Manifest {
    language
        .manifest()
        .expect("every Language variant has a vendored manifest")
}

/// The whole point: twelve of thirteen derive exactly.
#[test]
fn the_manifest_reproduces_the_hard_coded_comment_table() {
    let mut divergences = Vec::new();

    for &language in Language::all() {
        if language == Language::Json {
            // Asserted on its own terms below, with its reason.
            continue;
        }

        let (line, block) = language_tokens(language);
        let manifest = manifest(language);

        if manifest.line_comment() != line {
            divergences.push(format!(
                "{}: table says line {line:?}, manifest says {:?}",
                language.id(),
                manifest.line_comment()
            ));
        }
        if manifest.block_comment() != block {
            divergences.push(format!(
                "{}: table says block {block:?}, manifest says {:?}",
                language.id(),
                manifest.block_comment()
            ));
        }
    }

    // Collected rather than asserted one at a time: a derivation rule that is
    // subtly wrong breaks several languages at once, and seeing all of them is
    // what tells you which rule it was.
    assert!(
        divergences.is_empty(),
        "the manifest no longer derives the hard-coded comment table, so \
         replacing it would change behaviour:\n{}",
        divergences.join("\n")
    );
}

/// JSON, stated in both directions so neither side can drift unnoticed.
#[test]
fn json_is_the_one_language_where_the_two_disagree() {
    assert_eq!(
        language_tokens(Language::Json),
        (None, None),
        "Iridium's table gives JSON no comment syntax at all"
    );
    assert_eq!(
        manifest(Language::Json).line_comment(),
        Some("//"),
        "Zed's manifest gives JSON a line comment; this is the single \
         intentional difference, and reading the manifest is what settles it"
    );
    assert_eq!(
        manifest(Language::Json).block_comment(),
        None,
        "and neither source gives JSON a block comment"
    );
}

/// The four whose block pair exists only via the `documentation_comment`
/// fallback, named individually.
///
/// The bulk oracle above would catch a regression here, but it would report it
/// as four anonymous rows. This says which rule broke.
#[test]
fn the_four_languages_that_depend_on_the_fallback_still_have_their_block_pair() {
    for language in [Language::Rust, Language::Go, Language::C, Language::Cpp] {
        assert_eq!(
            manifest(language).block_comment(),
            Some(("/*", "*/")),
            "{} has no block_comment key; its pair comes from \
             documentation_comment, and losing the fallback loses block-comment \
             toggling for it",
            language.id()
        );
        assert_eq!(language_tokens(language).1, Some(("/*", "*/")));
    }
}

/// Languages that genuinely have no block comment must not acquire one.
///
/// A fallback rule that reached one key too far would give Python and friends
/// a `/* */` they do not have, and block toggling would start writing syntax
/// errors.
#[test]
fn the_languages_with_no_block_comment_gain_none_from_the_manifest() {
    for language in [
        Language::Python,
        Language::Yaml,
        Language::Bash,
        Language::Json,
    ] {
        assert_eq!(
            manifest(language).block_comment(),
            None,
            "{}",
            language.id()
        );
        assert_eq!(language_tokens(language).1, None, "{}", language.id());
    }
}

/// And the languages with no line comment must not acquire one.
#[test]
fn the_languages_with_no_line_comment_gain_none_from_the_manifest() {
    for language in [Language::Markdown, Language::Css] {
        assert_eq!(manifest(language).line_comment(), None, "{}", language.id());
        assert_eq!(language_tokens(language).0, None, "{}", language.id());
    }
}
