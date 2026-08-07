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
