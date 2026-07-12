//! Multi-cursor edit construction.
//!
//! This module turns per-cursor edit intents into a single reversible
//! [`Command`], with exact position accounting across cursors.
//!
//! # Algorithm
//!
//! Each cursor contributes one [`CursorEdit`]: a range to remove (empty for a
//! pure insertion) plus text to insert at the range start (empty for a pure
//! deletion). [`build_multi_cursor_command`] then:
//!
//! 1. Orders the edits by document position and clamps overlapping ranges
//!    (e.g. two word-deletions reaching into the same word) so each byte is
//!    deleted exactly once.
//! 2. Emits `Delete`/`Insert` commands from the **last** edit to the
//!    **first**, so every command's coordinates stay valid in the original
//!    document while the compound is applied front to back.
//! 3. Computes each cursor's post-edit position in **byte offsets**: the new
//!    caret offset is the edit's original start offset, plus the inserted
//!    text length, plus the cumulative byte delta of all edits at earlier
//!    offsets. This accounts for same-line column drift, newline insertions
//!    shifting later lines, and selection replacements alike. The offsets are
//!    converted back to line/column positions against a scratch copy of the
//!    document with all edits applied, so the mapping is exact for any mix of
//!    multi-byte characters and line endings.
//! 4. Merges cursors that converge on the same position and appends a
//!    trailing `SetSelection` carrying the full old and new multi-cursor
//!    states, which makes the compound fully undoable (text *and* cursors).

use crate::document::{CursorState, Document, Position, Range, Selection};
use crate::history::Command;

use super::motions;

/// A single cursor's contribution to a multi-cursor edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorEdit {
    /// Range of text to remove (may be empty for a pure insertion).
    pub range: Range,
    /// Text to insert at `range.start` (may be empty for a pure deletion).
    pub text: String,
}

impl CursorEdit {
    /// Creates an edit that replaces `range` with `text`.
    pub const fn replace(range: Range, text: String) -> Self {
        Self { range, text }
    }

    /// Creates an edit that deletes `range` without inserting anything.
    pub const fn delete(range: Range) -> Self {
        Self {
            range,
            text: String::new(),
        }
    }
}

/// Builds a single reversible command from one edit per cursor.
///
/// `edits` must contain exactly one entry per cursor, in
/// [`CursorState::all_selections`] order (primary first). Each cursor
/// collapses to the end of its inserted text (or the start of its deleted
/// range for pure deletions), and cursors that converge on the same position
/// are merged.
///
/// Returns `None` when the edits change neither the document nor the cursor
/// state (for example backspace with every cursor at the document start).
pub fn build_multi_cursor_command(
    document: &Document,
    cursor: &CursorState,
    edits: Vec<CursorEdit>,
) -> Option<Command> {
    // One edit per cursor is required for the caret bookkeeping below;
    // anything else indicates a caller bug, and doing nothing is the safe
    // response.
    if edits.len() != cursor.cursor_count() {
        return None;
    }

    // Pair each edit with whether it belongs to the primary cursor, then
    // order by document position so overlap clamping and offset accounting
    // can run front to back.
    let mut ordered: Vec<(CursorEdit, bool)> = edits
        .into_iter()
        .enumerate()
        .map(|(index, edit)| (edit, index == 0))
        .collect();
    ordered.sort_by_key(|(edit, _)| (edit.range.start, edit.range.end));

    // Clamp ranges to the document and to each other so no byte is deleted
    // twice (overlaps happen when e.g. two cursors word-delete into the same
    // word). A fully-consumed range degenerates to an empty range at the
    // previous edit's end, which naturally merges the cursors afterwards.
    let mut prev_end: Option<Position> = None;
    for (edit, _) in &mut ordered {
        let mut start = document.clamp_position(edit.range.start);
        let end = document.clamp_position(edit.range.end).max(start);
        if let Some(prev) = prev_end {
            start = start.max(prev);
        }
        edit.range = Range::new(start, end.max(start));
        prev_end = Some(edit.range.end);
    }

    // Byte offsets of every edit in the original document. Clamped positions
    // always convert; a failure means inconsistent state, in which case the
    // only safe command is none at all.
    let mut offsets: Vec<(usize, usize)> = Vec::with_capacity(ordered.len());
    for (edit, _) in &ordered {
        let start = document.position_to_offset(edit.range.start)?;
        let end = document.position_to_offset(edit.range.end)?;
        offsets.push((start, end));
    }

    // Emit content commands from last edit to first so each command's
    // coordinates remain valid in the original document, and apply the same
    // edits to a scratch copy for caret conversion below.
    let mut commands: Vec<Command> = Vec::new();
    let mut scratch = document.clone();
    for (edit, _) in ordered.iter().rev() {
        if !edit.range.is_empty() {
            let deleted_text = scratch.delete(edit.range).ok()?;
            commands.push(Command::Delete {
                range: edit.range,
                deleted_text,
            });
        }
        if !edit.text.is_empty() {
            scratch.insert(edit.range.start, &edit.text).ok()?;
            commands.push(Command::Insert {
                position: edit.range.start,
                text: edit.text.clone(),
            });
        }
    }

    // Compute post-edit caret positions front to back: original start offset
    // + inserted length + cumulative byte delta of all earlier edits, then
    // convert against the scratch document (which already contains every
    // edit).
    let mut new_selections: Vec<Selection> = Vec::with_capacity(ordered.len());
    let mut primary_index = 0usize;
    let mut delta: i64 = 0;
    for (index, ((edit, is_primary), (start_off, end_off))) in
        ordered.iter().zip(offsets.iter()).enumerate()
    {
        let caret = i64::try_from(start_off + edit.text.len()).ok()? + delta;
        let caret = usize::try_from(caret).ok()?;
        let position = scratch.offset_to_position(caret)?;

        if *is_primary {
            primary_index = index;
        }
        new_selections.push(Selection::collapsed(position));

        delta += i64::try_from(edit.text.len()).ok()?;
        delta -= i64::try_from(end_off - start_off).ok()?;
    }

    // Rebuild the cursor state, keeping the primary cursor primary and
    // merging any cursors that converged on the same position.
    let primary = new_selections.get(primary_index).copied()?;
    let mut new_state = CursorState::new(primary);
    for (index, selection) in new_selections.iter().enumerate() {
        if index != primary_index {
            new_state.add_cursor(*selection);
        }
    }

    // Always record the cursor transition when content changed, even if no
    // caret moved (e.g. delete-forward leaves every caret in place): the
    // SetSelection is what lets the command's inverse restore the exact
    // multi-cursor state on undo. Without it, history replay has no cursor
    // context and must fall back to a single caret at the edit site.
    if !commands.is_empty() || new_state != *cursor {
        commands.push(Command::SetSelection {
            old_state: cursor.clone(),
            new_state,
        });
    }

    match commands.len() {
        0 => None,
        1 => commands.pop(),
        _ => Some(Command::Compound { commands }),
    }
}

/// Builds one edit per cursor that replaces each cursor's selection with
/// `text` (typing, Enter, Tab, and paste-same-text-everywhere).
///
/// Collapsed cursors become pure insertions at the caret.
pub fn replace_all_edits(cursor: &CursorState, text: &str) -> Vec<CursorEdit> {
    cursor
        .all_selections()
        .map(|sel| CursorEdit::replace(sel.range(), text.to_owned()))
        .collect()
}

/// Builds one backspace edit per cursor.
///
/// Cursors with a selection delete the selection; collapsed cursors delete
/// one character (or one word when `word` is true) before the caret. Cursors
/// at the document start contribute an empty (no-op) edit.
pub fn backspace_edits(document: &Document, cursor: &CursorState, word: bool) -> Vec<CursorEdit> {
    cursor
        .all_selections()
        .map(|sel| {
            if sel.is_collapsed() {
                let start = if word {
                    motions::word_left(document, sel.head)
                } else {
                    motions::char_left(document, sel.head)
                };
                CursorEdit::delete(Range::new(start, sel.head))
            } else {
                CursorEdit::delete(sel.range())
            }
        })
        .collect()
}

/// Builds one forward-delete edit per cursor.
///
/// Cursors with a selection delete the selection; collapsed cursors delete
/// one character (or one word when `word` is true) after the caret. Cursors
/// at the document end contribute an empty (no-op) edit.
pub fn delete_forward_edits(
    document: &Document,
    cursor: &CursorState,
    word: bool,
) -> Vec<CursorEdit> {
    cursor
        .all_selections()
        .map(|sel| {
            if sel.is_collapsed() {
                let end = if word {
                    motions::word_right(document, sel.head)
                } else {
                    motions::char_right(document, sel.head)
                };
                CursorEdit::delete(Range::new(sel.head, end))
            } else {
                CursorEdit::delete(sel.range())
            }
        })
        .collect()
}

/// Returns the clipboard text for a copy (or the text half of a cut).
///
/// Standard multi-cursor clipboard behavior:
/// - When at least one cursor has a selection, the selected texts are joined
///   with the document line ending, in document order (collapsed cursors
///   contribute empty entries).
/// - When every cursor is collapsed, each cursor copies its whole line
///   (with a trailing line ending). Cursors sharing a line contribute that
///   line once, matching the cut path, which can only remove a line once.
pub fn copy_text(document: &Document, cursor: &CursorState) -> String {
    let line_ending = document.line_ending().as_str();
    let mut selections: Vec<Selection> = cursor.all_selections().copied().collect();
    selections.sort_by_key(Selection::start);

    if selections.iter().all(Selection::is_collapsed) {
        let mut lines: Vec<usize> = selections.iter().map(|sel| sel.head.line).collect();
        lines.dedup(); // Selections are sorted, so duplicates are consecutive.
        let mut text = String::new();
        for &line in &lines {
            text.push_str(&document.line(line).unwrap_or_default());
            text.push_str(line_ending);
        }
        text
    } else {
        selections
            .iter()
            .map(|sel| document.slice(sel.range()))
            .collect::<Vec<_>>()
            .join(line_ending)
    }
}

/// Builds one deletion edit per cursor for a cut operation.
///
/// Mirrors [`copy_text`]: when every cursor is collapsed each cursor cuts its
/// whole line (cursors sharing a line collapse to a single removal via
/// overlap clamping); otherwise each cursor deletes its selection.
pub fn cut_edits(document: &Document, cursor: &CursorState) -> Vec<CursorEdit> {
    let all_collapsed = cursor.all_selections().all(Selection::is_collapsed);
    cursor
        .all_selections()
        .map(|sel| {
            let range = if all_collapsed {
                line_cut_range(document, sel.head.line)
            } else {
                sel.range()
            };
            CursorEdit::delete(range)
        })
        .collect()
}

/// Returns the range removed when cutting an entire line.
///
/// Includes the trailing line ending, except on the last line (which has
/// none), where only the line content is removed.
fn line_cut_range(document: &Document, line: usize) -> Range {
    if line < document.line_count().saturating_sub(1) {
        // Interior line: remove the line together with its own line ending.
        return Range::new(Position::new(line, 0), Position::new(line + 1, 0));
    }

    // Last line: there is no trailing line ending to consume, so remove the
    // PRECEDING line ending instead — cutting the last line must make it
    // disappear, not leave an empty line behind.
    let start = if line > 0 {
        Position::new(line - 1, document.line_len(line - 1).unwrap_or(0))
    } else {
        Position::new(line, 0)
    };
    Range::new(
        start,
        Position::new(line, document.line_len(line).unwrap_or(0)),
    )
}
