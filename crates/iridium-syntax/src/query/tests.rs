//! Tests for the embedded query table and the compile-once cache.
//!
//! The load-bearing one is [`every_embedded_query_compiles_against_its_grammar`]:
//! a vendored `.scm` is only checked when something runs it, so without this
//! test a grammar bump that invalidates a pattern would ship, and would surface
//! as a language that silently stops highlighting.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "assertions in tests read better than error plumbing"
)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use tree_sitter::Query;

use super::{COMPILED, KIND_COUNT, LANGUAGE_COUNT, QueryKind, compiled, source};
use crate::Language;

/// The absolute path a language's query file would occupy on disk.
fn vendored_path(language: Language, kind: QueryKind) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/languages/queries")
        .join(language.id())
        .join(kind.file_name())
}

/// The three `(language, kind)` pairs with no vendored file.
///
/// Spelled out rather than derived so that a vendor refresh which adds or drops
/// a file has to be acknowledged here, in a diff a reviewer can see.
const KNOWN_ABSENCES: &[(Language, QueryKind)] = &[
    (Language::Json, QueryKind::Injections),
    (Language::Yaml, QueryKind::Indents),
    (Language::Bash, QueryKind::Outline),
];

#[test]
fn every_embedded_query_compiles_against_its_grammar() {
    // Every failure is collected before reporting: a grammar bump usually
    // invalidates several files at once, and stopping at the first would turn
    // one fix into as many edit-run cycles as there are broken queries.
    let mut failures = Vec::new();

    for &language in Language::all() {
        for &kind in QueryKind::all() {
            match compiled(language, kind) {
                Ok(Some(query)) => assert!(
                    query.pattern_count() > 0,
                    "{}/{kind} compiled to no patterns at all",
                    language.id()
                ),
                Ok(None) => assert!(
                    source(language, kind).is_none(),
                    "{}/{kind} has source but the cache reported it absent",
                    language.id()
                ),
                Err(error) => failures.push(error.to_string()),
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} vendored quer{} did not compile:\n{}",
        failures.len(),
        if failures.len() == 1 { "y" } else { "ies" },
        failures.join("\n")
    );
}

#[test]
fn the_table_agrees_with_the_files_on_disk() {
    for &language in Language::all() {
        for &kind in QueryKind::all() {
            let path = vendored_path(language, kind);
            assert_eq!(
                source(language, kind).is_some(),
                path.exists(),
                "the table and the tree disagree about {}/{kind}: {} on disk",
                language.id(),
                if path.exists() { "present" } else { "missing" }
            );
        }
    }
}

#[test]
fn the_only_absences_are_the_three_that_were_never_vendored() {
    let absent: BTreeSet<(&str, QueryKind)> = Language::all()
        .iter()
        .flat_map(|&language| {
            QueryKind::all()
                .iter()
                .map(move |&kind| (language, kind))
                .filter(|&(language, kind)| source(language, kind).is_none())
                .map(|(language, kind)| (language.id(), kind))
        })
        .collect();

    let expected: BTreeSet<(&str, QueryKind)> = KNOWN_ABSENCES
        .iter()
        .map(|&(language, kind)| (language.id(), kind))
        .collect();

    assert_eq!(
        absent, expected,
        "the set of languages missing a query kind changed; \
         update KNOWN_ABSENCES and the loader's documentation together"
    );
}

#[test]
fn the_table_carries_a_query_for_every_other_pairing() {
    let pairings = LANGUAGE_COUNT * KIND_COUNT;
    let present = Language::all()
        .iter()
        .flat_map(|&language| {
            QueryKind::all()
                .iter()
                .filter(move |&&kind| source(language, kind).is_some())
        })
        .count();

    assert_eq!(
        present,
        pairings - KNOWN_ABSENCES.len(),
        "expected {pairings} pairings less {} absences",
        KNOWN_ABSENCES.len()
    );
}

#[test]
fn every_language_ships_the_highlights_query_the_highlighter_requires() {
    for &language in Language::all() {
        assert!(
            compiled(language, QueryKind::Highlights)
                .expect("highlights must compile")
                .is_some(),
            "{} has no highlights query, so Highlighter::new would fail for it",
            language.id()
        );
    }
}

#[test]
fn a_query_is_compiled_once_and_then_shared() {
    let first = compiled(Language::Rust, QueryKind::TextObjects)
        .expect("rust textobjects must compile")
        .expect("rust ships textobjects");
    let second = compiled(Language::Rust, QueryKind::TextObjects)
        .expect("rust textobjects must compile")
        .expect("rust ships textobjects");

    assert!(
        std::ptr::eq(first, second),
        "the cache handed out two different queries, so it is recompiling"
    );
}

#[test]
fn each_pairing_occupies_its_own_cache_slot() {
    // Two pairings resolving to one slot would serve a language another
    // language's query — the failure this indexing scheme has to rule out.
    let mut seen: BTreeSet<*const Query> = BTreeSet::new();

    for &language in Language::all() {
        for &kind in QueryKind::all() {
            if let Some(query) = compiled(language, kind).expect("every query must compile") {
                assert!(
                    seen.insert(std::ptr::from_ref(query)),
                    "{}/{kind} shares a compiled query with another pairing",
                    language.id()
                );
            }
        }
    }
}

#[test]
fn the_cache_is_shaped_like_the_two_enums_it_indexes() {
    assert_eq!(COMPILED.len(), Language::all().len());
    for row in &COMPILED {
        assert_eq!(row.len(), QueryKind::all().len());
    }
}

#[test]
fn the_vendored_text_objects_carry_exactly_the_five_documented_captures() {
    // Structural selection and text objects are built on these, and only
    // these. If a vendor refresh adds `@parameter.inside` or `@comment.inside`
    // this fails — which is the point: it means richer text objects became
    // available without anyone noticing.
    let mut captures: BTreeSet<&str> = BTreeSet::new();
    for &language in Language::all() {
        let query = compiled(language, QueryKind::TextObjects)
            .expect("textobjects must compile")
            .expect("every language ships textobjects");
        captures.extend(query.capture_names().iter().copied());
    }

    let expected: BTreeSet<&str> = [
        "class.around",
        "class.inside",
        "comment.around",
        "function.around",
        "function.inside",
    ]
    .into_iter()
    .collect();

    assert_eq!(
        captures, expected,
        "the text-object captures the vendored queries offer have changed"
    );
}
