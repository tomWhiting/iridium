//! Document-state-level tests for the whole-line operations: move
//! (Alt+Up/Down), duplicate (Shift+Alt+Up/Down), delete (Ctrl+Shift+K), and
//! join (Ctrl+J).
//!
//! Every test applies the produced command to a real document and asserts
//! both the resulting text and every cursor position; each operation's undo
//! test asserts that the command's inverse restores the text *and* the full
//! multi-cursor state.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::tests::{cursors_at, cursors_with, heads};
use super::*;
use crate::editor::CaretScopes;

/// All cursor selections in `all_selections` order, as
/// `((anchor.line, anchor.column), (head.line, head.column))` pairs.
fn selections(cursor: &CursorState) -> Vec<((usize, usize), (usize, usize))> {
    cursor
        .all_selections()
        .map(|sel| {
            (
                (sel.anchor.line, sel.anchor.column),
                (sel.head.line, sel.head.column),
            )
        })
        .collect()
}

/// Handles a key event and applies the resulting command (if any),
/// returning the command for undo assertions. `None` means the key was
/// handled as a no-op.
fn press(
    handler: &mut KeyboardHandler,
    event: &KeyEvent,
    document: &mut Document,
    cursor: &mut CursorState,
) -> Option<Command> {
    let result = handler.handle_key(
        event,
        document,
        cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
        &CaretScopes::none(),
    );
    match result {
        KeyResult::Command(cmd) => {
            cmd.apply(document, cursor).expect("command must apply");
            Some(cmd)
        },
        KeyResult::Handled => None,
        other => panic!("expected a command or Handled, got {other:?}"),
    }
}

const ALT: Modifiers = Modifiers {
    shift: false,
    ctrl: false,
    alt: true,
    meta: false,
    alt_graph: false,
};
const SHIFT_ALT: Modifiers = Modifiers {
    shift: true,
    ctrl: false,
    alt: true,
    meta: false,
    alt_graph: false,
};

const MOVE_UP: KeyEvent = KeyEvent::new(KeyCode::Up, ALT);
const MOVE_DOWN: KeyEvent = KeyEvent::new(KeyCode::Down, ALT);
const DUP_UP: KeyEvent = KeyEvent::new(KeyCode::Up, SHIFT_ALT);
const DUP_DOWN: KeyEvent = KeyEvent::new(KeyCode::Down, SHIFT_ALT);
const DELETE_LINES: KeyEvent = KeyEvent::new(KeyCode::Char('k'), Modifiers::ctrl_shift());
const JOIN_LINES: KeyEvent = KeyEvent::new(KeyCode::Char('j'), Modifiers::ctrl());

// ========== Move lines ==========

#[test]
fn move_line_down_single_cursor() {
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &MOVE_DOWN, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "bb\naa\ncc");
    assert_eq!(heads(&cursor), vec![(1, 1)]);
}

#[test]
fn move_line_up_single_cursor() {
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_at(&[(1, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &MOVE_UP, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "bb\naa\ncc");
    assert_eq!(heads(&cursor), vec![(0, 1)]);
}

#[test]
fn move_selection_down_selection_rides_with_text() {
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 1), Position::new(1, 1))]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &MOVE_DOWN, &mut doc, &mut cursor);

    // The two-line block swaps with the line below it; the selection keeps
    // covering the same text.
    assert_eq!(doc.text(), "cc\naa\nbb");
    assert_eq!(selections(&cursor), vec![((1, 1), (2, 1))]);
}

#[test]
fn move_up_at_top_is_noop() {
    let mut doc = Document::new("aa\nbb");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    let cmd = press(&mut handler, &MOVE_UP, &mut doc, &mut cursor);

    assert!(cmd.is_none(), "moving the top line up must be a no-op");
    assert_eq!(doc.text(), "aa\nbb");
    assert_eq!(heads(&cursor), vec![(0, 1)]);
}

#[test]
fn move_down_at_bottom_is_noop() {
    let mut doc = Document::new("aa\nbb");
    let mut cursor = cursors_at(&[(1, 1)]);
    let mut handler = KeyboardHandler::new();

    let cmd = press(&mut handler, &MOVE_DOWN, &mut doc, &mut cursor);

    assert!(cmd.is_none(), "moving the bottom line down must be a no-op");
    assert_eq!(doc.text(), "aa\nbb");
    assert_eq!(heads(&cursor), vec![(1, 1)]);
}

#[test]
fn move_multi_cursor_same_line_moves_it_once() {
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_at(&[(1, 0), (1, 2)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &MOVE_UP, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "bb\naa\ncc");
    assert_eq!(heads(&cursor), vec![(0, 0), (0, 2)]);
}

#[test]
fn move_adjacent_cursor_blocks_merge_and_move_as_one() {
    let mut doc = Document::new("aa\nbb\ncc\ndd");
    let mut cursor = cursors_at(&[(1, 1), (2, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &MOVE_UP, &mut doc, &mut cursor);

    // Lines 1 and 2 form one colliding block and swap with line 0 together.
    assert_eq!(doc.text(), "bb\ncc\naa\ndd");
    assert_eq!(heads(&cursor), vec![(0, 1), (1, 1)]);
}

#[test]
fn move_up_block_at_edge_stays_while_other_block_moves() {
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_at(&[(0, 0), (2, 0)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &MOVE_UP, &mut doc, &mut cursor);

    // The top block cannot move; the other block still swaps up.
    assert_eq!(doc.text(), "aa\ncc\nbb");
    assert_eq!(heads(&cursor), vec![(0, 0), (1, 0)]);
}

#[test]
fn move_separate_blocks_move_independently() {
    let mut doc = Document::new("aa\nbb\ncc\ndd");
    let mut cursor = cursors_at(&[(0, 0), (2, 0)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &MOVE_DOWN, &mut doc, &mut cursor);

    // Blocks with a gap between them do not merge: each swaps with its own
    // adjacent line (the second block's displaced line is the last line).
    assert_eq!(doc.text(), "bb\naa\ndd\ncc");
    assert_eq!(heads(&cursor), vec![(1, 0), (3, 0)]);
}

#[test]
fn move_selection_ending_at_column_zero_excludes_that_line() {
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_with(&[Selection::new(Position::new(1, 0), Position::new(2, 0))]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &MOVE_UP, &mut doc, &mut cursor);

    // The line under the trailing column-0 caret is not part of the block:
    // only line 1 moves, and the selection still covers "bb\n".
    assert_eq!(doc.text(), "bb\naa\ncc");
    assert_eq!(selections(&cursor), vec![((0, 0), (1, 0))]);
}

#[test]
fn move_down_onto_last_line_consumes_ending_correctly() {
    let mut doc = Document::new("aa\nbb");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &MOVE_DOWN, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "bb\naa");
    assert_eq!(heads(&cursor), vec![(1, 1)]);
}

#[test]
fn move_up_mixed_endings_reinserts_exact_separator_and_caret_rides() {
    // Document line ending detects as CRLF, but the displaced "b" line is
    // separated from the block by a one-byte LF: that exact separator must
    // be reinserted and drive the cursor delta, or the caret lands inside
    // the first CRLF instead of on the moved "c".
    let mut doc = Document::new("a\r\nb\nc");
    let mut cursor = cursors_at(&[(2, 0)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &MOVE_UP, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "a\r\nc\nb");
    assert_eq!(heads(&cursor), vec![(1, 0)]);
}

#[test]
fn move_up_interior_mixed_endings_displaced_line_keeps_its_own_ending() {
    // Interior Alt+Up on a mixed-ending document: the displaced "aa" line
    // owns a CRLF and the moved "bb" line owns a bare LF. Each line must
    // keep its own ending — "bb\naa\r\ncc", the exact mirror of the Down
    // branch — not swap endings ("bb\r\naa\ncc") by reinserting the
    // separator above the displaced content.
    let mut doc = Document::new("aa\r\nbb\ncc");
    let mut cursor = cursors_at(&[(1, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &MOVE_UP, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "bb\naa\r\ncc");
    assert_eq!(heads(&cursor), vec![(0, 1)]);
}

#[test]
fn move_down_then_up_mixed_endings_restores_exact_bytes() {
    // Alt+Down followed by Alt+Up must be a byte-for-byte round trip on a
    // mixed-ending document (and restore the caret), not silently reassign
    // which lines are LF vs CRLF.
    let mut doc = Document::new("aa\r\nbb\ncc");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &MOVE_DOWN, &mut doc, &mut cursor);
    assert_eq!(doc.text(), "bb\naa\r\ncc");
    assert_eq!(heads(&cursor), vec![(1, 1)]);

    press(&mut handler, &MOVE_UP, &mut doc, &mut cursor);
    assert_eq!(doc.text(), "aa\r\nbb\ncc");
    assert_eq!(heads(&cursor), vec![(0, 1)]);
}

#[test]
fn move_down_mixed_endings_displaced_line_keeps_its_own_ending() {
    // Dominant ending is CRLF but the displaced "c" line carries a bare LF:
    // moving "b" down must keep that LF with "c" (not widen it to CRLF)
    // and shift the caret by the real two removed bytes.
    let mut doc = Document::new("a\r\nb\r\nc\nd\r\ne");
    let mut cursor = cursors_at(&[(1, 0)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &MOVE_DOWN, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "a\r\nc\nb\r\nd\r\ne");
    assert_eq!(heads(&cursor), vec![(2, 0)]);
}

#[test]
fn move_down_onto_last_line_mixed_endings_reuses_blocks_own_ending() {
    // The block's own trailing ending is a bare LF while the document
    // detects CRLF: swapping with the (ending-less) last line must reuse
    // those exact LF bytes as the new separator.
    let mut doc = Document::new("a\r\nb\r\nc\nd");
    let mut cursor = cursors_at(&[(2, 0)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &MOVE_DOWN, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "a\r\nb\r\nd\nc");
    assert_eq!(heads(&cursor), vec![(3, 0)]);
}

#[test]
fn undo_move_restores_text_and_all_cursors() {
    let mut doc = Document::new("aa\nbb\ncc");
    let original = cursors_at(&[(0, 1), (1, 1)]);
    let mut cursor = original.clone();
    let mut handler = KeyboardHandler::new();

    let cmd = press(&mut handler, &MOVE_DOWN, &mut doc, &mut cursor)
        .expect("move must produce a command");

    assert_eq!(doc.text(), "cc\naa\nbb");
    assert_eq!(heads(&cursor), vec![(1, 1), (2, 1)]);

    cmd.inverse()
        .apply(&mut doc, &mut cursor)
        .expect("inverse must apply");
    assert_eq!(doc.text(), "aa\nbb\ncc");
    assert_eq!(cursor, original);
}

// ========== Duplicate lines/selections ==========

#[test]
fn duplicate_down_keeps_cursor_on_the_copy() {
    let mut doc = Document::new("abc");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &DUP_DOWN, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "abc\nabc");
    assert_eq!(heads(&cursor), vec![(1, 1)]);
}

#[test]
fn duplicate_up_keeps_cursor_on_the_original() {
    let mut doc = Document::new("abc");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &DUP_UP, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "abc\nabc");
    assert_eq!(heads(&cursor), vec![(0, 1)]);
}

#[test]
fn duplicate_selection_down_selects_the_copy() {
    let mut doc = Document::new("abcdef");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 1), Position::new(0, 3))]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &DUP_DOWN, &mut doc, &mut cursor);

    // The selected text is duplicated inline; the selection covers the copy.
    assert_eq!(doc.text(), "abcbcdef");
    assert_eq!(selections(&cursor), vec![((0, 3), (0, 5))]);
}

#[test]
fn duplicate_selection_up_keeps_selection_on_the_original() {
    let mut doc = Document::new("abcdef");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 1), Position::new(0, 3))]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &DUP_UP, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "abcbcdef");
    assert_eq!(selections(&cursor), vec![((0, 1), (0, 3))]);
}

#[test]
fn duplicate_backward_selection_down_preserves_orientation() {
    let mut doc = Document::new("abcdef");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 3), Position::new(0, 1))]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &DUP_DOWN, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "abcbcdef");
    assert_eq!(selections(&cursor), vec![((0, 5), (0, 3))]);
}

#[test]
fn duplicate_multiline_selection_inline() {
    let mut doc = Document::new("aa\nbb");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 1), Position::new(1, 1))]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &DUP_DOWN, &mut doc, &mut cursor);

    // "a\nb" is duplicated at the selection end; the copy is selected.
    assert_eq!(doc.text(), "aa\nba\nbb");
    assert_eq!(selections(&cursor), vec![((1, 1), (2, 1))]);
}

#[test]
fn duplicate_down_multi_cursor_same_line_duplicates_once() {
    let mut doc = Document::new("abc");
    let mut cursor = cursors_at(&[(0, 1), (0, 2)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &DUP_DOWN, &mut doc, &mut cursor);

    // Two cursors on one line duplicate it once; both land on the copy at
    // their own columns.
    assert_eq!(doc.text(), "abc\nabc");
    assert_eq!(heads(&cursor), vec![(1, 1), (1, 2)]);
}

#[test]
fn duplicate_up_multi_cursor_same_line_duplicates_once() {
    let mut doc = Document::new("abc");
    let mut cursor = cursors_at(&[(0, 1), (0, 2)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &DUP_UP, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "abc\nabc");
    assert_eq!(heads(&cursor), vec![(0, 1), (0, 2)]);
}

#[test]
fn duplicate_down_multiple_lines_each_cursor_lands_on_its_copy() {
    let mut doc = Document::new("aa\nbb");
    let mut cursor = cursors_at(&[(0, 1), (1, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &DUP_DOWN, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "aa\naa\nbb\nbb");
    assert_eq!(heads(&cursor), vec![(1, 1), (3, 1)]);
}

#[test]
fn duplicate_last_line_without_trailing_newline() {
    let mut doc = Document::new("aa\nbb");
    let mut cursor = cursors_at(&[(1, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &DUP_DOWN, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "aa\nbb\nbb");
    assert_eq!(heads(&cursor), vec![(2, 1)]);
}

#[test]
fn undo_duplicate_restores_text_and_all_cursors() {
    let mut doc = Document::new("aa\nbb");
    let original = cursors_at(&[(0, 1), (1, 1)]);
    let mut cursor = original.clone();
    let mut handler = KeyboardHandler::new();

    let cmd = press(&mut handler, &DUP_DOWN, &mut doc, &mut cursor)
        .expect("duplicate must produce a command");

    assert_eq!(doc.text(), "aa\naa\nbb\nbb");

    cmd.inverse()
        .apply(&mut doc, &mut cursor)
        .expect("inverse must apply");
    assert_eq!(doc.text(), "aa\nbb");
    assert_eq!(cursor, original);
}

// ========== Delete lines ==========

#[test]
fn delete_line_caret_lands_on_next_line_same_column() {
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_at(&[(1, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &DELETE_LINES, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "aa\ncc");
    assert_eq!(heads(&cursor), vec![(1, 1)]);
}

#[test]
fn delete_line_clamps_column_to_replacing_line() {
    let mut doc = Document::new("aaaa\nbb");
    let mut cursor = cursors_at(&[(0, 3)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &DELETE_LINES, &mut doc, &mut cursor);

    // The replacing line "bb" is shorter than column 3: clamp to its end.
    assert_eq!(doc.text(), "bb");
    assert_eq!(heads(&cursor), vec![(0, 2)]);
}

#[test]
fn delete_last_line_consumes_preceding_ending() {
    let mut doc = Document::new("aa\nbb");
    let mut cursor = cursors_at(&[(1, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &DELETE_LINES, &mut doc, &mut cursor);

    // Deleting the last line removes the ending before it; the caret lands
    // on the line above at the same column.
    assert_eq!(doc.text(), "aa");
    assert_eq!(heads(&cursor), vec![(0, 1)]);
}

#[test]
fn delete_only_line_leaves_empty_document() {
    let mut doc = Document::new("aa");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &DELETE_LINES, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "");
    assert_eq!(heads(&cursor), vec![(0, 0)]);
}

#[test]
fn delete_multi_cursor_same_line_deletes_it_once() {
    let mut doc = Document::new("aa\nbbb");
    let mut cursor = cursors_at(&[(0, 1), (0, 2)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &DELETE_LINES, &mut doc, &mut cursor);

    // One deletion; both carets keep their own columns on the new line.
    assert_eq!(doc.text(), "bbb");
    assert_eq!(heads(&cursor), vec![(0, 1), (0, 2)]);
}

#[test]
fn delete_adjacent_cursor_lines_as_one_block() {
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_at(&[(0, 1), (1, 0)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &DELETE_LINES, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "cc");
    assert_eq!(heads(&cursor), vec![(0, 1), (0, 0)]);
}

#[test]
fn delete_selection_removes_every_touched_line() {
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 1), Position::new(1, 1))]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &DELETE_LINES, &mut doc, &mut cursor);

    // Both touched lines are removed and the selection collapses.
    assert_eq!(doc.text(), "cc");
    assert_eq!(selections(&cursor), vec![((0, 1), (0, 1))]);
}

#[test]
fn delete_selection_ending_at_column_zero_excludes_that_line() {
    let mut doc = Document::new("aa\nbb");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(1, 0))]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &DELETE_LINES, &mut doc, &mut cursor);

    // The full-line selection touches only line 0.
    assert_eq!(doc.text(), "bb");
    assert_eq!(heads(&cursor), vec![(0, 0)]);
}

#[test]
fn undo_delete_lines_restores_text_and_all_cursors() {
    let mut doc = Document::new("aaa\nbb\ncc");
    let original = cursors_at(&[(0, 2), (1, 0)]);
    let mut cursor = original.clone();
    let mut handler = KeyboardHandler::new();

    let cmd = press(&mut handler, &DELETE_LINES, &mut doc, &mut cursor)
        .expect("delete lines must produce a command");

    assert_eq!(doc.text(), "cc");

    cmd.inverse()
        .apply(&mut doc, &mut cursor)
        .expect("inverse must apply");
    assert_eq!(doc.text(), "aaa\nbb\ncc");
    assert_eq!(cursor, original);
}

// ========== Join lines ==========

#[test]
fn join_replaces_ending_with_space_and_puts_caret_at_join_point() {
    let mut doc = Document::new("foo\nbar");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &JOIN_LINES, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "foo bar");
    assert_eq!(heads(&cursor), vec![(0, 3)]);
}

#[test]
fn join_strips_leading_whitespace_of_joined_line() {
    let mut doc = Document::new("foo\n   bar");
    let mut cursor = cursors_at(&[(0, 2)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &JOIN_LINES, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "foo bar");
    assert_eq!(heads(&cursor), vec![(0, 3)]);
}

#[test]
fn join_on_last_line_is_noop() {
    let mut doc = Document::new("foo\nbar");
    let mut cursor = cursors_at(&[(1, 1)]);
    let mut handler = KeyboardHandler::new();

    let cmd = press(&mut handler, &JOIN_LINES, &mut doc, &mut cursor);

    assert!(cmd.is_none(), "joining on the last line must be a no-op");
    assert_eq!(doc.text(), "foo\nbar");
    assert_eq!(heads(&cursor), vec![(1, 1)]);
}

#[test]
fn join_multi_cursor_joins_each_line_with_the_next() {
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_at(&[(0, 0), (1, 0)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &JOIN_LINES, &mut doc, &mut cursor);

    // Both joins happen in one command; each caret lands on its join point.
    assert_eq!(doc.text(), "aa bb cc");
    assert_eq!(heads(&cursor), vec![(0, 2), (0, 5)]);
}

#[test]
fn join_two_cursors_on_same_line_join_once_and_merge() {
    let mut doc = Document::new("aa\nbb");
    let mut cursor = cursors_at(&[(0, 0), (0, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &JOIN_LINES, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "aa bb");
    // Both carets converge on the single join point and merge.
    assert_eq!(heads(&cursor), vec![(0, 2)]);
}

#[test]
fn join_multiline_selection_joins_span_and_keeps_selection() {
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 1), Position::new(2, 1))]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &JOIN_LINES, &mut doc, &mut cursor);

    // Every line the selection spans is joined; the selection keeps
    // covering the same text.
    assert_eq!(doc.text(), "aa bb cc");
    assert_eq!(selections(&cursor), vec![((0, 1), (0, 7))]);
}

#[test]
fn join_cursor_on_last_line_rides_earlier_join() {
    let mut doc = Document::new("aa\nbb");
    let mut cursor = cursors_at(&[(0, 0), (1, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &JOIN_LINES, &mut doc, &mut cursor);

    // The first cursor joins; the last-line cursor has nothing to join and
    // keeps its position within the (now joined) text.
    assert_eq!(doc.text(), "aa bb");
    assert_eq!(heads(&cursor), vec![(0, 2), (0, 4)]);
}

#[test]
fn join_selection_endpoint_at_indented_line_start_lands_after_space() {
    // The selection's head sits at the joined line's column zero, inside
    // the indentation that the join strips. The selected newline became
    // the inserted space, so the selection must end after it, covering
    // "aa " — not floor before the space and cover only "aa".
    let mut doc = Document::new("aa\n  bb");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(1, 0))]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &JOIN_LINES, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "aa bb");
    assert_eq!(selections(&cursor), vec![((0, 0), (0, 3))]);
}

#[test]
fn join_backward_selection_endpoint_at_indented_line_start_lands_after_space() {
    let mut doc = Document::new("aa\n  bb");
    let mut cursor = cursors_with(&[Selection::new(Position::new(1, 0), Position::new(0, 0))]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &JOIN_LINES, &mut doc, &mut cursor);

    // Same coverage with the opposite orientation: the anchor lands after
    // the inserted space.
    assert_eq!(doc.text(), "aa bb");
    assert_eq!(selections(&cursor), vec![((0, 3), (0, 0))]);
}

#[test]
fn join_selection_endpoint_inside_stripped_whitespace_floors_after_space() {
    // An endpoint strictly inside the stripped indentation has no exact
    // post-edit image; it floors where the whitespace was — just after the
    // inserted space.
    let mut doc = Document::new("aa\n  bb");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(1, 1))]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &JOIN_LINES, &mut doc, &mut cursor);

    assert_eq!(doc.text(), "aa bb");
    assert_eq!(selections(&cursor), vec![((0, 0), (0, 3))]);
}

#[test]
fn undo_join_restores_text_and_all_cursors() {
    let mut doc = Document::new("foo\n  bar\nbaz");
    let original = cursors_at(&[(0, 1), (1, 4)]);
    let mut cursor = original.clone();
    let mut handler = KeyboardHandler::new();

    let cmd = press(&mut handler, &JOIN_LINES, &mut doc, &mut cursor)
        .expect("join must produce a command");

    assert_eq!(doc.text(), "foo bar baz");

    cmd.inverse()
        .apply(&mut doc, &mut cursor)
        .expect("inverse must apply");
    assert_eq!(doc.text(), "foo\n  bar\nbaz");
    assert_eq!(cursor, original);
}

// ========== Dispatch: compound modifier chords must pass through ==========

const CTRL_ALT: Modifiers = Modifiers {
    shift: false,
    ctrl: true,
    alt: true,
    meta: false,
    alt_graph: false,
};
const CTRL_META: Modifiers = Modifiers {
    shift: false,
    ctrl: true,
    alt: false,
    meta: true,
    alt_graph: false,
};
const CTRL_SHIFT_ALT: Modifiers = Modifiers {
    shift: true,
    ctrl: true,
    alt: true,
    meta: false,
    alt_graph: false,
};
const CTRL_SHIFT_META: Modifiers = Modifiers {
    shift: true,
    ctrl: true,
    alt: false,
    meta: true,
    alt_graph: false,
};

#[test]
fn ctrl_chords_with_alt_or_meta_do_not_trigger_line_ops() {
    // Ctrl+Alt / Ctrl+Meta chords are reserved for the host/OS, and AltGr
    // layouts report character composition as Ctrl+Alt: none of these may
    // join or delete lines — they must be ignored so the host sees them,
    // and the document must be untouched.
    let doc = Document::new("aa\n  bb\ncc");
    let cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    for event in [
        KeyEvent::new(KeyCode::Char('j'), CTRL_ALT),
        KeyEvent::new(KeyCode::Char('j'), CTRL_META),
        KeyEvent::new(KeyCode::Char('J'), CTRL_ALT),
        KeyEvent::new(KeyCode::Char('k'), CTRL_SHIFT_ALT),
        KeyEvent::new(KeyCode::Char('k'), CTRL_SHIFT_META),
        KeyEvent::new(KeyCode::Char('K'), CTRL_SHIFT_ALT),
    ] {
        let result = handler.handle_key(
            &event,
            &doc,
            &cursor,
            &UndoTree::new(),
            &EditorConfig::default(),
            &CaretScopes::none(),
        );
        assert!(
            matches!(result, KeyResult::Ignored),
            "chord {event:?} must pass through as Ignored, got {result:?}"
        );
    }
    assert_eq!(doc.text(), "aa\n  bb\ncc");
    assert_eq!(heads(&cursor), vec![(0, 1)]);
}
