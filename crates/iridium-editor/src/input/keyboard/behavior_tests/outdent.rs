//! `Shift+Tab`: taking one level of indentation back off.
//!
//! Split out of a 1,690-line `behavior_tests.rs` for #92, on the rule the
//! file already carried. The harness and the shared fixtures are in
//! [`super`].

use super::*;

// ========== Shift+Tab: outdent ==========

#[test]
fn shift_tab_removes_one_level_of_spaces() {
    let mut doc = Document::new("        foo");
    let mut cursor = cursors_at(&[(0, 10)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &SHIFT_TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    // Only one level (4 spaces) is removed; the cursor shifts by the amount
    // actually removed.
    assert_eq!(doc.text(), "    foo");
    assert_eq!(heads(&cursor), vec![(0, 6)]);
}

#[test]
fn shift_tab_removes_leading_tab() {
    let mut doc = Document::new("\tfoo");
    let mut cursor = cursors_at(&[(0, 3)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &SHIFT_TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "foo");
    assert_eq!(heads(&cursor), vec![(0, 2)]);
}

#[test]
fn shift_tab_removes_partial_indent_of_fewer_spaces() {
    let mut doc = Document::new("  foo");
    let mut cursor = cursors_at(&[(0, 4)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &SHIFT_TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    // Fewer than tab_width leading spaces: remove what is there, nothing more.
    assert_eq!(doc.text(), "foo");
    assert_eq!(heads(&cursor), vec![(0, 2)]);
}

#[test]
fn shift_tab_never_removes_non_whitespace() {
    let mut doc = Document::new("foo");
    let mut cursor = cursors_at(&[(0, 2)]);
    let mut handler = KeyboardHandler::new();

    let cmd = press(
        &mut handler,
        &SHIFT_TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert!(cmd.is_none(), "outdent with no indentation must be a no-op");
    assert_eq!(doc.text(), "foo");
    assert_eq!(heads(&cursor), vec![(0, 2)]);
}

#[test]
fn shift_tab_cursor_inside_indent_floors_at_boundary() {
    let mut doc = Document::new("    foo");
    let mut cursor = cursors_at(&[(0, 2)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &SHIFT_TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    // The cursor sat inside the removed indentation: it floors at column 0
    // instead of underflowing.
    assert_eq!(doc.text(), "foo");
    assert_eq!(heads(&cursor), vec![(0, 0)]);
}

#[test]
fn shift_tab_selection_outdents_mixed_indentation_per_line() {
    // Line 0: full level of spaces; line 1: a tab; line 2: two spaces then a
    // tab (only the spaces before the tab are removable, up to tab_width).
    let mut doc = Document::new("    aa\n\tbb\n  \tcc");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(2, 5))]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &SHIFT_TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "aa\nbb\n\tcc");
    // Anchor floors at the boundary of line 0; head shifts by the 2 columns
    // removed on line 2.
    assert_eq!(selections(&cursor), vec![((0, 0), (2, 3))]);
}

#[test]
fn shift_tab_multi_cursor_outdents_each_line_and_shifts_cursors() {
    let mut doc = Document::new("    aa\n\tbb");
    let mut cursor = cursors_at(&[(0, 5), (1, 2)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &SHIFT_TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "aa\nbb");
    assert_eq!(heads(&cursor), vec![(0, 1), (1, 1)]);
}

#[test]
fn undo_outdent_restores_text_and_cursors() {
    let mut doc = Document::new("    aa\n    bb");
    let original = cursors_at(&[(0, 5), (1, 5)]);
    let mut cursor = original.clone();
    let mut handler = KeyboardHandler::new();

    let cmd = press(
        &mut handler,
        &SHIFT_TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    )
    .expect("outdent must produce a command");

    assert_eq!(doc.text(), "aa\nbb");

    cmd.inverse()
        .apply(&mut doc, &mut cursor)
        .expect("inverse must apply");
    assert_eq!(doc.text(), "    aa\n    bb");
    assert_eq!(cursor, original);
}
