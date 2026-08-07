//! Tests that the embedded query table agrees with the tree it was built from.
//!
//! These check the *table against the files on disk* and need no grammar, so
//! they belong beside the vendored tree rather than beside the compiler. The
//! companion test that every query actually compiles lives in `iridium-syntax`,
//! which is the only crate that can run it.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::{QueryKind, source};
use crate::Language;

/// The absolute path a language's query file would occupy on disk.
fn vendored_path(language: Language, kind: QueryKind) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/languages/queries")
        .join(language.id())
        .join(kind.file_name())
}

/// Every `(language, kind)` pair with no vendored file.
///
/// Spelled out rather than derived so that a vendor refresh which adds or drops
/// a file has to be acknowledged here, in a diff a reviewer can see.
///
/// The three upstream languages each miss exactly one kind. AWL misses four,
/// because it is not an upstream language: its queries are written by hand in
/// the aion repository and cover what AWL actually needs. A language shipping
/// only some kinds is the normal case for a first-party grammar, not a gap
/// waiting to be filled — the loader models every kind as optional, and the
/// features keying off the missing four degrade to nothing rather than fail.
const KNOWN_ABSENCES: &[(Language, QueryKind)] = &[
    (Language::Json, QueryKind::Injections),
    (Language::Yaml, QueryKind::Indents),
    (Language::Bash, QueryKind::Outline),
    (Language::Awl, QueryKind::Brackets),
    (Language::Awl, QueryKind::TextObjects),
    (Language::Awl, QueryKind::Injections),
    (Language::Awl, QueryKind::Outline),
];

/// What specific languages ship, written out by hand.
///
/// This is the check that survives the table being generated. Every other test
/// here compares the table against the directory, and once `build.rs` builds
/// the table *from* that directory both sides read the same source — such a
/// comparison can catch a mangled table but never a file that quietly went
/// away in a vendor refresh.
///
/// So these rows are asserted, not derived. They are a claim about what
/// Iridium supports, and if a refresh makes one false the right outcome is a
/// failing test naming the language, not a feature that silently stops.
const PINNED: &[(Language, &[QueryKind])] = &[
    // The reference language: everything present.
    (
        Language::Rust,
        &[
            QueryKind::Highlights,
            QueryKind::Brackets,
            QueryKind::TextObjects,
            QueryKind::Indents,
            QueryKind::Injections,
            QueryKind::Outline,
        ],
    ),
    // The languages carrying a documented absence, pinned from the other side:
    // what they *do* ship, so a refresh that dropped a second file is caught
    // by more than the absence list alone.
    //
    // AWL matters most here. Its four absences mean the absence list alone
    // would still pass if its highlights.scm vanished and its brackets.scm
    // appeared — the count would balance. This says which two it ships.
    (Language::Awl, &[QueryKind::Highlights, QueryKind::Indents]),
    (
        Language::Json,
        &[
            QueryKind::Highlights,
            QueryKind::Brackets,
            QueryKind::TextObjects,
            QueryKind::Indents,
            QueryKind::Outline,
        ],
    ),
    (
        Language::Yaml,
        &[
            QueryKind::Highlights,
            QueryKind::Brackets,
            QueryKind::TextObjects,
            QueryKind::Injections,
            QueryKind::Outline,
        ],
    ),
    (
        Language::Bash,
        &[
            QueryKind::Highlights,
            QueryKind::Brackets,
            QueryKind::TextObjects,
            QueryKind::Indents,
            QueryKind::Injections,
        ],
    ),
];

#[test]
fn the_pinned_languages_ship_exactly_what_they_are_claimed_to() {
    for &(language, kinds) in PINNED {
        for &kind in QueryKind::all() {
            let expected = kinds.contains(&kind);
            assert_eq!(
                source(language, kind).is_some(),
                expected,
                "{} should {} ship {kind}",
                language.id(),
                if expected { "" } else { "not" }
            );
        }
    }
}

#[test]
fn a_pinned_query_carries_real_text_rather_than_an_empty_file() {
    // The generated table would happily carry an empty string, and every
    // presence check above would still pass. Highlights is the one kind
    // nothing degrades gracefully without.
    for &(language, _) in PINNED {
        let highlights = source(language, QueryKind::Highlights)
            .unwrap_or_else(|| panic!("{} ships no highlights.scm", language.id()));
        assert!(
            highlights.contains('('),
            "{}'s highlights.scm carries no s-expression, so it is not a query",
            language.id()
        );
    }
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
fn the_only_absences_are_the_ones_written_down() {
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
    let pairings = Language::COUNT * QueryKind::COUNT;
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
