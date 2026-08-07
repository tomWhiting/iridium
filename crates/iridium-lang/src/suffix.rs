//! Resolving a file's extension to a language, from the vendored manifests.
//!
//! Every language's `config.toml` carries `path_suffixes`, and that list is now
//! the answer rather than a hand-written `match`. Two things about the vendored
//! data shape this module, and neither is obvious:
//!
//! # `path_suffixes` is not all suffixes
//!
//! It carries whole file names too — `flake.lock`, `tsconfig.json`, `.env`,
//! `pixi.lock`. This module handles **only the dotless entries**, which are
//! extensions in the `Path::extension` sense. A dotted entry could never match
//! an extension anyway (`Path::extension` of `flake.lock` is `lock`), so
//! ignoring them here is not a loss — it is deferring them to whole-file-name
//! matching, which is a separate behaviour with its own decisions.
//!
//! # Case is load-bearing, in exactly one place
//!
//! `cpp` claims `"C"` and `"H"`; `c` claims `"c"`. On a case-sensitive
//! filesystem `.C` is the old Unix convention for a C++ source file, and `.c`
//! is C — two languages, one letter apart. Lower-casing first would merge them
//! and hand every C file to the C++ grammar.
//!
//! So the scan is **case-sensitive first, then case-insensitive**. `.C` and
//! `.c` are both resolved by the first pass, so they never reach the second;
//! `README.MD` falls through to the second and is Markdown. `c` is the only
//! extension in the whole vendored set whose lower-cased form is claimed by two
//! languages, and a test asserts that stays true — if a refresh creates a
//! second one, the fallback becomes order-dependent and someone has to decide.

use crate::Language;
use crate::manifest::Manifest;

/// Extensions Iridium claims that the vendored manifests do not.
///
/// A deliberate delta, not a second copy of the table: each entry exists
/// because dropping it would be a regression, and carries the reason. Kept
/// separate from the manifests because editing a vendored file would be undone
/// by the next refresh, silently.
///
/// This is also the seed of the user-facing override: "the manifests say X, and
/// this installation says Y" is the same mechanism whether the override comes
/// from Iridium or from a configuration file.
const LOCAL_EXTENSIONS: &[(Language, &str)] = &[
    // Python on Windows: a `.pyw` runs without a console window. It is Python
    // by every other measure and Iridium has always treated it as such. Zed's
    // manifest omits it and lists `mpy` instead, which reading the manifest
    // gains us; this keeps what reading it would otherwise lose.
    (Language::Python, "pyw"),
];

/// Manifests whose file associations belong to a different language here.
///
/// Zed makes JSONC a language of its own, with `grammar = "jsonc"`. Iridium
/// vendors no such grammar and does not need one: `tree-sitter-json` carries
/// `comment` in its `extras`, so a commented document parses with no error node
/// at all — verified in `iridium-syntax` by
/// `jsonc_is_the_json_grammar_and_its_comments_parse`, which pins it against a
/// grammar bump.
///
/// So the `jsonc` manifest's associations resolve to [`Language::Json`]. That
/// is a statement about which grammar parses which format, not a duplicated
/// extension list: `.jsonc`, and in due course `tsconfig.json` and its
/// relatives, come out of the vendored file.
const MANIFEST_ALIASES: &[(&str, Language)] = &[("jsonc", Language::Json)];

/// Every manifest whose `path_suffixes` this language claims: its own, plus any
/// aliased onto it.
fn claimed_by(language: Language) -> impl Iterator<Item = &'static str> {
    let own = language.manifest().into_iter();
    let aliased = MANIFEST_ALIASES
        .iter()
        .filter(move |(_, target)| *target == language)
        .filter_map(|(id, _)| crate::manifest::by_id(id));

    own.chain(aliased)
        .flat_map(Manifest::path_suffixes)
        .map(String::as_str)
}

/// Whether `entry` is an extension rather than a whole file name.
///
/// A dot means the manifest is naming a file — `flake.lock`, `.env` — and
/// `Path::extension` would never produce that string.
fn is_extension(entry: &str) -> bool {
    !entry.contains('.')
}

/// The extensions a language claims: its manifest's, then any aliased onto it,
/// then Iridium's own additions.
///
/// Whole file names are excluded; see the module note.
pub fn extensions_of(language: Language) -> impl Iterator<Item = &'static str> {
    let local = LOCAL_EXTENSIONS
        .iter()
        .filter(move |(owner, _)| *owner == language)
        .map(|(_, extension)| *extension);

    claimed_by(language)
        .filter(|entry| is_extension(entry))
        .chain(local)
}

/// The language claiming `ext`, which arrives without its leading dot.
///
/// Case-sensitive first, then case-insensitive — see the module note on why
/// that order is not a detail.
pub fn language_for_extension(ext: &str) -> Option<Language> {
    if let Some(language) = Language::all()
        .iter()
        .copied()
        .find(|&language| extensions_of(language).any(|claimed| claimed == ext))
    {
        return Some(language);
    }

    let lowered = ext.to_lowercase();
    Language::all()
        .iter()
        .copied()
        .find(|&language| extensions_of(language).any(|claimed| claimed.to_lowercase() == lowered))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::{LOCAL_EXTENSIONS, extensions_of, language_for_extension};
    use crate::Language;

    /// Everything the twelve languages Iridium named before #66 must keep
    /// resolving to, spelled out.
    ///
    /// Hand-written from the pre-#66 table rather than derived, so it is a
    /// genuine third opinion: reading the manifests must not quietly drop an
    /// association Iridium already had.
    const MUST_STILL_RESOLVE: &[(&str, Language)] = &[
        ("rs", Language::Rust),
        ("py", Language::Python),
        ("pyi", Language::Python),
        ("pyw", Language::Python),
        ("ts", Language::TypeScript),
        ("mts", Language::TypeScript),
        ("cts", Language::TypeScript),
        ("js", Language::JavaScript),
        ("mjs", Language::JavaScript),
        ("cjs", Language::JavaScript),
        ("jsx", Language::JavaScript),
        ("tsx", Language::Tsx),
        ("go", Language::Go),
        ("json", Language::Json),
        ("jsonc", Language::Json),
        ("yaml", Language::Yaml),
        ("yml", Language::Yaml),
        ("md", Language::Markdown),
        ("markdown", Language::Markdown),
        ("css", Language::Css),
        ("sh", Language::Bash),
        ("bash", Language::Bash),
        ("zsh", Language::Bash),
        ("c", Language::C),
        ("cpp", Language::Cpp),
        ("cxx", Language::Cpp),
        ("cc", Language::Cpp),
        ("hpp", Language::Cpp),
        ("hxx", Language::Cpp),
        ("hh", Language::Cpp),
    ];

    #[test]
    fn every_association_iridium_already_had_still_resolves() {
        let mut lost = Vec::new();
        for &(ext, expected) in MUST_STILL_RESOLVE {
            match language_for_extension(ext) {
                Some(actual) if actual == expected => {},
                other => lost.push(format!(
                    ".{ext} should be {}, resolved {:?}",
                    expected.id(),
                    other.map(|language| language.id())
                )),
            }
        }
        assert!(
            lost.is_empty(),
            "reading the manifests dropped associations Iridium already had:\n{}",
            lost.join("\n")
        );
    }

    /// The one association that deliberately changed.
    ///
    /// Iridium's old table gave `.h` to C, conceding in a comment that it was
    /// ambiguous. Zed's `cpp` claims it and its `c` does not, and the C++
    /// grammar is a superset — it parses a C header correctly, where the C
    /// grammar fails on a template or a namespace. So the manifest's answer is
    /// also the better one.
    #[test]
    fn the_h_header_moved_from_c_to_cpp() {
        assert_eq!(language_for_extension("h"), Some(Language::Cpp));
    }

    /// `.C` is C++ and `.c` is C, one letter apart.
    ///
    /// This is the reason the scan is case-sensitive before it is
    /// case-insensitive. Lower-casing first would give every C file to the C++
    /// grammar.
    #[test]
    fn capital_c_is_cpp_and_lower_c_is_c() {
        assert_eq!(language_for_extension("C"), Some(Language::Cpp));
        assert_eq!(language_for_extension("c"), Some(Language::C));
        assert_eq!(language_for_extension("H"), Some(Language::Cpp));
    }

    /// And the case-insensitive pass still does its job for everything else.
    #[test]
    fn an_extension_in_the_wrong_case_still_resolves() {
        assert_eq!(language_for_extension("MD"), Some(Language::Markdown));
        assert_eq!(language_for_extension("Rs"), Some(Language::Rust));
        assert_eq!(language_for_extension("JSON"), Some(Language::Json));
    }

    /// The guard on the fallback being order-independent.
    ///
    /// `c` is the only extension in the vendored set whose lower-cased form two
    /// languages claim, and both spellings are resolved by the case-sensitive
    /// pass, so the fallback never has to choose. If a refresh creates a second
    /// such extension the fallback becomes order-dependent, and this fails so
    /// someone decides rather than the scan order deciding.
    #[test]
    fn only_c_is_ambiguous_once_case_is_folded_and_both_spellings_resolve_exactly() {
        let mut folded: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
        for &language in Language::all() {
            for extension in extensions_of(language) {
                folded
                    .entry(extension.to_lowercase())
                    .or_default()
                    .insert(language.id());
            }
        }

        let ambiguous: Vec<&String> = folded
            .iter()
            .filter(|(_, owners)| owners.len() > 1)
            .map(|(extension, _)| extension)
            .collect();

        assert_eq!(
            ambiguous,
            vec!["c"],
            "the set of case-folding-ambiguous extensions changed; the \
             case-sensitive pass has to resolve every one of them"
        );

        // And it does, for both spellings.
        assert_eq!(language_for_extension("c"), Some(Language::C));
        assert_eq!(language_for_extension("C"), Some(Language::Cpp));
    }

    /// Whole file names are not extensions and must not leak into this path.
    ///
    /// `Path::extension` of `flake.lock` is `lock`, so a manifest entry of
    /// `flake.lock` matching here would mean nothing could ever produce it —
    /// and worse, an entry like `.env` would claim the empty-ish string.
    #[test]
    fn a_whole_file_name_is_not_offered_as_an_extension() {
        for &language in Language::all() {
            for extension in extensions_of(language) {
                assert!(
                    !extension.contains('.'),
                    "{} offers {extension:?} as an extension, but it is a file name",
                    language.id()
                );
            }
        }

        assert_eq!(language_for_extension("flake.lock"), None);
        assert_eq!(language_for_extension("tsconfig.json"), None);
        assert_eq!(language_for_extension(".env"), None);
    }

    /// Gains that come free with reading the manifest, sampled.
    ///
    /// Not exhaustive — the point is that the list grew rather than that it
    /// grew by exactly this much.
    #[test]
    fn reading_the_manifest_gains_associations_the_hand_written_table_lacked() {
        assert_eq!(language_for_extension("mdx"), Some(Language::Markdown));
        assert_eq!(language_for_extension("zshrc"), Some(Language::Bash));
        assert_eq!(language_for_extension("postcss"), Some(Language::Css));
        assert_eq!(language_for_extension("mpy"), Some(Language::Python));
        assert_eq!(language_for_extension("ino"), Some(Language::Cpp));
    }

    /// An extension nobody claims resolves to nothing.
    #[test]
    fn an_unclaimed_extension_resolves_to_nothing() {
        assert_eq!(language_for_extension("awl"), None);
        assert_eq!(language_for_extension(""), None);
        assert_eq!(language_for_extension("zzzz"), None);
    }

    /// Every local addition is genuinely an addition.
    ///
    /// If a refresh adds one of these to the manifest, the delta entry becomes
    /// a duplicate that nothing would notice — so it is checked, and the entry
    /// should then be deleted.
    #[test]
    fn no_local_extension_duplicates_one_the_manifest_already_carries() {
        for &(language, extension) in LOCAL_EXTENSIONS {
            let manifest = language.manifest().expect("vendored");
            assert!(
                !manifest.path_suffixes().iter().any(|s| s == extension),
                "{} now carries {extension:?} itself; the local entry is a \
                 duplicate and should be deleted",
                language.id()
            );
        }
    }

    /// Every language still claims at least one extension.
    #[test]
    fn no_language_lost_every_association_it_had() {
        for &language in Language::all() {
            assert!(
                extensions_of(language).next().is_some(),
                "{} claims no extension at all",
                language.id()
            );
        }
    }
}
