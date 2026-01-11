//! # Iridium Syntax
//!
//! Syntax highlighting and code intelligence via tree-sitter.
//!
//! This crate provides incremental syntax parsing and highlighting
//! for multiple languages using tree-sitter grammars.
//!
//! ## Supported Languages
//!
//! - Cypher (Neo4j query language)
//! - SQL
//! - Rust
//! - Python
//! - TypeScript
//!
//! ## Example
//!
//! ```ignore
//! use iridium_syntax::{Highlighter, Language};
//!
//! let mut highlighter = Highlighter::new(Language::Rust);
//! let spans = highlighter.highlight("fn main() {}");
//! ```

#![doc(html_root_url = "https://docs.rs/iridium-syntax/0.1.0")]

mod folding;
mod highlight;
pub mod languages;

pub use folding::{FoldKind, FoldRegion};
pub use highlight::{HighlightSpan, HighlightType, Highlighter};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Supported programming languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// Cypher query language (Neo4j)
    Cypher,
    /// SQL
    Sql,
    /// Rust
    Rust,
    /// Python
    Python,
    /// TypeScript
    TypeScript,
}

impl Language {
    /// Returns the language identifier string.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        match self {
            Self::Cypher => "cypher",
            Self::Sql => "sql",
            Self::Rust => "rust",
            Self::Python => "python",
            Self::TypeScript => "typescript",
        }
    }

    /// Parses a language from its identifier string.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        match id.to_lowercase().as_str() {
            "cypher" => Some(Self::Cypher),
            "sql" => Some(Self::Sql),
            "rust" | "rs" => Some(Self::Rust),
            "python" | "py" => Some(Self::Python),
            "typescript" | "ts" => Some(Self::TypeScript),
            _ => None,
        }
    }

    /// Returns all supported languages.
    #[must_use]
    pub fn all() -> &'static [Self] {
        &[Self::Cypher, Self::Sql, Self::Rust, Self::Python, Self::TypeScript]
    }
}

/// Errors that can occur in syntax processing.
#[derive(Debug, Error)]
pub enum SyntaxError {
    /// Language is not supported.
    #[error("Language not supported: {0}")]
    UnsupportedLanguage(String),

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
