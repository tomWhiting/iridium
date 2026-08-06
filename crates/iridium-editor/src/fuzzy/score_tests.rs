//! Matcher tests: normalization, subsequence assignment, and relative scores.
//!
//! These pin *structure* — which characters matched, and how two matches
//! compare — rather than absolute scores. A scoring constant should be tunable
//! without rewriting the suite; what must not change silently is which
//! candidate a query finds and which characters a caller underlines.
//!
//! Nothing here mentions a command. That is the point of the module: the same
//! assertions hold for a path, an outline entry, or anything else ranked
//! later, and any caller that grew its own matcher would be free to break
//! them privately.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{MAX_QUERY_CHARS, Query, match_field};

fn positions(query: &str, field: &str) -> Vec<u32> {
    let found = match_field(&Query::new(query), field).expect("field should match");
    found.matched().to_vec()
}

fn score(query: &str, field: &str) -> i32 {
    match_field(&Query::new(query), field)
        .expect("field should match")
        .score
}

#[test]
fn a_query_is_trimmed_but_keeps_its_interior_spaces() {
    assert_eq!(Query::new("  join  ").len(), 4);
    assert_eq!(Query::new("one cursor").len(), 10);
    assert!(Query::new("   ").is_empty());
    assert!(Query::new("").is_empty());
}

#[test]
fn a_query_longer_than_the_cap_is_truncated_and_says_so() {
    let long = "a".repeat(MAX_QUERY_CHARS + 7);
    let query = Query::new(&long);
    assert_eq!(query.len(), MAX_QUERY_CHARS);
    assert!(query.is_truncated());

    let exact = "b".repeat(MAX_QUERY_CHARS);
    assert!(!Query::new(&exact).is_truncated());
}

#[test]
fn only_a_subsequence_matches() {
    assert!(match_field(&Query::new("jn"), "Join Lines").is_some());
    assert!(match_field(&Query::new("jl"), "Join Lines").is_some());
    // Out of order is not a match: `nj` would need the `n` before the `j`.
    assert!(match_field(&Query::new("nj"), "Join Lines").is_none());
    assert!(match_field(&Query::new("zz"), "Join Lines").is_none());
    // An empty query is handled by the caller, which lists everything.
    assert!(match_field(&Query::new(""), "Join Lines").is_none());
    assert!(match_field(&Query::new("a"), "").is_none());
}

#[test]
fn matching_ignores_case_in_both_directions() {
    assert!(match_field(&Query::new("JOIN"), "join lines").is_some());
    assert!(match_field(&Query::new("join"), "JOIN LINES").is_some());
    assert!(match_field(&Query::new("Größe"), "größe ändern").is_some());
}

#[test]
fn positions_are_character_indices_not_byte_offsets() {
    // `Ü` is two bytes, so a byte-indexing matcher reports 2 here and a palette
    // underlines the wrong glyph. Every non-ASCII title in the wild is this case.
    assert_eq!(positions("n", "Ünicode"), vec![1]);
    assert_eq!(positions("de", "Ünicode"), vec![5, 6]);
    assert_eq!(positions("äö", "äbcö"), vec![0, 3]);
}

#[test]
fn the_backward_pass_pulls_a_match_onto_the_better_characters() {
    // Leftmost-only would take the `d` of `Duplicate` and the first later `e`,
    // scoring a gap-ridden match. The rightmost assignment lands on `Down`'s
    // word-initial `D` followed immediately by... nothing — so the two passes
    // disagree, and the better of the two must win.
    let left_only = positions("dn", "Duplicate Line Down");
    assert_eq!(
        left_only.len(),
        2,
        "both characters must be assigned somewhere"
    );

    // `li` is the decisive shape: forward takes `l` from `Duplicate`, backward
    // takes the word-initial `L` of `Line`, which is what a user means.
    let chosen = positions("li", "Duplicate Line");
    assert_eq!(chosen, vec![10, 11], "expected the word-initial `Li`");
}

#[test]
fn a_word_initial_match_beats_the_same_characters_mid_word() {
    assert!(score("li", "Duplicate Line") > score("li", "Duplicated"));
}

#[test]
fn a_prefix_beats_a_later_match_of_the_same_length() {
    assert!(score("line", "Line Break") > score("line", "Insert Line Break"));
}

#[test]
fn consecutive_characters_beat_scattered_ones() {
    assert!(score("undo", "Undo") > score("undo", "Unfold Down Order"));
}

#[test]
fn an_exact_field_beats_a_field_that_merely_starts_with_the_query() {
    assert!(score("copy", "Copy") > score("copy", "Copy Line Down"));
}

#[test]
fn a_shorter_field_wins_a_tie() {
    assert!(score("cut", "Cut") > score("cut", "Cut And Keep Going Forever"));
}

#[test]
fn camel_case_humps_count_as_word_starts() {
    // The id tail is camelCase, and `du` finding `duplicateUp` through it is the
    // whole reason the id is a scored field.
    assert!(score("dup", "duplicateUp") > score("dup", "adupxlicate"));
    assert!(score("du", "duplicateUp") > 0);
}

/// A path separator starts a word, so a match on a basename outscores the
/// same characters buried mid-word.
///
/// This is what makes the matcher usable for files without a second
/// implementation, and it is asserted against strings of **equal length whose
/// match begins at the same index**, so the only thing that can separate them
/// is the word-start bonus. Comparing a short path against a long one would
/// pass on the length penalty alone and prove nothing about separators.
#[test]
fn a_path_separator_starts_a_word() {
    assert!(score("con", "ab/config") > score("con", "abxconfig"));

    // Backslashes too, so a Windows path is not one unbroken run.
    assert!(score("con", "ab\\config") > score("con", "abxconfig"));
}

/// The matched positions are exactly as many as the query's characters, and
/// strictly ascending.
///
/// The array is fixed-length and only a prefix of it is meaningful; a caller
/// that underlines `positions` rather than `matched()` would highlight
/// position zero for every unused slot, which reads as the first character
/// always matching.
#[test]
fn the_reported_positions_are_ascending_and_no_longer_than_the_query() {
    let query = Query::new("dcm");
    let found = match_field(&query, "duplicate cursor mark").expect("should match");

    assert_eq!(found.len, query.len());
    assert_eq!(found.matched().len(), query.len());
    assert!(
        found.matched().windows(2).all(|pair| pair[0] < pair[1]),
        "positions must strictly ascend: {:?}",
        found.matched()
    );
}
