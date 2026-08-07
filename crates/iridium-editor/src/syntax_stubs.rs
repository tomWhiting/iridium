//! Stub types for when syntax highlighting is disabled.
//!
//! These types provide API compatibility without requiring tree-sitter.
//!
//! # `Language` is deliberately not here
//!
//! It used to be, as a hand-maintained mirror of `iridium_syntax::Language`
//! — and the two disagreed on aliases, on case, and on two file extensions.
//! Nothing could catch that, because a stub and the type it mirrors are
//! feature *alternatives*: no build has both in scope, so no test can
//! compare them. Language identity is pure data with no parser in it, so it
//! now lives in `iridium-lang` and both builds use the same type.
//!
//! Everything that remains here genuinely needs tree-sitter to exist, which
//! is the test for whether something belongs in this file: if a stub could
//! answer a question *correctly*, it should not be a stub.

use iridium_lang::Language;

/// Stub [`FoldKind`] — fold kinds for non-syntax folding.
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

/// Stub [`FoldRegion`] — represents a foldable region.
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
    pub const fn hidden_line_count(&self) -> usize {
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

/// Stub position, mirroring `tree_sitter::Point`.
///
/// The column is in **bytes** within its row, as tree-sitter's is, so the same
/// arithmetic is valid in both configurations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Point {
    /// Zero-based line.
    pub row: usize,
    /// Zero-based byte offset within the line.
    pub column: usize,
}

/// Stub edit descriptor, mirroring `tree_sitter::InputEdit`.
///
/// It carries the whole description rather than nothing. There is no tree to
/// shift without a parser, but there is still a *document* to shift, and the
/// brace scanner that stands in for tree-sitter fold detection needs to know
/// which lines the edit moved in order to rescan only those. An edit descriptor
/// that carried nothing forced it to rescan everything.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputEdit {
    /// Byte offset where the edit begins, in both documents.
    pub start_byte: usize,
    /// Byte offset where the replaced text ended, in the pre-edit document.
    pub old_end_byte: usize,
    /// Byte offset where the new text ends, in the post-edit document.
    pub new_end_byte: usize,
    /// Position where the edit begins, in both documents.
    pub start_position: Point,
    /// Position where the replaced text ended, in the pre-edit document.
    pub old_end_position: Point,
    /// Position where the new text ends, in the post-edit document.
    pub new_end_position: Point,
}

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
    #[expect(
        clippy::missing_const_for_fn,
        reason = "the real `iridium_syntax` counterpart is not const, and a stub \
    that accepted a const context the real one refuses would let code compile \
    without the `syntax` feature and fail with it"
    )]
    pub fn changed_ranges(&self, _old: &Tree) -> Vec<std::ops::Range<usize>> {
        Vec::new()
    }
}

/// Presents one brace pair as a fold region.
///
/// Fold detection without a grammar is brace matching, and it lives in
/// [`crate::brace_folds`] because it is maintained across edits rather than
/// redone per keystroke. This is the one line of translation between what that
/// scanner finds and what a fold consumer expects: every brace pair is a block,
/// because the scanner has no way to recognise any other kind and inventing one
/// would report a fold shape nothing detected.
pub(crate) const fn fold_region_for(region: &crate::brace_folds::BraceRegion) -> FoldRegion {
    FoldRegion {
        start_line: region.start_line,
        end_line: region.end_line,
        kind: FoldKind::Block,
    }
}

/// Stub [`SyntaxError`] — errors from syntax operations.
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

/// Stub [`HighlightType`] — highlight types for syntax coloring.
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

/// Stub [`HighlightSpan`] — a highlighted region.
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
    #[expect(
        clippy::missing_const_for_fn,
        reason = "the real `iridium_syntax` counterpart is not const, and a stub \
    that accepted a const context the real one refuses would let code compile \
    without the `syntax` feature and fail with it"
    )]
    pub fn new(_language: Language) -> Result<Self, SyntaxError> {
        Err(SyntaxError::UnsupportedLanguage)
    }

    /// Produces highlight spans (always empty without syntax).
    #[must_use]
    #[expect(
        clippy::missing_const_for_fn,
        reason = "the real `iridium_syntax` counterpart is not const, and a stub \
    that accepted a const context the real one refuses would let code compile \
    without the `syntax` feature and fail with it"
    )]
    pub fn spans_in(&self, _tree: &Tree, _source: &str) -> Vec<HighlightSpan> {
        Vec::new()
    }
}
