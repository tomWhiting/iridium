//! Multi-cursor navigation.
//!
//! Split out of a 1,142-line `tests.rs` for #92, on the rule the file
//! already carried. The harness and the cursor builders are in [`super`].

use super::*;

// ========== Multi-cursor navigation ==========

#[test]
fn arrows_move_each_cursor() {
    let mut doc = Document::new("abc\ndef");
    let mut cursor = cursors_at(&[(0, 2), (1, 2)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Left),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(heads(&cursor), vec![(0, 1), (1, 1)]);

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Right),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(heads(&cursor), vec![(0, 2), (1, 2)]);

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Up),
        &mut doc,
        &mut cursor,
    );
    // The second cursor moves to line 0; the first hits the document start.
    assert_eq!(heads(&cursor), vec![(0, 0), (0, 2)]);
}

#[test]
fn word_motion_moves_each_cursor() {
    let mut doc = Document::new("foo bar\nbaz qux");
    let mut cursor = cursors_at(&[(0, 7), (1, 7)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::new(KeyCode::Left, Modifiers::ctrl()),
        &mut doc,
        &mut cursor,
    );

    assert_eq!(heads(&cursor), vec![(0, 4), (1, 4)]);
}

#[test]
fn shift_arrow_extends_each_cursor() {
    let mut doc = Document::new("abc\ndef");
    let mut cursor = cursors_at(&[(0, 1), (1, 1)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::new(KeyCode::Right, Modifiers::shift()),
        &mut doc,
        &mut cursor,
    );

    assert_eq!(cursor.cursor_count(), 2);
    for sel in cursor.all_selections() {
        assert_eq!(sel.anchor.column, 1);
        assert_eq!(sel.head.column, 2);
    }
}

#[test]
fn home_and_end_move_each_cursor() {
    let mut doc = Document::new("  foo\n  bar");
    let mut cursor = cursors_at(&[(0, 4), (1, 4)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Home),
        &mut doc,
        &mut cursor,
    );
    // Smart home: to first non-whitespace column on each line.
    assert_eq!(heads(&cursor), vec![(0, 2), (1, 2)]);

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::End),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(heads(&cursor), vec![(0, 5), (1, 5)]);
}

#[test]
fn ctrl_home_merges_all_cursors_at_document_start() {
    let mut doc = Document::new("abc\ndef");
    let mut cursor = cursors_at(&[(0, 2), (1, 2)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::new(KeyCode::Home, Modifiers::ctrl()),
        &mut doc,
        &mut cursor,
    );

    assert_eq!(cursor.cursor_count(), 1);
    assert_eq!(heads(&cursor), vec![(0, 0)]);
}

#[test]
fn vertical_motion_preserves_per_cursor_sticky_columns() {
    let mut doc = Document::new("long line one\nab\nlong line two\ncd\nlong line xyz");
    let mut cursor = cursors_at(&[(0, 10), (2, 10)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Down),
        &mut doc,
        &mut cursor,
    );
    // Both cursors are clamped to the short lines.
    assert_eq!(heads(&cursor), vec![(1, 2), (3, 2)]);

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Down),
        &mut doc,
        &mut cursor,
    );
    // Each cursor returns to its own sticky column 10.
    assert_eq!(heads(&cursor), vec![(2, 10), (4, 10)]);
}

#[test]
fn cursors_merge_when_converging_via_motion() {
    let mut doc = Document::new("abc");
    let mut cursor = cursors_at(&[(0, 0), (0, 1)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Left),
        &mut doc,
        &mut cursor,
    );

    assert_eq!(cursor.cursor_count(), 1);
    assert_eq!(heads(&cursor), vec![(0, 0)]);
}

#[test]
fn cursors_merge_when_backspaces_converge() {
    let mut doc = Document::new("hello");
    let mut cursor = cursors_at(&[(0, 4), (0, 5)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Backspace),
        &mut doc,
        &mut cursor,
    );

    assert_eq!(doc.text(), "hel");
    assert_eq!(cursor.cursor_count(), 1);
    assert_eq!(heads(&cursor), vec![(0, 3)]);
}

#[test]
fn escape_collapses_to_primary_only() {
    let mut doc = Document::new("abc\ndef\nghi");
    let mut cursor = cursors_at(&[(0, 1), (1, 1), (2, 1)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Escape),
        &mut doc,
        &mut cursor,
    );

    assert_eq!(cursor.cursor_count(), 1);
    assert_eq!(heads(&cursor), vec![(0, 1)]);
    assert!(cursor.primary.is_collapsed());
}
