//! Structural selections are visible and editable through ordinary terminal input.
use super::*;
use iridium_editor::{CursorState, Position, Selection};

fn fixture() -> App {
    let mut app = unnamed();
    let editor = app.workspace.active_editor_mut().expect("editor");
    editor.set_content("[11,22,33]");
    editor.set_language(Language::Json);
    editor.set_cursor(Position::new(0, 1));
    app.resize(80, 10);
    app
}

fn structural_arrow(app: &mut App, key: KeyCode) {
    app.handle_input(&TerminalInput::Key(KeyEvent::new(
        key,
        Modifiers {
            shift: true,
            alt: true,
            ..Modifiers::none()
        },
    )));
}

fn palette(app: &mut App, command: &str) {
    app.handle_input(&ctrl('k'));
    assert!(app.is_palette_open());
    type_text(app, command);
    app.handle_input(&press(KeyCode::Enter));
    assert!(!app.is_palette_open());
}

#[test]
fn expand_and_shrink_restore_multiple_cursors_after_they_merge() {
    let mut app = fixture();
    let editor = app.workspace.active_editor_mut().expect("editor");
    let cursors = CursorState::from_selections(
        Selection::new(Position::new(0, 1), Position::new(0, 3)),
        vec![
            Selection::new(Position::new(0, 4), Position::new(0, 6)),
            Selection::new(Position::new(0, 7), Position::new(0, 9)),
        ],
    );
    editor.set_selections(cursors.clone());
    structural_arrow(&mut app, KeyCode::Right);
    let editor = app.editor().expect("editor");
    assert_eq!(editor.state().cursor.cursor_count(), 1);
    assert_eq!(editor.selected_syntax_kind(), Some("array"));
    app.resize(40, 3);
    structural_arrow(&mut app, KeyCode::Left);
    assert_eq!(app.editor().expect("editor").state().cursor, cursors);
    app.handle_input(&TerminalInput::Paste("0".into()));
    assert_eq!(app.editor().expect("editor").content(), "[0,0,0]");
    app.handle_input(&ctrl('z'));
    assert_eq!(app.editor().expect("editor").content(), "[11,22,33]");
    assert_eq!(app.editor().expect("editor").state().cursor, cursors);
}

#[test]
fn palette_exposes_node_sibling_and_child_navigation() {
    let mut app = fixture();
    palette(&mut app, "Select Syntax Node");
    assert_eq!(
        app.editor().expect("editor").selected_syntax_kind(),
        Some("number")
    );
    palette(&mut app, "Select Next Sibling Node");
    assert_eq!(
        app.editor().expect("editor").state().cursor.primary.start(),
        Position::new(0, 4)
    );
    palette(&mut app, "Select Previous Sibling Node");
    assert_eq!(
        app.editor().expect("editor").state().cursor.primary.start(),
        Position::new(0, 1)
    );
    structural_arrow(&mut app, KeyCode::Right);
    palette(&mut app, "Select First Child Node");
    assert_eq!(
        app.editor().expect("editor").state().cursor.primary.range(),
        iridium_editor::Range::new(Position::new(0, 1), Position::new(0, 3))
    );
    assert_eq!(
        app.editor().expect("editor").selected_syntax_kind(),
        Some("number")
    );
}
