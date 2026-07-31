//! Search tests against the real built-in command set.
//!
//! These are the tests that matter: they assert what a user sees after typing
//! four characters. Every one is written against the actual registry rather than
//! a fixture, so a new command, a retitled command or a new alias that would
//! displace an expected result fails here rather than in someone's hands.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{CommandMru, Query, search, search_text};
use crate::commands::builtin::{BUILTIN_COMMAND_COUNT, builtin_registry};
use crate::commands::{CommandId, CommandRegistry};

fn registry() -> CommandRegistry {
    builtin_registry().expect("the built-in table is consistent")
}

fn top(registry: &CommandRegistry, query: &str) -> String {
    let results = search_text(registry, &CommandMru::new(), query, None);
    let best = results
        .first()
        .unwrap_or_else(|| panic!("`{query}` matched nothing"));
    best.id().as_str().to_owned()
}

fn ids(registry: &CommandRegistry, query: &str, limit: Option<usize>) -> Vec<String> {
    search_text(registry, &CommandMru::new(), query, limit)
        .into_iter()
        .map(|entry| entry.id().as_str().to_owned())
        .collect()
}

#[test]
fn an_empty_query_lists_every_command() {
    let registry = registry();
    let results = search_text(&registry, &CommandMru::new(), "", None);
    assert_eq!(results.len(), BUILTIN_COMMAND_COUNT);
    assert!(
        results.iter().all(|entry| entry.matched_field().is_none()),
        "nothing was matched, so nothing may claim a matched field"
    );
    assert!(results.iter().all(|entry| entry.matches().is_empty()));

    // Alphabetical by title, since no score separates them.
    let titles: Vec<&str> = results.iter().map(super::PaletteEntry::title).collect();
    let mut sorted = titles.clone();
    sorted.sort_unstable();
    assert_eq!(titles, sorted);
}

#[test]
fn a_whitespace_only_query_is_an_empty_query() {
    let registry = registry();
    assert_eq!(
        search_text(&registry, &CommandMru::new(), "   ", None).len(),
        BUILTIN_COMMAND_COUNT
    );
}

#[test]
fn typing_a_title_finds_that_command_first() {
    let registry = registry();
    assert_eq!(top(&registry, "join"), "lines.join");
    assert_eq!(top(&registry, "join lines"), "lines.join");
    assert_eq!(top(&registry, "undo"), "history.undo");
    assert_eq!(top(&registry, "redo"), "history.redo");
    assert_eq!(top(&registry, "select all"), "selection.selectAll");
    assert_eq!(top(&registry, "outdent"), "edit.outdent");
    assert_eq!(top(&registry, "toggle line comment"), "comment.toggleLine");
}

#[test]
fn typing_an_alias_finds_the_command_the_alias_belongs_to() {
    // The whole justification for `CommandMeta::aliases`: none of these words
    // appears in the title of the command they must find.
    let registry = registry();
    assert_eq!(top(&registry, "yank"), "clipboard.copy");
    assert_eq!(top(&registry, "eol"), "cursor.lineEnd");
    assert_eq!(top(&registry, "bof"), "cursor.documentStart");
    assert_eq!(top(&registry, "dedent"), "edit.outdent");
    assert_eq!(top(&registry, "backspace"), "edit.deleteBackward");
    assert_eq!(top(&registry, "merge lines"), "lines.join");
    assert_eq!(top(&registry, "revert"), "history.undo");
}

#[test]
fn an_alias_shared_by_two_commands_returns_both_deterministically() {
    let registry = registry();
    let erase = ids(&registry, "erase", None);
    let first_two: Vec<&String> = erase.iter().take(2).collect();
    assert!(
        first_two.contains(&&"edit.deleteBackward".to_owned())
            && first_two.contains(&&"edit.deleteForward".to_owned()),
        "both deletions carry the alias `erase`: {erase:?}"
    );

    // Repeating the search must not permute the tie.
    for _ in 0..8 {
        assert_eq!(ids(&registry, "erase", None), erase);
    }
}

#[test]
fn an_abbreviation_finds_its_command_through_the_id() {
    let registry = registry();
    assert_eq!(top(&registry, "dupup"), "lines.duplicateUp");
    assert_eq!(top(&registry, "collapse"), "selection.collapseToPrimary");
}

#[test]
fn a_query_matching_nothing_returns_nothing() {
    let registry = registry();
    assert!(search_text(&registry, &CommandMru::new(), "qqqqzzz", None).is_empty());
}

#[test]
fn results_report_the_text_and_positions_a_palette_should_highlight() {
    let registry = registry();
    let results = search_text(&registry, &CommandMru::new(), "yank", None);
    let copy = results
        .iter()
        .find(|entry| entry.id().as_str() == "clipboard.copy")
        .expect("copy should match its alias");

    assert_eq!(copy.matched_field(), Some(super::MatchField::Alias));
    assert_eq!(
        copy.matched_text(),
        "yank",
        "positions index the matched text, so it must be the alias, not the title"
    );
    assert_eq!(copy.matches(), [0, 1, 2, 3]);
    assert_eq!(copy.title(), "Copy");

    let join = search_text(&registry, &CommandMru::new(), "jln", None);
    let join = join.first().expect("`jln` should match something");
    assert_eq!(join.id().as_str(), "lines.join");
    assert_eq!(join.matched_text(), "Join Lines");
    // `J`, then the `L` of `Lines`, then its `n` — a subsequence, not a substring.
    assert_eq!(join.matches(), [0, 5, 7]);
}

#[test]
fn every_match_position_lies_inside_the_text_it_indexes() {
    // The invariant a highlighting host relies on. Checked over every command for
    // several query shapes rather than one hand-picked case.
    let registry = registry();
    for query in ["e", "li", "cur", "select", "o", "delete", "up"] {
        for entry in search_text(&registry, &CommandMru::new(), query, None) {
            let length =
                u32::try_from(entry.matched_text().chars().count()).expect("a short field");
            assert_eq!(
                entry.matches().len(),
                Query::new(query).len(),
                "every query character must be assigned a position"
            );
            assert!(
                entry.matches().iter().all(|&position| position < length),
                "`{query}` on `{}` produced {:?}, outside a {length}-character text",
                entry.matched_text(),
                entry.matches()
            );
            assert!(
                entry.matches().windows(2).all(|pair| pair[0] < pair[1]),
                "positions must ascend: {:?}",
                entry.matches()
            );
        }
    }
}

#[test]
fn the_limit_truncates_without_changing_the_top() {
    let registry = registry();
    let all = ids(&registry, "line", None);
    assert!(all.len() > 3, "expected several matches: {all:?}");

    assert_eq!(ids(&registry, "line", Some(3)), all[..3].to_vec());
    assert_eq!(ids(&registry, "line", Some(0)), Vec::<String>::new());
    assert_eq!(ids(&registry, "line", Some(all.len() + 10)), all);
}

#[test]
fn recency_decides_between_comparable_matches() {
    let registry = registry();
    let neutral = ids(&registry, "cursor", None);
    let loser = neutral
        .iter()
        .find(|id| *id != &neutral[0])
        .expect("expected at least two matches")
        .clone();

    let mut mru = CommandMru::new();
    mru.record(&CommandId::new(loser.clone()));
    let biased = search_text(&registry, &mru, "cursor", None);

    assert_eq!(
        biased[0].id().as_str(),
        loser,
        "the recently used command should lead a field of similar matches"
    );
}

#[test]
fn recency_does_not_override_a_clearly_better_match() {
    // The failure mode that makes an MRU worse than none: typing the exact title
    // of one command and getting a different one because it was used recently.
    let registry = registry();
    let mut mru = CommandMru::new();
    for id in [
        "multiCursor.skipLastOccurrence",
        "selection.collapseToPrimary",
        "multiCursor.addSelectionToNextMatch",
    ] {
        mru.record(&CommandId::from_static(id));
    }

    for (query, expected) in [
        ("select all", "selection.selectAll"),
        ("undo", "history.undo"),
        ("join", "lines.join"),
        ("copy", "clipboard.copy"),
    ] {
        let biased = search_text(&registry, &mru, query, None);
        assert_eq!(
            biased[0].id().as_str(),
            expected,
            "`{query}` must still find its own command with a loaded recency list"
        );
    }
}

#[test]
fn results_are_ordered_by_score_then_title_then_id() {
    let registry = registry();
    let results = search_text(&registry, &CommandMru::new(), "e", None);
    assert!(results.len() > 5);

    for pair in results.windows(2) {
        let (left, right) = (&pair[0], &pair[1]);
        assert!(
            left.score() > right.score()
                || (left.score() == right.score() && left.title() <= right.title()),
            "`{}` ({}) must not sort after `{}` ({})",
            left.title(),
            left.score(),
            right.title(),
            right.score()
        );
    }
}

#[test]
fn a_query_longer_than_the_cap_still_returns_a_sane_result() {
    // Truncation widens the query, so it can only return more than the user asked
    // for — never a panic, and never a slice out of range.
    let registry = registry();
    let long = "select all".repeat(20);
    let results = search_text(&registry, &CommandMru::new(), &long, None);
    for entry in &results {
        assert_eq!(entry.matches().len(), super::MAX_QUERY_CHARS);
    }
}

#[test]
fn searching_a_prepared_query_matches_searching_its_text() {
    let registry = registry();
    let mru = CommandMru::new();
    let by_text = search_text(&registry, &mru, "line", None);
    let by_query = search(&registry, &mru, &Query::new("line"), None);

    let text_ids: Vec<&str> = by_text.iter().map(|entry| entry.id().as_str()).collect();
    let query_ids: Vec<&str> = by_query.iter().map(|entry| entry.id().as_str()).collect();
    assert_eq!(text_ids, query_ids);
}
