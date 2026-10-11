//! Mouse regression coverage for exact source selection and modal ownership.
use super::{App, Flow};
use crate::cli::Options;
use iridium_editor::{KeyCode, KeyEvent, Position};
use iridium_tui::frame::{Chrome, Frame};
use iridium_tui::input::{MouseButton, MouseEvent, MouseEventKind, MouseModifiers, TerminalInput};

pub(super) fn mouse(app: &mut App, kind: MouseEventKind, column: u16, row: u16) {
    assert_eq!(
        app.handle_input(&TerminalInput::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: MouseModifiers::empty()
        })),
        Flow::Running
    );
}
fn key(app: &mut App, code: KeyCode) {
    app.handle_input(&TerminalInput::Key(KeyEvent::simple(code)));
}

#[test]
fn drag_and_yank_exclude_gutter_and_padding_preserving_source_lines() {
    let mut app = App::with_config(
        Options::default(),
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("app");
    let text = format!(
        "{}use serde::{{Deserialize, Serialize}};\n\t漢e\u{301} text;",
        "\n".repeat(35)
    );
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_content(&text);
    app.resize(100, 15);
    let editor = app.workspace.active_editor_mut().expect("editor");
    let mut viewport = editor.state().viewport.clone();
    viewport.first_line = 35;
    editor.set_viewport(viewport);
    let layout = Frame::layout(
        app.editor().expect("editor"),
        100,
        15,
        Chrome {
            tab_bar: true,
            ..Chrome::default()
        },
    );
    let start = u16::try_from(layout.text.origin).expect("column");
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), start, 0);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 99, 1);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 99, 1);
    key(&mut app, KeyCode::Char('y'));
    assert_eq!(
        app.clipboard,
        "use serde::{Deserialize, Serialize};\n\t漢e\u{301} text;"
    );
    assert!(app.mouse_anchor.is_none());
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 99, 1);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 0, 0);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 0, 0);
    key(&mut app, KeyCode::Char('y'));
    assert_eq!(
        app.clipboard,
        "use serde::{Deserialize, Serialize};\n\t漢e\u{301} text;"
    );
}

#[test]
fn hit_testing_matches_tabs_wide_glyphs_combining_marks_and_horizontal_scroll() {
    let mut app = App::with_config(
        Options::default(),
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("app");
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_content("\t漢e\u{301}x");
    app.resize(30, 10);
    let editor = app.workspace.active_editor_mut().expect("editor");
    let mut viewport = editor.state().viewport.clone();
    viewport.scroll_offset_x = 4.0;
    editor.set_viewport(viewport);
    let layout = Frame::layout(
        editor,
        30,
        10,
        Chrome {
            tab_bar: true,
            ..Chrome::default()
        },
    );
    let start = u16::try_from(layout.text.origin).expect("column");
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        start + 1,
        0,
    );
    mouse(
        &mut app,
        MouseEventKind::Up(MouseButton::Left),
        start + 3,
        0,
    );
    let selection = app.editor().expect("editor").state().cursor.primary;
    assert_eq!(selection.start(), Position::new(0, 1));
    assert_eq!(selection.end(), Position::new(0, 4));
    key(&mut app, KeyCode::Char('y'));
    assert_eq!(app.clipboard, "漢e\u{301}");
}

#[test]
fn overlays_and_status_do_not_select_hidden_document_and_blur_cancels_drag() {
    let mut app = App::with_config(
        Options::default(),
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("app");
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_content("hello world");
    app.resize(40, 10);
    let original = app.editor().expect("editor").state().cursor.clone();
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 5, 9);
    assert_eq!(app.editor().expect("editor").state().cursor, original);
    key(&mut app, KeyCode::Char(':'));
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 5, 0);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 10, 0);
    assert_eq!(app.editor().expect("editor").state().cursor, original);
    key(&mut app, KeyCode::Escape);
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 5, 0);
    app.handle_input(&TerminalInput::FocusLost);
    let before = app.editor().expect("editor").state().cursor.clone();
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 10, 0);
    assert_eq!(app.editor().expect("editor").state().cursor, before);
}

#[test]
fn folded_rows_map_to_source_after_the_fold() {
    let mut app = App::with_config(
        Options::default(),
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("app");
    let editor = app.workspace.active_editor_mut().expect("editor");
    editor.set_content("[\n  [1,2],\n  [3,4]\n]\ntail");
    editor.set_language(iridium_editor::Language::Json);
    editor.fold_all();
    app.resize(40, 10);
    let layout = Frame::layout(
        app.editor().expect("editor"),
        40,
        10,
        Chrome {
            tab_bar: true,
            ..Chrome::default()
        },
    );
    let start = u16::try_from(layout.text.origin).expect("column");
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), start, 1);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 39, 1);
    key(&mut app, KeyCode::Char('y'));
    assert_eq!(app.clipboard, "tail");
}
