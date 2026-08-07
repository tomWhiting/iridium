//! # Iridium Syntax
//!
//! Syntax highlighting and code intelligence via tree-sitter.
//!
//! This crate provides incremental syntax parsing and highlighting
//! for multiple languages using tree-sitter grammars.
//!
//! ## Supported Languages
//!
//! - Rust
//! - Python
//! - TypeScript
//! - JavaScript
//! - TSX
//! - Go
//! - JSON
//! - YAML
//! - Markdown
//! - CSS
//! - Bash
//! - C
//! - C++
//!
//! ## Example
//!
//! ```ignore
//! use iridium_syntax::{Highlighter, Language};
//!
//! let mut highlighter = Highlighter::new(Language::Rust)?;
//! let spans = highlighter.highlight("fn main() {}");
//! ```

#![doc(html_root_url = "https://docs.rs/iridium-syntax/0.1.0")]

mod folding;
mod grammar;
mod highlight;
pub mod navigate;
pub mod query;
mod tree;

pub use folding::{FoldCache, FoldDetector, FoldKind, FoldRegion};
pub use highlight::{HighlightSpan, HighlightType, Highlighter};
pub use query::QueryKind;
pub use tree::{SyntaxTree, byte_point};

// Re-exported so an embedder can describe an edit without taking its own
// dependency on tree-sitter, and so the stub build has the same names to
// mirror.
pub use tree_sitter::{InputEdit, Node, Point, Tree};

use thiserror::Error;

// `Language` lives in its own crate, and is re-exported here so that every
// existing `iridium_syntax::Language` path still resolves.
//
// It was defined here until a second, hand-maintained copy in
// `iridium-editor` — the one that build uses without the `syntax` feature —
// was found to disagree with it on aliases, on case, and on two file
// extensions. The two were feature *alternatives*, so no build had both in
// scope and no test could have compared them. One type is the only guard that
// works; see `iridium_lang` for the full account.
pub use iridium_lang::Language;

/// Errors that can occur in syntax processing.
#[derive(Debug, Error)]
pub enum SyntaxError {
    /// Language is not supported.
    #[error("Unsupported language: {language}")]
    UnsupportedLanguage {
        /// The language identifier
        language: String,
    },

    /// Parse error in source code.
    #[error("Parse error: {message}")]
    ParseError {
        /// Error message
        message: String,
    },

    /// Tree-sitter query error.
    #[error("Query error: {message}")]
    QueryError {
        /// Error message
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_from_extension() {
        assert_eq!(Language::from_extension("rs"), Some(Language::Rust));
        assert_eq!(Language::from_extension("py"), Some(Language::Python));
        assert_eq!(Language::from_extension("ts"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("js"), Some(Language::JavaScript));
        assert_eq!(Language::from_extension("tsx"), Some(Language::Tsx));
        assert_eq!(Language::from_extension("go"), Some(Language::Go));
        assert_eq!(Language::from_extension("json"), Some(Language::Json));
        assert_eq!(Language::from_extension("yaml"), Some(Language::Yaml));
        assert_eq!(Language::from_extension("md"), Some(Language::Markdown));
        assert_eq!(Language::from_extension("css"), Some(Language::Css));
        assert_eq!(Language::from_extension("sh"), Some(Language::Bash));
        assert_eq!(Language::from_extension("c"), Some(Language::C));
        assert_eq!(Language::from_extension("cpp"), Some(Language::Cpp));
    }

    #[test]
    fn indices_agree_with_the_declared_order() {
        for (position, &language) in Language::all().iter().enumerate() {
            assert_eq!(
                language.index(),
                position,
                "{} claims index {} but sits at {position} in all()",
                language.id(),
                language.index()
            );
        }
    }

    #[test]
    fn test_language_id_roundtrip() {
        for lang in Language::all() {
            let id = lang.id();
            let parsed = Language::from_id(id);
            assert_eq!(parsed, Some(*lang), "Failed roundtrip for {id}");
        }
    }
}
