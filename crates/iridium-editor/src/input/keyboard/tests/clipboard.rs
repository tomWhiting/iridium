//! Multi-cursor cut, copy and paste.
//!
//! Split out of a 1,142-line `tests.rs` for #92, on the rule the file
//! already carried. The harness and the cursor builders are in [`super`].

use super::*;

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
        &CaretScopes::none(),
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
        &CaretScopes::none(),
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
        &CaretScopes::none(),
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
        &CaretScopes::none(),
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
        &CaretScopes::none(),
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
        &CaretScopes::none(),
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
        &CaretScopes::none(),
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
