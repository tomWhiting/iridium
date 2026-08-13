//! Multi-cursor editing, at document-state level.
//!
//! Split out of a 1,142-line `tests.rs` for #92, on the rule the file
//! already carried. The harness and the cursor builders are in [`super`].

use super::*;

// ========== Multi-cursor editing (document-state-level) ==========

#[test]
fn multi_cursor_typing_same_line_applies_all_and_keeps_columns() {
    let mut doc = Document::new("aaa bbb ccc");
    let mut cursor = cursors_at(&[(0, 0), (0, 4), (0, 8)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Char('x')),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(doc.text(), "xaaa xbbb xccc");
    assert_eq!(heads(&cursor), vec![(0, 1), (0, 6), (0, 11)]);

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Char('y')),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(doc.text(), "xyaaa xybbb xyccc");
    assert_eq!(heads(&cursor), vec![(0, 2), (0, 8), (0, 14)]);
}

#[test]
fn multi_cursor_paste_multichar_same_line_no_drift() {
    let mut doc = Document::new("aaa bbb ccc");
    let mut cursor = cursors_at(&[(0, 0), (0, 4), (0, 8)]);
    let handler = KeyboardHandler::new();

    let result = handler.handle_paste("xy", &doc, &cursor);
    let KeyResult::Command(cmd) = result else {
        panic!("expected a command, got {result:?}");
    };
    cmd.apply(&mut doc, &mut cursor)
        .expect("command must apply");

    assert_eq!(doc.text(), "xyaaa xybbb xyccc");
    assert_eq!(heads(&cursor), vec![(0, 2), (0, 8), (0, 14)]);
}

#[test]
fn multi_cursor_enter_shifts_later_cursors_to_new_lines() {
    let mut doc = Document::new("abcdef");
    let mut cursor = cursors_at(&[(0, 2), (0, 4)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Enter),
        &mut doc,
        &mut cursor,
    );

    assert_eq!(doc.text(), "ab\ncd\nef");
    assert_eq!(heads(&cursor), vec![(1, 0), (2, 0)]);
}

#[test]
fn multi_cursor_insert_replaces_selections() {
    let mut doc = Document::new("foo bar foo");
    let mut cursor = cursors_with(&[
        Selection::new(Position::new(0, 0), Position::new(0, 3)),
        Selection::new(Position::new(0, 8), Position::new(0, 11)),
    ]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Char('X')),
        &mut doc,
        &mut cursor,
    );

    assert_eq!(doc.text(), "X bar X");
    assert_eq!(heads(&cursor), vec![(0, 1), (0, 7)]);
    assert!(cursor.all_selections().all(Selection::is_collapsed));
}

#[test]
fn multi_cursor_backspace_collapsed() {
    let mut doc = Document::new("abc def ghi");
    let mut cursor = cursors_at(&[(0, 3), (0, 7), (0, 11)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Backspace),
        &mut doc,
        &mut cursor,
    );

    assert_eq!(doc.text(), "ab de gh");
    assert_eq!(heads(&cursor), vec![(0, 2), (0, 5), (0, 8)]);
}

#[test]
fn multi_cursor_backspace_with_selections() {
    let mut doc = Document::new("abc def ghi");
    let mut cursor = cursors_with(&[
        Selection::new(Position::new(0, 0), Position::new(0, 3)),
        Selection::new(Position::new(0, 8), Position::new(0, 11)),
    ]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Backspace),
        &mut doc,
        &mut cursor,
    );

    assert_eq!(doc.text(), " def ");
    assert_eq!(heads(&cursor), vec![(0, 0), (0, 5)]);
}

#[test]
fn multi_cursor_backspace_mixed_selection_and_caret() {
    let mut doc = Document::new("abc def ghi");
    let mut cursor = cursors_with(&[
        Selection::new(Position::new(0, 0), Position::new(0, 3)),
        Selection::collapsed(Position::new(0, 7)),
    ]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Backspace),
        &mut doc,
        &mut cursor,
    );

    // Selection deletes "abc"; the collapsed caret deletes the 'f' before it.
    assert_eq!(doc.text(), " de ghi");
    assert_eq!(heads(&cursor), vec![(0, 0), (0, 3)]);
}

#[test]
fn multi_cursor_backspace_at_document_start_is_handled() {
    let mut doc = Document::new("ab");
    let mut cursor = cursors_at(&[(0, 0)]);
    let mut handler = KeyboardHandler::new();

    let cmd = press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Backspace),
        &mut doc,
        &mut cursor,
    );

    assert!(cmd.is_none());
    assert_eq!(doc.text(), "ab");
    assert_eq!(heads(&cursor), vec![(0, 0)]);
}

#[test]
fn multi_cursor_delete_forward() {
    let mut doc = Document::new("abc def");
    let mut cursor = cursors_at(&[(0, 0), (0, 4)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Delete),
        &mut doc,
        &mut cursor,
    );

    assert_eq!(doc.text(), "bc ef");
    assert_eq!(heads(&cursor), vec![(0, 0), (0, 3)]);
}

#[test]
fn multi_cursor_delete_forward_joins_lines() {
    let mut doc = Document::new("ab\ncd\nef");
    let mut cursor = cursors_at(&[(0, 2), (1, 2)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Delete),
        &mut doc,
        &mut cursor,
    );

    assert_eq!(doc.text(), "abcdef");
    assert_eq!(heads(&cursor), vec![(0, 2), (0, 4)]);
}

#[test]
fn multi_cursor_word_backspace() {
    let mut doc = Document::new("foo bar baz\nfoo bar baz");
    let mut cursor = cursors_at(&[(0, 3), (1, 7)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::new(KeyCode::Backspace, Modifiers::ctrl()),
        &mut doc,
        &mut cursor,
    );

    assert_eq!(doc.text(), " bar baz\nfoo  baz");
    assert_eq!(heads(&cursor), vec![(0, 0), (1, 4)]);
}

#[test]
fn multi_cursor_word_backspace_overlapping_ranges_merge() {
    let mut doc = Document::new("hello");
    let mut cursor = cursors_at(&[(0, 3), (0, 5)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::new(KeyCode::Backspace, Modifiers::ctrl()),
        &mut doc,
        &mut cursor,
    );

    // Both word-deletions reach into the same word: each byte is deleted
    // exactly once and the converging cursors merge.
    assert_eq!(doc.text(), "");
    assert_eq!(cursor.cursor_count(), 1);
    assert_eq!(heads(&cursor), vec![(0, 0)]);
}

#[test]
fn multi_cursor_word_delete_forward() {
    let mut doc = Document::new("foo bar\nfoo bar");
    let mut cursor = cursors_at(&[(0, 0), (1, 0)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::new(KeyCode::Delete, Modifiers::ctrl()),
        &mut doc,
        &mut cursor,
    );

    assert_eq!(doc.text(), "bar\nbar");
    assert_eq!(heads(&cursor), vec![(0, 0), (1, 0)]);
}
