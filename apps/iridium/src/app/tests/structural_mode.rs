use super::*;
use iridium_editor::commands::structural_keymap::{STRUCTURAL, STRUCTURAL_INSERT};

#[test]
fn palette_enters_navigation_and_arrows_move_the_selected_target() {
    let directory = TempDir::new("structural-palette");
    let (mut app, path) = open(&directory, "nodes.json", "[[11,22],33]");
    app.resize(100, 24);
    app.handle_input(&press(KeyCode::Right));
    app.handle_input(&press(KeyCode::Right));
    app.handle_input(&ctrl('k'));
    type_text(&mut app, "Enter Structural Navigation");
    app.handle_input(&press(KeyCode::Enter));
    assert!(!app.is_palette_open());
    assert_eq!(app.editor().expect("editor").mode(), Some(&STRUCTURAL));
    for (key, expected) in [
        (KeyCode::Down, "22"),
        (KeyCode::Left, "[11,22]"),
        (KeyCode::Down, "33"),
        (KeyCode::Up, "[11,22]"),
        (KeyCode::Right, "11"),
        (KeyCode::Down, "22"),
    ] {
        app.handle_input(&press(key));
        let state = app.editor().expect("editor").state();
        assert_eq!(state.document.slice(state.cursor.primary.range()), expected);
    }
    app.handle_input(&press(KeyCode::Escape));
    type_text(&mut app, "9");
    app.handle_input(&ctrl('s'));
    assert_eq!(fs::read_to_string(path).expect("saved"), "[[11,9],33]");
}

#[test]
fn structural_arrows_are_installed_and_edit_all_children_through_the_host() {
    let directory = TempDir::new("structural-mode");
    let (mut app, path) = open(&directory, "nodes.json", "[[11,22],33]");
    app.resize(100, 24);
    app.handle_input(&press(KeyCode::F6));
    assert_eq!(app.editor().expect("editor").mode(), Some(&STRUCTURAL));
    app.handle_input(&press(KeyCode::Right));
    app.handle_input(&press(KeyCode::Char('a')));
    assert_eq!(
        app.editor().expect("editor").state().cursor.cursor_count(),
        2
    );
    app.handle_input(&press(KeyCode::Left));
    app.handle_input(&press(KeyCode::Right));
    app.handle_input(&press(KeyCode::Char('i')));
    assert_eq!(
        app.editor().expect("editor").mode(),
        Some(&STRUCTURAL_INSERT)
    );
    type_text(&mut app, "9");
    app.handle_input(&ctrl('s'));
    assert_eq!(fs::read_to_string(&path).expect("saved"), "[[9,9],33]");
    app.handle_input(&ctrl('z'));
    app.handle_input(&ctrl('s'));
    assert_eq!(fs::read_to_string(&path).expect("undone"), "[[11,22],33]");
}

#[test]
fn search_field_keeps_typing_while_the_document_is_in_structural_mode() {
    let directory = TempDir::new("structural-mode");
    let (mut app, _) = open(&directory, "nodes.json", "[11,22]");
    app.handle_input(&ctrl(' '));
    app.handle_input(&ctrl('f'));
    type_text(&mut app, "22");
    assert_eq!(app.editor().expect("editor").search_match_count(), 1);
    assert_eq!(app.editor().expect("editor").content(), "[11,22]");
    app.handle_input(&press(KeyCode::Escape));
    assert_eq!(app.editor().expect("editor").mode(), Some(&STRUCTURAL));
}
