//! The `transform.*` editing verbs.
//!
//! Two families, and they differ in what they act on rather than in what they
//! compute — the computing is all in [`crate::text`].
//!
//! **Case verbs** act on each caret's *own* range: its selection when it has
//! one, otherwise the word under it. Falling back to the word is what makes
//! `Upper Case` useful without selecting first, and it is why a caret sitting
//! on punctuation contributes nothing rather than mangling a neighbour.
//!
//! **Line verbs** act on whole lines. Every caret's selection is expanded to
//! the lines it touches, overlapping blocks from different carets are merged,
//! and each resulting block is replaced in one edit — so sorting a selection
//! sorts exactly the lines the user can see are selected, and two carets inside
//! one block sort it once.

use std::collections::BTreeMap;

use crate::document::{CursorState, Document, Position, Range, Selection};
use crate::history::Command;
use crate::text::{case, lines};

use super::editing::{self, CaretPlacement, CursorEdit};
use super::motions;
use super::{KeyResult, KeyboardHandler};

/// A case transformation, as the verb that names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CaseVerb {
    /// Every character uppercased.
    Upper,
    /// Every character lowercased.
    Lower,
    /// Every cased character inverted.
    Swap,
    /// Lower, then upper, then title — cycling on each press.
    Toggle,
    /// Words re-joined in the named style.
    Style(case::CaseStyle),
}

impl CaseVerb {
    /// Applies the verb to one range's text.
    fn apply(self, text: &str) -> String {
        match self {
            Self::Upper => case::upper(text).into_owned(),
            Self::Lower => case::lower(text).into_owned(),
            Self::Swap => case::swap(text),
            Self::Toggle => Self::toggle(text),
            Self::Style(style) => case::restyle(text, style),
        }
    }

    /// Cycles `text` through lower → upper → title → lower.
    ///
    /// The cycle is driven by what the text *is*, not by remembered state, so
    /// it behaves the same however the text got there — an undo, a paste and a
    /// previous press all leave the next press predictable. Text with no cased
    /// characters at all cycles to upper, which is a no-op, rather than sitting
    /// in a state the next press cannot leave.
    fn toggle(text: &str) -> String {
        let has_lower = text.chars().any(char::is_lowercase);
        let has_upper = text.chars().any(char::is_uppercase);
        match (has_lower, has_upper) {
            // All lowercase (or uncased): go up.
            (_, false) => text.to_uppercase(),
            // All uppercase: go to title.
            (false, true) => title_each_word(text),
            // Mixed — already title-ish: go back down.
            (true, true) => text.to_lowercase(),
        }
    }
}

/// Capitalises each whitespace-separated run, leaving the separators as they
/// are.
///
/// Unlike [`case::restyle`] with [`case::CaseStyle::Title`], this preserves the
/// original spacing and punctuation — it is the "Title Case" a person means
/// about a *sentence*, not about an identifier.
fn title_each_word(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at_word_start = true;
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            if at_word_start {
                out.extend(ch.to_uppercase());
            } else {
                out.extend(ch.to_lowercase());
            }
            at_word_start = false;
        } else {
            out.push(ch);
            at_word_start = true;
        }
    }
    out
}

/// A line-block transformation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LineVerb {
    /// Sort ascending.
    Sort,
    /// Sort descending.
    SortReverse,
    /// Reverse the existing order.
    Reverse,
    /// Drop repeated lines, keeping the first of each.
    Dedupe,
    /// Strip trailing whitespace from every line.
    TrimTrailing,
}

impl LineVerb {
    /// Applies the verb to one block of whole lines.
    fn apply(self, block: &str, line_ending: &str) -> String {
        match self {
            Self::Sort => lines::sort(block, line_ending, false),
            Self::SortReverse => lines::sort(block, line_ending, true),
            Self::Reverse => lines::reverse(block, line_ending),
            Self::Dedupe => lines::dedupe(block, line_ending),
            Self::TrimTrailing => lines::trim_trailing(block, line_ending),
        }
    }
}

impl KeyboardHandler {
    /// Runs a case verb over every caret's selection, or the word under it.
    pub(super) fn handle_case_transform(
        document: &Document,
        cursor: &CursorState,
        verb: CaseVerb,
    ) -> KeyResult {
        let edits: Vec<(CursorEdit, CaretPlacement)> = cursor
            .all_selections()
            .map(|sel| case_edit(document, sel, verb))
            .collect();
        editing::build_multi_cursor_command_placed(document, cursor, edits)
            .map_or(KeyResult::Handled, KeyResult::Command)
    }

    /// Runs a line verb over every block of lines a caret touches.
    pub(super) fn handle_line_transform(
        document: &Document,
        cursor: &CursorState,
        verb: LineVerb,
    ) -> KeyResult {
        line_transform_command(document, cursor, verb)
            .map_or(KeyResult::Handled, KeyResult::Command)
    }
}

/// Builds one caret's case edit, with the placement that keeps its selection.
fn case_edit(document: &Document, sel: &Selection, verb: CaseVerb) -> (CursorEdit, CaretPlacement) {
    let (range, was_selected) = if sel.is_collapsed() {
        // No selection: act on the word under the caret. `select_word_at`
        // returns a collapsed selection off a word, which yields an empty
        // range and therefore a no-op edit.
        (motions::select_word_at(document, sel.head).range(), false)
    } else {
        (sel.range(), true)
    };

    let original = document.slice(range);
    let replacement = verb.apply(&original);
    // A verb that changes nothing must produce a genuinely empty edit, not a
    // replacement of text with itself — otherwise pressing Upper Case on
    // already-uppercase text would push an undo entry that undoes nothing.
    if replacement == original {
        return (
            CursorEdit::delete(Range::new(sel.head, sel.head)),
            CaretPlacement::collapsed(0),
        );
    }

    let placement = if was_selected {
        // Keep the transformed text selected, so verbs chain: select once,
        // press snake, press upper, press kebab.
        let head_first = sel.head < sel.anchor;
        if head_first {
            CaretPlacement::selection(replacement.len(), 0)
        } else {
            CaretPlacement::selection(0, replacement.len())
        }
    } else {
        // The caret was inside a word. Keep it at the same offset into that
        // word where it can be — the word may have changed length, so clamp.
        let offset_in_word = document
            .position_to_offset(sel.head)
            .zip(document.position_to_offset(range.start))
            .and_then(|(head, start)| head.checked_sub(start))
            .unwrap_or(0);
        CaretPlacement::collapsed(offset_in_word.min(replacement.len()))
    };

    (CursorEdit::replace(range, replacement), placement)
}

/// Builds the whole-document command for a line verb, or `None` when no block
/// changed.
fn line_transform_command(
    document: &Document,
    cursor: &CursorState,
    verb: LineVerb,
) -> Option<Command> {
    let line_ending = document.line_ending().as_str();

    // Merge every caret's touched lines into non-overlapping blocks, keyed by
    // first line. Two carets inside one block must sort it once, and two
    // adjacent blocks must merge rather than be sorted separately — otherwise
    // the result would depend on how many carets happened to be where.
    let mut blocks: BTreeMap<usize, usize> = BTreeMap::new();
    for sel in cursor.all_selections() {
        let first = sel.start().line;
        // A selection ending at column zero has not entered that line: the
        // user selected up to it, not into it.
        let end = sel.end();
        let last = if end.line > first && end.column == 0 {
            end.line - 1
        } else {
            end.line
        };
        merge_block(&mut blocks, first, last);
    }

    let last_line = document.line_count().saturating_sub(1);
    let mut edits: Vec<CursorEdit> = Vec::with_capacity(blocks.len());
    for (&first, &last) in &blocks {
        if first > last_line {
            continue;
        }
        let last = last.min(last_line);
        // Take the block through to the start of the line after it, so the
        // line endings between its lines are inside the edit and the one after
        // it is not. On the final line there is no line after, so the block
        // ends at that line's content and carries no trailing ending — which
        // is exactly the distinction `lines::` preserves.
        let range = if last < last_line {
            Range::new(Position::new(first, 0), Position::new(last + 1, 0))
        } else {
            Range::new(
                Position::new(first, 0),
                Position::new(last, document.line_len(last)?),
            )
        };

        let block = document.slice(range);
        let transformed = verb.apply(&block, line_ending);
        if transformed != block {
            edits.push(CursorEdit::replace(range, transformed));
        }
    }

    if edits.is_empty() {
        return None;
    }
    editing::build_line_edits_command(document, cursor, edits)
}

/// Inserts `[first, last]` into `blocks`, merging it with any block it touches
/// or abuts.
fn merge_block(blocks: &mut BTreeMap<usize, usize>, first: usize, last: usize) {
    let mut first = first;
    let mut last = last;

    // Everything starting at or before `last + 1` and ending at or after
    // `first` overlaps or abuts, so it folds into this block. `last + 1` rather
    // than `last` because two adjacent blocks share no line but do share the
    // line ending between them, and sorting them separately would leave that
    // boundary unsorted.
    let touching: Vec<usize> = blocks
        .range(..=last.saturating_add(1))
        .filter(|&(_, &end)| end.saturating_add(1) >= first)
        .map(|(&start, _)| start)
        .collect();

    for start in touching {
        if let Some(end) = blocks.remove(&start) {
            first = first.min(start);
            last = last.max(end);
        }
    }
    blocks.insert(first, last);
}
