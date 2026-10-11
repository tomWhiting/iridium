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

use super::positions::MatchPositions;
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

use crate::document::{Document, Position, Range};
use crate::input::keyboard::motions;

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
    /// Diagnostic from the latest search, including automatic refresh.
    error: Option<String>,
    searched_revision: Option<u64>,
    /// Selected-text scope, independent of the moving search caret.
    pub(super) scope: Option<super::scope::SearchScope>,
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

        self.current_match = Some(idx.unwrap_or(0));
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
        self.run_search(query, options, document);
        self.error.clone().map_or(Ok(()), Err)
    }

    /// Returns the latest diagnostic. A successful search or clear removes it.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Re-runs the current query and preserves the nearest current match.
    /// Diagnostics remain available through [`Self::error`]; invalid queries
    /// leave no stale matches. Returns whether a nonempty query was refreshed.
    pub fn refresh(&mut self, document: &Document) -> bool {
        if self.query.is_empty() && self.scope.is_none() {
            return false;
        }
        let previous = self.current_range().map(|range| range.start);
        self.run_search(&self.query.clone(), &self.options.clone(), document);
        if let Some(position) = previous {
            self.goto_nearest_match(position);
        }
        true
    }

    /// Publishes results or a diagnostic as one authoritative search outcome.
    fn run_search(&mut self, query: &str, options: &SearchOptions, document: &Document) {
        self.searched_revision = Some(document.revision());
        self.query = query.to_string();
        self.options = options.clone();
        self.matches.clear();
        self.current_match = None;
        self.compiled_regex = None;
        self.error = None;
        self.is_active = !query.is_empty();
        if let Some(scope) = &mut self.scope {
            if !scope.is_current(document) {
                scope.invalidate();
                self.error = Some(super::scope::STALE_SCOPE.to_owned());
                return;
            }
        }
        if query.is_empty() {
            return;
        }
        let text = document.text();
        self.error = if options.regex {
            self.find_regex_matches(&text, query, options)
        } else {
            self.find_literal_matches(&text, query, options)
        }
        .err();
        if self.error.is_some() {
            self.matches.clear();
        } else if !self.matches.is_empty() {
            self.current_match = Some(0);
        }
    }

    /// Finds all literal (non-regex) matches.
    ///
    /// The case-sensitive path uses substring search. The case-insensitive
    /// path uses Unicode simple case folding with original-text byte offsets;
    /// neither lowercasing nor normalization rewrites the document being searched.
    /// Matcher construction errors are returned to the search UI.
    fn find_literal_matches(
        &mut self,
        text: &str,
        query: &str,
        options: &SearchOptions,
    ) -> Result<(), String> {
        let mut positions = MatchPositions::new(text);
        if options.case_sensitive {
            let mut start = 0;
            while let Some(offset) = text[start..].find(query) {
                let match_start = start + offset;
                let match_end = match_start + query.len();

                // Resume one *character* past the match start, never one byte.
                // `start` indexes `text` directly on the next iteration, so a
                // byte inside a multi-byte character panics there — searching
                // "é" in "ééé" used to crash on the second iteration. The
                // case-insensitive path below already advances this way; on
                // ASCII the two are the same step, so the overlapping-match
                // semantics are unchanged.
                start = match_start + text[match_start..].chars().next().map_or(1, char::len_utf8);

                // Check whole-word boundary if required
                if options.whole_word && !Self::is_word_boundary(text, match_start, match_end) {
                    continue;
                }

                if self.within_scope(match_start, match_end) {
                    self.matches.push(positions.range(match_start, match_end)?);
                }
            }
            return Ok(());
        }

        let matcher = super::literal::FoldedLiteral::new(query)?;
        for range in matcher.matches(text) {
            if options.whole_word && !Self::is_word_boundary(text, range.start, range.end) {
                continue;
            }
            if self.within_scope(range.start, range.end) {
                self.matches.push(positions.range(range.start, range.end)?);
            }
        }

        Ok(())
    }

    /// Finds all regex matches.
    fn find_regex_matches(
        &mut self,
        text: &str,
        pattern: &str,
        options: &SearchOptions,
    ) -> Result<(), String> {
        let mut positions = MatchPositions::new(text);
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
            if options.whole_word && !Self::is_word_boundary(text, match_start, match_end) {
                continue;
            }

            if self.within_scope(match_start, match_end) {
                self.matches.push(positions.range(match_start, match_end)?);
            }
        }

        Ok(())
    }

    /// Whole-word matches require complete graphemes with non-word neighbours.
    /// Invalid offsets are rejected instead of being treated as boundaries.
    /// Both search and regex replacement use this predicate against full text.
    pub(super) fn is_word_boundary(text: &str, start: usize, end: usize) -> bool {
        if start > end || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            return false;
        }
        for offset in [start, end] {
            if GraphemeCursor::new(offset, text.len(), true).is_boundary(text, 0) != Ok(true) {
                return false;
            }
        }
        let before = text[..start].graphemes(true).next_back();
        let after = text[end..].graphemes(true).next();
        !before.is_some_and(motions::is_word_cluster)
            && !after.is_some_and(motions::is_word_cluster)
    }

    /// Updates the search with new query, returning whether matches changed.
    ///
    /// This is useful for incremental search where the query is updated
    /// as the user types.
    pub fn update_query(&mut self, query: &str, document: &Document) -> Result<bool, String> {
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
        self.searched_revision = None;
        self.query.clear();
        self.matches.clear();
        self.current_match = None;
        self.compiled_regex = None;
        self.error = None;
        self.is_active = false;
        self.scope = None;
    }

    pub(crate) fn clear_scope(&mut self) {
        self.scope = None;
        if self.query.is_empty() {
            self.error = None;
        }
    }

    pub(crate) fn refresh_if_stale(&mut self, document: &Document) -> bool {
        self.searched_revision != Some(document.revision()) && self.refresh(document)
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
    pub fn validate_regex(pattern: &str) -> Result<(), String> {
        Regex::new(pattern)
            .map(|_| ())
            .map_err(|e| format!("Invalid regex: {e}"))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;
