//! Position and range types for document locations.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

/// A position in the document specified by line and column.
///
/// Both line and column are 0-indexed. Column is measured in UTF-8 code points,
/// not bytes or grapheme clusters.
///
/// # Example
///
/// ```
/// use iridium_editor::Position;
///
/// let pos = Position::new(0, 5); // Line 0, column 5
/// assert_eq!(pos.line, 0);
/// assert_eq!(pos.column, 5);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Position {
    /// Line number (0-indexed)
    pub line: usize,
    /// Column offset in UTF-8 code points (0-indexed)
    pub column: usize,
}

impl Position {
    /// Creates a new position at the given line and column.
    #[must_use]
    pub const fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }

    /// Creates a position at the start of the document (0, 0).
    #[must_use]
    pub const fn zero() -> Self {
        Self { line: 0, column: 0 }
    }
}

impl PartialOrd for Position {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Position {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.line.cmp(&other.line) {
            Ordering::Equal => self.column.cmp(&other.column),
            ord => ord,
        }
    }
}

/// A range in the document from start to end.
///
/// Start is inclusive, end is exclusive. The range is always stored in
/// canonical form where `start <= end`.
///
/// # Example
///
/// ```
/// use iridium_editor::{Position, Range};
///
/// let range = Range::new(
///     Position::new(0, 0),
///     Position::new(0, 10),
/// );
/// assert!(!range.is_empty());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Range {
    /// Start position (inclusive)
    pub start: Position,
    /// End position (exclusive)
    pub end: Position,
}

impl Range {
    /// Creates a new range from start to end.
    ///
    /// The range is normalized so that `start <= end`.
    #[must_use]
    pub fn new(start: Position, end: Position) -> Self {
        if start <= end {
            Self { start, end }
        } else {
            Self { start: end, end: start }
        }
    }

    /// Creates an empty range at the given position.
    #[must_use]
    pub const fn empty(position: Position) -> Self {
        Self { start: position, end: position }
    }

    /// Returns true if the range is empty (start == end).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Returns true if the range contains the given position.
    #[must_use]
    pub fn contains(&self, position: Position) -> bool {
        position >= self.start && position < self.end
    }

    /// Returns true if this range intersects with another range.
    #[must_use]
    pub fn intersects(&self, other: &Self) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Returns the union of this range with another.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        Self {
            start: std::cmp::min(self.start, other.start),
            end: std::cmp::max(self.end, other.end),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_ordering() {
        let a = Position::new(0, 5);
        let b = Position::new(0, 10);
        let c = Position::new(1, 0);

        assert!(a < b);
        assert!(b < c);
        assert!(a < c);
    }

    #[test]
    fn range_normalization() {
        let start = Position::new(1, 0);
        let end = Position::new(0, 5);

        // Should normalize so start <= end
        let range = Range::new(start, end);
        assert_eq!(range.start, end);
        assert_eq!(range.end, start);
    }

    #[test]
    fn range_contains() {
        let range = Range::new(Position::new(0, 5), Position::new(0, 10));

        assert!(!range.contains(Position::new(0, 4)));
        assert!(range.contains(Position::new(0, 5)));
        assert!(range.contains(Position::new(0, 7)));
        assert!(!range.contains(Position::new(0, 10))); // End is exclusive
    }
}
