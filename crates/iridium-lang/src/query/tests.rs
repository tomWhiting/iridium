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

/// Every query kind, for the ten languages that ship all of them.
const EVERYTHING: &[QueryKind] = &[
    QueryKind::Highlights,
    QueryKind::Brackets,
    QueryKind::TextObjects,
    QueryKind::Indents,
    QueryKind::Injections,
    QueryKind::Outline,
];

/// What every language ships, written out by hand — the one table.
///
/// This replaced a pair: a list of known absences and a separate list pinning
/// what a few languages positively shipped. Two tables describing one fact is
/// one table too many, and the split had a hole in it — a language could lose
/// one file and gain another and the absence *count* would still balance.
///
/// **These rows are asserted, not derived, and that is the whole point.**
/// Every other test here compares the generated table against the directory it
/// was generated from, so both sides read the same source: such a comparison
/// catches a mangled table but never a file that quietly went away in a vendor
/// refresh. This is the third opinion. If a refresh makes a row false, the
/// right outcome is a failing test naming the language and the kind, not a
/// feature that silently stops.
///
/// A language shipping only some kinds is routine rather than a gap waiting to
/// be filled — the loader models every kind as optional, and each feature
/// degrades to nothing for a language that ships no query for it.
const SHIPS: &[(Language, &[QueryKind])] = &[
    // Hand-written queries from the aion repository, covering what AWL needs.
    (Language::Awl, &[QueryKind::Highlights, QueryKind::Indents]),
    // The four listed with no grammar linked. Each ships highlights it cannot
    // yet run and an injections query nothing consumes; what makes them worth
    // listing is the manifest beside them, not these.
    (
        Language::Diff,
        &[QueryKind::Highlights, QueryKind::Injections],
    ),
    (
        Language::GitCommit,
        &[QueryKind::Highlights, QueryKind::Injections],
    ),
    (
        Language::GoMod,
        &[QueryKind::Highlights, QueryKind::Injections],
    ),
    (
        Language::GoWork,
        &[QueryKind::Highlights, QueryKind::Injections],
    ),
    // Upstream languages, each missing exactly one kind.
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
    // Everything present.
    (Language::C, EVERYTHING),
    (Language::Cpp, EVERYTHING),
    (Language::Css, EVERYTHING),
    (Language::Go, EVERYTHING),
    (Language::JavaScript, EVERYTHING),
    (Language::Markdown, EVERYTHING),
    (Language::Python, EVERYTHING),
    (Language::Rust, EVERYTHING),
    (Language::Tsx, EVERYTHING),
    (Language::TypeScript, EVERYTHING),
];

#[test]
fn every_language_ships_exactly_what_the_table_says() {
    // Collected rather than asserted one at a time: a vendor refresh usually
    // moves several files at once, and seeing all of them is what tells you
    // whether a language changed or the refresh did.
    let mut wrong = Vec::new();

    for &(language, kinds) in SHIPS {
        for &kind in QueryKind::all() {
            let expected = kinds.contains(&kind);
            if source(language, kind).is_some() != expected {
                wrong.push(format!(
                    "{} should {}ship {kind}",
                    language.id(),
                    if expected { "" } else { "not " }
                ));
            }
        }
    }

    assert!(
        wrong.is_empty(),
        "the vendored queries no longer match what SHIPS claims:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn the_table_names_every_language_exactly_once() {
    // Without this, a language added to `languages.txt` and forgotten here
    // would ship whatever the directory happened to hold, unchecked.
    assert_eq!(
        SHIPS.len(),
        Language::COUNT,
        "a language was added without saying which queries it ships"
    );
    for &language in Language::all() {
        assert_eq!(
            SHIPS.iter().filter(|(l, _)| *l == language).count(),
            1,
            "{} appears in SHIPS other than exactly once",
            language.id()
        );
    }
}

#[test]
fn a_shipped_query_carries_real_text_rather_than_an_empty_file() {
    // The generated table would happily carry an empty string, and every
    // presence check above would still pass. Highlights is the one kind
    // nothing degrades gracefully without, and every language ships one.
    for &(language, _) in SHIPS {
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
fn the_absences_are_exactly_the_ones_the_table_accounts_for() {
    // The other direction from `every_language_ships_exactly_what_the_table_
    // says`, and not redundant with it: that test walks SHIPS, so a language
    // missing from SHIPS entirely is invisible to it. This walks the registry.
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

    let expected: BTreeSet<(&str, QueryKind)> = SHIPS
        .iter()
        .flat_map(|&(language, kinds)| {
            QueryKind::all()
                .iter()
                .filter(move |kind| !kinds.contains(kind))
                .map(move |&kind| (language.id(), kind))
        })
        .collect();

    assert_eq!(
        absent, expected,
        "the set of languages missing a query kind changed; \
         update SHIPS and the loader's documentation together"
    );
}

#[test]
fn the_table_carries_a_query_for_every_pairing_it_claims() {
    let pairings = Language::COUNT * QueryKind::COUNT;
    let present = Language::all()
        .iter()
        .flat_map(|&language| {
            QueryKind::all()
                .iter()
                .filter(move |&&kind| source(language, kind).is_some())
        })
        .count();
    let claimed: usize = SHIPS.iter().map(|(_, kinds)| kinds.len()).sum();

    assert_eq!(
        present, claimed,
        "the table carries {present} of {pairings} pairings, but SHIPS claims \
         {claimed}"
    );
}
