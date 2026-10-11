//! User-visible wrapping, movement and selection contracts.
use super::{App, Flow};
use crate::cli::Options;
use iridium_editor::commands::structural_keymap::{STRUCTURAL, STRUCTURAL_INSERT};
use iridium_editor::{CursorState, KeyCode, KeyEvent, Modifiers, Position, Selection};
use iridium_tui::frame::{Chrome, Frame};
use iridium_tui::input::{MouseButton, MouseEventKind, TerminalInput};

fn app(text: &str, width: usize) -> App {
    let config = iridium_config::UserConfig::parse(
        "[editor]\nword_wrap = true\nshow_line_numbers = false\n",
    );
    assert!(config.problems.is_empty());
    let mut app = App::with_config(Options::default(), config, None).expect("app");
    let editor = app.workspace.active_editor_mut().expect("editor");
    editor.set_content(text);
    editor.set_selection(Position::new(0, 0), Position::new(0, 0));
    app.resize(width, 8);
    app
}
fn key(app: &mut App, code: KeyCode) {
    app.handle_input(&TerminalInput::Key(KeyEvent::simple(code)));
}
fn modified(app: &mut App, code: KeyCode, modifiers: Modifiers) {
    app.handle_input(&TerminalInput::Key(KeyEvent::new(code, modifiers)));
}
fn cursor(app: &App) -> Position {
    app.editor().expect("editor").cursor()
}
fn ex(app: &mut App, text: &str) {
    key(app, KeyCode::Char(':'));
    assert!(app.is_palette_open());
    for c in text.chars() {
        key(app, KeyCode::Char(c));
    }
    key(app, KeyCode::Enter);
    assert!(
        !app.message().is_some_and(super::Message::is_error),
        "{:?}",
        app.message()
    );
}
#[test]
fn arrows_move_screen_rows_and_comma_period_change_granularity() {
    let mut app = app("abcdefghijklmnopqr", 8);
    key(&mut app, KeyCode::Right);
    assert_eq!(cursor(&app), Position::new(0, 1));
    key(&mut app, KeyCode::Down);
    assert_eq!(cursor(&app), Position::new(0, 9));
    key(&mut app, KeyCode::Down);
    assert_eq!(cursor(&app), Position::new(0, 17));
    key(&mut app, KeyCode::Up);
    assert_eq!(cursor(&app), Position::new(0, 9));
    key(&mut app, KeyCode::Left);
    assert_eq!(cursor(&app), Position::new(0, 8));
    key(&mut app, KeyCode::Char('.'));
    assert!(
        !app.editor()
            .expect("editor")
            .state()
            .cursor
            .primary
            .is_collapsed()
    );
    key(&mut app, KeyCode::Char(','));
    assert_eq!(
        app.editor().expect("editor").content(),
        "abcdefghijklmnopqr"
    );
}
#[test]
fn wrap_toggle_preserves_source_selection_and_horizontally_reveals_caret() {
    let mut app = app("abcdefghijklmnopqr", 8);
    for _ in 0..14 {
        key(&mut app, KeyCode::Right);
    }
    let before = app.editor().expect("editor").state().cursor.clone();
    ex(&mut app, "nowrap");
    let editor = app.editor().expect("editor");
    assert!(!editor.state().config.word_wrap);
    assert!(editor.state().viewport.scroll_offset_x > 0.0);
    assert_eq!(editor.state().cursor, before);
    ex(&mut app, "wrap");
    let editor = app.editor().expect("editor");
    assert!(editor.state().config.word_wrap);
    assert_eq!(editor.state().cursor, before);
    assert_eq!(editor.content(), "abcdefghijklmnopqr");
    assert_eq!(
        editor.state().viewport.scroll_offset_x.to_bits(),
        0.0_f32.to_bits()
    );
}
#[test]
fn mouse_drag_on_continuation_enters_select_mode_and_yanks_only_source() {
    let mut app = app("hello wonderful world", 8);
    key(&mut app, KeyCode::Char('i'));
    assert_eq!(
        app.editor().expect("editor").mode(),
        Some(&STRUCTURAL_INSERT)
    );
    super::mouse_tests::mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 0, 0);
    assert_eq!(
        app.editor().expect("editor").mode(),
        Some(&STRUCTURAL_INSERT)
    );
    super::mouse_tests::mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 3, 1);
    super::mouse_tests::mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 3, 1);
    assert_eq!(app.editor().expect("editor").mode(), Some(&STRUCTURAL));
    key(&mut app, KeyCode::Char('y'));
    assert_eq!(app.clipboard, "hello won");
}
#[test]
fn shift_arrows_enter_select_mode_without_snapping() {
    let mut app = app("abcdefghijklmnopqr", 8);
    key(&mut app, KeyCode::Char('i'));
    modified(&mut app, KeyCode::Down, Modifiers::shift());
    let editor = app.editor().expect("editor");
    assert_eq!(editor.mode(), Some(&STRUCTURAL));
    assert_eq!(
        editor.state().cursor.primary,
        Selection::new(Position::new(0, 0), Position::new(0, 8))
    );
}
#[test]
fn wrapped_moves_preserve_multicursor_sticky_columns_and_graphemes() {
    let mut app = app("abcdefghijk\na\nabcdefghijk", 8);
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_selections(CursorState::from_selections(
            Selection::collapsed(Position::new(0, 6)),
            vec![Selection::collapsed(Position::new(2, 6))],
        ));
    key(&mut app, KeyCode::Down);
    assert_eq!(cursor(&app), Position::new(0, 11));
    key(&mut app, KeyCode::Up);
    assert_eq!(cursor(&app), Position::new(0, 6));
    let mut unicode = self::app("漢e\u{301}👩‍💻abc", 4);
    key(&mut unicode, KeyCode::Right);
    key(&mut unicode, KeyCode::Right);
    assert_eq!(cursor(&unicode), Position::new(0, 3));
    let layout = Frame::layout(
        unicode.editor().expect("editor"),
        4,
        8,
        Chrome {
            tab_bar: true,
            ..Chrome::default()
        },
    );
    assert_eq!(layout.primary_caret.expect("caret").row, 1);
    key(&mut unicode, KeyCode::Down);
    assert_eq!(cursor(&unicode), Position::new(0, 8));
}
#[test]
fn a_resize_reflows_without_changing_content_or_selection() {
    let mut app = app("abcdefghijklmnopqr", 8);
    key(&mut app, KeyCode::Down);
    let before = app.editor().expect("editor").state().cursor.clone();
    app.resize(5, 8);
    assert_eq!(app.editor().expect("editor").state().cursor, before);
    key(&mut app, KeyCode::Down);
    assert_eq!(cursor(&app), Position::new(0, 13));
}
#[test]
fn wrapped_movement_inside_change_remains_one_undo_unit() {
    let mut app = app("abcdefghijklmnopqr", 8);
    modified(&mut app, KeyCode::Right, Modifiers::shift());
    key(&mut app, KeyCode::Char('c'));
    key(&mut app, KeyCode::Char('X'));
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Char('Y'));
    key(&mut app, KeyCode::Escape);
    assert_eq!(
        app.editor().expect("editor").content(),
        "XbcdefghiYjklmnopqr"
    );
    key(&mut app, KeyCode::Char('u'));
    assert_eq!(
        app.editor().expect("editor").content(),
        "abcdefghijklmnopqr"
    );
    assert_eq!(app.handle_input(&TerminalInput::FocusLost), Flow::Running);
}
