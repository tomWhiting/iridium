//! Keyboard intentions must survive host-only viewport changes.
use super::*;
use iridium_editor::Position;

fn fixture() -> App {
    let mut app = unnamed();
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_content("abcdefghij\nxy\nabcdefghij\nabcdefghij");
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_cursor(Position::new(0, 8));
    app
}

fn add_below(app: &mut App) {
    app.handle_input(&TerminalInput::Key(KeyEvent::new(
        KeyCode::Down,
        Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::none()
        },
    )));
}

#[test]
fn sticky_column_survives_scrolling_across_a_one_row_viewport() {
    let mut app = fixture();
    app.resize(60, 3);
    app.handle_input(&press(KeyCode::Down));
    assert_eq!(app.editor().expect("editor").cursor(), Position::new(1, 2));
    app.handle_input(&press(KeyCode::Down));
    assert_eq!(app.editor().expect("editor").cursor(), Position::new(2, 8));
}

#[test]
fn sticky_column_survives_resize_between_vertical_motions() {
    let mut app = fixture();
    app.resize(60, 10);
    app.handle_input(&press(KeyCode::Down));
    app.resize(80, 12);
    app.handle_input(&press(KeyCode::Down));
    assert_eq!(app.editor().expect("editor").cursor(), Position::new(2, 8));
}

#[test]
fn resize_keeps_the_real_cursor_addition_order_for_remove_last() {
    let mut app = fixture();
    app.resize(60, 10);
    add_below(&mut app);
    let two = app.editor().expect("editor").state().cursor.clone();
    add_below(&mut app);
    assert_eq!(
        app.editor().expect("editor").state().cursor.cursor_count(),
        3
    );
    app.resize(80, 12);
    app.handle_input(&ctrl('u'));
    assert_eq!(app.editor().expect("editor").state().cursor, two);
}

#[test]
fn resize_does_not_abort_a_user_key_sequence_in_progress() {
    let binding = iridium_editor::KeyBinding::parse(
        "ctrl+alt+j ctrl+l",
        iridium_editor::commands::builtin::CURSOR_CHAR_RIGHT,
    )
    .expect("binding");
    let mut app = with_bindings(vec![binding]);
    type_text(&mut app, "abc");
    app.handle_input(&press(KeyCode::Home));
    app.handle_input(&ctrl_alt('j'));
    assert!(
        !app.editor()
            .expect("editor")
            .pending_key_sequence()
            .is_empty()
    );
    app.resize(80, 12);
    app.handle_input(&ctrl('l'));
    assert_eq!(app.editor().expect("editor").cursor(), Position::new(0, 1));
}

#[test]
fn shift_selection_keeps_its_anchor_and_column_across_scroll_and_resize() {
    let mut app = fixture();
    app.resize(60, 3);
    let shift_down = TerminalInput::Key(KeyEvent::new(
        KeyCode::Down,
        Modifiers {
            shift: true,
            ..Modifiers::none()
        },
    ));
    app.handle_input(&shift_down);
    app.resize(40, 4);
    app.handle_input(&shift_down);
    let selection = app.editor().expect("editor").state().cursor.primary;
    assert_eq!(selection.anchor, Position::new(0, 8));
    assert_eq!(selection.head, Position::new(2, 8));
}

#[test]
fn repeated_resize_preserves_each_multicursor_vertical_column() {
    let mut app = fixture();
    app.resize(60, 10);
    add_below(&mut app);
    app.resize(30, 3);
    app.handle_input(&press(KeyCode::Down));
    app.resize(90, 7);
    app.handle_input(&press(KeyCode::Down));
    let heads: Vec<_> = app
        .editor()
        .expect("editor")
        .state()
        .cursor
        .all_selections()
        .map(|selection| selection.head)
        .collect();
    assert_eq!(heads, [Position::new(3, 8), Position::new(2, 8)]);
}

#[test]
fn added_cursor_is_active_and_visible_in_one_row_viewport() {
    let mut app = fixture();
    app.resize(60, 3);
    add_below(&mut app);
    let editor = app.editor().expect("editor");
    assert_eq!(editor.cursor(), Position::new(1, 2));
    assert_eq!(editor.state().viewport.first_line, 1);
    add_below(&mut app);
    let editor = app.editor().expect("editor");
    assert_eq!(editor.cursor(), Position::new(2, 8));
    assert_eq!(editor.state().viewport.first_line, 2);
    app.handle_input(&ctrl('u'));
    assert_eq!(app.editor().expect("editor").cursor(), Position::new(1, 2));
    app.handle_input(&press(KeyCode::Escape));
    let editor = app.editor().expect("editor");
    assert_eq!(editor.cursor(), Position::new(1, 2));
    assert_eq!(editor.state().cursor.cursor_count(), 1);
}

#[test]
fn added_occurrence_scrolls_to_new_target_and_undo_restores_focus() {
    let mut app = unnamed();
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_content("needle\n\n\nneedle\n\nneedle");
    app.resize(60, 3);
    app.handle_input(&ctrl('d'));
    app.handle_input(&ctrl('d'));
    let before = app.editor().expect("editor").state().cursor.clone();
    assert_eq!(before.primary.head, Position::new(3, 6));
    assert_eq!(app.editor().expect("editor").state().viewport.first_line, 3);
    type_text(&mut app, "X");
    assert_eq!(
        app.editor().expect("editor").content(),
        "X\n\n\nX\n\nneedle"
    );
    app.handle_input(&ctrl('z'));
    assert_eq!(app.editor().expect("editor").state().cursor, before);
    app.handle_input(&press(KeyCode::Escape));
    assert_eq!(app.editor().expect("editor").cursor(), Position::new(3, 6));
}
