//! Per-cursor motion primitives for keyboard navigation.
//!
//! Every motion in this module operates on a single [`Position`] so that the
//! keyboard handler (and, later, other input paths) can apply the same motion
//! independently to each cursor of a multi-cursor [`CursorState`].
//!
//! [`apply_to_all`] is the shared applicator: it computes a new head for the
//! primary cursor and every secondary cursor, optionally extending the
//! existing selections, and merges any cursors that end up overlapping or
//! adjacent (via [`CursorState::from_selections`], which maintains the sorted,
//! non-overlapping invariants of the cursor state).

pub(crate) use super::words::{is_word_cluster, select_run_at};
pub use super::words::{select_word_at, word_left, word_left_byte, word_right, word_right_byte};
use crate::document::{CursorState, Document, Position, Selection};

/// Returns true for characters that belong to a word (alphanumeric or `_`).
///
/// This is the single word-character definition in the editor, and every
/// consumer of the idea reaches for it: word motions, word-wise deletion, the
/// auto-pair quote suppression in [`super::behaviors`], double-click word
/// selection in [`crate::input::mouse`], and whole-word search boundaries in
/// [`crate::search`].
///
/// The last two are here because they were not. Search asked its own question
/// of a single *byte* (`is_ascii_alphanumeric`), which reads every non-ASCII
/// character as a non-word character and made `na` a whole word inside
/// `naïve`; the mouse kept a character-wise copy of this body, which agreed
/// but was free to stop agreeing. A definition that claims to be the only one
/// has to be the only one.
#[must_use]
pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Direction for vertical cursor movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalDirection {
    /// Move to the previous line.
    Up,
    /// Move to the next line.
    Down,
}

/// Applies a motion to every cursor (primary and all secondaries).
///
/// The `new_head` callback receives the cursor's index in
/// [`CursorState::all_selections`] order (0 = primary) and the current
/// selection, and returns the new head position. When `extend` is true the
/// selection anchor is preserved (Shift+motion); otherwise the selection
/// collapses to the new head.
///
/// Cursors that end up overlapping or adjacent after the motion are merged,
/// so the returned state always upholds the multi-cursor invariants.
pub fn apply_to_all<F>(cursor: &CursorState, extend: bool, mut new_head: F) -> CursorState
where
    F: FnMut(usize, &Selection) -> Position,
{
    let make = |sel: &Selection, head: Position| {
        if extend {
            Selection::new(sel.anchor, head)
        } else {
            Selection::collapsed(head)
        }
    };

    let primary_head = new_head(0, &cursor.primary);
    let primary = make(&cursor.primary, primary_head);
    let secondary = cursor
        .secondary
        .iter()
        .enumerate()
        .map(|(idx, sel)| {
            let head = new_head(idx + 1, sel);
            make(sel, head)
        })
        .collect();
    CursorState::from_selections(primary, secondary)
}

/// Moves one extended grapheme left, crossing line endings as a unit.
pub fn char_left(document: &Document, head: Position) -> Position {
    super::graphemes::move_to(document, head, super::graphemes::Boundary::Previous)
}

/// Moves one extended grapheme right, crossing line endings as a unit.
pub fn char_right(document: &Document, head: Position) -> Position {
    super::graphemes::move_to(document, head, super::graphemes::Boundary::Next)
}

/// Moves a position to the start of its line (smart home).
///
/// Toggles between the first non-whitespace column and column 0.
pub fn line_start_smart(document: &Document, head: Position) -> Position {
    let line_text = document.line(head.line).unwrap_or_default();

    // Find first non-whitespace character
    let first_non_ws = line_text
        .chars()
        .position(|c| !c.is_whitespace())
        .unwrap_or(0);

    // Smart home: toggle between first non-ws and column 0
    let new_column = if head.column == first_non_ws || head.column == 0 {
        if head.column == 0 { first_non_ws } else { 0 }
    } else {
        first_non_ws
    };

    snap_to_grapheme(document, Position::new(head.line, new_column))
}

/// Snap an interactive caret before its containing grapheme, if necessary.
pub(crate) fn snap_to_grapheme(document: &Document, position: Position) -> Position {
    super::graphemes::move_to(document, position, super::graphemes::Boundary::Floor)
}

/// Moves a position to the end of its line.
pub fn line_end(document: &Document, head: Position) -> Position {
    let line_len = document.line_len(head.line).unwrap_or(0);
    Position::new(head.line, line_len)
}

/// Returns the start-of-document position.
pub const fn document_start() -> Position {
    Position::zero()
}

/// Returns the end-of-document position.
pub fn document_end(document: &Document) -> Position {
    let last_line = document.line_count().saturating_sub(1);
    let last_col = document.line_len(last_line).unwrap_or(0);
    Position::new(last_line, last_col)
}

/// Moves a position one line up or down, honoring a preferred ("sticky")
/// column.
///
/// Returns the new head position and the preferred column to remember for
/// subsequent vertical moves of the same cursor:
/// - Normal moves keep `target_column` sticky (the head may be clamped to a
///   shorter line, but the target is preserved).
/// - Moving up at the first line goes to `(0, 0)` and resets the sticky
///   column to 0.
/// - Moving down at the last line goes to the line end and resets the sticky
///   column to that end column.
pub fn vertical_move(
    document: &Document,
    head: Position,
    target_column: usize,
    direction: VerticalDirection,
) -> (Position, usize) {
    vertical_move_by(document, head, target_column, direction, 1)
}

/// Moves a position `lines` lines up or down, honoring a preferred ("sticky")
/// column.
///
/// The many-line generalisation of [`vertical_move`], with identical boundary
/// behaviour: a hop that overshoots either end of the document clamps to the
/// boundary line keeping the sticky column, while a move *starting* on a
/// boundary line collapses to the document start (up) or the line end (down),
/// exactly as a single-line move does. A page motion is therefore
/// indistinguishable from a line motion everywhere but the hop size — folds and
/// sticky columns are handled by the caller for both, at the same layer.
pub fn vertical_move_by(
    document: &Document,
    head: Position,
    target_column: usize,
    direction: VerticalDirection,
    lines: usize,
) -> (Position, usize) {
    match direction {
        VerticalDirection::Up => {
            if head.line == 0 {
                // Already at first line: go to document start.
                (Position::new(0, 0), 0)
            } else {
                let line = head.line.saturating_sub(lines);
                let line_len = document.line_len(line).unwrap_or(0);
                (
                    super::graphemes::move_to(
                        document,
                        Position::new(line, target_column.min(line_len)),
                        super::graphemes::Boundary::Floor,
                    ),
                    target_column,
                )
            }
        },
        VerticalDirection::Down => {
            let last_line = document.line_count().saturating_sub(1);
            if head.line >= last_line {
                // Already at last line: go to line end.
                let line_len = document.line_len(last_line).unwrap_or(0);
                (Position::new(last_line, line_len), line_len)
            } else {
                let line = head.line.saturating_add(lines).min(last_line);
                let line_len = document.line_len(line).unwrap_or(0);
                (
                    super::graphemes::move_to(
                        document,
                        Position::new(line, target_column.min(line_len)),
                        super::graphemes::Boundary::Floor,
                    ),
                    target_column,
                )
            }
        },
    }
}
