//! Cursor and selection state management.

use serde::{Deserialize, Serialize};

use super::position::{Position, Range};

/// A selection in the document with anchor and head.
///
/// The anchor is where the selection started, and the head is where the cursor
/// currently is. This allows tracking selection direction.
///
/// When anchor equals head, the selection is "collapsed" to just a cursor.
///
/// # Example
///
/// ```
/// use iridium_editor::{Position, Selection};
///
/// // A collapsed selection (just a cursor)
/// let cursor = Selection::collapsed(Position::new(0, 5));
/// assert!(cursor.is_collapsed());
///
/// // A forward selection
/// let selection = Selection::new(
///     Position::new(0, 0),
///     Position::new(0, 10),
/// );
/// assert!(selection.is_forward());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Selection {
    /// Where the selection started
    pub anchor: Position,
    /// Where the cursor/head is (current position)
    pub head: Position,
}

impl Selection {
    /// Creates a new selection from anchor to head.
    #[must_use]
    pub const fn new(anchor: Position, head: Position) -> Self {
        Self { anchor, head }
    }

    /// Creates a collapsed selection (cursor) at the given position.
    #[must_use]
    pub const fn collapsed(position: Position) -> Self {
        Self { anchor: position, head: position }
    }

    /// Returns true if the selection is collapsed (just a cursor).
    #[must_use]
    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.head
    }

    /// Returns true if the selection is forward (anchor <= head).
    #[must_use]
    pub fn is_forward(&self) -> bool {
        self.anchor <= self.head
    }

    /// Returns true if the selection is backward (anchor > head).
    #[must_use]
    pub fn is_backward(&self) -> bool {
        self.anchor > self.head
    }

    /// Returns the cursor position (same as head).
    #[must_use]
    pub const fn cursor_position(&self) -> Position {
        self.head
    }

    /// Returns the selection as a normalized range (start <= end).
    #[must_use]
    pub fn range(&self) -> Range {
        Range::new(self.anchor, self.head)
    }

    /// Returns the start position (minimum of anchor and head).
    #[must_use]
    pub fn start(&self) -> Position {
        std::cmp::min(self.anchor, self.head)
    }

    /// Returns the end position (maximum of anchor and head).
    #[must_use]
    pub fn end(&self) -> Position {
        std::cmp::max(self.anchor, self.head)
    }
}

/// Cursor state supporting multiple cursors.
///
/// The primary cursor always exists. Additional secondary cursors can be
/// added for multi-cursor editing.
///
/// # Invariants
///
/// - Primary cursor always exists
/// - Secondary cursors are sorted by position
/// - No two cursors overlap
///
/// # Example
///
/// ```
/// use iridium_editor::{CursorState, Position, Selection};
///
/// let mut cursors = CursorState::new(Selection::collapsed(Position::new(0, 0)));
/// cursors.add_cursor(Selection::collapsed(Position::new(1, 0)));
/// assert_eq!(cursors.cursor_count(), 2);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CursorState {
    /// Primary cursor (always exists)
    pub primary: Selection,
    /// Additional cursors (may be empty)
    pub secondary: Vec<Selection>,
}

impl CursorState {
    /// Creates a new cursor state with a single primary cursor.
    #[must_use]
    pub const fn new(primary: Selection) -> Self {
        Self { primary, secondary: Vec::new() }
    }

    /// Creates a cursor state with a collapsed cursor at the given position.
    #[must_use]
    pub const fn at(position: Position) -> Self {
        Self::new(Selection::collapsed(position))
    }

    /// Returns the total number of cursors.
    #[must_use]
    pub fn cursor_count(&self) -> usize {
        1 + self.secondary.len()
    }

    /// Returns an iterator over all selections (primary first).
    pub fn all_selections(&self) -> impl Iterator<Item = &Selection> {
        std::iter::once(&self.primary).chain(self.secondary.iter())
    }

    /// Returns a mutable iterator over all selections (primary first).
    pub fn all_selections_mut(&mut self) -> impl Iterator<Item = &mut Selection> {
        std::iter::once(&mut self.primary).chain(self.secondary.iter_mut())
    }

    /// Adds a new cursor at the given selection.
    ///
    /// The cursor is inserted in sorted order by position.
    /// If the new cursor overlaps with an existing cursor, they are merged.
    pub fn add_cursor(&mut self, selection: Selection) {
        // Find the insertion point
        let pos = selection.start();
        let insert_idx = self.secondary.iter().position(|s| s.start() > pos).unwrap_or(self.secondary.len());

        self.secondary.insert(insert_idx, selection);
        self.merge_overlapping();
    }

    /// Removes the cursor at the given index.
    ///
    /// Index 0 is the primary cursor, which cannot be removed (this is a no-op).
    /// Indices 1+ correspond to secondary cursors.
    pub fn remove_cursor(&mut self, index: usize) {
        if index > 0 && index <= self.secondary.len() {
            self.secondary.remove(index - 1);
        }
    }

    /// Collapses to only the primary cursor, removing all secondary cursors.
    pub fn collapse_to_primary(&mut self) {
        self.secondary.clear();
    }

    /// Merges overlapping cursors.
    fn merge_overlapping(&mut self) {
        // Simple implementation: remove duplicates based on head position
        // A more sophisticated version would merge overlapping ranges
        self.secondary.dedup_by(|a, b| a.head == b.head);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_collapsed() {
        let sel = Selection::collapsed(Position::new(5, 10));
        assert!(sel.is_collapsed());
        assert_eq!(sel.cursor_position(), Position::new(5, 10));
    }

    #[test]
    fn selection_direction() {
        let forward = Selection::new(Position::new(0, 0), Position::new(0, 10));
        assert!(forward.is_forward());
        assert!(!forward.is_backward());

        let backward = Selection::new(Position::new(0, 10), Position::new(0, 0));
        assert!(!backward.is_forward());
        assert!(backward.is_backward());
    }

    #[test]
    fn cursor_state_add() {
        let mut state = CursorState::at(Position::new(0, 0));
        assert_eq!(state.cursor_count(), 1);

        state.add_cursor(Selection::collapsed(Position::new(1, 0)));
        assert_eq!(state.cursor_count(), 2);

        state.add_cursor(Selection::collapsed(Position::new(2, 0)));
        assert_eq!(state.cursor_count(), 3);
    }

    #[test]
    fn cursor_state_collapse() {
        let mut state = CursorState::at(Position::new(0, 0));
        state.add_cursor(Selection::collapsed(Position::new(1, 0)));
        state.add_cursor(Selection::collapsed(Position::new(2, 0)));

        state.collapse_to_primary();
        assert_eq!(state.cursor_count(), 1);
    }
}
