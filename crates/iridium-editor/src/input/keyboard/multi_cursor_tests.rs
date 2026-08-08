//! Document-state-level tests for the multi-cursor verbs: add cursor
//! above/below (Ctrl+Alt+Up/Down), select all occurrences (Ctrl+Shift+L),
//! undo last cursor (Ctrl+U), and skip occurrence
//! ([`KeyboardHandler::skip_last_added_occurrence`]).
//!
//! Every test drives the real [`KeyboardHandler`], applies the produced
//! command to a document and cursor, and asserts the full cursor state.
//! Undo/redo tests apply the command's inverse and re-apply it to prove the
//! `SetSelection` roundtrips the cursor state exactly.

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

/// Handles a key event and applies the resulting command (if any), returning
/// the command for undo assertions. `None` means a no-op (Handled).
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

/// Calls the skip verb directly (it has no key binding) and applies the
/// resulting command, returning it. `None` means a no-op (Handled).
fn skip(
    handler: &mut KeyboardHandler,
    document: &mut Document,
    cursor: &mut CursorState,
) -> Option<Command> {
    let result = handler.skip_last_added_occurrence(document, cursor);
    match result {
        KeyResult::Command(cmd) => {
            cmd.apply(document, cursor).expect("command must apply");
            Some(cmd)
        },
        KeyResult::Handled => None,
        other => panic!("expected a command or Handled, got {other:?}"),
    }
}

const CTRL: Modifiers = Modifiers {
    shift: false,
    ctrl: true,
    alt: false,
    meta: false,
    alt_graph: false,
};
const CTRL_ALT: Modifiers = Modifiers {
    shift: false,
    ctrl: true,
    alt: true,
    meta: false,
    alt_graph: false,
};
const CTRL_SHIFT: Modifiers = Modifiers {
    shift: true,
    ctrl: true,
    alt: false,
    meta: false,
    alt_graph: false,
};

const ADD_ABOVE: KeyEvent = KeyEvent::new(KeyCode::Up, CTRL_ALT);
const ADD_BELOW: KeyEvent = KeyEvent::new(KeyCode::Down, CTRL_ALT);
const ADD_NEXT: KeyEvent = KeyEvent::new(KeyCode::Char('d'), CTRL);
const SELECT_ALL_OCC: KeyEvent = KeyEvent::new(KeyCode::Char('l'), CTRL_SHIFT);
const UNDO_CURSOR: KeyEvent = KeyEvent::new(KeyCode::Char('u'), CTRL);
const ESCAPE: KeyEvent = KeyEvent::simple(KeyCode::Escape);

/// Lines of length 10 / 2 / 10 for exercising sticky columns across a short
/// middle line.
const STICKY_DOC: &str = "aaaaaaaaaa\nbb\ncccccccccc";

// ========== Add cursor above/below ==========

#[test]
fn add_below_is_sticky_across_a_short_line() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new(STICKY_DOC);
    let mut cursor = cursors_at(&[(0, 8)]);

    // First add clamps to the short middle line.
    assert!(press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor).is_some());
    assert_eq!(heads(&cursor), vec![(0, 8), (1, 2)]);

    // Second add: the clamped cursor's sticky column (8) lands on the long
    // third line, not the clamped column (2).
    assert!(press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor).is_some());
    assert_eq!(heads(&cursor), vec![(0, 8), (1, 2), (2, 8)]);
}

#[test]
fn add_above_is_sticky_across_a_short_line() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new(STICKY_DOC);
    let mut cursor = cursors_at(&[(2, 8)]);

    assert!(press(&mut handler, &ADD_ABOVE, &mut doc, &mut cursor).is_some());
    assert_eq!(heads(&cursor), vec![(2, 8), (1, 2)]);

    assert!(press(&mut handler, &ADD_ABOVE, &mut doc, &mut cursor).is_some());
    // The middle cursor's sticky 8 restores on the long first line.
    assert_eq!(heads(&cursor), vec![(2, 8), (0, 8), (1, 2)]);
}

#[test]
fn repeated_adds_extend_the_block_and_reverse_direction() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("l0\nl1\nl2\nl3\nl4");
    let mut cursor = cursors_at(&[(2, 1)]);

    press(&mut handler, &ADD_ABOVE, &mut doc, &mut cursor);
    press(&mut handler, &ADD_ABOVE, &mut doc, &mut cursor);
    // Cloning every cursor each press grows the contiguous block upward.
    assert_eq!(cursor.cursor_count(), 3);
    assert_eq!(heads(&cursor), vec![(2, 1), (0, 1), (1, 1)]);

    // Reversing direction extends the block downward by one.
    press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor);
    assert_eq!(cursor.cursor_count(), 4);
    assert_eq!(heads(&cursor), vec![(2, 1), (0, 1), (1, 1), (3, 1)]);
}

#[test]
fn add_below_merges_colliding_clones() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aa\nbb\ncc\ndd");
    // Two adjacent cursors: cloning both down makes the upper clone land on
    // the lower cursor, which must merge (3 cursors, not 4).
    let mut cursor = cursors_at(&[(0, 1), (1, 1)]);

    assert!(press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 3);
    assert_eq!(heads(&cursor), vec![(0, 1), (1, 1), (2, 1)]);
}

#[test]
fn add_above_at_top_is_a_noop() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_at(&[(0, 0)]);

    // At the first line there is no line above and no wraparound.
    assert!(press(&mut handler, &ADD_ABOVE, &mut doc, &mut cursor).is_none());
    assert_eq!(cursor.cursor_count(), 1);
    assert_eq!(heads(&cursor), vec![(0, 0)]);
}

#[test]
fn add_below_at_bottom_is_a_noop() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_at(&[(2, 1)]);

    assert!(press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor).is_none());
    assert_eq!(cursor.cursor_count(), 1);
}

#[test]
fn add_above_only_the_non_edge_cursors_contribute() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aa\nbb\ncc");
    // Cursor on the top line contributes nothing; the one on line 2 clones up.
    let mut cursor = cursors_at(&[(0, 1), (2, 1)]);

    assert!(press(&mut handler, &ADD_ABOVE, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 3);
    assert_eq!(heads(&cursor), vec![(0, 1), (1, 1), (2, 1)]);
}

// ========== Select all occurrences ==========

#[test]
fn select_all_occurrences_from_selection() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo bar foo baz foo");
    // Select the first "foo".
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 3))]);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 3);
    // The current occurrence stays primary; the others become secondaries.
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 3)), ((0, 8), (0, 11)), ((0, 16), (0, 19)),]
    );
}

#[test]
fn select_all_occurrences_from_word_under_caret() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo bar foo");
    // Collapsed caret inside the first "foo".
    let mut cursor = cursors_at(&[(0, 1)]);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 2);
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 3)), ((0, 8), (0, 11))]
    );
}

#[test]
fn select_all_occurrences_is_non_overlapping() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aaa");
    // Selecting "aa" in "aaa" must match once (non-overlapping), like Ctrl+D —
    // the overlapping "aa" starting at column 1 is not a separate cursor.
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 2))]);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_none());
    assert_eq!(cursor.cursor_count(), 1);
}

#[test]
fn select_all_occurrences_finds_separated_matches() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aa xx aa xx aa");
    // Three separated "aa" matches become three cursors.
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 2))]);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 3);
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 2)), ((0, 6), (0, 8)), ((0, 12), (0, 14))]
    );
}

#[test]
fn select_all_occurrences_adjacent_matches_stay_distinct() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aaaa");
    // "aa" in "aaaa" yields two adjacent, non-overlapping matches (0-2, 2-4).
    // Each occurrence must remain its own edit target — building the cursor
    // state directly (not via `add_cursor`, whose adjacency merge would fuse
    // them) keeps them separate.
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 2))]);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 2);
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 2)), ((0, 2), (0, 4))]
    );
}

#[test]
fn select_all_occurrences_adjacent_matches_edit_independently() {
    // Regression for the reviewer's finding: selecting the first "foo" in
    // "foofoo" and pressing Ctrl+Shift+L must yield two occurrence cursors, so
    // typing "x" produces "xx" (two edits), not a single fused "x".
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foofoo");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 3))]);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 2);
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 3)), ((0, 3), (0, 6))]
    );

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Char('x')),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(doc.text(), "xx");
    assert_eq!(heads(&cursor), vec![(0, 1), (0, 2)]);
}

#[test]
fn select_all_occurrences_empty_source_is_noop() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("   foo");

    // Collapsed caret on whitespace: no word to select, so nothing happens.
    let mut cursor = cursors_at(&[(0, 0)]);
    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_none());
    assert_eq!(cursor.cursor_count(), 1);
}

#[test]
fn select_all_occurrences_honors_explicit_whitespace_selection() {
    // Regression for the reviewer's finding: Ctrl+D accepts a non-empty
    // whitespace selection, so Ctrl+Shift+L must too — selecting the first
    // space in "a b c" selects every space, not nothing.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("a b c");
    // The single space between "a" and "b" (offset 1..2).
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 1), Position::new(0, 2))]);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 2);
    assert_eq!(
        selections(&cursor),
        vec![((0, 1), (0, 2)), ((0, 3), (0, 4))]
    );
}

#[test]
fn select_all_occurrences_unique_word_selects_the_occurrence() {
    // Reviewer finding: with a collapsed caret on a uniquely occurring word,
    // Ctrl+Shift+L must still expand the caret to select that occurrence rather
    // than short-circuiting to a no-op because there are no *other* matches.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("hello world");
    let mut cursor = cursors_at(&[(0, 1)]);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 1);
    assert_eq!(selections(&cursor), vec![((0, 0), (0, 5))]);
}

#[test]
fn select_all_occurrences_drops_unrelated_cursor_when_reference_is_unique() {
    // Reviewer finding: a unique primary occurrence plus an unrelated secondary
    // (added by a vertical/mouse path) must collapse to just the reference
    // occurrence. Previously an empty secondary short-circuited to a no-op,
    // preserving the unrelated cursor so a subsequent keystroke would edit both
    // locations.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo xyz");
    // Primary selects the sole "foo"; an unrelated caret sits inside "xyz".
    let mut cursor = cursors_with(&[
        Selection::new(Position::new(0, 0), Position::new(0, 3)),
        Selection::collapsed(Position::new(0, 5)),
    ]);
    assert_eq!(cursor.cursor_count(), 2);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 1);
    assert_eq!(selections(&cursor), vec![((0, 0), (0, 3))]);

    // Typing now edits only the reference occurrence.
    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Char('Z')),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(doc.text(), "Z xyz");
}

// ========== AltGr (AltGraph) must not reach the add-cursor chord ==========

#[test]
fn altgraph_arrow_does_not_spawn_cursors() {
    // Reviewer finding: on non-US layouts AltGr is reported as Ctrl+Alt while
    // composing a character. An AltGraph-marked Up/Down must NOT be treated as
    // the add-cursor-above/below chord (which is Ctrl+Alt without AltGraph), or
    // it would spawn cursors and subsequent typing/deletion would apply at
    // multiple locations.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_at(&[(1, 1)]);

    let altgr_ctrl_alt = Modifiers {
        shift: false,
        ctrl: true,
        alt: true,
        meta: false,
        alt_graph: true,
    };
    let altgr_up = KeyEvent::new(KeyCode::Up, altgr_ctrl_alt);
    let altgr_down = KeyEvent::new(KeyCode::Down, altgr_ctrl_alt);

    // Excluded from the add-cursor dispatch, the AltGraph arrow falls through to
    // plain single-cursor vertical navigation. The essential guarantee is that
    // it never *spawns* a cursor, so a following keystroke cannot edit at
    // multiple locations.
    press(&mut handler, &altgr_up, &mut doc, &mut cursor);
    assert_eq!(cursor.cursor_count(), 1, "AltGr+Up must not add a cursor");
    assert_eq!(heads(&cursor), vec![(0, 1)]); // moved up, still one cursor

    press(&mut handler, &altgr_down, &mut doc, &mut cursor);
    assert_eq!(cursor.cursor_count(), 1, "AltGr+Down must not add a cursor");
    assert_eq!(heads(&cursor), vec![(1, 1)]);

    // The genuine Ctrl+Alt chord (AltGraph absent) still adds cursors.
    assert!(press(&mut handler, &ADD_ABOVE, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 2);
}

// ========== Undo last cursor (Ctrl+U) ==========

#[test]
fn undo_last_cursor_removes_most_recent_after_ctrl_d() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo foo foo");
    let mut cursor = cursors_at(&[(0, 1)]);

    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor); // select word "foo"
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor); // add second
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor); // add third
    assert_eq!(cursor.cursor_count(), 3);

    // Ctrl+U peels the most recently added cursor first.
    assert!(press(&mut handler, &UNDO_CURSOR, &mut doc, &mut cursor).is_some());
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 3)), ((0, 4), (0, 7))]
    );

    assert!(press(&mut handler, &UNDO_CURSOR, &mut doc, &mut cursor).is_some());
    assert_eq!(selections(&cursor), vec![((0, 0), (0, 3))]);

    // Single cursor: no-op.
    assert!(press(&mut handler, &UNDO_CURSOR, &mut doc, &mut cursor).is_none());
}

#[test]
fn undo_last_cursor_orders_across_add_above_and_below() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_at(&[(1, 1)]);

    press(&mut handler, &ADD_ABOVE, &mut doc, &mut cursor); // add (0,1)
    press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor); // add (2,1)
    assert_eq!(heads(&cursor), vec![(1, 1), (0, 1), (2, 1)]);

    // Most recent is the (2,1) added by ADD_BELOW.
    assert!(press(&mut handler, &UNDO_CURSOR, &mut doc, &mut cursor).is_some());
    assert_eq!(heads(&cursor), vec![(1, 1), (0, 1)]);

    assert!(press(&mut handler, &UNDO_CURSOR, &mut doc, &mut cursor).is_some());
    assert_eq!(heads(&cursor), vec![(1, 1)]);
}

#[test]
fn undo_last_cursor_single_cursor_is_noop() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("hello");
    let mut cursor = cursors_at(&[(0, 2)]);

    assert!(press(&mut handler, &UNDO_CURSOR, &mut doc, &mut cursor).is_none());
    assert_eq!(cursor.cursor_count(), 1);
}

#[test]
fn motion_invalidates_the_addition_order_stack() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_at(&[(1, 1)]);

    press(&mut handler, &ADD_ABOVE, &mut doc, &mut cursor);
    assert_eq!(cursor.cursor_count(), 2);

    // A plain motion moves the cursors without going through an add verb; the
    // addition-order stack is invalidated, so Ctrl+U becomes a no-op rather
    // than removing an arbitrary cursor.
    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Right),
        &mut doc,
        &mut cursor,
    );
    assert!(press(&mut handler, &UNDO_CURSOR, &mut doc, &mut cursor).is_none());
    assert_eq!(cursor.cursor_count(), 2);
}

// ========== Escape collapse ==========

#[test]
fn escape_collapses_to_primary_and_clears_stack() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo foo");
    let mut cursor = cursors_at(&[(0, 1)]);

    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor); // select "foo"
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor); // add second
    assert_eq!(cursor.cursor_count(), 2);

    assert!(press(&mut handler, &ESCAPE, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 1);
    assert!(cursor.primary.is_collapsed());

    // The stack cleared with the collapse: Ctrl+U is now a no-op.
    assert!(press(&mut handler, &UNDO_CURSOR, &mut doc, &mut cursor).is_none());
}

// ========== Skip occurrence ==========

#[test]
fn skip_drops_last_and_advances_with_wrap() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo foo foo foo");
    let mut cursor = cursors_at(&[(0, 1)]);

    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor); // "foo" @0
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor); // add @4
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor); // add @8
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 3)), ((0, 4), (0, 7)), ((0, 8), (0, 11))]
    );

    // Skip drops the @8 cursor and advances to the next match @12.
    assert!(skip(&mut handler, &mut doc, &mut cursor).is_some());
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 3)), ((0, 4), (0, 7)), ((0, 12), (0, 15))]
    );

    // Skip again drops @12; no match after it, so it wraps to the first
    // unselected occurrence, @8.
    assert!(skip(&mut handler, &mut doc, &mut cursor).is_some());
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 3)), ((0, 4), (0, 7)), ((0, 8), (0, 11))]
    );
}

#[test]
fn skip_single_occurrence_is_noop() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo bar");
    let mut cursor = cursors_at(&[(0, 1)]);

    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor); // select the only "foo"
    // Only one occurrence: nothing to advance to.
    assert!(skip(&mut handler, &mut doc, &mut cursor).is_none());
    assert_eq!(selections(&cursor), vec![((0, 0), (0, 3))]);
}

#[test]
fn skip_with_collapsed_caret_bootstraps_word_selection() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo foo");
    let mut cursor = cursors_at(&[(0, 1)]);

    // No added cursor and a collapsed caret: skip selects the word first.
    assert!(skip(&mut handler, &mut doc, &mut cursor).is_some());
    assert_eq!(selections(&cursor), vec![((0, 0), (0, 3))]);
}

#[test]
fn skip_with_single_selection_moves_primary() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo foo foo");
    // A single non-empty selection, no added cursors: skip moves the primary.
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 3))]);

    assert!(skip(&mut handler, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 1);
    assert_eq!(selections(&cursor), vec![((0, 4), (0, 7))]);
}

// ========== Undo / redo roundtrips ==========

#[test]
fn add_below_command_roundtrips_cursor_state() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new(STICKY_DOC);
    let mut cursor = cursors_at(&[(0, 8)]);
    let before = cursor.clone();

    let cmd = press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor).expect("adds a cursor");
    let after = cursor.clone();

    // Undo restores the exact prior single-cursor state.
    cmd.inverse().apply(&mut doc, &mut cursor).unwrap();
    assert_eq!(cursor, before);

    // Redo restores the two-cursor state.
    cmd.apply(&mut doc, &mut cursor).unwrap();
    assert_eq!(cursor, after);
}

#[test]
fn select_all_occurrences_command_roundtrips_cursor_state() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo bar foo");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 3))]);
    let before = cursor.clone();

    let cmd = press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).expect("selects all");
    let after = cursor.clone();
    assert_eq!(cursor.cursor_count(), 2);

    cmd.inverse().apply(&mut doc, &mut cursor).unwrap();
    assert_eq!(cursor, before);

    cmd.apply(&mut doc, &mut cursor).unwrap();
    assert_eq!(cursor, after);
}

#[test]
fn undo_last_cursor_command_roundtrips_cursor_state() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo foo foo");
    let mut cursor = cursors_at(&[(0, 1)]);
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor);
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor);
    let before = cursor.clone();

    let cmd = press(&mut handler, &UNDO_CURSOR, &mut doc, &mut cursor).expect("removes a cursor");
    let after = cursor.clone();

    cmd.inverse().apply(&mut doc, &mut cursor).unwrap();
    assert_eq!(cursor, before);

    cmd.apply(&mut doc, &mut cursor).unwrap();
    assert_eq!(cursor, after);
}

// ========== Reviewer findings: regression coverage ==========

#[test]
fn stale_addition_stack_invalidated_by_bare_document_edit() {
    // Finding 1: a host-driven bare Insert can move text under unchanged cursor
    // positions. The addition stack must invalidate on the content-revision
    // bump, or skip would slice a stale range (" fo") as its search term.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo foo foo");
    let mut cursor = cursors_at(&[(0, 1)]);

    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor); // select "foo" @0
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor); // add "foo" @4..7
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 3)), ((0, 4), (0, 7))]
    );

    // Bare insert of "x" at the document start: the cursor positions are left
    // untouched, but byte range 4..7 now covers " fo" instead of "foo".
    Command::Insert {
        position: Position::new(0, 0),
        text: "x".to_string(),
    }
    .apply(&mut doc, &mut cursor)
    .expect("bare insert applies");
    assert_eq!(doc.text(), "xfoo foo foo");
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 3)), ((0, 4), (0, 7))]
    );

    // Skip must not trust the stale 4..7 entry. The revision bump clears the
    // stack, so skip finds no occurrence cursor to advance and is a no-op —
    // never selecting a malformed " fo" range.
    assert!(skip(&mut handler, &mut doc, &mut cursor).is_none());
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 3)), ((0, 4), (0, 7))]
    );
}

#[test]
fn select_all_records_new_cursors_even_when_count_shrinks() {
    // Finding 4: select-all can create fresh occurrence selections while
    // reducing the cursor count. Those additions must still be recorded so
    // Ctrl+U can peel them.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo\nfoo\nfoo\nbar\nbaz");
    let mut cursor = cursors_at(&[(0, 1)]);

    for _ in 0..4 {
        press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor);
    }
    assert_eq!(cursor.cursor_count(), 5);

    // Ctrl+Shift+L on "foo" (three lines) shrinks 5 cursors down to 3.
    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 3);
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 3)), ((1, 0), (1, 3)), ((2, 0), (2, 3))]
    );

    // Ctrl+U now peels the most recently recorded occurrence, leaving two.
    assert!(press(&mut handler, &UNDO_CURSOR, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 2);
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 3)), ((1, 0), (1, 3))]
    );
}

#[test]
fn select_all_occurrences_preserves_backward_primary_direction() {
    // Finding 5 (direction): a backward primary selection stays backward, so
    // the active head does not jump to the forward match boundary.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo bar foo");
    // Backward "foo": anchor at column 3, head at column 0.
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 3), Position::new(0, 0))]);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 2);
    // Primary keeps anchor 3 / head 0 (backward); the head stays at column 0.
    assert_eq!(
        selections(&cursor),
        vec![((0, 3), (0, 0)), ((0, 8), (0, 11))]
    );
    assert_eq!(cursor.primary.head, Position::new(0, 0));
}

#[test]
fn select_all_occurrences_misaligned_source_keeps_primary_only() {
    // Finding 5 (misalignment): selecting "aa" at columns 1..3 of "aaa" — a
    // valid "aa" that the non-overlapping scan (which finds 0..2) does not
    // align to — must not merge into "aaa". The primary stays exactly 1..3 and
    // the overlapping scan match is dropped, so no unintended text is targeted.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aaa");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 1), Position::new(0, 3))]);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_none());
    assert_eq!(cursor.cursor_count(), 1);
    assert_eq!(selections(&cursor), vec![((0, 1), (0, 3))]);
}

#[test]
fn ctrl_d_fills_gap_left_by_skip() {
    // Finding 8: after skip leaves an earlier occurrence unselected, the next
    // Ctrl+D must fill that gap rather than stopping on the first (already
    // selected) match.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo foo foo foo");
    let mut cursor = cursors_at(&[(0, 1)]);

    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor); // "foo" @0
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor); // add @4
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor); // add @8

    // Skip advances the @8 cursor to @12, leaving @8 unselected.
    assert!(skip(&mut handler, &mut doc, &mut cursor).is_some());
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 3)), ((0, 4), (0, 7)), ((0, 12), (0, 15))]
    );

    // Ctrl+D fills the @8 gap.
    assert!(press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor).is_some());
    assert_eq!(
        selections(&cursor),
        vec![
            ((0, 0), (0, 3)),
            ((0, 4), (0, 7)),
            ((0, 8), (0, 11)),
            ((0, 12), (0, 15))
        ]
    );
}

#[test]
fn skip_advances_last_occurrence_past_vertical_cursors() {
    // Finding 9: a collapsed vertical add sitting on top of the addition stack
    // must not block skip. Skip advances the last *occurrence* cursor and
    // leaves the vertical cursors in place.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo foo foo\nxxxxxxxxxxx");
    let mut cursor = cursors_at(&[(0, 1)]);

    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor); // select "foo" @0
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor); // add "foo" @4
    press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor); // collapsed clones on line 1
    assert_eq!(cursor.cursor_count(), 4);

    // Skip advances the @4 occurrence to @8, ignoring the collapsed vertical
    // entries, and keeps every cursor count.
    assert!(skip(&mut handler, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 4);
    assert_eq!(
        selections(&cursor),
        vec![
            ((0, 0), (0, 3)),
            ((0, 8), (0, 11)),
            ((1, 3), (1, 3)),
            ((1, 7), (1, 7))
        ]
    );
}

#[test]
fn select_all_then_vertical_move_ignores_stale_sticky_column() {
    // Finding 2: Ctrl+A replaces the cursor set (its head ends at (2, 12)), so
    // the sticky column captured by a prior Down (8) must not be reused by a
    // following vertical move. Validating sticky columns by exact cursor-state
    // identity makes the select-all state fail the check, so the next Up seeds
    // from the real head column (12) and lands at (1, 12), not (1, 8).
    let mut handler = KeyboardHandler::new();
    // Lines of length 10 / 20 / 12.
    let mut doc = Document::new("aaaaaaaaaa\nbbbbbbbbbbbbbbbbbbbb\ncccccccccccc");
    let mut cursor = cursors_at(&[(0, 8)]);

    // Down records sticky column 8.
    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Down),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(heads(&cursor), vec![(1, 8)]);

    // Select-all: the primary now spans the document, head at (2, 12).
    press(
        &mut handler,
        &KeyEvent::new(KeyCode::Char('a'), CTRL),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(heads(&cursor), vec![(2, 12)]);

    // Up seeds from the select-all head column (12), not the stale sticky (8).
    assert!(
        press(
            &mut handler,
            &KeyEvent::simple(KeyCode::Up),
            &mut doc,
            &mut cursor,
        )
        .is_some()
    );
    assert_eq!(heads(&cursor), vec![(1, 12)]);
}

// ========== Reviewer findings (independent review): regression coverage ==========

#[test]
fn ctrl_u_over_adjacent_occurrences_keeps_them_distinct() {
    // Finding 1: select-all on "foofoofoo" yields adjacent occurrence cursors
    // [0..3] primary, [3..6], [6..9]. Removing the last one via Ctrl+U rebuilds
    // the survivors through `remove_secondary`; the touching [3..6] must NOT
    // fuse into the primary [0..3]. Before the merge-predicate fix it collapsed
    // to a single [0..6] selection and the next edit replaced the whole span.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foofoofoo");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 3))]);

    press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor);
    assert_eq!(cursor.cursor_count(), 3);

    assert!(press(&mut handler, &UNDO_CURSOR, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 2);
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 3)), ((0, 3), (0, 6))]
    );

    // The two occurrences edit independently: typing replaces each, producing
    // "xxfoo" — not a single "xfoo" from a fused [0..6] selection.
    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Char('x')),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(doc.text(), "xxfoo");
    assert_eq!(heads(&cursor), vec![(0, 1), (0, 2)]);
}

#[test]
fn add_below_over_adjacent_occurrences_preserves_them() {
    // Finding 1: add-cursor-below rebuilds the existing secondaries before
    // adding the vertical clones. The adjacent occurrences [0..3],[3..6],[6..9]
    // must survive that rebuild rather than fuse.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foofoofoo\nxxxxxxxxx");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 3))]);

    press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor);
    assert_eq!(cursor.cursor_count(), 3);

    assert!(press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 6);
    assert_eq!(
        selections(&cursor),
        vec![
            ((0, 0), (0, 3)),
            ((0, 3), (0, 6)),
            ((0, 6), (0, 9)),
            ((1, 3), (1, 3)),
            ((1, 6), (1, 6)),
            ((1, 9), (1, 9)),
        ]
    );
}

#[test]
fn skip_over_adjacent_occurrences_preserves_remaining() {
    // Finding 1: skip drops the last occurrence and rebuilds the remaining
    // cursors via `remove_secondary`; adjacent survivors [0..3],[3..6] must
    // stay distinct while the dropped one advances to the next match.
    let mut handler = KeyboardHandler::new();
    // "foo" matches adjacently at 0, 3, 6 and separately at 10. Start from an
    // explicit [0..3] selection so the search term stays "foo" (a collapsed
    // bootstrap would select the whole unbroken word "foofoofoo").
    let mut doc = Document::new("foofoofoo foo");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 3))]);

    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor); // add adjacent @3
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor); // add adjacent @6
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 3)), ((0, 3), (0, 6)), ((0, 6), (0, 9))]
    );

    assert!(skip(&mut handler, &mut doc, &mut cursor).is_some());
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 3)), ((0, 3), (0, 6)), ((0, 10), (0, 13))]
    );
}

#[test]
fn ctrl_d_from_misaligned_selection_adds_next_untaken_match() {
    // Finding 4: an explicit "aa" selection at columns 1..3 of "aaaaa" does not
    // fall on the zero-anchored non-overlapping partition. Ctrl+D must search
    // from the selection end and add the untaken match 3..5 (not wrap to 0..2
    // and fuse into the primary).
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aaaaa");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 1), Position::new(0, 3))]);

    assert!(press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 2);
    assert_eq!(
        selections(&cursor),
        vec![((0, 1), (0, 3)), ((0, 3), (0, 5))]
    );
}

#[test]
fn external_cursor_mutation_invalidates_addition_order_even_on_round_trip() {
    // Finding 2: the value-equality snapshot is defeated by a sequence of
    // mutations that returns to the same cursor state. The editor's explicit
    // invalidation hook (invoked from every non-add cursor path — mouse,
    // set_cursor, undo/redo, ...) cannot be fooled by such a round trip.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_at(&[(1, 1)]);

    press(&mut handler, &ADD_ABOVE, &mut doc, &mut cursor); // add (0,1)
    press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor); // add (2,1)
    let restored = cursor.clone();
    assert_eq!(cursor.cursor_count(), 3);

    // Simulate an external path (e.g. mouse clicks) that rebuilds the exact
    // same three-cursor state. The editor calls this on every such mutation.
    handler.invalidate_cursor_order();
    cursor = restored;

    // Ctrl+U must no-op: the stale stack was invalidated, so it does not remove
    // an arbitrarily-ordered cursor.
    assert!(press(&mut handler, &UNDO_CURSOR, &mut doc, &mut cursor).is_none());
    assert_eq!(cursor.cursor_count(), 3);
}

#[test]
fn horizontal_round_trip_does_not_resurrect_sticky_column() {
    // Finding 3: a Left-then-Right round trip returns the caret to the same
    // coordinates, but a horizontal motion resets the preferred column. The
    // stale sticky column (8) must NOT be resurrected; the following vertical
    // add seeds from the live head column (2) and lands at (2, 2).
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new(STICKY_DOC);
    let mut cursor = cursors_at(&[(0, 8)]);

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Down),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(heads(&cursor), vec![(1, 2)]);

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Left),
        &mut doc,
        &mut cursor,
    );
    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Right),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(heads(&cursor), vec![(1, 2)]);

    assert!(press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor).is_some());
    assert_eq!(heads(&cursor), vec![(1, 2), (2, 2)]);
}

#[test]
fn skip_round_trip_discards_stale_sticky_column() {
    // Reviewer major (round 3): skip never invalidated sticky columns. A
    // Shift+Down over the short line stores preferred column 8 for the
    // selection (0,8)..(1,2); two skips advance the primary to (2,8)..(3,2)
    // and wrap back to the byte-identical original selection. That revived
    // state passed the exact-identity sticky check, so add-cursor-below
    // seeded from the stale column 8 instead of the live head column 2.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aaaaaaaaaa\nbb\naaaaaaaaaa\nbb\naaaaaaaaaa");
    let mut cursor = cursors_at(&[(0, 8)]);

    // Shift+Down: selection (0,8)..(1,2), sticky column 8 captured.
    press(
        &mut handler,
        &KeyEvent::new(KeyCode::Down, Modifiers::shift()),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(selections(&cursor), vec![((0, 8), (1, 2))]);

    // Two skips: away to the second occurrence of "aa\nbb", then wrapping
    // home to the byte-identical original selection.
    assert!(skip(&mut handler, &mut doc, &mut cursor).is_some());
    assert_eq!(selections(&cursor), vec![((2, 8), (3, 2))]);
    assert!(skip(&mut handler, &mut doc, &mut cursor).is_some());
    assert_eq!(selections(&cursor), vec![((0, 8), (1, 2))]);

    // Add-cursor-below seeds from the live head column (2), not the sticky
    // snapshot (8) the skips invalidated.
    assert!(press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor).is_some());
    assert_eq!(heads(&cursor), vec![(1, 2), (2, 2)]);
}

#[test]
fn skip_without_selection_change_preserves_sticky_column() {
    // Counterpart to the invalidation test: a skip that finds nothing to
    // advance (the selection's text occurs nowhere else) is a no-op and must
    // NOT destroy the sticky column a vertical move just captured.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new(STICKY_DOC);
    let mut cursor = cursors_at(&[(0, 8)]);

    // Shift+Down: selection (0,8)..(1,2), sticky column 8 captured. Its text
    // "aa\nbb" occurs exactly once in STICKY_DOC.
    press(
        &mut handler,
        &KeyEvent::new(KeyCode::Down, Modifiers::shift()),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(selections(&cursor), vec![((0, 8), (1, 2))]);

    // Nothing to advance to: skip is a Handled no-op.
    assert!(skip(&mut handler, &mut doc, &mut cursor).is_none());
    assert_eq!(selections(&cursor), vec![((0, 8), (1, 2))]);

    // Shift+Down again: the surviving sticky column (8) lands the head on
    // (2,8) of the long third line, not the clamped column (2).
    assert!(
        press(
            &mut handler,
            &KeyEvent::new(KeyCode::Down, Modifiers::shift()),
            &mut doc,
            &mut cursor,
        )
        .is_some()
    );
    assert_eq!(selections(&cursor), vec![((0, 8), (2, 8))]);
}

#[test]
fn select_all_occurrences_unicode_at_buffer_end() {
    // Reviewer coverage gap: multi-byte occurrences with the final match
    // flush against the end of the buffer. Offsets must convert back to
    // code-point columns on char boundaries and the last match must not be
    // dropped. "héllo" is 5 code points / 6 bytes.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("héllo wörld héllo");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 5))]);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_some());
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 5)), ((0, 12), (0, 17))]
    );
}

#[test]
fn skip_wraps_over_unicode_occurrence_at_buffer_end() {
    // Reviewer coverage gap: skip from the buffer-end occurrence must wrap
    // to the first match without slicing inside a multi-byte character.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("wörld héllo wörld");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 12), Position::new(0, 17))]);

    assert!(skip(&mut handler, &mut doc, &mut cursor).is_some());
    assert_eq!(selections(&cursor), vec![((0, 0), (0, 5))]);
}
