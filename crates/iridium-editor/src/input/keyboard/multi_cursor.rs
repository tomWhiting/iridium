//! Occurrence search for the multi-cursor "select" verbs.
//!
//! These pure helpers back Ctrl+Shift+L (select all occurrences), Ctrl+D
//! (add-next-match, via [`super::KeyboardHandler::handle_add_selection_next_match`])
//! and [`super::KeyboardHandler::skip_last_added_occurrence`]. Matching is
//! case-sensitive and exact.
//!
//! The search enumerates every candidate match start (on UTF-8 boundaries,
//! *including* overlapping candidates) and then selects among them relative to
//! the ranges already claimed by existing cursors. Anchoring the selection to
//! the already-claimed ranges — rather than to a fixed non-overlapping
//! partition that always begins at byte zero — is what lets Ctrl+D advance
//! correctly from an explicitly selected match that does not fall on the
//! zero-anchored partition boundary (e.g. selecting columns 1..3 of `aaaaa`
//! still finds the untaken match 3..5).

use crate::document::{Document, Range, Selection};

/// Returns every byte offset where `source` matches the document text.
///
/// Candidates are enumerated on UTF-8 character boundaries and **may overlap**
/// (searching `"aa"` in `"aaaaa"` yields starts `0, 1, 2, 3`). Callers impose
/// their own non-overlap policy relative to already-claimed ranges. `source`
/// must be non-empty (the callers guard this); an empty source yields no
/// starts rather than an infinite scan.
fn match_starts(text: &str, source: &str) -> Vec<usize> {
    if source.is_empty() {
        return Vec::new();
    }

    let mut starts = Vec::new();
    let mut from = 0;
    while from <= text.len() {
        let Some(relative) = text[from..].find(source) else {
            break;
        };
        let start = from + relative;
        starts.push(start);
        // Advance to the next character boundary after this match so
        // overlapping candidates are enumerated. `start` is a character
        // boundary (a substring match aligns to one) and the match is
        // non-empty, so `chars().next()` always yields a character.
        from = start + text[start..].chars().next().map_or(1, char::len_utf8);
    }
    starts
}

/// Returns a maximal set of non-overlapping occurrences of `source`, in
/// document order, that do not overlap any range in `occupied`.
///
/// Candidates are scanned left to right; a candidate is accepted when it
/// overlaps neither an `occupied` range nor a previously accepted occurrence.
/// Because every candidate has the same byte length, this greedy earliest-start
/// scan yields a maximal non-overlapping set. Adjacent (endpoint-touching)
/// matches are both accepted, so they remain distinct cursors.
///
/// Select-all-occurrences seeds `occupied` with the exact primary range, so the
/// reference selection is preserved verbatim while every other occurrence that
/// does not overlap it becomes a distinct secondary cursor.
pub fn occurrences_excluding(
    document: &Document,
    source: &str,
    occupied: &[Range],
) -> Vec<Selection> {
    if source.is_empty() {
        return Vec::new();
    }

    let text = document.text();
    let len = source.len();
    let mut result = Vec::new();
    let mut last_end: Option<usize> = None;
    for start in match_starts(&text, source) {
        // Skip candidates overlapping the most recently accepted occurrence.
        if let Some(end) = last_end {
            if start < end {
                continue;
            }
        }
        let Some(selection) = offsets_to_selection(document, start, start + len) else {
            continue;
        };
        if occupied
            .iter()
            .any(|range| ranges_overlap(selection.range(), *range))
        {
            continue;
        }
        result.push(selection);
        last_end = Some(start + len);
    }
    result
}

/// Finds the next occurrence of `source` at or after byte offset `after`,
/// wrapping to the start of the document, whose range does not overlap any
/// range in `taken`.
///
/// Returns `None` when `source` is empty or every candidate overlaps a taken
/// range. The scan is cyclic: it starts from the first candidate beginning at
/// or after `after`, then continues (wrapping once) until it either finds a
/// candidate that overlaps no taken range or returns to where it started.
///
/// Candidates are enumerated relative to the whole document (overlapping
/// starts included) and filtered against `taken` by overlap, so the result is
/// always disjoint from the cursors that produced `taken`.
pub fn next_occurrence(
    document: &Document,
    source: &str,
    after: usize,
    taken: &[Range],
) -> Option<Selection> {
    if source.is_empty() {
        return None;
    }

    let text = document.text();
    let starts = match_starts(&text, source);
    if starts.is_empty() {
        return None;
    }

    let len = source.len();
    // Index of the first candidate starting at or after `after`; if none, the
    // wrap begins at the first candidate (index 0).
    let first = starts.iter().position(|&start| start >= after).unwrap_or(0);

    let count = starts.len();
    for step in 0..count {
        let start = starts[(first + step) % count];
        let Some(selection) = offsets_to_selection(document, start, start + len) else {
            continue;
        };
        if !taken
            .iter()
            .any(|range| ranges_overlap(selection.range(), *range))
        {
            return Some(selection);
        }
    }
    None
}

/// Returns true when two ranges share at least one interior position.
///
/// Touching endpoints (one range ends exactly where the other begins) do not
/// count as overlap, so adjacent matches are treated as disjoint.
fn ranges_overlap(a: Range, b: Range) -> bool {
    a.start < b.end && b.start < a.end
}

/// Builds a forward selection from a start/end byte offset pair.
fn offsets_to_selection(document: &Document, start: usize, end: usize) -> Option<Selection> {
    let start_position = document.offset_to_position(start)?;
    let end_position = document.offset_to_position(end)?;
    Some(Selection::new(start_position, end_position))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::document::Position;

    #[test]
    fn match_starts_enumerates_overlapping_candidates() {
        assert_eq!(match_starts("aaaaa", "aa"), vec![0, 1, 2, 3]);
        assert_eq!(match_starts("abcabc", "abc"), vec![0, 3]);
        assert_eq!(match_starts("abc", "z"), Vec::<usize>::new());
    }

    #[test]
    fn match_starts_handles_multibyte_text() {
        // "é" is two bytes; ensure boundary advancement never slices mid-char.
        let text = "éaéaé";
        // "é" occurs at byte offsets 0, 3, 6.
        assert_eq!(match_starts(text, "é"), vec![0, 3, 6]);
    }

    #[test]
    fn next_occurrence_anchors_to_taken_not_byte_zero() {
        // Finding 4: selecting "aa" at columns 1..3 of "aaaaa" must find the
        // untaken match 3..5, not wrap to the zero-anchored partition 0..2.
        let doc = Document::new("aaaaa");
        let taken = [Range::new(Position::new(0, 1), Position::new(0, 3))];
        let next = next_occurrence(&doc, "aa", 3, &taken).expect("finds 3..5");
        assert_eq!(next.range().start, Position::new(0, 3));
        assert_eq!(next.range().end, Position::new(0, 5));
    }

    #[test]
    fn occurrences_excluding_seeds_primary_and_keeps_adjacent() {
        let doc = Document::new("aaaa");
        let occupied = [Range::new(Position::new(0, 0), Position::new(0, 2))];
        let found = occurrences_excluding(&doc, "aa", &occupied);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].range().start, Position::new(0, 2));
        assert_eq!(found[0].range().end, Position::new(0, 4));
    }
}
