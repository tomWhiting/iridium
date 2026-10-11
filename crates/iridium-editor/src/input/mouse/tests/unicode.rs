use super::*;

#[test]
fn double_click_and_word_drag_keep_clusters_with_their_run() {
    let document = Document::new("cafe\u{301} 👩🏽‍💻 !");
    for column in 0..5 {
        let selection = MouseHandler::select_word_at(Position::new(0, column), &document);
        assert_eq!(document.slice(selection.range()), "cafe\u{301}");
    }
    let emoji = MouseHandler::select_word_at(Position::new(0, 8), &document);
    assert_eq!(document.slice(emoji.range()), "👩🏽‍💻");
    let selection =
        MouseHandler::create_word_selection(Position::new(0, 8), Position::new(0, 4), &document);
    assert!(selection.anchor > selection.head);
    assert_eq!(document.slice(selection.range()), "cafe\u{301} 👩🏽‍💻");
}

#[test]
fn pointer_grid_snaps_inside_clusters_and_preserves_boundary_columns() {
    let document = Document::new("e\u{301}👩🏽‍💻x");
    let viewport = create_test_viewport();
    let handler = MouseHandler::new();
    // This grid carries scalar columns; measured compositor hit-testing is
    // upstream. Verify each scalar location without claiming glyph geometry.
    for (x, expected) in [
        (18.0, 0),
        (30.0, 2),
        (42.0, 2),
        (54.0, 2),
        (66.0, 2),
        (78.0, 6),
    ] {
        assert_eq!(
            handler.pixel_to_position(x + 48.0, 0.0, &document, &viewport),
            Position::new(0, expected)
        );
    }
}
