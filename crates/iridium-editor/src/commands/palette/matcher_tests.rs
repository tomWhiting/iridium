//! Field-choice tests: which of a command's texts a query matches, and how
//! much a hit in each is worth.
//!
//! The matcher's own behaviour — normalization, subsequence assignment,
//! relative scores — moved to `crate::fuzzy` with the code it covers. What is
//! left here is the part that is genuinely about *commands*. The ranking
//! consequences over the real command set are in `search_tests`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::MatchField;
use super::matcher::{best_match, id_tail};
use crate::commands::{CommandCategory, CommandId, CommandMeta};
use crate::fuzzy::Query;

fn meta(id: &'static str, title: &'static str) -> CommandMeta {
    CommandMeta::from_static(CommandId::from_static(id), title, CommandCategory::EDITING)
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
