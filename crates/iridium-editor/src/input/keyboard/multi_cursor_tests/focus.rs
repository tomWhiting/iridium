//! Active-cursor policy across additions, wraparound and removal.
use super::*;

#[test]
fn adding_occurrences_wraps_from_active_match_and_removes_in_reverse_order() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo foo foo foo");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 8), Position::new(0, 11))]);
    let original = cursor.clone();
    let mut states = vec![original.clone()];
    for column in [12, 0, 4] {
        assert!(press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor).is_some());
        assert_eq!(cursor.primary.start(), Position::new(0, column));
        states.push(cursor.clone());
    }
    assert!(press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor).is_none());
    states.pop();
    for expected in states.iter().rev() {
        assert!(press(&mut handler, &UNDO_CURSOR, &mut doc, &mut cursor).is_some());
        assert_eq!(&cursor, expected);
    }
    assert_eq!(cursor, original);
}

#[test]
fn bulk_vertical_addition_focuses_outer_edge_and_peels_every_added_cursor() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("a\nb\nc\nd\ne\nf\ng");
    let mut cursor = cursors_at(&[(4, 1), (1, 1)]);
    let original = cursor.clone();
    press(&mut handler, &ADD_ABOVE, &mut doc, &mut cursor);
    assert_eq!(cursor.primary.head, Position::new(0, 1));
    assert_eq!(cursor.cursor_count(), 4);
    press(&mut handler, &UNDO_CURSOR, &mut doc, &mut cursor);
    assert_eq!(cursor.primary.head, Position::new(3, 1));
    press(&mut handler, &UNDO_CURSOR, &mut doc, &mut cursor);
    assert_eq!(cursor, original);
}

#[test]
fn collision_only_add_does_not_change_focus_or_removal_order() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("a\nb");
    let mut cursor = cursors_at(&[(1, 1)]);
    press(&mut handler, &ADD_ABOVE, &mut doc, &mut cursor);
    let full = cursor.clone();
    assert!(press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor).is_none());
    assert_eq!(cursor, full);
    press(&mut handler, &UNDO_CURSOR, &mut doc, &mut cursor);
    assert_eq!(cursor, cursors_at(&[(1, 1)]));
}

#[test]
fn select_all_keeps_active_match_after_sequential_additions() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo foo foo");
    let mut cursor = cursors_at(&[(0, 1)]);
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor);
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor);
    let active = cursor.primary;
    press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor);
    assert_eq!(cursor.primary, active);
    assert_eq!(cursor.cursor_count(), 3);
    press(&mut handler, &UNDO_CURSOR, &mut doc, &mut cursor);
    assert_eq!(cursor.primary, active);
    press(&mut handler, &UNDO_CURSOR, &mut doc, &mut cursor);
    assert_eq!(cursor.primary.start(), Position::new(0, 0));
}

#[test]
fn skip_after_vertical_addition_focuses_occurrence_and_remove_returns_to_vertical_caret() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo foo foo\nabcdefghijk");
    let mut cursor = cursors_at(&[(0, 1)]);
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor);
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor);
    press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor);
    let vertical = cursor.primary;
    skip(&mut handler, &mut doc, &mut cursor);
    assert_eq!(cursor.primary.start(), Position::new(0, 8));
    press(&mut handler, &UNDO_CURSOR, &mut doc, &mut cursor);
    assert_eq!(cursor.primary, vertical);
    assert_eq!(cursor.cursor_count(), 3);
}

#[test]
fn escape_keeps_newest_occurrence_after_word_motion() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo bar\nfoo baz");
    let mut cursor = cursors_at(&[(0, 1)]);
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor);
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor);
    press(
        &mut handler,
        &KeyEvent::new(KeyCode::Right, CTRL),
        &mut doc,
        &mut cursor,
    );
    let active = cursor.primary.head;
    assert_eq!(active.line, 1);
    press(&mut handler, &ESCAPE, &mut doc, &mut cursor);
    assert_eq!(cursor, CursorState::at(active));
}
