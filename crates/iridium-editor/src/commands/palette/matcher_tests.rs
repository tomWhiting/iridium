//! Matcher tests: normalization, subsequence assignment, and field choice.
//!
//! These pin *structure* — which characters matched, in which field, and how two
//! matches compare — rather than absolute scores. A scoring constant should be
//! tunable without rewriting the suite; what must not change silently is which
//! command a query finds and which characters a palette underlines. The ranking
//! consequences over the real command set are in `search_tests`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::MatchField;
use super::matcher::{MAX_QUERY_CHARS, Query, best_match, id_tail, match_field};
use crate::commands::{CommandCategory, CommandId, CommandMeta};

fn meta(id: &'static str, title: &'static str) -> CommandMeta {
    CommandMeta::from_static(CommandId::from_static(id), title, CommandCategory::EDITING)
}

fn positions(query: &str, field: &str) -> Vec<u32> {
    let found = match_field(&Query::new(query), field).expect("field should match");
    found.positions[..found.len].to_vec()
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

#[test]
fn the_id_tail_drops_the_namespace() {
    assert_eq!(
        id_tail(&meta("lines.duplicateUp", "Duplicate Line Up")),
        "duplicateUp"
    );
    assert_eq!(id_tail(&meta("noDot", "No Dot")), "noDot");
    assert_eq!(id_tail(&meta("a.b.c", "Nested")), "c");
}

#[test]
fn the_best_field_wins_and_reports_itself() {
    let command = meta("lines.duplicateUp", "Duplicate Line Up")
        .with_aliases(&["clone", "dupe"])
        .with_description("Copies each cursor's lines upwards.");

    let query = Query::new("clone");
    let (field, text, _) = best_match(&query, &command).expect("alias should match");
    assert_eq!(field, MatchField::Alias);
    assert_eq!(text, "clone", "the matched text must be the alias itself");

    let query = Query::new("duplicate");
    let (field, text, _) = best_match(&query, &command).expect("title should match");
    assert_eq!(field, MatchField::Title);
    assert_eq!(text, "Duplicate Line Up");

    let query = Query::new("upwards");
    let (field, _, _) = best_match(&query, &command).expect("description should match");
    assert_eq!(field, MatchField::Description);

    assert!(best_match(&Query::new("zzz"), &command).is_none());
}

#[test]
fn a_title_hit_outranks_the_same_hit_in_a_weaker_field() {
    // Identical text in the title and in the description: only the field weight
    // can separate them, which is what proves the weight is applied at all.
    let titled = meta("test.one", "Sortie");
    let described = meta("test.two", "Unrelated").with_description("Sortie");

    let query = Query::new("sortie");
    let (title_field, _, title_match) = best_match(&query, &titled).expect("title matches");
    let (other_field, _, other_match) =
        best_match(&query, &described).expect("description matches");

    assert_eq!(title_field, MatchField::Title);
    assert_eq!(other_field, MatchField::Description);
    assert!(title_match.score > other_match.score);
}

#[test]
fn field_weights_descend_from_title_to_description() {
    // Pinned because the ordering is a design decision, not an accident: a hit in
    // what the command is *called* must always be worth more than a hit in prose
    // about it.
    let weights = [
        MatchField::Title,
        MatchField::Alias,
        MatchField::Id,
        MatchField::Category,
        MatchField::Description,
    ]
    .map(MatchField::weight);

    assert!(
        weights.windows(2).all(|pair| pair[0] > pair[1]),
        "weights must strictly descend: {weights:?}"
    );
    assert_eq!(weights[0], 100, "a title hit keeps its full score");
}
