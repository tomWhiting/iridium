//! Stub types for when syntax highlighting is disabled.
//!
//! These types provide API compatibility without requiring tree-sitter.

/// Language identity, available without the `syntax` feature.
///
/// Mirrors the variants of `iridium_syntax::Language`. Only *parsing* needs
/// tree-sitter; language **identity** is pure data, and the kernel depends on
/// it for behaviour that has nothing to do with highlighting — comment
/// toggling, indent rules, and auto-pairs all key off the active language. A
/// stub that reported no languages would silently change those behaviours
/// between feature configurations, which is worse than having no stub at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    /// Rust
    Rust,
    /// Python
    Python,
    /// TypeScript
    TypeScript,
    /// JavaScript
    JavaScript,
    /// TSX
    Tsx,
    /// Go
    Go,
    /// JSON
    Json,
    /// YAML
    Yaml,
    /// Markdown
    Markdown,
    /// CSS
    Css,
    /// Bash
    Bash,
    /// C
    C,
    /// C++
    Cpp,
}

impl Language {
    /// Every language identity the kernel understands.
    #[must_use]
    pub fn all() -> &'static [Language] {
        &[
            Self::Rust,
            Self::Python,
            Self::TypeScript,
            Self::JavaScript,
            Self::Tsx,
            Self::Go,
            Self::Json,
            Self::Yaml,
            Self::Markdown,
            Self::Css,
            Self::Bash,
            Self::C,
            Self::Cpp,
        ]
    }

    /// Returns the stable language ID.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::TypeScript => "typescript",
            Self::JavaScript => "javascript",
            Self::Tsx => "tsx",
            Self::Go => "go",
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Markdown => "markdown",
            Self::Css => "css",
            Self::Bash => "bash",
            Self::C => "c",
            Self::Cpp => "cpp",
        }
    }

    /// Detects a language from a file extension.
    #[must_use]
    pub fn from_extension(ext: &str) -> Option<Language> {
        match ext.to_ascii_lowercase().as_str() {
            "rs" => Some(Self::Rust),
            "py" | "pyi" => Some(Self::Python),
            "ts" | "mts" | "cts" => Some(Self::TypeScript),
            "js" | "mjs" | "cjs" | "jsx" => Some(Self::JavaScript),
            "tsx" => Some(Self::Tsx),
            "go" => Some(Self::Go),
            "json" | "jsonc" => Some(Self::Json),
            "yaml" | "yml" => Some(Self::Yaml),
            "md" | "markdown" => Some(Self::Markdown),
            "css" => Some(Self::Css),
            "sh" | "bash" | "zsh" => Some(Self::Bash),
            "c" | "h" => Some(Self::C),
            "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => Some(Self::Cpp),
            _ => None,
        }
    }

    /// Resolves a language from its stable ID.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Language> {
        Self::all().iter().copied().find(|lang| lang.id() == id)
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

/// Stub parse tree — there is no parser without the `syntax` feature.
///
/// It exists so that callers have one shape to write against: with the feature
/// on they hand a real tree to a real detector, with it off they hand this to
/// the brace scanner, and no call site needs a `cfg`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tree;

/// Stub edit descriptor, mirroring `tree_sitter::InputEdit`.
///
/// Carries nothing: with no parser there is no tree to shift.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputEdit;

/// Stub retained tree, mirroring `iridium_syntax::SyntaxTree`.
///
/// Every method is a no-op that keeps the caller's control flow intact: parsing
/// "succeeds" and yields the unit tree the stub detector ignores.
#[derive(Debug, Default)]
pub struct SyntaxTree {
    tree: Tree,
}

impl SyntaxTree {
    /// Creates a stub tree. Never fails.
    ///
    /// # Errors
    ///
    /// Never. The signature matches the real one so call sites are identical.
    pub const fn new(_language: Language) -> Result<Self, SyntaxError> {
        Ok(Self { tree: Tree })
    }

    /// Returns the stub tree, so a caller's `else` branch is not taken.
    pub const fn parse(&mut self, _source: &str) -> Option<&Tree> {
        Some(&self.tree)
    }

    /// Returns the stub tree, so a caller's `else` branch is not taken.
    pub const fn edit_bytes(
        &mut self,
        _source: &str,
        _start_byte: usize,
        _old_end_byte: usize,
        _new_end_byte: usize,
    ) -> Option<&Tree> {
        Some(&self.tree)
    }

    /// Returns the stub tree.
    pub const fn tree(&self) -> Option<&Tree> {
        Some(&self.tree)
    }

    /// Records an edit against nothing.
    pub const fn edit(&mut self, _edit: &InputEdit) {}

    /// Returns the stub tree, so a caller's `else` branch is not taken.
    pub const fn reparse(&mut self, _source: &str) -> Option<&Tree> {
        Some(&self.tree)
    }

    /// Reports that nothing changed, because nothing was ever parsed.
    ///
    /// Empty is the honest answer here and also the safe one: a consumer that
    /// treats it as "no structural change" is right, and the fold detector that
    /// stands in without the `syntax` feature does not read it at all.
    #[must_use]
    pub fn changed_ranges(&self, _old: &Tree) -> Vec<std::ops::Range<usize>> {
        Vec::new()
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
pub struct FoldDetector;

impl FoldDetector {
    /// Creates a new brace-based fold detector.
    #[must_use]
    pub const fn new(_language: Language) -> Self {
        Self
    }

    /// Detects fold regions by matching braces, ignoring the stub tree.
    #[must_use]
    pub fn regions_in(&self, _tree: &Tree, source: &str) -> Vec<FoldRegion> {
        Self::detect_brace_folds(source)
    }

    /// Detects foldable regions by matching braces.
    fn detect_brace_folds(source: &str) -> Vec<FoldRegion> {
        let mut regions = Vec::new();
        let mut brace_stack: Vec<(usize, usize)> = Vec::new(); // (line, char_index)
        let mut in_string = false;
        let mut string_char = '"';
        let mut in_block_comment = false;
        let mut prev_char = '\0';

        // A line comment always runs to the end of its line, so the scan
        // breaks out rather than tracking a flag that could never be read
        // again before being reset.
        for (line_num, line) in source.lines().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            let mut i = 0;

            while i < chars.len() {
                let ch = chars[i];
                let next_char = chars.get(i + 1).copied().unwrap_or('\0');

                // Handle block comment start
                if !in_string && !in_block_comment && ch == '/' && next_char == '*' {
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
                    break; // Rest of line is comment
                }

                // Handle string literals (basic - doesn't handle all escape sequences)
                if ch == '"' || ch == '\'' {
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

    /// Produces highlight spans (always empty without syntax).
    #[must_use]
    pub fn spans_in(&self, _tree: &Tree, _source: &str) -> Vec<HighlightSpan> {
        Vec::new()
    }
}
