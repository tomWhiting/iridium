//! Rope-derived edit-span tracking for incremental parsing.
//!
//! The WASM surface reports every content mutation to the host as a
//! tree-sitter-style edit — byte offsets plus `(row, byte_column)` points —
//! computed from the rope, never from JavaScript strings. This module holds
//! the pure math: span extraction from [`Command`]s against the pre-edit
//! document, and exact composition of sequential edits between two host
//! consumptions. It is kept free of `wasm-bindgen` so it compiles (and its
//! unit tests run) on every target.
//!
//! Coordinate spaces: the *base document* is the document as the host last
//! saw it (at the previous consumption). A pending [`EditSpan`] maps the
//! base document to the current one: `start_byte` and `old_end_byte` are
//! base-document offsets, `new_end_byte` is a current-document offset. The
//! old-end point must be captured against the pre-edit rope at record time
//! (that document no longer exists afterwards); the start and new-end
//! points are derived from the current document at consumption time, which
//! is exact because the bytes before `start_byte` are untouched.

use iridium_editor::document::Document;
use iridium_editor::history::Command;

/// One recorded edit in base-document coordinates (see module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditSpan {
    /// Byte offset where the edit begins (identical in base and current
    /// documents).
    pub start_byte: usize,
    /// Byte offset where the replaced text ended in the base document.
    pub old_end_byte: usize,
    /// Byte offset where the new text ends in the current document.
    pub new_end_byte: usize,
    /// Row of the old end position (base document).
    pub old_end_row: usize,
    /// Byte column of the old end position within its row (base document).
    pub old_end_column: usize,
}

/// A pending edit span plus the point of its new end in the current
/// document.
///
/// The new-end point is captured when the span is recorded or composed
/// (against the then-current document, which stays current until the next
/// composition refreshes it). It is required to back-map a later edit whose
/// replaced range extends beyond the pending new text: bytes past
/// `span.new_end_byte` in the current document correspond 1:1 to bytes past
/// `span.old_end_byte` in the base document, and the stored point anchors
/// the row/column arithmetic for that region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingSpan {
    /// The composed base-to-current edit span.
    pub span: EditSpan,
    /// Row of `span.new_end_byte` in the current document.
    pub new_end_row: usize,
    /// Byte column of `span.new_end_byte` within its row (current document).
    pub new_end_column: usize,
}

/// Accumulated edit state between two host consumptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PendingEdit {
    /// No content change since the last consumption.
    #[default]
    None,
    /// Exactly one composed edit span is known.
    Exact(PendingSpan),
    /// Content changed, but the combined span can no longer be described
    /// exactly (inconsistent coordinates were detected while composing; the
    /// module refuses to ever report a wrong span). The consumer must fall
    /// back to a full reparse.
    Degraded,
}

/// Converts a byte offset into a tree-sitter point `(row, byte_column)`
/// against the given document.
///
/// Returns `None` when the offset lies outside the document.
#[must_use]
pub fn byte_point(document: &Document, offset: usize) -> Option<(usize, usize)> {
    let position = document.offset_to_position(offset)?;
    let line_start = document.line_to_byte_offset(position.line)?;
    Some((position.line, offset.checked_sub(line_start)?))
}

/// Computes the byte span affected by a command against the pre-edit
/// document.
///
/// Content commands inside a `Compound` are emitted by the editor core in
/// reverse document order with coordinates that are all valid in the
/// original document (see `emit_content_commands` in
/// `iridium_editor::input::keyboard::editing`), so every position converts
/// against the same pre-edit rope. The combined span is the minimum start /
/// maximum old end over the content commands, with the new end derived from
/// the total byte delta.
///
/// Returns `Ok(None)` when the command contains no content edit, and
/// `Err(())` when a position cannot be resolved (inconsistent state; the
/// caller must degrade to a full reparse rather than report a wrong span).
#[allow(clippy::result_unit_err)]
pub fn compute_edit_span(document: &Document, command: &Command) -> Result<Option<EditSpan>, ()> {
    struct Acc {
        start: usize,
        old_end: usize,
        delta: i64,
    }

    fn merge(acc: &mut Option<Acc>, start: usize, old_end: usize, delta: i64) {
        match acc {
            Some(acc) => {
                acc.start = acc.start.min(start);
                acc.old_end = acc.old_end.max(old_end);
                acc.delta += delta;
            },
            None => {
                *acc = Some(Acc {
                    start,
                    old_end,
                    delta,
                });
            },
        }
    }

    fn walk(document: &Document, command: &Command, acc: &mut Option<Acc>) -> Result<(), ()> {
        match command {
            Command::Insert { position, text } => {
                let start = document.position_to_offset(*position).ok_or(())?;
                let inserted = i64::try_from(text.len()).map_err(|_| ())?;
                merge(acc, start, start, inserted);
            },
            Command::Delete {
                range,
                deleted_text,
            } => {
                let start = document.position_to_offset(range.start).ok_or(())?;
                let removed = i64::try_from(deleted_text.len()).map_err(|_| ())?;
                merge(acc, start, start + deleted_text.len(), -removed);
            },
            Command::Replace {
                range,
                old_text,
                new_text,
            } => {
                let start = document.position_to_offset(range.start).ok_or(())?;
                let removed = i64::try_from(old_text.len()).map_err(|_| ())?;
                let inserted = i64::try_from(new_text.len()).map_err(|_| ())?;
                merge(acc, start, start + old_text.len(), inserted - removed);
            },
            Command::SetSelection { .. } => {},
            Command::Compound { commands } => {
                for inner in commands {
                    walk(document, inner, acc)?;
                }
            },
        }
        Ok(())
    }

    let mut acc: Option<Acc> = None;
    walk(document, command, &mut acc)?;

    let Some(acc) = acc else {
        return Ok(None);
    };

    let new_end = i64::try_from(acc.old_end).map_err(|_| ())? + acc.delta;
    let new_end_byte = usize::try_from(new_end).map_err(|_| ())?.max(acc.start);
    let (old_end_row, old_end_column) = byte_point(document, acc.old_end).ok_or(())?;

    Ok(Some(EditSpan {
        start_byte: acc.start,
        old_end_byte: acc.old_end,
        new_end_byte,
        old_end_row,
        old_end_column,
    }))
}

/// Merges a new edit span into the pending edit.
///
/// `pending` maps the base document to the previous document; `span` maps
/// the previous document to its successor, and `document` is that successor
/// (the post-edit, now current, document — used to capture the composed
/// span's new-end point for future compositions).
///
/// Composition is always exact for consistent inputs. Two cases:
///
/// - `span.old_end_byte <= prev.new_end_byte`: the new edit's replaced
///   range ends at or before the end of the pending new text (consecutive
///   typing, backspacing into just-typed text). The pending old end is
///   unchanged and the new end grows by the untouched tail.
/// - `span.old_end_byte > prev.new_end_byte`: the new edit's replaced range
///   ends beyond the pending new text. Bytes past `prev.new_end_byte` in
///   the previous document map 1:1 onto bytes past `prev.old_end_byte` in
///   the base document, so the combined old end (byte and point) is
///   back-mapped exactly. The composed span covers everything from the
///   minimum start to that old end — including any unchanged middle between
///   the two edits — which is a conservative superset that stays harmless
///   for tree-sitter because all offsets and deltas are exact.
///
/// The state degrades only when the coordinates are internally inconsistent
/// (point arithmetic would underflow, or the composed new end does not
/// resolve in `document`): the module never reports a wrong span.
#[must_use]
pub fn compose_pending(document: &Document, pending: PendingEdit, span: EditSpan) -> PendingEdit {
    let composed = match pending {
        PendingEdit::None => span,
        PendingEdit::Degraded => return PendingEdit::Degraded,
        PendingEdit::Exact(prev) => {
            if span.old_end_byte <= prev.span.new_end_byte {
                let untouched_tail = prev.span.new_end_byte - span.old_end_byte;
                EditSpan {
                    // Bytes before either start are unshifted between the
                    // base and current documents, so the minimum is valid in
                    // the base document.
                    start_byte: prev.span.start_byte.min(span.start_byte),
                    old_end_byte: prev.span.old_end_byte,
                    new_end_byte: span.new_end_byte + untouched_tail,
                    old_end_row: prev.span.old_end_row,
                    old_end_column: prev.span.old_end_column,
                }
            } else {
                match compose_beyond(&prev, span) {
                    Some(composed) => composed,
                    None => return PendingEdit::Degraded,
                }
            }
        },
    };
    // Capture the composed new-end point against the current document so
    // the next composition can back-map past it.
    match byte_point(document, composed.new_end_byte) {
        Some((new_end_row, new_end_column)) => PendingEdit::Exact(PendingSpan {
            span: composed,
            new_end_row,
            new_end_column,
        }),
        None => PendingEdit::Degraded,
    }
}

/// Composes a span whose replaced range ends beyond the pending new text
/// (`span.old_end_byte > prev.span.new_end_byte`).
///
/// The old end is back-mapped through the 1:1 region after the pending
/// edit: `old_end_byte = prev.old_end_byte + (span.old_end_byte -
/// prev.new_end_byte)`. Its base-document point follows from the stored
/// new-end point: when `span`'s old end sits on the same row as the pending
/// new end, only the column shifts; on a later row, the row shifts and the
/// column is untouched (the line containing it lies entirely in the 1:1
/// region). The minimum start is valid in the base document for every
/// position of `span.start_byte` (before, inside, or after the pending
/// edit's region).
///
/// Returns `None` when the coordinates are inconsistent (arithmetic would
/// wrap); the caller degrades rather than report a wrong span.
fn compose_beyond(prev: &PendingSpan, span: EditSpan) -> Option<EditSpan> {
    let beyond = span.old_end_byte.checked_sub(prev.span.new_end_byte)?;
    let old_end_byte = prev.span.old_end_byte.checked_add(beyond)?;
    let (old_end_row, old_end_column) = if span.old_end_row == prev.new_end_row {
        let column_gap = span.old_end_column.checked_sub(prev.new_end_column)?;
        (
            prev.span.old_end_row,
            prev.span.old_end_column.checked_add(column_gap)?,
        )
    } else {
        let row_gap = span.old_end_row.checked_sub(prev.new_end_row)?;
        (
            prev.span.old_end_row.checked_add(row_gap)?,
            span.old_end_column,
        )
    };
    Some(EditSpan {
        start_byte: prev.span.start_byte.min(span.start_byte),
        old_end_byte,
        new_end_byte: span.new_end_byte,
        old_end_row,
        old_end_column,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use iridium_editor::{Position, Range};

    fn doc(content: &str) -> Document {
        Document::new(content)
    }

    fn insert(line: usize, column: usize, text: &str) -> Command {
        Command::Insert {
            position: Position::new(line, column),
            text: text.to_string(),
        }
    }

    fn delete(start: (usize, usize), end: (usize, usize), deleted: &str) -> Command {
        Command::Delete {
            range: Range::new(Position::new(start.0, start.1), Position::new(end.0, end.1)),
            deleted_text: deleted.to_string(),
        }
    }

    // ===== byte_point =====

    #[test]
    fn byte_point_ascii() {
        let d = doc("hello\nworld");
        assert_eq!(byte_point(&d, 0), Some((0, 0)));
        assert_eq!(byte_point(&d, 5), Some((0, 5)));
        assert_eq!(byte_point(&d, 6), Some((1, 0)));
        assert_eq!(byte_point(&d, 11), Some((1, 5)));
    }

    #[test]
    fn byte_point_returns_byte_columns_for_multibyte_text() {
        // 'é' is 2 bytes in UTF-8.
        let d = doc("aé\nb");
        assert_eq!(byte_point(&d, 3), Some((0, 3))); // after 'é' (byte column)
        assert_eq!(byte_point(&d, 4), Some((1, 0)));
    }

    #[test]
    fn byte_point_out_of_bounds_is_none() {
        let d = doc("ab");
        assert_eq!(byte_point(&d, 3), None);
    }

    // ===== compute_edit_span =====

    #[test]
    fn span_for_insert() {
        let d = doc("hello\nworld");
        let span = compute_edit_span(&d, &insert(1, 0, "xy"))
            .expect("valid command")
            .expect("content edit");
        assert_eq!(
            span,
            EditSpan {
                start_byte: 6,
                old_end_byte: 6,
                new_end_byte: 8,
                old_end_row: 1,
                old_end_column: 0,
            }
        );
    }

    #[test]
    fn span_for_delete() {
        let d = doc("hello\nworld");
        let span = compute_edit_span(&d, &delete((0, 1), (0, 3), "el"))
            .expect("valid command")
            .expect("content edit");
        assert_eq!(
            span,
            EditSpan {
                start_byte: 1,
                old_end_byte: 3,
                new_end_byte: 1,
                old_end_row: 0,
                old_end_column: 3,
            }
        );
    }

    #[test]
    fn span_for_replace() {
        let d = doc("hello");
        let cmd = Command::Replace {
            range: Range::new(Position::new(0, 1), Position::new(0, 3)),
            old_text: "el".to_string(),
            new_text: "ELLO".to_string(),
        };
        let span = compute_edit_span(&d, &cmd)
            .expect("valid command")
            .expect("content edit");
        assert_eq!(
            span,
            EditSpan {
                start_byte: 1,
                old_end_byte: 3,
                new_end_byte: 5,
                old_end_row: 0,
                old_end_column: 3,
            }
        );
    }

    #[test]
    fn span_for_selection_only_command_is_none() {
        let d = doc("hello");
        let cmd = Command::Compound {
            commands: Vec::new(),
        };
        assert_eq!(compute_edit_span(&d, &cmd), Ok(None));
    }

    #[test]
    fn span_for_multi_cursor_compound_covers_all_edits() {
        // Two cursors inserting "ab" at (0,0) and (1,0) — the core emits the
        // commands in reverse document order with pre-edit coordinates.
        let d = doc("one\ntwo");
        let cmd = Command::Compound {
            commands: vec![insert(1, 0, "ab"), insert(0, 0, "ab")],
        };
        let span = compute_edit_span(&d, &cmd)
            .expect("valid command")
            .expect("content edit");
        // start = min(0, 4) = 0; old_end = max(0, 4) = 4; delta = +4.
        assert_eq!(
            span,
            EditSpan {
                start_byte: 0,
                old_end_byte: 4,
                new_end_byte: 8,
                old_end_row: 1,
                old_end_column: 0,
            }
        );
    }

    #[test]
    fn span_with_invalid_position_is_err() {
        let d = doc("hi");
        assert_eq!(compute_edit_span(&d, &insert(5, 0, "x")), Err(()));
    }

    // ===== compose_pending =====

    /// Applies a sequence of raw byte edits `(start, old_end, new_text)` to
    /// `base`, computing each edit's span exactly as `track_and_apply` does
    /// (old-end point captured against the pre-edit document) and folding
    /// through `compose_pending` with the post-edit document.
    ///
    /// Returns the final content and the composed pending edit.
    fn compose_sequence(base: &str, edits: &[(usize, usize, &str)]) -> (String, PendingEdit) {
        let mut content = base.to_string();
        let mut pending = PendingEdit::None;
        for &(start, old_end, text) in edits {
            let pre = doc(&content);
            let (old_end_row, old_end_column) =
                byte_point(&pre, old_end).expect("edit old end must lie inside the document");
            let span = EditSpan {
                start_byte: start,
                old_end_byte: old_end,
                new_end_byte: start + text.len(),
                old_end_row,
                old_end_column,
            };
            content.replace_range(start..old_end, text);
            pending = compose_pending(&doc(&content), pending, span);
        }
        (content, pending)
    }

    /// Asserts the composed pending edit maps `base` to the final content
    /// exactly: untouched prefix and suffix, exact base-document old-end
    /// point, exact current-document new-end point, and a byte delta equal
    /// to the sum of the individual edit deltas. Returns the pending span
    /// for additional assertions.
    fn assert_exact_composition(base: &str, edits: &[(usize, usize, &str)]) -> PendingSpan {
        let (content, pending) = compose_sequence(base, edits);
        let PendingEdit::Exact(p) = pending else {
            panic!("expected exact composition, got {pending:?}");
        };
        let span = p.span;
        assert!(span.start_byte <= span.old_end_byte, "span must be ordered");
        assert_eq!(
            base[..span.start_byte],
            content[..span.start_byte],
            "prefix before start_byte must be untouched"
        );
        assert_eq!(
            base[span.old_end_byte..],
            content[span.new_end_byte..],
            "suffix after the old/new ends must be untouched"
        );
        assert_eq!(
            byte_point(&doc(base), span.old_end_byte),
            Some((span.old_end_row, span.old_end_column)),
            "old-end point must be exact in the base document"
        );
        assert_eq!(
            byte_point(&doc(&content), span.new_end_byte),
            Some((p.new_end_row, p.new_end_column)),
            "new-end point must be exact in the current document"
        );
        let total_delta: i64 = edits
            .iter()
            .map(|&(start, old_end, text)| {
                let inserted = i64::try_from(text.len()).expect("test sizes fit i64");
                let removed = i64::try_from(old_end - start).expect("test sizes fit i64");
                inserted - removed
            })
            .sum();
        let new_end = i64::try_from(span.new_end_byte).expect("test sizes fit i64");
        let old_end = i64::try_from(span.old_end_byte).expect("test sizes fit i64");
        assert_eq!(
            new_end - old_end,
            total_delta,
            "composed delta must equal the sum of the individual deltas"
        );
        p
    }

    #[test]
    fn compose_onto_none_stores_span_with_new_end_point() {
        // Insert 'x' at the end of "abc" -> "abcx".
        let p = assert_exact_composition("abc", &[(3, 3, "x")]);
        assert_eq!(
            p.span,
            EditSpan {
                start_byte: 3,
                old_end_byte: 3,
                new_end_byte: 4,
                old_end_row: 0,
                old_end_column: 3,
            }
        );
        assert_eq!((p.new_end_row, p.new_end_column), (0, 4));
    }

    #[test]
    fn compose_sequential_typing_is_exact() {
        // Type 'x' at offset 3, then 'y' at offset 4 (inside the new text).
        let p = assert_exact_composition("abc", &[(3, 3, "x"), (4, 4, "y")]);
        assert_eq!(
            p.span,
            EditSpan {
                start_byte: 3,
                old_end_byte: 3,
                new_end_byte: 5,
                old_end_row: 0,
                old_end_column: 3,
            }
        );
    }

    #[test]
    fn compose_backspace_into_typed_text_is_exact() {
        // Type "xy" at offset 3 (3..3 -> 3..5), then backspace the 'y'
        // (4..5 -> 4..4 in the current document).
        let p = assert_exact_composition("abc", &[(3, 3, "xy"), (4, 5, "")]);
        assert_eq!(
            p.span,
            EditSpan {
                start_byte: 3,
                old_end_byte: 3,
                new_end_byte: 4,
                old_end_row: 0,
                old_end_column: 3,
            }
        );
    }

    #[test]
    fn compose_edit_before_pending_is_exact() {
        // Insert "AB" at offset 10 (10..10 -> 10..12), then delete bytes
        // 2..4 (before the first edit's region).
        let p = assert_exact_composition("0123456789", &[(10, 10, "AB"), (2, 4, "")]);
        // Combined: base 2..10 old end stays 10; new end = second.new_end
        // (2) + untouched tail (12 - 4 = 8) = 10.
        assert_eq!(
            p.span,
            EditSpan {
                start_byte: 2,
                old_end_byte: 10,
                new_end_byte: 10,
                old_end_row: 0,
                old_end_column: 10,
            }
        );
    }

    #[test]
    fn compose_disjoint_after_is_exact() {
        // Norn's example: base "ab", insert "x" at 0..0 -> 0..1, then insert
        // "y" at 3..3 -> 3..4 (entirely after the first edit's new text).
        // Base-to-final is exactly 0..2 -> 0..4.
        let p = assert_exact_composition("ab", &[(0, 0, "x"), (3, 3, "y")]);
        assert_eq!(
            p.span,
            EditSpan {
                start_byte: 0,
                old_end_byte: 2,
                new_end_byte: 4,
                old_end_row: 0,
                old_end_column: 2,
            }
        );
        assert_eq!((p.new_end_row, p.new_end_column), (0, 4));
    }

    #[test]
    fn compose_adjacent_start_at_pending_new_end_is_exact() {
        // "ab" -> insert "x" at 0 -> "xab"; then delete 1..3 ("ab"), which
        // starts exactly at the pending new end and extends beyond it.
        let p = assert_exact_composition("ab", &[(0, 0, "x"), (1, 3, "")]);
        assert_eq!(
            p.span,
            EditSpan {
                start_byte: 0,
                old_end_byte: 2,
                new_end_byte: 1,
                old_end_row: 0,
                old_end_column: 2,
            }
        );
    }

    #[test]
    fn compose_adjacent_old_end_at_pending_new_end_is_exact() {
        // "ab" -> insert "x" at 0 -> "xab"; then replace 1..1 with "q"
        // (old end exactly at the pending new end: first-branch boundary).
        let p = assert_exact_composition("ab", &[(0, 0, "x"), (1, 1, "q")]);
        assert_eq!(
            p.span,
            EditSpan {
                start_byte: 0,
                old_end_byte: 0,
                new_end_byte: 2,
                old_end_row: 0,
                old_end_column: 0,
            }
        );
    }

    #[test]
    fn compose_straddling_pending_boundary_is_exact() {
        // "abcdef" -> insert "X" at 3 -> "abcXdef"; then delete 3..6 ("Xde"),
        // which starts inside the pending new text and extends beyond it.
        // Base-to-final deletes base bytes 3..5 ("de").
        let p = assert_exact_composition("abcdef", &[(3, 3, "X"), (3, 6, "")]);
        assert_eq!(
            p.span,
            EditSpan {
                start_byte: 3,
                old_end_byte: 5,
                new_end_byte: 3,
                old_end_row: 0,
                old_end_column: 5,
            }
        );
    }

    #[test]
    fn compose_disjoint_after_across_rows_back_maps_the_point() {
        // "ab\ncd" -> insert "x\n" at 0 -> "x\nab\ncd" (the pending new end
        // moves to row 1); then insert "y" at the document end (row 2). The
        // back-mapped old end is the base document end (1, 2).
        let p = assert_exact_composition("ab\ncd", &[(0, 0, "x\n"), (7, 7, "y")]);
        assert_eq!(
            p.span,
            EditSpan {
                start_byte: 0,
                old_end_byte: 5,
                new_end_byte: 8,
                old_end_row: 1,
                old_end_column: 2,
            }
        );
        assert_eq!((p.new_end_row, p.new_end_column), (2, 3));
    }

    #[test]
    fn compose_whole_document_span_after_pending_is_exact() {
        // Mirrors the undo path: a tracked edit followed by a conservative
        // whole-document span (0..pre_len -> 0..new_len) composes exactly.
        let p = assert_exact_composition("one\ntwo", &[(0, 0, "z"), (0, 8, "one\ntwo")]);
        assert_eq!(
            p.span,
            EditSpan {
                start_byte: 0,
                old_end_byte: 7,
                new_end_byte: 7,
                old_end_row: 1,
                old_end_column: 3,
            }
        );
    }

    #[test]
    fn compose_deltas_sum_over_edit_sequences() {
        // Property-style check across mixed sequences: for every exactly
        // composable chain, assert_exact_composition verifies that the
        // composed old/new delta equals the sum of the individual deltas
        // and that prefix/suffix/points are exact.
        let sequences: &[(&str, &[(usize, usize, &str)])] = &[
            // Typing runs.
            ("fn main() {}", &[(3, 3, "x"), (4, 4, "y"), (5, 5, "z")]),
            // Disjoint inserts marching right.
            ("abcdef", &[(0, 0, "1"), (3, 3, "2"), (8, 8, "3")]),
            // Insert, then a disjoint delete after it.
            ("hello world", &[(0, 0, "say "), (9, 15, "")]),
            // Delete, then a disjoint insert after it.
            ("hello world", &[(0, 5, ""), (3, 3, "XY")]),
            // Straddle: replacement crossing the pending boundary.
            ("abcdef", &[(2, 4, "XYZ"), (1, 6, "q")]),
            // Multi-line: newline insert, then edits after it.
            ("aa\nbb\ncc", &[(2, 2, "\nzz"), (9, 11, "Q"), (10, 10, "!")]),
            // Enclosing replacement (second edit covers the first entirely).
            ("0123456789", &[(4, 5, "AB"), (2, 9, "")]),
        ];
        for &(base, edits) in sequences {
            assert_exact_composition(base, edits);
        }
    }

    #[test]
    fn compose_onto_degraded_stays_degraded() {
        let span = EditSpan {
            start_byte: 0,
            old_end_byte: 0,
            new_end_byte: 1,
            old_end_row: 0,
            old_end_column: 0,
        };
        assert_eq!(
            compose_pending(&doc("x"), PendingEdit::Degraded, span),
            PendingEdit::Degraded
        );
    }

    #[test]
    fn compose_inconsistent_points_degrade() {
        // A beyond-composition whose stored new-end point is inconsistent
        // with the incoming span's old-end point (row would go backwards)
        // must degrade instead of reporting a wrong span.
        let prev = PendingSpan {
            span: EditSpan {
                start_byte: 0,
                old_end_byte: 0,
                new_end_byte: 5,
                old_end_row: 0,
                old_end_column: 0,
            },
            // Claims the pending new end sits on row 2...
            new_end_row: 2,
            new_end_column: 0,
        };
        let span = EditSpan {
            start_byte: 6,
            old_end_byte: 7,
            new_end_byte: 6,
            // ...but the later old end reports row 0: underflow -> degrade.
            old_end_row: 0,
            old_end_column: 7,
        };
        assert_eq!(
            compose_pending(&doc("0123456789"), PendingEdit::Exact(prev), span),
            PendingEdit::Degraded
        );
    }

    #[test]
    fn compose_unresolvable_new_end_degrades() {
        // The composed new end must resolve in the supplied document; a
        // document shorter than the span's new end degrades the state.
        let span = EditSpan {
            start_byte: 0,
            old_end_byte: 0,
            new_end_byte: 10,
            old_end_row: 0,
            old_end_column: 0,
        };
        assert_eq!(
            compose_pending(&doc("ab"), PendingEdit::None, span),
            PendingEdit::Degraded
        );
    }
}
