//! Rope-based text buffer implementation.
//!
//! Uses the ropey crate for efficient text storage and manipulation.
//! The rope data structure provides O(log n) performance for insertions
//! and deletions at arbitrary positions.

use ropey::Rope;

/// A text buffer backed by a rope data structure.
///
/// The `Buffer` provides efficient storage and manipulation of text content,
/// particularly for large files. All operations maintain O(log n) complexity.
#[derive(Debug, Clone)]
pub struct Buffer {
    /// The underlying rope storing the text content.
    rope: Rope,
}

impl Buffer {
    /// Creates a new empty buffer.
    ///
    /// # Examples
    ///
    /// ```
    /// use iridium_editor::Buffer;
    ///
    /// let buffer = Buffer::new();
    /// assert_eq!(buffer.len_chars(), 0);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self { rope: Rope::new() }
    }

    /// Creates a buffer from a string.
    ///
    /// This is a convenience method that calls `Buffer::from(text)`.
    /// You can also use `"text".parse::<Buffer>()` or `Buffer::from("text")`.
    ///
    /// # Examples
    ///
    /// ```
    /// use iridium_editor::Buffer;
    ///
    /// let buffer = Buffer::from("Hello, world!");
    /// assert_eq!(buffer.len_chars(), 13);
    /// ```
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        Self::from(text)
    }

    /// Returns the number of characters in the buffer.
    #[must_use]
    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    /// Returns the number of bytes in the buffer.
    #[must_use]
    pub fn len_bytes(&self) -> usize {
        self.rope.len_bytes()
    }

    /// Returns the number of lines in the buffer.
    ///
    /// An empty buffer has 1 line. A buffer ending with a newline
    /// has an additional empty final line.
    #[must_use]
    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    /// Returns true if the buffer is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rope.len_chars() == 0
    }

    /// Inserts text at the specified character index.
    ///
    /// # Panics
    ///
    /// Panics if `char_idx` is out of bounds (greater than `len_chars()`).
    pub fn insert(&mut self, char_idx: usize, text: &str) {
        self.rope.insert(char_idx, text);
    }

    /// Removes text in the specified character range.
    ///
    /// # Panics
    ///
    /// Panics if the range is out of bounds or if `start > end`.
    pub fn remove(&mut self, start: usize, end: usize) {
        self.rope.remove(start..end);
    }

    /// Returns the character at the specified index.
    ///
    /// # Panics
    ///
    /// Panics if `char_idx` is out of bounds.
    #[must_use]
    pub fn char_at(&self, char_idx: usize) -> char {
        self.rope.char(char_idx)
    }

    /// Returns the line at the specified line index.
    ///
    /// # Panics
    ///
    /// Panics if `line_idx` is out of bounds.
    #[must_use]
    pub fn line(&self, line_idx: usize) -> ropey::RopeSlice<'_> {
        self.rope.line(line_idx)
    }

    /// Returns the character index at the start of the specified line.
    ///
    /// # Panics
    ///
    /// Panics if `line_idx` is out of bounds.
    #[must_use]
    pub fn line_to_char(&self, line_idx: usize) -> usize {
        self.rope.line_to_char(line_idx)
    }

    /// Returns the line index containing the specified character index.
    ///
    /// # Panics
    ///
    /// Panics if `char_idx` is out of bounds.
    #[must_use]
    pub fn char_to_line(&self, char_idx: usize) -> usize {
        self.rope.char_to_line(char_idx)
    }

    /// Returns the entire buffer contents as a string.
    ///
    /// Note: This allocates a new String. For large buffers, prefer
    /// iterating over lines or chunks instead. Use `.to_string()` from
    /// the Display trait.
    #[must_use]
    pub fn contents(&self) -> String {
        self.rope.to_string()
    }

    /// Returns a slice of the buffer from start to end character indices.
    ///
    /// # Panics
    ///
    /// Panics if the range is out of bounds.
    #[must_use]
    pub fn slice(&self, start: usize, end: usize) -> ropey::RopeSlice<'_> {
        self.rope.slice(start..end)
    }

    /// Returns an iterator over the lines in the buffer.
    pub fn lines(&self) -> ropey::iter::Lines<'_> {
        self.rope.lines()
    }

    /// Replaces text in the specified range with new text.
    ///
    /// This is equivalent to a remove followed by an insert.
    ///
    /// # Panics
    ///
    /// Panics if the range is out of bounds.
    pub fn replace(&mut self, start: usize, end: usize, text: &str) {
        self.rope.remove(start..end);
        self.rope.insert(start, text);
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::str::FromStr for Buffer {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            rope: Rope::from_str(s),
        })
    }
}

impl std::fmt::Display for Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.rope)
    }
}

impl From<&str> for Buffer {
    fn from(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
        }
    }
}

impl From<String> for Buffer {
    fn from(text: String) -> Self {
        Self::from(&*text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_buffer_is_empty() {
        let buffer = Buffer::new();
        assert!(buffer.is_empty());
        assert_eq!(buffer.len_chars(), 0);
        assert_eq!(buffer.len_bytes(), 0);
        assert_eq!(buffer.len_lines(), 1); // Empty buffer has 1 line
    }

    #[test]
    fn test_from_str() {
        let buffer = Buffer::from("Hello");
        assert_eq!(buffer.len_chars(), 5);
        assert_eq!(buffer.to_string(), "Hello");
    }

    #[test]
    fn test_insert() {
        let mut buffer = Buffer::from("Hello");
        buffer.insert(5, ", world!");
        assert_eq!(buffer.to_string(), "Hello, world!");
    }

    #[test]
    fn test_insert_at_beginning() {
        let mut buffer = Buffer::from("world");
        buffer.insert(0, "Hello, ");
        assert_eq!(buffer.to_string(), "Hello, world");
    }

    #[test]
    fn test_insert_in_middle() {
        let mut buffer = Buffer::from("Helo");
        buffer.insert(2, "l");
        assert_eq!(buffer.to_string(), "Hello");
    }

    #[test]
    fn test_remove() {
        let mut buffer = Buffer::from("Hello, world!");
        buffer.remove(5, 13);
        assert_eq!(buffer.to_string(), "Hello");
    }

    #[test]
    fn test_remove_at_beginning() {
        let mut buffer = Buffer::from("Hello, world!");
        buffer.remove(0, 7);
        assert_eq!(buffer.to_string(), "world!");
    }

    #[test]
    fn test_replace() {
        let mut buffer = Buffer::from("Hello, world!");
        buffer.replace(7, 12, "Rust");
        assert_eq!(buffer.to_string(), "Hello, Rust!");
    }

    #[test]
    fn test_char_at() {
        let buffer = Buffer::from("Hello");
        assert_eq!(buffer.char_at(0), 'H');
        assert_eq!(buffer.char_at(4), 'o');
    }

    #[test]
    fn test_lines() {
        let buffer = Buffer::from("Line 1\nLine 2\nLine 3");
        assert_eq!(buffer.len_lines(), 3);

        let lines: Vec<String> = buffer.lines().map(|l| l.to_string()).collect();
        assert_eq!(lines[0], "Line 1\n");
        assert_eq!(lines[1], "Line 2\n");
        assert_eq!(lines[2], "Line 3");
    }

    #[test]
    fn test_line_to_char() {
        let buffer = Buffer::from("Line 1\nLine 2\nLine 3");
        assert_eq!(buffer.line_to_char(0), 0);
        assert_eq!(buffer.line_to_char(1), 7); // After "Line 1\n"
        assert_eq!(buffer.line_to_char(2), 14); // After "Line 2\n"
    }

    #[test]
    fn test_char_to_line() {
        let buffer = Buffer::from("Line 1\nLine 2\nLine 3");
        assert_eq!(buffer.char_to_line(0), 0);
        assert_eq!(buffer.char_to_line(6), 0); // Still on line 0
        assert_eq!(buffer.char_to_line(7), 1); // Start of line 1
        assert_eq!(buffer.char_to_line(14), 2); // Start of line 2
    }

    #[test]
    fn test_unicode_handling() {
        // "Hello, 世界! 🦀" = H e l l o , space 世 界 ! space 🦀 = 12 chars
        let buffer = Buffer::from("Hello, 世界! 🦀");
        assert_eq!(buffer.len_chars(), 12);
        assert_eq!(buffer.char_at(7), '世');
        assert_eq!(buffer.char_at(11), '🦀');
    }

    #[test]
    fn test_slice() {
        let buffer = Buffer::from("Hello, world!");
        let slice = buffer.slice(0, 5);
        assert_eq!(slice.to_string(), "Hello");
    }

    #[test]
    fn test_default() {
        let buffer: Buffer = Default::default();
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_from_string() {
        let buffer: Buffer = String::from("Hello").into();
        assert_eq!(buffer.to_string(), "Hello");
    }
}
