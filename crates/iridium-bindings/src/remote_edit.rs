//! Positional application of authority-sequenced remote edits.
//!
//! The authoring surface feeds `edit-applied` deltas — byte-range replaces
//! minted by the server-side document authority — into a local editor; on
//! followers, into a *read-only* editor. This module holds the
//! target-independent mechanics: byte-range validation, direct rope
//! application that bypasses the undo tree, transformation of every local
//! cursor and selection across the change, and the [`EditSpan`] handed to
//! the pending-edit composition in [`crate::edit_tracking`]. It is kept
//! free of `wasm-bindgen` so it compiles (and its unit tests run) on every
//! target.
//!
//! Two laws govern this path:
//!
//! - **A remote edit is never locally undoable.** The edit does not enter
//!   the undo tree, and the existing history is reset: its recorded
//!   commands carry coordinates valid only in the pre-edit document
//!   lineage, and replaying one after the rebase would apply an inverse at
//!   shifted positions. This is the same exact-or-refuse discipline as the
//!   edit tracker — never guess — applied to history. (The full-swap
//!   precedent, `EditorState::set_content`, resets history for the same
//!   reason.)
//! - **Cursor transformation is deterministic.** Offsets before the edit
//!   are untouched; offsets at or after the replaced range's old end shift
//!   by the byte delta (so an insertion at a cursor's exact position keeps
//!   the cursor glued to the text that followed it); offsets strictly
//!   inside the replaced range collapse to the edit start (the text they
//!   pointed into no longer exists). Cursors that land on shared ground
//!   merge through the cursor state's own invariant-preserving rules.

use iridium_editor::document::Document;
use iridium_editor::editor::Editor;
use iridium_editor::history::UndoTree;
use iridium_editor::{CursorState, Position, Range, Selection};
use thiserror::Error;

use crate::edit_tracking::{EditSpan, byte_point};
use crate::text_range::{ByteRangeError, validate_byte_range};

/// Why a remote edit was refused. On error the document, history, and
/// cursors are untouched.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RemoteEditError {
    /// The byte range is inverted, out of bounds, or not on UTF-8
    /// character boundaries.
    #[error(transparent)]
    Range(#[from] ByteRangeError),
    /// A position/offset conversion that the editor's invariants guarantee
    /// failed anyway; the edit is refused rather than applied against an
    /// inconsistent state.
    #[error("editor state is inconsistent: {context}")]
    InconsistentState {
        /// What failed to resolve.
        context: &'static str,
    },
    /// The document rejected the validated replacement (an editor-core
    /// invariant violation).
    #[error("the document rejected the replacement: {message}")]
    Apply {
        /// The document's error.
        message: String,
    },
}

/// Transforms a byte offset across a `[start, old_end) -> new_len` replace.
///
/// - `offset >= old_end`: shifts by the byte delta. This branch also
///   covers an insertion at exactly the offset (`start == old_end ==
///   offset`): the offset lands after the inserted text, keeping it glued
///   to the text that followed it.
/// - `offset <= start`: untouched (bytes before the edit never move).
/// - strictly inside the replaced range: collapses to `start` — the text
///   it pointed into no longer exists, and the edit boundary is the only
///   position that is not a guess.
#[must_use]
pub const fn transform_offset(
    offset: usize,
    start: usize,
    old_end: usize,
    new_len: usize,
) -> usize {
    if offset >= old_end {
        offset - old_end + start + new_len
    } else if offset <= start {
        offset
    } else {
        start
    }
}

/// Applies an authority-sequenced edit at `[start_byte, old_end_byte)`
/// with `text`, bypassing the undo tree (see the module docs for the two
/// laws this enforces).
///
/// Works regardless of the editor's read-only flag: read-only gates local
/// mutation sources, and a follower must still apply remote deltas.
///
/// Returns the edit's span in pre-edit coordinates for composition into
/// the pending edit tracking (`Some`), or `None` for a validated no-op
/// (empty range and empty text — nothing changed, nothing to compose).
///
/// # Errors
///
/// Returns a [`RemoteEditError`] when the range is invalid or the editor
/// state is inconsistent; the document, history, and cursors are untouched
/// on every error path.
pub fn apply_remote_edit(
    editor: &mut Editor,
    start_byte: usize,
    old_end_byte: usize,
    text: &str,
) -> Result<Option<EditSpan>, RemoteEditError> {
    // Phase 1: validate and capture everything against the pre-edit
    // document. No mutation happens until every fallible step has passed.
    let document = &editor.state().document;
    validate_byte_range(document, start_byte, old_end_byte)?;
    if start_byte == old_end_byte && text.is_empty() {
        return Ok(None);
    }
    let (old_end_row, old_end_column) =
        byte_point(document, old_end_byte).ok_or(RemoteEditError::InconsistentState {
            context: "the old end offset does not resolve to a point",
        })?;
    let span = EditSpan {
        start_byte,
        old_end_byte,
        new_end_byte: start_byte + text.len(),
        old_end_row,
        old_end_column,
    };
    let start_position =
        document
            .offset_to_position(start_byte)
            .ok_or(RemoteEditError::InconsistentState {
                context: "the start offset does not resolve to a position",
            })?;
    let old_end_position =
        document
            .offset_to_position(old_end_byte)
            .ok_or(RemoteEditError::InconsistentState {
                context: "the old end offset does not resolve to a position",
            })?;
    let primary_offsets = selection_offsets(document, &editor.state().cursor.primary)?;
    let mut secondary_offsets = Vec::with_capacity(editor.state().cursor.secondary.len());
    for selection in &editor.state().cursor.secondary {
        secondary_offsets.push(selection_offsets(document, selection)?);
    }

    // Phase 2: apply the replacement directly to the document. This is the
    // undo-tree bypass: `Editor::apply_command` (which pushes history) is
    // deliberately not used.
    let state = editor.state_mut();
    state
        .document
        .replace(Range::new(start_position, old_end_position), text)
        .map_err(|error| RemoteEditError::Apply {
            message: error.to_string(),
        })?;

    // Phase 3: reset the history (module docs, first law). This both makes
    // the remote edit un-undoable and refuses to replay recorded commands
    // whose coordinates the rebase has invalidated.
    state.history = UndoTree::new();

    // Phase 4: transform every cursor across the change and rebuild the
    // cursor state through its own invariant-preserving construction
    // (sorting and merging of cursors that now share ground).
    let new_len = text.len();
    let document = &state.document;
    let transformed = |(anchor, head): (usize, usize)| {
        Selection::new(
            position_for_offset(
                document,
                transform_offset(anchor, start_byte, old_end_byte, new_len),
            ),
            position_for_offset(
                document,
                transform_offset(head, start_byte, old_end_byte, new_len),
            ),
        )
    };
    let primary = transformed(primary_offsets);
    let secondary: Vec<Selection> = secondary_offsets.into_iter().map(transformed).collect();
    let mut cursor = CursorState::new(primary);
    for selection in secondary {
        cursor.add_cursor(selection);
    }
    state.cursor = cursor;

    // Phase 5: recorded search match ranges point into the pre-edit
    // document; re-synchronize, exactly like every content-modifying
    // command path.
    state.refresh_search();

    Ok(Some(span))
}

/// Converts a selection's anchor and head to byte offsets against the
/// given (pre-edit) document.
fn selection_offsets(
    document: &Document,
    selection: &Selection,
) -> Result<(usize, usize), RemoteEditError> {
    let anchor = document.position_to_offset(selection.anchor).ok_or(
        RemoteEditError::InconsistentState {
            context: "a cursor anchor does not resolve to an offset",
        },
    )?;
    let head =
        document
            .position_to_offset(selection.head)
            .ok_or(RemoteEditError::InconsistentState {
                context: "a cursor head does not resolve to an offset",
            })?;
    Ok((anchor, head))
}

/// Converts a transformed byte offset to a position against the post-edit
/// document.
///
/// Transformed offsets are within bounds and on character boundaries by
/// construction (untouched offsets keep their boundary, shifted offsets
/// land in the byte-identical suffix, collapsed offsets land on the
/// validated edit start, and the inserted text is valid UTF-8). The
/// clamped fallback exists so an impossible conversion degrades to the
/// document end instead of panicking mid-application.
fn position_for_offset(document: &Document, offset: usize) -> Position {
    let clamped = offset.min(document.byte_count());
    document
        .offset_to_position(clamped)
        .unwrap_or_else(|| document.clamp_position(Position::new(usize::MAX, usize::MAX)))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use iridium_editor::history::Command;
    use iridium_editor::{Editor, Selection};

    use super::*;
    use crate::edit_tracking::{PendingEdit, compose_pending, compute_edit_span};

    fn editor_with(content: &str) -> Editor {
        let mut editor = Editor::with_defaults();
        editor.set_content(content);
        editor
    }

    /// Applies a local, undoable edit through the command path while
    /// composing its span into `pending` — the native mirror of the wasm
    /// surface's `track_and_apply`/`record_edit` pair.
    fn tracked_apply(editor: &mut Editor, pending: PendingEdit, command: Command) -> PendingEdit {
        let span = compute_edit_span(&editor.state().document, &command)
            .expect("test commands are valid")
            .expect("test commands modify content");
        editor.apply_command(command);
        compose_pending(&editor.state().document, pending, span)
    }

    fn insert(line: usize, column: usize, text: &str) -> Command {
        Command::Insert {
            position: Position::new(line, column),
            text: text.to_string(),
        }
    }

    // ===== transform_offset =====

    #[test]
    fn transform_offset_covers_every_region() {
        // Replace [3, 6) with 2 bytes: delta -1.
        assert_eq!(transform_offset(0, 3, 6, 2), 0); // before
        assert_eq!(transform_offset(3, 3, 6, 2), 3); // at start
        assert_eq!(transform_offset(4, 3, 6, 2), 3); // interior collapses
        assert_eq!(transform_offset(5, 3, 6, 2), 3); // interior collapses
        assert_eq!(transform_offset(6, 3, 6, 2), 5); // at old end shifts
        assert_eq!(transform_offset(9, 3, 6, 2), 8); // after shifts
        // Pure insertion at the offset itself rides after the new text.
        assert_eq!(transform_offset(4, 4, 4, 3), 7);
        // Pure deletion with the offset at the old end lands on the start.
        assert_eq!(transform_offset(6, 3, 6, 0), 3);
    }

    // ===== basic application =====

    #[test]
    fn applies_replacement_and_returns_span() {
        let mut editor = editor_with("hello\nworld");
        let span = apply_remote_edit(&mut editor, 6, 11, "there!")
            .expect("valid edit")
            .expect("content changed");
        assert_eq!(editor.content(), "hello\nthere!");
        assert_eq!(
            span,
            EditSpan {
                start_byte: 6,
                old_end_byte: 11,
                new_end_byte: 12,
                old_end_row: 1,
                old_end_column: 5,
            }
        );
    }

    #[test]
    fn applies_insertion_into_empty_document() {
        let mut editor = editor_with("");
        apply_remote_edit(&mut editor, 0, 0, "hi").expect("valid edit");
        assert_eq!(editor.content(), "hi");
        // The caret at offset 0 rides after the inserted text.
        assert_eq!(editor.cursor(), Position::new(0, 2));
    }

    #[test]
    fn bumps_the_document_revision() {
        let mut editor = editor_with("abc");
        let before = editor.state().document.revision();
        apply_remote_edit(&mut editor, 1, 2, "XY").expect("valid edit");
        assert!(editor.state().document.revision() > before);
    }

    #[test]
    fn validated_noop_changes_nothing() {
        let mut editor = editor_with("abc");
        let pending = tracked_apply(&mut editor, PendingEdit::None, insert(0, 3, "x"));
        assert!(matches!(pending, PendingEdit::Exact(_)));
        assert!(editor.state().history.can_undo());
        let result = apply_remote_edit(&mut editor, 2, 2, "").expect("valid no-op");
        assert_eq!(result, None);
        assert_eq!(editor.content(), "abcx");
        // History survives a no-op: nothing was rebased.
        assert!(editor.state().history.can_undo());
    }

    // ===== validation refusals leave state untouched =====

    #[test]
    fn rejects_invalid_ranges_without_mutating() {
        let mut editor = editor_with("aéb");
        tracked_apply(&mut editor, PendingEdit::None, insert(0, 3, "x"));
        editor.set_cursor(Position::new(0, 2));
        let cursor_before = editor.state().cursor.clone();
        let revision_before = editor.state().document.revision();

        // Inverted.
        assert_eq!(
            apply_remote_edit(&mut editor, 3, 1, "z"),
            Err(RemoteEditError::Range(ByteRangeError::Inverted {
                start: 3,
                end: 1
            }))
        );
        // Out of bounds.
        assert_eq!(
            apply_remote_edit(&mut editor, 0, 99, "z"),
            Err(RemoteEditError::Range(ByteRangeError::OutOfBounds {
                end: 99,
                len: 5
            }))
        );
        // Mid-character ('é' occupies bytes 1..3).
        assert_eq!(
            apply_remote_edit(&mut editor, 2, 3, "z"),
            Err(RemoteEditError::Range(ByteRangeError::NotCharBoundary {
                offset: 2
            }))
        );

        assert_eq!(editor.content(), "aébx");
        assert_eq!(editor.state().document.revision(), revision_before);
        assert_eq!(editor.state().cursor, cursor_before);
        // The undoable local edit is still undoable: history untouched.
        assert!(editor.state().history.can_undo());
    }

    // ===== undo-tree bypass =====

    #[test]
    fn undo_after_remote_edit_does_not_revert_it() {
        let mut editor = editor_with("hello world");
        // A local, undoable edit first.
        tracked_apply(&mut editor, PendingEdit::None, insert(0, 0, "X"));
        assert_eq!(editor.content(), "Xhello world");
        assert!(editor.state().history.can_undo());

        // The remote edit: insert "big " before "world" (byte 7).
        apply_remote_edit(&mut editor, 7, 7, "big ").expect("valid edit");
        assert_eq!(editor.content(), "Xhello big world");

        // Undo must not revert the remote edit — and must not replay the
        // local command against rebased coordinates either. The history
        // was reset: undo reports nothing to undo and the content stands.
        assert!(!editor.undo());
        assert!(!editor.state().history.can_undo());
        assert_eq!(editor.content(), "Xhello big world");
    }

    #[test]
    fn redo_after_remote_edit_is_also_gone() {
        let mut editor = editor_with("abc");
        tracked_apply(&mut editor, PendingEdit::None, insert(0, 3, "x"));
        assert!(editor.undo());
        assert_eq!(editor.content(), "abc");
        assert!(editor.state().history.can_redo());

        apply_remote_edit(&mut editor, 0, 0, "Q").expect("valid edit");
        assert_eq!(editor.content(), "Qabc");
        assert!(!editor.redo());
        assert_eq!(editor.content(), "Qabc");
    }

    // ===== cursor transformation =====

    #[test]
    fn cursor_before_the_edit_is_untouched() {
        let mut editor = editor_with("hello world");
        editor.set_cursor(Position::new(0, 2));
        apply_remote_edit(&mut editor, 6, 11, "there").expect("valid edit");
        assert_eq!(editor.cursor(), Position::new(0, 2));
    }

    #[test]
    fn cursor_after_the_edit_shifts_by_the_delta() {
        let mut editor = editor_with("hello world");
        editor.set_cursor(Position::new(0, 9));
        // Replace "hello" (0..5) with "hi": delta -3.
        apply_remote_edit(&mut editor, 0, 5, "hi").expect("valid edit");
        assert_eq!(editor.content(), "hi world");
        assert_eq!(editor.cursor(), Position::new(0, 6));
    }

    #[test]
    fn cursor_inside_the_replaced_range_collapses_to_the_start() {
        let mut editor = editor_with("abcdefgh");
        editor.set_cursor(Position::new(0, 4));
        apply_remote_edit(&mut editor, 2, 7, "XY").expect("valid edit");
        assert_eq!(editor.content(), "abXYh");
        assert_eq!(editor.cursor(), Position::new(0, 2));
    }

    #[test]
    fn insertion_at_the_cursor_position_keeps_it_glued_to_following_text() {
        let mut editor = editor_with("hello world");
        editor.set_cursor(Position::new(0, 5));
        apply_remote_edit(&mut editor, 5, 5, "!!!").expect("valid edit");
        assert_eq!(editor.content(), "hello!!! world");
        assert_eq!(editor.cursor(), Position::new(0, 8));
    }

    #[test]
    fn selection_straddling_the_edit_keeps_its_direction() {
        let mut editor = editor_with("abcdefghij");
        // Forward selection anchor 1 -> head 8 across a 3..6 replacement
        // with "XY" (delta -1).
        editor.set_selection(Position::new(0, 1), Position::new(0, 8));
        apply_remote_edit(&mut editor, 3, 6, "XY").expect("valid edit");
        assert_eq!(editor.content(), "abcXYghij");
        let primary = editor.state().cursor.primary;
        assert_eq!(primary.anchor, Position::new(0, 1));
        assert_eq!(primary.head, Position::new(0, 7));
        assert!(primary.is_forward());

        // The backward variant preserves backwardness.
        let mut editor = editor_with("abcdefghij");
        editor.set_selection(Position::new(0, 8), Position::new(0, 1));
        apply_remote_edit(&mut editor, 3, 6, "XY").expect("valid edit");
        let primary = editor.state().cursor.primary;
        assert_eq!(primary.anchor, Position::new(0, 7));
        assert_eq!(primary.head, Position::new(0, 1));
        assert!(primary.is_backward());
    }

    #[test]
    fn every_cursor_of_a_multi_cursor_set_is_transformed() {
        let mut editor = editor_with("aa\nbb\ncc");
        editor.set_cursor(Position::new(2, 1));
        editor
            .state_mut()
            .cursor
            .add_cursor(Selection::collapsed(Position::new(0, 1)));
        editor
            .state_mut()
            .cursor
            .add_cursor(Selection::collapsed(Position::new(1, 1)));
        assert_eq!(editor.state().cursor.cursor_count(), 3);

        // Insert a line at the very top: every offset shifts by 2.
        apply_remote_edit(&mut editor, 0, 0, "Z\n").expect("valid edit");
        assert_eq!(editor.content(), "Z\naa\nbb\ncc");
        let cursor = &editor.state().cursor;
        assert_eq!(cursor.cursor_count(), 3);
        assert_eq!(cursor.primary.head, Position::new(3, 1));
        assert_eq!(cursor.secondary[0].head, Position::new(1, 1));
        assert_eq!(cursor.secondary[1].head, Position::new(2, 1));
    }

    #[test]
    fn cursors_collapsing_onto_shared_ground_merge() {
        let mut editor = editor_with("abcdef");
        editor.set_cursor(Position::new(0, 2));
        editor
            .state_mut()
            .cursor
            .add_cursor(Selection::collapsed(Position::new(0, 4)));
        assert_eq!(editor.state().cursor.cursor_count(), 2);

        // Both carets sit inside the deleted range and collapse to its
        // start — duplicate carets must merge into one.
        apply_remote_edit(&mut editor, 1, 5, "").expect("valid edit");
        assert_eq!(editor.content(), "af");
        assert_eq!(editor.state().cursor.cursor_count(), 1);
        assert_eq!(editor.cursor(), Position::new(0, 1));
    }

    #[test]
    fn multibyte_text_shifts_cursors_by_bytes_not_chars() {
        // "aé日b": a=0, é=1..3, 日=3..6, b=6..7.
        let mut editor = editor_with("aé日b");
        // Caret before 'b' (character column 3, byte offset 6).
        editor.set_cursor(Position::new(0, 3));
        // Replace 'é' (1..3, 2 bytes) with "ü" (2 bytes): delta 0.
        apply_remote_edit(&mut editor, 1, 3, "ü").expect("valid edit");
        assert_eq!(editor.content(), "aü日b");
        assert_eq!(editor.cursor(), Position::new(0, 3));

        // Insert 6 bytes (2 chars) at the front: char column moves by 2.
        apply_remote_edit(&mut editor, 0, 0, "日本").expect("valid edit");
        assert_eq!(editor.content(), "日本aü日b");
        assert_eq!(editor.cursor(), Position::new(0, 5));
    }

    // ===== read-only interaction =====

    #[test]
    fn read_only_editor_still_applies_remote_edits() {
        let mut editor = editor_with("follower");
        editor.state_mut().read_only = true;
        apply_remote_edit(&mut editor, 0, 0, "the ").expect("valid edit");
        assert_eq!(editor.content(), "the follower");
        // Read-only still gates local mutation sources: undo is refused at
        // the core level regardless of history state.
        assert!(!editor.undo());
    }

    // ===== pending-edit composition =====

    #[test]
    fn remote_span_composes_exactly_with_a_pending_local_edit() {
        let mut editor = editor_with("abc");
        // Local tracked edit: insert 'x' at the end (3..3 -> 3..4).
        let pending = tracked_apply(&mut editor, PendingEdit::None, insert(0, 3, "x"));

        // Remote edit at the front: insert 'Q' (0..0 -> 0..1).
        let span = apply_remote_edit(&mut editor, 0, 0, "Q")
            .expect("valid edit")
            .expect("content changed");
        let pending = compose_pending(&editor.state().document, pending, span);

        // Base "abc" -> final "Qabcx": the composed span must map the base
        // exactly (start 0, old end 3, new end 5).
        let PendingEdit::Exact(composed) = pending else {
            panic!("expected exact composition, got {pending:?}");
        };
        assert_eq!(editor.content(), "Qabcx");
        assert_eq!(
            composed.span,
            EditSpan {
                start_byte: 0,
                old_end_byte: 3,
                new_end_byte: 5,
                old_end_row: 0,
                old_end_column: 3,
            }
        );
    }

    #[test]
    fn degraded_pending_state_stays_degraded_across_a_remote_edit() {
        let mut editor = editor_with("abc");
        let span = apply_remote_edit(&mut editor, 1, 2, "XY")
            .expect("valid edit")
            .expect("content changed");
        // A consumer whose pending state already degraded must stay
        // degraded (full reparse) — the remote span never resurrects it.
        assert_eq!(
            compose_pending(&editor.state().document, PendingEdit::Degraded, span),
            PendingEdit::Degraded
        );
    }
}
