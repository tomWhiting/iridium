//! `Tab`: padding a collapsed caret, and indenting a selection.
//!
//! Split out of a 1,690-line `behavior_tests.rs` for #92, on the rule the
//! file already carried. The harness and the shared fixtures are in
//! [`super`].

use super::*;

// ========== Tab: collapsed cursors ==========

#[test]
fn tab_pads_with_spaces_to_next_tab_stop_from_odd_column() {
    let mut doc = Document::new("abc");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    // Column 1, width 4: pad 3 spaces to reach the tab stop at column 4.
    assert_eq!(doc.text(), "a   bc");
    assert_eq!(heads(&cursor), vec![(0, 4)]);
}

#[test]
fn tab_at_tab_stop_inserts_full_width() {
    let mut doc = Document::new("abcd");
    let mut cursor = cursors_at(&[(0, 4)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "abcd    ");
    assert_eq!(heads(&cursor), vec![(0, 8)]);
}

#[test]
fn tab_honors_configured_width() {
    let mut doc = Document::new("abc");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();
    let config = EditorConfig {
        tab_width: 2,
        ..EditorConfig::default()
    };

    press(&mut handler, &TAB, &mut doc, &mut cursor, &config);

    // Column 1, width 2: pad 1 space to reach the tab stop at column 2.
    assert_eq!(doc.text(), "a bc");
    assert_eq!(heads(&cursor), vec![(0, 2)]);
}

#[test]
fn tab_inserts_tab_character_when_insert_spaces_disabled() {
    let mut doc = Document::new("abc");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &TAB, &mut doc, &mut cursor, &tabs_config());

    assert_eq!(doc.text(), "a\tbc");
    assert_eq!(heads(&cursor), vec![(0, 2)]);
}

#[test]
fn tab_multi_cursor_same_line_pads_each_from_its_own_column() {
    let mut doc = Document::new("abcdef");
    let mut cursor = cursors_at(&[(0, 1), (0, 4)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    // Cursor at column 1 pads 3, cursor at (pre-edit) column 4 pads 4; the
    // second caret lands after both insertions.
    assert_eq!(doc.text(), "a   bcd    ef");
    assert_eq!(heads(&cursor), vec![(0, 4), (0, 11)]);
}

#[test]
fn tab_multi_cursor_inserts_tab_characters() {
    let mut doc = Document::new("ab\ncd");
    let mut cursor = cursors_at(&[(0, 1), (1, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &TAB, &mut doc, &mut cursor, &tabs_config());

    assert_eq!(doc.text(), "a\tb\nc\td");
    assert_eq!(heads(&cursor), vec![(0, 2), (1, 2)]);
}

// ========== Tab: selection indent ==========

#[test]
fn tab_with_selection_indents_every_touched_line() {
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 1), Position::new(2, 1))]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "    aa\n    bb\n    cc");
    // The selection keeps covering the same text.
    assert_eq!(selections(&cursor), vec![((0, 5), (2, 5))]);
}

#[test]
fn tab_with_selection_indents_with_tab_characters() {
    let mut doc = Document::new("aa\nbb");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(1, 2))]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &TAB, &mut doc, &mut cursor, &tabs_config());

    assert_eq!(doc.text(), "\taa\n\tbb");
    assert_eq!(selections(&cursor), vec![((0, 1), (1, 3))]);
}

#[test]
fn tab_selection_ending_at_column_zero_does_not_indent_that_line() {
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(2, 0))]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "    aa\n    bb\ncc");
    assert_eq!(selections(&cursor), vec![((0, 4), (2, 0))]);
}

#[test]
fn tab_multi_cursor_selections_indent_all_touched_lines_once() {
    let mut doc = Document::new("aa\nbb\ncc\ndd");
    let mut cursor = cursors_with(&[
        Selection::new(Position::new(0, 0), Position::new(1, 1)),
        Selection::new(Position::new(3, 0), Position::new(3, 2)),
    ]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "    aa\n    bb\ncc\n    dd");
    assert_eq!(
        selections(&cursor),
        vec![((0, 4), (1, 5)), ((3, 4), (3, 6))]
    );
}

#[test]
fn tab_mixed_selection_and_collapsed_cursor_indents_both_lines() {
    let mut doc = Document::new("aa\nbb");
    let mut cursor = cursors_with(&[
        Selection::new(Position::new(0, 0), Position::new(0, 2)),
        Selection::collapsed(Position::new(1, 1)),
    ]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    // Any non-collapsed selection switches Tab to line-indent mode for every
    // touched line, including lines under collapsed cursors.
    assert_eq!(doc.text(), "    aa\n    bb");
    assert_eq!(
        selections(&cursor),
        vec![((0, 4), (0, 6)), ((1, 5), (1, 5))]
    );
}

#[test]
fn undo_selection_indent_restores_text_and_selection() {
    let mut doc = Document::new("aa\nbb");
    let original = cursors_with(&[Selection::new(Position::new(0, 1), Position::new(1, 1))]);
    let mut cursor = original.clone();
    let mut handler = KeyboardHandler::new();

    let cmd = press(
        &mut handler,
        &TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    )
    .expect("indent must produce a command");

    assert_eq!(doc.text(), "    aa\n    bb");

    cmd.inverse()
        .apply(&mut doc, &mut cursor)
        .expect("inverse must apply");
    assert_eq!(doc.text(), "aa\nbb");
    assert_eq!(cursor, original);
}
