//! What each language comments with, pinned independently of where it is read
//! from.
//!
//! This module began as the oracle for #66 step 4: it held the hand-written
//! `language_tokens` match against the vendored manifests and asserted the two
//! agreed, language by language, so that deleting the match was provably not a
//! behaviour change. That match is now gone, and with it the ability to compare
//! two implementations.
//!
//! What replaces it is the same guarantee stated as data. The table below is
//! written out by hand, deliberately, and is *not* derived from the manifests —
//! deriving it would make it agree with any manifest, including a broken one.
//! It is the third opinion: if a vendor refresh silently changes what comments
//! Rust, this fails, and it fails naming the language and both tokens.
//!
//! # The one row that changed
//!
//! JSON. The deleted match gave it nothing, on the grounds that the JSON
//! specification has no comments. Its manifest gives it `//`, and reading the
//! manifest is what settles it — this is #62, answered by data rather than by
//! a ruling. `Ctrl+/` now writes a comment in a `.json` file, which strict
//! parsers reject and which JSONC, JSON5 and every editor-owned `.json` in
//! practice accept.
//!
//! # What this does not cover
//!
//! Only languages the registry lists. The vendored manifests describe seven
//! more — `diff`, `gitcommit`, `gomod`, `gowork`, `jsdoc`, `markdown-inline`
//! and `regex` — which are vendored but not yet admitted; see
//! `docs/IN-FLIGHT-registry.md` §6.
//!
//! The unknown-language tests below say `"nonesuch"`. They used to say
//! `"awl"`, chosen because nothing claimed it — and then AWL became a real
//! language, which is exactly the hazard of picking a sentinel that looks like
//! a plausible name. `"nonesuch"` is not a language anyone is going to add.

#![allow(clippy::expect_used)]

use iridium_lang::Language;

use super::comments::{CommentSyntax, resolve_comment_syntax};
use crate::document::Document;
use crate::editor::EditorConfig;

/// One row of [`EXPECTED`]: a language id, its line token, its block pair.
type CommentRow = (
    &'static str,
    Option<&'static str>,
    Option<(&'static str, &'static str)>,
);

/// What every language Iridium knows comments with.
///
/// Hand-written and not derived — see the module note on why that is the point.
const EXPECTED: &[CommentRow] = &[
    // AWL has no block pair at all, so `Ctrl+Shift+/` correctly does nothing
    // in a `.awl` file. Its `///` and `//!` are distinct grammar tokens rather
    // than a convention over `//`, so — unlike Rust, which lists all three —
    // its manifest lists only `// `, and the toggle cannot accidentally write
    // a doc comment.
    ("awl", Some("//"), None),
    ("rust", Some("//"), Some(("/*", "*/"))),
    ("python", Some("#"), None),
    ("typescript", Some("//"), Some(("/*", "*/"))),
    ("javascript", Some("//"), Some(("/*", "*/"))),
    ("tsx", Some("//"), Some(("/*", "*/"))),
    ("go", Some("//"), Some(("/*", "*/"))),
    // The row #66 changed. See the module note.
    ("json", Some("//"), None),
    ("yaml", Some("#"), None),
    ("markdown", None, Some(("<!--", "-->"))),
    ("css", None, Some(("/*", "*/"))),
    ("bash", Some("#"), None),
    ("c", Some("//"), Some(("/*", "*/"))),
    ("cpp", Some("//"), Some(("/*", "*/"))),
];

/// A document tagged with a language identifier and nothing else.
fn document_in(id: &str) -> Document {
    let mut document = Document::new("");
    document.set_language(Some(id.to_owned()));
    document
}

/// Resolves through the real path a keypress takes, with no configuration
/// fallback available — so anything this returns came from the manifest.
fn resolved(id: &str) -> CommentSyntax {
    let config = EditorConfig {
        line_comment_token: None,
        ..EditorConfig::default()
    };
    resolve_comment_syntax(&document_in(id), &config)
        .unwrap_or_else(|| panic!("{id} resolved to no comment syntax at all"))
}

#[test]
fn every_language_comments_the_way_the_table_says() {
    let mut wrong = Vec::new();

    for &(id, line, block) in EXPECTED {
        let CommentSyntax {
            line: actual_line,
            block: actual_block,
        } = resolved(id);
        let expected_line = line.map(str::to_owned);
        let expected_block = block.map(|(open, close)| (open.to_owned(), close.to_owned()));

        if actual_line != expected_line {
            wrong.push(format!(
                "{id}: line should be {expected_line:?}, resolved {actual_line:?}"
            ));
        }
        if actual_block != expected_block {
            wrong.push(format!(
                "{id}: block should be {expected_block:?}, resolved {actual_block:?}"
            ));
        }
    }

    // Collected rather than asserted one at a time: a bad derivation rule
    // breaks several languages at once, and seeing all of them is what tells
    // you which rule it was.
    assert!(
        wrong.is_empty(),
        "what these languages comment with has changed:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn the_table_covers_every_language_exactly_once() {
    assert_eq!(
        EXPECTED.len(),
        Language::COUNT,
        "a language was added without saying what it comments with"
    );
    for &language in Language::all() {
        assert_eq!(
            EXPECTED
                .iter()
                .filter(|(id, ..)| *id == language.id())
                .count(),
            1,
            "{} appears in the table other than exactly once",
            language.id()
        );
    }
}

/// The four whose block pair exists only through the `documentation_comment`
/// fallback, named so a regression says which rule broke.
///
/// Rust, Go, C and C++ carry no `block_comment` key at all. The bulk test above
/// would catch this, but it would report four anonymous rows.
#[test]
fn the_four_languages_with_no_block_comment_key_still_have_their_pair() {
    for id in ["rust", "go", "c", "cpp"] {
        assert_eq!(
            resolved(id).block,
            Some(("/*".to_owned(), "*/".to_owned())),
            "{id} has no block_comment key; its pair comes from \
             documentation_comment, and losing that fallback loses block-comment \
             toggling for it"
        );
    }
}

/// And the ones that must not acquire a pair they do not have.
///
/// A fallback rule that reached one key too far would give Python a `/* */`,
/// and block toggling would start writing syntax errors into working files.
#[test]
fn the_languages_with_no_block_comment_do_not_gain_one() {
    for id in ["python", "yaml", "bash", "json"] {
        assert_eq!(resolved(id).block, None, "{id} gained a block comment");
    }
}

/// JSX's delimiters must not escape `[overrides.element]`.
///
/// `javascript` and `tsx` restate both comment keys under that table with JSX's
/// values. A reader that took the last occurrence of a key would comment every
/// line of a `.js` file with `{/*` — valid only inside JSX.
#[test]
fn jsx_delimiters_do_not_leak_into_the_language_itself() {
    for id in ["javascript", "tsx"] {
        let CommentSyntax { line, block } = resolved(id);
        assert_eq!(line.as_deref(), Some("//"), "{id}");
        assert_eq!(
            block,
            Some(("/*".to_owned(), "*/".to_owned())),
            "{id} took JSX's block delimiters"
        );
    }
}

/// The row #66 changed, stated on its own so the change is impossible to miss.
#[test]
fn json_gained_a_line_comment_and_that_was_the_point() {
    let json = resolved("json");
    assert_eq!(
        (json.line, json.block),
        (Some("//".to_owned()), None),
        "JSON's comment syntax now comes from its manifest; if this is ever \
         reverted it must be reverted deliberately, not by a refresh"
    );
}

/// An id no language claims still falls through to the configuration.
///
/// Worth pinning because that branch lost its only in-tree exercise when JSON
/// gained a token: before #66, JSON was the commentless language every fallback
/// test used.
#[test]
fn an_unknown_language_still_falls_back_to_the_configured_token() {
    let config = EditorConfig {
        line_comment_token: Some("%%".to_owned()),
        ..EditorConfig::default()
    };
    let syntax = resolve_comment_syntax(&document_in("nonesuch"), &config)
        .expect("the configured token is available");
    assert_eq!(syntax.line.as_deref(), Some("%%"));
    assert_eq!(syntax.block, None);
}

#[test]
fn an_unknown_language_with_no_configured_token_has_no_comment_syntax() {
    let config = EditorConfig {
        line_comment_token: None,
        ..EditorConfig::default()
    };
    assert!(resolve_comment_syntax(&document_in("nonesuch"), &config).is_none());
}
