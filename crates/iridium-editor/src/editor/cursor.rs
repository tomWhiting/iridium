//! Cursor state and multi-cursor management.
//!
//! This module provides cursor state management, including support for
//! multiple cursors editing simultaneously.

use super::{Position, Selection};

/// A cursor in the editor, which is essentially a selection.
///
/// The cursor wraps a selection and provides cursor-specific operations.
/// When no text is selected, the cursor is at the head position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Cursor {
    /// The underlying selection (or cursor position if collapsed).
    pub selection: Selection,
    /// The preferred column when moving vertically.
    ///
    /// When moving up/down, the cursor tries to maintain this column
    /// even if the current line is shorter. This is set when moving
    /// horizontally and preserved when moving vertically.
    preferred_column: Option<usize>,
}

impl Cursor {
    /// Creates a new cursor at the given position.
    ///
    /// # Examples
    ///
    /// ```
    /// use iridium_editor::{Cursor, Position};
    ///
    /// let cursor = Cursor::new(Position::new(0, 5));
    /// assert_eq!(cursor.position(), Position::new(0, 5));
    /// ```
    #[must_use]
    pub fn new(position: Position) -> Self {
        Self {
            selection: Selection::cursor(position),
            preferred_column: None,
        }
    }

    /// Creates a cursor with a selection.
    #[must_use]
    pub fn with_selection(selection: Selection) -> Self {
        Self {
            selection,
            preferred_column: None,
        }
    }

    /// Returns the current cursor position (the head of the selection).
    #[must_use]
    pub fn position(&self) -> Position {
        self.selection.head
    }

    /// Returns the current line number.
    #[must_use]
    pub fn line(&self) -> usize {
        self.selection.head.line
    }

    /// Returns the current column number.
    #[must_use]
    pub fn column(&self) -> usize {
        self.selection.head.column
    }

    /// Returns true if there is an active selection (not just a cursor).
    #[must_use]
    pub fn has_selection(&self) -> bool {
        self.selection.is_selection()
    }

    /// Returns the preferred column for vertical movement.
    ///
    /// If no preferred column is set, returns the current column.
    #[must_use]
    pub fn preferred_column(&self) -> usize {
        self.preferred_column.unwrap_or(self.column())
    }

    /// Sets the preferred column for vertical movement.
    pub fn set_preferred_column(&mut self, column: usize) {
        self.preferred_column = Some(column);
    }

    /// Clears the preferred column, so it will be recalculated on next horizontal move.
    pub fn clear_preferred_column(&mut self) {
        self.preferred_column = None;
    }

    /// Moves the cursor to a new position, clearing any selection.
    ///
    /// This also clears the preferred column since we're moving horizontally.
    pub fn move_to(&mut self, position: Position) {
        self.selection = Selection::cursor(position);
        self.preferred_column = None;
    }

    /// Moves the cursor to a new position while keeping the preferred column.
    ///
    /// Use this for vertical movement where we want to maintain column preference.
    pub fn move_to_preserving_column(&mut self, position: Position) {
        // Store current preferred column if not set
        if self.preferred_column.is_none() {
            self.preferred_column = Some(self.column());
        }
        self.selection = Selection::cursor(position);
    }

    /// Extends the selection to the new position.
    ///
    /// The anchor remains at its current position; only the head moves.
    pub fn extend_to(&mut self, position: Position) {
        self.selection = self.selection.extend_to(position);
    }

    /// Collapses the selection, placing the cursor at the head position.
    pub fn collapse(&mut self) {
        self.selection = self.selection.collapse_to_head();
    }

    /// Collapses the selection, placing the cursor at the start of the selection.
    pub fn collapse_to_start(&mut self) {
        self.selection = self.selection.collapse_to_start();
    }

    /// Collapses the selection, placing the cursor at the end of the selection.
    pub fn collapse_to_end(&mut self) {
        self.selection = self.selection.collapse_to_end();
    }

    /// Sets the selection directly.
    pub fn set_selection(&mut self, selection: Selection) {
        self.selection = selection;
    }
}

impl From<Position> for Cursor {
    fn from(position: Position) -> Self {
        Self::new(position)
    }
}

impl From<Selection> for Cursor {
    fn from(selection: Selection) -> Self {
        Self::with_selection(selection)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let cursor = Cursor::new(Position::new(5, 10));
        assert_eq!(cursor.position(), Position::new(5, 10));
        assert_eq!(cursor.line(), 5);
        assert_eq!(cursor.column(), 10);
        assert!(!cursor.has_selection());
    }

    #[test]
    fn test_with_selection() {
        let selection = Selection::new(Position::new(0, 0), Position::new(0, 10));
        let cursor = Cursor::with_selection(selection);
        assert!(cursor.has_selection());
        assert_eq!(cursor.position(), Position::new(0, 10)); // head
    }

    #[test]
    fn test_preferred_column() {
        let mut cursor = Cursor::new(Position::new(0, 10));

        // Initially uses current column
        assert_eq!(cursor.preferred_column(), 10);

        // Can be set explicitly
        cursor.set_preferred_column(15);
        assert_eq!(cursor.preferred_column(), 15);

        // Move to new position
        cursor.move_to(Position::new(1, 5));
        // preferred_column cleared on horizontal move
        assert_eq!(cursor.preferred_column(), 5);
    }

    #[test]
    fn test_move_to_preserving_column() {
        let mut cursor = Cursor::new(Position::new(0, 10));

        // Move vertically, preserving column preference
        cursor.move_to_preserving_column(Position::new(1, 5));
        assert_eq!(cursor.position(), Position::new(1, 5));
        assert_eq!(cursor.preferred_column(), 10); // Preserved from original
    }

    #[test]
    fn test_extend_to() {
        let mut cursor = Cursor::new(Position::new(0, 5));
        cursor.extend_to(Position::new(0, 15));

        assert!(cursor.has_selection());
        assert_eq!(cursor.selection.anchor, Position::new(0, 5));
        assert_eq!(cursor.selection.head, Position::new(0, 15));
    }

    #[test]
    fn test_collapse() {
        let selection = Selection::new(Position::new(0, 0), Position::new(0, 10));
        let mut cursor = Cursor::with_selection(selection);

        cursor.collapse();
        assert!(!cursor.has_selection());
        assert_eq!(cursor.position(), Position::new(0, 10)); // at head
    }

    #[test]
    fn test_collapse_to_start() {
        let selection = Selection::new(Position::new(0, 5), Position::new(0, 10));
        let mut cursor = Cursor::with_selection(selection);

        cursor.collapse_to_start();
        assert!(!cursor.has_selection());
        assert_eq!(cursor.position(), Position::new(0, 5));
    }

    #[test]
    fn test_collapse_to_end() {
        let selection = Selection::new(Position::new(0, 10), Position::new(0, 5)); // backward
        let mut cursor = Cursor::with_selection(selection);

        cursor.collapse_to_end();
        assert!(!cursor.has_selection());
        assert_eq!(cursor.position(), Position::new(0, 10));
    }

    #[test]
    fn test_from_position() {
        let cursor: Cursor = Position::new(5, 10).into();
        assert_eq!(cursor.position(), Position::new(5, 10));
    }

    #[test]
    fn test_from_selection() {
        let selection = Selection::new(Position::new(0, 0), Position::new(0, 10));
        let cursor: Cursor = selection.into();
        assert!(cursor.has_selection());
    }

    #[test]
    fn test_default() {
        let cursor: Cursor = Default::default();
        assert_eq!(cursor.position(), Position::origin());
        assert!(!cursor.has_selection());
    }
}
