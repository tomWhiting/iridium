//! Tests for path-aware matching.

use super::{PathField, Query, match_path};

/// The score `query` gets against `path`, or `None` for no match.
fn score(query: &str, path: &str) -> Option<i32> {
    match_path(&Query::new(query), path).map(|found| found.score)
}

/// Which field won for `query` against `path`.
fn field(query: &str, path: &str) -> Option<PathField> {
    match_path(&Query::new(query), path).map(|found| found.field)
}

/// The basename positions `query` highlights in `path`.
fn positions(query: &str, path: &str) -> Vec<u32> {
    match_path(&Query::new(query), path)
        .map_or_else(Vec::new, |found| found.matched_in_name().to_vec())
}

#[test]
fn a_query_that_names_the_file_beats_one_that_only_brushes_the_folders() {
    // Both paths contain `panel` as a subsequence, and in the second it is
    // spread across the directory chain — `p` and `a` from `packages`, `n`
    // from `analysis`, `e` and `l` from `element`. The whole point of
    // weighting the basename is that the first one wins.
    let named = score("panel", "src/file_tree/panel.rs").expect("the name matched");
    let chained = score("panel", "packages/analysis/element.rs").expect("the path matched");
    assert!(
        named > chained,
        "a basename hit ({named}) must outrank a hit spread through the chain ({chained})"
    );
    assert_eq!(
        field("panel", "src/file_tree/panel.rs"),
        Some(PathField::Name)
    );
}

#[test]
fn typing_part_of_a_directory_still_finds_what_is_inside_it() {
    // The reason the path is scored at all. `tree/pan` names no basename, and
    // a matcher that only looked at basenames would find nothing.
    assert_eq!(
        field("tree/pan", "src/file_tree/panel.rs"),
        Some(PathField::Path),
        "a query spanning a separator can only match the whole path"
    );
    assert!(score("tree/pan", "src/file_tree/panel.rs").is_some());
}

#[test]
fn the_highlighted_characters_are_the_ones_inside_the_name() {
    // A path-field win whose characters straddle the separator highlights only
    // the half that is actually drawn. `panel.rs` starts at character 14 of
    // `src/file_tree/panel.rs`, so `tree/pan` contributes `p`, `a`, `n` at 0,
    // 1, 2 — and nothing for the `tree/` half.
    assert_eq!(
        positions("tree/pan", "src/file_tree/panel.rs"),
        vec![0, 1, 2]
    );
}

#[test]
fn a_match_entirely_in_the_directory_chain_highlights_nothing() {
    // Not a failure — the honest answer. A row that underlined something
    // anyway would be claiming the match was somewhere it was not.
    let found = match_path(&Query::new("src"), "src/panel.rs").expect("the path matched");
    assert_eq!(found.field, PathField::Path);
    assert!(
        found.matched_in_name().is_empty(),
        "nothing in `panel.rs` matched `src`, so nothing in it is underlined"
    );
}

#[test]
fn a_name_match_reports_positions_into_the_name_not_the_path() {
    // The trap this function exists to close: the matcher's positions are
    // relative to whichever string it scored, and the caller draws only the
    // basename. Positions must never be path-relative.
    assert_eq!(positions("pan", "a/very/deep/panel.rs"), vec![0, 1, 2]);
}

#[test]
fn a_path_that_ends_in_a_separator_has_an_empty_name() {
    // A directory row. The name cannot match, but the chain still can, and
    // neither case may panic on the empty slice.
    assert_eq!(field("src", "src/"), Some(PathField::Path));
    assert!(positions("src", "src/").is_empty());
}

#[test]
fn a_windows_separator_divides_a_path_the_same_way() {
    assert_eq!(positions("pan", r"src\file_tree\panel.rs"), vec![0, 1, 2]);
    assert_eq!(
        field("pan", r"src\file_tree\panel.rs"),
        Some(PathField::Name)
    );
}

#[test]
fn a_non_ascii_name_is_indexed_by_character_and_not_by_byte() {
    // `é` is two bytes. A byte-indexed implementation would underline the
    // middle of it, which the shaper cannot draw.
    assert_eq!(positions("ré", "src/réponse.txt"), vec![0, 1]);
}

#[test]
fn an_empty_query_matches_nothing_because_everything_is_the_callers_decision() {
    assert!(score("", "src/panel.rs").is_none());
    assert!(score("   ", "src/panel.rs").is_none());
}

#[test]
fn a_query_that_is_not_a_subsequence_of_either_field_does_not_match() {
    assert!(score("zzz", "src/panel.rs").is_none());
}

#[test]
fn an_empty_path_matches_nothing_and_does_not_panic() {
    assert!(score("a", "").is_none());
}

#[test]
fn a_shorter_name_wins_a_tie_against_a_longer_one() {
    // The property that makes a file list feel right: `main.rs` before
    // `maintenance_report.rs` for `main`.
    let short = score("main", "src/main.rs").expect("the name matched");
    let long = score("main", "src/maintenance_report.rs").expect("the name matched");
    assert!(
        short > long,
        "the shorter name ({short}) must outrank the longer one ({long})"
    );
}

#[test]
fn every_matched_position_is_inside_the_name_it_indexes() {
    // A property, over every prefix of a realistic path set: a position past
    // the end of the name would index a character that is not drawn, and the
    // face maps positions to grapheme clusters by indexing.
    const PATHS: &[&str] = &[
        "src/file_tree/panel.rs",
        "crates/iridium-editor/src/fuzzy/path.rs",
        "README.md",
        "a/b/c/d/e/f.txt",
        r"win\style\path.rs",
        "src/réponse.txt",
        "trailing/",
    ];
    const QUERIES: &[&str] = &[
        "p", "pa", "pan", "path", "src", "tree/pan", "rs", "f", "e/f", "md", "ré",
    ];

    for path in PATHS {
        let name_chars = path
            .rsplit_once(['/', '\\'])
            .map_or(*path, |(_, name)| name)
            .chars()
            .count();
        for query in QUERIES {
            let Some(found) = match_path(&Query::new(query), path) else {
                continue;
            };
            for &position in found.matched_in_name() {
                assert!(
                    (position as usize) < name_chars,
                    "`{query}` against `{path}` reported position {position} in a name of \
                     {name_chars} characters"
                );
            }
        }
    }
}

#[test]
fn positions_are_ascending_and_never_repeat() {
    // The face merges adjacent same-coloured clusters as it walks the name
    // once, in order. Out-of-order positions would silently drop highlights.
    for query in ["pan", "tree/pan", "src", "p"] {
        let Some(found) = match_path(&Query::new(query), "src/file_tree/panel.rs") else {
            continue;
        };
        let matched = found.matched_in_name();
        for pair in matched.windows(2) {
            assert!(
                pair[0] < pair[1],
                "`{query}` reported {matched:?}, which is not strictly ascending"
            );
        }
    }
}
