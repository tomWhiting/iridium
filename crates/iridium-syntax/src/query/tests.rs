//! Tests for the compile-once query cache.
//!
//! The load-bearing one is [`every_embedded_query_compiles_against_its_grammar`]:
//! a vendored `.scm` is only checked when something runs it, so without this
//! test a grammar bump that invalidates a pattern would ship, and would surface
//! as a language that silently stops highlighting.
//!
//! The table those queries come out of is checked against the files on disk in
//! `iridium-lang`, which owns them. Compiling needs a grammar, so it can only
//! be checked here.

use std::collections::BTreeSet;

use tree_sitter::Query;

use super::{COMPILED, QueryKind, compiled, source};
use crate::Language;

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
    // A language that ships no `textobjects.scm` is skipped rather than
    // expected. This used to `.expect("every language ships textobjects")`,
    // which was true only by accident of which languages were vendored: AWL
    // ships highlights, indents and folds and no text objects at all. Text
    // objects are a feature a language may simply not have, and the loader
    // already models that as a routine `Ok(None)`.
    //
    // What is deliberately NOT weakened is the claim about languages that do
    // ship one — every capture in every such file still has to be one of the
    // five below.
    let mut captures: BTreeSet<&str> = BTreeSet::new();
    let mut shipped = 0_usize;
    for &language in Language::all() {
        let Some(query) = compiled(language, QueryKind::TextObjects)
            .expect("a vendored textobjects.scm must compile against its grammar")
        else {
            continue;
        };
        shipped += 1;
        captures.extend(query.capture_names().iter().copied());
    }

    // Guards the skip above: if a refresh left *no* language shipping text
    // objects, every capture check below would pass vacuously and structural
    // selection would be silently dead.
    assert!(
        shipped > 0,
        "no language ships a textobjects.scm, so this test proves nothing"
    );

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
