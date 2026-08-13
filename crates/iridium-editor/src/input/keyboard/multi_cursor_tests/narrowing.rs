//! Taking carets away: undoing the last one, collapsing to the primary,
//! and skipping an occurrence.
//!
//! Split out of a 1,122-line `multi_cursor_tests.rs` for #92, on the rules
//! the file already carried. The harness and the chords are in [`super`].

use super::*;

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
