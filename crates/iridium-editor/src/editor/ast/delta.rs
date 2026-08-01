//! What a [`super::SyntaxState::sync`] did to the retained tree.
//!
//! Fold detection is the first consumer, and the reason this exists: recomputing
//! folds over a whole tree costs O(nodes), which on a large file is the single
//! most expensive thing a keystroke does. Knowing *what moved* turns that into
//! work proportional to the edit. Nothing else can tell folds that — the tree
//! itself has no memory of the tree before it.

#[cfg(not(feature = "syntax"))]
use crate::syntax_stubs::InputEdit;
#[cfg(feature = "syntax")]
use iridium_syntax::InputEdit;

use std::ops::Range;

/// How the parse tree changed since the last time a consumer looked.
///
/// Deliberately conservative: every case that is not *certainly* a single
/// well-described edit reports [`SyntaxDelta::Full`], because a consumer that
/// recomputes everything is merely slow, and one that trusts a wrong
/// description is wrong.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SyntaxDelta {
    /// The tree has not moved. Anything derived from it is still valid.
    #[default]
    Unchanged,

    /// The tree bears no usable relationship to the previous one.
    ///
    /// A first parse, a language change, a wholesale content replacement, an
    /// edit that never reached `note_edit` — and any case where two or more
    /// edits were folded into one reparse, because describing their combined
    /// effect on a coordinate space that moved twice is not worth getting
    /// subtly wrong.
    Full,

    /// One edit was applied and the tree was reparsed against it.
    Incremental {
        /// The edit, in the coordinates it was applied in.
        edit: InputEdit,
        /// Byte ranges of the new tree whose structure differs from the old.
        changed: Vec<Range<usize>>,
    },
}

impl SyntaxDelta {
    /// Combines two deltas observed in order, `self` then `next`.
    ///
    /// Exists because `sync` is called from more places than fold refreshing —
    /// a structural navigation verb syncs too — and the description of what
    /// changed must survive until whoever needs it looks. Anything that is not
    /// a single edit against a known tree collapses to [`SyntaxDelta::Full`],
    /// which every consumer can always answer correctly.
    #[must_use]
    pub fn then(self, next: Self) -> Self {
        match (self, next) {
            (Self::Unchanged, other) | (other, Self::Unchanged) => other,
            _ => Self::Full,
        }
    }
}
