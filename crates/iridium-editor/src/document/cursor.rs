//! Cursor and selection state management.

use serde::{Deserialize, Serialize};

use super::position::{Position, Range};

/// A selection in the document with anchor and head.
///
/// The anchor is where the selection started, and the head is where the cursor
/// currently is. This allows tracking selection direction.
///
/// When anchor equals head, the selection is "collapsed" to just a cursor.
///
/// # Example
///
/// ```
/// use iridium_editor::{Position, Selection};
///
/// // A collapsed selection (just a cursor)
/// let cursor = Selection::collapsed(Position::new(0, 5));
/// assert!(cursor.is_collapsed());
///
/// // A forward selection
/// let selection = Selection::new(
///     Position::new(0, 0),
///     Position::new(0, 10),
/// );
/// assert!(selection.is_forward());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Selection {
    /// Where the selection started
    pub anchor: Position,
    /// Where the cursor/head is (current position)
    pub head: Position,
}

impl Selection {
    /// Creates a new selection from anchor to head.
    #[must_use]
    pub const fn new(anchor: Position, head: Position) -> Self {
        Self { anchor, head }
    }

    /// Creates a collapsed selection (cursor) at the given position.
    #[must_use]
    pub const fn collapsed(position: Position) -> Self {
        Self {
            anchor: position,
            head: position,
        }
    }

    /// Returns true if the selection is collapsed (just a cursor).
    #[must_use]
    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.head
    }

    /// Returns true if the selection is forward (anchor <= head).
    #[must_use]
    pub fn is_forward(&self) -> bool {
        self.anchor <= self.head
    }

    /// Returns true if the selection is backward (anchor > head).
    #[must_use]
    pub fn is_backward(&self) -> bool {
        self.anchor > self.head
    }

    /// Returns the cursor position (same as head).
    #[must_use]
    pub const fn cursor_position(&self) -> Position {
        self.head
    }

    /// Returns the selection as a normalized range (start <= end).
    #[must_use]
    pub fn range(&self) -> Range {
        Range::new(self.anchor, self.head)
    }

    /// Returns the start position (minimum of anchor and head).
    #[must_use]
    pub fn start(&self) -> Position {
        std::cmp::min(self.anchor, self.head)
    }

    /// Returns the end position (maximum of anchor and head).
    #[must_use]
    pub fn end(&self) -> Position {
        std::cmp::max(self.anchor, self.head)
    }

    /// Returns true if this selection overlaps with another.
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        let self_start = self.start();
        let self_end = self.end();
        let other_start = other.start();
        let other_end = other.end();

        // Two ranges overlap if one starts before the other ends
        self_start < other_end && other_start < self_end
    }

    /// Merges this selection with another, expanding to cover both.
    ///
    /// The direction is preserved from `self`.
    #[must_use]
    pub fn merge(&self, other: &Self) -> Self {
        let min_start = std::cmp::min(self.start(), other.start());
        let max_end = std::cmp::max(self.end(), other.end());

        if self.is_forward() {
            Self::new(min_start, max_end)
        } else {
            Self::new(max_end, min_start)
        }
    }
}

/// Cursor state supporting multiple cursors.
///
/// The primary cursor always exists. Additional secondary cursors can be
/// added for multi-cursor editing.
///
/// # Invariants
///
/// - Primary cursor always exists
/// - Secondary cursors are sorted by position
/// - No two cursors overlap
///
/// # Example
///
/// ```
/// use iridium_editor::{CursorState, Position, Selection};
///
/// let mut cursors = CursorState::new(Selection::collapsed(Position::new(0, 0)));
/// cursors.add_cursor(Selection::collapsed(Position::new(1, 0)));
/// assert_eq!(cursors.cursor_count(), 2);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CursorState {
    /// Primary cursor (always exists)
    pub primary: Selection,
    /// Additional cursors (may be empty)
    pub secondary: Vec<Selection>,
}

impl CursorState {
    /// Creates a new cursor state with a single primary cursor.
    #[must_use]
    pub const fn new(primary: Selection) -> Self {
        Self {
            primary,
            secondary: Vec::new(),
        }
    }

    /// Creates a cursor state with a collapsed cursor at the given position.
    #[must_use]
    pub const fn at(position: Position) -> Self {
        Self::new(Selection::collapsed(position))
    }

    /// Returns the total number of cursors.
    #[must_use]
    pub fn cursor_count(&self) -> usize {
        1 + self.secondary.len()
    }

    /// Returns an iterator over all selections (primary first).
    pub fn all_selections(&self) -> impl Iterator<Item = &Selection> {
        std::iter::once(&self.primary).chain(self.secondary.iter())
    }

    /// Returns a mutable iterator over all selections (primary first).
    pub fn all_selections_mut(&mut self) -> impl Iterator<Item = &mut Selection> {
        std::iter::once(&mut self.primary).chain(self.secondary.iter_mut())
    }

    /// Adds a new cursor at the given selection.
    ///
    /// The cursor is inserted in sorted order by position.
    /// If the new cursor overlaps with an existing cursor, they are merged.
    pub fn add_cursor(&mut self, selection: Selection) {
        // Find the insertion point
        let pos = selection.start();
        let insert_idx = self
            .secondary
            .iter()
            .position(|s| s.start() > pos)
            .unwrap_or(self.secondary.len());

        self.secondary.insert(insert_idx, selection);
        self.merge_overlapping();
    }

    /// Removes the cursor at the given index.
    ///
    /// Index 0 is the primary cursor, which cannot be removed (this is a no-op).
    /// Indices 1+ correspond to secondary cursors.
    pub fn remove_cursor(&mut self, index: usize) {
        if index > 0 && index <= self.secondary.len() {
            self.secondary.remove(index - 1);
        }
    }

    /// Collapses to only the primary cursor, removing all secondary cursors.
    pub fn collapse_to_primary(&mut self) {
        self.secondary.clear();
    }

    /// Sorts all cursors by position.
    ///
    /// Secondary cursors are sorted by their start position.
    pub fn sort(&mut self) {
        self.secondary.sort_by_key(Selection::start);
    }

    /// Returns true when two cursors must collapse into one.
    ///
    /// Two cursors merge when they genuinely occupy shared ground:
    /// - their ranges strictly overlap (share at least one interior position),
    ///   or
    /// - they touch at an endpoint (one ends exactly where the other begins)
    ///   **and at least one side is collapsed** — which also covers duplicate
    ///   collapsed carets at the exact same position, and a collapsed caret
    ///   sitting on either endpoint of a non-empty selection.
    ///
    /// Crucially, two *non-empty* selections that merely touch at an endpoint
    /// do **not** merge. Occurrence-based multi-cursor verbs
    /// (select-all-occurrences, Ctrl+D, skip) legitimately place adjacent
    /// matches such as the two `foo` halves of `foofoo`; those must remain
    /// distinct edit targets rather than fuse into a single selection.
    ///
    /// A collapsed caret touching a non-empty selection *must* merge, however:
    /// leaving it distinct (e.g. a Ctrl-click at the exact end of an existing
    /// selection) makes a single keystroke apply both the selection replacement
    /// and a boundary insertion, duplicating the typed text. Merging touching
    /// non-empty ranges (fusing adjacent occurrences) and failing to merge a
    /// touching collapsed caret (duplicate edits) were the two poles of the
    /// adjacency defect.
    fn should_merge(a: &Selection, b: &Selection) -> bool {
        if a.overlaps(b) {
            return true;
        }
        if !a.is_collapsed() && !b.is_collapsed() {
            // Two non-empty selections that only touch at an endpoint stay
            // distinct edit targets.
            return false;
        }
        // At least one side is collapsed: merge when the ranges touch at an
        // endpoint (covers duplicate collapsed carets and a caret on either
        // endpoint of a non-empty selection).
        a.end() == b.start() || b.end() == a.start()
    }

    /// Merges cursors that share ground (see [`Self::should_merge`]).
    ///
    /// After merging, all cursors are sorted by position and no two cursors
    /// overlap. Adjacent non-empty selections are preserved as distinct
    /// cursors; only strict overlaps and duplicate collapsed carets collapse.
    fn merge_overlapping(&mut self) {
        if self.secondary.is_empty() {
            return;
        }

        // First, fold any secondary cursor that shares ground with the primary
        // into the primary and drop it from the secondary list.
        let mut i = 0;
        while i < self.secondary.len() {
            if Self::should_merge(&self.primary, &self.secondary[i]) {
                self.primary = self.primary.merge(&self.secondary[i]);
                self.secondary.remove(i);
            } else {
                i += 1;
            }
        }

        // Sort secondaries by start position
        self.sort();

        // Merge secondaries that share ground with their successor.
        let mut i = 0;
        while i + 1 < self.secondary.len() {
            if Self::should_merge(&self.secondary[i], &self.secondary[i + 1]) {
                let merged = self.secondary[i].merge(&self.secondary[i + 1]);
                self.secondary[i] = merged;
                self.secondary.remove(i + 1);
                // Don't increment i, check the merged selection against the next one
            } else {
                i += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_collapsed() {
        let sel = Selection::collapsed(Position::new(5, 10));
        assert!(sel.is_collapsed());
        assert_eq!(sel.cursor_position(), Position::new(5, 10));
    }

    #[test]
    fn selection_direction() {
        let forward = Selection::new(Position::new(0, 0), Position::new(0, 10));
        assert!(forward.is_forward());
        assert!(!forward.is_backward());

        let backward = Selection::new(Position::new(0, 10), Position::new(0, 0));
        assert!(!backward.is_forward());
        assert!(backward.is_backward());
    }

    #[test]
    fn cursor_state_add() {
        let mut state = CursorState::at(Position::new(0, 0));
        assert_eq!(state.cursor_count(), 1);

        state.add_cursor(Selection::collapsed(Position::new(1, 0)));
        assert_eq!(state.cursor_count(), 2);

        state.add_cursor(Selection::collapsed(Position::new(2, 0)));
        assert_eq!(state.cursor_count(), 3);
    }

    #[test]
    fn cursor_state_collapse() {
        let mut state = CursorState::at(Position::new(0, 0));
        state.add_cursor(Selection::collapsed(Position::new(1, 0)));
        state.add_cursor(Selection::collapsed(Position::new(2, 0)));

        state.collapse_to_primary();
        assert_eq!(state.cursor_count(), 1);
    }

    #[test]
    fn selection_overlaps() {
        let sel1 = Selection::new(Position::new(0, 0), Position::new(0, 10));
        let sel2 = Selection::new(Position::new(0, 5), Position::new(0, 15));
        assert!(sel1.overlaps(&sel2));
        assert!(sel2.overlaps(&sel1));

        let sel3 = Selection::new(Position::new(0, 20), Position::new(0, 30));
        assert!(!sel1.overlaps(&sel3));
    }

    #[test]
    fn selection_merge() {
        let sel1 = Selection::new(Position::new(0, 0), Position::new(0, 10));
        let sel2 = Selection::new(Position::new(0, 5), Position::new(0, 15));
        let merged = sel1.merge(&sel2);
        assert_eq!(merged.start(), Position::new(0, 0));
        assert_eq!(merged.end(), Position::new(0, 15));
    }

    #[test]
    fn cursor_state_merge_overlapping() {
        let mut state = CursorState::new(Selection::new(Position::new(0, 0), Position::new(0, 5)));
        // Add overlapping cursor
        state.add_cursor(Selection::new(Position::new(0, 3), Position::new(0, 10)));
        // Should be merged into primary
        assert_eq!(state.cursor_count(), 1);
        assert_eq!(state.primary.start(), Position::new(0, 0));
        assert_eq!(state.primary.end(), Position::new(0, 10));
    }

    #[test]
    fn cursor_state_adjacent_non_empty_selections_stay_distinct() {
        let mut state = CursorState::new(Selection::new(Position::new(0, 0), Position::new(0, 5)));
        // Add an adjacent selection (starts exactly where the primary ends).
        // Two non-empty selections that only touch must NOT fuse: occurrence
        // verbs rely on adjacent matches remaining separate edit targets.
        state.add_cursor(Selection::new(Position::new(0, 5), Position::new(0, 10)));
        assert_eq!(state.cursor_count(), 2);
        assert_eq!(state.primary.range().end, Position::new(0, 5));
        assert_eq!(state.secondary[0].start(), Position::new(0, 5));
        assert_eq!(state.secondary[0].end(), Position::new(0, 10));
    }

    #[test]
    fn cursor_state_collapsed_caret_touching_selection_end_merges() {
        // Reviewer finding: a Ctrl-click at the exact end of an existing
        // selection lands a collapsed caret touching the selection's endpoint.
        // It must merge, or a single keystroke applies both the selection
        // replacement and a boundary insertion (duplicating the typed text).
        let mut state = CursorState::new(Selection::new(Position::new(0, 0), Position::new(0, 3)));
        state.add_cursor(Selection::collapsed(Position::new(0, 3)));
        assert_eq!(state.cursor_count(), 1);
        assert_eq!(state.primary.start(), Position::new(0, 0));
        assert_eq!(state.primary.end(), Position::new(0, 3));
    }

    #[test]
    fn cursor_state_collapsed_caret_touching_selection_start_merges() {
        // The symmetric case: a Ctrl-click at the exact start of an existing
        // selection also merges rather than leaving a duplicate edit target.
        let mut state = CursorState::new(Selection::new(Position::new(0, 2), Position::new(0, 5)));
        state.add_cursor(Selection::collapsed(Position::new(0, 2)));
        assert_eq!(state.cursor_count(), 1);
        assert_eq!(state.primary.start(), Position::new(0, 2));
        assert_eq!(state.primary.end(), Position::new(0, 5));
    }

    #[test]
    fn cursor_state_collapsed_caret_not_touching_selection_stays_distinct() {
        // A collapsed caret with a genuine gap from the selection is a real
        // second cursor and must not be swallowed by the endpoint-merge rule.
        let mut state = CursorState::new(Selection::new(Position::new(0, 0), Position::new(0, 3)));
        state.add_cursor(Selection::collapsed(Position::new(0, 5)));
        assert_eq!(state.cursor_count(), 2);
    }

    #[test]
    fn cursor_state_duplicate_collapsed_carets_merge() {
        let mut state = CursorState::at(Position::new(0, 5));
        // A second collapsed caret at the same position is a duplicate and
        // collapses into one cursor.
        state.add_cursor(Selection::collapsed(Position::new(0, 5)));
        assert_eq!(state.cursor_count(), 1);
        assert_eq!(state.primary.head, Position::new(0, 5));
    }

    #[test]
    fn cursor_state_no_merge_non_overlapping() {
        let mut state = CursorState::new(Selection::collapsed(Position::new(0, 0)));
        state.add_cursor(Selection::collapsed(Position::new(0, 10)));
        state.add_cursor(Selection::collapsed(Position::new(0, 20)));
        // All distinct, should not merge
        assert_eq!(state.cursor_count(), 3);
    }

    #[test]
    fn cursor_state_sorted() {
        let mut state = CursorState::at(Position::new(1, 0));
        state.add_cursor(Selection::collapsed(Position::new(0, 0))); // Before primary
        state.add_cursor(Selection::collapsed(Position::new(2, 0))); // After primary

        // Secondary should be sorted
        assert_eq!(state.secondary[0].head, Position::new(0, 0));
        assert_eq!(state.secondary[1].head, Position::new(2, 0));
    }
}
