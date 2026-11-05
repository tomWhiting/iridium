//! A parser on its own, for callers that keep their trees somewhere else.
//!
//! [`crate::SyntaxTree`] owns a parser and the one tree it maintains, which is
//! right for a document parsed on the thread that edits it. A document parsed
//! *off* that thread needs the two apart: the tree stays with the editor, which
//! shifts it on every keystroke, while the parser lives on the thread that does
//! the slow part and is handed a copy of the tree to build on.
//!
//! # Text arrives in pieces
//!
//! [`SyntaxParser::parse`] reads the document through a callback rather than a
//! string. An editor holds its text as a rope, and turning a rope into one
//! string copies the whole document — on a 500,000-line file, tens of
//! megabytes on every parse. Read in pieces, the parse touches only the text
//! tree-sitter actually asks for, which on an incremental reparse is little
//! more than the edit.

use std::ops::{ControlFlow, Range};

use tree_sitter::{ParseOptions, Parser, Tree};

use crate::grammar::grammar;
use crate::{Language, SyntaxError};

/// A tree-sitter parser for one language, holding no tree of its own.
pub struct SyntaxParser {
    language: Language,
    parser: Parser,
}

impl std::fmt::Debug for SyntaxParser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyntaxParser")
            .field("language", &self.language)
            .finish_non_exhaustive()
    }
}

/// How a [`SyntaxParser::parse`] ended.
#[derive(Debug)]
pub enum ParseOutcome {
    /// The parse finished, and this is its tree.
    Parsed(Tree),
    /// The caller's progress check asked the parse to stop, and it did.
    ///
    /// The parser has already been reset, so the next parse starts afresh
    /// rather than resuming a parse of text that may no longer exist.
    Cancelled,
    /// Tree-sitter produced no tree for a reason other than cancellation.
    ///
    /// It documents only a missing language, a cancellation and a timeout as
    /// causes, and none of those can reach here, so this is the honest name
    /// for "the library broke its contract" rather than an expected case.
    Failed,
}

impl SyntaxParser {
    /// Creates a parser for `language`.
    ///
    /// # Errors
    ///
    /// The same as [`crate::SyntaxTree::new`]:
    /// [`SyntaxError::UnsupportedLanguage`] when no grammar is linked for the
    /// language, and [`SyntaxError::ParseError`] when one is linked but this
    /// build of tree-sitter refuses it.
    pub fn new(language: Language) -> Result<Self, SyntaxError> {
        let grammar = grammar(language).ok_or_else(|| SyntaxError::UnsupportedLanguage {
            language: language.id().to_owned(),
        })?;

        let mut parser = Parser::new();
        parser
            .set_language(&grammar)
            .map_err(|e| SyntaxError::ParseError {
                message: format!("Failed to set language: {e}"),
            })?;

        Ok(Self { language, parser })
    }

    /// Returns the language this parser reads.
    #[must_use]
    pub const fn language(&self) -> Language {
        self.language
    }

    /// Parses the text `read` supplies, building on `old` when given one.
    ///
    /// `read(byte)` returns the text starting at `byte` — any non-empty prefix
    /// of the rest of the document, such as the remainder of the rope chunk
    /// holding that byte — and an empty slice at or past the end. Every piece
    /// must end on a character boundary: tree-sitter decodes each piece on its
    /// own, and a character split across two reads as an invalid byte. A rope's
    /// chunks are whole strings, so reading one chunk at a time satisfies this
    /// for free.
    ///
    /// `old`, when given, must already have been passed every edit made since
    /// it was parsed (`Tree::edit`): it is then the reference the reparse
    /// reuses unchanged subtrees from. A tree that was not told about an edit
    /// produces a parse that silently disagrees with the text.
    ///
    /// `keep_going` is asked periodically while the parse runs, and returning
    /// `false` stops it. That is what lets a parse of text that has since been
    /// replaced be abandoned instead of finished for nothing.
    pub fn parse<T, R, K>(
        &mut self,
        mut read: R,
        old: Option<&Tree>,
        mut keep_going: K,
    ) -> ParseOutcome
    where
        T: AsRef<[u8]>,
        R: FnMut(usize) -> T,
        K: FnMut() -> bool,
    {
        let mut cancelled = false;
        let mut progress = |_: &tree_sitter::ParseState| {
            if keep_going() {
                ControlFlow::Continue(())
            } else {
                cancelled = true;
                ControlFlow::Break(())
            }
        };
        let options = ParseOptions::new().progress_callback(&mut progress);
        let parsed = self
            .parser
            .parse_with_options(&mut |byte, _| read(byte), old, Some(options));
        match parsed {
            Some(tree) => ParseOutcome::Parsed(tree),
            None if cancelled => {
                // A cancelled parse resumes on the next call unless reset,
                // and the next call is for different text.
                self.parser.reset();
                ParseOutcome::Cancelled
            },
            None => {
                self.parser.reset();
                ParseOutcome::Failed
            },
        }
    }
}

/// Returns the byte ranges whose structure differs between `old` and `new`.
///
/// `old` must have been edited to match the text `new` was parsed from. The
/// cost is tree-sitter's: it walks both trees side by side and skips subtrees
/// they share, so it is proportional to how much of the tree the reparse
/// rebuilt. That is not always small. A grammar whose top-level repetition
/// tree-sitter cannot reuse after an edit — Rust's is one — has its whole
/// spine rebuilt by every reparse, and this then walks the whole spine.
#[must_use]
pub fn changed_ranges(old: &Tree, new: &Tree) -> Vec<Range<usize>> {
    old.changed_ranges(new)
        .map(|range| range.start_byte..range.end_byte)
        .collect()
}

#[cfg(test)]
mod tests;
