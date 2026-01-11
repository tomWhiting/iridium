//! # Iridium Syntax
//!
//! Syntax highlighting and code intelligence via tree-sitter.
//!
//! This crate provides incremental syntax parsing and highlighting
//! for multiple languages using tree-sitter grammars.
//!
//! ## Supported Languages
//!
//! **With full tree-sitter grammar support:**
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
//! **Query files bundled (grammar pending):**
//! - Cypher (Neo4j query language) - requires tree-sitter-cypher git dependency
//! - SQL - tree-sitter-sql uses incompatible tree-sitter version
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
    /// JavaScript
    JavaScript,
    /// TSX (TypeScript + JSX)
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
    /// Bash/Shell
    Bash,
    /// C
    C,
    /// C++
    Cpp,
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

    /// Parses a language from its identifier string.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        match id.to_lowercase().as_str() {
            "cypher" | "cql" => Some(Self::Cypher),
            "sql" => Some(Self::Sql),
            "rust" | "rs" => Some(Self::Rust),
            "python" | "py" => Some(Self::Python),
            "typescript" | "ts" => Some(Self::TypeScript),
            "javascript" | "js" => Some(Self::JavaScript),
            "tsx" => Some(Self::Tsx),
            "go" | "golang" => Some(Self::Go),
            "json" => Some(Self::Json),
            "yaml" | "yml" => Some(Self::Yaml),
            "markdown" | "md" => Some(Self::Markdown),
            "css" => Some(Self::Css),
            "bash" | "sh" | "shell" | "zsh" => Some(Self::Bash),
            "c" => Some(Self::C),
            "cpp" | "c++" | "cxx" | "cc" => Some(Self::Cpp),
            _ => None,
        }
    }

    /// Detects language from a file extension.
    #[must_use]
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "rs" => Some(Self::Rust),
            "py" | "pyi" | "pyw" => Some(Self::Python),
            "ts" | "mts" | "cts" => Some(Self::TypeScript),
            "js" | "mjs" | "cjs" | "jsx" => Some(Self::JavaScript), // JSX uses JS grammar
            "tsx" => Some(Self::Tsx),
            "go" => Some(Self::Go),
            "json" => Some(Self::Json),
            "yaml" | "yml" => Some(Self::Yaml),
            "md" | "markdown" => Some(Self::Markdown),
            "css" => Some(Self::Css),
            "sh" | "bash" | "zsh" => Some(Self::Bash),
            "c" | "h" => Some(Self::C),
            "cpp" | "cxx" | "cc" | "hpp" | "hxx" | "hh" => Some(Self::Cpp),
            "sql" => Some(Self::Sql),
            "cypher" | "cql" => Some(Self::Cypher),
            _ => None,
        }
    }

    /// Returns all supported languages.
    #[must_use]
    pub const fn all() -> &'static [Self] {
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
            Self::Cypher,
            Self::Sql,
        ]
    }

    /// Returns languages that have full tree-sitter grammar support.
    ///
    /// These languages can be used with the `Highlighter` without errors.
    #[must_use]
    pub const fn supported() -> &'static [Self] {
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

    /// Returns true if this language has full tree-sitter grammar support.
    #[must_use]
    pub const fn is_supported(&self) -> bool {
        !matches!(self, Self::Cypher | Self::Sql)
    }
}

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
    fn test_language_id_roundtrip() {
        for lang in Language::all() {
            let id = lang.id();
            let parsed = Language::from_id(id);
            assert_eq!(parsed, Some(*lang), "Failed roundtrip for {id}");
        }
    }

    #[test]
    fn test_language_is_supported() {
        assert!(Language::Rust.is_supported());
        assert!(Language::Python.is_supported());
        assert!(!Language::Cypher.is_supported());
        assert!(!Language::Sql.is_supported());
    }
}
