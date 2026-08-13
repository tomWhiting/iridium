//! Cells, not characters: cluster width, and which cells a match paints.
//!
//! Split out of a 1,136-line `tests.rs` for #92, on the rules the file
//! already carried. The fixtures and the [`Screen`](super::Screen) reader are
//! in [`super`].

use super::*;

// ==================== A cell is not a character ====================

#[test]
fn a_double_width_query_puts_the_caret_two_cells_along_per_glyph() {
    let mut editor = editor_with("漢字 test");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "漢字");
    assert_eq!(editor.search_match_count(), 1);

    let screen = render(&editor, &overlay, 60, 8);
    let (first, _) = screen.layout.search_rows.expect("panel");
    let caret = screen.layout.caret().expect("the query field has a caret");
    assert_eq!(caret.row, first);
    // Nine cells of label, then four cells for two ideographs — not two.
    assert_eq!(
        caret.column,
        13,
        "the caret must count cells, not chars: {}",
        screen.find_row()
    );
    assert!(screen.find_row().contains("漢字"), "{}", screen.find_row());
}

#[test]
fn a_grapheme_cluster_is_one_backspace_however_many_chars_it_is() {
    let mut editor = editor_with("nothing");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, FAMILY);
    assert_eq!(overlay.query().chars().count(), 7);

    press(&mut overlay, &mut editor, KeyCode::Backspace);
    assert_eq!(
        overlay.query(),
        "",
        "one press must take the whole cluster, joiners and all"
    );
}

#[test]
fn an_emoji_query_is_two_cells_wide_in_the_caret_position() {
    let mut editor = editor_with("nothing");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, FAMILY);

    let caret = render(&editor, &overlay, 60, 8)
        .layout
        .caret()
        .expect("caret");
    assert_eq!(caret.column, 9 + 2, "seven chars, two cells");
}

#[test]
fn a_query_wider_than_its_box_scrolls_to_keep_the_caret_visible() {
    let mut editor = editor_with("nothing");
    let mut overlay = opened(&mut editor);
    // Thirty cells of query in a panel about twenty cells wide.
    type_text(&mut overlay, &mut editor, "abcdefghijklmnopqrstuvwxyz0123");

    let screen = render(&editor, &overlay, 30, 8);
    let caret = screen.layout.caret().expect("caret");
    assert!(
        caret.column < 30,
        "the caret must stay on screen: {caret:?} in {}",
        screen.find_row()
    );
    assert!(
        screen.find_row().contains("0123"),
        "the end of the query must be what is shown: {}",
        screen.find_row()
    );
}

// ==================== Match highlighting ====================

/// The background of the first cell of the text area on `row`.
fn text_background(screen: &Screen, row: usize, offset: usize) -> Color {
    screen
        .background(screen.layout.text.origin + offset, row)
        .expect("the text area must be painted")
}

#[test]
fn every_match_is_marked_and_the_current_one_is_marked_apart() {
    // Marking only the current match makes the rest invisible; marking them all
    // the same makes navigation unreadable. Both are silent: the editor still
    // renders and the counts are still right.
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");

    // Sampled one cell into each match: `find` parks the caret on a match's
    // first cell, and a caret paints its own background over whatever it sits
    // on — so cell zero reports the caret rather than the match.
    let screen = render(&editor, &overlay, 60, 8);
    let current = text_background(&screen, 0, 1);
    let other = text_background(&screen, 0, 9);
    let plain = text_background(&screen, 0, 4);

    assert_ne!(current, plain, "the current match must be marked");
    assert_ne!(other, plain, "every other match must be marked too");
    assert_ne!(current, other, "and the current one must stand apart");
}

#[test]
fn the_current_match_marking_follows_navigation() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    let before = render(&editor, &overlay, 60, 8);
    // One cell in, away from the caret the search parks on the match start.
    let current_style = text_background(&before, 0, 1);
    assert_ne!(
        text_background(&before, 0, 9),
        current_style,
        "the second match must not start out as the current one"
    );

    press(&mut overlay, &mut editor, KeyCode::Enter);
    let after = render(&editor, &overlay, 60, 8);
    assert_eq!(
        text_background(&after, 0, 9),
        current_style,
        "the second match must now carry the current-match background"
    );
    assert_ne!(
        text_background(&after, 0, 1),
        current_style,
        "and the first must have given it up"
    );
}

#[test]
fn a_match_covers_exactly_its_own_cells() {
    let mut editor = editor_with("xfoox");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");

    let screen = render(&editor, &overlay, 60, 8);
    let outside = text_background(&screen, 0, 4);
    for offset in 1..4 {
        assert_ne!(
            text_background(&screen, 0, offset),
            outside,
            "cell {offset} is inside the match"
        );
    }
    assert_eq!(
        text_background(&screen, 0, 0),
        outside,
        "the cell before the match must be untouched"
    );
}

#[test]
fn a_double_width_match_is_marked_across_both_of_its_cells() {
    let mut editor = editor_with("a漢b");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "漢");

    let screen = render(&editor, &overlay, 60, 8);
    let marked = text_background(&screen, 0, 1);
    assert_eq!(
        text_background(&screen, 0, 2),
        marked,
        "the continuation half must carry its glyph's background"
    );
    assert_ne!(text_background(&screen, 0, 3), marked, "the `b` must not");
}

#[test]
fn a_match_after_a_double_width_glyph_is_measured_in_cells() {
    // `字` is the second *char* of the line and its third *cell*. Resolving a
    // match's document columns as cells marks the ideograph before it instead,
    // which is the whole reason ranges go through `LineLayout`.
    let mut editor = editor_with("漢字x");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "字");
    assert_eq!(editor.search_match_count(), 1);

    let screen = render(&editor, &overlay, 60, 8);
    let plain = text_background(&screen, 0, 4);
    assert_ne!(
        text_background(&screen, 0, 2),
        plain,
        "`字` is cells 2 and 3"
    );
    assert_ne!(text_background(&screen, 0, 3), plain);
    assert_eq!(
        text_background(&screen, 0, 0),
        plain,
        "`漢` is not the match and must not be marked"
    );
    assert_eq!(text_background(&screen, 0, 1), plain, "nor its second half");
}

#[test]
fn a_match_ending_after_a_double_width_glyph_covers_its_last_cell() {
    // `漢a` is two document columns and three cells. Treating the match's end
    // column as a cell stops the mark one glyph short and leaves the `a`
    // unmarked — visible only because the ideograph before it is two cells wide,
    // which is exactly the case an ASCII test cannot reach.
    let mut editor = editor_with("漢ab x");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "漢a");
    assert_eq!(editor.search_match_count(), 1);

    let screen = render(&editor, &overlay, 60, 8);
    let plain = text_background(&screen, 0, 3);
    assert_ne!(
        text_background(&screen, 0, 2),
        plain,
        "the `a` is the match's last cell and must be marked"
    );
    assert_eq!(
        text_background(&screen, 0, 4),
        plain,
        "and the space after `b` must not be"
    );
}

#[test]
fn matches_on_lines_below_the_first_are_marked_too() {
    let mut editor = editor_with("foo\nbar\nfoo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");

    let screen = render(&editor, &overlay, 60, 8);
    let plain = text_background(&screen, 1, 0);
    assert_ne!(
        text_background(&screen, 2, 0),
        plain,
        "the match on line 2 must be marked"
    );
}

#[test]
fn closing_the_search_takes_the_marks_with_it() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    let marked = render(&editor, &overlay, 60, 8);
    assert_ne!(
        text_background(&marked, 0, 8),
        text_background(&marked, 0, 4)
    );

    press(&mut overlay, &mut editor, KeyCode::Escape);
    let cleared = render(&editor, &overlay, 60, 8);
    assert_eq!(
        text_background(&cleared, 0, 8),
        text_background(&cleared, 0, 4),
        "a closed search must leave no marks behind"
    );
}
