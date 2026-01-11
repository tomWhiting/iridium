//! Syntax highlighting engine.

use serde::{Deserialize, Serialize};

/// Types of syntax highlights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HighlightType {
    /// Language keywords (if, else, fn, etc.)
    Keyword,
    /// String literals
    String,
    /// Numeric literals
    Number,
    /// Comments
    Comment,
    /// Function names
    Function,
    /// Variable names
    Variable,
    /// Type names
    Type,
    /// Operators (+, -, *, etc.)
    Operator,
    /// Punctuation (braces, parentheses, etc.)
    Punctuation,
    /// Object properties
    Property,
    /// Constants
    Constant,
    /// HTML/XML tags
    Tag,
    /// HTML/XML attributes
    Attribute,
    /// Syntax errors
    Error,
}

/// A highlighted span of text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightSpan {
    /// Start byte offset
    pub start: usize,
    /// End byte offset
    pub end: usize,
    /// The highlight type
    pub highlight: HighlightType,
}

impl HighlightSpan {
    /// Creates a new highlight span.
    #[must_use]
    pub const fn new(start: usize, end: usize, highlight: HighlightType) -> Self {
        Self { start, end, highlight }
    }
}

/// Syntax highlighter using tree-sitter.
#[derive(Debug)]
pub struct Highlighter {
    language: super::Language,
    // Tree-sitter parser will be added during implementation
    _placeholder: (),
}

impl Highlighter {
    /// Creates a new highlighter for the given language.
    #[must_use]
    pub fn new(language: super::Language) -> Self {
        Self { language, _placeholder: () }
    }

    /// Returns the language this highlighter is configured for.
    #[must_use]
    pub const fn language(&self) -> super::Language {
        self.language
    }

    /// Highlights the given source code.
    ///
    /// Returns a list of highlight spans sorted by start position.
    #[must_use]
    pub fn highlight(&self, _source: &str) -> Vec<HighlightSpan> {
        // Tree-sitter highlighting will be implemented
        Vec::new()
    }

    /// Updates highlights incrementally after an edit.
    ///
    /// This is more efficient than re-highlighting the entire document.
    pub fn update(
        &mut self,
        _source: &str,
        _start_byte: usize,
        _old_end_byte: usize,
        _new_end_byte: usize,
    ) -> Vec<HighlightSpan> {
        // Incremental update will be implemented
        Vec::new()
    }
}
