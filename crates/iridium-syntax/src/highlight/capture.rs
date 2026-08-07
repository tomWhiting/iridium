//! Capture names in, highlight categories out.
//!
//! The vendored Zed queries name captures far more finely than any theme
//! colours them — `@keyword.control`, `@function.method`, `@type.builtin`
//! — so this is where that vocabulary collapses onto the fixed set of
//! categories a `SyntaxColors` can paint.

use serde::{Deserialize, Serialize};

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
    /// Punctuation brackets — `{`, `}`, `[`, `]`, `(`, `)`.
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
            // `namespace` and `module` ride along with the type arm rather
            // than taking a variant of their own. `SyntaxColors` is a fixed
            // struct of concrete colours, so a new variant means a new field in
            // every theme and in the TUI palette — disproportionate for a token
            // that reads as a type-like name everywhere it appears. `Lifetime`
            // already sets this precedent in `highlight_to_color`.
            //
            // They mapped to nothing until AWL arrived, which meant AWL, C++,
            // CSS and Go all rendered these tokens in the plain foreground with
            // nothing reporting it.
            "type" | "type.name" | "type.definition" | "constructor" | "namespace" | "module" => {
                Some(Self::Type)
            },
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
            //
            // `tag.jsx` is listed rather than reached by a `tag` prefix arm,
            // and deliberately so. Every other family here has a prefix arm,
            // but a prefix would also swallow names that are not tags: several
            // grammars capture `@tag.attribute` for an attribute *name*, which
            // would silently take the tag colour and never surface in
            // `every_vendored_capture_maps_to_a_highlight_type` — the test
            // that found this gap in the first place. Listing costs one line
            // per vendor refresh and keeps the ratchet.
            "tag" | "tag.name" | "tag.jsx" => Some(Self::Tag),

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
        if name.starts_with("namespace") || name.starts_with("module") {
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
