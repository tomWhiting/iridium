//! Document buffer backed by a rope data structure.

use std::sync::atomic::{AtomicU64, Ordering};

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
    /// Monotonic content-revision counter, bumped on every mutation that
    /// changes the text (insert of non-empty text, deletion of a non-empty
    /// range). Callers that cache document-relative state (e.g. the keyboard
    /// handler's multi-cursor addition stack) compare revisions to detect
    /// that content moved under otherwise-unchanged cursor positions.
    revision: u64,
    /// Which document this is, as opposed to how many times it has changed.
    ///
    /// See [`Document::id`]. Distinct from [`revision`](Self::revision) and
    /// load-bearing precisely where that one is not: every document starts its
    /// counter at zero, so a revision tells two *observations of one document*
    /// apart and says nothing at all about two documents.
    id: u64,
}

/// Hands out document identities.
///
/// Wraps at `u64::MAX`. At one document per nanosecond that is 584 years, and
/// the failure at the wrap is one avoidable cache hit, not unsoundness — so
/// `wrapping_add` rather than a panic or a saturating counter that would hand
/// every later document the same identity.
static NEXT_DOCUMENT_ID: AtomicU64 = AtomicU64::new(0);

/// Takes the next identity.
fn next_document_id() -> u64 {
    NEXT_DOCUMENT_ID.fetch_add(1, Ordering::Relaxed)
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
            revision: 0,
            id: next_document_id(),
        }
    }

    /// Creates a document that continues `previous`'s revision counter.
    ///
    /// Replacing a buffer's whole content is a text change like any other,
    /// and [`revision`](Self::revision) promises that a repeated value means
    /// the text did not move. A fresh [`new`](Self::new) restarts at zero and
    /// breaks that promise: load a file, edit it, load a different one, and
    /// the counter reads a value the *old* text already used. Everything that
    /// caches document-relative state — the sticky column, the multi-cursor
    /// addition stack, the retained syntax tree's parsed revision — compares
    /// revisions for inequality and would be told nothing had changed.
    ///
    /// Taking the previous document rather than a bare number is what makes
    /// the counter monotonic by construction: there is no way to spell "go
    /// backwards".
    ///
    /// The [`id`](Self::id) is a **fresh** one, not `previous`'s. The content
    /// is wholly replaced, so anything built for the old text is worthless, and
    /// a new identity says so directly instead of relying on the revision bump
    /// to say it indirectly. It costs nothing — the bump already forces every
    /// revision-keyed cache to miss — and it keeps a future cache that keys on
    /// the id alone correct.
    #[must_use]
    pub fn continuing_from(content: &str, previous: &Self) -> Self {
        let mut document = Self::new(content);
        document.revision = previous.revision.wrapping_add(1);
        document
    }

    /// Returns the current content-revision counter.
    ///
    /// The value increases by at least one every time the document's text
    /// changes; it never decreases and is unaffected by pure cursor moves.
    /// Two observations of the same value guarantee the text did not change
    /// between them — including across a whole-content replacement, which is
    /// why [`continuing_from`](Self::continuing_from) exists.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Which document this is — an identity, not a change counter.
    ///
    /// Taken from a process-wide counter at construction and never reused, so
    /// two documents alive at the same time never share one. Deliberately
    /// **not** derived from the content: two files with identical text are
    /// still two documents, and anything holding state built for one of them
    /// must rebuild for the other.
    ///
    /// ⭐ **Why this exists, when [`revision`](Self::revision) is right there.**
    /// A revision counts a document's own changes and every document starts at
    /// zero, so equal revisions mean "the same document did not change" and
    /// carry no information whatever about two *different* documents. A cache
    /// shared across documents — one [`FrameCompositor`] serving several tabs —
    /// that keys on the revision alone will treat a switch between two files
    /// edited the same number of times as a hit, and re-present the previous
    /// file's buffers. That was live until 8 Aug 2026; see
    /// `docs/IN-FLIGHT-87-document-identity.md`.
    ///
    /// A `Clone` keeps the id, because a clone is a copy of *this* document
    /// made to compute against, not a second document someone will edit.
    ///
    /// [`FrameCompositor`]: crate::render::FrameCompositor
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
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

    /// Returns a reference to the underlying rope structure.
    ///
    /// This is useful for efficient viewport-based operations that need
    /// direct access to the rope's line indexing capabilities.
    #[must_use]
    pub const fn rope(&self) -> &Rope {
        &self.content
    }

    /// Returns the byte offset of the start of a line.
    ///
    /// This is useful for viewport-based span queries where we need to
    /// convert line numbers to byte offsets efficiently.
    ///
    /// Returns `None` if the line number is out of bounds.
    #[must_use]
    pub fn line_to_byte_offset(&self, line: usize) -> Option<usize> {
        if line >= self.line_count() {
            return None;
        }
        Some(self.content.line_to_byte_idx(line, LINE_TYPE))
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
    /// The column is interpreted as a character (code point) index within the line,
    /// which is then converted to the appropriate byte offset. This ensures correct
    /// handling of multi-byte UTF-8 characters.
    ///
    /// Returns `None` if the position is out of bounds.
    #[must_use]
    pub fn position_to_offset(&self, position: Position) -> Option<usize> {
        if position.line >= self.line_count() {
            return None;
        }

        let line_start = self.content.line_to_byte_idx(position.line, LINE_TYPE);
        let line_slice = self.content.line(position.line, LINE_TYPE);
        let line_char_len = line_slice.len_chars();

        // Column is measured in characters (code points), not bytes
        if position.column > line_char_len {
            return None;
        }

        // Convert character offset within line to byte offset
        let byte_offset_in_line = line_slice.char_to_byte_idx(position.column);
        Some(line_start + byte_offset_in_line)
    }

    /// Converts a byte offset to a position (line, column).
    ///
    /// The resulting column is a character (code point) index within the line,
    /// ensuring correct handling of multi-byte UTF-8 characters.
    ///
    /// Returns `None` if the offset is out of bounds.
    #[must_use]
    pub fn offset_to_position(&self, offset: usize) -> Option<Position> {
        if offset > self.byte_count() {
            return None;
        }

        let line = self.content.byte_to_line_idx(offset, LINE_TYPE);
        let line_start = self.content.line_to_byte_idx(line, LINE_TYPE);
        let byte_offset_in_line = offset - line_start;

        // Convert byte offset within line to character offset
        let line_slice = self.content.line(line, LINE_TYPE);
        let column = line_slice.byte_to_char_idx(byte_offset_in_line);

        Some(Position::new(line, column))
    }

    /// Inserts text at the given position.
    ///
    /// # Errors
    ///
    /// Returns an error if the position is out of bounds.
    pub fn insert(&mut self, position: Position, text: &str) -> Result<(), IridiumError> {
        let offset = self
            .position_to_offset(position)
            .ok_or(IridiumError::InvalidPosition {
                line: position.line,
                column: position.column,
            })?;

        self.content.insert(offset, text);
        if !text.is_empty() {
            self.revision = self.revision.wrapping_add(1);
        }
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
        let start_offset =
            self.position_to_offset(range.start)
                .ok_or(IridiumError::InvalidPosition {
                    line: range.start.line,
                    column: range.start.column,
                })?;

        let end_offset =
            self.position_to_offset(range.end)
                .ok_or(IridiumError::InvalidPosition {
                    line: range.end.line,
                    column: range.end.column,
                })?;

        let deleted = self.content.slice(start_offset..end_offset).to_string();
        self.content.remove(start_offset..end_offset);
        if start_offset != end_offset {
            self.revision = self.revision.wrapping_add(1);
        }
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
        let deleted = doc
            .delete(Range::new(Position::new(0, 5), Position::new(0, 11)))
            .unwrap();
        assert_eq!(deleted, " World");
        assert_eq!(doc.text(), "Hello");
    }

    #[test]
    fn document_replace() {
        let mut doc = Document::new("Hello World");
        let old = doc
            .replace(
                Range::new(Position::new(0, 6), Position::new(0, 11)),
                "Rust",
            )
            .unwrap();
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

    #[test]
    fn position_offset_conversion_utf8() {
        // Test with multi-byte UTF-8 characters
        // "日本語" = 3 characters, 9 bytes (3 bytes each)
        let doc = Document::new("日本語\nHello");

        // First line: "日本語" (3 chars, 9 bytes)
        assert_eq!(doc.line_len(0), Some(3)); // 3 characters
        assert_eq!(doc.position_to_offset(Position::new(0, 0)), Some(0)); // Start
        assert_eq!(doc.position_to_offset(Position::new(0, 1)), Some(3)); // After 日
        assert_eq!(doc.position_to_offset(Position::new(0, 2)), Some(6)); // After 日本
        assert_eq!(doc.position_to_offset(Position::new(0, 3)), Some(9)); // After 日本語

        // Second line: "Hello" starts at byte 10 (9 + newline)
        assert_eq!(doc.position_to_offset(Position::new(1, 0)), Some(10));
        assert_eq!(doc.position_to_offset(Position::new(1, 2)), Some(12)); // He|llo

        // Reverse conversion
        assert_eq!(doc.offset_to_position(0), Some(Position::new(0, 0)));
        assert_eq!(doc.offset_to_position(3), Some(Position::new(0, 1))); // After 日
        assert_eq!(doc.offset_to_position(6), Some(Position::new(0, 2))); // After 日本
        assert_eq!(doc.offset_to_position(10), Some(Position::new(1, 0))); // Start of Hello
    }

    #[test]
    fn revision_bumps_on_text_mutations_only() {
        let mut doc = Document::new("hello world");
        assert_eq!(doc.revision(), 0);

        // Non-empty insert bumps.
        doc.insert(Position::new(0, 5), ",").unwrap();
        let after_insert = doc.revision();
        assert!(after_insert > 0);

        // Empty insert does not bump.
        doc.insert(Position::new(0, 0), "").unwrap();
        assert_eq!(doc.revision(), after_insert);

        // Non-empty delete bumps.
        doc.delete(Range::new(Position::new(0, 5), Position::new(0, 6)))
            .unwrap();
        let after_delete = doc.revision();
        assert!(after_delete > after_insert);

        // Empty delete does not bump.
        doc.delete(Range::new(Position::new(0, 3), Position::new(0, 3)))
            .unwrap();
        assert_eq!(doc.revision(), after_delete);

        // Replace bumps (at least once).
        doc.replace(
            Range::new(Position::new(0, 6), Position::new(0, 11)),
            "rust",
        )
        .unwrap();
        assert!(doc.revision() > after_delete);
    }

    #[test]
    fn insert_delete_utf8() {
        let mut doc = Document::new("日本語");

        // Insert after first character (column 1 = after 日)
        doc.insert(Position::new(0, 1), "X").unwrap();
        assert_eq!(doc.text(), "日X本語");

        // Delete the X (at column 1, length 1)
        let deleted = doc
            .delete(Range::new(Position::new(0, 1), Position::new(0, 2)))
            .unwrap();
        assert_eq!(deleted, "X");
        assert_eq!(doc.text(), "日本語");

        // Delete a multi-byte character (本 at column 1)
        let deleted = doc
            .delete(Range::new(Position::new(0, 1), Position::new(0, 2)))
            .unwrap();
        assert_eq!(deleted, "本");
        assert_eq!(doc.text(), "日語");
    }
}
