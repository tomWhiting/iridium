//! Which language a document is written in.
//!
//! Identity only: a closed enum, its stable identifier, and the two functions
//! that map a string onto it. Nothing here parses anything, and this crate
//! depends on nothing that does.
//!
//! # Why this is its own crate
//!
//! Language *identity* is needed in configurations that have no parser.
//! `iridium-editor` built without its `syntax` feature carries no tree-sitter
//! at all, and yet still has to know that a document is Rust: comment
//! toggling, indent rules and auto-pairs all key off the language and none of
//! them involves a syntax tree.
//!
//! Before this crate existed there were **two** `Language` enums — the real
//! one in `iridium-syntax` and a hand-maintained stub in `iridium-editor`,
//! selected by that feature. They disagreed, quietly and in both directions:
//!
//! - the real `from_id` accepted aliases (`rs`, `py`, `golang`, `zsh`) and
//!   lower-cased its input; the stub matched canonical ids exactly and
//!   case-sensitively, so `from_id("rs")` was `Some(Rust)` in one build and
//!   `None` in the other;
//! - the real `from_extension` knew `.pyw` and the stub did not; the stub knew
//!   `.jsonc` and the real one did not. Collapsing the two took the real one's
//!   answer, which left `.jsonc` resolving to nothing at all; the grammar was
//!   then checked and it turned out the stub had been right, so `.jsonc` is
//!   back — see `MANIFEST_ALIASES` in the crate's `suffix` module for how
//!   it resolves now;
//! - the real enum derived `Serialize`/`Deserialize` and the stub did not, so
//!   a value that round-tripped through a configuration in one build would not
//!   compile in the other.
//!
//! None of that could be caught by a test, because the two types are feature
//! *alternatives*: no build has both in scope, so nothing could compare them.
//! A guard was structurally impossible, which is why the answer is one type
//! rather than two copies and a check.
//!
//! The divergence had already produced a live defect — see the note on
//! `language_tokens` in `iridium-editor`, where the comment-syntax table was
//! gated on the same feature and answered "this language has no comments" for
//! every language in the parser-free build.
//!
//! # What else lives here
//!
//! The vendored `languages/` tree, and [`query`], which serves its `.scm`
//! files as text. That is data about languages, not a parser — this crate
//! still depends on nothing that parses anything — and it is here for the same
//! reason the enum is: the `config.toml` manifests sitting beside those
//! queries describe comment syntax, brackets and file associations, all of
//! which the parser-free build needs. Splitting the directory so only the
//! manifests came along would leave one vendored upstream tree owned by two
//! crates, and the next refresh would have to know that.
//!
//! [`manifest`] reads those files; a private `suffix` module turns their file
//! associations into an answer to "what language is this path?". Between them they are why
//! this crate no longer hand-maintains a single fact about a language that the
//! vendored tree already states.

use serde::{Deserialize, Serialize};

pub mod manifest;
pub mod query;
mod suffix;

/// A language a document can be written in.
///
/// Every variant has a tree-sitter grammar behind it when `iridium-syntax` is
/// compiled in, and a stable identity regardless.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
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
    /// How many languages there are.
    ///
    /// [`Language::all`] returns an array of exactly this length, which makes
    /// the count load-bearing rather than decorative: a variant added to the
    /// enum but forgotten in `all()` fails to compile here instead of leaving
    /// a language that exists but is invisible to everything that iterates.
    pub const COUNT: usize = 13;

    /// The stable identifier — the spelling used in a configuration file, a
    /// serialized document, and across the language boundary to TypeScript.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        match self {
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

    /// Parses a language from its identifier, or a common alias for one.
    ///
    /// Case-insensitive, and deliberately generous: this is what a
    /// hand-written configuration file and a host API are both read through,
    /// and refusing `rs` because the canonical spelling is `rust` would be
    /// pedantry with no upside.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        match id.to_lowercase().as_str() {
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

    /// The file extensions this language claims, without leading dots.
    ///
    /// Read from the language's vendored manifest rather than written out here
    /// — see the crate's `suffix` module for what that entails, including the whole file
    /// names the manifest also carries and the one case-sensitivity that
    /// matters.
    ///
    /// Returns an iterator rather than a slice because the list is assembled
    /// from up to three sources and no longer exists as one contiguous array.
    pub fn extensions(self) -> impl Iterator<Item = &'static str> {
        crate::suffix::extensions_of(self)
    }

    /// Detects a language from a file extension, without the leading dot.
    ///
    /// Case-sensitive first and case-insensitive second, so `README.MD` is
    /// Markdown while `.C` stays C++ and `.c` stays C. The crate's `suffix`
    /// module has the reason that ordering is load-bearing, not incidental.
    ///
    /// The search is linear over [`Self::all`], which is a few dozen string
    /// comparisons on the path that opens a file — not on any path that runs
    /// per keystroke or per frame.
    #[must_use]
    pub fn from_extension(ext: &str) -> Option<Self> {
        crate::suffix::language_for_extension(ext)
    }

    /// Every language, in the order that is this type's index space.
    ///
    /// The order is part of the contract, not a presentation detail:
    /// `iridium-syntax` indexes its compiled-query cache by position here.
    #[must_use]
    pub const fn all() -> &'static [Self; Self::COUNT] {
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

    /// This language's position in [`Language::all`].
    ///
    /// Written as a match rather than a search so it stays constant-time and
    /// cannot silently disagree with `all()`; a test below asserts they agree.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Rust => 0,
            Self::Python => 1,
            Self::TypeScript => 2,
            Self::JavaScript => 3,
            Self::Tsx => 4,
            Self::Go => 5,
            Self::Json => 6,
            Self::Yaml => 7,
            Self::Markdown => 8,
            Self::Css => 9,
            Self::Bash => 10,
            Self::C => 11,
            Self::Cpp => 12,
        }
    }

    /// This language's vendored manifest, if one was vendored for it.
    ///
    /// Every variant has one today and a test asserts it, but the return type
    /// is an `Option` rather than a bare reference because the manifests are a
    /// vendored upstream tree: a refresh can drop a directory, and that should
    /// cost a caller a `None` to handle rather than this crate a panic.
    #[must_use]
    pub fn manifest(self) -> Option<&'static manifest::Manifest> {
        manifest::by_id(self.id())
    }
}

#[cfg(test)]
mod tests {
    use super::Language;

    #[test]
    fn every_language_round_trips_through_its_identifier() {
        for &language in Language::all() {
            assert_eq!(
                Language::from_id(language.id()),
                Some(language),
                "{language:?} does not survive its own id"
            );
        }
    }

    #[test]
    fn index_agrees_with_the_order_of_all() {
        for (position, &language) in Language::all().iter().enumerate() {
            assert_eq!(language.index(), position, "{language:?}");
        }
    }

    #[test]
    fn identifiers_are_distinct() {
        let mut seen: Vec<&str> = Language::all().iter().map(Language::id).collect();
        seen.sort_unstable();
        let count = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), count, "two languages share an identifier");
    }

    #[test]
    fn an_identifier_is_read_whatever_its_case() {
        // A configuration file is written by hand, and `Rust` is what somebody
        // writes when they are naming a language rather than an identifier.
        assert_eq!(Language::from_id("Rust"), Some(Language::Rust));
        assert_eq!(Language::from_id("RUST"), Some(Language::Rust));
    }

    #[test]
    fn common_aliases_resolve() {
        assert_eq!(Language::from_id("rs"), Some(Language::Rust));
        assert_eq!(Language::from_id("py"), Some(Language::Python));
        assert_eq!(Language::from_id("golang"), Some(Language::Go));
        assert_eq!(Language::from_id("zsh"), Some(Language::Bash));
        assert_eq!(Language::from_id("c++"), Some(Language::Cpp));
    }

    #[test]
    fn an_unknown_identifier_is_none_rather_than_a_guess() {
        assert_eq!(Language::from_id("cobol"), None);
        assert_eq!(Language::from_id(""), None);
    }

    #[test]
    fn extensions_are_read_whatever_their_case() {
        assert_eq!(Language::from_extension("RS"), Some(Language::Rust));
        assert_eq!(Language::from_extension("MD"), Some(Language::Markdown));
    }

    #[test]
    fn header_extensions_belong_to_cpp() {
        // `.h` used to be C here, with a comment conceding the ambiguity. The
        // vendored manifests give it to C++ — its `cpp` claims `h` and its `c`
        // does not — and that is also the better answer, because the C++
        // grammar is a superset: it parses a C header, where the C grammar
        // fails on a template or a namespace. See `suffix::tests` for the
        // `.C` / `.c` pair, which is the case where one letter is two
        // languages.
        assert_eq!(Language::from_extension("h"), Some(Language::Cpp));
        assert_eq!(Language::from_extension("hpp"), Some(Language::Cpp));
        assert_eq!(Language::from_extension("hh"), Some(Language::Cpp));
        assert_eq!(Language::from_extension("c"), Some(Language::C));
    }

    #[test]
    fn jsonc_is_json_but_is_not_an_identifier() {
        // The extension resolves, because a `.jsonc` file wants the JSON
        // grammar and that grammar parses its comments.
        assert_eq!(Language::from_extension("jsonc"), Some(Language::Json));
        assert_eq!(Language::from_extension("JSONC"), Some(Language::Json));

        // The *identifier* deliberately does not, because nothing produces it:
        // a face sets a document's language from `id()`, which is `"json"`.
        // Accepting `"jsonc"` here would claim a name this type never emits and
        // that no serialized document can contain.
        assert_eq!(Language::from_id("jsonc"), None);
        assert_eq!(Language::Json.id(), "json");
    }

    /// The forward and reverse directions agree.
    ///
    /// `from_extension` searches `extensions()`, so this cannot fail by
    /// construction today. It is written down anyway because the day someone
    /// reintroduces a hand-written match for speed, this is the test that says
    /// what the match now owes.
    #[test]
    fn every_listed_extension_resolves_to_the_language_that_lists_it() {
        for &language in Language::all() {
            for extension in language.extensions() {
                assert_eq!(
                    Language::from_extension(extension),
                    Some(language),
                    "{language:?} lists {extension:?}, which resolves elsewhere"
                );
            }
        }
    }

    #[test]
    fn no_extension_names_two_languages() {
        // This is what makes the scan order in `from_extension` irrelevant.
        // Compared exactly, not case-folded: `cpp` claims `"C"` and `c` claims
        // `"c"`, which are two extensions rather than one. `suffix::tests`
        // holds the separate guarantee that every case-folding clash is
        // resolved before the case-insensitive pass runs.
        let mut seen: Vec<(&str, Language)> = Vec::new();
        for &language in Language::all() {
            for extension in language.extensions() {
                if let Some((_, owner)) = seen.iter().find(|(name, _)| *name == extension) {
                    panic!("{extension:?} is claimed by both {owner:?} and {language:?}");
                }
                seen.push((extension, language));
            }
        }
    }

    #[test]
    fn every_extension_carries_no_dot() {
        // Callers pass what `Path::extension` returns, which never includes a
        // leading dot and never contains one. Case is *not* asserted: the
        // manifests carry `"C"`, `"H"` and `"MD"` deliberately, and
        // `from_extension` matches exactly before it folds case.
        for &language in Language::all() {
            assert!(
                language.extensions().next().is_some(),
                "{language:?} claims no extension, so no file can ever select it"
            );
            for extension in language.extensions() {
                assert!(
                    !extension.contains('.'),
                    "{language:?} lists {extension:?}, which is a file name rather \
                     than an extension"
                );
            }
        }
    }

    #[test]
    fn an_unknown_extension_is_none() {
        assert_eq!(Language::from_extension("cobol"), None);
        assert_eq!(Language::from_extension(""), None);
    }

    #[test]
    fn serialization_uses_the_lowercase_identifier() {
        // The wire form the web face and any serialized document share. It
        // must equal `id()`, or a document saved by one face names a language
        // another cannot read back.
        for &language in Language::all() {
            let json = serde_json::to_string(&language).expect("a language serializes");
            assert_eq!(json, format!("\"{}\"", language.id()), "{language:?}");
            let parsed: Language = serde_json::from_str(&json).expect("a language deserializes");
            assert_eq!(parsed, language);
        }
    }
}
