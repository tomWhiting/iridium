//! Caret motion commands and the sticky-column machinery they share.
//!
//! Horizontal and absolute motions are stateless: they map every cursor's head
//! through a [`motions`] primitive and emit one selection command. Vertical
//! motion is not — it carries a per-cursor *preferred column* so that walking
//! down through a short line and out the other side returns to the column you
//! started in. That state lives on [`KeyboardHandler`] and is validated by exact
//! cursor-state identity plus the document's content revision; see the field
//! documentation on the handler for why, and
//! [`KeyboardHandler::note_operation`](super::KeyboardHandler::note_operation)
//! for what invalidates it.

use crate::document::{CursorState, Document, Position, Selection};

use super::KeyboardHandler;
use super::motions::{self, VerticalDirection};
use super::types::KeyResult;

impl KeyboardHandler {
    /// Applies `motion` to every cursor once and emits the resulting selection
    /// command.
    ///
    /// `extend` keeps each selection's anchor (the selecting variant of a
    /// motion) instead of collapsing onto the new head. Cursors that converge
    /// are merged by [`motions::apply_to_all`], and an unchanged cursor state
    /// yields [`KeyResult::Handled`] rather than a no-op command.
    ///
    /// # Once, and only once
    ///
    /// This is the helper for motions a count **cannot** repeat, and the
    /// distinction is not stylistic. `line_start_smart` *toggles* between the
    /// first non-whitespace column and column zero, so applying it twice returns
    /// the caret to where it started; the document and line boundaries are
    /// absolute, so applying them twice is the same as applying them once.
    /// Repeating either would be wrong in one case and pointless in the other.
    /// See [`Self::apply_repeated_motion`] for the ones a count does repeat.
    pub(super) fn apply_motion<F>(cursor: &CursorState, extend: bool, motion: F) -> KeyResult
    where
        F: Fn(Position) -> Position,
    {
        let new_cursor = motions::apply_to_all(cursor, extend, |_, sel| motion(sel.head));
        Self::create_selection_command(cursor, &new_cursor)
    }

    /// Applies `motion` to every cursor `count` times and emits **one** selection
    /// command for the whole journey.
    ///
    /// One command, not `count` of them: `3l` is a single entry in the undo
    /// history, and undoing it returns the caret to where it was before the
    /// three, not to two thirds of the way through. That is the difference
    /// between honouring a count here and running the action three times at the
    /// dispatch layer.
    ///
    /// # Why the loop stops early
    ///
    /// A motion that returns the position it was given has reached a boundary,
    /// and no number of further repeats can pass it. Breaking there is what
    /// bounds `999999999h` by the size of the document rather than by the number
    /// the user typed — without an arbitrary cap on counts, which would be a
    /// limit nobody could predict from the outside.
    pub(super) fn apply_repeated_motion<F>(
        cursor: &CursorState,
        extend: bool,
        count: u32,
        motion: F,
    ) -> KeyResult
    where
        F: Fn(Position) -> Position,
    {
        let new_cursor = motions::apply_to_all(cursor, extend, |_, sel| {
            let mut head = sel.head;
            for _ in 0..count {
                let next = motion(head);
                if next == head {
                    break;
                }
                head = next;
            }
            head
        });
        Self::create_selection_command(cursor, &new_cursor)
    }

    /// Moves every cursor `lines` lines up or down, honouring the sticky
    /// columns, and emits the resulting selection command.
    ///
    /// A vertical count is a *parameter*, not a repetition:
    /// [`motions::vertical_move_by`] takes the hop size and clamps it to the
    /// document, so `3j` costs exactly what `j` costs and a count larger than
    /// the document is one clamped hop rather than a loop.
    pub(super) fn vertical_motion(
        &mut self,
        document: &Document,
        cursor: &CursorState,
        extend: bool,
        direction: VerticalDirection,
        lines: usize,
    ) -> KeyResult {
        let new_cursor = self.move_vertically(document, cursor, extend, direction, lines.max(1));
        Self::create_selection_command(cursor, &new_cursor)
    }

    /// Moves every cursor one page up or down, honouring the sticky columns, and
    /// emits the resulting selection command.
    ///
    /// A page is [`Self::page_rows`] lines — the viewport height the host last
    /// synced. A host that reports zero rows (chrome can consume every row a
    /// terminal has) or has never synced still pages **one** line: the key must
    /// always do something visible rather than silently die with the layout.
    pub(super) fn page_motion(
        &mut self,
        document: &Document,
        cursor: &CursorState,
        extend: bool,
        direction: VerticalDirection,
        pages: usize,
    ) -> KeyResult {
        let lines = self.page_rows.max(1).saturating_mul(pages.max(1));
        let new_cursor = self.move_vertically(document, cursor, extend, direction, lines);
        Self::create_selection_command(cursor, &new_cursor)
    }

    /// Collapses to the primary cursor and clears its selection.
    ///
    /// A state that is already a single collapsed caret is left alone, so the key
    /// is acknowledged without producing a command.
    pub(super) fn handle_collapse_to_primary(cursor: &CursorState) -> KeyResult {
        if cursor.secondary.is_empty() && cursor.primary.is_collapsed() {
            return KeyResult::Handled;
        }

        let mut new_cursor = cursor.clone();
        new_cursor.collapse_to_primary();
        new_cursor.primary = Selection::collapsed(cursor.primary.head);

        Self::create_selection_command(cursor, &new_cursor)
    }

    /// Moves every cursor `lines` lines up or down with per-cursor sticky
    /// columns.
    fn move_vertically(
        &mut self,
        document: &Document,
        cursor: &CursorState,
        extend_selection: bool,
        direction: VerticalDirection,
        lines: usize,
    ) -> CursorState {
        let count = cursor.cursor_count();
        let revision = document.revision();

        // Reuse the sticky columns from the previous vertical move only when
        // they were captured for this exact cursor state; otherwise start from
        // the current head columns.
        let mut preferred = self.take_sticky_columns_for(cursor, revision);

        let new_cursor = motions::apply_to_all(cursor, extend_selection, |index, sel| {
            let target = preferred.get(index).copied().unwrap_or(sel.head.column);
            let (head, sticky) =
                motions::vertical_move_by(document, sel.head, target, direction, lines);
            if let Some(slot) = preferred.get_mut(index) {
                *slot = sticky;
            }
            head
        });

        // If cursors merged, the per-cursor mapping is gone; rebuild from the
        // actual columns on the next vertical move. Otherwise remember the
        // columns keyed to the state they now describe.
        if new_cursor.cursor_count() == count {
            self.store_sticky_columns(&new_cursor, preferred, revision);
        } else {
            self.preferred_columns = None;
            self.sticky_state = None;
            self.sticky_revision = None;
            self.sticky_dirty = false;
        }

        new_cursor
    }

    /// Returns the sticky columns to seed a vertical operation for `cursor`.
    ///
    /// When [`Self::preferred_columns`] was captured for exactly this cursor
    /// state (and still lines up with it), those columns are taken; otherwise
    /// a fresh vector of the current head columns is returned. Either way the
    /// stored sticky state is consumed, to be re-stored against the resulting
    /// state by [`Self::store_sticky_columns`].
    pub(super) fn take_sticky_columns_for(
        &mut self,
        cursor: &CursorState,
        revision: u64,
    ) -> Vec<usize> {
        let reusable = !self.sticky_dirty
            && self.sticky_state.as_ref() == Some(cursor)
            && self.sticky_revision == Some(revision)
            && self
                .preferred_columns
                .as_ref()
                .is_some_and(|columns| columns.len() == cursor.cursor_count());
        self.sticky_state = None;
        self.sticky_revision = None;
        self.sticky_dirty = false;
        if reusable {
            if let Some(columns) = self.preferred_columns.take() {
                return columns;
            }
        }
        self.preferred_columns = None;
        cursor.all_selections().map(|sel| sel.head.column).collect()
    }

    /// Records `columns` as the sticky columns for `state` (the cursor state a
    /// vertical operation just produced) at content revision `revision`,
    /// aligned to its [`CursorState::all_selections`] order. The freshly stored
    /// columns are clean (not dirty).
    pub(super) fn store_sticky_columns(
        &mut self,
        state: &CursorState,
        columns: Vec<usize>,
        revision: u64,
    ) {
        self.preferred_columns = Some(columns);
        self.sticky_state = Some(state.clone());
        self.sticky_revision = Some(revision);
        self.sticky_dirty = false;
    }
}
