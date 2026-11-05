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
//! 5. merges the two sorted halves back into one pre-ordered list, and
//! 6. republishes only the lines the update reached, moving the published
//!    regions after them by the edits' row delta.
//!
//! Steps 1–3 happen in place and step 5 leaves everything before the edit
//! where it is. Step 6 is what keeps a keystroke from costing a pass over every
//! region in the file: publishing sorts, de-duplicates and merges import runs,
//! all of it line-local, so the published list can be patched rather than
//! rebuilt — see `Window` for why the patch equals the rebuild. What is left
//! proportional to the file is moving the regions after the edit, a tight loop
//! of additions over a contiguous array; making even that independent of the
//! file needs regions stored relative to one another, which every reader of
//! the published slice would then have to resolve.
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

use super::{FoldDetector, FoldKind, FoldRegion, TrackedRegion, publish};
use crate::Language;

/// Fold regions for one document, maintained incrementally.
///
/// `Clone` so that a cache maintained on one thread can be handed whole to
/// another: the editor's background parser keeps its own and gives the editing
/// thread a copy with each tree it finishes.
#[derive(Debug, Clone)]
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
    ///
    /// No longer `const`, following [`FoldDetector::new`], which stopped being
    /// one when the fold table began keying on the language identifier. Nothing
    /// built one of these in a constant, and the cost is a handful of string
    /// comparisons once per cache.
    #[must_use]
    pub fn new(language: Language) -> Self {
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
        self.apply(tree, std::slice::from_ref(edit), changed)
    }

    /// Updates the regions for any number of edits, without walking the whole
    /// tree.
    ///
    /// `edits` are every edit applied to the tree the retained regions came
    /// from, in the order they were applied, each in the coordinates of the
    /// document as it stood when it was made. `changed` holds byte ranges of
    /// `tree` — in its own, final coordinates — whose structure differs from
    /// the edited previous tree; empty when `tree` *is* the edited previous
    /// tree, not yet reparsed. Returns true if the published regions changed.
    ///
    /// A cache that has already followed some edits on the unparsed tree, and
    /// is now handed the reparse with no edits, must find the spans those
    /// edits wrote among `changed`, not only tree-sitter's ranges. A node an
    /// edit deleted outright survives in the edited tree, collapsed to nothing
    /// where the deletion was, so a walk of that tree finds it; the reparse
    /// drops it, and tree-sitter reports no range for a node that was never
    /// wider than nothing. Passing the edits themselves, as the editor does,
    /// covers this without further thought.
    ///
    /// With no edits and no changed ranges nothing can have moved, and nothing
    /// is walked. Otherwise every retained region is carried through each edit
    /// in turn, exactly as `Tree::edit` carries the tree's nodes, and one span
    /// is re-walked: the edits' spans, each moved on by the edits after it,
    /// together with `changed`. See [`dirty_span`] for why that one span is
    /// enough.
    pub fn apply(&mut self, tree: &Tree, edits: &[InputEdit], changed: &[Range<usize>]) -> bool {
        if !self.primed {
            // There is nothing to carry across. Retaining only what the changed
            // ranges happen to cover would publish a fraction of the document's
            // folds and call it the whole list.
            return self.rebuild(tree);
        }
        let Some(dirty) = dirty_span(edits, changed) else {
            return false;
        };
        // No arithmetic can describe where a region went if an edit's own
        // deltas do not fit the machine's integers, so nothing retained can be
        // trusted and walking the tree again is the only right answer.
        let Some(moves) = edits.iter().map(Move::of).collect::<Option<Vec<_>>>() else {
            return self.rebuild(tree);
        };
        let Some(total) = Move::total(&moves) else {
            return self.rebuild(tree);
        };

        // The retained list, in order, falls into three runs.
        //
        // - Regions starting before the dirty span. No edit moved their start:
        //   the dirty span begins at or before every edit. Those ending before
        //   it too are untouched and stay; those reaching into it enclose the
        //   edit, and go. Only the enclosing ones need finding, and the first
        //   of them marks where the work begins.
        // - Regions from there to the first one every edit moved past: carried
        //   edit by edit, and kept only if they survive clear of the dirty
        //   span.
        // - Everything from that first moved-past region on. Each starts no
        //   earlier than it, so each was moved past by every edit too, and
        //   lands after the dirty span: moved by the edits' summed deltas and
        //   otherwise left alone.
        let before = self
            .tracked
            .partition_point(|tracked| tracked.start_byte < dirty.start);
        let from = self
            .tracked
            .get(..before)
            .unwrap_or(&[])
            .iter()
            .position(|tracked| tracked.end_byte >= dirty.start)
            .unwrap_or(before);

        // The lines removed regions started on, in the coordinates the
        // published list still uses, bound the part of that list that has to be
        // worked out again.
        let mut removed = Lines::default();
        let mut kept = Vec::new();
        let mut moved_from = self.tracked.len();
        for (index, tracked) in self.tracked.iter().enumerate().skip(from) {
            let line = tracked.region.start_line;
            if index < before {
                if tracked.end_byte < dirty.start {
                    kept.push(tracked.clone());
                } else {
                    removed.include(line);
                }
                continue;
            }
            let mut carried = tracked.clone();
            match Move::carry_all(&moves, &mut carried) {
                Carried::Kept if carried.start_byte > dirty.end => {
                    moved_from = index;
                    break;
                },
                Carried::Kept if !carried.touches(&dirty) => kept.push(carried),
                Carried::Kept | Carried::Dropped => removed.include(line),
                // Unreachable for any document that fits in memory; see
                // `Carried::Unrepresentable`. Silently dropping the region
                // would publish a fold list missing a fold nothing would ever
                // put back.
                Carried::Unrepresentable => return self.rebuild(tree),
            }
        }
        if let Some(after) = self.tracked.get_mut(moved_from..)
            && !shift_run(after, total.bytes, total.rows)
        {
            return self.rebuild(tree);
        }

        // The furthest a region no edit moved reaches into the lines about to
        // be republished: the last one kept from before the dirty span, or
        // failing that the last one before the work began.
        let last_unmoved = kept
            .iter()
            .rev()
            .find(|tracked| tracked.start_byte < dirty.start)
            .or_else(|| {
                from.checked_sub(1)
                    .and_then(|index| self.tracked.get(index))
            })
            .map(|tracked| tracked.region.start_line);

        let mut fresh = Vec::new();
        self.nodes_visited += self.detector.collect_scoped(tree, &dirty, &mut fresh);
        let mut found = Lines::default();
        for tracked in &fresh {
            found.include(tracked.region.start_line);
        }
        self.tracked
            .splice(from..moved_from, merge_ordered(kept, fresh));

        let first_edited_row = edits.iter().map(|edit| edit.start_position.row).min();
        let Some(first) = [first_edited_row, removed.first, found.first]
            .into_iter()
            .flatten()
            .min()
        else {
            // No edit, and nothing retained or found near what tree-sitter
            // reported: no region moved and none appeared or vanished.
            return false;
        };
        match Window::around(
            &self.tracked,
            first,
            total.rows,
            &found,
            &removed,
            last_unmoved,
        ) {
            Some(window) => self.republish_window(&window),
            None => self.republish(),
        }
    }

    /// Rebuilds the published list from the raw one, reporting any change.
    ///
    /// The whole-list path: right after a full walk, and as the fallback when
    /// the window of lines an edit reached cannot be stated in the machine's
    /// integers.
    fn republish(&mut self) -> bool {
        let regions = publish(&self.tracked);
        if regions == self.regions {
            return false;
        }
        self.regions = regions;
        true
    }

    /// Republishes only the lines in `window`, moving everything after it by
    /// the edits' row delta, and reports any change.
    ///
    /// [`Window`] explains why the regions either side of it are already
    /// right: the ones before are untouched, the ones after are untouched but
    /// for that one uniform shift, and nothing [`publish`] does reaches across
    /// either boundary.
    fn republish_window(&mut self, window: &Window) -> bool {
        let raw_from = self
            .tracked
            .partition_point(|tracked| tracked.region.start_line < window.first);
        let raw_to = self
            .tracked
            .partition_point(|tracked| tracked.region.start_line < window.end);
        let middle = publish(self.tracked.get(raw_from..raw_to).unwrap_or(&[]));

        let from = self
            .regions
            .partition_point(|region| region.start_line < window.first);
        let to = self
            .regions
            .partition_point(|region| region.start_line < window.old_end)
            .max(from);
        let rows_moved = window.rows != 0 && to < self.regions.len();
        let changed = rows_moved || self.regions.get(from..to) != Some(middle.as_slice());

        let after = from + middle.len();
        self.regions.splice(from..to, middle);
        if window.rows != 0
            && let Some(moved) = self.regions.get_mut(after..)
            && !shift_lines(moved, window.rows)
        {
            // The raw regions were carried by the same delta without
            // overflowing, so this cannot happen; if it somehow did, the whole
            // list is still recoverable from them.
            self.regions = publish(&self.tracked);
            return true;
        }
        changed
    }
}

/// The run of lines an edit reached, in which the published regions have to be
/// worked out again.
///
/// [`publish`] works in line order: it sorts by start line, collapses regions
/// sharing a start *and* end line, and merges runs of single-line imports on
/// consecutive lines. So the published list splits cleanly at any line
/// boundary that no import run crosses — the regions on either side are
/// whatever publishing that side alone would give. The window is chosen so
/// that
///
/// - every raw region starting before [`Window::first`] is one no edit moved
///   and the update kept, identical before and after;
/// - every raw region starting at or after [`Window::end`] (in the new lines),
///   or at or after [`Window::old_end`] (in the old), is one every edit moved
///   and the update kept, so the two sides are one list shifted by
///   [`Window::rows`];
/// - neither boundary falls inside a run of single-line imports.
///
/// The first two follow from where the update put things. A retained region
/// starting before the first edited row starts before every edit, and if it
/// survived it ended before every edit too. Everything the update removed or
/// found is inside the window by construction. A survivor lying after the
/// dirty span was moved by every edit — anything an edit left in place sits at
/// or before that edit, and so at or before the dirty span, which covers every
/// edit carried to its final position.
struct Window {
    /// First line republished, the same in both coordinate spaces.
    first: usize,
    /// One past the last line republished, in the new lines.
    end: usize,
    /// One past the last line republished, in the old lines.
    old_end: usize,
    /// How far every region after the window moved.
    rows: isize,
}

impl Window {
    /// The window starting no later than `first`, reaching past every region
    /// the update found, removed or left unmoved, and widened until no import
    /// run crosses either end — or `None` if one of its ends cannot be stated
    /// in the machine's integers.
    ///
    /// `tracked` is the updated raw list and `found` the lines it gained, both
    /// in the new lines; `removed` holds the lines of what it lost, in the old
    /// ones; `last_unmoved` is the last line on which a region no edit moved
    /// starts, the same in both.
    fn around(
        tracked: &[TrackedRegion],
        first: usize,
        rows: isize,
        found: &Lines,
        removed: &Lines,
        last_unmoved: Option<usize>,
    ) -> Option<Self> {
        let mut first = first;
        while first > 0 && starts_single_import(tracked, first - 1) {
            first -= 1;
        }

        // Lines only ever grow by this much between the two coordinate spaces;
        // a requirement stated in old lines is met in new ones by adding it.
        let lift = rows.max(0).unsigned_abs();
        // The old end has to land at or after the first line too, so the old
        // range the new one replaces is never inverted.
        let mut end = first.checked_add(lift)?;
        if let Some(line) = found.last {
            end = end.max(line.checked_add(1)?);
        }
        if let Some(line) = removed.last {
            // Where that line sits once the edits are applied. One that a
            // deletion pulls up past the top of the document places no
            // requirement at all, which a floor of zero states.
            let moved = if rows < 0 {
                line.saturating_sub(rows.unsigned_abs())
            } else {
                line.checked_add(lift)?
            };
            end = end.max(moved.checked_add(1)?);
        }
        if let Some(line) = last_unmoved {
            end = end.max(line.checked_add(lift)?.checked_add(1)?);
        }
        while starts_single_import(tracked, end) {
            end = end.checked_add(1)?;
        }

        let old_end = end.checked_add_signed(rows.checked_neg()?)?;
        Some(Self {
            first,
            end,
            old_end,
            rows,
        })
    }
}

/// Whether a single-line import starts on `line` of `tracked`.
///
/// Those are the regions [`publish`] merges with their neighbours on adjacent
/// lines, so a window boundary next to one could split a run that publishing
/// the whole list would have joined.
fn starts_single_import(tracked: &[TrackedRegion], line: usize) -> bool {
    let from = tracked.partition_point(|candidate| candidate.region.start_line < line);
    tracked
        .get(from..)
        .unwrap_or(&[])
        .iter()
        .take_while(|candidate| candidate.region.start_line == line)
        .any(|candidate| {
            candidate.region.kind == FoldKind::Import && candidate.region.end_line == line
        })
}

/// The first and last of a set of lines, as they are noted.
#[derive(Default)]
struct Lines {
    /// The lowest line noted.
    first: Option<usize>,
    /// The highest line noted.
    last: Option<usize>,
}

impl Lines {
    /// Notes one more line.
    fn include(&mut self, line: usize) {
        self.first = Some(self.first.map_or(line, |first| first.min(line)));
        self.last = Some(self.last.map_or(line, |last| last.max(line)));
    }
}

/// The one byte span that must be looked at again, in the final tree's
/// coordinates, or `None` when nothing moved at all.
///
/// Every edit's own span is included alongside what tree-sitter reports, and it
/// has to be. `changed_ranges` compares *structure*: replacing an identifier
/// with a different one of the same length leaves both trees identical and the
/// reported set empty. The nodes around the edit still need re-reading, because
/// the retained regions covering that span were thrown away. An earlier edit's
/// span is carried through each later edit first, the same way a region is, so
/// every span lands in the coordinates `tree` uses.
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
pub(super) fn dirty_span(edits: &[InputEdit], changed: &[Range<usize>]) -> Option<Range<usize>> {
    let mut span: Option<Range<usize>> = None;
    for edit in edits {
        let own = edit.start_byte..edit.new_end_byte.max(edit.start_byte);
        span = Some(match span {
            Some(earlier) => hull(&carry(&earlier, edit), &own),
            None => own,
        });
    }
    for range in changed {
        span = Some(span.map_or_else(|| range.clone(), |earlier| hull(&earlier, range)));
    }
    span
}

/// The smallest range covering both `a` and `b`.
fn hull(a: &Range<usize>, b: &Range<usize>) -> Range<usize> {
    a.start.min(b.start)..a.end.max(b.end)
}

/// Carries a byte range across an edit into the post-edit coordinates.
///
/// Positions before the edit stay, positions after what it replaced move by its
/// length change, and a position inside what it replaced widens to the edit's
/// new extent — the range then covers everything the edit could have put there.
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

/// One edit, reduced to what carrying a retained region across it needs.
struct Move {
    /// First byte the edit replaced.
    start: usize,
    /// One past the last byte it replaced, before the edit.
    old_end: usize,
    /// How far bytes after what it replaced moved.
    bytes: isize,
    /// How far rows after what it replaced moved.
    rows: isize,
}

impl Move {
    /// The move `edit` makes, or `None` if its deltas do not fit an `isize`.
    fn of(edit: &InputEdit) -> Option<Self> {
        Some(Self {
            start: edit.start_byte,
            old_end: edit.old_end_byte,
            bytes: signed_delta(edit.new_end_byte, edit.old_end_byte)?,
            rows: signed_delta(edit.new_end_position.row, edit.old_end_position.row)?,
        })
    }

    /// The move made by applying all of `moves` to a position past every one of
    /// them, or `None` if the sums do not fit an `isize`.
    ///
    /// Only its deltas mean anything; it replaces nothing.
    fn total(moves: &[Self]) -> Option<Self> {
        let mut total = Self {
            start: 0,
            old_end: 0,
            bytes: 0,
            rows: 0,
        };
        for step in moves {
            total.bytes = total.bytes.checked_add(step.bytes)?;
            total.rows = total.rows.checked_add(step.rows)?;
        }
        Some(total)
    }

    /// Carries a retained region through every one of `moves` in turn, in
    /// place, stopping at the first that drops it.
    fn carry_all(moves: &[Self], tracked: &mut TrackedRegion) -> Carried {
        for step in moves {
            match step.carry(tracked) {
                Carried::Kept => {},
                gone => return gone,
            }
        }
        Carried::Kept
    }

    /// Shifts a region known to lie past everything replaced by this move's
    /// deltas, in place.
    const fn carry_past(&self, tracked: &mut TrackedRegion) -> Carried {
        let (Some(start_byte), Some(end_byte), Some(start_line), Some(end_line)) = (
            tracked.start_byte.checked_add_signed(self.bytes),
            tracked.end_byte.checked_add_signed(self.bytes),
            tracked.region.start_line.checked_add_signed(self.rows),
            tracked.region.end_line.checked_add_signed(self.rows),
        ) else {
            return Carried::Unrepresentable;
        };
        tracked.start_byte = start_byte;
        tracked.end_byte = end_byte;
        tracked.region.start_line = start_line;
        tracked.region.end_line = end_line;
        Carried::Kept
    }

    /// Moves a retained region into the post-edit coordinate space, in place.
    ///
    /// Three cases, and they are exhaustive. A node ending strictly before the
    /// edit begins is untouched. A node beginning strictly after the span the
    /// edit replaced moves by the edit's byte and row deltas, which is the same
    /// arithmetic `Tree::edit` applies to the tree's own nodes. Anything else
    /// shares a byte with what the edit replaced: it may not survive the
    /// reparse at all, and where it does its extent is decided by text that did
    /// not exist before, so it is dropped for the re-walk to find.
    const fn carry(&self, tracked: &mut TrackedRegion) -> Carried {
        if tracked.end_byte < self.start {
            return Carried::Kept;
        }
        if tracked.start_byte <= self.old_end {
            return Carried::Dropped;
        }
        self.carry_past(tracked)
    }
}

/// Moves every region in `run` by `bytes` and `rows`, returning false if any
/// position overflowed — the regions are then garbage, and the caller rebuilds.
///
/// This is the one loop in an update that runs over the regions after the
/// edit, and so the one whose cost grows with the file. It is written to run
/// at memory speed: the overflow flags are gathered without a branch per
/// field, and the check is made once at the end. Every overflow is still
/// caught — nothing is assumed about how large an offset can get.
fn shift_run(run: &mut [TrackedRegion], bytes: isize, rows: isize) -> bool {
    let mut overflowed = false;
    for tracked in run {
        let (start_byte, start_byte_overflowed) = tracked.start_byte.overflowing_add_signed(bytes);
        let (end_byte, end_byte_overflowed) = tracked.end_byte.overflowing_add_signed(bytes);
        let (start_line, start_line_overflowed) =
            tracked.region.start_line.overflowing_add_signed(rows);
        let (end_line, end_line_overflowed) = tracked.region.end_line.overflowing_add_signed(rows);
        overflowed |= start_byte_overflowed
            | end_byte_overflowed
            | start_line_overflowed
            | end_line_overflowed;
        tracked.start_byte = start_byte;
        tracked.end_byte = end_byte;
        tracked.region.start_line = start_line;
        tracked.region.end_line = end_line;
    }
    !overflowed
}

/// Moves every published region in `regions` by `rows`, returning false if
/// any line overflowed. Written the same way as [`shift_run`], for the same
/// reason.
fn shift_lines(regions: &mut [FoldRegion], rows: isize) -> bool {
    let mut overflowed = false;
    for region in regions {
        let (start_line, start_overflowed) = region.start_line.overflowing_add_signed(rows);
        let (end_line, end_overflowed) = region.end_line.overflowing_add_signed(rows);
        overflowed |= start_overflowed | end_overflowed;
        region.start_line = start_line;
        region.end_line = end_line;
    }
    !overflowed
}

/// What became of one retained region when an edit was applied to it.
enum Carried {
    /// It survived, and now holds its post-edit position.
    Kept,
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

/// `new - old` as a signed offset, or `None` if it does not fit.
fn signed_delta(new: usize, old: usize) -> Option<isize> {
    let new = isize::try_from(new).ok()?;
    let old = isize::try_from(old).ok()?;
    new.checked_sub(old)
}

/// Merges two ordered lists into one ordered list.
///
/// Both inputs are sorted by [`TrackedRegion::order_key`] and describe disjoint
/// sets of nodes, so a straight merge reproduces exactly the order a single
/// sorted walk of the whole tree would have produced. That is what keeps the
/// incremental result byte-for-byte equal to a full recompute, because
/// [`publish`] resolves duplicate line spans by input order. On a tie the
/// retained region goes first.
fn merge_ordered(kept: Vec<TrackedRegion>, fresh: Vec<TrackedRegion>) -> Vec<TrackedRegion> {
    let mut out = Vec::with_capacity(kept.len() + fresh.len());
    let mut kept = kept.into_iter().peekable();
    let mut fresh = fresh.into_iter().peekable();
    while let Some(next) = fresh.peek() {
        let key = next.order_key();
        match kept.next_if(|retained| retained.order_key() <= key) {
            Some(retained) => out.push(retained),
            None => out.extend(fresh.next()),
        }
    }
    out.extend(kept);
    out
}
