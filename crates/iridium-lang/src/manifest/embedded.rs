//! The vendored manifests, compiled in and parsed once.
//!
//! Every `config.toml` under `languages/queries/` is here, including the eight
//! languages Iridium has no grammar for and the three that exist only to be
//! injected. Filtering is the caller's business: a language with no grammar is
//! still a language whose file associations and comment syntax are known, and
//! the registry step needs the whole set.
//!
//! # Why parsing is deferred and why it cannot fail loudly
//!
//! The files are `include_str!`'d, so they cannot go missing after install —
//! the same reasoning as the queries beside them. Parsing happens on first use
//! rather than at startup, because a face that never asks about a language
//! should not pay for twenty-two TOML documents.
//!
//! A malformed manifest is a build-time defect in a vendored in-tree file, not
//! a runtime condition, and this crate has no business panicking in an editor.
//! So a manifest that fails to parse is dropped from [`all`] and its error is
//! recorded in [`parse_failures`], which the test module asserts is empty. That
//! turns "someone broke a manifest" into a failing test with the TOML error in
//! it, rather than either a panic or a language that silently stops working.

use std::sync::LazyLock;

use super::Manifest;

/// Every vendored manifest, as `(directory, file contents)`.
///
/// Named one by one rather than globbed: a build script that walked the
/// directory would make the set of languages depend on what happens to be on
/// disk, and a vendor refresh that dropped a directory would then remove a
/// language without any diff saying so.
const SOURCES: &[(&str, &str)] = &[
    ("awl", include_str!("../languages/queries/awl/config.toml")),
    (
        "bash",
        include_str!("../languages/queries/bash/config.toml"),
    ),
    ("c", include_str!("../languages/queries/c/config.toml")),
    ("cpp", include_str!("../languages/queries/cpp/config.toml")),
    ("css", include_str!("../languages/queries/css/config.toml")),
    (
        "diff",
        include_str!("../languages/queries/diff/config.toml"),
    ),
    (
        "gitcommit",
        include_str!("../languages/queries/gitcommit/config.toml"),
    ),
    ("go", include_str!("../languages/queries/go/config.toml")),
    (
        "gomod",
        include_str!("../languages/queries/gomod/config.toml"),
    ),
    (
        "gowork",
        include_str!("../languages/queries/gowork/config.toml"),
    ),
    (
        "javascript",
        include_str!("../languages/queries/javascript/config.toml"),
    ),
    (
        "jsdoc",
        include_str!("../languages/queries/jsdoc/config.toml"),
    ),
    (
        "json",
        include_str!("../languages/queries/json/config.toml"),
    ),
    (
        "jsonc",
        include_str!("../languages/queries/jsonc/config.toml"),
    ),
    (
        "markdown",
        include_str!("../languages/queries/markdown/config.toml"),
    ),
    (
        "markdown-inline",
        include_str!("../languages/queries/markdown-inline/config.toml"),
    ),
    (
        "python",
        include_str!("../languages/queries/python/config.toml"),
    ),
    (
        "regex",
        include_str!("../languages/queries/regex/config.toml"),
    ),
    (
        "rust",
        include_str!("../languages/queries/rust/config.toml"),
    ),
    ("tsx", include_str!("../languages/queries/tsx/config.toml")),
    (
        "typescript",
        include_str!("../languages/queries/typescript/config.toml"),
    ),
    (
        "yaml",
        include_str!("../languages/queries/yaml/config.toml"),
    ),
];

/// What came of parsing [`SOURCES`].
struct Parsed {
    /// The manifests that parsed, in [`SOURCES`] order.
    manifests: Vec<Manifest>,
    /// One message per manifest that did not, naming the file.
    failures: Vec<String>,
}

/// The parse, run at most once however many threads reach it together.
static PARSED: LazyLock<Parsed> = LazyLock::new(|| {
    let mut manifests = Vec::with_capacity(SOURCES.len());
    let mut failures = Vec::new();

    for &(id, source) in SOURCES {
        match Manifest::parse(id, source) {
            Ok(manifest) => manifests.push(manifest),
            Err(message) => failures.push(message),
        }
    }

    Parsed {
        manifests,
        failures,
    }
});

/// Every vendored manifest that parsed, in directory order.
///
/// Includes hidden languages and languages with no grammar. Callers that want
/// only the languages a document can be set to must filter on
/// [`Manifest::is_hidden`] themselves — the flag is data, and dropping it here
/// would hide a distinction the manifests make deliberately.
#[must_use]
pub fn all() -> &'static [Manifest] {
    &PARSED.manifests
}

/// The manifest vendored under `id`, if there is one.
///
/// A linear scan: twenty-two entries, and a map would cost an allocation and a
/// hash to beat a comparison of short strings that almost always fails on the
/// first byte.
#[must_use]
pub fn by_id(id: &str) -> Option<&'static Manifest> {
    all().iter().find(|manifest| manifest.id() == id)
}

/// One message per vendored manifest that failed to parse.
///
/// Empty in any build whose tests passed — see this module's note on why a
/// parse failure is reported rather than raised.
#[must_use]
pub fn parse_failures() -> &'static [String] {
    &PARSED.failures
}
