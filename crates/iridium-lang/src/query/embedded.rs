//! The vendored query sources, compiled into the binary.
//!
//! Iridium ships its grammars statically and must ship their queries the same
//! way: a face running in a browser, a terminal or an editor pane has no
//! guaranteed filesystem, and a query read at runtime is a query that can go
//! missing after install. Every `.scm` below is therefore an `include_str!`.
//!
//! The match is exhaustive over both [`Language`] and [`QueryKind`] — no
//! wildcard arm. That is the point of naming all seventy-eight pairings by
//! hand: adding a language or a kind fails to compile until someone has
//! decided, file by file, what it ships. A wildcard would turn that decision
//! into a silent `None`, and a silently missing `textobjects.scm` is a language
//! whose structural selection does nothing at all.
//!
//! Three pairings resolve to `None`, and they are real absences in the vendored
//! files rather than oversights: YAML ships no `indents.scm`, JSON no
//! `injections.scm`, and Bash no `outline.scm`. The test module cross-checks
//! every one of the seventy-eight against the directory on disk, so a file that
//! appears or disappears in a future vendor refresh fails the build rather than
//! going unnoticed.
//!
//! Not every query directory under `languages/queries/` is reachable from here:
//! `diff`, `gitcommit`, `gomod`, `gowork`, `jsdoc`, `jsonc`, `markdown-inline`
//! and `regex` carry query files but have no registered grammar, so there is no
//! [`Language`] to ask for them. They are vendored, unused, and deliberately
//! left out.

use crate::Language;

use super::QueryKind;

/// Includes one vendored query file.
macro_rules! query {
    ($language:literal, $kind:literal) => {
        Some(include_str!(concat!(
            "../languages/queries/",
            $language,
            "/",
            $kind
        )))
    };
}

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
#[must_use]
pub const fn source(language: Language, kind: QueryKind) -> Option<&'static str> {
    match (language, kind) {
        // The three pairings with no vendored file. Grouped rather than left
        // in their language's block so the whole set of absences is legible at
        // once — the test module holds the same list and asserts they match.
        (Language::Json, QueryKind::Injections)
        | (Language::Yaml, QueryKind::Indents)
        | (Language::Bash, QueryKind::Outline) => None,

        (Language::Rust, QueryKind::Highlights) => query!("rust", "highlights.scm"),
        (Language::Rust, QueryKind::Brackets) => query!("rust", "brackets.scm"),
        (Language::Rust, QueryKind::TextObjects) => query!("rust", "textobjects.scm"),
        (Language::Rust, QueryKind::Indents) => query!("rust", "indents.scm"),
        (Language::Rust, QueryKind::Injections) => query!("rust", "injections.scm"),
        (Language::Rust, QueryKind::Outline) => query!("rust", "outline.scm"),

        (Language::Python, QueryKind::Highlights) => query!("python", "highlights.scm"),
        (Language::Python, QueryKind::Brackets) => query!("python", "brackets.scm"),
        (Language::Python, QueryKind::TextObjects) => query!("python", "textobjects.scm"),
        (Language::Python, QueryKind::Indents) => query!("python", "indents.scm"),
        (Language::Python, QueryKind::Injections) => query!("python", "injections.scm"),
        (Language::Python, QueryKind::Outline) => query!("python", "outline.scm"),

        (Language::TypeScript, QueryKind::Highlights) => query!("typescript", "highlights.scm"),
        (Language::TypeScript, QueryKind::Brackets) => query!("typescript", "brackets.scm"),
        (Language::TypeScript, QueryKind::TextObjects) => query!("typescript", "textobjects.scm"),
        (Language::TypeScript, QueryKind::Indents) => query!("typescript", "indents.scm"),
        (Language::TypeScript, QueryKind::Injections) => query!("typescript", "injections.scm"),
        (Language::TypeScript, QueryKind::Outline) => query!("typescript", "outline.scm"),

        (Language::JavaScript, QueryKind::Highlights) => query!("javascript", "highlights.scm"),
        (Language::JavaScript, QueryKind::Brackets) => query!("javascript", "brackets.scm"),
        (Language::JavaScript, QueryKind::TextObjects) => query!("javascript", "textobjects.scm"),
        (Language::JavaScript, QueryKind::Indents) => query!("javascript", "indents.scm"),
        (Language::JavaScript, QueryKind::Injections) => query!("javascript", "injections.scm"),
        (Language::JavaScript, QueryKind::Outline) => query!("javascript", "outline.scm"),

        (Language::Tsx, QueryKind::Highlights) => query!("tsx", "highlights.scm"),
        (Language::Tsx, QueryKind::Brackets) => query!("tsx", "brackets.scm"),
        (Language::Tsx, QueryKind::TextObjects) => query!("tsx", "textobjects.scm"),
        (Language::Tsx, QueryKind::Indents) => query!("tsx", "indents.scm"),
        (Language::Tsx, QueryKind::Injections) => query!("tsx", "injections.scm"),
        (Language::Tsx, QueryKind::Outline) => query!("tsx", "outline.scm"),

        (Language::Go, QueryKind::Highlights) => query!("go", "highlights.scm"),
        (Language::Go, QueryKind::Brackets) => query!("go", "brackets.scm"),
        (Language::Go, QueryKind::TextObjects) => query!("go", "textobjects.scm"),
        (Language::Go, QueryKind::Indents) => query!("go", "indents.scm"),
        (Language::Go, QueryKind::Injections) => query!("go", "injections.scm"),
        (Language::Go, QueryKind::Outline) => query!("go", "outline.scm"),

        (Language::Json, QueryKind::Highlights) => query!("json", "highlights.scm"),
        (Language::Json, QueryKind::Brackets) => query!("json", "brackets.scm"),
        (Language::Json, QueryKind::TextObjects) => query!("json", "textobjects.scm"),
        (Language::Json, QueryKind::Indents) => query!("json", "indents.scm"),
        (Language::Json, QueryKind::Outline) => query!("json", "outline.scm"),

        (Language::Yaml, QueryKind::Highlights) => query!("yaml", "highlights.scm"),
        (Language::Yaml, QueryKind::Brackets) => query!("yaml", "brackets.scm"),
        (Language::Yaml, QueryKind::TextObjects) => query!("yaml", "textobjects.scm"),
        (Language::Yaml, QueryKind::Injections) => query!("yaml", "injections.scm"),
        (Language::Yaml, QueryKind::Outline) => query!("yaml", "outline.scm"),

        (Language::Markdown, QueryKind::Highlights) => query!("markdown", "highlights.scm"),
        (Language::Markdown, QueryKind::Brackets) => query!("markdown", "brackets.scm"),
        (Language::Markdown, QueryKind::TextObjects) => query!("markdown", "textobjects.scm"),
        (Language::Markdown, QueryKind::Indents) => query!("markdown", "indents.scm"),
        (Language::Markdown, QueryKind::Injections) => query!("markdown", "injections.scm"),
        (Language::Markdown, QueryKind::Outline) => query!("markdown", "outline.scm"),

        (Language::Css, QueryKind::Highlights) => query!("css", "highlights.scm"),
        (Language::Css, QueryKind::Brackets) => query!("css", "brackets.scm"),
        (Language::Css, QueryKind::TextObjects) => query!("css", "textobjects.scm"),
        (Language::Css, QueryKind::Indents) => query!("css", "indents.scm"),
        (Language::Css, QueryKind::Injections) => query!("css", "injections.scm"),
        (Language::Css, QueryKind::Outline) => query!("css", "outline.scm"),

        (Language::Bash, QueryKind::Highlights) => query!("bash", "highlights.scm"),
        (Language::Bash, QueryKind::Brackets) => query!("bash", "brackets.scm"),
        (Language::Bash, QueryKind::TextObjects) => query!("bash", "textobjects.scm"),
        (Language::Bash, QueryKind::Indents) => query!("bash", "indents.scm"),
        (Language::Bash, QueryKind::Injections) => query!("bash", "injections.scm"),

        (Language::C, QueryKind::Highlights) => query!("c", "highlights.scm"),
        (Language::C, QueryKind::Brackets) => query!("c", "brackets.scm"),
        (Language::C, QueryKind::TextObjects) => query!("c", "textobjects.scm"),
        (Language::C, QueryKind::Indents) => query!("c", "indents.scm"),
        (Language::C, QueryKind::Injections) => query!("c", "injections.scm"),
        (Language::C, QueryKind::Outline) => query!("c", "outline.scm"),

        (Language::Cpp, QueryKind::Highlights) => query!("cpp", "highlights.scm"),
        (Language::Cpp, QueryKind::Brackets) => query!("cpp", "brackets.scm"),
        (Language::Cpp, QueryKind::TextObjects) => query!("cpp", "textobjects.scm"),
        (Language::Cpp, QueryKind::Indents) => query!("cpp", "indents.scm"),
        (Language::Cpp, QueryKind::Injections) => query!("cpp", "injections.scm"),
        (Language::Cpp, QueryKind::Outline) => query!("cpp", "outline.scm"),
    }
}
