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

/// Simple brace-based fold detector for when syntax feature is disabled.
///
/// This provides basic code folding by matching braces `{}` without
/// requiring tree-sitter. It handles:
/// - Nested braces
/// - Skipping braces inside string literals (basic heuristic)
/// - Multi-line blocks only (single-line braces are not foldable)
#[derive(Debug)]
pub struct FoldDetector {
    /// Cached fold regions
    regions: Vec<FoldRegion>,
}

impl FoldDetector {
    /// Creates a new brace-based fold detector.
    #[must_use]
    pub fn new(_language: Language) -> Option<Self> {
        Some(Self {
            regions: Vec::new(),
        })
    }

    /// Detects fold regions based on brace matching.
    #[must_use]
    pub fn detect(&mut self, source: &str) -> Vec<FoldRegion> {
        self.regions = Self::detect_brace_folds(source);
        self.regions.clone()
    }

    /// Updates fold regions (re-detects for simplicity).
    #[must_use]
    pub fn update(
        &mut self,
        source: &str,
        _start_byte: usize,
        _old_end_byte: usize,
        _new_end_byte: usize,
    ) -> Vec<FoldRegion> {
        self.detect(source)
    }

    /// Detects foldable regions by matching braces.
    fn detect_brace_folds(source: &str) -> Vec<FoldRegion> {
        let mut regions = Vec::new();
        let mut brace_stack: Vec<(usize, usize)> = Vec::new(); // (line, char_index)
        let mut in_string = false;
        let mut string_char = '"';
        let mut in_line_comment = false;
        let mut in_block_comment = false;
        let mut prev_char = '\0';

        for (line_num, line) in source.lines().enumerate() {
            in_line_comment = false; // Reset at start of each line

            let chars: Vec<char> = line.chars().collect();
            let mut i = 0;

            while i < chars.len() {
                let ch = chars[i];
                let next_char = chars.get(i + 1).copied().unwrap_or('\0');

                // Handle block comment start
                if !in_string
                    && !in_line_comment
                    && !in_block_comment
                    && ch == '/'
                    && next_char == '*'
                {
                    in_block_comment = true;
                    i += 2;
                    continue;
                }

                // Handle block comment end
                if in_block_comment && ch == '*' && next_char == '/' {
                    in_block_comment = false;
                    i += 2;
                    continue;
                }

                // Skip if in block comment
                if in_block_comment {
                    i += 1;
                    continue;
                }

                // Handle line comment start
                if !in_string && ch == '/' && next_char == '/' {
                    in_line_comment = true;
                    break; // Rest of line is comment
                }

                // Handle string literals (basic - doesn't handle all escape sequences)
                if !in_line_comment && (ch == '"' || ch == '\'') {
                    if !in_string {
                        in_string = true;
                        string_char = ch;
                    } else if ch == string_char && prev_char != '\\' {
                        in_string = false;
                    }
                    prev_char = ch;
                    i += 1;
                    continue;
                }

                // Skip if in string
                if in_string {
                    prev_char = ch;
                    i += 1;
                    continue;
                }

                // Handle opening brace
                if ch == '{' {
                    brace_stack.push((line_num, i));
                }

                // Handle closing brace
                if ch == '}' {
                    if let Some((start_line, _)) = brace_stack.pop() {
                        // Only create fold region if it spans multiple lines
                        if line_num > start_line {
                            regions.push(FoldRegion {
                                start_line,
                                end_line: line_num,
                                kind: FoldKind::Block,
                            });
                        }
                    }
                }

                prev_char = ch;
                i += 1;
            }
        }

        // Sort by start line for consistent ordering
        regions.sort_by_key(|r| r.start_line);
        regions
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
