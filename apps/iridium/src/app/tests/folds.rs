//! Folded code occupies one visible row for vertical navigation.
use super::*;
use iridium_editor::Position;

fn folded() -> App {
    let mut app = unnamed();
    let editor = app.workspace.active_editor_mut().expect("editor");
    editor.set_language(Language::Rust);
    editor.set_content(
        "fn first() {\n    let x = 1;\n    let y = 2;\n}\nlet tail = 3;\nlet end = 4;",
    );
    assert!(editor.fold_at(0));
    editor.set_cursor(Position::new(0, 5));
    app.resize(60, 6);
    app
}

#[test]
fn down_and_up_cross_folded_bodies_and_keep_the_column() {
    let mut app = folded();
    app.handle_input(&press(KeyCode::Down));
    assert_eq!(app.editor().expect("editor").cursor(), Position::new(4, 5));
    app.handle_input(&press(KeyCode::Up));
    assert_eq!(app.editor().expect("editor").cursor(), Position::new(0, 5));
}

#[test]
fn shift_down_keeps_the_anchor_while_skipping_hidden_rows() {
    let mut app = folded();
    app.handle_input(&TerminalInput::Key(KeyEvent::new(
        KeyCode::Down,
        Modifiers {
            shift: true,
            ..Modifiers::none()
        },
    )));
    let selection = app.editor().expect("editor").state().cursor.primary;
    assert_eq!(selection.anchor, Position::new(0, 5));
    assert_eq!(selection.head, Position::new(4, 5));
}

#[test]
fn page_down_counts_visible_rows() {
    let mut app = folded();
    app.resize(60, 4);
    app.handle_input(&press(KeyCode::PageDown));
    assert_eq!(app.editor().expect("editor").cursor(), Position::new(5, 5));
}

#[test]
fn adding_below_skips_the_folded_body() {
    let mut app = folded();
    app.handle_input(&TerminalInput::Key(KeyEvent::new(
        KeyCode::Down,
        Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::none()
        },
    )));
    let heads: Vec<_> = app
        .editor()
        .expect("editor")
        .state()
        .cursor
        .all_selections()
        .map(|selection| selection.head)
        .collect();
    assert_eq!(heads, [Position::new(4, 5), Position::new(0, 5)]);
}

#[test]
fn named_counted_motions_use_the_same_visible_lines_as_keys() {
    let mut app = folded();
    let editor = app.workspace.active_editor_mut().expect("editor");
    editor
        .run_command(
            "cursor.lineDown",
            iridium_editor::CommandArgs::with_count(2),
        )
        .expect("command");
    assert_eq!(editor.cursor(), Position::new(5, 5));
    editor
        .run_command(
            "cursor.lineUp",
            iridium_editor::CommandArgs::with_count(u32::MAX),
        )
        .expect("command");
    assert_eq!(editor.cursor(), Position::new(0, 5));
}

#[test]
fn unfolding_changes_the_next_dispatch_without_losing_the_column() {
    let mut app = folded();
    app.handle_input(&press(KeyCode::Down));
    app.handle_input(&press(KeyCode::Up));
    app.handle_input(&alt('u'));
    app.handle_input(&press(KeyCode::Down));
    assert_eq!(app.editor().expect("editor").cursor(), Position::new(1, 5));
}

#[test]
fn fold_all_lifts_the_caret_to_the_outer_visible_header() {
    let mut app = unnamed();
    let editor = app.workspace.active_editor_mut().expect("editor");
    editor.set_language(Language::Rust);
    editor.set_content("fn first() {\n    if true {\n        go();\n    }\n}\nlet tail = 3;");
    editor.set_cursor(Position::new(2, 8));
    app.resize(60, 6);
    app.handle_input(&ctrl_alt('f'));
    assert_eq!(app.editor().expect("editor").cursor(), Position::new(0, 0));
    app.handle_input(&press(KeyCode::Down));
    assert_eq!(app.editor().expect("editor").cursor(), Position::new(5, 0));
}

#[test]
fn down_at_a_fold_hiding_eof_stays_on_the_visible_header() {
    let mut app = unnamed();
    let editor = app.workspace.active_editor_mut().expect("editor");
    editor.set_language(Language::Rust);
    editor.set_content("fn first() {\n    go();\n}");
    assert!(editor.fold_at(0));
    app.resize(60, 6);
    app.handle_input(&press(KeyCode::Down));
    assert_eq!(app.editor().expect("editor").cursor(), Position::new(0, 12));
}

#[test]
fn every_cursor_moves_across_visible_rows_independently() {
    let mut app = folded();
    app.handle_input(&TerminalInput::Key(KeyEvent::new(
        KeyCode::Down,
        Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::none()
        },
    )));
    app.handle_input(&press(KeyCode::Down));
    let heads: Vec<_> = app
        .editor()
        .expect("editor")
        .state()
        .cursor
        .all_selections()
        .map(|selection| selection.head)
        .collect();
    assert_eq!(heads, [Position::new(5, 5), Position::new(4, 5)]);
}
