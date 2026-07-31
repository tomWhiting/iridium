//! Syntax highlighting: parse tree in, coloured spans out.
//!
//! The rules come from the bundled query files (.scm) vendored from the Zed
//! editor; the tree comes from [`crate::SyntaxTree`], which this module reads
//! and never owns.

use serde::{Deserialize, Serialize};
use tree_sitter::{Query, QueryCursor, StreamingIterator, Tree};

use crate::SyntaxError;
use crate::query::{self, QueryKind};

/// Types of syntax highlights.
///
/// These map to semantic token types used in syntax highlighting.
/// The Zed query files use more specific capture names (e.g., @keyword.control,
/// @function.method) which are mapped to these broader categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HighlightType {
    /// Language keywords (if, else, fn, etc.)
    Keyword,
    /// Control flow keywords (if, else, for, while, return, etc.)
    KeywordControl,
    /// String literals
    String,
    /// String escape sequences
    StringEscape,
    /// Numeric literals
    Number,
    /// Boolean literals
    Boolean,
    /// Comments
    Comment,
    /// Documentation comments
    CommentDoc,
    /// Function names
    Function,
    /// Function definitions
    FunctionDefinition,
    /// Method names
    FunctionMethod,
    /// Special functions (macros, etc.)
    FunctionSpecial,
    /// Variable names
    Variable,
    /// Variable parameters
    VariableParameter,
    /// Special variables (self, this, etc.)
    VariableSpecial,
    /// Type names
    Type,
    /// Built-in types
    TypeBuiltin,
    /// Interface/trait types
    TypeInterface,
    /// Operators (+, -, *, etc.)
    Operator,
    /// Punctuation brackets ({, }, [, ], (, ))
    PunctuationBracket,
    /// Punctuation delimiters (., ,, ;, ::)
    PunctuationDelimiter,
    /// Special punctuation (#, etc.)
    PunctuationSpecial,
    /// Object properties/fields
    Property,
    /// Constants
    Constant,
    /// Lifetimes (Rust-specific)
    Lifetime,
    /// Attributes/decorators
    Attribute,
    /// HTML/XML tags
    Tag,
    /// Embedded content (e.g., code in markdown)
    Embedded,
    /// Syntax errors
    Error,
}

impl HighlightType {
    /// Maps a tree-sitter capture name to a `HighlightType`.
    ///
    /// Capture names follow Zed's convention with hierarchical naming
    /// (e.g., @keyword.control, @function.method).
    #[must_use]
    pub fn from_capture_name(name: &str) -> Option<Self> {
        // Handle hierarchical names by checking prefixes
        let name = name.trim_start_matches('@');

        // Exact matches first, consolidated to fix clippy::match_same_arms
        let result = match name {
            // Keywords
            "keyword" | "keyword.function" | "keyword.storage" | "keyword.modifier" => {
                Some(Self::Keyword)
            },
            "keyword.control" | "keyword.return" | "keyword.control.return" => {
                Some(Self::KeywordControl)
            },
            "keyword.operator" | "operator" => Some(Self::Operator),

            // Strings
            "string" | "string.literal" | "string.special" => Some(Self::String),
            "string.escape" | "escape_sequence" | "escape" => Some(Self::StringEscape),

            // Numbers and booleans
            "number" | "number.literal" | "integer" | "float" => Some(Self::Number),
            "boolean" | "constant.builtin.boolean" => Some(Self::Boolean),

            // Comments
            "comment" => Some(Self::Comment),
            "comment.doc" | "comment.documentation" => Some(Self::CommentDoc),

            // Functions
            "function" | "function.call" | "function.builtin" => Some(Self::Function),
            "function.definition" | "function.name" => Some(Self::FunctionDefinition),
            "function.method" | "method" | "method.call" => Some(Self::FunctionMethod),
            "function.special" | "function.macro" | "macro" | "function.special.definition" => {
                Some(Self::FunctionSpecial)
            },

            // Variables
            "variable" | "identifier" => Some(Self::Variable),
            "variable.parameter" | "parameter" => Some(Self::VariableParameter),
            "variable.special" | "variable.builtin" => Some(Self::VariableSpecial),

            // Types
            "type" | "type.name" | "type.definition" | "constructor" => Some(Self::Type),
            "type.builtin" | "type.primitive" => Some(Self::TypeBuiltin),
            "type.interface" | "interface" => Some(Self::TypeInterface),

            // Punctuation
            "punctuation.bracket" | "bracket" => Some(Self::PunctuationBracket),
            "punctuation.delimiter" | "delimiter" | "punctuation" => {
                Some(Self::PunctuationDelimiter)
            },
            "punctuation.special" => Some(Self::PunctuationSpecial),

            // Properties/fields
            "property" | "field" | "property.name" | "label" | "variable.member"
            | "variable.field" => Some(Self::Property),

            // Constants
            "constant" | "constant.builtin" | "enum" | "enumerator" => Some(Self::Constant),

            // Rust-specific
            "lifetime" => Some(Self::Lifetime),

            // Attributes/decorators
            "attribute" | "decorator" | "annotation" => Some(Self::Attribute),

            // Tags (HTML/XML)
            "tag" | "tag.name" => Some(Self::Tag),

            // Embedded content
            "embedded" => Some(Self::Embedded),

            // Error
            "error" => Some(Self::Error),

            // Fallback for common patterns
            _ => None,
        };

        // If no exact match, try prefix matching
        if result.is_some() {
            return result;
        }

        // Check for hierarchical prefixes
        if name.starts_with("keyword.control") {
            return Some(Self::KeywordControl);
        }
        if name.starts_with("keyword") {
            return Some(Self::Keyword);
        }
        if name.starts_with("string.escape") {
            return Some(Self::StringEscape);
        }
        if name.starts_with("string") {
            return Some(Self::String);
        }
        if name.starts_with("comment.doc") {
            return Some(Self::CommentDoc);
        }
        if name.starts_with("comment") {
            return Some(Self::Comment);
        }
        if name.starts_with("function.method") {
            return Some(Self::FunctionMethod);
        }
        if name.starts_with("function.special") {
            return Some(Self::FunctionSpecial);
        }
        if name.starts_with("function.definition") {
            return Some(Self::FunctionDefinition);
        }
        if name.starts_with("function") {
            return Some(Self::Function);
        }
        if name.starts_with("variable.parameter") {
            return Some(Self::VariableParameter);
        }
        if name.starts_with("variable.special") || name.starts_with("variable.builtin") {
            return Some(Self::VariableSpecial);
        }
        if name.starts_with("variable") {
            return Some(Self::Variable);
        }
        if name.starts_with("type.builtin") {
            return Some(Self::TypeBuiltin);
        }
        if name.starts_with("type.interface") {
            return Some(Self::TypeInterface);
        }
        if name.starts_with("type") {
            return Some(Self::Type);
        }
        if name.starts_with("punctuation.bracket") {
            return Some(Self::PunctuationBracket);
        }
        if name.starts_with("punctuation.delimiter") {
            return Some(Self::PunctuationDelimiter);
        }
        if name.starts_with("punctuation") {
            return Some(Self::PunctuationDelimiter);
        }
        if name.starts_with("constant") {
            return Some(Self::Constant);
        }
        if name.starts_with("property") {
            return Some(Self::Property);
        }
        if name.starts_with("attribute") {
            return Some(Self::Attribute);
        }
        if name.starts_with("number") {
            return Some(Self::Number);
        }

        None
    }
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
        Self {
            start,
            end,
            highlight,
        }
    }
}

impl PartialOrd for HighlightSpan {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HighlightSpan {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.start
            .cmp(&other.start)
            .then_with(|| self.end.cmp(&other.end))
    }
}

/// Maps a parse tree's nodes to highlight spans, for one language.
///
/// The highlighter owns no parser and no tree. It holds the rules — a compiled
/// `highlights.scm` and the capture-name mapping — and reads a [`Tree`] someone
/// else parsed, so there is no second copy of the document's structure that
/// could disagree with the first. See [`crate::SyntaxTree`] for the owner.
///
/// The query itself is borrowed from the process-wide cache in [`crate::query`]:
/// `highlights.scm` runs to thousands of patterns, and every open buffer of a
/// language compiling its own copy put that cost on the path that opens a file.
pub struct Highlighter {
    language: crate::Language,
    query: &'static Query,
    capture_names: Vec<Option<HighlightType>>,
}

impl std::fmt::Debug for Highlighter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Highlighter")
            .field("language", &self.language)
            .field("captures", &self.capture_names.len())
            .finish_non_exhaustive()
    }
}

impl Highlighter {
    /// Creates a highlighter for the given language.
    ///
    /// # Errors
    ///
    /// Returns an error if the language's bundled `highlights.scm` does not
    /// compile against its grammar. Every supported language ships one, so this
    /// is never routine — it means a dependency moved underneath a vendored
    /// file.
    pub fn new(language: crate::Language) -> Result<Self, SyntaxError> {
        let query = query::compiled(language, QueryKind::Highlights)?.ok_or_else(|| {
            SyntaxError::QueryError {
                message: format!("No highlights query for language: {}", language.id()),
            }
        })?;

        // Pre-compute capture name mappings for performance
        let capture_names = query
            .capture_names()
            .iter()
            .map(|name| HighlightType::from_capture_name(name))
            .collect();

        Ok(Self {
            language,
            query,
            capture_names,
        })
    }

    /// Creates a highlighter that will work without crashing, even if the
    /// language isn't fully supported. Returns None for unsupported languages.
    #[must_use]
    pub fn try_new(language: crate::Language) -> Option<Self> {
        Self::new(language).ok()
    }

    /// Returns the language this highlighter is configured for.
    #[must_use]
    pub const fn language(&self) -> crate::Language {
        self.language
    }

    /// Returns the highlight spans for `tree`, sorted by position.
    ///
    /// `source` must be the exact text `tree` was parsed from: every span is a
    /// byte range into it, and a mismatch would colour the wrong characters.
    ///
    /// Spans may overlap where one capture nests inside another; the renderer
    /// decides precedence.
    #[must_use]
    pub fn spans_in(&self, tree: &Tree, source: &str) -> Vec<HighlightSpan> {
        let mut spans = Vec::new();
        let mut cursor = QueryCursor::new();

        // Note: tree-sitter 0.26 uses StreamingIterator instead of Iterator
        let mut matches = cursor.matches(self.query, tree.root_node(), source.as_bytes());

        while let Some(match_) = matches.next() {
            for capture in match_.captures {
                let capture_idx = capture.index as usize;

                // Skip captures that don't map to a highlight type
                let Some(Some(highlight_type)) = self.capture_names.get(capture_idx).copied()
                else {
                    continue;
                };

                let node = capture.node;
                let start = node.start_byte();
                let end = node.end_byte();

                // Skip empty spans
                if start >= end {
                    continue;
                }

                spans.push(HighlightSpan::new(start, end, highlight_type));
            }
        }

        // Sort by start position, then by end position
        spans.sort();

        // Remove duplicates (same start/end/type)
        spans.dedup();

        spans
    }
}

#[cfg(test)]
mod tests;
