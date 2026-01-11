//! Search/find functionality.
//!
//! This module provides comprehensive search functionality including:
//! - Plain text search
//! - Regular expression search
//! - Case-sensitive and case-insensitive modes
//! - Whole-word matching
//! - Match navigation (next/previous)
//!
//! # Example
//!
//! ```
//! use iridium_editor::search::{SearchOptions, SearchState};
//! use iridium_editor::Document;
//!
//! let doc = Document::new("foo bar foo baz");
//! let mut state = SearchState::new();
//! state.find_all("foo", &SearchOptions::default(), &doc);
//!
//! assert_eq!(state.match_count(), 2);
//! // After find_all, current_match is set to the first match
//! assert_eq!(state.current_match, Some(0));
//! state.next_match();
//! // After next_match, we're at the second match
//! assert_eq!(state.current_match, Some(1));
//! ```

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

use crate::document::{Document, Range, Position};

/// Search options for find operations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchOptions {
    /// Case-sensitive matching (default: false)
    pub case_sensitive: bool,
    /// Match whole words only (default: false)
    pub whole_word: bool,
    /// Interpret query as regex (default: false)
    pub regex: bool,
}

impl SearchOptions {
    /// Creates search options with case sensitivity enabled.
    #[must_use]
    pub const fn case_sensitive() -> Self {
        Self {
            case_sensitive: true,
            whole_word: false,
            regex: false,
        }
    }

    /// Creates search options with regex mode enabled.
    #[must_use]
    pub const fn regex_mode() -> Self {
        Self {
            case_sensitive: false,
            whole_word: false,
            regex: true,
        }
    }

    /// Creates search options with whole-word matching enabled.
    #[must_use]
    pub const fn whole_word() -> Self {
        Self {
            case_sensitive: false,
            whole_word: true,
            regex: false,
        }
    }
}

/// State for find/replace operations.
///
/// This struct manages the current search query, options, and match results.
/// It provides methods for finding all matches and navigating between them.
#[derive(Debug, Clone, Default)]
pub struct SearchState {
    /// Current search query
    pub query: String,
    /// Search options
    pub options: SearchOptions,
    /// All matches in document
    pub matches: Vec<Range>,
    /// Currently selected match index
    pub current_match: Option<usize>,
    /// Cached regex pattern (if regex mode)
    compiled_regex: Option<String>,
    /// Whether the search is active
    pub is_active: bool,
}

impl SearchState {
    /// Creates a new empty search state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of matches.
    #[must_use]
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// Returns the current match range.
    #[must_use]
    pub fn current_range(&self) -> Option<&Range> {
        self.current_match.and_then(|i| self.matches.get(i))
    }

    /// Returns true if there are any matches.
    #[must_use]
    pub fn has_matches(&self) -> bool {
        !self.matches.is_empty()
    }

    /// Moves to the next match.
    ///
    /// Wraps around to the first match when at the end.
    pub fn next_match(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.current_match = Some(match self.current_match {
            Some(i) => (i + 1) % self.matches.len(),
            None => 0,
        });
    }

    /// Moves to the previous match.
    ///
    /// Wraps around to the last match when at the beginning.
    pub fn previous_match(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.current_match = Some(match self.current_match {
            Some(0) | None => self.matches.len().saturating_sub(1),
            Some(i) => i - 1,
        });
    }

    /// Navigates to the match nearest to the given position.
    ///
    /// Sets `current_match` to the match whose start is closest to (or just after)
    /// the provided position. This is useful when starting a search to position
    /// the user at the nearest match to their cursor.
    pub fn goto_nearest_match(&mut self, position: Position) {
        if self.matches.is_empty() {
            return;
        }

        // Find the first match that starts at or after the position
        let idx = self.matches.iter().position(|r| r.start >= position);

        self.current_match = Some(match idx {
            Some(i) => i,
            None => 0, // Wrap to first match if none found after position
        });
    }

    /// Finds all matches of the query in the document.
    ///
    /// This method updates the internal `matches` vector with all found ranges
    /// and optionally sets the current match to the first one.
    ///
    /// # Arguments
    ///
    /// * `query` - The search query (plain text or regex pattern)
    /// * `options` - Search options controlling matching behavior
    /// * `document` - The document to search in
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or `Err(String)` if the regex pattern is invalid.
    pub fn find_all(
        &mut self,
        query: &str,
        options: &SearchOptions,
        document: &Document,
    ) -> Result<(), String> {
        self.query = query.to_string();
        self.options = options.clone();
        self.matches.clear();
        self.current_match = None;
        self.is_active = !query.is_empty();

        if query.is_empty() {
            return Ok(());
        }

        let text = document.text();

        if options.regex {
            self.find_regex_matches(&text, query, options, document)?;
        } else {
            self.find_literal_matches(&text, query, options, document);
        }

        // Set current match to first if there are matches
        if !self.matches.is_empty() {
            self.current_match = Some(0);
        }

        Ok(())
    }

    /// Finds all literal (non-regex) matches.
    fn find_literal_matches(
        &mut self,
        text: &str,
        query: &str,
        options: &SearchOptions,
        document: &Document,
    ) {
        let search_text: String;
        let search_query: String;

        // Handle case insensitivity by converting to lowercase
        let (haystack, needle) = if options.case_sensitive {
            (text, query)
        } else {
            search_text = text.to_lowercase();
            search_query = query.to_lowercase();
            (search_text.as_str(), search_query.as_str())
        };

        let mut start = 0;
        while let Some(offset) = haystack[start..].find(needle) {
            let match_start = start + offset;
            let match_end = match_start + query.len();

            // Check whole-word boundary if required
            if options.whole_word && !self.is_word_boundary(text, match_start, match_end) {
                start = match_start + 1;
                continue;
            }

            // Convert byte offsets to positions
            if let (Some(start_pos), Some(end_pos)) = (
                document.offset_to_position(match_start),
                document.offset_to_position(match_end),
            ) {
                self.matches.push(Range::new(start_pos, end_pos));
            }

            start = match_start + 1;
        }
    }

    /// Finds all regex matches.
    fn find_regex_matches(
        &mut self,
        text: &str,
        pattern: &str,
        options: &SearchOptions,
        document: &Document,
    ) -> Result<(), String> {
        // Build regex with options
        let regex = RegexBuilder::new(pattern)
            .case_insensitive(!options.case_sensitive)
            .build()
            .map_err(|e| format!("Invalid regex: {e}"))?;

        self.compiled_regex = Some(pattern.to_string());

        for mat in regex.find_iter(text) {
            let match_start = mat.start();
            let match_end = mat.end();

            // Check whole-word boundary if required
            if options.whole_word && !self.is_word_boundary(text, match_start, match_end) {
                continue;
            }

            // Convert byte offsets to positions
            if let (Some(start_pos), Some(end_pos)) = (
                document.offset_to_position(match_start),
                document.offset_to_position(match_end),
            ) {
                self.matches.push(Range::new(start_pos, end_pos));
            }
        }

        Ok(())
    }

    /// Checks if a match is at a word boundary.
    fn is_word_boundary(&self, text: &str, start: usize, end: usize) -> bool {
        let bytes = text.as_bytes();

        // Check character before start
        let at_word_start = if start == 0 {
            true
        } else {
            let prev_char = bytes.get(start - 1).copied().unwrap_or(b' ');
            !Self::is_word_char(prev_char)
        };

        // Check character after end
        let at_word_end = if end >= bytes.len() {
            true
        } else {
            let next_char = bytes.get(end).copied().unwrap_or(b' ');
            !Self::is_word_char(next_char)
        };

        at_word_start && at_word_end
    }

    /// Returns true if the byte is a word character (alphanumeric or underscore).
    fn is_word_char(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || byte == b'_'
    }

    /// Updates the search with new query, returning whether matches changed.
    ///
    /// This is useful for incremental search where the query is updated
    /// as the user types.
    pub fn update_query(
        &mut self,
        query: &str,
        document: &Document,
    ) -> Result<bool, String> {
        let old_count = self.matches.len();
        self.find_all(query, &self.options.clone(), document)?;
        Ok(self.matches.len() != old_count)
    }

    /// Sets search options and re-runs the search if a query exists.
    pub fn set_options(
        &mut self,
        options: SearchOptions,
        document: &Document,
    ) -> Result<(), String> {
        if !self.query.is_empty() {
            self.find_all(&self.query.clone(), &options, document)?;
        } else {
            self.options = options;
        }
        Ok(())
    }

    /// Clears the search state.
    pub fn clear(&mut self) {
        self.query.clear();
        self.matches.clear();
        self.current_match = None;
        self.compiled_regex = None;
        self.is_active = false;
    }

    /// Returns all match ranges.
    #[must_use]
    pub fn all_matches(&self) -> &[Range] {
        &self.matches
    }

    /// Creates a pattern string for escaping special regex characters.
    ///
    /// Use this when you want to search for literal text that may contain
    /// regex special characters.
    #[must_use]
    pub fn escape_regex(text: &str) -> String {
        regex::escape(text)
    }

    /// Validates a regex pattern without performing a search.
    #[must_use]
    pub fn validate_regex(pattern: &str) -> Result<(), String> {
        Regex::new(pattern).map(|_| ()).map_err(|e| format!("Invalid regex: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_doc() -> Document {
        Document::new("foo bar foo baz foo\nfoo in line 2")
    }

    #[test]
    fn find_all_basic() {
        let doc = create_test_doc();
        let mut state = SearchState::new();
        state.find_all("foo", &SearchOptions::default(), &doc).unwrap();

        assert_eq!(state.match_count(), 4);
        assert_eq!(state.current_match, Some(0));
    }

    #[test]
    fn find_all_case_insensitive() {
        let doc = Document::new("FOO foo Foo fOO");
        let mut state = SearchState::new();
        state.find_all("foo", &SearchOptions::default(), &doc).unwrap();

        // Default is case-insensitive
        assert_eq!(state.match_count(), 4);
    }

    #[test]
    fn find_all_case_sensitive() {
        let doc = Document::new("FOO foo Foo fOO");
        let mut state = SearchState::new();
        state.find_all("foo", &SearchOptions::case_sensitive(), &doc).unwrap();

        assert_eq!(state.match_count(), 1);
    }

    #[test]
    fn find_all_whole_word() {
        let doc = Document::new("foo foobar barfoo foo");
        let mut state = SearchState::new();
        state.find_all("foo", &SearchOptions::whole_word(), &doc).unwrap();

        // Should only match standalone "foo", not "foobar" or "barfoo"
        assert_eq!(state.match_count(), 2);
    }

    #[test]
    fn find_all_regex() {
        let doc = Document::new("foo123 bar456 baz789");
        let mut state = SearchState::new();
        state.find_all(r"\w+\d+", &SearchOptions::regex_mode(), &doc).unwrap();

        assert_eq!(state.match_count(), 3);
    }

    #[test]
    fn find_all_regex_invalid() {
        let doc = Document::new("test");
        let mut state = SearchState::new();
        let result = state.find_all("[invalid", &SearchOptions::regex_mode(), &doc);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid regex"));
    }

    #[test]
    fn next_match_wraps() {
        let doc = Document::new("foo foo");
        let mut state = SearchState::new();
        state.find_all("foo", &SearchOptions::default(), &doc).unwrap();

        assert_eq!(state.current_match, Some(0));
        state.next_match();
        assert_eq!(state.current_match, Some(1));
        state.next_match();
        assert_eq!(state.current_match, Some(0)); // Wrapped
    }

    #[test]
    fn previous_match_wraps() {
        let doc = Document::new("foo foo");
        let mut state = SearchState::new();
        state.find_all("foo", &SearchOptions::default(), &doc).unwrap();

        assert_eq!(state.current_match, Some(0));
        state.previous_match();
        assert_eq!(state.current_match, Some(1)); // Wrapped to last
    }

    #[test]
    fn current_range() {
        let doc = Document::new("foo bar");
        let mut state = SearchState::new();
        state.find_all("foo", &SearchOptions::default(), &doc).unwrap();

        let range = state.current_range().unwrap();
        assert_eq!(range.start, Position::new(0, 0));
        assert_eq!(range.end, Position::new(0, 3));
    }

    #[test]
    fn goto_nearest_match() {
        let doc = Document::new("foo bar foo baz foo");
        let mut state = SearchState::new();
        state.find_all("foo", &SearchOptions::default(), &doc).unwrap();

        // Start at position after second foo
        state.goto_nearest_match(Position::new(0, 10));

        // Should go to third foo at position 16
        assert_eq!(state.current_match, Some(2));
    }

    #[test]
    fn clear_resets_state() {
        let doc = Document::new("foo bar");
        let mut state = SearchState::new();
        state.find_all("foo", &SearchOptions::default(), &doc).unwrap();
        assert!(state.has_matches());

        state.clear();
        assert!(!state.has_matches());
        assert!(state.query.is_empty());
        assert!(!state.is_active);
    }

    #[test]
    fn empty_query_clears_matches() {
        let doc = Document::new("foo bar");
        let mut state = SearchState::new();
        state.find_all("foo", &SearchOptions::default(), &doc).unwrap();
        assert!(state.has_matches());

        state.find_all("", &SearchOptions::default(), &doc).unwrap();
        assert!(!state.has_matches());
        assert!(!state.is_active);
    }

    #[test]
    fn multiline_search() {
        let doc = Document::new("foo bar\nfoo baz");
        let mut state = SearchState::new();
        state.find_all("foo", &SearchOptions::default(), &doc).unwrap();

        assert_eq!(state.match_count(), 2);

        // First match on line 0
        let first = &state.matches[0];
        assert_eq!(first.start.line, 0);
        assert_eq!(first.start.column, 0);

        // Second match on line 1
        let second = &state.matches[1];
        assert_eq!(second.start.line, 1);
        assert_eq!(second.start.column, 0);
    }

    #[test]
    fn update_query_detects_changes() {
        let doc = Document::new("foo bar baz");
        let mut state = SearchState::new();
        state.find_all("foo", &SearchOptions::default(), &doc).unwrap();
        assert_eq!(state.match_count(), 1);

        // Query that finds different number of matches
        let changed = state.update_query("a", &doc).unwrap();
        assert!(changed);
        assert_eq!(state.match_count(), 2); // "a" in bar and baz
    }

    #[test]
    fn escape_regex_special_chars() {
        assert_eq!(SearchState::escape_regex("foo.bar"), r"foo\.bar");
        assert_eq!(SearchState::escape_regex("[test]"), r"\[test\]");
    }

    #[test]
    fn validate_regex_valid() {
        assert!(SearchState::validate_regex(r"\w+").is_ok());
    }

    #[test]
    fn validate_regex_invalid() {
        assert!(SearchState::validate_regex("[invalid").is_err());
    }
}
