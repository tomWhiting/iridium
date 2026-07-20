//! Pure edit-delta adjustment for tree-sitter highlight spans.
//!
//! Kept free of `wasm-bindgen`/`web-sys` (like [`crate::edit_tracking`]) so
//! it compiles and its unit tests run on every target, including plain
//! `cargo test` on the host — `WebEditor` itself is wasm32-only and cannot
//! be exercised without a browser.
//!
//! # The defect this closes
//!
//! `WebEditor::setTreeSitterHighlights` stores spans as ABSOLUTE byte
//! offsets into the document. Every edit (keystroke, paste, delete, remote
//! apply, undo/redo) shifts every byte after the edit point, but nothing
//! previously re-positioned the stored spans to match. Between an edit's
//! synchronous render and the highlight worker's asynchronous replacement
//! spans landing (a few milliseconds later, off the main thread), the
//! renderer sliced the CURRENT (post-edit) document using the STALE
//! (pre-edit) byte offsets — coloring the wrong characters for one or more
//! frames. That misalignment is the flicker: text appears to flash to
//! wrong colors, then "recolor" correctly once the worker's fresh spans
//! arrive and overwrite the stale set.
//!
//! [`shift_span`] is the fix: called for every tracked span at the exact
//! point an edit is recorded (`WebEditor::record_edit`, the single choke
//! point every content mutation — local keystrokes, paste, cut, delete,
//! `applyRemoteEdit`, undo/redo — already funnels through), it either
//! carries the span forward at its correct post-edit position (untouched,
//! or shifted by the edit's exact byte delta) or drops it when the edit
//! overlaps the span's own range (its post-edit shape is genuinely
//! unknown — dropping is honest, guessing would not be). The document
//! never renders with a byte-misaligned span; the worst case is a
//! momentarily-absent color for the handful of characters the edit
//! actually touched, until the fresh parse arrives.

/// Adjusts one highlight span `[start, end)` across an edit that replaced
/// `[edit_start, edit_old_end)` with `edit_new_end - edit_start` bytes of
/// new content.
///
/// Returns:
/// - `Some((start, end))` unchanged when the span lies entirely before the
///   edit (`end <= edit_start`) — untouched bytes, untouched span.
/// - `Some((shifted_start, shifted_end))` when the span lies entirely at or
///   after the edit's old end (`start >= edit_old_end`) — the same bytes,
///   now at a new position; shifted by the edit's exact byte delta
///   (`edit_new_end - edit_old_end`, positive for a net insertion,
///   negative for a net deletion).
/// - `None` when the span overlaps the edited range at all. The edit
///   changed bytes the span covered, so its post-edit shape cannot be
///   known without a reparse — dropping it (rather than guessing a shape)
///   is the honest outcome; the next grammar pass replaces it.
///
/// Byte offsets are `usize`; the arithmetic here never overflows or
/// underflows for a well-formed edit: `edit_new_end >= edit_start` always
/// holds (an edit's new end can never precede its own start), so a span
/// with `start >= edit_old_end` shifted by a net deletion
/// (`edit_old_end - edit_new_end`) never goes below `edit_new_end >=
/// edit_start >= 0`.
#[must_use]
pub const fn shift_span(
    start: usize,
    end: usize,
    edit_start: usize,
    edit_old_end: usize,
    edit_new_end: usize,
) -> Option<(usize, usize)> {
    if end <= edit_start {
        return Some((start, end));
    }
    if start >= edit_old_end {
        return Some(if edit_new_end >= edit_old_end {
            let delta = edit_new_end - edit_old_end;
            (start + delta, end + delta)
        } else {
            let delta = edit_old_end - edit_new_end;
            (start - delta, end - delta)
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::shift_span;

    // ===== Untouched: span entirely before the edit =====

    #[test]
    fn span_before_edit_is_unchanged_on_insertion() {
        // "keyword" at [0, 7); insert 1 byte at [10, 10).
        assert_eq!(shift_span(0, 7, 10, 10, 11), Some((0, 7)));
    }

    #[test]
    fn span_before_edit_is_unchanged_on_deletion() {
        // "keyword" at [0, 7); delete 3 bytes at [10, 13).
        assert_eq!(shift_span(0, 7, 10, 13, 10), Some((0, 7)));
    }

    #[test]
    fn span_ending_exactly_at_edit_start_is_unchanged() {
        // end == edit_start: the boundary belongs to "before", not overlap.
        assert_eq!(shift_span(0, 10, 10, 10, 11), Some((0, 10)));
    }

    // ===== Shifted: span entirely at/after the edit's old end =====

    #[test]
    fn span_after_insertion_shifts_forward_by_inserted_length() {
        // "keyword" at [10, 17); insert 1 byte at [0, 0).
        assert_eq!(shift_span(10, 17, 0, 0, 1), Some((11, 18)));
    }

    #[test]
    fn span_after_multi_byte_insertion_shifts_by_the_full_delta() {
        // Typing "é" (2 UTF-8 bytes) at byte 0.
        assert_eq!(shift_span(10, 17, 0, 0, 2), Some((12, 19)));
    }

    #[test]
    fn span_after_deletion_shifts_backward_by_deleted_length() {
        // "keyword" at [10, 17); backspace deletes 1 byte at [5, 6).
        assert_eq!(shift_span(10, 17, 5, 6, 5), Some((9, 16)));
    }

    #[test]
    fn span_starting_exactly_at_edit_old_end_shifts() {
        // start == edit_old_end: the boundary belongs to "after", not overlap.
        assert_eq!(shift_span(10, 20, 5, 10, 11), Some((11, 21)));
    }

    #[test]
    fn span_after_replace_with_equal_length_is_unchanged_in_position() {
        // A same-length replace (delta == 0) leaves later spans in place.
        assert_eq!(shift_span(10, 17, 0, 3, 3), Some((10, 17)));
    }

    #[test]
    fn a_run_of_spans_all_shift_consistently_after_an_insertion() {
        let edit = (5usize, 5usize, 6usize); // insert 1 byte at offset 5
        let spans = [(0, 3), (5, 8), (8, 12), (20, 25)];
        let shifted: Vec<_> = spans
            .into_iter()
            .map(|(s, e)| shift_span(s, e, edit.0, edit.1, edit.2).expect("no overlap"))
            .collect();
        assert_eq!(shifted, vec![(0, 3), (6, 9), (9, 13), (21, 26)]);
    }

    // ===== Dropped: span overlaps the edited range =====

    #[test]
    fn span_containing_the_whole_edit_is_dropped() {
        // "keyword" at [0, 20); edit replaces [5, 10).
        assert_eq!(shift_span(0, 20, 5, 10, 12), None);
    }

    #[test]
    fn span_overlapping_the_edit_start_is_dropped() {
        // span [5, 15); edit replaces [10, 20).
        assert_eq!(shift_span(5, 15, 10, 20, 21), None);
    }

    #[test]
    fn span_overlapping_the_edit_end_is_dropped() {
        // span [15, 25); edit replaces [10, 20).
        assert_eq!(shift_span(15, 25, 10, 20, 21), None);
    }

    #[test]
    fn span_exactly_equal_to_the_edited_range_is_dropped() {
        assert_eq!(shift_span(10, 20, 10, 20, 25), None);
    }

    #[test]
    fn zero_width_span_at_the_edit_point_is_dropped_not_kept() {
        // A degenerate [n, n) span at the exact edit point: end <= edit_start
        // is false (10 <= 10 is TRUE actually) -- covered by the boundary
        // test above; this checks a genuinely zero-width span strictly
        // inside a non-empty edit range never survives as ambiguous.
        assert_eq!(shift_span(12, 12, 10, 20, 25), None);
    }

    // ===== Whole-document edits (undo/redo, content swap tracking) =====

    #[test]
    fn a_whole_document_edit_drops_every_pre_existing_span() {
        // record_whole_document_edit always spans [0, pre_len) -> every
        // existing span overlaps and must be dropped; nothing survives
        // stale across an undo/redo whose new content is unrelated.
        let pre_len = 50;
        let spans = [(0, 5), (10, 15), (45, 50)];
        for (s, e) in spans {
            assert_eq!(shift_span(s, e, 0, pre_len, 30), None);
        }
    }
}
