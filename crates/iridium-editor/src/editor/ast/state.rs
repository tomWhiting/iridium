//! The editor's retained syntax tree, and the rules for keeping it honest.
//!
//! One document, one tree. Before this, fold detection and highlighting each
//! parsed independently and the editor's own copy was refreshed only on load —
//! so folds were correct until the first keystroke and wrong from then on.
//!
//! # The contract
//!
//! Every text mutation must be reported through [`SyntaxState::note_edit`],
//! which shifts the retained tree's positions in O(tree depth) and parses
//! nothing. [`SyntaxState::sync`] is what parses, and only when something
//! actually needs the tree.
//!
//! Correctness does not depend on that reporting. `sync` compares the document
//! revision it last saw with the one it is given: if a mutation slipped past
//! `note_edit`, the revisions disagree and it parses the document whole. The
//! incremental path is an optimisation that can be missed; it is never the only
//! thing standing between the tree and the truth.

#[cfg(not(feature = "syntax"))]
use crate::syntax_stubs::{Language, SyntaxTree, Tree};
#[cfg(feature = "syntax")]
use iridium_syntax::{Language, SyntaxTree, Tree};

use super::ExpandStack;
use super::expand::selection_for;
use crate::document::{CursorState, Document, EditSpan, byte_point};
use crate::input::keyboard::AstRequest;

/// The parse tree for the document being edited, and how current it is.
#[derive(Debug, Default)]
pub struct SyntaxState {
    /// The parser and its tree, once a language has been set.
    tree: Option<SyntaxTree>,
    /// The language the tree is parsed with.
    language: Option<Language>,
    /// The document revision the tree has been *told about*, whether or not it
    /// has been reparsed since.
    ///
    /// A disagreement between this and the document's own revision is the
    /// signal that an edit was never reported, and the reason a missed
    /// `note_edit` degrades to a full parse instead of to a wrong tree.
    tracked_revision: u64,
    /// True when edits have been applied to the tree but no parse has run.
    dirty: bool,
    /// True when the document was replaced wholesale rather than edited.
    ///
    /// A revision comparison cannot catch that on its own: a replacement
    /// document starts its count again, so a tree parsed from the old text at
    /// revision 3 and a new document at revision 3 look identical. This says
    /// outright that the two are unrelated.
    replaced: bool,
    /// How many times the document has been parsed whole.
    full_parses: u64,
    /// How many times the tree has been reparsed against an edited copy.
    incremental_parses: u64,
    /// The selections an expansion walked out through, so a shrink can retrace
    /// them.
    ///
    /// Syntax state rather than keyboard state: it describes where in the *tree*
    /// the selection came from, and it must survive a keymap being pushed or
    /// popped underneath it.
    expand: ExpandStack,
}

impl SyntaxState {
    /// Creates a syntax state with no language and no tree.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the language the tree is parsed with, if any.
    #[must_use]
    pub const fn language(&self) -> Option<Language> {
        self.language
    }

    /// Sets the language, discarding any tree parsed with the previous one.
    ///
    /// A tree is only as meaningful as the grammar that produced it, so there
    /// is nothing here worth carrying across a language change.
    pub fn set_language(&mut self, language: Language) {
        self.language = Some(language);
        self.tree = SyntaxTree::new(language).ok();
        self.dirty = false;
        self.replaced = false;
        self.expand.clear();
        // Nothing has been parsed for this language yet. Zero cannot collide
        // with a live document revision, which starts above it and only rises.
        self.tracked_revision = 0;
    }

    /// How many times this state has parsed the document whole.
    ///
    /// Exposed because "typing never parses" and "an edit reparses
    /// incrementally, not wholly" are claims about *how often* work happens,
    /// and a claim nothing can observe is a claim nothing can hold to.
    #[must_use]
    pub const fn full_parses(&self) -> u64 {
        self.full_parses
    }

    /// How many times this state has reparsed against an edited tree.
    #[must_use]
    pub const fn incremental_parses(&self) -> u64 {
        self.incremental_parses
    }

    /// Marks the tree as describing a document that no longer exists.
    ///
    /// Call this whenever the document is replaced rather than edited. The next
    /// [`SyntaxState::sync`] then parses from scratch instead of trusting a
    /// revision number that started over.
    pub fn invalidate(&mut self) {
        self.replaced = true;
        self.expand.clear();
    }

    /// Clears the language and drops the tree.
    pub fn clear_language(&mut self) {
        self.language = None;
        self.tree = None;
        self.dirty = false;
        self.replaced = false;
        self.expand.clear();
        self.tracked_revision = 0;
    }

    /// Returns the retained tree without parsing.
    ///
    /// The tree may be stale — it reflects whatever the last [`SyntaxState::sync`]
    /// produced, shifted by any edits reported since. Callers that need an
    /// accurate tree must call `sync` first.
    #[must_use]
    pub fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref().and_then(SyntaxTree::tree)
    }

    /// Reports an edit against the tree, without parsing.
    ///
    /// `document` must be the document **after** the edit, and `span` must have
    /// been computed against it **before** — that is what
    /// [`crate::document::compute_edit_span`] returns, and why [`EditSpan`]
    /// carries the old end position rather than deriving it later from text
    /// that no longer contains it.
    ///
    /// If either position cannot be resolved, the edit is left unreported: the
    /// revision then disagrees at the next `sync`, which parses the document
    /// whole. Silently shifting the tree by a guessed amount is the one outcome
    /// this must never produce.
    pub fn note_edit(&mut self, document: &Document, span: &EditSpan) {
        // Unconditionally, and before anything can return early: the recorded
        // frames describe ranges in a document that no longer exists. The lazy
        // check inside the stack would catch this too, but only at the next
        // request — which would leave `expansion_depth` reporting frames that
        // are already dead.
        self.expand.clear();

        let Some(tree) = self.tree.as_mut() else {
            return;
        };

        let Some((start_row, start_column)) = byte_point(document, span.start_byte) else {
            return;
        };
        let Some((new_end_row, new_end_column)) = byte_point(document, span.new_end_byte) else {
            return;
        };

        tree.edit(&edit_for(
            span,
            (start_row, start_column),
            (new_end_row, new_end_column),
        ));
        self.tracked_revision = document.revision();
        self.dirty = true;
    }

    /// Brings the tree up to date with `document` and returns it.
    ///
    /// Parses only when something has changed: an incremental reparse when
    /// every edit was reported, a full parse when one was missed or when there
    /// is no tree yet, and nothing at all when the document has not moved since
    /// the last call.
    ///
    /// Returns `None` when no language is set, which is not an error — it means
    /// there is no structure to reason about and callers should do nothing.
    pub fn sync(&mut self, document: &Document) -> Option<&Tree> {
        let revision = document.revision();
        let stale = self.replaced || self.tracked_revision != revision;
        let tree = self.tree.as_mut()?;

        if stale || tree.tree().is_none() {
            // Either an edit never reached `note_edit` or there is no tree to
            // build on. Both are answered the same way, and the answer is
            // always right.
            tree.parse(&document.text());
            self.full_parses += 1;
        } else if self.dirty {
            tree.reparse(&document.text());
            self.incremental_parses += 1;
        }

        self.tracked_revision = revision;
        self.dirty = false;
        self.replaced = false;
        tree.tree()
    }
}

impl SyntaxState {
    /// Computes the cursor state a structural request should produce.
    ///
    /// Brings the tree up to date first, because a structural verb is exactly
    /// the moment the tree has to be right — this is the "something needs the
    /// tree" that [`SyntaxState::sync`] exists to serve, and the only reason
    /// typing itself never parses.
    ///
    /// Returns `None` when nothing should change: no language, no tree, a
    /// selection already at the outermost node, a shrink with nothing left. The
    /// caller applies a selection command only when there is one to apply, so a
    /// held key neither errors nor fills the undo history with no-ops.
    pub fn apply_ast_request(
        &mut self,
        document: &Document,
        cursor: &CursorState,
        request: AstRequest,
    ) -> Option<CursorState> {
        self.sync(document)?;

        // Copied out before the tree is borrowed. The named-region verbs need it
        // — the vendored queries are per-language files — and this field already
        // holds it, so nothing new has to be stored to answer them.
        let language = self.language;

        // Disjoint field borrows: the tree is read while the stack is written.
        let tree = self.tree.as_ref()?.tree()?;
        selection_for(request, language, tree, document, cursor, &mut self.expand)
    }

    /// How many expansions the current stack can still undo.
    #[must_use]
    pub fn expansion_depth(&self) -> usize {
        self.expand.depth()
    }
}

/// Builds the tree-sitter edit descriptor for a span.
///
/// Split out so the coordinate juggling is in one place: the start position is
/// valid in both documents because everything before it is untouched, the old
/// end comes from the span (pre-edit), and the new end from the caller's
/// measurement of the document as it is now.
#[cfg(feature = "syntax")]
const fn edit_for(
    span: &EditSpan,
    start: (usize, usize),
    new_end: (usize, usize),
) -> iridium_syntax::InputEdit {
    use iridium_syntax::{InputEdit, Point};

    InputEdit {
        start_byte: span.start_byte,
        old_end_byte: span.old_end_byte,
        new_end_byte: span.new_end_byte,
        start_position: Point {
            row: start.0,
            column: start.1,
        },
        old_end_position: Point {
            row: span.old_end_row,
            column: span.old_end_column,
        },
        new_end_position: Point {
            row: new_end.0,
            column: new_end.1,
        },
    }
}

/// Builds the stub edit descriptor, which carries nothing.
#[cfg(not(feature = "syntax"))]
const fn edit_for(
    _span: &EditSpan,
    _start: (usize, usize),
    _new_end: (usize, usize),
) -> crate::syntax_stubs::InputEdit {
    crate::syntax_stubs::InputEdit
}

// The tests exercise real parsing against real grammars; with the stub they
// would assert that nothing does nothing.
#[cfg(test)]
#[cfg(feature = "syntax")]
mod tests;
