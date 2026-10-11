//! Visible-character navigation and edits preserve Unicode grapheme clusters.
use super::*;

#[test]
fn standalone_word_offsets_share_document_boundaries_and_are_cluster_aligned() {
    use unicode_segmentation::UnicodeSegmentation as _;
    for text in ["cafe\u{301}:: 猫 dog", "👩🏽‍💻 ++x", "  a_b", ""] {
        let document = Document::new(text);
        for byte in text
            .grapheme_indices(true)
            .map(|(byte, _)| byte)
            .chain([text.len()])
        {
            let column = text[..byte].chars().count();
            for (standalone, document_position) in [
                (
                    motions::word_left_byte(text, byte),
                    motions::word_left(&document, Position::new(0, column)),
                ),
                (
                    motions::word_right_byte(text, byte),
                    motions::word_right(&document, Position::new(0, column)),
                ),
            ] {
                assert_eq!(text[..standalone].chars().count(), document_position.column);
            }
        }
    }
    assert_eq!(motions::word_left_byte("a\r\nb", 3), 0);
    assert_eq!(motions::word_right_byte("a\r\nb", 0), 3);
    assert_eq!(motions::word_right_byte("猫", usize::MAX), 3);
    assert_eq!(motions::word_left_byte("猫", 1), 0);
}

#[test]
fn smart_home_never_lands_between_a_space_and_its_combining_mark() {
    let document = Document::new(" \u{301}abc\n  cafe\u{301}");
    assert_eq!(
        motions::line_start_smart(&document, Position::new(0, 5)),
        Position::new(0, 0)
    );
    assert_eq!(
        motions::line_start_smart(&document, Position::new(1, 7)),
        Position::new(1, 2)
    );
    assert_eq!(
        motions::line_start_smart(&document, Position::new(1, 2)),
        Position::new(1, 0)
    );
}

#[test]
fn word_motions_and_selection_include_combining_marks() {
    let doc = Document::new("cafe\u{301} next\n👩🏽‍💻 next");
    assert_eq!(
        motions::word_right(&doc, Position::new(0, 0)),
        Position::new(0, 6)
    );
    assert_eq!(
        motions::word_left(&doc, Position::new(0, 6)),
        Position::new(0, 0)
    );
    assert_eq!(
        motions::word_right(&doc, Position::new(1, 0)),
        Position::new(1, 5)
    );
    assert_eq!(
        motions::word_left(&doc, Position::new(1, 5)),
        Position::new(1, 0)
    );
    let selection = motions::select_word_at(&doc, Position::new(0, 4));
    assert_eq!(doc.slice(selection.range()), "cafe\u{301}");
}

#[test]
fn deletion_from_an_interior_caret_removes_the_containing_cluster() {
    for code in [KeyCode::Backspace, KeyCode::Delete] {
        let mut doc = Document::new("e\u{301}x\n👩🏽‍💻x");
        let mut cursor = cursors_at(&[(0, 1), (1, 2)]);
        let before = cursor.clone();
        let mut handler = KeyboardHandler::new();
        let command =
            press(&mut handler, &KeyEvent::simple(code), &mut doc, &mut cursor).expect("deletion");
        assert_eq!(doc.text(), "x\nx");
        command
            .inverse()
            .apply(&mut doc, &mut cursor)
            .expect("undo");
        assert_eq!(doc.text(), "e\u{301}x\n👩🏽‍💻x");
        assert_eq!(cursor, before);
    }
}

#[test]
fn shift_left_selects_complete_reversed_clusters_for_replacement() {
    let mut doc = Document::new("e\u{301}x\n👩🏽‍💻x");
    let mut cursor = cursors_at(&[(0, 2), (1, 4)]);
    let mut handler = KeyboardHandler::new();
    press(
        &mut handler,
        &KeyEvent::new(KeyCode::Left, Modifiers::shift()),
        &mut doc,
        &mut cursor,
    );
    assert!(
        cursor
            .all_selections()
            .all(|selection| selection.anchor > selection.head)
    );
    assert_eq!(
        cursor
            .all_selections()
            .map(|selection| doc.slice(selection.range()))
            .collect::<Vec<_>>(),
        vec!["e\u{301}", "👩🏽‍💻"]
    );
    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Char('z')),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(doc.text(), "zx\nzx");
}

#[test]
fn an_explicit_partial_selection_keeps_its_exact_edit_range() {
    let mut doc = Document::new("e\u{301}x");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 2), Position::new(0, 1))]);
    let mut handler = KeyboardHandler::new();
    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Delete),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(doc.text(), "ex");
}

#[test]
fn arrows_cross_whole_clusters_at_every_cursor() {
    let mut doc = Document::new("e\u{301}x\n👩🏽‍💻x\n🇦🇺x");
    let mut cursor = cursors_at(&[(0, 0), (1, 0), (2, 0)]);
    let mut handler = KeyboardHandler::new();
    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Right),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(heads(&cursor), vec![(0, 2), (1, 4), (2, 2)]);
    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Left),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(heads(&cursor), vec![(0, 0), (1, 0), (2, 0)]);
}

#[test]
fn backspace_removes_whole_clusters_and_undo_restores_all_cursors() {
    let original = "e\u{301}x\n👩🏽‍💻x\n🇦🇺x";
    let mut doc = Document::new(original);
    let mut cursor = cursors_at(&[(0, 2), (1, 4), (2, 2)]);
    let before = cursor.clone();
    let mut handler = KeyboardHandler::new();
    let command = press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Backspace),
        &mut doc,
        &mut cursor,
    )
    .expect("deletion command");
    assert_eq!(doc.text(), "x\nx\nx");
    command
        .inverse()
        .apply(&mut doc, &mut cursor)
        .expect("undo");
    assert_eq!(doc.text(), original);
    assert_eq!(cursor, before);
}

#[test]
fn forward_delete_removes_whole_clusters_at_every_cursor() {
    let mut doc = Document::new("e\u{301}x\n👩🏽‍💻x\n🇦🇺x");
    let mut cursor = cursors_at(&[(0, 0), (1, 0), (2, 0)]);
    let mut handler = KeyboardHandler::new();
    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Delete),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(doc.text(), "x\nx\nx");
    assert_eq!(heads(&cursor), vec![(0, 0), (1, 0), (2, 0)]);
}

#[test]
fn vertical_motion_snaps_before_cluster_and_keeps_sticky_column() {
    let mut doc = Document::new("abcd\ne\u{301}xy\nabcd");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();
    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Down),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(heads(&cursor), vec![(1, 0)]);
    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Down),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(heads(&cursor), vec![(2, 1)]);
}
