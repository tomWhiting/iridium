//! Core editor state.
//!
//! The Editor struct manages the primary editing state including the buffer,
//! cursor, and change tracking.

use crate::buffer::Buffer;
use crate::history::{Command, History};

use super::{Cursor, Position, Selection};

/// The core editor state.
///
/// The Editor coordinates the buffer, cursor state, and command history
/// to provide a complete editing experience.
#[derive(Debug)]
pub struct Editor {
    /// The text buffer.
    buffer: Buffer,
    /// The primary cursor.
    cursor: Cursor,
    /// Command history for undo/redo.
    history: History,
    /// Whether the buffer has been modified since last save.
    modified: bool,
}

impl Editor {
    /// Creates a new editor with empty content.
    ///
    /// # Examples
    ///
    /// ```
    /// use iridium_editor::Editor;
    ///
    /// let editor = Editor::new();
    /// assert!(!editor.is_modified());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffer: Buffer::new(),
            cursor: Cursor::default(),
            history: History::new(),
            modified: false,
        }
    }

    /// Creates an editor with the given initial content.
    ///
    /// # Examples
    ///
    /// ```
    /// use iridium_editor::Editor;
    ///
    /// let editor = Editor::with_content("Hello, world!");
    /// assert_eq!(editor.content(), "Hello, world!");
    /// ```
    #[must_use]
    pub fn with_content(content: &str) -> Self {
        Self {
            buffer: Buffer::from(content),
            cursor: Cursor::default(),
            history: History::new(),
            modified: false,
        }
    }

    /// Returns a reference to the buffer.
    #[must_use]
    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    /// Returns the entire content as a string.
    #[must_use]
    pub fn content(&self) -> String {
        self.buffer.to_string()
    }

    /// Returns the current cursor position.
    #[must_use]
    pub fn cursor_position(&self) -> Position {
        self.cursor.position()
    }

    /// Returns the current selection, if any.
    ///
    /// Returns None if the cursor has no selection (just a point).
    #[must_use]
    pub fn selection(&self) -> Option<Selection> {
        if self.cursor.has_selection() {
            Some(self.cursor.selection)
        } else {
            None
        }
    }

    /// Returns true if the buffer has been modified since creation or last save mark.
    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Marks the buffer as unmodified (e.g., after saving).
    pub fn mark_saved(&mut self) {
        self.modified = false;
    }

    /// Returns the number of lines in the buffer.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.buffer.len_lines()
    }

    /// Returns the number of characters in the buffer.
    #[must_use]
    pub fn char_count(&self) -> usize {
        self.buffer.len_chars()
    }

    /// Converts a buffer character index to a Position.
    #[must_use]
    pub fn char_to_position(&self, char_idx: usize) -> Position {
        let line = self.buffer.char_to_line(char_idx);
        let line_start = self.buffer.line_to_char(line);
        let column = char_idx - line_start;
        Position::new(line, column)
    }

    /// Converts a Position to a buffer character index.
    ///
    /// Returns None if the position is out of bounds.
    #[must_use]
    pub fn position_to_char(&self, position: Position) -> Option<usize> {
        if position.line >= self.buffer.len_lines() {
            return None;
        }

        let line_start = self.buffer.line_to_char(position.line);
        let line_len = self.buffer.line(position.line).len_chars();

        // Allow column to be at the end of line (for cursor positioning)
        if position.column > line_len {
            return None;
        }

        Some(line_start + position.column)
    }

    /// Moves the cursor to the specified position.
    ///
    /// Clears any selection.
    pub fn move_cursor_to(&mut self, position: Position) {
        self.cursor.move_to(position);
    }

    /// Extends the selection to the specified position.
    ///
    /// The anchor remains at its current position.
    pub fn extend_selection_to(&mut self, position: Position) {
        self.cursor.extend_to(position);
    }

    /// Selects all text in the buffer.
    pub fn select_all(&mut self) {
        let start = Position::origin();
        let end = self.char_to_position(self.buffer.len_chars());
        self.cursor.set_selection(Selection::new(start, end));
    }

    /// Clears the current selection, keeping the cursor at the head position.
    pub fn clear_selection(&mut self) {
        self.cursor.collapse();
    }

    /// Inserts text at the current cursor position.
    ///
    /// If there is a selection, it is replaced with the inserted text.
    pub fn insert(&mut self, text: &str) {
        let char_idx = if self.cursor.has_selection() {
            // Delete selection first
            let start = self.cursor.selection.start();
            let end = self.cursor.selection.end();
            let start_idx = self.position_to_char(start).unwrap_or(0);
            let end_idx = self
                .position_to_char(end)
                .unwrap_or(self.buffer.len_chars());

            // Record delete command
            let deleted_text = self.buffer.slice(start_idx, end_idx).to_string();
            self.history.push(Command::Delete {
                position: start_idx,
                text: deleted_text,
            });

            self.buffer.remove(start_idx, end_idx);
            self.cursor.move_to(start);
            start_idx
        } else {
            self.position_to_char(self.cursor.position()).unwrap_or(0)
        };

        // Record insert command
        self.history.push(Command::Insert {
            position: char_idx,
            text: text.to_string(),
        });

        self.buffer.insert(char_idx, text);

        // Move cursor to end of inserted text
        let new_position = self.char_to_position(char_idx + text.chars().count());
        self.cursor.move_to(new_position);
        self.modified = true;
    }

    /// Deletes text in the specified range.
    pub fn delete_range(&mut self, start: Position, end: Position) {
        let start_idx = match self.position_to_char(start) {
            Some(idx) => idx,
            None => return,
        };
        let end_idx = match self.position_to_char(end) {
            Some(idx) => idx,
            None => return,
        };

        if start_idx >= end_idx {
            return;
        }

        // Record delete command
        let deleted_text = self.buffer.slice(start_idx, end_idx).to_string();
        self.history.push(Command::Delete {
            position: start_idx,
            text: deleted_text,
        });

        self.buffer.remove(start_idx, end_idx);
        self.cursor.move_to(start);
        self.modified = true;
    }

    /// Deletes the current selection, if any.
    ///
    /// Returns true if text was deleted.
    pub fn delete_selection(&mut self) -> bool {
        if !self.cursor.has_selection() {
            return false;
        }

        let start = self.cursor.selection.start();
        let end = self.cursor.selection.end();
        self.delete_range(start, end);
        true
    }

    /// Deletes the character before the cursor (backspace).
    ///
    /// If there is a selection, deletes the selection instead.
    pub fn backspace(&mut self) {
        if self.delete_selection() {
            return;
        }

        let position = self.cursor.position();
        if position.is_origin() {
            return; // Nothing to delete
        }

        let char_idx = self.position_to_char(position).unwrap_or(0);
        if char_idx == 0 {
            return;
        }

        let prev_idx = char_idx - 1;
        let deleted_char = self.buffer.char_at(prev_idx).to_string();

        // Record delete command
        self.history.push(Command::Delete {
            position: prev_idx,
            text: deleted_char,
        });

        self.buffer.remove(prev_idx, char_idx);
        self.cursor.move_to(self.char_to_position(prev_idx));
        self.modified = true;
    }

    /// Deletes the character after the cursor (delete key).
    ///
    /// If there is a selection, deletes the selection instead.
    pub fn delete(&mut self) {
        if self.delete_selection() {
            return;
        }

        let position = self.cursor.position();
        let char_idx = self.position_to_char(position).unwrap_or(0);

        if char_idx >= self.buffer.len_chars() {
            return; // Nothing to delete
        }

        let deleted_char = self.buffer.char_at(char_idx).to_string();

        // Record delete command
        self.history.push(Command::Delete {
            position: char_idx,
            text: deleted_char,
        });

        self.buffer.remove(char_idx, char_idx + 1);
        self.modified = true;
    }

    /// Undoes the last command.
    ///
    /// Returns true if a command was undone.
    pub fn undo(&mut self) -> bool {
        if let Some(command) = self.history.undo() {
            command.unapply(&mut self.buffer);
            self.modified = true;
            true
        } else {
            false
        }
    }

    /// Redoes the last undone command.
    ///
    /// Returns true if a command was redone.
    pub fn redo(&mut self) -> bool {
        if let Some(command) = self.history.redo() {
            command.apply(&mut self.buffer);
            self.modified = true;
            true
        } else {
            false
        }
    }

    /// Returns true if undo is available.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Returns true if redo is available.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let editor = Editor::new();
        assert_eq!(editor.content(), "");
        assert!(!editor.is_modified());
        assert_eq!(editor.cursor_position(), Position::origin());
    }

    #[test]
    fn test_with_content() {
        let editor = Editor::with_content("Hello");
        assert_eq!(editor.content(), "Hello");
        assert!(!editor.is_modified());
    }

    #[test]
    fn test_insert() {
        let mut editor = Editor::new();
        editor.insert("Hello");
        assert_eq!(editor.content(), "Hello");
        assert!(editor.is_modified());
        assert_eq!(editor.cursor_position(), Position::new(0, 5));
    }

    #[test]
    fn test_insert_multiline() {
        let mut editor = Editor::new();
        editor.insert("Hello\nWorld");
        assert_eq!(editor.content(), "Hello\nWorld");
        assert_eq!(editor.cursor_position(), Position::new(1, 5));
    }

    #[test]
    fn test_backspace() {
        let mut editor = Editor::with_content("Hello");
        editor.move_cursor_to(Position::new(0, 5));
        editor.backspace();
        assert_eq!(editor.content(), "Hell");
        assert_eq!(editor.cursor_position(), Position::new(0, 4));
    }

    #[test]
    fn test_backspace_at_start() {
        let mut editor = Editor::with_content("Hello");
        editor.move_cursor_to(Position::origin());
        editor.backspace();
        assert_eq!(editor.content(), "Hello"); // No change
    }

    #[test]
    fn test_delete() {
        let mut editor = Editor::with_content("Hello");
        editor.move_cursor_to(Position::origin());
        editor.delete();
        assert_eq!(editor.content(), "ello");
        assert_eq!(editor.cursor_position(), Position::origin());
    }

    #[test]
    fn test_delete_at_end() {
        let mut editor = Editor::with_content("Hello");
        editor.move_cursor_to(Position::new(0, 5));
        editor.delete();
        assert_eq!(editor.content(), "Hello"); // No change
    }

    #[test]
    fn test_selection_and_insert() {
        let mut editor = Editor::with_content("Hello, world!");
        editor.move_cursor_to(Position::new(0, 7));
        editor.extend_selection_to(Position::new(0, 12));
        editor.insert("Rust");
        assert_eq!(editor.content(), "Hello, Rust!");
    }

    #[test]
    fn test_select_all() {
        let mut editor = Editor::with_content("Hello");
        editor.select_all();
        assert!(editor.selection().is_some());
        let sel = editor.selection().unwrap();
        assert_eq!(sel.start(), Position::origin());
        assert_eq!(sel.end(), Position::new(0, 5));
    }

    #[test]
    fn test_undo_redo() {
        let mut editor = Editor::new();
        editor.insert("Hello");
        assert_eq!(editor.content(), "Hello");

        editor.undo();
        assert_eq!(editor.content(), "");

        editor.redo();
        assert_eq!(editor.content(), "Hello");
    }

    #[test]
    fn test_undo_multiple() {
        let mut editor = Editor::new();
        // Insert non-adjacent to prevent merging
        editor.insert("A");
        editor.move_cursor_to(Position::new(0, 0)); // Move to start
        editor.insert("B"); // Insert at position 0
        editor.move_cursor_to(Position::new(0, 0));
        editor.insert("C"); // Insert at position 0
        assert_eq!(editor.content(), "CBA");

        // Each insert is separate because they're at different positions
        editor.undo();
        assert_eq!(editor.content(), "BA");
        editor.undo();
        assert_eq!(editor.content(), "A");
        editor.undo();
        assert_eq!(editor.content(), "");
    }

    #[test]
    fn test_can_undo_redo() {
        let mut editor = Editor::new();
        assert!(!editor.can_undo());
        assert!(!editor.can_redo());

        editor.insert("X");
        assert!(editor.can_undo());
        assert!(!editor.can_redo());

        editor.undo();
        assert!(!editor.can_undo());
        assert!(editor.can_redo());
    }

    #[test]
    fn test_mark_saved() {
        let mut editor = Editor::new();
        editor.insert("Hello");
        assert!(editor.is_modified());
        editor.mark_saved();
        assert!(!editor.is_modified());
        editor.insert("!");
        assert!(editor.is_modified());
    }

    #[test]
    fn test_char_to_position() {
        let editor = Editor::with_content("Hello\nWorld");
        assert_eq!(editor.char_to_position(0), Position::new(0, 0));
        assert_eq!(editor.char_to_position(5), Position::new(0, 5));
        assert_eq!(editor.char_to_position(6), Position::new(1, 0));
        assert_eq!(editor.char_to_position(11), Position::new(1, 5));
    }

    #[test]
    fn test_position_to_char() {
        let editor = Editor::with_content("Hello\nWorld");
        assert_eq!(editor.position_to_char(Position::new(0, 0)), Some(0));
        assert_eq!(editor.position_to_char(Position::new(0, 5)), Some(5));
        assert_eq!(editor.position_to_char(Position::new(1, 0)), Some(6));
        assert_eq!(editor.position_to_char(Position::new(1, 5)), Some(11));
        assert_eq!(editor.position_to_char(Position::new(5, 0)), None); // Out of bounds
    }
}
