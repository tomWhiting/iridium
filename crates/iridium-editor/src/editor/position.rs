//! Text position representation.
//!
//! A Position represents a location in the text buffer using line and
//! column coordinates. This is the user-facing coordinate system.

/// A position in the text buffer, represented as line and column.
///
/// Positions use 0-based indexing for both line and column.
/// The column represents a character offset within the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Position {
    /// The 0-based line number.
    pub line: usize,
    /// The 0-based column (character offset within the line).
    pub column: usize,
}

impl Position {
    /// Creates a new position at the specified line and column.
    ///
    /// # Examples
    ///
    /// ```
    /// use iridium_editor::Position;
    ///
    /// let pos = Position::new(5, 10);
    /// assert_eq!(pos.line, 5);
    /// assert_eq!(pos.column, 10);
    /// ```
    #[must_use]
    pub const fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }

    /// Creates a position at line 0, column 0 (the origin).
    ///
    /// # Examples
    ///
    /// ```
    /// use iridium_editor::Position;
    ///
    /// let pos = Position::origin();
    /// assert_eq!(pos, Position::new(0, 0));
    /// ```
    #[must_use]
    pub const fn origin() -> Self {
        Self { line: 0, column: 0 }
    }

    /// Returns true if this position is at the origin (0, 0).
    #[must_use]
    pub const fn is_origin(&self) -> bool {
        self.line == 0 && self.column == 0
    }

    /// Returns a new position with the line offset by the given delta.
    ///
    /// The delta can be negative to move to earlier lines.
    /// Returns None if the resulting line would be negative.
    #[must_use]
    pub fn with_line_offset(&self, delta: isize) -> Option<Self> {
        let new_line = if delta >= 0 {
            self.line.checked_add(delta as usize)
        } else {
            self.line.checked_sub((-delta) as usize)
        };

        new_line.map(|line| Self {
            line,
            column: self.column,
        })
    }

    /// Returns a new position with the column offset by the given delta.
    ///
    /// The delta can be negative to move to earlier columns.
    /// Returns None if the resulting column would be negative.
    #[must_use]
    pub fn with_column_offset(&self, delta: isize) -> Option<Self> {
        let new_column = if delta >= 0 {
            self.column.checked_add(delta as usize)
        } else {
            self.column.checked_sub((-delta) as usize)
        };

        new_column.map(|column| Self {
            line: self.line,
            column,
        })
    }

    /// Returns a new position with the specified column.
    #[must_use]
    pub const fn with_column(&self, column: usize) -> Self {
        Self {
            line: self.line,
            column,
        }
    }

    /// Returns a new position with the specified line.
    #[must_use]
    pub const fn with_line(&self, line: usize) -> Self {
        Self {
            line,
            column: self.column,
        }
    }
}

impl PartialOrd for Position {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Position {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.line.cmp(&other.line) {
            std::cmp::Ordering::Equal => self.column.cmp(&other.column),
            ordering => ordering,
        }
    }
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Display as 1-based for human readability
        write!(f, "{}:{}", self.line + 1, self.column + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let pos = Position::new(5, 10);
        assert_eq!(pos.line, 5);
        assert_eq!(pos.column, 10);
    }

    #[test]
    fn test_origin() {
        let pos = Position::origin();
        assert_eq!(pos.line, 0);
        assert_eq!(pos.column, 0);
        assert!(pos.is_origin());
    }

    #[test]
    fn test_default_is_origin() {
        let pos: Position = Default::default();
        assert!(pos.is_origin());
    }

    #[test]
    fn test_ordering() {
        let a = Position::new(0, 0);
        let b = Position::new(0, 5);
        let c = Position::new(1, 0);
        let d = Position::new(1, 5);

        assert!(a < b);
        assert!(b < c);
        assert!(c < d);
        assert!(a < d);
    }

    #[test]
    fn test_with_line_offset_positive() {
        let pos = Position::new(5, 10);
        let new_pos = pos.with_line_offset(3).unwrap();
        assert_eq!(new_pos.line, 8);
        assert_eq!(new_pos.column, 10);
    }

    #[test]
    fn test_with_line_offset_negative() {
        let pos = Position::new(5, 10);
        let new_pos = pos.with_line_offset(-3).unwrap();
        assert_eq!(new_pos.line, 2);
        assert_eq!(new_pos.column, 10);
    }

    #[test]
    fn test_with_line_offset_underflow() {
        let pos = Position::new(2, 10);
        let result = pos.with_line_offset(-5);
        assert!(result.is_none());
    }

    #[test]
    fn test_with_column_offset_positive() {
        let pos = Position::new(5, 10);
        let new_pos = pos.with_column_offset(5).unwrap();
        assert_eq!(new_pos.line, 5);
        assert_eq!(new_pos.column, 15);
    }

    #[test]
    fn test_with_column_offset_negative() {
        let pos = Position::new(5, 10);
        let new_pos = pos.with_column_offset(-5).unwrap();
        assert_eq!(new_pos.line, 5);
        assert_eq!(new_pos.column, 5);
    }

    #[test]
    fn test_with_column() {
        let pos = Position::new(5, 10);
        let new_pos = pos.with_column(20);
        assert_eq!(new_pos.line, 5);
        assert_eq!(new_pos.column, 20);
    }

    #[test]
    fn test_with_line() {
        let pos = Position::new(5, 10);
        let new_pos = pos.with_line(15);
        assert_eq!(new_pos.line, 15);
        assert_eq!(new_pos.column, 10);
    }

    #[test]
    fn test_display() {
        let pos = Position::new(5, 10);
        assert_eq!(format!("{pos}"), "6:11"); // 1-based display
    }
}
