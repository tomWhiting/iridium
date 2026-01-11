//! Cursor navigation and movement.
//!
//! This module provides cursor navigation functions for the editor,
//! including character, word, line, and document-level movements.

use crate::buffer::Buffer;

use super::Position;

/// Word boundary detection for cursor navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordBoundary {
    /// Start of a word (first alphanumeric after whitespace/punctuation).
    Start,
    /// End of a word (last alphanumeric before whitespace/punctuation).
    End,
}

/// Returns true if the character is a word character (alphanumeric or underscore).
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Finds the next word boundary from the given character position.
///
/// Returns the character index of the next word boundary.
pub fn find_word_boundary_forward(buffer: &Buffer, char_idx: usize) -> usize {
    let total_chars = buffer.len_chars();
    if char_idx >= total_chars {
        return total_chars;
    }

    let mut idx = char_idx;

    // Skip current word characters
    while idx < total_chars && is_word_char(buffer.char_at(idx)) {
        idx += 1;
    }

    // Skip non-word characters (whitespace, punctuation)
    while idx < total_chars && !is_word_char(buffer.char_at(idx)) {
        idx += 1;
    }

    idx
}

/// Finds the previous word boundary from the given character position.
///
/// Returns the character index of the previous word boundary.
pub fn find_word_boundary_backward(buffer: &Buffer, char_idx: usize) -> usize {
    if char_idx == 0 {
        return 0;
    }

    let mut idx = char_idx;

    // Move back from current position
    idx = idx.saturating_sub(1);

    // Skip non-word characters (whitespace, punctuation)
    while idx > 0 && !is_word_char(buffer.char_at(idx)) {
        idx -= 1;
    }

    // If we're on a non-word char and at position 0, just return 0
    if idx == 0 && !is_word_char(buffer.char_at(idx)) {
        return 0;
    }

    // Skip word characters to find the start of the word
    while idx > 0 && is_word_char(buffer.char_at(idx.saturating_sub(1))) {
        idx -= 1;
    }

    idx
}

/// Returns the start and end character indices for the word at the given position.
///
/// Returns None if the position is not on a word.
pub fn word_at_position(buffer: &Buffer, char_idx: usize) -> Option<(usize, usize)> {
    if char_idx >= buffer.len_chars() {
        return None;
    }

    let current_char = buffer.char_at(char_idx);
    if !is_word_char(current_char) {
        return None;
    }

    // Find start of word
    let mut start = char_idx;
    while start > 0 && is_word_char(buffer.char_at(start.saturating_sub(1))) {
        start -= 1;
    }

    // Find end of word
    let mut end = char_idx;
    let total = buffer.len_chars();
    while end < total && is_word_char(buffer.char_at(end)) {
        end += 1;
    }

    Some((start, end))
}

/// Calculates the position after moving left by one character.
pub fn move_left(buffer: &Buffer, position: Position) -> Position {
    if position.column > 0 {
        // Move left within the line
        Position::new(position.line, position.column - 1)
    } else if position.line > 0 {
        // Move to end of previous line
        let prev_line = position.line - 1;
        let prev_line_len = line_length(buffer, prev_line);
        Position::new(prev_line, prev_line_len)
    } else {
        // Already at start of document
        position
    }
}

/// Calculates the position after moving right by one character.
pub fn move_right(buffer: &Buffer, position: Position) -> Position {
    let line_len = line_length(buffer, position.line);

    if position.column < line_len {
        // Move right within the line
        Position::new(position.line, position.column + 1)
    } else if position.line < buffer.len_lines().saturating_sub(1) {
        // Move to start of next line
        Position::new(position.line + 1, 0)
    } else {
        // Already at end of document
        position
    }
}

/// Calculates the position after moving up by one line.
///
/// The `preferred_column` is used to try to maintain horizontal position
/// when moving through lines of varying length.
pub fn move_up(buffer: &Buffer, position: Position, preferred_column: usize) -> Position {
    if position.line == 0 {
        // Already at first line, move to start
        Position::new(0, 0)
    } else {
        let new_line = position.line - 1;
        let new_line_len = line_length(buffer, new_line);
        let new_column = preferred_column.min(new_line_len);
        Position::new(new_line, new_column)
    }
}

/// Calculates the position after moving down by one line.
///
/// The `preferred_column` is used to try to maintain horizontal position
/// when moving through lines of varying length.
pub fn move_down(buffer: &Buffer, position: Position, preferred_column: usize) -> Position {
    let last_line = buffer.len_lines().saturating_sub(1);

    if position.line >= last_line {
        // Already at last line, move to end
        let line_len = line_length(buffer, last_line);
        Position::new(last_line, line_len)
    } else {
        let new_line = position.line + 1;
        let new_line_len = line_length(buffer, new_line);
        let new_column = preferred_column.min(new_line_len);
        Position::new(new_line, new_column)
    }
}

/// Calculates the position at the start of the line.
pub fn move_to_line_start(position: Position) -> Position {
    Position::new(position.line, 0)
}

/// Calculates the position at the end of the line.
pub fn move_to_line_end(buffer: &Buffer, position: Position) -> Position {
    let line_len = line_length(buffer, position.line);
    Position::new(position.line, line_len)
}

/// Calculates the position at the start of the document.
pub fn move_to_document_start() -> Position {
    Position::origin()
}

/// Calculates the position at the end of the document.
pub fn move_to_document_end(buffer: &Buffer) -> Position {
    let last_line = buffer.len_lines().saturating_sub(1);
    let line_len = line_length(buffer, last_line);
    Position::new(last_line, line_len)
}

/// Calculates the position after moving left by one word.
pub fn move_word_left(buffer: &Buffer, position: Position) -> Position {
    // Convert position to char index
    let char_idx = position_to_char(buffer, position);
    let new_char_idx = find_word_boundary_backward(buffer, char_idx);
    char_to_position(buffer, new_char_idx)
}

/// Calculates the position after moving right by one word.
pub fn move_word_right(buffer: &Buffer, position: Position) -> Position {
    // Convert position to char index
    let char_idx = position_to_char(buffer, position);
    let new_char_idx = find_word_boundary_forward(buffer, char_idx);
    char_to_position(buffer, new_char_idx)
}

/// Calculates the position after moving up by a page.
///
/// `page_lines` is the number of lines in a page (viewport height).
pub fn page_up(
    buffer: &Buffer,
    position: Position,
    page_lines: usize,
    preferred_column: usize,
) -> Position {
    if position.line == 0 {
        Position::new(0, 0)
    } else {
        let new_line = position.line.saturating_sub(page_lines);
        let new_line_len = line_length(buffer, new_line);
        let new_column = preferred_column.min(new_line_len);
        Position::new(new_line, new_column)
    }
}

/// Calculates the position after moving down by a page.
///
/// `page_lines` is the number of lines in a page (viewport height).
pub fn page_down(
    buffer: &Buffer,
    position: Position,
    page_lines: usize,
    preferred_column: usize,
) -> Position {
    let last_line = buffer.len_lines().saturating_sub(1);
    let new_line = (position.line + page_lines).min(last_line);
    let new_line_len = line_length(buffer, new_line);
    let new_column = preferred_column.min(new_line_len);
    Position::new(new_line, new_column)
}

/// Returns the length of a line (excluding newline character).
pub fn line_length(buffer: &Buffer, line_idx: usize) -> usize {
    if line_idx >= buffer.len_lines() {
        return 0;
    }

    let line = buffer.line(line_idx);
    let len = line.len_chars();

    // Exclude trailing newline if present
    if len > 0 {
        let last_char_idx = len - 1;
        // Convert RopeSlice index to char
        let slice_chars: Vec<char> = line.chars().collect();
        if !slice_chars.is_empty() && slice_chars[last_char_idx] == '\n' {
            return len - 1;
        }
    }

    len
}

/// Converts a Position to a character index.
pub fn position_to_char(buffer: &Buffer, position: Position) -> usize {
    if position.line >= buffer.len_lines() {
        return buffer.len_chars();
    }

    let line_start = buffer.line_to_char(position.line);
    let line_len = line_length(buffer, position.line);
    let column = position.column.min(line_len);

    line_start + column
}

/// Converts a character index to a Position.
pub fn char_to_position(buffer: &Buffer, char_idx: usize) -> Position {
    let clamped_idx = char_idx.min(buffer.len_chars());
    let line = buffer.char_to_line(clamped_idx);
    let line_start = buffer.line_to_char(line);
    let column = clamped_idx - line_start;

    Position::new(line, column)
}

/// Expands the selection to include the entire word at the given position.
///
/// Returns (start, end) positions for the word selection.
pub fn select_word_at(buffer: &Buffer, position: Position) -> (Position, Position) {
    let char_idx = position_to_char(buffer, position);

    if let Some((start, end)) = word_at_position(buffer, char_idx) {
        (
            char_to_position(buffer, start),
            char_to_position(buffer, end),
        )
    } else {
        // Not on a word, select the character at position
        let next = move_right(buffer, position);
        (position, next)
    }
}

/// Expands the selection to include the entire line at the given position.
///
/// Returns (start, end) positions for the line selection (including newline).
pub fn select_line_at(buffer: &Buffer, position: Position) -> (Position, Position) {
    let line_start = Position::new(position.line, 0);

    let line_end = if position.line < buffer.len_lines().saturating_sub(1) {
        // Include the newline, selection extends to start of next line
        Position::new(position.line + 1, 0)
    } else {
        // Last line, select to end
        move_to_line_end(buffer, position)
    };

    (line_start, line_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_buffer(text: &str) -> Buffer {
        Buffer::from(text)
    }

    #[test]
    fn test_is_word_char() {
        assert!(is_word_char('a'));
        assert!(is_word_char('Z'));
        assert!(is_word_char('0'));
        assert!(is_word_char('_'));
        assert!(!is_word_char(' '));
        assert!(!is_word_char('.'));
        assert!(!is_word_char('\n'));
    }

    #[test]
    fn test_find_word_boundary_forward() {
        let buffer = make_buffer("hello world foo");

        // From start of "hello"
        assert_eq!(find_word_boundary_forward(&buffer, 0), 6); // "world"
                                                               // From middle of "hello"
        assert_eq!(find_word_boundary_forward(&buffer, 2), 6); // "world"
                                                               // From start of "world"
        assert_eq!(find_word_boundary_forward(&buffer, 6), 12); // "foo"
                                                                // From end
        assert_eq!(find_word_boundary_forward(&buffer, 15), 15);
    }

    #[test]
    fn test_find_word_boundary_backward() {
        let buffer = make_buffer("hello world foo");

        // From end
        assert_eq!(find_word_boundary_backward(&buffer, 15), 12); // start of "foo"
                                                                  // From start of "foo"
        assert_eq!(find_word_boundary_backward(&buffer, 12), 6); // start of "world"
                                                                 // From middle of "world"
        assert_eq!(find_word_boundary_backward(&buffer, 8), 6); // start of "world"
                                                                // From start of "world"
        assert_eq!(find_word_boundary_backward(&buffer, 6), 0); // start of "hello"
                                                                // From start
        assert_eq!(find_word_boundary_backward(&buffer, 0), 0);
    }

    #[test]
    fn test_word_at_position() {
        let buffer = make_buffer("hello world");

        // On "hello"
        assert_eq!(word_at_position(&buffer, 2), Some((0, 5)));
        // On "world"
        assert_eq!(word_at_position(&buffer, 8), Some((6, 11)));
        // On space (not a word)
        assert_eq!(word_at_position(&buffer, 5), None);
    }

    #[test]
    fn test_move_left() {
        let buffer = make_buffer("hello\nworld");

        // Normal movement within line
        let pos = move_left(&buffer, Position::new(0, 3));
        assert_eq!(pos, Position::new(0, 2));

        // At line start, move to previous line end
        let pos = move_left(&buffer, Position::new(1, 0));
        assert_eq!(pos, Position::new(0, 5));

        // At document start, stay
        let pos = move_left(&buffer, Position::new(0, 0));
        assert_eq!(pos, Position::new(0, 0));
    }

    #[test]
    fn test_move_right() {
        let buffer = make_buffer("hello\nworld");

        // Normal movement within line
        let pos = move_right(&buffer, Position::new(0, 3));
        assert_eq!(pos, Position::new(0, 4));

        // At line end, move to next line start
        let pos = move_right(&buffer, Position::new(0, 5));
        assert_eq!(pos, Position::new(1, 0));

        // At document end, stay
        let pos = move_right(&buffer, Position::new(1, 5));
        assert_eq!(pos, Position::new(1, 5));
    }

    #[test]
    fn test_move_up() {
        let buffer = make_buffer("hello\nworldworld\nfoo");

        // Normal movement
        let pos = move_up(&buffer, Position::new(1, 5), 5);
        assert_eq!(pos, Position::new(0, 5));

        // Preferred column clamped to line length
        let pos = move_up(&buffer, Position::new(1, 10), 10);
        assert_eq!(pos, Position::new(0, 5)); // "hello" is only 5 chars

        // At first line, move to start
        let pos = move_up(&buffer, Position::new(0, 3), 3);
        assert_eq!(pos, Position::new(0, 0));
    }

    #[test]
    fn test_move_down() {
        let buffer = make_buffer("hello\nworldworld\nfoo");

        // Normal movement
        let pos = move_down(&buffer, Position::new(0, 5), 5);
        assert_eq!(pos, Position::new(1, 5));

        // Preferred column clamped to line length
        let pos = move_down(&buffer, Position::new(1, 10), 10);
        assert_eq!(pos, Position::new(2, 3)); // "foo" is only 3 chars

        // At last line, move to end
        let pos = move_down(&buffer, Position::new(2, 1), 1);
        assert_eq!(pos, Position::new(2, 3));
    }

    #[test]
    fn test_move_to_line_start_end() {
        let buffer = make_buffer("hello\nworld");

        let pos = move_to_line_start(Position::new(0, 3));
        assert_eq!(pos, Position::new(0, 0));

        let pos = move_to_line_end(&buffer, Position::new(0, 0));
        assert_eq!(pos, Position::new(0, 5));
    }

    #[test]
    fn test_move_to_document_start_end() {
        let buffer = make_buffer("hello\nworld\nfoo");

        let pos = move_to_document_start();
        assert_eq!(pos, Position::new(0, 0));

        let pos = move_to_document_end(&buffer);
        assert_eq!(pos, Position::new(2, 3));
    }

    #[test]
    fn test_move_word_left_right() {
        let buffer = make_buffer("hello world foo");

        let pos = move_word_right(&buffer, Position::new(0, 0));
        assert_eq!(pos, Position::new(0, 6)); // start of "world"

        let pos = move_word_left(&buffer, Position::new(0, 12));
        assert_eq!(pos, Position::new(0, 6)); // start of "world"
    }

    #[test]
    fn test_page_up_down() {
        let buffer = make_buffer("line1\nline2\nline3\nline4\nline5\nline6");

        let pos = page_down(&buffer, Position::new(0, 0), 3, 0);
        assert_eq!(pos.line, 3);

        let pos = page_up(&buffer, Position::new(5, 0), 3, 0);
        assert_eq!(pos.line, 2);
    }

    #[test]
    fn test_line_length() {
        let buffer = make_buffer("hello\nworld\n");

        assert_eq!(line_length(&buffer, 0), 5); // "hello"
        assert_eq!(line_length(&buffer, 1), 5); // "world"
        assert_eq!(line_length(&buffer, 2), 0); // empty line after trailing newline
    }

    #[test]
    fn test_position_to_char_and_back() {
        let buffer = make_buffer("hello\nworld");

        let pos = Position::new(0, 3);
        let char_idx = position_to_char(&buffer, pos);
        let back = char_to_position(&buffer, char_idx);
        assert_eq!(pos, back);

        let pos = Position::new(1, 2);
        let char_idx = position_to_char(&buffer, pos);
        let back = char_to_position(&buffer, char_idx);
        assert_eq!(pos, back);
    }

    #[test]
    fn test_select_word_at() {
        let buffer = make_buffer("hello world");

        let (start, end) = select_word_at(&buffer, Position::new(0, 2));
        assert_eq!(start, Position::new(0, 0));
        assert_eq!(end, Position::new(0, 5));
    }

    #[test]
    fn test_select_line_at() {
        let buffer = make_buffer("hello\nworld\nfoo");

        // First line
        let (start, end) = select_line_at(&buffer, Position::new(0, 2));
        assert_eq!(start, Position::new(0, 0));
        assert_eq!(end, Position::new(1, 0));

        // Last line
        let (start, end) = select_line_at(&buffer, Position::new(2, 1));
        assert_eq!(start, Position::new(2, 0));
        assert_eq!(end, Position::new(2, 3));
    }
}
