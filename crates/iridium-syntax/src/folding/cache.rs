//! Fold regions maintained across edits instead of recomputed from scratch.
//!
//! # Why this exists
//!
//! Walking a whole tree to find folds costs O(nodes). On a 100,000-line JSON
//! document that is millions of nodes and, measured, over half a second — per
//! typed character, to produce a single fold region. The budget for a keystroke
//! is 8 ms.
//!
//! # What it does
//!
//! It keeps every raw detection, each tagged with the byte extent of the node
//! that produced it. On an edit it is handed three things: the new tree, the
//! [`InputEdit`] that was applied, and the byte ranges tree-sitter reports as
//! structurally changed. From those it
//!
//! 1. throws away every retained region whose node the edit ran through,
//! 2. shifts the rest — the ones before the edit do not move, the ones after it
//!    move by the edit's byte and row deltas, which is precisely what
//!    `Tree::edit` does to the tree's own nodes,
//! 3. throws away anything the shift left touching the dirty span,
//! 4. re-walks only that span, pruning any subtree that misses it entirely, and
//! 5. merges the two sorted halves back into one pre-ordered list.
//!
//! # Why the result is the same list
//!
//! Tree-sitter's contract for [`tree_sitter::Tree::changed_ranges`] is that
//! outside those ranges the old edited tree and the new tree agree — same nodes,
//! same kinds, same positions. So every foldable node of the new tree either
//! touches the dirty span, in which case step 4 finds it, or does not, in which
//! case it also existed in the old tree and step 2 carried its region across
//! unchanged. The two cases are decided by one predicate used in both
//! directions, so they neither overlap nor leave a gap.
//!
//! That is an argument, not a proof, and it rests on a contract this crate does
//! not own. `cache_matches_a_full_recompute_across_long_edit_sequences` is the
//! check: it drives real edits through real grammars and compares against the
//! full walk after every single one.

use std::ops::Range;

use tree_sitter::{InputEdit, Tree};

use super::{FoldDetector, FoldRegion, TrackedRegion, publish};
use crate::Language;

/// Fold regions for one document, maintained incrementally.
#[derive(Debug)]
pub struct FoldCache {
    /// The rules, and the language they belong to.
    detector: FoldDetector,
    /// Every raw detection, in the tree's pre-order.
    tracked: Vec<TrackedRegion>,
    /// The published list: sorted, de-duplicated, import runs merged.
    regions: Vec<FoldRegion>,
    /// Whether a tree has ever been walked into this cache.
    ///
    /// An empty region list cannot answer that on its own — a document with no
    /// folds has one too — and the difference matters: an incremental update
    /// against a cache that never saw a tree would publish only whatever the
    /// changed ranges happened to contain.
    primed: bool,
    /// How many tree nodes have been looked at, over the cache's whole life.
    nodes_visited: u64,
}

impl FoldCache {
    /// Creates an empty cache for `language`.
    ///
    /// Empty is the honest starting point: no tree has been seen, so no region
    /// has been detected. The first [`FoldCache::rebuild`] fills it.
    #[must_use]
    pub const fn new(language: Language) -> Self {
        Self {
            detector: FoldDetector::new(language),
            tracked: Vec::new(),
            regions: Vec::new(),
            primed: false,
            nodes_visited: 0,
        }
    }

    /// Whether this cache has ever been given a tree to work from.
    ///
    /// False until the first [`FoldCache::rebuild`]. A caller deciding whether
    /// it can skip a refresh must consult this: "nothing changed since last
    /// time" says nothing useful when there was no last time.
    #[must_use]
    pub const fn is_primed(&self) -> bool {
        self.primed
    }

    /// Returns the language this cache detects folds for.
    #[must_use]
    pub const fn language(&self) -> Language {
        self.detector.language()
    }

    /// The regions currently held: the complete, sorted, whole-document list.
    #[must_use]
    pub fn regions(&self) -> &[FoldRegion] {
        &self.regions
    }

    /// How many tree nodes this cache has examined since it was created.
    ///
    /// Exposed for the same reason [`crate::SyntaxTree`]'s parse counters are:
    /// "an edit does not walk the document" is a claim about how much work
    /// happens, and a claim nothing can observe is a claim nothing can hold to.
    /// Wall-clock timing would measure the machine; this measures the
    /// algorithm.
    #[must_use]
    pub const fn nodes_visited(&self) -> u64 {
        self.nodes_visited
    }

    /// Recomputes every region by walking `tree` whole.
    ///
    /// Returns true if the published regions changed. This is the right entry
    /// point whenever the tree's relationship to the previous one is unknown: a
    /// file load, a language change, a parse that could not reuse the old tree.
    pub fn rebuild(&mut self, tree: &Tree) -> bool {
        let mut tracked = Vec::new();
        self.nodes_visited += self.detector.collect_all(tree, &mut tracked);
        self.tracked = tracked;
        self.primed = true;
        self.republish()
    }

    /// Updates the regions for one edit, without walking the whole tree.
    ///
    /// `tree` is the reparsed tree, `edit` is the edit that was applied to the
    /// previous one, and `changed` is
    /// [`crate::SyntaxTree::changed_ranges`] between them. Returns true if the
    /// published regions changed.
    ///
    /// Callers must not pass an `edit` that was never applied to the tree the
    /// retained regions came from: the shift below assumes exactly the mapping
    /// `Tree::edit` performs, and an unrelated edit would move regions to
    /// positions no node occupies. When in doubt, [`FoldCache::rebuild`] is
    /// always correct.
    pub fn update(&mut self, tree: &Tree, edit: &InputEdit, changed: &[Range<usize>]) -> bool {
        if !self.primed {
            // There is nothing to carry across. Retaining only what the changed
            // ranges happen to cover would publish a fraction of the document's
            // folds and call it the whole list.
            return self.rebuild(tree);
        }

        let dirty = dirty_span(edit, changed);

        let previous = std::mem::take(&mut self.tracked);
        let mut kept = Vec::with_capacity(previous.len());
        for tracked in previous {
            match shift(tracked, edit) {
                Shifted::Moved(shifted) => {
                    if !shifted.touches(&dirty) {
                        kept.push(shifted);
                    }
                },
                Shifted::Dropped => {},
                // No arithmetic here can describe where this region went, so
                // nothing retained can be trusted. Walking the tree again is
                // always right, and this is the one case where it is the only
                // thing that is — silently dropping the region would publish a
                // fold list missing a fold nothing would ever put back.
                Shifted::Unrepresentable => return self.rebuild(tree),
            }
        }

        let mut fresh = Vec::new();
        self.nodes_visited += self.detector.collect_scoped(tree, &dirty, &mut fresh);

        self.tracked = merge_ordered(&kept, &fresh);
        self.republish()
    }

    /// Rebuilds the published list from the raw one, reporting any change.
    fn republish(&mut self) -> bool {
        let regions = publish(&self.tracked);
        if regions == self.regions {
            return false;
        }
        self.regions = regions;
        true
    }
}

/// The one byte span that must be looked at again, in the new tree's
/// coordinates.
///
/// The edit's own span is included alongside what tree-sitter reports, and it
/// has to be. `changed_ranges` compares *structure*: replacing an identifier
/// with a different one of the same length leaves both trees identical and the
/// reported set empty. The nodes around the edit still need re-reading, because
/// the retained regions covering that span were thrown away.
///
/// Everything is then collapsed into a single enclosing span rather than kept as
/// a list. That is deliberate, and it is a trade. Widening the dirty set only
/// ever moves work from the retained half to the re-walked half — the same span
/// decides both, so the two stay exactly complementary and the answer stays
/// identical — but it does mean two changed ranges far apart drag the span
/// between them along. Tree-sitter reports ranges that far apart when the reparse
/// really did restructure that much, which is precisely when the walk is owed;
/// and the alternative, threading a range list down the walk, buys nothing for a
/// keystroke, whose reported ranges sit within a few bytes of each other.
fn dirty_span(edit: &InputEdit, changed: &[Range<usize>]) -> Range<usize> {
    let mut span = edit.start_byte..edit.new_end_byte.max(edit.start_byte);
    for range in changed {
        span.start = span.start.min(range.start);
        span.end = span.end.max(range.end);
    }
    span
}

/// What became of one retained region when the edit was applied to it.
enum Shifted {
    /// It survived, at the position given.
    Moved(TrackedRegion),
    /// The edit ran through the node it came from. The re-walk covers exactly
    /// that span, so the region is found again there rather than guessed at.
    Dropped,
    /// Where it went cannot be expressed in the machine's integers.
    ///
    /// Unreachable for any document that fits in memory — it needs offsets past
    /// half the address space — and enumerated rather than assumed away, because
    /// the alternative to naming it is dropping a fold and never noticing.
    Unrepresentable,
}

/// Moves a retained region into the post-edit coordinate space.
///
/// Three cases, and they are exhaustive. A node ending strictly before the edit
/// begins is untouched. A node beginning strictly after the span the edit
/// replaced moves by the edit's byte and row deltas, which is the same
/// arithmetic `Tree::edit` applies to the tree's own nodes. Anything else shares
/// a byte with what the edit replaced: it may not survive the reparse at all,
/// and where it does its extent is decided by text that did not exist before, so
/// it is dropped for the re-walk to find.
fn shift(tracked: TrackedRegion, edit: &InputEdit) -> Shifted {
    if tracked.end_byte < edit.start_byte {
        return Shifted::Moved(tracked);
    }
    if tracked.start_byte <= edit.old_end_byte {
        return Shifted::Dropped;
    }

    let (Some(byte_delta), Some(row_delta)) = (
        signed_delta(edit.new_end_byte, edit.old_end_byte),
        signed_delta(edit.new_end_position.row, edit.old_end_position.row),
    ) else {
        return Shifted::Unrepresentable;
    };

    let (Some(start_byte), Some(end_byte), Some(start_line), Some(end_line)) = (
        tracked.start_byte.checked_add_signed(byte_delta),
        tracked.end_byte.checked_add_signed(byte_delta),
        tracked.region.start_line.checked_add_signed(row_delta),
        tracked.region.end_line.checked_add_signed(row_delta),
    ) else {
        return Shifted::Unrepresentable;
    };

    Shifted::Moved(TrackedRegion {
        start_byte,
        end_byte,
        region: FoldRegion {
            start_line,
            end_line,
            ..tracked.region
        },
    })
}

/// `new - old` as a signed offset, or `None` if it does not fit.
fn signed_delta(new: usize, old: usize) -> Option<isize> {
    let new = isize::try_from(new).ok()?;
    let old = isize::try_from(old).ok()?;
    new.checked_sub(old)
}

/// Merges two ordered halves into one ordered list.
///
/// Both inputs are sorted by [`TrackedRegion::order_key`] and describe disjoint
/// sets of nodes, so a straight merge reproduces exactly the order a single
/// sorted walk of the whole tree would have produced. That is what keeps the
/// incremental result byte-for-byte equal to a full recompute, because
/// [`publish`] resolves duplicate line spans by input order.
fn merge_ordered(left: &[TrackedRegion], right: &[TrackedRegion]) -> Vec<TrackedRegion> {
    if right.is_empty() {
        return left.to_vec();
    }
    if left.is_empty() {
        return right.to_vec();
    }

    let mut out = Vec::with_capacity(left.len() + right.len());
    let mut i = 0;
    let mut j = 0;
    while i < left.len() && j < right.len() {
        if left[i].order_key() <= right[j].order_key() {
            out.push(left[i].clone());
            i += 1;
        } else {
            out.push(right[j].clone());
            j += 1;
        }
    }
    out.extend_from_slice(&left[i..]);
    out.extend_from_slice(&right[j..]);
    out
}
