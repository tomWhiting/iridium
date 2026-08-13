//! The handler with one caret: motion, editing and the chords that
//! predate multi-cursor.
//!
//! Split out of a 1,142-line `tests.rs` for #92, on the rule the file
//! already carried. The harness and the cursor builders are in [`super`].

use super::*;

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
        &CaretScopes::none(),
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
        &CaretScopes::none(),
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
        &CaretScopes::none(),
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
        &CaretScopes::none(),
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
        &CaretScopes::none(),
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
        &CaretScopes::none(),
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
        &CaretScopes::none(),
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
        &CaretScopes::none(),
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
        &CaretScopes::none(),
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
        &CaretScopes::none(),
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
        &CaretScopes::none(),
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
        &CaretScopes::none(),
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
        &CaretScopes::none(),
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
        &CaretScopes::none(),
    );

    if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
        // Should have only 1 cursor
        assert_eq!(new_state.cursor_count(), 1);
        assert!(new_state.secondary.is_empty());
    } else {
        panic!("Expected SetSelection command");
    }
}
