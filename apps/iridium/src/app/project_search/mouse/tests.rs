use crate::{app::App, cli::Options, file::test_support::TempDir};
use iridium_editor::{KeyCode, KeyEvent, Modifiers};
use iridium_tui::input::{
    MouseButton, MouseEvent, MouseEventKind, MouseModifiers, ScrollDirection, TerminalInput,
};
use std::{
    fs, thread,
    time::{Duration, Instant},
};
fn app(root: &std::path::Path) -> App {
    fs::write(root.join("source"), "cat cat cat").expect("source");
    let mut app = App::with_config(
        Options {
            path: Some(root.join("source")),
            ..Options::default()
        },
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("app");
    app.resize(80, 12);
    app
}
fn mouse(app: &mut App, kind: MouseEventKind, column: u16, row: u16) {
    app.handle_input(&TerminalInput::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: MouseModifiers::empty(),
    }));
}
fn key(app: &mut App, code: KeyCode) {
    app.handle_input(&TerminalInput::Key(KeyEvent::simple(code)));
}
fn copy(app: &mut App) {
    app.handle_input(&TerminalInput::Key(KeyEvent::new(
        KeyCode::Char('c'),
        Modifiers::ctrl(),
    )));
}
fn wait(app: &mut App) {
    let start = Instant::now();
    while app.project_search_waiting() {
        app.poll_project_search();
        assert!(start.elapsed() < Duration::from_secs(10));
        thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn project_field_drag_excludes_labels_and_other_rows_and_mouse_focus_accepts_paste() {
    let dir = TempDir::new("project-mouse-fields");
    let mut app = app(dir.path());
    let text = "猫e\u{301}\t\r\nEND";
    app.open_project_search(text);
    key(&mut app, KeyCode::Home);
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 6, 1);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 15, 0);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 15, 8);
    copy(&mut app);
    assert_eq!(app.clipboard, text);
    assert_eq!(app.editor().expect("editor").content(), "cat cat cat");
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 7, 2);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 7, 2);
    app.handle_input(&TerminalInput::Paste("*.rs".into()));
    assert_eq!(
        app.project_search.as_ref().expect("panel").query.globs,
        ["*.rs"]
    );
}
#[test]
fn result_click_and_wheel_choose_matches_without_editing_or_opening_them() {
    let dir = TempDir::new("project-mouse-results");
    let mut app = app(dir.path());
    app.open_project_search("cat");
    wait(&mut app);
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 20, 4);
    assert_eq!(app.project_search.as_ref().expect("panel").selected, 1);
    mouse(
        &mut app,
        MouseEventKind::Scroll(ScrollDirection::Down),
        20,
        4,
    );
    assert_eq!(app.project_search.as_ref().expect("panel").selected, 2);
    assert!(app.project_search_open());
    assert_eq!(app.editor().expect("editor").content(), "cat cat cat");
    key(&mut app, KeyCode::Enter);
    assert!(!app.project_search_open());
    assert_eq!(
        app.editor()
            .expect("editor")
            .state()
            .cursor
            .primary
            .start()
            .column,
        8
    );
}
#[test]
fn review_checkbox_and_replacement_drag_share_keyboard_state() {
    let dir = TempDir::new("project-mouse-review");
    let mut app = app(dir.path());
    app.open_project_search("cat");
    wait(&mut app);
    key(&mut app, KeyCode::F5);
    app.handle_input(&TerminalInput::Paste("犬\nEND".into()));
    wait(&mut app);
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 3, 2);
    assert!(
        !app.project_search
            .as_ref()
            .expect("panel")
            .review
            .as_ref()
            .expect("review")
            .included[0]
    );
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 9, 1);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 15, 1);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 15, 1);
    copy(&mut app);
    assert_eq!(app.clipboard, "犬\nEND");
    key(&mut app, KeyCode::F4);
    mouse(
        &mut app,
        MouseEventKind::Scroll(ScrollDirection::Down),
        20,
        4,
    );
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 3, 2);
    assert_eq!(app.editor().expect("editor").content(), "cat cat cat");
    key(&mut app, KeyCode::Escape);
    key(&mut app, KeyCode::F9);
    wait(&mut app);
    assert_eq!(
        fs::read_to_string(dir.path().join("source")).expect("disk"),
        "cat 犬\nEND 犬\nEND"
    );
}
