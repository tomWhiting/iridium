//! Per-cursor edit *intents*: what each cursor wants removed and inserted.
//!
//! Every verb in this file answers the same question — "given the cursors as
//! they are, what does each one edit?" — and answers it as a `Vec<CursorEdit>`
//! in cursor order, one entry per cursor. None of them touch the document or
//! the history: turning the intents into a single reversible [`Command`] is
//! [`super::build_multi_cursor_command`]'s job, and keeping the two halves
//! apart is what lets every verb here be read and tested as a pure function of
//! the cursor state.
//!
//! [`Command`]: crate::history::Command

use crate::document::{CursorState, Document, Position, Range, Selection};

use super::super::motions;
use super::CursorEdit;

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

/// Builds one delete-to-line-start edit per cursor.
///
/// Cursors with a selection delete the selection, exactly as
/// [`backspace_edits`] does — this is the macOS `Cmd+Backspace` verb, and a
/// delete key that ignored an existing selection would be the only delete in
/// the editor that does. Collapsed cursors delete from column zero to the
/// caret; a caret already at column zero contributes an empty (no-op) edit.
///
/// Two cursors on the same line contribute two overlapping deletes, which
/// [`build_multi_cursor_command_placed`](super::build_multi_cursor_command_placed)
/// clamps front to back, so the line's
/// prefix is removed once.
pub fn delete_to_line_start_edits(cursor: &CursorState) -> Vec<CursorEdit> {
    cursor
        .all_selections()
        .map(|sel| {
            if sel.is_collapsed() {
                CursorEdit::delete(Range::new(Position::new(sel.head.line, 0), sel.head))
            } else {
                CursorEdit::delete(sel.range())
            }
        })
        .collect()
}

/// Builds one delete-to-line-end edit per cursor.
///
/// The mirror of [`delete_to_line_start_edits`]: cursors with a selection
/// delete the selection, collapsed cursors delete from the caret to the end of
/// the line's content. The trailing line ending is deliberately **not**
/// consumed — this verb empties a line, it does not join it to the next, which
/// is what `Cmd+Delete` does on macOS and what `lines.join` is for.
pub fn delete_to_line_end_edits(document: &Document, cursor: &CursorState) -> Vec<CursorEdit> {
    cursor
        .all_selections()
        .map(|sel| {
            if sel.is_collapsed() {
                let line_len = document.line_len(sel.head.line).unwrap_or(0);
                CursorEdit::delete(Range::new(
                    sel.head,
                    Position::new(sel.head.line, line_len.max(sel.head.column)),
                ))
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
