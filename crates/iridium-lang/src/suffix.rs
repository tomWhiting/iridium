//! Resolving a file to a language, from the vendored manifests.
//!
//! Every language's `config.toml` carries `path_suffixes`, and that list is now
//! the answer rather than a hand-written `match`. Two things about the vendored
//! data shape this module, and neither is obvious:
//!
//! # `path_suffixes` is not all suffixes
//!
//! It carries whole file names too — `flake.lock`, `tsconfig.json`, `.env`,
//! `pixi.lock` — and dotless entries that are not extensions either: `bashrc`,
//! `zshrc` and `clang-format` exist to claim dotfiles for which
//! `Path::extension` returns nothing at all.
//!
//! So there are two entry points and they are not the same. [`entry_claims`]
//! has the single rule that covers every case — a name **equals** an entry or
//! **ends with `.` followed by** it — and [`language_for_file_name`] is the
//! capable one that faces should reach for. [`language_for_extension`] is the
//! narrower view, for callers holding only an extension; it sees the dotless
//! entries and nothing else.
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

/// Every entry a language claims: its manifest's, then any aliased onto it,
/// then Iridium's own additions.
///
/// Includes the whole file names; [`extensions_of`] is the filtered view.
fn entries_of(language: Language) -> impl Iterator<Item = &'static str> {
    let local = LOCAL_EXTENSIONS
        .iter()
        .filter(move |(owner, _)| *owner == language)
        .map(|(_, extension)| *extension);

    claimed_by(language).chain(local)
}

/// The extensions a language claims: its manifest's, then any aliased onto it,
/// then Iridium's own additions.
///
/// Whole file names are excluded; see the module note.
pub fn extensions_of(language: Language) -> impl Iterator<Item = &'static str> {
    entries_of(language).filter(|entry| is_extension(entry))
}

/// Whether a file name is claimed by one `path_suffixes` entry.
///
/// One rule covers both kinds of entry the manifests carry:
///
/// > the name **equals** the entry, or **ends with `.` followed by** it.
///
/// - `main.rs` ends with `.` + `rs`.
/// - `flake.lock` equals `flake.lock`.
/// - `.env` equals `.env`.
/// - `.bashrc` ends with `.` + `bashrc` — which is why the dotless entries are
///   not simply extensions: `bashrc`, `zshrc` and `clang-format` exist to claim
///   dotfiles that have no extension at all in the `Path::extension` sense.
///
/// The leading dot is what makes this safe. A bare `ends_with` would let `sh`
/// claim `splash`, and `c` claim `music`. Requiring the separator means an
/// entry can only ever match a whole dot-delimited tail.
fn entry_claims(file_name: &str, entry: &str) -> bool {
    if file_name == entry {
        return true;
    }
    // Indexing by byte is safe: `ends_with` has established the tail, and the
    // byte before it can only equal `b'.'` if it is a whole ASCII character.
    file_name.len() > entry.len()
        && file_name.ends_with(entry)
        && file_name.as_bytes()[file_name.len() - entry.len() - 1] == b'.'
}

/// The language claiming a whole file name — `main.rs`, `flake.lock`, `.env`.
///
/// Case-sensitive first, then case-insensitive, for the same reason as
/// [`language_for_extension`]: `.C` and `.c` are different languages.
pub fn language_for_file_name(file_name: &str) -> Option<Language> {
    if let Some(language) = Language::all()
        .iter()
        .copied()
        .find(|&language| entries_of(language).any(|entry| entry_claims(file_name, entry)))
    {
        return Some(language);
    }

    let lowered = file_name.to_lowercase();
    Language::all().iter().copied().find(|&language| {
        entries_of(language).any(|entry| entry_claims(&lowered, &entry.to_lowercase()))
    })
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

    use super::{LOCAL_EXTENSIONS, extensions_of, language_for_extension, language_for_file_name};
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
        assert_eq!(language_for_extension("nonesuch"), None);
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

    /// Whole file names the manifests claim, which no extension could reach.
    ///
    /// `Path::extension` of `flake.lock` is `lock`, and of `.bashrc` is
    /// nothing at all — so before whole-name matching every one of these opened
    /// as plain text.
    #[test]
    fn a_whole_file_name_the_manifests_claim_now_resolves() {
        for (name, expected) in [
            ("flake.lock", Language::Json),
            ("bun.lock", Language::Json),
            ("tsconfig.json", Language::Json),
            ("devcontainer.json", Language::Json),
            ("pyrightconfig.json", Language::Json),
            ("pixi.lock", Language::Yaml),
            (".env", Language::Bash),
            (".bashrc", Language::Bash),
            (".zshrc", Language::Bash),
            ("PKGBUILD", Language::Bash),
            (".clang-format", Language::Yaml),
        ] {
            assert_eq!(
                language_for_file_name(name),
                Some(expected),
                "{name} should be {}",
                expected.id()
            );
        }
    }

    /// An ordinary name still resolves by its extension, because an extension
    /// is only the tail of a name after a dot.
    #[test]
    fn an_ordinary_file_name_resolves_by_its_extension() {
        assert_eq!(language_for_file_name("main.rs"), Some(Language::Rust));
        assert_eq!(language_for_file_name("a.b.c.py"), Some(Language::Python));
        assert_eq!(
            language_for_file_name("README.MD"),
            Some(Language::Markdown)
        );
        assert_eq!(language_for_file_name("vec.C"), Some(Language::Cpp));
        assert_eq!(language_for_file_name("vec.c"), Some(Language::C));
    }

    /// The trap the leading dot exists to close.
    ///
    /// A bare `ends_with` would let `sh` claim `splash`, `c` claim `music`, and
    /// `go` claim `cargo`. Requiring a `.` before the match means an entry can
    /// only ever claim a whole dot-delimited tail.
    #[test]
    fn an_entry_cannot_claim_a_name_that_merely_ends_with_its_letters() {
        for name in ["splash", "music", "cargo", "notmd", "flourish"] {
            assert_eq!(
                language_for_file_name(name),
                None,
                "{name} was claimed by an entry it only happens to end with"
            );
        }
    }

    /// A name that is exactly an entry is claimed, which is the point of the
    /// dotless entries.
    #[test]
    fn a_name_equal_to_an_entry_is_claimed() {
        assert_eq!(language_for_file_name("profile"), Some(Language::Bash));
        assert_eq!(language_for_file_name("bashrc"), Some(Language::Bash));
    }

    #[test]
    fn a_name_nothing_claims_resolves_to_nothing() {
        for name in ["notes", "hello.nonesuch", "", ".", "..", "archive.tar.gz"] {
            assert_eq!(language_for_file_name(name), None, "{name}");
        }
    }

    /// `for_path` reads the name off the path and nothing else.
    #[test]
    fn for_path_uses_the_file_name_not_the_directories() {
        use std::path::Path;

        assert_eq!(
            Language::for_path(Path::new("/a/rs/deep/main.py")),
            Some(Language::Python),
            "a directory named `rs` must not make this Rust"
        );
        assert_eq!(
            Language::for_path(Path::new("/etc/nixos/flake.lock")),
            Some(Language::Json)
        );
        assert_eq!(Language::for_path(Path::new("/")), None);
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
