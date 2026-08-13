//! Undo and redo across a multi-cursor edit, there and back.
//!
//! Split out of a 1,122-line `multi_cursor_tests.rs` for #92, on the rules
//! the file already carried. The harness and the chords are in [`super`].

use super::*;

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
