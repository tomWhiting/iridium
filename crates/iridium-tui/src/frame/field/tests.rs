use super::*;
use crate::cell::CellContent;

#[test]
fn replacing_selection_does_not_move_past_a_temporarily_joined_cluster() {
    let mut field = Field::default();
    field.insert_text("\rX\n");
    field.move_home();
    field.move_right();
    field.navigate(Motion::Right, true);
    assert_eq!(field.selected_text(), Some("X"));
    field.insert_text("Y");
    assert_eq!(field.text(), "\rY\n");
    assert_eq!(field.caret, 2);
    field.navigate(Motion::Left, true);
    assert!(field.backspace());
    assert_eq!(field.text(), "\r\n");
    assert_eq!(field.caret, 2);
}

#[test]
fn selection_reversal_retains_its_anchor_and_empty_paste_keeps_selection() {
    let mut field = ending_at("abc");
    field.move_left();
    field.navigate(Motion::Left, true);
    assert_eq!(field.selected_text(), Some("b"));
    field.navigate(Motion::Right, true);
    assert_eq!(field.selected_text(), None);
    field.navigate(Motion::Right, true);
    assert_eq!(field.selected_text(), Some("c"));
    assert!(!field.insert_text(""));
    assert_eq!(field.selected_text(), Some("c"));
    field.insert_text("猫");
    assert_eq!(field.text(), "ab猫");
}

/// A family emoji: seven `char`s, one cluster, two cells.
const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";

/// A field holding `text` with the caret at its end.
fn ending_at(text: &str) -> Field {
    let mut field = Field::default();
    for character in text.chars() {
        assert!(field.insert(character), "{character:?} must be insertable");
    }
    field
}

/// The text of a row, with continuation cells contributing nothing.
fn row_text(buffer: &CellBuffer, row: usize) -> String {
    let mut out = String::new();
    let Some(cells) = buffer.row(row) else {
        return out;
    };
    for cell in cells {
        match cell.content() {
            CellContent::Grapheme(grapheme) => grapheme.push_to(&mut out),
            CellContent::Continuation => {},
        }
    }
    out
}

#[test]
fn deleting_between_cr_and_lf_keeps_the_joined_cluster_editable() {
    let mut field = Field::default();
    assert!(field.insert_text("\rX\n"));
    assert!(field.move_home());
    assert!(field.move_right());
    assert!(field.delete());
    assert_eq!(field.text(), "\r\n");
    assert_eq!(field.caret, 2);
    assert!(field.backspace());
    assert!(field.is_empty());
}

#[test]
fn bulk_paste_keeps_control_bytes_and_edits_crlf_as_one_cluster() {
    let mut field = Field::default();
    assert!(field.insert_text("猫\r\n"));
    assert_eq!(field.caret_cell(), 4);
    assert!(field.backspace());
    assert_eq!(field.text(), "猫");
    assert_eq!(field.caret_cell(), 2);
    assert!(field.insert_text("\tX"));
    assert!(field.move_left());
    assert_eq!(field.caret_cell(), 3);
    assert!(field.backspace());
    assert_eq!(field.text(), "猫X");
}

#[test]
fn paste_before_a_combining_mark_leaves_caret_after_the_joined_cluster() {
    let mut field = Field::default();
    assert!(field.insert_text("\u{301}X"));
    assert!(field.move_home());
    assert!(field.insert_text("e"));
    assert_eq!(field.text(), "e\u{301}X");
    assert_eq!(field.caret, "e\u{301}".len());
    assert_eq!(field.caret_cell(), 1);
    assert!(field.backspace());
    assert_eq!(field.text(), "X");
}

#[test]
fn typing_ascii_fills_the_field_and_moves_the_caret() {
    let field = ending_at("foo");
    assert_eq!(field.text(), "foo");
    assert_eq!(field.caret_cell(), 3);
}

#[test]
fn the_caret_counts_cells_not_chars_after_a_double_width_glyph() {
    let field = ending_at("漢字");
    assert_eq!(field.text().chars().count(), 2);
    assert_eq!(
        field.caret_cell(),
        4,
        "two ideographs are four cells, not two"
    );
}

#[test]
fn the_caret_counts_one_cell_for_a_combining_sequence() {
    let field = ending_at("e\u{301}");
    assert_eq!(field.text().chars().count(), 2);
    assert_eq!(field.caret_cell(), 1);
}

#[test]
fn backspace_removes_a_whole_cluster_not_a_char() {
    // Removing one `char` would leave a zero-width joiner dangling and the
    // query would search for text no document contains.
    let mut field = ending_at(FAMILY);
    assert_eq!(field.text().chars().count(), 7);
    assert!(field.backspace());
    assert_eq!(field.text(), "", "one press must remove the whole cluster");
    assert!(!field.backspace());
}

#[test]
fn backspace_removes_a_whole_combining_sequence() {
    let mut field = ending_at("ae\u{301}");
    assert!(field.backspace());
    assert_eq!(field.text(), "a");
}

#[test]
fn moving_left_and_right_steps_over_whole_clusters() {
    let mut field = ending_at(&format!("a{FAMILY}b"));
    assert_eq!(field.caret_cell(), 4);
    assert!(field.move_left());
    assert_eq!(field.caret_cell(), 3, "before the `b`");
    assert!(field.move_left());
    assert_eq!(field.caret_cell(), 1, "before the family, in one step");
    assert!(field.move_left());
    assert_eq!(field.caret_cell(), 0);
    assert!(!field.move_left());

    assert!(field.move_right());
    assert_eq!(field.caret_cell(), 1);
    assert!(field.move_right());
    assert_eq!(field.caret_cell(), 3, "past the family, in one step");
}

#[test]
fn deleting_forwards_removes_the_cluster_at_the_caret() {
    let mut field = ending_at("a漢b");
    field.move_home();
    assert!(field.move_right());
    assert!(field.delete());
    assert_eq!(field.text(), "ab");
    assert_eq!(field.caret_cell(), 1, "the caret does not move");
}

#[test]
fn deleting_at_the_end_does_nothing() {
    let mut field = ending_at("ab");
    assert!(!field.delete());
    assert_eq!(field.text(), "ab");
}

#[test]
fn insertion_lands_at_the_caret_rather_than_the_end() {
    let mut field = ending_at("ac");
    assert!(field.move_left());
    assert!(field.insert('b'));
    assert_eq!(field.text(), "abc");
    assert_eq!(field.caret_cell(), 2);
}

#[test]
fn home_and_end_report_whether_they_moved() {
    let mut field = ending_at("abc");
    assert!(!field.move_end());
    assert!(field.move_home());
    assert!(!field.move_home());
    assert!(field.move_end());
    assert_eq!(field.caret_cell(), 3);
}

#[test]
fn control_characters_are_refused() {
    let mut field = Field::default();
    for character in ['\n', '\t', '\r', '\u{7}', '\u{1b}'] {
        assert!(
            !field.insert(character),
            "{character:?} must not enter a field"
        );
    }
    assert!(field.is_empty());
}

#[test]
fn the_empty_field_has_no_motion_and_no_width() {
    let mut field = Field::default();
    assert!(field.is_empty());
    assert_eq!(field.caret_cell(), 0);
    assert!(!field.backspace());
    assert!(!field.delete());
    assert!(!field.move_left());
    assert!(!field.move_right());
}

#[test]
fn the_scroll_keeps_the_caret_inside_the_box() {
    assert_eq!(scroll_for(0, 10), 0);
    assert_eq!(scroll_for(9, 10), 0);
    assert_eq!(scroll_for(10, 10), 1, "the caret's own cell must fit");
    assert_eq!(scroll_for(25, 10), 16);
    assert_eq!(scroll_for(3, 0), 0, "a box with no cells cannot scroll");
}

#[test]
fn painting_blanks_the_part_of_the_box_the_text_does_not_fill() {
    let field = ending_at("ab");
    let mut buffer = CellBuffer::new(8, 1);
    let area = TextArea {
        origin: 2,
        width: 4,
        scroll: 0,
    };
    field.paint(&mut buffer, 0, area, Style::DEFAULT);
    assert_eq!(row_text(&buffer, 0), "  ab    ");
}

#[test]
fn a_field_wider_than_its_box_scrolls_to_the_caret() {
    let field = ending_at("abcdefgh");
    let mut buffer = CellBuffer::new(4, 1);
    let area = TextArea {
        origin: 0,
        width: 4,
        scroll: scroll_for(field.caret_cell(), 4),
    };
    field.paint(&mut buffer, 0, area, Style::DEFAULT);
    assert_eq!(row_text(&buffer, 0), "fgh ");
}

#[test]
fn a_glyph_the_scroll_cuts_through_is_painted_as_a_space() {
    // `漢` occupies cells 0 and 1; scrolling to 1 shows half of it, which
    // is a state no terminal can render.
    let field = ending_at("漢ab");
    let mut buffer = CellBuffer::new(3, 1);
    let area = TextArea {
        origin: 0,
        width: 3,
        scroll: 1,
    };
    field.paint(&mut buffer, 0, area, Style::DEFAULT);
    assert_eq!(row_text(&buffer, 0), " ab");
}
