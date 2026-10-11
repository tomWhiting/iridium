//! Evaluate jq path filters against immutable JSON source snapshots.
//!
//! Values identify source paths, never matching text. Callers must execute
//! untrusted or expensive filters in a cancellable worker with a deadline;
//! output limits alone cannot interrupt a filter that never yields.
mod evaluate;
mod source;

use serde::{Deserialize, Serialize};

/// Maximum source snapshot accepted by the initial interactive query service.
pub const MAX_SOURCE_BYTES: usize = 32 * 1024 * 1024;
/// Maximum filter size accepted before compilation.
pub const MAX_FILTER_BYTES: usize = 16 * 1024;
/// Maximum produced matches, before deduplication or overlap merging.
pub const MAX_MATCHES: usize = 10_000;
/// Maximum JSON nodes indexed in one query.
pub const MAX_NODES: usize = 500_000;

/// How source text is divided into jq inputs.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Format {
    /// A single JSON value, or line-delimited values if whole-document parsing fails.
    #[default]
    Auto,
    /// Exactly one JSON value.
    Json,
    /// One JSON value per nonempty physical line; blank records are errors.
    JsonLines,
}

/// Which source token is selected for each value addressed by a filter.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Target {
    /// The entire value token, including quotes on a string.
    #[default]
    Values,
    /// The object's key token corresponding to a matched member value.
    Keys,
}

/// A query independent of any editor or terminal state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    /// A jq expression that addresses existing source values.
    pub filter: String,
    /// JSON or JSONL input policy.
    pub format: Format,
    /// Values or member keys.
    pub target: Target,
}

/// A validated byte range into the unchanged source snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SourceRange {
    /// Inclusive UTF-8 byte offset.
    pub start: usize,
    /// Exclusive UTF-8 byte offset.
    pub end: usize,
}

/// Query failure is explicit and never carries a partial selectable result.
pub type QueryResult = Result<Vec<SourceRange>, String>;

/// Evaluate a jq filter and resolve every returned path to its exact source token.
///
/// # Errors
/// Rejects malformed input/query, duplicate keys, computed non-path results,
/// missing source addresses, invalid target types and exceeded size/count limits.
/// The caller must impose an execution deadline, including compilation and parsing.
pub fn select(source: &str, query: &Query) -> QueryResult {
    if source.len() > MAX_SOURCE_BYTES {
        return Err("JSON query source exceeds 32 MiB".into());
    }
    if query.filter.len() > MAX_FILTER_BYTES {
        return Err("jq filter exceeds 16 KiB".into());
    }
    if query.filter.trim().is_empty() {
        return Err("Enter a jq path filter".into());
    }
    evaluate::select(source, query)
}

#[cfg(test)]
mod tests;
