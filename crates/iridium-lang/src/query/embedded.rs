//! The vendored query sources, compiled into the binary.
//!
//! Iridium ships its grammars statically and must ship their queries the same
//! way: a face running in a browser, a terminal or an editor pane has no
//! guaranteed filesystem, and a query read at runtime is a query that can go
//! missing after install. Every `.scm` below is therefore an `include_str!` —
//! emitted by `build.rs`, which scans the vendored tree.
//!
//! # Why this table is generated, when the manifests beside it are hand-listed
//!
//! This module used to be an exhaustive match over `(Language, QueryKind)` with
//! no wildcard arm: seventy-eight pairings named by hand, so that adding a
//! language failed to compile until someone decided, file by file, what it
//! shipped. That was the right shape while the set of languages was fixed, and
//! it is exactly the wrong one now — it made adding a language mean editing
//! Rust, which is what the registry work exists to remove.
//!
//! The *set of languages* is still hand-listed, over in `manifest/embedded.rs`,
//! and for the reason stated there: a language must not appear or vanish
//! because of what happens to be on disk. Only the per-language file list is
//! scanned, because that half is a fact about the directory and cannot be
//! written as a macro — `include_str!` is a compile error on a missing file, so
//! "does this language ship `brackets.scm`?" is unaskable from inside Rust.
//!
//! # What was given up, and what replaces it
//!
//! A missing file is now a silent absence rather than a compile error. The
//! replacement is not another derived check — the test module compares this
//! table against the directory, and once both sides read the same directory
//! that comparison can only catch a mangled table, not a missing file.
//!
//! What actually holds the line is `KNOWN_ABSENCES` in the test module: a
//! hand-written list of the pairings that are legitimately absent, which fails
//! when the set changes in *either* direction. A file that disappears from a
//! vendor refresh turns into a failing test naming it, which is the diff a
//! reviewer needs.
//!
//! # Lookup is by file name, not by position
//!
//! The generated rows carry `("highlights.scm", <text>)`. Nothing here depends
//! on an agreed ordering of query kinds, so `build.rs` does not hold a second
//! copy of that order — the single shared fact is the file name, which
//! [`QueryKind::file_name`] owns and the scan reads off the directory.
//!
//! A language may therefore ship a `.scm` Iridium has no [`QueryKind`] for. It
//! is carried here and never asked for, which is the correct handling rather
//! than an oversight: a scan that rejected unknown files would fail the build
//! over a file nothing reads.
//!
//! # Languages with no grammar are still in this table
//!
//! `diff`, `gitcommit`, `gomod`, `gowork`, `jsdoc`, `jsonc`, `markdown-inline`
//! and `regex` are vendored and carry queries. Whether a [`Language`] exists to
//! ask for them is the registry's business, not this module's; serving their
//! text costs nothing and pretending they are absent would be a lie about the
//! tree.

use crate::Language;

use super::QueryKind;

include!(concat!(env!("OUT_DIR"), "/query_sources.rs"));

/// Returns the source text of a vendored query, if the language ships one.
///
/// This is the uncompiled `.scm`. Reach for it when the text itself is what is
/// wanted — a diagnostic, a test, a face that compiles queries with its own
/// tree-sitter build. Anything that *runs* a query should go through
/// `iridium_syntax::query::compiled` instead, so the compilation is paid for
/// once per process rather than once per reader.
///
/// `None` means the language genuinely has no query of that kind — it is not
/// an error, and callers should treat the corresponding feature as unavailable
/// for that language rather than failing.
///
/// Two linear scans over tables of at most a couple of dozen entries each. This
/// is called on a compiled-query cache miss, which happens once per process per
/// `(language, kind)` — never per keystroke and never per frame.
#[must_use]
pub fn source(language: Language, kind: QueryKind) -> Option<&'static str> {
    let files = QUERY_SOURCES
        .iter()
        .find(|(id, _)| *id == language.id())
        .map(|(_, files)| *files)?;

    files
        .iter()
        .find(|(name, _)| *name == kind.file_name())
        .map(|(_, text)| *text)
}
