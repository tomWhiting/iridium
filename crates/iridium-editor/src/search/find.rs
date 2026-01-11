//! Search/find functionality.

use serde::{Deserialize, Serialize};

use crate::document::Range;

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

/// State for find/replace operations.
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
}

impl SearchState {
    /// Creates a new empty search state.
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

    /// Moves to the next match.
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
    pub fn previous_match(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.current_match = Some(match self.current_match {
            Some(0) | None => self.matches.len().saturating_sub(1),
            Some(i) => i - 1,
        });
    }

    /// Clears the search state.
    pub fn clear(&mut self) {
        self.query.clear();
        self.matches.clear();
        self.current_match = None;
    }
}
