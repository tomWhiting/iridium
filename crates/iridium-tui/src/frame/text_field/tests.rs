use super::*;
use iridium_editor::Modifiers;
fn event(key: KeyCode, modifiers: Modifiers) -> KeyEvent {
    KeyEvent::new(key, modifiers)
}
#[test]
fn exact_multiline_paste_and_select_all_replacement_respect_output_limit() {
    let mut field = TextField::default();
    field.paste("猫\r\n👨‍👩‍👧‍👦\t", 128).expect("paste");
    let before = field.clone();
    assert!(field.paste(&"x".repeat(129), 128).is_err());
    assert_eq!(field, before);
    field
        .handle_key(
            &event(
                KeyCode::Char('a'),
                Modifiers {
                    ctrl: true,
                    ..Modifiers::none()
                },
            ),
            128,
        )
        .expect("select all");
    field.paste("é\n", 4).expect("replace selection");
    assert_eq!(field.text(), "é\n");
    field
        .handle_key(&event(KeyCode::Backspace, Modifiers::none()), 128)
        .expect("delete newline");
    field
        .handle_key(&event(KeyCode::Backspace, Modifiers::none()), 128)
        .expect("delete grapheme");
    assert_eq!(field.text(), "");
}
#[test]
fn word_motion_shift_extension_and_grapheme_deletion_share_existing_rules() {
    let mut field = TextField::default();
    field.paste("one two e\u{301}", 128).expect("paste");
    field
        .handle_key(&event(KeyCode::Backspace, Modifiers::none()), 128)
        .expect("cluster delete");
    assert_eq!(field.text(), "one two ");
    field
        .handle_key(
            &event(
                KeyCode::Left,
                Modifiers {
                    alt: true,
                    shift: true,
                    ..Modifiers::none()
                },
            ),
            128,
        )
        .expect("word selection");
    assert_eq!(field.field.selected_text(), Some("two "));
    field.paste("猫", 128).expect("replace");
    assert_eq!(field.text(), "one 猫");
}
#[test]
fn long_field_scroll_keeps_caret_visible_and_controls_are_display_only() {
    let mut field = TextField::default();
    field
        .paste(&format!("{}\nEND", "猫".repeat(100)), 1024)
        .expect("paste");
    let mut buffer = CellBuffer::new(24, 3);
    let palette = Palette::from_theme(&iridium_editor::Theme::default());
    let column = field
        .paint(&mut buffer, 1, "Find: ", true, &palette)
        .expect("caret");
    assert!(column < 24);
    let mut text = String::new();
    for cell in buffer.row(1).expect("row") {
        if let crate::cell::CellContent::Grapheme(g) = cell.content() {
            g.push_to(&mut text);
        }
    }
    assert!(text.contains("␊END"), "{text}");
    assert!(field.text().ends_with("\nEND"));
}

#[test]
fn cut_receipt_is_bound_to_field_selection_and_cannot_be_replayed() {
    let mut field = TextField::default();
    field.paste("猫\n", 128).expect("paste");
    field
        .handle_key(&event(KeyCode::Char('a'), Modifiers::ctrl()), 128)
        .expect("select");
    let cut = field.capture_cut().expect("cut");
    assert_eq!(cut.text(), "猫\n");
    assert!(field.clone().finish_cut(&cut).is_err());
    let mut other = TextField::default();
    other.paste("猫\n", 128).expect("paste");
    other
        .handle_key(&event(KeyCode::Char('a'), Modifiers::ctrl()), 128)
        .expect("select");
    assert!(other.finish_cut(&cut).is_err());
    field.finish_cut(&cut).expect("delivered");
    assert_eq!(field.text(), "");
    assert!(field.finish_cut(&cut).is_err());
}

#[test]
fn mouse_geometry_preserves_raw_clusters_and_control_markers() {
    let mut field = TextField::default();
    let text = "猫e\u{301}\t\r\nEND";
    field.paste(text, 128).expect("paste");
    field.point(6, 80, "Find: ", false);
    field.point(15, 80, "Find: ", true);
    assert_eq!(field.selected_text(), Some(text));
    field.point(7, 80, "Find: ", false);
    assert_eq!(field.field.caret_cell(), 2);
    field.point(10, 80, "Find: ", false);
    field.point(11, 80, "Find: ", true);
    assert_eq!(field.selected_text(), Some("\r\n"));
}
#[test]
fn mouse_uses_the_painted_horizontal_scroll_and_clamps_edges() {
    let mut field = TextField::default();
    field.paste("abcdefghijklmnop", 128).expect("paste");
    let mut buffer = CellBuffer::new(12, 3);
    let palette = Palette::from_theme(&iridium_editor::Theme::default());
    assert_eq!(
        field.paint(&mut buffer, 1, "Find: ", true, &palette),
        Some(11)
    );
    field.point(6, 12, "Find: ", false);
    assert_eq!(field.field.caret_cell(), 11);
    assert_eq!(
        field.paint(&mut buffer, 1, "Find: ", true, &palette),
        Some(6)
    );
    field.point(999, 12, "Find: ", true);
    assert_eq!(field.selected_text(), Some("lmnop"));
    field.point(0, 12, "Find: ", false);
    assert!(field.field.caret_cell() < 12);
    let before = field.clone();
    field.point(0, 0, "Find: ", false);
    assert_eq!(field, before);
}
