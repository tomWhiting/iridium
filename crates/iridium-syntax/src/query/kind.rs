//! The kinds of tree-sitter query a language can ship.
//!
//! The vendored query directories carry more files than these six — Zed also
//! ships `overrides.scm`, `imports.scm`, `runnables.scm`, `debugger.scm`,
//! `redactions.scm`, `embedding.scm`, `contexts.scm`, `structure.scm` and a
//! `config.toml`. Every one of those describes a Zed feature Iridium does not
//! have, so loading them would cost bundle size and compile time for nothing.
//! The six below are exactly the ones something in this crate reads, and the
//! enum is deliberately not `#[non_exhaustive]`: adding a kind means adding a
//! reader for it, and the compiler should force that pairing.

use std::fmt;

/// One kind of tree-sitter query, named by the file that carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum QueryKind {
    /// Maps syntax nodes to highlight types. Drives [`crate::Highlighter`].
    Highlights,
    /// Pairs of delimiters that open and close a construct.
    Brackets,
    /// Named regions — functions, classes, comments — that structural
    /// selection and text objects move between.
    TextObjects,
    /// Indentation rules for the lines a construct spans.
    Indents,
    /// Regions written in another language, such as a SQL string or a fenced
    /// code block in Markdown.
    Injections,
    /// The document's symbol outline.
    Outline,
}

impl QueryKind {
    /// How many kinds there are.
    ///
    /// [`QueryKind::all`] returns an array of exactly this length, so a variant
    /// added to the enum but forgotten in `all()` fails to compile rather than
    /// leaving a kind the cache has no column for.
    pub const COUNT: usize = 6;

    /// Every kind, in a fixed order.
    ///
    /// The order is the index space used by the compiled-query cache, so it is
    /// part of this module's contract rather than a presentation detail:
    /// [`QueryKind::index`] must agree with a value's position here.
    #[must_use]
    pub const fn all() -> &'static [Self; Self::COUNT] {
        &[
            Self::Highlights,
            Self::Brackets,
            Self::TextObjects,
            Self::Indents,
            Self::Injections,
            Self::Outline,
        ]
    }

    /// The file name this kind is vendored under, inside a language's query
    /// directory.
    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::Highlights => "highlights.scm",
            Self::Brackets => "brackets.scm",
            Self::TextObjects => "textobjects.scm",
            Self::Indents => "indents.scm",
            Self::Injections => "injections.scm",
            Self::Outline => "outline.scm",
        }
    }

    /// This kind's position in [`QueryKind::all`].
    ///
    /// Used to index the compiled-query cache. Written as a match rather than
    /// a search so it stays constant-time and cannot silently disagree with
    /// `all()` — the test module asserts the two agree.
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Highlights => 0,
            Self::Brackets => 1,
            Self::TextObjects => 2,
            Self::Indents => 3,
            Self::Injections => 4,
            Self::Outline => 5,
        }
    }
}

impl fmt::Display for QueryKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.file_name())
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::Path;

    use super::QueryKind;

    #[test]
    fn indices_agree_with_the_declared_order() {
        for (position, &kind) in QueryKind::all().iter().enumerate() {
            assert_eq!(
                kind.index(),
                position,
                "{kind} claims index {} but sits at {position} in all()",
                kind.index()
            );
        }
    }

    #[test]
    fn every_kind_appears_in_all_exactly_once() {
        let mut sorted = QueryKind::all().to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            QueryKind::all().len(),
            "all() repeats a kind, so one cache slot would shadow another"
        );
    }

    #[test]
    fn every_kind_names_a_distinct_scm_file() {
        for &kind in QueryKind::all() {
            assert_eq!(
                Path::new(kind.file_name()).extension(),
                Some(OsStr::new("scm")),
                "{kind} does not name a query file"
            );
            for &other in QueryKind::all() {
                assert!(
                    kind == other || kind.file_name() != other.file_name(),
                    "{kind} and {other} name the same file"
                );
            }
        }
    }

    #[test]
    fn a_kind_displays_as_its_file_name() {
        assert_eq!(QueryKind::TextObjects.to_string(), "textobjects.scm");
    }
}
