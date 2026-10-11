//! Visibility must not replace the user's edit targets.
use super::*;
use iridium_editor::{CursorState, Position, Selection};

const SOURCE: &str = "fn first() {\n    needle();\n    needle();\n}\nneedle();";

fn fixture() -> App {
    let mut app = unnamed();
    let editor = app.workspace.active_editor_mut().expect("editor");
    editor.set_language(Language::Rust);
    editor.set_content(SOURCE);
    assert!(editor.fold_at(0));
    app.resize(80, 12);
    app
}

#[test]
fn right_from_a_fold_header_end_reaches_the_next_visible_line() {
    let mut app = fixture();
    app.handle_input(&press(KeyCode::End));
    app.handle_input(&press(KeyCode::Right));
    assert_eq!(app.editor().expect("editor").cursor(), Position::new(4, 0));
}

#[test]
fn left_from_after_a_fold_reaches_the_header_end() {
    let mut app = fixture();
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_cursor(Position::new(4, 0));
    app.handle_input(&press(KeyCode::Left));
    assert_eq!(app.editor().expect("editor").cursor(), Position::new(0, 12));
}

#[test]
fn searching_hidden_text_reveals_the_match_without_moving_it() {
    let mut app = fixture();
    app.handle_input(&ctrl('f'));
    app.handle_input(&TerminalInput::Paste("needle".into()));
    assert_eq!(app.editor().expect("editor").cursor(), Position::new(1, 4));
    assert!(!app.editor().expect("editor").is_line_hidden(1));
}

#[test]
fn selecting_hidden_matches_keeps_every_edit_target_and_undo() {
    let mut app = fixture();
    app.handle_input(&ctrl('f'));
    app.handle_input(&TerminalInput::Paste("needle".into()));
    app.handle_input(&TerminalInput::Key(KeyEvent::new(
        KeyCode::Enter,
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
    )));
    let cursors = app.editor().expect("editor").state().cursor.clone();
    assert_eq!(cursors.cursor_count(), 3);
    app.handle_input(&TerminalInput::Paste("name".into()));
    assert_eq!(
        app.editor().expect("editor").content(),
        SOURCE.replace("needle", "name")
    );
    app.handle_input(&ctrl('z'));
    assert_eq!(app.editor().expect("editor").content(), SOURCE);
    assert_eq!(app.editor().expect("editor").state().cursor, cursors);
}

#[test]
fn explicit_folding_preserves_cursors_outside_the_fold() {
    let mut app = fixture();
    let editor = app.workspace.active_editor_mut().expect("editor");
    editor.unfold_all();
    editor.state_mut().cursor = CursorState::from_selections(
        Selection::collapsed(Position::new(1, 4)),
        vec![Selection::collapsed(Position::new(4, 4))],
    );
    app.handle_input(&ctrl_alt('f'));
    let heads: Vec<_> = app
        .editor()
        .expect("editor")
        .state()
        .cursor
        .all_selections()
        .map(|s| s.head)
        .collect();
    assert_eq!(heads, [Position::new(0, 0), Position::new(4, 4)]);
    assert!(app.editor().expect("editor").is_line_hidden(1));
}

#[test]
fn document_end_reveals_a_folded_final_line() {
    let mut app = fixture();
    let editor = app.workspace.active_editor_mut().expect("editor");
    editor.unfold_all();
    editor.set_content("fn first() {\n    needle();\n}");
    assert!(editor.fold_at(0));
    app.handle_input(&TerminalInput::Key(KeyEvent::new(
        KeyCode::End,
        Modifiers::ctrl(),
    )));
    assert_eq!(app.editor().expect("editor").cursor(), Position::new(2, 1));
    assert!(!app.editor().expect("editor").is_line_hidden(2));
}

#[test]
fn word_and_shift_motions_cross_fold_edges_without_opening_them() {
    for shift in [false, true] {
        let mut app = fixture();
        app.handle_input(&press(KeyCode::End));
        app.handle_input(&TerminalInput::Key(KeyEvent::new(
            KeyCode::Right,
            Modifiers {
                ctrl: true,
                shift,
                ..Modifiers::none()
            },
        )));
        let selection = app.editor().expect("editor").state().cursor.primary;
        assert_eq!(selection.head, Position::new(4, 0));
        if shift {
            assert_eq!(selection.anchor, Position::new(0, 12));
        }
        assert!(app.editor().expect("editor").is_line_hidden(1));
        app.handle_input(&TerminalInput::Key(KeyEvent::new(
            KeyCode::Left,
            Modifiers {
                ctrl: true,
                shift,
                ..Modifiers::none()
            },
        )));
        assert_eq!(app.editor().expect("editor").cursor(), Position::new(0, 12));
    }
}

#[test]
fn right_at_a_folded_eof_is_a_stable_boundary() {
    let mut app = fixture();
    let editor = app.workspace.active_editor_mut().expect("editor");
    editor.unfold_all();
    editor.set_content("fn first() {\n    needle();\n}");
    assert!(editor.fold_at(0));
    app.handle_input(&press(KeyCode::End));
    for _ in 0..3 {
        app.handle_input(&press(KeyCode::Right));
    }
    assert_eq!(app.editor().expect("editor").cursor(), Position::new(0, 12));
    assert!(app.editor().expect("editor").is_line_hidden(1));
}
