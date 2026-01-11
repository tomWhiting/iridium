//! Text selection representation.
//!
//! A Selection represents a range of text in the buffer, defined by
//! an anchor (where the selection started) and a head (the current cursor).

use super::Position;

/// A text selection defined by an anchor and head position.
///
/// The anchor is where the selection was started (e.g., where the user
/// clicked or started shift+arrow). The head is the current cursor position.
/// The anchor and head can be in any order - the anchor might be before
/// or after the head depending on the direction of selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Selection {
    /// The starting position of the selection.
    pub anchor: Position,
    /// The current cursor position (end of selection).
    pub head: Position,
}

impl Selection {
    /// Creates a new selection with the given anchor and head.
    ///
    /// # Examples
    ///
    /// ```
    /// use iridium_editor::{Selection, Position};
    ///
    /// let selection = Selection::new(Position::new(0, 0), Position::new(0, 5));
    /// assert_eq!(selection.len_chars(), 5);
    /// ```
    #[must_use]
    pub const fn new(anchor: Position, head: Position) -> Self {
        Self { anchor, head }
    }

    /// Creates a cursor (zero-width selection) at the given position.
    ///
    /// A cursor is a selection where the anchor and head are the same.
    ///
    /// # Examples
    ///
    /// ```
    /// use iridium_editor::{Selection, Position};
    ///
    /// let cursor = Selection::cursor(Position::new(5, 10));
    /// assert!(cursor.is_cursor());
    /// ```
    #[must_use]
    pub const fn cursor(position: Position) -> Self {
        Self {
            anchor: position,
            head: position,
        }
    }

    /// Returns true if this selection is a cursor (zero width).
    #[must_use]
    pub fn is_cursor(&self) -> bool {
        self.anchor == self.head
    }

    /// Returns true if this selection has content (non-zero width).
    #[must_use]
    pub fn is_selection(&self) -> bool {
        self.anchor != self.head
    }

    /// Returns the start position (earlier of anchor and head).
    #[must_use]
    pub fn start(&self) -> Position {
        std::cmp::min(self.anchor, self.head)
    }

    /// Returns the end position (later of anchor and head).
    #[must_use]
    pub fn end(&self) -> Position {
        std::cmp::max(self.anchor, self.head)
    }

    /// Returns true if the selection is forward (head >= anchor).
    #[must_use]
    pub fn is_forward(&self) -> bool {
        self.head >= self.anchor
    }

    /// Returns true if the selection is backward (head < anchor).
    #[must_use]
    pub fn is_backward(&self) -> bool {
        self.head < self.anchor
    }

    /// Returns the number of lines this selection spans.
    ///
    /// A selection within a single line returns 1.
    #[must_use]
    pub fn line_span(&self) -> usize {
        let start = self.start();
        let end = self.end();
        end.line - start.line + 1
    }

    /// Returns true if the selection spans multiple lines.
    #[must_use]
    pub fn is_multiline(&self) -> bool {
        self.line_span() > 1
    }

    /// Returns true if this selection contains the given position.
    ///
    /// A cursor (zero-width selection) contains only its own position.
    #[must_use]
    pub fn contains(&self, position: Position) -> bool {
        position >= self.start() && position <= self.end()
    }

    /// Returns true if this selection overlaps with another selection.
    #[must_use]
    pub fn overlaps(&self, other: &Selection) -> bool {
        self.start() <= other.end() && other.start() <= self.end()
    }

    /// Returns the approximate number of characters in the selection.
    ///
    /// Note: This is only accurate for single-line selections. For
    /// multi-line selections, this returns an estimate based on the
    /// column difference, which doesn't account for varying line lengths.
    #[must_use]
    pub fn len_chars(&self) -> usize {
        let start = self.start();
        let end = self.end();

        if start.line == end.line {
            end.column.saturating_sub(start.column)
        } else {
            // For multi-line, this is an approximation
            // Actual length requires buffer access
            0
        }
    }

    /// Extends the selection to include the given position.
    ///
    /// The anchor remains fixed; only the head moves.
    #[must_use]
    pub fn extend_to(&self, position: Position) -> Self {
        Self {
            anchor: self.anchor,
            head: position,
        }
    }

    /// Moves the cursor to the given position without changing the anchor.
    ///
    /// This is equivalent to `extend_to` but named for clarity in cursor operations.
    #[must_use]
    pub fn with_head(&self, position: Position) -> Self {
        self.extend_to(position)
    }

    /// Collapses the selection to a cursor at the head position.
    #[must_use]
    pub fn collapse_to_head(&self) -> Self {
        Self::cursor(self.head)
    }

    /// Collapses the selection to a cursor at the start position.
    #[must_use]
    pub fn collapse_to_start(&self) -> Self {
        Self::cursor(self.start())
    }

    /// Collapses the selection to a cursor at the end position.
    #[must_use]
    pub fn collapse_to_end(&self) -> Self {
        Self::cursor(self.end())
    }

    /// Swaps the anchor and head, reversing the selection direction.
    #[must_use]
    pub fn reversed(&self) -> Self {
        Self {
            anchor: self.head,
            head: self.anchor,
        }
    }

    /// Normalizes the selection so anchor <= head (forward direction).
    #[must_use]
    pub fn normalized(&self) -> Self {
        if self.is_forward() {
            *self
        } else {
            self.reversed()
        }
    }
}

impl Default for Selection {
    fn default() -> Self {
        Self::cursor(Position::origin())
    }
}

impl From<Position> for Selection {
    fn from(position: Position) -> Self {
        Self::cursor(position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor() {
        let pos = Position::new(5, 10);
        let cursor = Selection::cursor(pos);
        assert!(cursor.is_cursor());
        assert!(!cursor.is_selection());
        assert_eq!(cursor.anchor, pos);
        assert_eq!(cursor.head, pos);
    }

    #[test]
    fn test_selection() {
        let anchor = Position::new(0, 0);
        let head = Position::new(0, 10);
        let sel = Selection::new(anchor, head);
        assert!(!sel.is_cursor());
        assert!(sel.is_selection());
    }

    #[test]
    fn test_start_end_forward() {
        let sel = Selection::new(Position::new(0, 0), Position::new(0, 10));
        assert_eq!(sel.start(), Position::new(0, 0));
        assert_eq!(sel.end(), Position::new(0, 10));
        assert!(sel.is_forward());
    }

    #[test]
    fn test_start_end_backward() {
        let sel = Selection::new(Position::new(0, 10), Position::new(0, 0));
        assert_eq!(sel.start(), Position::new(0, 0));
        assert_eq!(sel.end(), Position::new(0, 10));
        assert!(sel.is_backward());
    }

    #[test]
    fn test_line_span() {
        let single_line = Selection::new(Position::new(5, 0), Position::new(5, 10));
        assert_eq!(single_line.line_span(), 1);
        assert!(!single_line.is_multiline());

        let multi_line = Selection::new(Position::new(0, 0), Position::new(5, 10));
        assert_eq!(multi_line.line_span(), 6);
        assert!(multi_line.is_multiline());
    }

    #[test]
    fn test_contains() {
        let sel = Selection::new(Position::new(0, 5), Position::new(0, 10));
        assert!(sel.contains(Position::new(0, 5)));
        assert!(sel.contains(Position::new(0, 7)));
        assert!(sel.contains(Position::new(0, 10)));
        assert!(!sel.contains(Position::new(0, 4)));
        assert!(!sel.contains(Position::new(0, 11)));
    }

    #[test]
    fn test_overlaps() {
        let sel1 = Selection::new(Position::new(0, 0), Position::new(0, 10));
        let sel2 = Selection::new(Position::new(0, 5), Position::new(0, 15));
        let sel3 = Selection::new(Position::new(0, 15), Position::new(0, 20));

        assert!(sel1.overlaps(&sel2));
        assert!(sel2.overlaps(&sel1));
        assert!(!sel1.overlaps(&sel3));
    }

    #[test]
    fn test_len_chars_single_line() {
        let sel = Selection::new(Position::new(0, 5), Position::new(0, 15));
        assert_eq!(sel.len_chars(), 10);
    }

    #[test]
    fn test_extend_to() {
        let sel = Selection::cursor(Position::new(0, 5));
        let extended = sel.extend_to(Position::new(0, 10));
        assert_eq!(extended.anchor, Position::new(0, 5));
        assert_eq!(extended.head, Position::new(0, 10));
    }

    #[test]
    fn test_collapse() {
        let sel = Selection::new(Position::new(0, 5), Position::new(0, 10));

        let collapsed_head = sel.collapse_to_head();
        assert_eq!(collapsed_head.head, Position::new(0, 10));
        assert!(collapsed_head.is_cursor());

        let collapsed_start = sel.collapse_to_start();
        assert_eq!(collapsed_start.head, Position::new(0, 5));
        assert!(collapsed_start.is_cursor());

        let collapsed_end = sel.collapse_to_end();
        assert_eq!(collapsed_end.head, Position::new(0, 10));
        assert!(collapsed_end.is_cursor());
    }

    #[test]
    fn test_reversed() {
        let sel = Selection::new(Position::new(0, 5), Position::new(0, 10));
        let reversed = sel.reversed();
        assert_eq!(reversed.anchor, Position::new(0, 10));
        assert_eq!(reversed.head, Position::new(0, 5));
    }

    #[test]
    fn test_normalized() {
        let backward = Selection::new(Position::new(0, 10), Position::new(0, 5));
        let normalized = backward.normalized();
        assert!(normalized.is_forward());
        assert_eq!(normalized.anchor, Position::new(0, 5));
        assert_eq!(normalized.head, Position::new(0, 10));

        let forward = Selection::new(Position::new(0, 5), Position::new(0, 10));
        let still_forward = forward.normalized();
        assert_eq!(still_forward, forward);
    }

    #[test]
    fn test_default() {
        let sel: Selection = Default::default();
        assert!(sel.is_cursor());
        assert!(sel.head.is_origin());
    }

    #[test]
    fn test_from_position() {
        let pos = Position::new(5, 10);
        let sel: Selection = pos.into();
        assert!(sel.is_cursor());
        assert_eq!(sel.head, pos);
    }
}
