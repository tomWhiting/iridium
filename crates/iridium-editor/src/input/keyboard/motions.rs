//! Per-cursor motion primitives for keyboard navigation.
//!
//! Every motion in this module operates on a single [`Position`] so that the
//! keyboard handler (and, later, other input paths) can apply the same motion
//! independently to each cursor of a multi-cursor [`CursorState`].
//!
//! [`apply_to_all`] is the shared applicator: it computes a new head for the
//! primary cursor and every secondary cursor, optionally extending the
//! existing selections, and merges any cursors that end up overlapping or
//! adjacent (via [`CursorState::add_cursor`], which maintains the sorted,
//! non-overlapping invariants of the cursor state).

use crate::document::{CursorState, Document, Position, Selection};

/// Returns true for characters that belong to a word (alphanumeric or `_`).
///
/// This is the single word-character definition shared by word motions,
/// word-wise deletion, and the auto-pair quote suppression in
/// [`super::behaviors`].
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
    let mut state = CursorState::new(make(&cursor.primary, primary_head));

    for (idx, sel) in cursor.secondary.iter().enumerate() {
        let head = new_head(idx + 1, sel);
        state.add_cursor(make(sel, head));
    }

    state
}

/// Moves a position one character to the left (wrapping to the end of the
/// previous line at column 0).
pub fn char_left(document: &Document, head: Position) -> Position {
    if head.column > 0 {
        Position::new(head.line, head.column - 1)
    } else if head.line > 0 {
        let prev_line = head.line - 1;
        let line_len = document.line_len(prev_line).unwrap_or(0);
        Position::new(prev_line, line_len)
    } else {
        head // Already at start of document
    }
}

/// Moves a position one character to the right (wrapping to the start of the
/// next line at end of line).
pub fn char_right(document: &Document, head: Position) -> Position {
    let line_len = document.line_len(head.line).unwrap_or(0);

    if head.column < line_len {
        Position::new(head.line, head.column + 1)
    } else if head.line < document.line_count().saturating_sub(1) {
        Position::new(head.line + 1, 0)
    } else {
        head // Already at end of document
    }
}

/// Finds the word boundary to the left of a position.
pub fn word_left(document: &Document, pos: Position) -> Position {
    // If at start of line, go to end of previous line
    if pos.column == 0 {
        if pos.line == 0 {
            return pos;
        }
        let prev_line = pos.line - 1;
        let line_len = document.line_len(prev_line).unwrap_or(0);
        return Position::new(prev_line, line_len);
    }

    let line_text = document.line(pos.line).unwrap_or_default();
    let chars: Vec<char> = line_text.chars().collect();
    let mut col = pos.column.min(chars.len());

    // Skip whitespace
    while col > 0
        && chars
            .get(col.saturating_sub(1))
            .is_some_and(|c| c.is_whitespace())
    {
        col -= 1;
    }

    // Skip word characters
    if col > 0
        && chars
            .get(col.saturating_sub(1))
            .is_some_and(|c| is_word_char(*c))
    {
        while col > 0
            && chars
                .get(col.saturating_sub(1))
                .is_some_and(|c| is_word_char(*c))
        {
            col -= 1;
        }
    } else {
        // Skip non-word, non-whitespace characters (punctuation)
        while col > 0
            && chars
                .get(col.saturating_sub(1))
                .is_some_and(|c| !is_word_char(*c) && !c.is_whitespace())
        {
            col -= 1;
        }
    }

    Position::new(pos.line, col)
}

/// Finds the word boundary to the right of a position.
pub fn word_right(document: &Document, pos: Position) -> Position {
    let line_text = document.line(pos.line).unwrap_or_default();
    let chars: Vec<char> = line_text.chars().collect();
    let line_len = chars.len();

    // If at end of line, go to start of next line
    if pos.column >= line_len {
        if pos.line >= document.line_count().saturating_sub(1) {
            return pos;
        }
        return Position::new(pos.line + 1, 0);
    }

    let mut col = pos.column;

    // Skip current word or punctuation
    if chars.get(col).is_some_and(|c| is_word_char(*c)) {
        while col < line_len && chars.get(col).is_some_and(|c| is_word_char(*c)) {
            col += 1;
        }
    } else if chars.get(col).is_some_and(|c| !c.is_whitespace()) {
        while col < line_len
            && chars
                .get(col)
                .is_some_and(|c| !is_word_char(*c) && !c.is_whitespace())
        {
            col += 1;
        }
    }

    // Skip whitespace
    while col < line_len && chars.get(col).is_some_and(|c| c.is_whitespace()) {
        col += 1;
    }

    Position::new(pos.line, col)
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

    Position::new(head.line, new_column)
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
                    Position::new(line, target_column.min(line_len)),
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
                    Position::new(line, target_column.min(line_len)),
                    target_column,
                )
            }
        },
    }
}

/// Selects the word at the given position.
///
/// Returns a collapsed selection when the position is not on a word
/// character.
pub fn select_word_at(document: &Document, position: Position) -> Selection {
    let line_text = document.line(position.line).unwrap_or_default();
    let chars: Vec<char> = line_text.chars().collect();

    if chars.is_empty() {
        return Selection::collapsed(position);
    }

    let col = position.column.min(chars.len().saturating_sub(1));

    // Check if we're on a word character
    if !chars.get(col).is_some_and(|c| is_word_char(*c)) {
        return Selection::collapsed(position);
    }

    // Find word boundaries
    let mut start = col;
    let mut end = col;

    while start > 0 && chars.get(start - 1).is_some_and(|c| is_word_char(*c)) {
        start -= 1;
    }
    while end < chars.len() && chars.get(end).is_some_and(|c| is_word_char(*c)) {
        end += 1;
    }

    Selection::new(
        Position::new(position.line, start),
        Position::new(position.line, end),
    )
}
