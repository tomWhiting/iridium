//! Fuzzy subsequence matching and scoring for one query against one field.
//!
//! The whole matcher is **integer arithmetic over character positions**, with no
//! floats, no recursion and no allocation. Each of those is deliberate:
//!
//! - *Integers* because the same query must produce the same ranking in the
//!   terminal, in a native embedder and in the browser. Floating point would be
//!   deterministic in practice here, but integers make it true by construction.
//! - *No recursion* because the obvious "try every assignment" matcher is
//!   exponential; the two-pass scheme below is linear in the field length and
//!   loses only pathological cases.
//! - *No allocation* because this runs on every keystroke against every
//!   registered command, and the positions can never outnumber the query, which
//!   is capped.
//!
//! # The two passes
//!
//! A query matches a field when its characters appear in order, not necessarily
//! together. Many assignments of query characters to field positions may exist —
//! `ln` against `Join Lines` can take the `l` of `Lines` or neither — and their
//! quality differs enormously, since consecutive and word-initial characters are
//! what a user perceives as a good match.
//!
//! The **forward** pass takes the leftmost assignment. It is also the canonical
//! subsequence test: if it fails, no assignment exists at all. The **backward**
//! pass takes the rightmost. Scoring both and keeping the better one costs one
//! extra linear scan and fixes the common failure of leftmost-only matchers —
//! `de` against `Duplicate Line Down` scoring the `d` of `Duplicate` and the `e`
//! of `Line` rather than the word-initial `D` and the `e` that follows it.

use super::entry::MatchField;
use crate::commands::CommandMeta;

/// The longest query the matcher considers.
///
/// Longer queries are truncated, which can only make the result set *larger*
/// than the user intended, never smaller. Thirty-two characters is far beyond
/// any command title, and the cap is what lets match positions live in a fixed
/// array.
pub const MAX_QUERY_CHARS: usize = 32;

/// Awarded for every character that matches at all.
const BASE: i32 = 100;
/// Awarded when a character matches immediately after the previous match.
const CONSECUTIVE: i32 = 80;
/// Awarded when a character matches at the start of a word.
const BOUNDARY: i32 = 120;
/// Awarded when the character matched without folding case.
const EXACT_CASE: i32 = 20;
/// Awarded when the first character matches at position zero.
const PREFIX: i32 = 400;
/// Awarded when the field is exactly the query.
const WHOLE_FIELD: i32 = 600;
/// Charged for every field character skipped *between* two matches.
const GAP: i32 = 25;
/// Charged per character before the first match.
const LEADING: i32 = 10;
/// The most [`LEADING`] may total, so a late match in a long description is
/// penalized but not annihilated.
const LEADING_CAP: i32 = 200;
/// The longest field length that still adds to the length penalty.
const LENGTH_CAP: usize = 80;

/// Folds one character for case-insensitive comparison.
///
/// Takes the first character of the full Unicode lowercase mapping. A handful of
/// characters lowercase to more than one (`İ` becomes `i` plus a combining dot),
/// and this treats them as their first: right for search, and it keeps the
/// comparison a single `char` against a single `char`.
fn fold(character: char) -> char {
    character.to_lowercase().next().unwrap_or(character)
}

/// Whether the character after `previous` begins a word.
///
/// The start of the text, anything after a separator, and the upper-case half of
/// a camelCase hump — which is what makes `du` find `lines.duplicateUp` through
/// its id and `dU` find the same command's `Up`.
const fn is_word_start(previous: Option<char>, current: char) -> bool {
    let Some(before) = previous else {
        return true;
    };
    matches!(before, ' ' | '-' | '_' | '.' | '/' | '\\' | ':')
        || (!before.is_uppercase() && current.is_uppercase())
}

/// A palette query, normalized and capped.
///
/// Holds both the folded and the original characters: folded to compare, original
/// so an exactly-cased match can be rewarded without a second pass.
#[derive(Debug, Clone)]
pub struct Query {
    folded: [char; MAX_QUERY_CHARS],
    raw: [char; MAX_QUERY_CHARS],
    len: usize,
    truncated: bool,
}

impl Query {
    /// Normalizes `text` into a query: trimmed, capped at [`MAX_QUERY_CHARS`].
    ///
    /// Interior spaces are kept, because multi-word aliases such as `one cursor`
    /// are meant to be matched with the space typed.
    #[must_use]
    pub fn new(text: &str) -> Self {
        let mut folded = ['\0'; MAX_QUERY_CHARS];
        let mut raw = ['\0'; MAX_QUERY_CHARS];
        let mut len = 0;
        let mut truncated = false;
        for character in text.trim().chars() {
            if len == MAX_QUERY_CHARS {
                truncated = true;
                break;
            }
            raw[len] = character;
            folded[len] = fold(character);
            len += 1;
        }
        Self {
            folded,
            raw,
            len,
            truncated,
        }
    }

    /// The number of characters the matcher will use.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the query matches everything, because nothing was typed.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Whether characters beyond [`MAX_QUERY_CHARS`] were discarded.
    #[must_use]
    pub const fn is_truncated(&self) -> bool {
        self.truncated
    }
}

/// One assignment of query characters to field positions.
#[derive(Debug, Clone, Copy)]
pub(super) struct Assignment {
    positions: [u32; MAX_QUERY_CHARS],
    len: usize,
}

impl Assignment {
    /// The ascending character positions this assignment chose.
    const fn positions(&self) -> [u32; MAX_QUERY_CHARS] {
        self.positions
    }
}

/// A scored match of one query against one field.
#[derive(Debug, Clone, Copy)]
pub(super) struct FieldMatch {
    pub(super) score: i32,
    pub(super) positions: [u32; MAX_QUERY_CHARS],
    pub(super) len: usize,
}

/// The leftmost assignment, or `None` when the query is not a subsequence.
///
/// Greedy first-fit is exact as a *feasibility* test: taking the earliest
/// possible position for each character never rules out a later one.
fn assign_forward(query: &Query, field: &str) -> Option<Assignment> {
    let mut positions = [0_u32; MAX_QUERY_CHARS];
    let mut matched = 0;
    for (index, character) in field.chars().enumerate() {
        if matched == query.len {
            break;
        }
        if fold(character) == query.folded[matched] {
            positions[matched] = u32::try_from(index).unwrap_or(u32::MAX);
            matched += 1;
        }
    }
    (matched == query.len).then_some(Assignment {
        positions,
        len: matched,
    })
}

/// The rightmost assignment, given a field already known to contain the query.
fn assign_backward(query: &Query, field: &str, field_len: usize) -> Option<Assignment> {
    let mut positions = [0_u32; MAX_QUERY_CHARS];
    let mut remaining = query.len;
    for (offset, character) in field.chars().rev().enumerate() {
        if remaining == 0 {
            break;
        }
        if fold(character) == query.folded[remaining - 1] {
            remaining -= 1;
            let index = field_len - 1 - offset;
            positions[remaining] = u32::try_from(index).unwrap_or(u32::MAX);
        }
    }
    (remaining == 0).then_some(Assignment {
        positions,
        len: query.len,
    })
}

/// Scores one assignment in a single pass over the field.
///
/// The pass continues past the last match because the field's total length is
/// itself a (small) penalty: given two commands that match equally well, the one
/// with the shorter title is the one the user meant.
fn score_assignment(query: &Query, field: &str, assignment: &Assignment) -> i32 {
    let mut score = 0;
    let mut matched = 0;
    let mut previous: Option<char> = None;
    let mut previous_position: Option<usize> = None;
    let mut field_len = 0_usize;

    for (index, character) in field.chars().enumerate() {
        field_len += 1;
        if matched < assignment.len && assignment.positions[matched] as usize == index {
            score += BASE;
            if previous_position.is_some_and(|previous| previous + 1 == index) {
                score += CONSECUTIVE;
            }
            if is_word_start(previous, character) {
                score += BOUNDARY;
            }
            if query.raw[matched] == character {
                score += EXACT_CASE;
            }
            previous_position = Some(index);
            matched += 1;
        }
        previous = Some(character);
    }

    if assignment.len == 0 {
        return score;
    }
    let first = assignment.positions[0] as usize;
    let last = previous_position.unwrap_or(first);

    if first == 0 {
        score += PREFIX;
    }
    if assignment.len == field_len {
        score += WHOLE_FIELD;
    }

    let span = last - first + 1;
    let gaps = span - assignment.len;
    score -= GAP * i32::try_from(gaps).unwrap_or(i32::MAX);
    score -= LEADING_CAP.min(LEADING * i32::try_from(first).unwrap_or(i32::MAX));
    score -= i32::try_from(field_len.min(LENGTH_CAP)).unwrap_or(0);

    score
}

/// Scores `query` against one field, unweighted.
///
/// `None` when the query is not a subsequence of the field.
pub(super) fn match_field(query: &Query, field: &str) -> Option<FieldMatch> {
    if query.is_empty() || field.is_empty() {
        return None;
    }
    let forward = assign_forward(query, field)?;
    let field_len = field.chars().count();
    let backward = assign_backward(query, field, field_len);

    let mut best = (score_assignment(query, field, &forward), forward);
    if let Some(backward) = backward {
        let backward_score = score_assignment(query, field, &backward);
        // Strictly greater, so the forward assignment wins a tie: it is the one a
        // reader reproduces by eye, and ties must resolve the same way every run.
        if backward_score > best.0 {
            best = (backward_score, backward);
        }
    }

    Some(FieldMatch {
        score: best.0,
        positions: best.1.positions(),
        len: best.1.len,
    })
}

/// The best-scoring field of one command, with the query's weight applied.
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
