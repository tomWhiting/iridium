//! What has happened to the retained tree since a consumer last looked.
//!
//! Fold detection is the first consumer, and the reason this exists: recomputing
//! folds over a whole tree costs O(nodes), which on a large file is the single
//! most expensive thing a keystroke does. Knowing *what moved* turns that into
//! work proportional to the edit. Nothing else can tell folds that — the tree
//! itself has no memory of the tree before it.
//!
//! # Two kinds of movement
//!
//! A tree moves in two ways. An **edit** shifts it: `Tree::edit` carries every
//! node past the edit by the edit's length, and nothing is parsed. A
//! **reparse** restructures it: the text is read again and some ranges come
//! back with different nodes. When parsing runs on the editing thread the two
//! arrive together; when it runs in the background a keystroke delivers the
//! edit at once and the reparse whenever it lands, possibly after several more
//! edits. [`SyntaxDelta::Incremental`] carries both, in a form that composes:
//! the edits in the order they happened, and the restructured ranges carried
//! forward through every later edit into the coordinates of the tree as it now
//! stands.

#[cfg(not(feature = "syntax"))]
use crate::syntax_stubs::InputEdit;
#[cfg(feature = "syntax")]
use iridium_syntax::InputEdit;

use std::ops::Range;

/// How the parse tree changed since the last time a consumer looked.
///
/// Deliberately conservative: anything that is not *certainly* described by a
/// list of edits and restructured ranges reports [`SyntaxDelta::Full`],
/// because a consumer that recomputes everything is merely slow, and one that
/// trusts a wrong description is wrong.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SyntaxDelta {
    /// The tree has not moved. Anything derived from it is still valid.
    #[default]
    Unchanged,

    /// The tree bears no usable relationship to the previous one.
    ///
    /// A first parse, a language change, a wholesale content replacement, an
    /// edit that never reached `note_edit`.
    Full,

    /// The tree was edited, reparsed, or both, and this says exactly how.
    Incremental {
        /// Every edit applied to the tree, in order, each in the coordinates
        /// of the document as it stood when that edit was made. Empty when the
        /// tree was only reparsed.
        edits: Vec<InputEdit>,
        /// Byte ranges of the tree as it now stands whose structure a reparse
        /// changed. Empty when the tree was only edited — shifted, with no new
        /// nodes anywhere.
        changed: Vec<Range<usize>>,
    },
}

impl SyntaxDelta {
    /// A tree shifted by one edit and not reparsed.
    #[must_use]
    pub fn edited(edit: InputEdit) -> Self {
        Self::Incremental {
            edits: vec![edit],
            changed: Vec::new(),
        }
    }

    /// A tree reparsed against the edits it had already been given, with
    /// `changed` the ranges tree-sitter reports as restructured.
    #[must_use]
    pub const fn reparsed(changed: Vec<Range<usize>>) -> Self {
        Self::Incremental {
            edits: Vec::new(),
            changed,
        }
    }

    /// Combines two deltas observed in order, `self` then `next`.
    ///
    /// Exists because `sync` is called from more places than fold refreshing —
    /// a structural navigation verb syncs too — and the description of what
    /// changed must survive until whoever needs it looks.
    ///
    /// Two incremental deltas compose exactly: the edit lists are concatenated,
    /// and `self`'s changed ranges are carried through `next`'s edits so that
    /// every range is expressed against the tree as it stands after both.
    #[must_use]
    pub fn then(self, next: Self) -> Self {
        match (self, next) {
            (Self::Unchanged, other) | (other, Self::Unchanged) => other,
            (
                Self::Incremental {
                    mut edits,
                    changed: earlier,
                },
                Self::Incremental {
                    edits: later_edits,
                    changed: later,
                },
            ) => {
                let mut changed: Vec<Range<usize>> = earlier
                    .into_iter()
                    .map(|range| {
                        later_edits
                            .iter()
                            .fold(range, |range, edit| carry(&range, edit))
                    })
                    .collect();
                changed.extend(later);
                edits.extend(later_edits);
                Self::Incremental { edits, changed }
            },
            _ => Self::Full,
        }
    }
}

/// Carries a byte range across an edit into the post-edit coordinates.
///
/// Positions before the edit stay, positions after what it replaced move by its
/// length change, and a position inside what it replaced widens to the edit's
/// new extent — the range then covers everything the edit could have put there.
/// The same rule the fold cache uses for the same purpose, so a range carried
/// here and a region carried there agree about where things went.
fn carry(range: &Range<usize>, edit: &InputEdit) -> Range<usize> {
    let start = if range.start < edit.start_byte {
        range.start
    } else if range.start >= edit.old_end_byte {
        (range.start - edit.old_end_byte).saturating_add(edit.new_end_byte)
    } else {
        edit.start_byte
    };
    let end = if range.end < edit.start_byte {
        range.end
    } else if range.end >= edit.old_end_byte {
        (range.end - edit.old_end_byte).saturating_add(edit.new_end_byte)
    } else {
        edit.new_end_byte
    };
    start..end.max(start)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(feature = "syntax"))]
    use crate::syntax_stubs::Point;
    #[cfg(feature = "syntax")]
    use iridium_syntax::Point;

    /// An edit replacing `old` bytes at `start` with `new` bytes, all on one
    /// row — positions do not matter to the range arithmetic.
    fn edit(start: usize, old: usize, new: usize) -> InputEdit {
        InputEdit {
            start_byte: start,
            old_end_byte: start + old,
            new_end_byte: start + new,
            start_position: Point::default(),
            old_end_position: Point::default(),
            new_end_position: Point::default(),
        }
    }

    #[test]
    fn unchanged_is_the_identity_on_both_sides() {
        let delta = SyntaxDelta::edited(edit(3, 0, 1));
        assert_eq!(SyntaxDelta::Unchanged.then(delta.clone()), delta);
        assert_eq!(delta.clone().then(SyntaxDelta::Unchanged), delta);
    }

    #[test]
    fn full_absorbs_everything() {
        let delta = SyntaxDelta::edited(edit(3, 0, 1));
        assert_eq!(SyntaxDelta::Full.then(delta.clone()), SyntaxDelta::Full);
        assert_eq!(delta.then(SyntaxDelta::Full), SyntaxDelta::Full);
    }

    #[test]
    fn edits_concatenate_in_order() {
        let first = edit(10, 0, 2);
        let second = edit(4, 3, 0);
        let composed = SyntaxDelta::edited(first).then(SyntaxDelta::edited(second));
        assert_eq!(
            composed,
            SyntaxDelta::Incremental {
                edits: vec![first, second],
                changed: Vec::new(),
            }
        );
    }

    #[test]
    fn earlier_changed_ranges_follow_later_edits() {
        // A reparse restructured 20..30; then 5 bytes were inserted at 0 and
        // the range moves with them, while a later reparse's own range is
        // already in final coordinates and stays put.
        let composed = SyntaxDelta::reparsed(std::iter::once(20..30).collect())
            .then(SyntaxDelta::edited(edit(0, 0, 5)))
            .then(SyntaxDelta::reparsed(std::iter::once(1..2).collect()));
        assert_eq!(
            composed,
            SyntaxDelta::Incremental {
                edits: vec![edit(0, 0, 5)],
                changed: vec![25..35, 1..2],
            }
        );
    }

    #[test]
    fn a_range_an_edit_runs_through_widens_to_cover_it() {
        // Deleting 10 bytes at 15 swallows the end of 10..20 and the start of
        // 22..40; both keep the deletion point inside them.
        let composed = SyntaxDelta::reparsed(vec![10..20, 22..40, 0..3])
            .then(SyntaxDelta::edited(edit(15, 10, 0)));
        let SyntaxDelta::Incremental { changed, .. } = composed else {
            panic!("two incremental deltas compose incrementally");
        };
        assert_eq!(changed, vec![10..15, 15..30, 0..3]);
    }
}
