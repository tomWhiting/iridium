//! One retained parse tree, and the parser that maintains it.
//!
//! Before this module, highlighting and fold detection each owned a `Parser`
//! and a `Tree` and each parsed the same document independently. That is twice
//! the work on every keystroke, twice the memory for a large file, and — worse
//! than either — two trees that can disagree, because only one of them is
//! updated when a caller remembers to update only one.
//!
//! [`SyntaxTree`] is the single owner. [`crate::Highlighter`] and
//! [`crate::FoldDetector`] borrow it: they hold the rules for their own job and
//! read a tree they did not parse, so there is nothing left for them to keep in
//! sync.
//!
//! # Editing without parsing
//!
//! [`SyntaxTree::edit`] shifts the retained tree's node positions to account for
//! an edit. It costs O(tree depth) and does not parse, which is what makes it
//! safe to call on every keystroke. The tree is *stale but positioned* until
//! [`SyntaxTree::reparse`] runs, and reparsing then reuses every subtree the
//! edit did not touch.
//!
//! Callers must not skip the `edit` and reparse against a stale tree: passing
//! an unedited tree as the reference produces a parse that silently disagrees
//! with the document. When in doubt, [`SyntaxTree::parse`] discards the old
//! tree and starts over, which is always correct and merely slower.

use tree_sitter::{InputEdit, Node, Parser, Point, Tree};

use crate::grammar::grammar;
use crate::{Language, SyntaxError};

/// A parser and the tree it most recently produced, for one document.
pub struct SyntaxTree {
    language: Language,
    parser: Parser,
    tree: Option<Tree>,
}

impl std::fmt::Debug for SyntaxTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyntaxTree")
            .field("language", &self.language)
            .field("has_tree", &self.tree.is_some())
            .finish_non_exhaustive()
    }
}

impl SyntaxTree {
    /// Creates a parser for the given language, with no tree yet.
    ///
    /// # Errors
    ///
    /// [`SyntaxError::UnsupportedLanguage`] if no grammar is linked for the
    /// language. Since the language registry became data, that is a routine
    /// state rather than a broken one — `diff`, `go.mod` and a git commit
    /// message are supported languages with manifests and queries and no
    /// parser — so a caller that merely wants to *know* should ask
    /// [`crate::has_grammar`] rather than construct one of these and discard
    /// the error.
    ///
    /// [`SyntaxError::ParseError`] if a grammar is linked but is incompatible
    /// with this build of tree-sitter. That means a dependency moved, not that
    /// the language is unknown, and the two are worth telling apart.
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

        Ok(Self {
            language,
            parser,
            tree: None,
        })
    }

    /// Returns the language this tree is parsed with.
    #[must_use]
    pub const fn language(&self) -> Language {
        self.language
    }

    /// Returns the retained tree, if there is one.
    ///
    /// `None` before the first parse, and after a parse that failed. A caller
    /// that gets `None` has no structure to work with and should do nothing
    /// rather than guess.
    #[must_use]
    pub const fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }

    /// Returns the root node of the retained tree, if there is one.
    #[must_use]
    pub fn root(&self) -> Option<Node<'_>> {
        self.tree.as_ref().map(Tree::root_node)
    }

    /// Parses `source` from scratch, discarding any retained tree.
    ///
    /// Always correct, and the right choice whenever the retained tree's
    /// relationship to the document is in any doubt.
    pub fn parse(&mut self, source: &str) -> Option<&Tree> {
        self.tree = self.parser.parse(source, None);
        self.tree.as_ref()
    }

    /// Shifts the retained tree to account for an edit, without parsing.
    ///
    /// Does nothing when there is no retained tree, which is correct: there is
    /// nothing to shift, and the next [`SyntaxTree::reparse`] will parse the
    /// document whole.
    ///
    /// `edit`'s old-end position must be in the **pre-edit** document's
    /// coordinates and its new-end position in the post-edit document's. Those
    /// two documents differ, so a caller that derives both from the text it has
    /// after the edit will mis-position every multi-line change.
    pub fn edit(&mut self, edit: &InputEdit) {
        if let Some(tree) = self.tree.as_mut() {
            tree.edit(edit);
        }
    }

    /// Parses `source`, reusing the retained tree where the edit did not reach.
    ///
    /// Falls back to a full parse when there is no retained tree. This is only
    /// sound if every edit since the last parse was passed to
    /// [`SyntaxTree::edit`]; otherwise the reference tree describes text that
    /// no longer exists and the result will not match the document.
    pub fn reparse(&mut self, source: &str) -> Option<&Tree> {
        let old = self.tree.take();
        self.tree = self.parser.parse(source, old.as_ref());
        self.tree.as_ref()
    }

    /// Applies an edit expressed in byte offsets, then reparses.
    ///
    /// The convenience form for callers that know only where the edit landed in
    /// the text they now hold. Its one compromise is that the old-end *point*
    /// is derived from the post-edit `source`, because that is the only text
    /// the caller passed; for a single-line edit that is exact, and for a
    /// multi-line one it can be off. A caller that knows the pre-edit position
    /// should build the [`InputEdit`] itself and call [`SyntaxTree::edit`] and
    /// [`SyntaxTree::reparse`] instead.
    pub fn edit_bytes(
        &mut self,
        source: &str,
        start_byte: usize,
        old_end_byte: usize,
        new_end_byte: usize,
    ) -> Option<&Tree> {
        if self.tree.is_none() {
            return self.parse(source);
        }

        let edit = InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position: byte_point(source, start_byte),
            old_end_position: byte_point(source, old_end_byte),
            new_end_position: byte_point(source, new_end_byte),
        };

        self.edit(&edit);
        self.reparse(source)
    }

    /// Returns the byte ranges that differ between `old` and the retained tree.
    ///
    /// Empty when there is no retained tree — nothing is known to have changed,
    /// which is the safe answer for a renderer deciding what to repaint.
    #[must_use]
    pub fn changed_ranges(&self, old: &Tree) -> Vec<std::ops::Range<usize>> {
        let Some(new) = &self.tree else {
            return Vec::new();
        };

        old.changed_ranges(new)
            .map(|range| range.start_byte..range.end_byte)
            .collect()
    }
}

/// Converts a byte offset into a tree-sitter [`Point`].
///
/// The column is in **bytes**, not characters, because that is what
/// tree-sitter's `Point` means; a character column would misplace every edit on
/// a line holding anything outside ASCII.
///
/// An offset past the end of `source` yields the position of the end, which is
/// the closest true answer available and keeps the caller from having to handle
/// an error it cannot act on.
#[must_use]
pub fn byte_point(source: &str, byte_offset: usize) -> Point {
    let mut row = 0;
    let mut column = 0;
    let mut current = 0;

    for ch in source.chars() {
        if current >= byte_offset {
            break;
        }

        if ch == '\n' {
            row += 1;
            column = 0;
        } else {
            column += ch.len_utf8();
        }

        current += ch.len_utf8();
    }

    Point { row, column }
}

#[cfg(test)]
mod tests;
