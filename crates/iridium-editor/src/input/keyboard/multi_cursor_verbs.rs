//! Multi-cursor and multi-selection command implementations.
//!
//! Two kinds of verb live here and they interact:
//!
//! - **occurrence verbs** — add-selection-to-next-match,
//!   select-all-occurrences, skip-last-occurrence — which search the document
//!   for the text under the primary selection;
//! - **positional verbs** — add-cursor-above/below, which clone every cursor one
//!   line away at its sticky column.
//!
//! Both maintain the *addition-order stack* ([`KeyboardHandler::cursor_add_order`]),
//! which is what lets remove-last-cursor and skip peel the most recently added
//! cursor rather than an arbitrary one. The stack is only meaningful while it
//! describes the current cursor state, so every verb calls
//! [`KeyboardHandler::sync_add_order`] first and that discards a stack any other
//! path has invalidated.

use std::collections::HashMap;

use crate::document::{CursorState, Document, Position, Range, Selection};
use crate::history::Command;

use super::motions::VerticalDirection;
use super::types::KeyResult;
use super::{KeyboardHandler, motions, multi_cursor};

impl KeyboardHandler {
    /// Selects the whole document as one selection.
    pub(super) fn handle_select_all(document: &Document, cursor: &CursorState) -> KeyResult {
        let start = motions::document_start();
        let end = motions::document_end(document);

        let new_cursor = CursorState::new(Selection::new(start, end));

        KeyResult::Command(Command::SetSelection {
            old_state: cursor.clone(),
            new_state: new_cursor,
        })
    }

    /// Adds a selection at the next occurrence of the primary selection's text.
    ///
    /// If no selection exists, selects the word at the cursor.
    /// If a selection exists, finds the next occurrence and adds a cursor there.
    ///
    /// A genuinely added cursor is recorded on the addition-order stack (see
    /// [`Self::record_cursor_change`]) so remove-last-cursor and
    /// [`Self::skip_last_added_occurrence`] can later drop it.
    pub(super) fn handle_add_selection_next_match(
        &mut self,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        self.sync_add_order(cursor, document.revision());

        // Get the search text - either from selection or select word at cursor
        let (search_text, initial_selection) = if cursor.primary.is_collapsed() {
            // No selection - select word at cursor first
            let word_sel = motions::select_word_at(document, cursor.primary.head);
            if word_sel.is_collapsed() {
                return KeyResult::Handled; // No word to select
            }
            let text = document.slice(word_sel.range());
            (text, Some(word_sel))
        } else {
            // Use existing selection
            let text = document.slice(cursor.primary.range());
            (text, None)
        };

        if search_text.is_empty() {
            return KeyResult::Handled;
        }

        let mut new_cursor = cursor.clone();

        // If we had to select a word first, do that (no cursor is added yet).
        if let Some(sel) = initial_selection {
            new_cursor.primary = sel;
            return self.finish_cursor_command(cursor, &new_cursor);
        }

        // Find the next occurrence that is not already selected, starting after
        // the last cursor and wrapping. Using the shared occurrence scan (which
        // skips ranges already `taken`) rather than a bare first-match search
        // means add-next-match composes with skip: after skip leaves an earlier
        // occurrence unselected, the next add fills that gap instead of
        // stopping on the first (already selected) match.
        let search_start = Self::last_selection_end(&new_cursor);
        let after = document.position_to_offset(search_start).unwrap_or(0);
        let taken: Vec<Range> = new_cursor.all_selections().map(Selection::range).collect();

        let Some(next) = multi_cursor::next_occurrence(document, &search_text, after, &taken)
        else {
            // Every occurrence is already selected (or there is only one).
            return KeyResult::Handled;
        };

        new_cursor.add_cursor(next);
        self.finish_cursor_command(cursor, &new_cursor)
    }

    /// Gets the position after the last selection (for add-next-match).
    fn last_selection_end(cursor: &CursorState) -> Position {
        let mut last_end = cursor.primary.end();
        for sel in &cursor.secondary {
            if sel.end() > last_end {
                last_end = sel.end();
            }
        }
        last_end
    }

    /// Selects every occurrence of the primary selection's text (or the word
    /// under a collapsed caret) as its own cursor.
    ///
    /// Matching is case-sensitive and exact; occurrences are non-overlapping (as
    /// with add-next-match). The reference selection stays primary, keeping its
    /// exact range **and direction** (a backward selection stays backward, so the
    /// active head does not jump). Every other occurrence that does not overlap
    /// the primary becomes its own distinct secondary cursor — including
    /// occurrences adjacent to each other, which stay separate edit targets
    /// rather than collapsing into one. A source that is empty (an empty
    /// selection or no word under the caret) is a no-op; an explicit non-empty
    /// selection is always honored, whitespace included, matching
    /// add-next-match's contract.
    pub(super) fn handle_select_all_occurrences(
        &mut self,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        self.sync_add_order(cursor, document.revision());

        // Resolve the reference occurrence: the primary selection (kept
        // verbatim, direction included), or the word under a collapsed caret
        // (reusing the add-next-match word logic).
        let reference = if cursor.primary.is_collapsed() {
            let word = motions::select_word_at(document, cursor.primary.head);
            if word.is_collapsed() {
                return KeyResult::Handled;
            }
            word
        } else {
            cursor.primary
        };

        let source = document.slice(reference.range());
        if source.is_empty() {
            return KeyResult::Handled;
        }

        // Seed the occupied set with the exact primary range so the reference
        // selection is preserved verbatim (range and direction) while every
        // other occurrence that does not overlap it is returned as a distinct,
        // document-ordered secondary. The occurrence scanner keeps adjacent
        // matches separate (e.g. the two "foo" halves of "foofoo") and anchors
        // to the primary rather than a zero-based partition, so a primary that
        // does not fall on a non-overlapping boundary (columns 1..3 of "aaa")
        // still yields the correct set. Building the secondary list directly —
        // never via `add_cursor`, whose adjacency merge would fuse touching
        // occurrences — keeps every match its own edit target. The occurrences
        // are non-overlapping by construction, so the state upholds the
        // `CursorState` invariants.
        let secondary =
            multi_cursor::occurrences_excluding(document, &source, &[reference.range()]);

        // Always construct the desired state and route it through
        // finish_cursor_command; never short-circuit on an empty secondary.
        // When the reference is a uniquely occurring word under a collapsed
        // caret, there are no other matches yet the caret must still expand to
        // select that single occurrence. And when an unrelated secondary cursor
        // exists (added by a vertical/mouse path), select-all must replace the
        // whole set with just the reference occurrence rather than preserving
        // that unrelated cursor as a stray edit target. `create_selection_command`
        // still collapses to a no-op when the constructed state equals the
        // incoming one (e.g. the reference already is the sole cursor and has no
        // other occurrences).
        let new_state = CursorState {
            primary: reference,
            secondary,
        };

        self.finish_cursor_command(cursor, &new_state)
    }

    /// Removes the most recently added secondary cursor.
    ///
    /// The cursor to remove is the top of the addition-order stack (see
    /// [`Self::cursor_add_order`]). With a single cursor, or when the stack has
    /// been invalidated by an intervening motion or edit, this is a no-op.
    pub(super) fn handle_undo_last_cursor(
        &mut self,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        self.sync_add_order(cursor, document.revision());

        let Some(target) = self.cursor_add_order.pop() else {
            // Nothing tracked to remove (single cursor, or the stack was
            // invalidated since the last add verb).
            return KeyResult::Handled;
        };

        let new_state = Self::remove_secondary(cursor, &target);
        self.add_order_anchor = Some(new_state.clone());
        Self::create_selection_command(cursor, &new_state)
    }

    /// Adds a cursor one line above or below every existing cursor.
    ///
    /// Each existing cursor is cloned one line in `direction` at its sticky
    /// preferred column (clamped to the target line's length); cloning every
    /// cursor and merging collisions means repeated presses extend each
    /// contiguous block by one line. Cursors at the document edge (the first
    /// line going up, the last line going down) contribute nothing — there is
    /// no wraparound. Integrates with the same [`Self::preferred_columns`]
    /// sticky-column state as plain vertical movement.
    pub(super) fn handle_add_cursor_vertical(
        &mut self,
        document: &Document,
        cursor: &CursorState,
        direction: VerticalDirection,
    ) -> KeyResult {
        let revision = document.revision();
        self.sync_add_order(cursor, revision);

        // Reuse the sticky columns from a previous vertical move/add only when
        // they were captured for this exact cursor state; otherwise seed from
        // the current head columns.
        let preferred: Vec<usize> = self.take_sticky_columns_for(cursor, revision);

        // Clone each cursor one line in `direction`, keeping its sticky column.
        let last_line = document.line_count().saturating_sub(1);
        let mut clones: Vec<(Selection, usize)> = Vec::new();
        for (index, sel) in cursor.all_selections().enumerate() {
            let sticky = preferred.get(index).copied().unwrap_or(sel.head.column);
            let clone_line = match direction {
                VerticalDirection::Up => {
                    if sel.head.line == 0 {
                        continue; // At the top: no cursor above.
                    }
                    sel.head.line - 1
                },
                VerticalDirection::Down => {
                    if sel.head.line >= last_line {
                        continue; // At the bottom: no cursor below.
                    }
                    sel.head.line + 1
                },
            };
            let line_len = document.line_len(clone_line).unwrap_or(0);
            let clone_pos = Position::new(clone_line, sticky.min(line_len));
            clones.push((Selection::collapsed(clone_pos), sticky));
        }

        if clones.is_empty() {
            // Every cursor was already at the document edge: nothing to add,
            // but the cursor state is unchanged so its sticky columns remain
            // valid for a subsequent vertical operation.
            self.store_sticky_columns(cursor, preferred, revision);
            return KeyResult::Handled;
        }

        // Build the new cursor state: all originals plus the clones, merged.
        let mut new_state = CursorState::new(cursor.primary);
        for sel in &cursor.secondary {
            new_state.add_cursor(*sel);
        }
        for (sel, _) in &clones {
            new_state.add_cursor(*sel);
        }

        // Rebuild the sticky columns aligned to the new cursor order. Clones
        // seed their source's sticky column; an original at the same position
        // (a collision) wins, so its sticky column is preserved.
        let mut sticky_by_pos: HashMap<Position, usize> = HashMap::new();
        for (sel, sticky) in &clones {
            sticky_by_pos.insert(sel.head, *sticky);
        }
        for (index, sel) in cursor.all_selections().enumerate() {
            let sticky = preferred.get(index).copied().unwrap_or(sel.head.column);
            sticky_by_pos.insert(sel.head, sticky);
        }
        let new_preferred: Vec<usize> = new_state
            .all_selections()
            .map(|sel| {
                sticky_by_pos
                    .get(&sel.head)
                    .copied()
                    .unwrap_or(sel.head.column)
            })
            .collect();
        self.store_sticky_columns(&new_state, new_preferred, revision);

        self.record_cursor_change(cursor, &new_state);
        Self::create_selection_command(cursor, &new_state)
    }

    /// Drops the most recently added occurrence cursor and adds the next
    /// occurrence of its text instead, wrapping around the document.
    ///
    /// Bound to the chord `Ctrl+K Ctrl+D` in the default keymap (VS Code's "Move
    /// Last Selection to Next Find Match"), and still callable directly — it is
    /// threaded through [`crate::Editor`] — so a host that drives the editor
    /// without a keyboard can reach it too.
    ///
    /// Behavior:
    /// - With one or more added occurrence cursors, the most recently added
    ///   one is removed and the next occurrence after it that is not already
    ///   selected is added (wrapping); the added cursor becomes the new top of
    ///   the addition-order stack.
    /// - With no added cursors and a collapsed caret, the word under the caret
    ///   is selected (the add-next-match bootstrap), so a following call can
    ///   advance.
    /// - With no added cursors and a non-empty primary selection, the primary
    ///   is moved to the next occurrence (wrapping).
    ///
    /// Returns the resulting [`KeyResult`]; the caller applies any command.
    pub fn skip_last_added_occurrence(
        &mut self,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        let result = self.skip_occurrence_inner(document, cursor);
        if matches!(result, KeyResult::Command(_)) {
            // Skip changed the selection through a non-vertical path, so any
            // held sticky columns describe a cursor set the user has since
            // navigated away from — even when the skip wraps back to a
            // byte-identical selection, the live head's column is what a
            // following vertical verb must seed from. Destroy (not merely
            // dirty) the columns, exactly as `note_operation` treats the other
            // occurrence verbs. The addition-order stack is left intact: skip
            // maintains it itself and `reset_vertical_state` does not touch it.
            self.reset_vertical_state();
        }
        result
    }

    /// The body of [`Self::skip_last_added_occurrence`], separated so the
    /// public entry point can uniformly invalidate sticky columns on every
    /// branch that produced a selection change.
    fn skip_occurrence_inner(&mut self, document: &Document, cursor: &CursorState) -> KeyResult {
        self.sync_add_order(cursor, document.revision());

        // Find the most recently added *occurrence* cursor: a non-collapsed
        // selection on the stack. Collapsed additions (vertical add-above/below
        // cursors) are not find matches, so they are skipped over rather than
        // treated as the search source — a vertical add on top of the stack
        // must never block advancing the last occurrence.
        let occ_index = self
            .cursor_add_order
            .iter()
            .rposition(|sel| !sel.is_collapsed());

        let Some(idx) = occ_index else {
            // No occurrence cursor to drop: bootstrap or move the primary.
            if cursor.primary.is_collapsed() {
                let word = motions::select_word_at(document, cursor.primary.head);
                if word.is_collapsed() {
                    return KeyResult::Handled;
                }
                let mut new_state = cursor.clone();
                new_state.primary = word;
                return self.finish_cursor_command(cursor, &new_state);
            }
            return self.skip_move_primary(document, cursor);
        };

        let dropped = self.cursor_add_order[idx];
        let source = document.slice(dropped.range());
        if source.is_empty() {
            // A non-collapsed selection should always carry text; nothing safe
            // to advance if it somehow does not.
            return KeyResult::Handled;
        }

        let remaining = Self::remove_secondary(cursor, &dropped);
        let taken: Vec<Range> = remaining.all_selections().map(Selection::range).collect();
        let after = document
            .position_to_offset(dropped.range().end)
            .unwrap_or(0);

        match multi_cursor::next_occurrence(document, &source, after, &taken) {
            Some(next) if next.range() != dropped.range() => {
                let mut new_state = remaining;
                new_state.add_cursor(next);
                // Replace the dropped occurrence entry with the newly added
                // one, keeping it the most recent occurrence for a following
                // skip while leaving any collapsed (vertical) entries in place.
                self.cursor_add_order.remove(idx);
                self.cursor_add_order.push(next);
                self.add_order_anchor = Some(new_state.clone());
                Self::create_selection_command(cursor, &new_state)
            },
            _ => {
                // Only the dropped occurrence exists (or every other is already
                // selected): leave the cursor state and stack unchanged.
                KeyResult::Handled
            },
        }
    }

    /// Moves the primary selection to the next occurrence of its text
    /// (wrapping), used by [`Self::skip_last_added_occurrence`] when there are
    /// no added cursors to drop.
    fn skip_move_primary(&mut self, document: &Document, cursor: &CursorState) -> KeyResult {
        let source = document.slice(cursor.primary.range());
        if source.is_empty() {
            return KeyResult::Handled;
        }
        let taken: Vec<Range> = cursor.secondary.iter().map(Selection::range).collect();
        let after = document
            .position_to_offset(cursor.primary.range().end)
            .unwrap_or(0);

        match multi_cursor::next_occurrence(document, &source, after, &taken) {
            Some(next) if next.range() != cursor.primary.range() => {
                let mut new_state = cursor.clone();
                new_state.primary = next;
                self.finish_cursor_command(cursor, &new_state)
            },
            _ => KeyResult::Handled,
        }
    }

    /// Removes the secondary cursor whose selection equals `target`, rebuilding
    /// the cursor state from the primary and remaining secondaries.
    fn remove_secondary(cursor: &CursorState, target: &Selection) -> CursorState {
        let mut new_state = CursorState::new(cursor.primary);
        for sel in &cursor.secondary {
            if sel != target {
                new_state.add_cursor(*sel);
            }
        }
        new_state
    }

    /// Ensures the addition-order stack is consistent with `cursor`.
    ///
    /// If the incoming cursor state differs from the one the stack was last
    /// consistent with ([`Self::add_order_anchor`]), or the document content
    /// changed since ([`Self::add_order_revision`] vs `revision`), some other
    /// path moved, added, or removed a cursor — or shifted the text under
    /// otherwise-unchanged cursors — so the stack is stale and is cleared.
    /// Otherwise the stack is left intact (and any entry that somehow no longer
    /// corresponds to a current secondary is dropped defensively).
    fn sync_add_order(&mut self, cursor: &CursorState, revision: u64) {
        if self.add_order_anchor.as_ref() != Some(cursor)
            || self.add_order_revision != Some(revision)
        {
            self.cursor_add_order.clear();
            self.add_order_anchor = Some(cursor.clone());
            self.add_order_revision = Some(revision);
            return;
        }
        let secondaries: Vec<Selection> = cursor.secondary.clone();
        self.cursor_add_order
            .retain(|sel| secondaries.contains(sel));
    }

    /// Records the cursor transition produced by a multi-cursor add verb.
    ///
    /// Every secondary cursor in `new_state` that was not already a selection
    /// in `old` is pushed onto the addition-order stack in document order, and
    /// the anchor is advanced to `new_state` so the next verb trusts the stack.
    /// The comparison is by selection identity, not cursor *count*: select-all
    /// can create fresh occurrence selections while *reducing* the count (five
    /// vertical cursors collapsing to three matches), and those new selections
    /// must still be recorded so remove-last-cursor/skip can peel them. Verbs
    /// that only reshape the primary (the add-next-match word bootstrap) add no
    /// new secondary and so push nothing.
    ///
    /// [`Self::add_order_revision`] is intentionally left untouched here: an
    /// add verb never changes document content, so the revision recorded by
    /// the preceding [`Self::sync_add_order`] still describes `new_state`.
    fn record_cursor_change(&mut self, old: &CursorState, new_state: &CursorState) {
        let old_selections: Vec<Selection> = old.all_selections().copied().collect();
        for sel in &new_state.secondary {
            if !old_selections.contains(sel) {
                self.cursor_add_order.push(*sel);
            }
        }
        self.add_order_anchor = Some(new_state.clone());
    }

    /// Records the cursor change and emits the resulting selection command.
    ///
    /// The sticky vertical column is not touched: the cursor set changed
    /// through a non-vertical path, so any held sticky columns no longer match
    /// the new state and are ignored on the next vertical move without being
    /// destroyed (which keeps them recoverable if undo restores the prior
    /// state).
    fn finish_cursor_command(&mut self, old: &CursorState, new_state: &CursorState) -> KeyResult {
        self.record_cursor_change(old, new_state);
        Self::create_selection_command(old, new_state)
    }
}
