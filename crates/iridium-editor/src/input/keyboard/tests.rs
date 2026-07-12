//! Tests for the keyboard handler, including document-state-level
//! multi-cursor editing tests that apply the produced commands to a real
//! document and assert both the resulting text and every cursor position.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

fn create_test_document() -> Document {
    Document::new("Hello World\nSecond Line\nThird Line")
}

/// Builds a multi-cursor state from collapsed positions (first is primary).
pub(super) fn cursors_at(positions: &[(usize, usize)]) -> CursorState {
    let (first, rest) = positions
        .split_first()
        .expect("at least one cursor position");
    let mut state = CursorState::at(Position::new(first.0, first.1));
    for &(line, column) in rest {
        state.add_cursor(Selection::collapsed(Position::new(line, column)));
    }
    state
}

/// Builds a multi-cursor state from selections (first is primary).
pub(super) fn cursors_with(selections: &[Selection]) -> CursorState {
    let (first, rest) = selections.split_first().expect("at least one selection");
    let mut state = CursorState::new(*first);
    for sel in rest {
        state.add_cursor(*sel);
    }
    state
}

/// All cursor head positions in `all_selections` order.
pub(super) fn heads(cursor: &CursorState) -> Vec<(usize, usize)> {
    cursor
        .all_selections()
        .map(|sel| (sel.head.line, sel.head.column))
        .collect()
}

/// Handles a key event and applies the resulting command (if any) to the
/// document and cursor, returning the command for undo tests.
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

// ========== Existing single-cursor behavior ==========

#[test]
fn move_char_left() {
    let doc = create_test_document();
    let cursor = CursorState::at(Position::new(0, 5));
    let mut handler = KeyboardHandler::new();

    let result = handler.handle_key(
        &KeyEvent::simple(KeyCode::Left),
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
        assert_eq!(new_state.primary.head, Position::new(0, 4));
    } else {
        panic!("Expected SetSelection command");
    }
}

#[test]
fn move_char_right() {
    let doc = create_test_document();
    let cursor = CursorState::at(Position::new(0, 5));
    let mut handler = KeyboardHandler::new();

    let result = handler.handle_key(
        &KeyEvent::simple(KeyCode::Right),
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
        assert_eq!(new_state.primary.head, Position::new(0, 6));
    } else {
        panic!("Expected SetSelection command");
    }
}

#[test]
fn move_to_next_line() {
    let doc = create_test_document();
    // At end of first line
    let cursor = CursorState::at(Position::new(0, 11));
    let mut handler = KeyboardHandler::new();

    let result = handler.handle_key(
        &KeyEvent::simple(KeyCode::Right),
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
        assert_eq!(new_state.primary.head, Position::new(1, 0));
    } else {
        panic!("Expected SetSelection command");
    }
}

#[test]
fn shift_extends_selection() {
    let doc = create_test_document();
    let cursor = CursorState::at(Position::new(0, 5));
    let mut handler = KeyboardHandler::new();

    let event = KeyEvent::new(KeyCode::Right, Modifiers::shift());
    let result = handler.handle_key(
        &event,
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
        assert_eq!(new_state.primary.anchor, Position::new(0, 5));
        assert_eq!(new_state.primary.head, Position::new(0, 6));
        assert!(!new_state.primary.is_collapsed());
    } else {
        panic!("Expected SetSelection command");
    }
}

#[test]
fn ctrl_moves_by_word() {
    let doc = create_test_document();
    let cursor = CursorState::at(Position::new(0, 6));
    let mut handler = KeyboardHandler::new();

    let event = KeyEvent::new(KeyCode::Left, Modifiers::ctrl());
    let result = handler.handle_key(
        &event,
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
        // Should move to start of "World"
        assert_eq!(new_state.primary.head, Position::new(0, 0));
    } else {
        panic!("Expected SetSelection command");
    }
}

#[test]
fn char_insertion() {
    let doc = Document::new("Hello");
    let cursor = CursorState::at(Position::new(0, 5));
    let mut handler = KeyboardHandler::new();

    let event = KeyEvent::simple(KeyCode::Char('!'));
    let result = handler.handle_key(
        &event,
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    if let KeyResult::Command(Command::Compound { commands }) = result {
        assert!(!commands.is_empty());
    } else {
        panic!("Expected Compound command");
    }
}

#[test]
fn backspace_deletes_char() {
    let doc = Document::new("Hello");
    let cursor = CursorState::at(Position::new(0, 5));
    let mut handler = KeyboardHandler::new();

    let result = handler.handle_key(
        &KeyEvent::simple(KeyCode::Backspace),
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    if let KeyResult::Command(Command::Compound { commands }) = result {
        assert!(commands.iter().any(|c| matches!(c, Command::Delete { .. })));
    } else {
        panic!("Expected Compound command");
    }
}

#[test]
fn home_moves_to_line_start() {
    let doc = create_test_document();
    let cursor = CursorState::at(Position::new(0, 5));
    let mut handler = KeyboardHandler::new();

    let result = handler.handle_key(
        &KeyEvent::simple(KeyCode::Home),
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
        assert_eq!(new_state.primary.head.column, 0);
    } else {
        panic!("Expected SetSelection command");
    }
}

#[test]
fn end_moves_to_line_end() {
    let doc = create_test_document();
    let cursor = CursorState::at(Position::new(0, 0));
    let mut handler = KeyboardHandler::new();

    let result = handler.handle_key(
        &KeyEvent::simple(KeyCode::End),
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
        assert_eq!(new_state.primary.head, Position::new(0, 11));
    } else {
        panic!("Expected SetSelection command");
    }
}

#[test]
fn escape_collapses_selection() {
    let selection = Selection::new(Position::new(0, 0), Position::new(0, 5));
    let cursor = CursorState::new(selection);
    let doc = Document::new("Hello World");
    let mut handler = KeyboardHandler::new();

    let result = handler.handle_key(
        &KeyEvent::simple(KeyCode::Escape),
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
        assert!(new_state.primary.is_collapsed());
        assert_eq!(new_state.primary.head, Position::new(0, 5));
    } else {
        panic!("Expected SetSelection command");
    }
}

#[test]
fn up_arrow_with_sticky_column() {
    let doc = Document::new("Short\nMedium Line\nShort");
    let cursor = CursorState::at(Position::new(1, 11)); // End of "Medium Line"
    let mut handler = KeyboardHandler::new();

    // Move up - should try to stay at column 11 but will be clamped
    let result = handler.handle_key(
        &KeyEvent::simple(KeyCode::Up),
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
        assert_eq!(new_state.primary.head.line, 0);
        assert_eq!(new_state.primary.head.column, 5); // Clamped to "Short" length
    } else {
        panic!("Expected SetSelection command");
    }
}

#[test]
fn ctrl_d_selects_word_first() {
    let doc = Document::new("foo bar foo baz foo");
    let cursor = CursorState::at(Position::new(0, 1)); // Inside "foo"
    let mut handler = KeyboardHandler::new();

    // First Ctrl+D should select the word
    let event = KeyEvent::new(KeyCode::Char('d'), Modifiers::ctrl());
    let result = handler.handle_key(
        &event,
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
        assert!(!new_state.primary.is_collapsed());
        assert_eq!(new_state.primary.start(), Position::new(0, 0));
        assert_eq!(new_state.primary.end(), Position::new(0, 3)); // "foo"
    } else {
        panic!("Expected SetSelection command");
    }
}

#[test]
fn ctrl_d_adds_next_match() {
    let doc = Document::new("foo bar foo baz foo");
    // Start with "foo" already selected
    let cursor = CursorState::new(Selection::new(Position::new(0, 0), Position::new(0, 3)));
    let mut handler = KeyboardHandler::new();

    // Ctrl+D should find next "foo"
    let event = KeyEvent::new(KeyCode::Char('d'), Modifiers::ctrl());
    let result = handler.handle_key(
        &event,
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
        // Should have 2 cursors now
        assert_eq!(new_state.cursor_count(), 2);
        // Second cursor at "foo" position 8-11
        assert!(
            new_state
                .secondary
                .iter()
                .any(|s| s.start() == Position::new(0, 8))
        );
    } else {
        panic!("Expected SetSelection command");
    }
}

#[test]
fn escape_collapses_multi_cursor() {
    let mut cursor = CursorState::at(Position::new(0, 0));
    cursor.add_cursor(Selection::collapsed(Position::new(1, 0)));
    cursor.add_cursor(Selection::collapsed(Position::new(2, 0)));

    let doc = Document::new("Line 1\nLine 2\nLine 3");
    let mut handler = KeyboardHandler::new();

    let result = handler.handle_key(
        &KeyEvent::simple(KeyCode::Escape),
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
        // Should have only 1 cursor
        assert_eq!(new_state.cursor_count(), 1);
        assert!(new_state.secondary.is_empty());
    } else {
        panic!("Expected SetSelection command");
    }
}

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

// ========== Multi-cursor clipboard ==========

#[test]
fn copy_joins_selected_texts_with_line_ending() {
    let doc = Document::new("abc def ghi");
    let cursor = cursors_with(&[
        Selection::new(Position::new(0, 0), Position::new(0, 3)),
        Selection::new(Position::new(0, 8), Position::new(0, 11)),
    ]);
    let mut handler = KeyboardHandler::new();

    let event = KeyEvent::new(KeyCode::Char('c'), Modifiers::ctrl());
    let result = handler.handle_key(
        &event,
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    assert_eq!(
        result,
        KeyResult::Clipboard(ClipboardOperation::Copy("abc\nghi".to_string()))
    );
}

#[test]
fn copy_collapsed_multi_cursor_copies_each_line() {
    let doc = Document::new("one\ntwo\nthree");
    let cursor = cursors_at(&[(0, 1), (2, 2)]);
    let mut handler = KeyboardHandler::new();

    let event = KeyEvent::new(KeyCode::Char('c'), Modifiers::ctrl());
    let result = handler.handle_key(
        &event,
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    assert_eq!(
        result,
        KeyResult::Clipboard(ClipboardOperation::Copy("one\nthree\n".to_string()))
    );
}

#[test]
fn copy_collapsed_cursors_on_same_line_copy_line_once() {
    let doc = Document::new("one\ntwo");
    let cursor = cursors_at(&[(0, 1), (0, 3)]);
    let mut handler = KeyboardHandler::new();

    let event = KeyEvent::new(KeyCode::Char('c'), Modifiers::ctrl());
    let result = handler.handle_key(
        &event,
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    assert_eq!(
        result,
        KeyResult::Clipboard(ClipboardOperation::Copy("one\n".to_string()))
    );
}

#[test]
fn cut_removes_all_selections() {
    let mut doc = Document::new("abc def ghi");
    let mut cursor = cursors_with(&[
        Selection::new(Position::new(0, 0), Position::new(0, 3)),
        Selection::new(Position::new(0, 8), Position::new(0, 11)),
    ]);
    let mut handler = KeyboardHandler::new();

    let event = KeyEvent::new(KeyCode::Char('x'), Modifiers::ctrl());
    let result = handler.handle_key(
        &event,
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    let KeyResult::Clipboard(ClipboardOperation::Cut { text, command }) = result else {
        panic!("expected a cut operation, got {result:?}");
    };
    assert_eq!(text, "abc\nghi");

    command
        .apply(&mut doc, &mut cursor)
        .expect("cut command must apply");
    assert_eq!(doc.text(), " def ");
    assert_eq!(heads(&cursor), vec![(0, 0), (0, 5)]);
}

#[test]
fn cut_collapsed_multi_cursor_removes_lines() {
    let mut doc = Document::new("one\ntwo\nthree");
    let mut cursor = cursors_at(&[(0, 0), (2, 1)]);
    let mut handler = KeyboardHandler::new();

    let event = KeyEvent::new(KeyCode::Char('x'), Modifiers::ctrl());
    let result = handler.handle_key(
        &event,
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    let KeyResult::Clipboard(ClipboardOperation::Cut { text, command }) = result else {
        panic!("expected a cut operation, got {result:?}");
    };
    assert_eq!(text, "one\nthree\n");

    command
        .apply(&mut doc, &mut cursor)
        .expect("cut command must apply");
    // Cutting the last line consumes the preceding line ending, so the line
    // disappears entirely instead of leaving an empty trailing line.
    assert_eq!(doc.text(), "two");
    assert_eq!(heads(&cursor), vec![(0, 0), (0, 3)]);
}

/// Regression test (Norn review, 2026-07-12): cut must always deliver the
/// same clipboard text copy would, even when nothing can be deleted. On an
/// empty document, copy yields the empty current line ("\n") — cut must hand
/// the host that same text with a no-op command, not silently skip the
/// clipboard update.
#[test]
fn cut_on_empty_document_still_updates_clipboard() {
    let doc = Document::new("");
    let cursor = CursorState::at(Position::zero());
    let mut handler = KeyboardHandler::new();

    let event = KeyEvent::new(KeyCode::Char('x'), Modifiers::ctrl());
    let result = handler.handle_key(
        &event,
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    let KeyResult::Clipboard(ClipboardOperation::Cut { text, command }) = result else {
        panic!("expected a cut operation, got {result:?}");
    };
    assert_eq!(text, "\n");
    assert!(command.is_empty(), "empty document: cut must be a no-op");
}

/// Cutting the empty trailing line of "a\n" removes the newline that creates
/// it, matching the "\n" the clipboard receives.
#[test]
fn cut_trailing_empty_line_removes_preceding_newline() {
    let mut doc = Document::new("a\n");
    let mut cursor = CursorState::at(Position::new(1, 0));
    let mut handler = KeyboardHandler::new();

    let event = KeyEvent::new(KeyCode::Char('x'), Modifiers::ctrl());
    let result = handler.handle_key(
        &event,
        &doc,
        &cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );

    let KeyResult::Clipboard(ClipboardOperation::Cut { text, command }) = result else {
        panic!("expected a cut operation, got {result:?}");
    };
    assert_eq!(text, "\n");

    command
        .apply(&mut doc, &mut cursor)
        .expect("cut command must apply");
    assert_eq!(doc.text(), "a");
    assert_eq!(heads(&cursor), vec![(0, 1)]);
}

// ========== Undo of multi-cursor edits ==========

#[test]
fn undo_multi_cursor_edit_restores_text_and_cursors() {
    let mut doc = Document::new("abc def");
    let original_cursor = cursors_at(&[(0, 0), (0, 4)]);
    let mut cursor = original_cursor.clone();
    let mut handler = KeyboardHandler::new();

    let cmd = press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Char('x')),
        &mut doc,
        &mut cursor,
    )
    .expect("typing must produce a command");

    assert_eq!(doc.text(), "xabc xdef");
    assert_eq!(heads(&cursor), vec![(0, 1), (0, 6)]);

    cmd.inverse()
        .apply(&mut doc, &mut cursor)
        .expect("inverse command must apply");

    assert_eq!(doc.text(), "abc def");
    assert_eq!(cursor, original_cursor);
}

#[test]
fn undo_multi_cursor_backspace_restores_text_and_cursors() {
    let mut doc = Document::new("abc def ghi");
    let original_cursor = cursors_at(&[(0, 3), (0, 7), (0, 11)]);
    let mut cursor = original_cursor.clone();
    let mut handler = KeyboardHandler::new();

    let cmd = press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Backspace),
        &mut doc,
        &mut cursor,
    )
    .expect("backspace must produce a command");

    assert_eq!(doc.text(), "ab de gh");

    cmd.inverse()
        .apply(&mut doc, &mut cursor)
        .expect("inverse command must apply");

    assert_eq!(doc.text(), "abc def ghi");
    assert_eq!(cursor, original_cursor);
}
