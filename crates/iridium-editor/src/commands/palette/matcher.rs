//! Field choice for one query against one command.
//!
//! The matching itself lives in [`crate::fuzzy`], which knows nothing about
//! commands. What is command-specific — and therefore here — is *which strings
//! of a command are worth matching against*, and how much a hit in each is
//! worth.

use super::entry::MatchField;
use crate::commands::CommandMeta;
use crate::fuzzy::{FieldMatch, Query, match_field};

/// The best-scoring field of one command, with the field's weight applied.
///
/// Every field is scored independently and the best one wins, rather than
/// concatenating them into one haystack: a query is a guess at *one* name, and
/// letters gathered from a title and a description are not a name the user was
/// thinking of.
///
/// Ties break toward the earlier [`MatchField`], which is the more authoritative
/// one — a title hit beats an alias hit of the same strength.
pub(super) fn best_match<'a>(
    query: &Query,
    meta: &'a CommandMeta,
) -> Option<(MatchField, &'a str, FieldMatch)> {
    let mut best: Option<(MatchField, &'a str, FieldMatch)> = None;

    let mut consider = |field: MatchField, text: &'a str| {
        let Some(mut found) = match_field(query, text) else {
            return;
        };
        found.score = found.score * field.weight() / 100;
        let improves = best
            .as_ref()
            .is_none_or(|(_, _, current)| found.score > current.score);
        if improves {
            best = Some((field, text, found));
        }
    };

    consider(MatchField::Title, meta.title());
    for alias in meta.aliases() {
        consider(MatchField::Alias, alias);
    }
    consider(MatchField::Id, id_tail(meta));
    consider(MatchField::Category, meta.category().as_str());
    if let Some(description) = meta.description() {
        consider(MatchField::Description, description);
    }

    best
}

/// The final segment of a command id — `duplicateUp` of `lines.duplicateUp`.
///
/// The namespace is dropped because it duplicates the category, which is scored
/// separately and reads better; what the tail adds is the camelCase verb, which
/// is often what a user who knows the API types.
pub(super) fn id_tail(meta: &CommandMeta) -> &str {
    let id = meta.id().as_str();
    id.rsplit_once('.').map_or(id, |(_, tail)| tail)
}
