//! Stub types for when syntax highlighting is disabled.
//!
//! These types provide API compatibility without requiring tree-sitter.

/// Stub Language type - no languages available without syntax feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    /// Placeholder variant to make the enum non-empty
    #[doc(hidden)]
    _Placeholder,
}

impl Language {
    /// Returns all available languages (empty without syntax feature).
    #[must_use]
    pub fn all() -> &'static [Language] {
        &[]
    }

    /// Returns the language ID (always None without syntax feature).
    #[must_use]
    pub fn id(&self) -> &'static str {
        ""
    }

    /// Detects language from file extension (always None without syntax feature).
    #[must_use]
    pub fn from_extension(_ext: &str) -> Option<Language> {
        None
    }

    /// Gets language from ID (always None without syntax feature).
    #[must_use]
    pub fn from_id(_id: &str) -> Option<Language> {
        None
    }
}

/// Stub FoldKind - fold kinds for non-syntax folding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldKind {
    /// Block fold (braces)
    Block,
    /// Comment fold
    Comment,
    /// Import section fold
    Imports,
    /// Region fold (custom markers)
    Region,
}

/// Stub FoldRegion - represents a foldable region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldRegion {
    /// Starting line (0-indexed)
    pub start_line: usize,
    /// Ending line (inclusive, 0-indexed)
    pub end_line: usize,
    /// The kind of fold
    pub kind: FoldKind,
}

impl FoldRegion {
    /// Returns the number of hidden lines when this region is folded.
    #[must_use]
    pub fn hidden_line_count(&self) -> usize {
        self.end_line.saturating_sub(self.start_line)
    }
}

/// Stub FoldDetector - does nothing without syntax feature.
#[derive(Debug)]
pub struct FoldDetector;

impl FoldDetector {
    /// Creates a new fold detector (always None without syntax).
    #[must_use]
    pub fn new(_language: Language) -> Option<Self> {
        None
    }

    /// Detects fold regions (always empty without syntax).
    #[must_use]
    pub fn detect(&mut self, _source: &str) -> Vec<FoldRegion> {
        Vec::new()
    }

    /// Updates fold regions incrementally (always empty without syntax).
    #[must_use]
    pub fn update(
        &mut self,
        _source: &str,
        _start_byte: usize,
        _old_end_byte: usize,
        _new_end_byte: usize,
    ) -> Vec<FoldRegion> {
        Vec::new()
    }
}

/// Stub SyntaxError - errors from syntax operations.
#[derive(Debug, Clone)]
pub enum SyntaxError {
    /// Language not supported
    UnsupportedLanguage,
}

impl std::fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedLanguage => write!(f, "Syntax highlighting not available"),
        }
    }
}

impl std::error::Error for SyntaxError {}

/// Stub HighlightType - highlight types for syntax coloring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightType {
    /// Keyword
    Keyword,
    /// Control keyword
    KeywordControl,
    /// String literal
    String,
    /// String escape
    StringEscape,
    /// Number
    Number,
    /// Boolean
    Boolean,
    /// Comment
    Comment,
    /// Doc comment
    CommentDoc,
    /// Function name
    Function,
    /// Function definition
    FunctionDefinition,
    /// Method
    FunctionMethod,
    /// Special function
    FunctionSpecial,
    /// Variable
    Variable,
    /// Parameter
    VariableParameter,
    /// Special variable
    VariableSpecial,
    /// Type name
    Type,
    /// Builtin type
    TypeBuiltin,
    /// Interface type
    TypeInterface,
    /// Operator
    Operator,
    /// Bracket
    PunctuationBracket,
    /// Delimiter
    PunctuationDelimiter,
    /// Special punctuation
    PunctuationSpecial,
    /// Property
    Property,
    /// Constant
    Constant,
    /// Lifetime (Rust)
    Lifetime,
    /// Attribute
    Attribute,
    /// Tag (HTML/XML)
    Tag,
    /// Embedded content
    Embedded,
    /// Error
    Error,
}

/// Stub HighlightSpan - a highlighted region.
#[derive(Debug, Clone)]
pub struct HighlightSpan {
    /// Start byte offset
    pub start: usize,
    /// End byte offset
    pub end: usize,
    /// Highlight type
    pub highlight: HighlightType,
}

/// Stub Highlighter - does nothing without syntax feature.
#[derive(Debug)]
pub struct Highlighter;

impl Highlighter {
    /// Creates a new highlighter (no-op without syntax).
    pub fn new(_language: Language) -> Result<Self, SyntaxError> {
        Err(SyntaxError::UnsupportedLanguage)
    }

    /// Highlights source code (always empty without syntax).
    pub fn highlight(&mut self, _source: &str) -> Vec<HighlightSpan> {
        Vec::new()
    }

    /// Updates highlighting incrementally (no-op without syntax).
    pub fn update(
        &mut self,
        _source: &str,
        _start_byte: usize,
        _old_end_byte: usize,
        _new_end_byte: usize,
    ) -> Vec<HighlightSpan> {
        Vec::new()
    }
}
