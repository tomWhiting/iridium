//! Syntax highlighting engine using tree-sitter.
//!
//! This module provides incremental syntax highlighting using tree-sitter
//! grammars and bundled query files (.scm) from the Zed editor.

use serde::{Deserialize, Serialize};
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator, Tree};

use crate::SyntaxError;
use crate::grammar::grammar;
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

/// Syntax highlighter using tree-sitter.
///
/// The highlighter maintains a parser and parse tree for incremental updates,
/// and maps syntax nodes to highlight types through the shared compiled query
/// for its language.
///
/// The query is borrowed from the process-wide cache in [`crate::query`] rather
/// than owned: `highlights.scm` runs to thousands of patterns, and every open
/// buffer of a language compiling its own copy put that cost on the path that
/// opens a file.
pub struct Highlighter {
    language: crate::Language,
    parser: Parser,
    tree: Option<Tree>,
    query: &'static Query,
    capture_names: Vec<Option<HighlightType>>,
}

impl std::fmt::Debug for Highlighter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Highlighter")
            .field("language", &self.language)
            .field("has_tree", &self.tree.is_some())
            .field("captures", &self.capture_names.len())
            .finish_non_exhaustive()
    }
}

impl Highlighter {
    /// Creates a new highlighter for the given language.
    ///
    /// # Errors
    ///
    /// Returns an error if the language's grammar is incompatible with this
    /// build of tree-sitter, or if its bundled `highlights.scm` does not
    /// compile against that grammar. Every supported language has both, so
    /// neither failure is routine — both mean a dependency moved underneath a
    /// vendored file.
    pub fn new(language: crate::Language) -> Result<Self, SyntaxError> {
        let mut parser = Parser::new();
        parser
            .set_language(&grammar(language))
            .map_err(|e| SyntaxError::ParseError {
                message: format!("Failed to set language: {e}"),
            })?;

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
            parser,
            tree: None,
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

    /// Highlights the given source code.
    ///
    /// Returns a list of highlight spans sorted by start position.
    /// Spans may overlap; the renderer should handle precedence.
    #[must_use]
    pub fn highlight(&mut self, source: &str) -> Vec<HighlightSpan> {
        // Parse the source
        self.tree = self.parser.parse(source, None);

        let Some(tree) = &self.tree else {
            return Vec::new();
        };

        let mut spans = Vec::new();
        let mut cursor = QueryCursor::new();

        // Execute the query against the tree
        // Note: tree-sitter 0.26 uses StreamingIterator instead of Iterator
        let mut matches = cursor.matches(self.query, tree.root_node(), source.as_bytes());

        while let Some(match_) = matches.next() {
            for capture in match_.captures {
                let capture_idx = capture.index as usize;

                // Skip captures that don't map to a highlight type
                if capture_idx >= self.capture_names.len() {
                    continue;
                }

                let Some(highlight_type) = self.capture_names[capture_idx] else {
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

    /// Updates highlights incrementally after an edit.
    ///
    /// This is more efficient than re-highlighting the entire document
    /// because tree-sitter can reuse unchanged parts of the parse tree.
    ///
    /// # Arguments
    ///
    /// * `source` - The new source code after the edit
    /// * `start_byte` - The byte offset where the edit started
    /// * `old_end_byte` - The byte offset where the edit ended in the old source
    /// * `new_end_byte` - The byte offset where the edit ended in the new source
    #[must_use]
    pub fn update(
        &mut self,
        source: &str,
        start_byte: usize,
        old_end_byte: usize,
        new_end_byte: usize,
    ) -> Vec<HighlightSpan> {
        // If we don't have an existing tree, just do a full parse
        let Some(old_tree) = self.tree.take() else {
            return self.highlight(source);
        };

        // Create the edit descriptor for tree-sitter
        let input_edit = tree_sitter::InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position: byte_to_point(source, start_byte),
            old_end_position: byte_to_point(source, old_end_byte),
            new_end_position: byte_to_point(source, new_end_byte),
        };

        // Clone the old tree and apply the edit
        let mut edited_tree = old_tree;
        edited_tree.edit(&input_edit);

        // Reparse with the edited tree as a reference
        self.tree = self.parser.parse(source, Some(&edited_tree));

        let Some(tree) = &self.tree else {
            return Vec::new();
        };

        // For incremental highlighting, we could be smarter about only
        // re-querying changed regions, but for now we re-query everything.
        // The actual performance win comes from tree-sitter's incremental parsing.
        let mut spans = Vec::new();
        let mut cursor = QueryCursor::new();

        // Use StreamingIterator for tree-sitter 0.26+
        let mut matches = cursor.matches(self.query, tree.root_node(), source.as_bytes());

        while let Some(match_) = matches.next() {
            for capture in match_.captures {
                let capture_idx = capture.index as usize;

                if capture_idx >= self.capture_names.len() {
                    continue;
                }

                let Some(highlight_type) = self.capture_names[capture_idx] else {
                    continue;
                };

                let node = capture.node;
                let start = node.start_byte();
                let end = node.end_byte();

                if start >= end {
                    continue;
                }

                spans.push(HighlightSpan::new(start, end, highlight_type));
            }
        }

        spans.sort();
        spans.dedup();
        spans
    }

    /// Returns the ranges that changed since the last parse.
    ///
    /// This can be used by the renderer to only update affected regions.
    #[must_use]
    pub fn changed_ranges(&self, old_tree: &Tree) -> Vec<std::ops::Range<usize>> {
        let Some(new_tree) = &self.tree else {
            return Vec::new();
        };

        old_tree
            .changed_ranges(new_tree)
            .map(|r| r.start_byte..r.end_byte)
            .collect()
    }
}

/// Convert a byte offset to a tree-sitter Point (row, column).
fn byte_to_point(source: &str, byte_offset: usize) -> tree_sitter::Point {
    let mut row = 0;
    let mut col = 0;
    let mut current_byte = 0;

    for ch in source.chars() {
        if current_byte >= byte_offset {
            break;
        }

        if ch == '\n' {
            row += 1;
            col = 0;
        } else {
            col += ch.len_utf8();
        }

        current_byte += ch.len_utf8();
    }

    tree_sitter::Point { row, column: col }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::similar_names)]
mod tests {
    use super::*;
    use crate::Language;

    #[test]
    fn test_highlight_type_from_capture_name() {
        assert_eq!(
            HighlightType::from_capture_name("keyword"),
            Some(HighlightType::Keyword)
        );
        assert_eq!(
            HighlightType::from_capture_name("@keyword"),
            Some(HighlightType::Keyword)
        );
        assert_eq!(
            HighlightType::from_capture_name("keyword.control"),
            Some(HighlightType::KeywordControl)
        );
        assert_eq!(
            HighlightType::from_capture_name("function.method"),
            Some(HighlightType::FunctionMethod)
        );
        assert_eq!(
            HighlightType::from_capture_name("string"),
            Some(HighlightType::String)
        );
        assert_eq!(
            HighlightType::from_capture_name("comment.doc"),
            Some(HighlightType::CommentDoc)
        );
    }

    #[test]
    fn test_highlighter_rust() {
        let mut highlighter = Highlighter::new(Language::Rust).expect("Rust should be supported");
        let source = "fn main() { let x = 42; }";
        let spans = highlighter.highlight(source);

        // Should have at least some spans
        assert!(
            !spans.is_empty(),
            "Rust code should produce highlight spans"
        );

        // Verify the 'fn' keyword is highlighted
        let has_fn_span = spans.iter().any(|s| s.start == 0 && s.end == 2);
        assert!(has_fn_span, "'fn' should be highlighted");
    }

    #[test]
    fn test_highlighter_python() {
        let mut highlighter =
            Highlighter::new(Language::Python).expect("Python should be supported");
        let source = "def hello():\n    print('Hello')";
        let spans = highlighter.highlight(source);

        assert!(
            !spans.is_empty(),
            "Python code should produce highlight spans"
        );
    }

    #[test]
    fn test_highlighter_typescript() {
        let mut highlighter =
            Highlighter::new(Language::TypeScript).expect("TypeScript should be supported");
        let source = "function greet(name: string): void { console.log(name); }";
        let spans = highlighter.highlight(source);

        assert!(
            !spans.is_empty(),
            "TypeScript code should produce highlight spans"
        );
    }

    #[test]
    fn test_highlighter_javascript() {
        let mut highlighter =
            Highlighter::new(Language::JavaScript).expect("JavaScript should be supported");
        let source = "const x = 42; function foo() { return x; }";
        let spans = highlighter.highlight(source);

        assert!(
            !spans.is_empty(),
            "JavaScript code should produce highlight spans"
        );
    }

    #[test]
    fn test_highlighter_go() {
        let mut highlighter = Highlighter::new(Language::Go).expect("Go should be supported");
        let source = "package main\n\nfunc main() { fmt.Println(\"Hello\") }";
        let spans = highlighter.highlight(source);

        assert!(!spans.is_empty(), "Go code should produce highlight spans");
    }

    #[test]
    fn test_highlighter_json() {
        let mut highlighter = Highlighter::new(Language::Json).expect("JSON should be supported");
        let source = r#"{"name": "test", "value": 42, "active": true}"#;
        let spans = highlighter.highlight(source);

        assert!(
            !spans.is_empty(),
            "JSON code should produce highlight spans"
        );
    }

    #[test]
    fn test_highlighter_yaml() {
        let mut highlighter = Highlighter::new(Language::Yaml).expect("YAML should be supported");
        let source = "name: test\nvalue: 42\nactive: true";
        let spans = highlighter.highlight(source);

        // YAML highlighting might produce different results depending on the grammar
        // Just verify no crash
        let _ = spans.len();
    }

    #[test]
    fn test_highlighter_css() {
        let mut highlighter = Highlighter::new(Language::Css).expect("CSS should be supported");
        let source = ".class { color: red; font-size: 12px; }";
        let spans = highlighter.highlight(source);

        assert!(!spans.is_empty(), "CSS code should produce highlight spans");
    }

    #[test]
    fn test_highlighter_bash() {
        let mut highlighter = Highlighter::new(Language::Bash).expect("Bash should be supported");
        let source = "#!/bin/bash\necho \"Hello World\"";
        let spans = highlighter.highlight(source);

        assert!(
            !spans.is_empty(),
            "Bash code should produce highlight spans"
        );
    }

    #[test]
    fn test_highlighter_c() {
        let mut highlighter = Highlighter::new(Language::C).expect("C should be supported");
        let source = "int main() { return 0; }";
        let spans = highlighter.highlight(source);

        assert!(!spans.is_empty(), "C code should produce highlight spans");
    }

    #[test]
    fn test_highlighter_cpp() {
        let mut highlighter = Highlighter::new(Language::Cpp).expect("C++ should be supported");
        let source = "class Foo { public: int bar(); };";
        let spans = highlighter.highlight(source);

        assert!(!spans.is_empty(), "C++ code should produce highlight spans");
    }

    #[test]
    fn test_highlighter_markdown() {
        let mut highlighter =
            Highlighter::new(Language::Markdown).expect("Markdown should be supported");
        let source = "# Hello\n\nThis is **bold** and *italic*.";
        let spans = highlighter.highlight(source);

        // Markdown should produce some spans
        let _ = spans.len();
    }

    #[test]
    fn test_incremental_update() {
        let mut highlighter = Highlighter::new(Language::Rust).expect("Rust should be supported");

        // Initial parse
        let source = "fn main() { }";
        let spans1 = highlighter.highlight(source);

        // Insert text: "fn main() { let x = 1; }"
        let new_source = "fn main() { let x = 1; }";
        let spans2 = highlighter.update(
            new_source, 12, // start_byte: after "{ "
            12, // old_end_byte: same position
            23, // new_end_byte: after "let x = 1; "
        );

        assert!(!spans1.is_empty());
        assert!(!spans2.is_empty());
        // After insertion, we should have more spans
        assert!(spans2.len() >= spans1.len());
    }

    #[test]
    fn test_span_ordering() {
        let first = HighlightSpan::new(0, 5, HighlightType::Keyword);
        let second = HighlightSpan::new(10, 15, HighlightType::String);
        let third = HighlightSpan::new(0, 10, HighlightType::Function);

        let mut spans = [second, first, third];
        spans.sort();

        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, 5);
        assert_eq!(spans[1].start, 0);
        assert_eq!(spans[1].end, 10);
        assert_eq!(spans[2].start, 10);
    }

    #[test]
    fn test_tsx() {
        let mut highlighter = Highlighter::new(Language::Tsx).expect("TSX should be supported");
        let source = "const App = () => <div>Hello</div>;";
        let spans = highlighter.highlight(source);

        assert!(!spans.is_empty(), "TSX code should produce highlight spans");
    }
}
