//! Compiled tree-sitter queries, loaded once per process.
//!
//! Compiling a `.scm` is not cheap — a language's `highlights.scm` runs to
//! thousands of patterns, and tree-sitter compiles each one into a bytecode
//! program. Before this module every [`crate::Highlighter`] compiled its own
//! copy at construction, which put that cost on the path that opens a file and
//! meant N buffers of one language held N identical queries.
//!
//! A query is immutable once compiled and tree-sitter's `Query` is `Send` and
//! `Sync`, so one compiled copy per `(language, kind)` can be shared by every
//! reader in the process. The cache is a flat array of [`OnceLock`]s indexed by
//! language and kind: no lock is taken on the read path once a slot is filled,
//! and a slot is filled at most once even if several threads race for it.
//!
//! The memory is never reclaimed. That is deliberate — a compiled query is
//! small next to a document, and the alternative, reference-counting queries
//! against open buffers, would trade a bounded cost for a lifetime problem on
//! the hot path.
//!
//! # Failure modes, and which of them is an error
//!
//! - The language ships no query of that kind: `Ok(None)`. Routine — see
//!   `iridium_lang::query` for the three that are genuinely absent. Callers
//!   should degrade the feature for that language, not fail.
//! - The query is present but does not compile against the grammar:
//!   `Err(SyntaxError::QueryError)`. That is a build-time defect that escaped —
//!   almost always a grammar bump whose node kinds no longer match a vendored
//!   pattern — and the test module compiles every entry so it cannot escape
//!   twice.

use std::sync::OnceLock;

use tree_sitter::Query;

use crate::{Language, SyntaxError, grammar::grammar};

// The `.scm` text and the kinds it comes in live with the vendored tree, in
// `iridium-lang`, so that the manifests beside them are reachable from builds
// that carry no parser. Re-exported here because compiling a query is what
// this module is for, and a caller should not have to name two crates to do
// it.
pub use iridium_lang::query::{QueryKind, capture_names, source, sources};
pub use textobject::{Direction, TextObject, Variant};

/// Number of languages the cache has a row for.
///
/// Taken from the enum's own count rather than from `all().len()`: the two are
/// tied together by the array type `all()` returns, so a language that exists
/// but was left out of `all()` is a compile error rather than a row this array
/// never allocates and [`compiled`] indexes past.
const LANGUAGE_COUNT: usize = Language::COUNT;

/// Number of query kinds the cache has a column for.
const KIND_COUNT: usize = QueryKind::COUNT;

/// The outcome of compiling one vendored query.
///
/// The failure carries its message rather than a `SyntaxError` because the
/// error is reported to every caller that asks, not just the first, and
/// `SyntaxError` is not `Clone`.
enum Compiled {
    /// The language ships no query of this kind.
    Absent,
    /// The query compiled and is ready to run.
    Ready(Query),
    /// The query is vendored but does not compile against this grammar.
    Broken(String),
}

/// One slot per `(language, kind)`, filled on first use.
static COMPILED: [[OnceLock<Compiled>; KIND_COUNT]; LANGUAGE_COUNT] =
    [const { [const { OnceLock::new() }; KIND_COUNT] }; LANGUAGE_COUNT];

/// Returns the compiled query for a language and kind, compiling it once.
///
/// `Ok(None)` means the language ships no query of that kind, which is a
/// routine absence rather than a failure.
///
/// # Errors
///
/// [`SyntaxError::QueryError`] if the vendored query does not compile against
/// the language's grammar. The message names the file and the offset within it.
pub fn compiled(
    language: Language,
    kind: QueryKind,
) -> Result<Option<&'static Query>, SyntaxError> {
    let slot: &'static OnceLock<Compiled> = &COMPILED[language.index()][kind.index()];

    match slot.get_or_init(|| compile(language, kind)) {
        Compiled::Absent => Ok(None),
        Compiled::Ready(query) => Ok(Some(query)),
        Compiled::Broken(message) => Err(SyntaxError::QueryError {
            message: message.clone(),
        }),
    }
}

/// Compiles one vendored query against its grammar.
fn compile(language: Language, kind: QueryKind) -> Compiled {
    let Some(source) = source(language, kind) else {
        return Compiled::Absent;
    };

    // A language with no linked grammar is a supported language that cannot be
    // parsed — see `grammar`'s module note. There is nothing to compile the
    // query against, so it resolves to the same routine absence as a language
    // that ships no `.scm` of this kind, and callers degrade the feature
    // exactly as they already do. Emphatically not an error: a language reaches
    // here with queries and a manifest and no grammar entirely legitimately.
    let Some(grammar) = grammar(language) else {
        return Compiled::Absent;
    };

    match Query::new(&grammar, source) {
        Ok(query) => Compiled::Ready(query),
        Err(error) => Compiled::Broken(format!(
            "{}/{} does not compile against the {} grammar: {} at offset {}",
            language.id(),
            kind.file_name(),
            language.id(),
            error.message,
            error.offset
        )),
    }
}

pub mod textobject;

#[cfg(test)]
mod tests;
