//! Tests for the manifest reader.
//!
//! Two jobs. The first is that the twenty-one vendored files parse and that the
//! table naming them matches the tree on disk — without that, a vendor refresh
//! could add or drop a language silently. The second is the *derivation* rules:
//! which comment token wins, which block pair wins, and that the `[overrides.*]`
//! tables further down several manifests do not leak into the top-level fields.
//! Those rules are where a plausible-looking reader would be quietly wrong.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use super::{all, by_id, parse_failures};
use crate::Language;

/// The vendored tree on disk.
fn queries_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/languages/queries")
}

#[test]
fn every_vendored_manifest_parses() {
    assert!(
        parse_failures().is_empty(),
        "{} vendored manifest(s) did not parse:\n{}",
        parse_failures().len(),
        parse_failures().join("\n")
    );
}

#[test]
fn the_table_names_exactly_the_directories_that_carry_a_manifest() {
    let on_disk: BTreeSet<String> = fs::read_dir(queries_root())
        .expect("the vendored query tree must be readable")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("config.toml").is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();

    let in_table: BTreeSet<String> = all()
        .iter()
        .map(|manifest| manifest.id().to_owned())
        .collect();

    assert_eq!(
        in_table, on_disk,
        "the manifest table and the vendored tree disagree; a refresh added or \
         dropped a language and the table has to say so"
    );
}

#[test]
fn no_two_manifests_claim_the_same_id() {
    let ids: BTreeSet<&str> = all().iter().map(super::Manifest::id).collect();
    assert_eq!(
        ids.len(),
        all().len(),
        "two manifests share an id, so by_id would shadow one with the other"
    );
}

#[test]
fn every_language_has_a_manifest_and_it_is_the_one_named_by_its_id() {
    for &language in Language::all() {
        let manifest = language
            .manifest()
            .unwrap_or_else(|| panic!("{} has no vendored manifest", language.id()));
        assert_eq!(manifest.id(), language.id());
    }
}

#[test]
fn by_id_says_no_to_a_language_that_was_never_vendored() {
    assert!(by_id("nonesuch").is_none());
    assert!(by_id("").is_none());
    assert!(
        by_id("RUST").is_none(),
        "ids are matched exactly, not folded"
    );
}

/// The three that exist only to be injected into another language.
///
/// Asserted as an exact set rather than one-by-one: a refresh that marks a
/// fourth language hidden is a language quietly leaving the list a document can
/// be set to, and that should be a decision someone made.
#[test]
fn exactly_three_manifests_are_hidden() {
    let hidden: BTreeSet<&str> = all()
        .iter()
        .filter(|manifest| manifest.is_hidden())
        .map(super::Manifest::id)
        .collect();

    let expected: BTreeSet<&str> = ["jsdoc", "markdown-inline", "regex"].into_iter().collect();
    assert_eq!(hidden, expected);
}

#[test]
fn no_language_iridium_offers_is_a_hidden_one() {
    for &language in Language::all() {
        let manifest = language.manifest().expect("every language has a manifest");
        assert!(
            !manifest.is_hidden(),
            "{} is offered as a language a document can be, but its manifest \
             marks it injection-only",
            language.id()
        );
    }
}

/// The trailing space the manifests write is a Zed insertion convention, not
/// part of the token.
#[test]
fn the_line_comment_token_loses_the_trailing_space() {
    assert_eq!(
        by_id("rust").and_then(super::Manifest::line_comment),
        Some("//")
    );
    assert_eq!(
        by_id("python").and_then(super::Manifest::line_comment),
        Some("#")
    );
}

/// Two manifests write theirs with no trailing space at all, which is why
/// trimming is the rule rather than a fixed one-character strip.
#[test]
fn a_token_written_without_a_trailing_space_survives_unchanged() {
    assert_eq!(
        by_id("gomod").and_then(super::Manifest::line_comment),
        Some("//")
    );
    assert_eq!(
        by_id("gitcommit").and_then(super::Manifest::line_comment),
        Some("#")
    );
}

/// Rust lists `// `, `/// ` and `//! `. Toggling a comment must write the first.
#[test]
fn the_first_token_wins_when_a_language_lists_several() {
    let rust = by_id("rust").expect("rust is vendored");
    assert_eq!(rust.line_comment(), Some("//"));
    assert_ne!(rust.line_comment(), Some("///"));
}

#[test]
fn a_language_with_no_line_comment_reports_none() {
    // CSS genuinely has no line comment, and Markdown none either.
    assert_eq!(by_id("css").and_then(super::Manifest::line_comment), None);
    assert_eq!(
        by_id("markdown").and_then(super::Manifest::line_comment),
        None
    );
}

/// The fallback that four languages depend on entirely.
///
/// Rust, Go, C and C++ carry no `block_comment` key. Without falling back to
/// `documentation_comment` they would report no block comment at all, and
/// block-comment toggling would silently stop working in the four languages
/// most likely to want it.
#[test]
fn the_block_pair_falls_back_to_the_documentation_comment() {
    for id in ["rust", "go", "c", "cpp"] {
        let manifest = by_id(id).expect("vendored");
        assert_eq!(
            manifest.block_comment(),
            Some(("/*", "*/")),
            "{id} lost its block comment to the missing block_comment key"
        );
    }
}

/// And the precedence in the other direction.
///
/// TypeScript, JavaScript and TSX carry both keys, and their
/// `documentation_comment` opens `/**`. Preferring it would make a plain block
/// toggle write a doc comment.
#[test]
fn block_comment_wins_over_documentation_comment_where_both_exist() {
    for id in ["typescript", "javascript", "tsx"] {
        let manifest = by_id(id).expect("vendored");
        assert_eq!(
            manifest.block_comment(),
            Some(("/*", "*/")),
            "{id} took its documentation_comment, which opens /**"
        );
    }
}

#[test]
fn markdown_keeps_its_own_block_pair() {
    assert_eq!(
        by_id("markdown").and_then(super::Manifest::block_comment),
        Some(("<!--", "-->"))
    );
}

#[test]
fn a_language_with_neither_key_reports_no_block_comment() {
    for id in ["python", "yaml", "bash", "json"] {
        assert_eq!(
            by_id(id).and_then(super::Manifest::block_comment),
            None,
            "{id} reported a block comment it does not have"
        );
    }
}

/// The trap this reader has to avoid.
///
/// `javascript` and `tsx` restate `line_comments` and `block_comment` inside
/// `[overrides.element]` — JSX elements comment as `{/* … */}`, not `/* … */`.
/// A reader that flattened the file, or that took the *last* occurrence of a
/// key, would hand JSX's delimiters to every toggle in the language. The
/// top-level values are the ones that must come through.
#[test]
fn an_override_table_does_not_reach_the_top_level_fields() {
    for id in ["javascript", "tsx"] {
        let manifest = by_id(id).expect("vendored");
        assert_eq!(
            manifest.line_comment(),
            Some("//"),
            "{id} took its line comment from [overrides.element]"
        );
        assert_eq!(
            manifest.block_comment(),
            Some(("/*", "*/")),
            "{id} took its block comment from [overrides.element], so a toggle \
             would write JSX braces outside JSX"
        );
    }
}

/// Keys nobody here has heard of must not stop a manifest parsing.
///
/// Every vendored file already carries several — `debuggers`,
/// `prettier_parser_name`, `scope_opt_in_language_servers` — so the fact that
/// they all parse is most of this. The synthetic case pins the behaviour
/// against a future `deny_unknown_fields`.
#[test]
fn an_unknown_key_is_ignored_rather_than_rejected() {
    let manifest = super::Manifest::parse(
        "invented",
        r#"
        name = "Invented"
        grammar = "invented"
        path_suffixes = ["inv"]
        a_key_from_a_future_refresh = { with = "a shape nobody expected" }
        "#,
    )
    .expect("an unknown key must not fail the parse");

    assert_eq!(manifest.name(), "Invented");
    assert_eq!(manifest.path_suffixes(), ["inv"]);
}

#[test]
fn a_malformed_manifest_names_itself_in_the_error() {
    let error =
        super::Manifest::parse("broken", "name = ").expect_err("truncated TOML must not parse");
    assert!(
        error.contains("broken/config.toml"),
        "the error must name the file: {error}"
    );
}

/// `path_suffixes` is not only suffixes, and a reader that assumed otherwise
/// would drop the whole-file-name entries on the floor.
#[test]
fn path_suffixes_carries_whole_file_names_too() {
    let json = by_id("json").expect("vendored");
    assert!(json.path_suffixes().iter().any(|s| s == "flake.lock"));

    let jsonc = by_id("jsonc").expect("vendored");
    assert!(jsonc.path_suffixes().iter().any(|s| s == "tsconfig.json"));
}

/// The grammar is not always the language's own id.
#[test]
fn a_manifest_can_name_a_grammar_that_is_not_its_own_id() {
    let javascript = by_id("javascript").expect("vendored");
    assert_eq!(javascript.id(), "javascript");
    assert_eq!(
        javascript.grammar(),
        "tsx",
        "Zed parses JavaScript with the TSX grammar; Iridium does not, and \
         whichever wins, the two must not be conflated"
    );
}

#[test]
fn the_display_name_is_not_the_id() {
    assert_eq!(
        by_id("bash").map(super::Manifest::name),
        Some("Shell Script")
    );
    assert_eq!(by_id("cpp").map(super::Manifest::name), Some("C++"));
}

// ========== brackets ==========

/// The three filters `auto_close_pairs` applies, each on a manifest that
/// actually exercises it.
#[test]
fn auto_close_pairs_reads_only_the_rows_it_can_honour() {
    let rust = by_id("rust").expect("vendored");
    let pairs: Vec<_> = rust
        .auto_close_pairs()
        .expect("rust declares brackets")
        .collect();

    // `<` is declared `close = false` — a matching rule, not a typing one.
    // `r#"`, `r##"`, `r###"` and `/*` are multi-character.
    assert_eq!(pairs, vec![('{', '}'), ('[', ']'), ('(', ')'), ('"', '"')]);
    assert!(
        !pairs.iter().any(|&(open, _)| open == '\''),
        "Rust declares no single quote — a lifetime is not a character literal"
    );
}

/// `close` absent means `close = true`, which seventeen rows across the
/// vendored tree rely on. Reading it as `false` would silently disable the
/// most common pair in the estate.
#[test]
fn a_bracket_row_with_no_close_key_still_auto_closes() {
    let c = by_id("c").expect("vendored");
    assert!(
        c.auto_close_pairs()
            .expect("c declares brackets")
            .any(|pair| pair == ('{', '}')),
        "`{{` is declared with no `close` key in every C-like manifest"
    );
}

/// An absent `brackets` key and an empty one are different answers, and both
/// occur in the vendored tree.
#[test]
fn no_brackets_key_and_an_empty_one_are_told_apart() {
    let awl = by_id("awl").expect("vendored");
    assert!(
        awl.auto_close_pairs().is_none(),
        "awl declares no brackets key at all — it has not said"
    );

    let diff = by_id("diff").expect("vendored");
    assert_eq!(
        diff.auto_close_pairs()
            .expect("diff declares an empty brackets table")
            .count(),
        0,
        "diff has said: pair nothing"
    );
}

/// Exactly two vendored manifests carry no `brackets` key, and they are named
/// rather than counted — a count would go stale silently on a vendor refresh,
/// and *which* language has not said is the fact a caller depends on.
#[test]
fn only_awl_and_markdown_inline_declare_no_brackets_key() {
    let silent: Vec<&str> = super::all()
        .iter()
        .filter(|manifest| manifest.auto_close_pairs().is_none())
        .map(super::Manifest::id)
        .collect();
    assert_eq!(silent, vec!["awl", "markdown-inline"]);
}
