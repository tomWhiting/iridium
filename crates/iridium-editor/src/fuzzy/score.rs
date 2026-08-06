//! Assignment of query characters to string positions, and the score of one.

use super::query::{MAX_QUERY_CHARS, Query, fold};

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
/// Awarded when the string is exactly the query.
const WHOLE_FIELD: i32 = 600;
/// Charged for every character skipped *between* two matches.
const GAP: i32 = 25;
/// Charged per character before the first match.
const LEADING: i32 = 10;
/// The most [`LEADING`] may total, so a late match in a long description is
/// penalized but not annihilated.
const LEADING_CAP: i32 = 200;
/// The longest string length that still adds to the length penalty.
const LENGTH_CAP: usize = 80;

/// Whether the character after `previous` begins a word.
///
/// The start of the text, anything after a separator, and the upper-case half of
/// a camelCase hump — which is what makes `du` find `lines.duplicateUp` through
/// its id and `dU` find the same command's `Up`.
///
/// `/` and `\` are separators here, which is also what makes a path's basename
/// score as a word start rather than as the tail of one long run.
const fn is_word_start(previous: Option<char>, current: char) -> bool {
    let Some(before) = previous else {
        return true;
    };
    matches!(before, ' ' | '-' | '_' | '.' | '/' | '\\' | ':')
        || (!before.is_uppercase() && current.is_uppercase())
}

/// One assignment of query characters to string positions.
#[derive(Debug, Clone, Copy)]
struct Assignment {
    positions: [u32; MAX_QUERY_CHARS],
    len: usize,
}

/// A scored match of one query against one string.
#[derive(Debug, Clone, Copy)]
pub struct FieldMatch {
    /// The score, higher being better. Comparable only against other scores
    /// from this matcher, and only before any caller's weighting.
    pub score: i32,
    /// The matched **character** indices, ascending. Only the first
    /// [`Self::len`] entries are meaningful.
    pub positions: [u32; MAX_QUERY_CHARS],
    /// How many of [`Self::positions`] were filled.
    pub len: usize,
}

impl FieldMatch {
    /// The matched character indices, without the unused tail.
    #[must_use]
    pub fn matched(&self) -> &[u32] {
        &self.positions[..self.len]
    }
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

/// The rightmost assignment, given a string already known to contain the query.
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

/// Scores one assignment in a single pass over the string.
///
/// The pass continues past the last match because the total length is itself a
/// (small) penalty: given two candidates that match equally well, the shorter
/// one is the one the user meant.
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

/// Scores `query` against one string, unweighted.
///
/// `None` when the query is not a subsequence of `field`, and also when either
/// side is empty — an empty query means "everything", which is a listing
/// decision for the caller rather than a match.
#[must_use]
pub fn match_field(query: &Query, field: &str) -> Option<FieldMatch> {
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
        positions: best.1.positions,
        len: best.1.len,
    })
}
