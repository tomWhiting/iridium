//! Native project search. Hosts own buffers and presentation; workers never edit.
mod job;
mod replacement;
mod scan;
mod stamp;
mod walk;
pub use job::Job;
pub use replacement::{Replacement, preview_replacements};
pub use stamp::DiskStamp;
use std::{ops::Range, path::PathBuf, sync::Arc};

/// Maximum retained query bytes.
pub const QUERY_LIMIT: usize = 16 * 1024;
/// Maximum total copied open-buffer bytes per request.
pub const SNAPSHOT_LIMIT: usize = 32 * 1024 * 1024;
/// Maximum bytes in one searched line. Larger files may have many short lines.
pub const LINE_LIMIT: usize = 1024 * 1024;
/// Maximum number of matches retained by a request.
pub const MATCH_LIMIT: usize = 10_000;
/// Search options. Regexes match within a line; default is case-sensitive literal.
#[derive(Clone, Debug, Default)]
pub struct Query {
    /// Literal text or regular expression.
    pub text: String,
    /// Interpret text as a regular expression.
    pub regex: bool,
    /// Unicode case-insensitive search.
    pub ignore_case: bool,
    /// Require Unicode word boundaries around matches.
    pub whole_word: bool,
    /// Ordered include globs; a leading ! excludes. Relative to the root.
    pub globs: Vec<String>,
}
/// A named open buffer supersedes the disk file, even when it has no matches.
#[derive(Clone, Debug)]
pub struct Buffer {
    /// Host-owned document identity.
    pub id: u64,
    /// Revision captured with text.
    pub revision: u64,
    /// Absolute normalized file path.
    pub path: PathBuf,
    /// Current contents, including unsaved edits.
    pub text: Arc<str>,
}
/// Where a result came from. Disk results must be revalidated before editing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Current open-buffer snapshot.
    Buffer {
        /// Host document identity.
        id: u64,
        /// Snapshot revision.
        revision: u64,
    },
    /// Disk contents observed by this scan.
    Disk(DiskStamp),
}
/// A source match; offsets are UTF-8 bytes within the line, excluding its ending.
#[derive(Clone, Debug)]
pub struct Match {
    /// Absolute normalized source path.
    pub path: Arc<PathBuf>,
    /// Buffer identity/revision or disk provenance.
    pub origin: Origin,
    /// Zero-based logical line (LF, CRLF and CR supported).
    pub line: usize,
    /// Zero-based Unicode scalar column, computed once by the worker.
    pub column: usize,
    /// Exact range in `line_text`, including zero-width regex matches.
    pub range: Range<usize>,
    /// Original source line shared by matches on that line.
    pub line_text: Arc<str>,
}
/// Bounded worker messages. Results remain provisional until completion.
#[derive(Debug)]
pub enum Event {
    /// A source match.
    Match(Match),
    /// Remove all results from this path and show why the file was skipped.
    Invalidated {
        /// Source path.
        path: PathBuf,
        /// Actionable diagnostic.
        reason: String,
    },
    /// A traversal error or incomplete scan diagnostic.
    Warning(String),
    /// Completed replacement preview, computed off the UI thread.
    Preview(Result<Vec<Replacement>, String>),
    /// Search ended normally; limits/errors are separate visible messages.
    Finished,
}
#[cfg(test)]
mod tests;

fn compile(query: &Query) -> Result<regex::Regex, String> {
    if query.text.len() > QUERY_LIMIT {
        return Err("project query exceeds 16 KiB".into());
    }
    let pattern = if query.regex {
        query.text.clone()
    } else {
        regex::escape(&query.text)
    };
    let pattern = if query.whole_word {
        format!(r"\b(?:{pattern})\b")
    } else {
        pattern
    };
    regex::RegexBuilder::new(&pattern)
        .case_insensitive(query.ignore_case)
        .size_limit(4 * 1024 * 1024)
        .build()
        .map_err(|e| e.to_string())
}
