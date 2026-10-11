use super::*;

#[test]
fn scope_survives_panel_replacement_and_clears_only_when_requested() {
    let mut editor = editor_with("foo foo");
    editor.set_selection(Position::zero(), Position::new(0, 3));
    editor.search_in_selections().expect("scope");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    let screen = render(&editor, &overlay, 100, 8);
    assert!(screen.find_row().contains("1 sel"));
    assert!(screen.message_row().contains("whole document"));
    overlay.handle_key(&KeyEvent::simple(KeyCode::Tab), &mut editor);
    type_text(&mut overlay, &mut editor, "x");
    overlay.handle_key(
        &KeyEvent::new(
            KeyCode::Char('r'),
            Modifiers {
                ctrl: true,
                alt: true,
                ..Modifiers::default()
            },
        ),
        &mut editor,
    );
    assert_eq!(editor.content(), "x foo");
    assert_eq!(
        editor.search_match_count(),
        0,
        "panel refresh must keep the scope"
    );
    assert!(editor.undo());
    assert_eq!(editor.search_match_count(), 1);
    overlay.handle_key(&KeyEvent::new(KeyCode::Char('s'), alt()), &mut editor);
    assert_eq!(editor.search_state().scope_range_count(), None);
    assert_eq!(editor.search_match_count(), 2);
}

#[test]
fn stale_scope_is_named_instead_of_blaming_a_valid_regex() {
    let mut editor = editor_with("foo foo");
    editor.set_selection(Position::zero(), Position::new(0, 3));
    editor.search_in_selections().expect("scope");
    let mut overlay = opened(&mut editor);
    overlay.handle_key(&KeyEvent::new(KeyCode::Char('r'), alt()), &mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    editor.set_content("foo foo foo");
    let screen = render(&editor, &overlay, 100, 8);
    assert!(screen.find_row().contains("Scope changed"));
    assert!(!screen.find_row().contains("Invalid regex"));
    assert!(screen.message_row().contains("outside tracked edits"));
    assert_eq!(editor.replace_all_matches("x"), 0);
}
