//! The shape of a vendored `config.toml`, and what Iridium reads out of it.
//!
//! # Only the fields something uses
//!
//! The manifests carry around thirty top-level keys between them, and most
//! describe Zed features Iridium does not have — `debuggers`,
//! `prettier_parser_name`, `scope_opt_in_language_servers`,
//! `completion_query_characters`. Unknown keys are ignored rather than
//! rejected: these are vendored upstream files, a refresh will add keys nobody
//! here has heard of, and failing to parse a manifest because it grew a field
//! would be turning someone else's new feature into Iridium's outage.
//!
//! That cuts the other way too. Nothing is deserialized speculatively — a field
//! appears here when something reads it, so the struct never claims to
//! understand more of the file than it does.

use serde::Deserialize;

/// A paired comment delimiter, as the manifests spell it.
///
/// The manifests also carry `prefix` (what to put on continuation lines) and
/// `tab_size`; neither is read, so neither is named. Serde ignores them.
#[derive(Debug, Clone, Deserialize)]
struct CommentBlock {
    /// The opening delimiter, such as `/*`.
    start: String,
    /// The closing delimiter, such as `*/`.
    end: String,
}

/// One language's vendored manifest.
///
/// Constructed only by [`super::embedded`], which owns the `include_str!`
/// table; `id` is the directory the file came out of rather than anything in
/// the file, which is why this type cannot be deserialized on its own.
#[derive(Debug, Clone)]
pub struct Manifest {
    /// The directory this manifest was vendored under — `rust`, `markdown-inline`.
    ///
    /// This, not [`Manifest::name`], is the stable identifier: `name` is a
    /// display string (`"Shell Script"`, `"C++"`) and `grammar` names a parser
    /// that several languages can share.
    id: &'static str,
    /// The parsed file.
    fields: Fields,
}

/// The keys read out of the file itself.
#[derive(Debug, Clone, Deserialize)]
struct Fields {
    /// The display name — `"Shell Script"`, not `"bash"`.
    name: String,
    /// The tree-sitter grammar this language is parsed with.
    ///
    /// Not always the language's own id: Zed parses JavaScript with the `tsx`
    /// grammar, and `jsonc` names a grammar Iridium does not vendor.
    grammar: String,
    /// File names and extensions that select this language.
    ///
    /// Despite the name these are not all suffixes in the `.ext` sense — the
    /// list carries whole file names (`flake.lock`, `tsconfig.json`,
    /// `COMMIT_EDITMSG`) alongside true extensions.
    #[serde(default)]
    path_suffixes: Vec<String>,
    /// Every line-comment token the language accepts, most common first.
    ///
    /// Written with a trailing space (`"// "`) because Zed inserts them
    /// verbatim. Rust lists three — `// `, `/// `, `//! ` — and only the first
    /// is a plain comment.
    #[serde(default)]
    line_comments: Vec<String>,
    /// The block-comment pair, when the language has one distinct from its
    /// documentation comment.
    #[serde(default)]
    block_comment: Option<CommentBlock>,
    /// The documentation-comment pair.
    ///
    /// Load-bearing as a fallback: Rust, Go, C and C++ carry no
    /// `block_comment` at all, and this is where their `/* */` lives.
    #[serde(default)]
    documentation_comment: Option<CommentBlock>,
    /// Whether this language exists only to be injected into another.
    ///
    /// `jsdoc`, `regex` and `markdown-inline` are hidden: they have queries and
    /// a grammar but no document is ever "written in" them, so they must not be
    /// offered as a language a buffer can be set to.
    #[serde(default)]
    hidden: bool,
}

impl Manifest {
    /// Parses one manifest, tagging it with the directory it came from.
    ///
    /// # Errors
    ///
    /// The TOML error, with the manifest's id prefixed so a failure names the
    /// file rather than just a line and column.
    pub(super) fn parse(id: &'static str, source: &str) -> Result<Self, String> {
        toml::from_str(source)
            .map(|fields| Self { id, fields })
            .map_err(|error| format!("{id}/config.toml: {error}"))
    }

    /// The directory this manifest was vendored under.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        self.id
    }

    /// The display name, such as `"Shell Script"`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.fields.name
    }

    /// The tree-sitter grammar this language is parsed with.
    #[must_use]
    pub fn grammar(&self) -> &str {
        &self.fields.grammar
    }

    /// File names and extensions that select this language, in the order the
    /// manifest lists them.
    #[must_use]
    pub fn path_suffixes(&self) -> &[String] {
        &self.fields.path_suffixes
    }

    /// Whether this language exists only to be injected into another.
    #[must_use]
    pub const fn is_hidden(&self) -> bool {
        self.fields.hidden
    }

    /// The line-comment token, without the trailing space the manifest writes.
    ///
    /// The first entry wins: the list is ordered most-common-first, and the
    /// later ones are doc-comment variants (`/// `, `//! `) that toggling a
    /// plain comment must not produce.
    ///
    /// The trailing space is stripped rather than kept because it is a *Zed*
    /// insertion convention, and two manifests (`gitcommit`, `gomod`) omit it —
    /// so trimming is the rule that makes all twenty-one agree, not a special
    /// case for the ones that have it. A token that is nothing but whitespace
    /// is treated as absent.
    #[must_use]
    pub fn line_comment(&self) -> Option<&str> {
        self.fields
            .line_comments
            .first()
            .map(|token| token.trim_end())
            .filter(|token| !token.is_empty())
    }

    /// The block-comment pair, as `(start, end)`.
    ///
    /// `block_comment` when the manifest has one, and `documentation_comment`
    /// otherwise. The fallback is not a convenience: Rust, Go, C and C++ have
    /// no `block_comment` key, and without it those four would report no block
    /// comment at all. The precedence matters in the other direction too —
    /// TypeScript, JavaScript and TSX carry both, and their
    /// `documentation_comment` opens `/**`, which is a doc comment rather than
    /// the `/*` a toggle should write.
    #[must_use]
    pub fn block_comment(&self) -> Option<(&str, &str)> {
        self.fields
            .block_comment
            .as_ref()
            .or(self.fields.documentation_comment.as_ref())
            .map(|block| (block.start.as_str(), block.end.as_str()))
    }
}
