//! Command palette search: fuzzy matching, recency, and result ordering.
//!
//! # Why this is in the kernel
//!
//! Ranking is *behaviour*, not presentation. "The same query puts a different
//! command first in the terminal than in the browser" is a bug a user feels on
//! the second day, and it is the kind that never gets filed because it reads as
//! flakiness. One implementation, in the language with the test suite, keeps
//! every face honest — the faces contribute a text input and a list, and nothing
//! else.
//!
//! The cost is one search per keystroke across the process boundary. Against
//! roughly fifty commands the match itself is microseconds and allocation-free;
//! serializing the results dwarfs it, and both dwarf a frame.
//!
//! # What a search is
//!
//! [`search`] scores every registered command against a [`Query`], keeps those
//! that matched, adds each one's recency bonus from a [`CommandMru`], and returns
//! them in a total order — best score, then title, then id. An empty query is not
//! a special case in the caller: it lists every command, ordered by recency and
//! then alphabetically.
//!
//! Scoring is per *field* — title, aliases, id tail, category, description — with
//! the best field winning and reporting itself, so a palette can highlight the
//! text that actually matched. See [`matcher`] for the scoring rules and the
//! two-pass assignment they run on.

mod entry;
mod matcher;
mod mru;

#[cfg(test)]
mod matcher_tests;
#[cfg(test)]
mod mru_tests;
#[cfg(test)]
mod search_tests;

pub use entry::{MatchField, PaletteEntry, compare_ranked};
// Re-exported rather than owned: the matcher moved to `crate::fuzzy` so a file
// finder ranks with the same code, and every existing caller keeps its path.
pub use crate::fuzzy::{MAX_QUERY_CHARS, Query};
pub use mru::{CommandMru, MRU_CAPACITY};

use crate::commands::CommandRegistry;

/// Searches `registry` for `query`, ranked, with `mru` breaking near ties.
///
/// `limit` caps the number of results returned; `None` returns every match.
/// Capping happens *after* the full sort, so the top of a limited list is the
/// same as the top of an unlimited one.
///
/// An empty query returns every command rather than nothing — a palette opens
/// showing what is available, and the recency bonus is what makes that first
/// screen useful.
#[must_use]
pub fn search<'a>(
    registry: &'a CommandRegistry,
    mru: &CommandMru,
    query: &Query,
    limit: Option<usize>,
) -> Vec<PaletteEntry<'a>> {
    let mut results: Vec<PaletteEntry<'a>> = if query.is_empty() {
        registry
            .commands()
            .map(|meta| PaletteEntry::listed(meta, mru.bonus(meta.id().as_str())))
            .collect()
    } else {
        registry
            .commands()
            .filter_map(|meta| {
                let (field, text, found) = matcher::best_match(query, meta)?;
                Some(
                    PaletteEntry::matched(
                        meta,
                        found.score,
                        field,
                        text,
                        found.positions,
                        found.len,
                    )
                    .boosted(mru.bonus(meta.id().as_str())),
                )
            })
            .collect()
    };

    results.sort_by(compare_ranked);
    if let Some(limit) = limit {
        results.truncate(limit);
    }
    results
}

/// Searches for `text`, normalizing it into a [`Query`] first.
///
/// The entry point a host calls with a raw input value.
#[must_use]
pub fn search_text<'a>(
    registry: &'a CommandRegistry,
    mru: &CommandMru,
    text: &str,
    limit: Option<usize>,
) -> Vec<PaletteEntry<'a>> {
    search(registry, mru, &Query::new(text), limit)
}
