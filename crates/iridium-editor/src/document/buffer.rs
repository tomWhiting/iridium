//! Document buffer backed by a rope data structure.

use ropey::{LineType, Rope};
use serde::{Deserialize, Serialize};

use super::position::{Position, Range};
use crate::editor::IridiumError;

/// The line type used for line counting and indexing.
/// `LF_CR` treats both `\n` and `\r` as line endings.
const LINE_TYPE: LineType = LineType::LF_CR;

/// Line ending style for the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LineEnding {
    /// Unix-style line endings: `\n`
    #[default]
    Lf,
    /// Windows-style line endings: `\r\n`
    CrLf,
    /// Legacy Mac-style line endings: `\r`
    Cr,
}

impl LineEnding {
    /// Returns the string representation of this line ending.
    #[must_use]
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
            Self::Cr => "\r",
        }
    }

    /// Detects the predominant line ending style in the given text.
    #[must_use]
    pub fn detect(text: &str) -> Self {
        let mut lf_count = 0;
        let mut crlf_count = 0;
        let mut cr_count = 0;

        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\r' {
                if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                    crlf_count += 1;
                    i += 2;
                    continue;
                }
                cr_count += 1;
            } else if bytes[i] == b'\n' {
                lf_count += 1;
            }
            i += 1;
        }

        if crlf_count >= lf_count && crlf_count >= cr_count && crlf_count > 0 {
            return Self::CrLf;
        }
        if cr_count > lf_count && cr_count > 0 {
            return Self::Cr;
        }
        Self::Lf
    }
}

/// The document being edited, backed by a rope data structure.
///
/// A rope provides efficient O(log N) insertions and deletions even for
/// very large documents, making it suitable for files with 100k+ lines.
///
/// # Example
///
/// ```
/// use iridium_editor::{Document, Position};
///
/// let mut doc = Document::new("Hello, world!");
/// assert_eq!(doc.line_count(), 1);
///
/// doc.insert(Position::new(0, 7), "Rust ");
/// assert_eq!(doc.text(), "Hello, Rust world!");
/// ```
#[derive(Debug, Clone)]
pub struct Document {
    /// Text content stored in rope structure
    content: Rope,
    /// Line ending style
    line_ending: LineEnding,
    /// Language identifier for syntax highlighting
    language: Option<String>,
}

impl Default for Document {
    fn default() -> Self {
        Self::new("")
    }
}

impl Document {
    /// Creates a new document with the given content.
    #[must_use]
    pub fn new(content: &str) -> Self {
        let line_ending = LineEnding::detect(content);
        Self {
            content: Rope::from_str(content),
            line_ending,
            language: None,
        }
    }

    /// Creates an empty document.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Returns the full text content of the document.
    ///
    /// Note: This allocates a new String. For large documents, prefer
    /// working with slices or iterating over lines.
    #[must_use]
    pub fn text(&self) -> String {
        self.content.to_string()
    }

    /// Returns the content of a specific line (without line ending).
    ///
    /// Returns `None` if the line number is out of bounds.
    #[must_use]
    pub fn line(&self, line_number: usize) -> Option<String> {
        if line_number >= self.line_count() {
            return None;
        }
        let line = self.content.line(line_number, LINE_TYPE);
        // Remove trailing newline if present
        let text = line.to_string();
        Some(text.trim_end_matches(['\r', '\n']).to_string())
    }

    /// Returns the number of lines in the document.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.content.len_lines(LINE_TYPE)
    }

    /// Returns the number of bytes in the document.
    #[must_use]
    pub fn byte_count(&self) -> usize {
        self.content.len()
    }

    /// Returns the number of characters in the document.
    #[must_use]
    pub fn char_count(&self) -> usize {
        self.content.len_chars()
    }

    /// Returns the length of a specific line in characters (without line ending).
    #[must_use]
    pub fn line_len(&self, line_number: usize) -> Option<usize> {
        self.line(line_number).map(|l| l.chars().count())
    }

    /// Returns a slice of the document for the given range.
    #[must_use]
    pub fn slice(&self, range: Range) -> String {
        let start_offset = self.position_to_offset(range.start);
        let end_offset = self.position_to_offset(range.end);

        if let (Some(start), Some(end)) = (start_offset, end_offset) {
            self.content.slice(start..end).to_string()
        } else {
            String::new()
        }
    }

    /// Converts a position (line, column) to a byte offset.
    ///
    /// Returns `None` if the position is out of bounds.
    #[must_use]
    pub fn position_to_offset(&self, position: Position) -> Option<usize> {
        if position.line >= self.line_count() {
            return None;
        }

        let line_start = self.content.line_to_byte_idx(position.line, LINE_TYPE);
        let line_len = self.line_len(position.line).unwrap_or(0);

        if position.column > line_len {
            return None;
        }

        Some(line_start + position.column)
    }

    /// Converts a byte offset to a position (line, column).
    ///
    /// Returns `None` if the offset is out of bounds.
    #[must_use]
    pub fn offset_to_position(&self, offset: usize) -> Option<Position> {
        if offset > self.byte_count() {
            return None;
        }

        let line = self.content.byte_to_line_idx(offset, LINE_TYPE);
        let line_start = self.content.line_to_byte_idx(line, LINE_TYPE);
        let column = offset - line_start;

        Some(Position::new(line, column))
    }

    /// Inserts text at the given position.
    ///
    /// # Errors
    ///
    /// Returns an error if the position is out of bounds.
    pub fn insert(&mut self, position: Position, text: &str) -> Result<(), IridiumError> {
        let offset = self.position_to_offset(position).ok_or(IridiumError::InvalidPosition {
            line: position.line,
            column: position.column,
        })?;

        self.content.insert(offset, text);
        Ok(())
    }

    /// Deletes text in the given range.
    ///
    /// Returns the deleted text.
    ///
    /// # Errors
    ///
    /// Returns an error if the range is out of bounds.
    pub fn delete(&mut self, range: Range) -> Result<String, IridiumError> {
        let start_offset = self.position_to_offset(range.start).ok_or(IridiumError::InvalidPosition {
            line: range.start.line,
            column: range.start.column,
        })?;

        let end_offset = self.position_to_offset(range.end).ok_or(IridiumError::InvalidPosition {
            line: range.end.line,
            column: range.end.column,
        })?;

        let deleted = self.content.slice(start_offset..end_offset).to_string();
        self.content.remove(start_offset..end_offset);
        Ok(deleted)
    }

    /// Replaces text in the given range with new text.
    ///
    /// Returns the old text that was replaced.
    ///
    /// # Errors
    ///
    /// Returns an error if the range is out of bounds.
    pub fn replace(&mut self, range: Range, new_text: &str) -> Result<String, IridiumError> {
        let old_text = self.delete(range)?;
        self.insert(range.start, new_text)?;
        Ok(old_text)
    }

    /// Returns the line ending style of the document.
    #[must_use]
    pub const fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    /// Sets the line ending style for the document.
    pub const fn set_line_ending(&mut self, line_ending: LineEnding) {
        self.line_ending = line_ending;
    }

    /// Returns the language identifier for syntax highlighting.
    #[must_use]
    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    /// Sets the language identifier for syntax highlighting.
    pub fn set_language(&mut self, language: Option<String>) {
        self.language = language;
    }

    /// Clamps a position to be within the document bounds.
    #[must_use]
    pub fn clamp_position(&self, position: Position) -> Position {
        let line = position.line.min(self.line_count().saturating_sub(1));
        let max_column = self.line_len(line).unwrap_or(0);
        let column = position.column.min(max_column);
        Position::new(line, column)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn document_new() {
        let doc = Document::new("Hello\nWorld");
        assert_eq!(doc.line_count(), 2);
        assert_eq!(doc.line(0), Some("Hello".to_string()));
        assert_eq!(doc.line(1), Some("World".to_string()));
    }

    #[test]
    fn document_insert() {
        let mut doc = Document::new("Hello World");
        doc.insert(Position::new(0, 6), "Rust ").unwrap();
        assert_eq!(doc.text(), "Hello Rust World");
    }

    #[test]
    fn document_delete() {
        let mut doc = Document::new("Hello World");
        let deleted = doc.delete(Range::new(Position::new(0, 5), Position::new(0, 11))).unwrap();
        assert_eq!(deleted, " World");
        assert_eq!(doc.text(), "Hello");
    }

    #[test]
    fn document_replace() {
        let mut doc = Document::new("Hello World");
        let old = doc.replace(Range::new(Position::new(0, 6), Position::new(0, 11)), "Rust").unwrap();
        assert_eq!(old, "World");
        assert_eq!(doc.text(), "Hello Rust");
    }

    #[test]
    fn position_offset_conversion() {
        let doc = Document::new("Hello\nWorld\nTest");

        assert_eq!(doc.position_to_offset(Position::new(0, 0)), Some(0));
        assert_eq!(doc.position_to_offset(Position::new(0, 5)), Some(5));
        assert_eq!(doc.position_to_offset(Position::new(1, 0)), Some(6));
        assert_eq!(doc.position_to_offset(Position::new(2, 0)), Some(12));

        assert_eq!(doc.offset_to_position(0), Some(Position::new(0, 0)));
        assert_eq!(doc.offset_to_position(6), Some(Position::new(1, 0)));
    }

    #[test]
    fn line_ending_detection() {
        assert_eq!(LineEnding::detect("hello\nworld"), LineEnding::Lf);
        assert_eq!(LineEnding::detect("hello\r\nworld"), LineEnding::CrLf);
        assert_eq!(LineEnding::detect("hello\rworld"), LineEnding::Cr);
        assert_eq!(LineEnding::detect("hello"), LineEnding::Lf); // Default
    }
}
