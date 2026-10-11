use super::*;
use crate::{
    cli::Options,
    clipboard::{Backend, ClipboardMode, Provider},
    file::test_support::TempDir,
};
use iridium_tui::input::TerminalInput;
use std::{
    cell::RefCell,
    fs,
    rc::Rc,
    thread,
    time::{Duration, Instant},
};
#[derive(Default)]
struct Board {
    text: String,
    fail: bool,
}
struct Stub(Rc<RefCell<Board>>);
impl Provider for Stub {
    fn read(&mut self) -> Result<String, String> {
        let board = self.0.borrow();
        if board.fail {
            Err("clipboard unavailable".into())
        } else {
            Ok(board.text.clone())
        }
    }
    fn write(&mut self, text: &str) -> Result<(), String> {
        let mut board = self.0.borrow_mut();
        if board.fail {
            return Err("clipboard unavailable".into());
        }
        board.text = text.into();
        Ok(())
    }
}
fn app(root: &std::path::Path) -> App {
    fs::write(root.join("source"), "cat").expect("source");
    let mut app = App::with_config(
        Options {
            path: Some(root.join("source")),
            ..Options::default()
        },
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("app");
    app.open_project_search("cat");
    app
}
fn key(app: &mut App, key: KeyCode, modifiers: Modifiers) {
    app.handle_input(&TerminalInput::Key(KeyEvent::new(key, modifiers)));
}
fn ctrl(app: &mut App, ch: char) {
    key(app, KeyCode::Char(ch), Modifiers::ctrl());
}
fn field(app: &App) -> &str {
    app.project_search
        .as_ref()
        .expect("panel")
        .clipboard_field()
        .expect("field")
        .text()
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
fn provider_failures_are_visible_and_exact_cut_paste_never_edit_document() {
    let dir = TempDir::new("project-field-clipboard");
    let mut app = app(dir.path());
    let board = Rc::new(RefCell::new(Board {
        text: "fresh".into(),
        fail: true,
    }));
    app.clipboard_backend = Backend::System(Box::new(Stub(board.clone())));
    app.clipboard = "stale".into();
    ctrl(&mut app, 'a');
    ctrl(&mut app, 'x');
    assert_eq!(field(&app), "cat");
    assert!(
        app.project_search
            .as_ref()
            .expect("panel")
            .feedback
            .contains("unavailable")
    );
    ctrl(&mut app, 'v');
    wait(&mut app);
    assert!(
        app.project_search
            .as_ref()
            .expect("panel")
            .clipboard_message
            .as_deref()
            .is_some_and(|message| message.contains("unavailable"))
    );
    assert_eq!(field(&app), "cat");
    board.borrow_mut().fail = false;
    ctrl(&mut app, 'x');
    assert_eq!(field(&app), "");
    assert_eq!(board.borrow().text, "cat");
    let exact = "猫\r\ne\u{301}\t";
    board.borrow_mut().text = exact.into();
    ctrl(&mut app, 'v');
    assert_eq!(field(&app), exact);
    ctrl(&mut app, 'a');
    ctrl(&mut app, 'c');
    assert_eq!(board.borrow().text, exact);
    assert_eq!(app.editor().expect("editor").content(), "cat");
}
#[test]
fn terminal_cut_waits_for_delivery_and_refuses_changed_focus_closed_panel_and_io_failure() {
    let dir = TempDir::new("project-field-delivery");
    let mut app = app(dir.path());
    app.configure_clipboard(ClipboardMode::Terminal, true);
    ctrl(&mut app, 'a');
    ctrl(&mut app, 'x');
    assert_eq!(field(&app), "cat");
    key(&mut app, KeyCode::Tab, Modifiers::none());
    app.flush_terminal_clipboard(|text| {
        assert_eq!(text, "cat");
        Ok(())
    })
    .expect("delivery");
    assert_eq!(field(&app), "");
    key(&mut app, KeyCode::Tab, Modifiers::none());
    assert_eq!(field(&app), "cat");
    ctrl(&mut app, 'x');
    assert!(
        app.flush_terminal_clipboard(|_| Err(std::io::Error::other("write refused")))
            .is_err()
    );
    assert_eq!(field(&app), "cat");
    ctrl(&mut app, 'x');
    app.project_search.as_mut().expect("panel").close();
    app.open_project_search("cat");
    app.flush_terminal_clipboard(|_| Ok(())).expect("delivery");
    assert_eq!(field(&app), "cat");
    assert!(
        app.project_search
            .as_ref()
            .expect("panel")
            .feedback
            .contains("cancelled")
    );
}
#[test]
fn rebound_clipboard_works_in_replacement_and_inspection_never_reads_it() {
    let dir = TempDir::new("project-review-clipboard");
    let mut app = app(dir.path());
    let stroke =
        |ch| StrokePattern::new(KeyCode::Char(ch), ModifierPattern::exact(Modifiers::ctrl()));
    app.user_keys
        .push(KeyBinding::new(stroke('j'), &[], CLIPBOARD_PASTE));
    app.open_project_search("cat");
    wait(&mut app);
    key(&mut app, KeyCode::F5, Modifiers::none());
    app.clipboard = "dog\n猫".into();
    ctrl(&mut app, 'j');
    wait(&mut app);
    assert_eq!(field(&app), "dog\n猫");
    ctrl(&mut app, 'a');
    ctrl(&mut app, 'c');
    assert_eq!(app.clipboard, "dog\n猫");
    key(&mut app, KeyCode::F4, Modifiers::none());
    let board = Rc::new(RefCell::new(Board {
        fail: true,
        ..Board::default()
    }));
    app.clipboard_backend = Backend::System(Box::new(Stub(board)));
    app.message = None;
    ctrl(&mut app, 'j');
    assert!(app.message.is_none());
    key(&mut app, KeyCode::Escape, Modifiers::none());
    assert_eq!(field(&app), "dog\n猫");
    assert_eq!(app.editor().expect("editor").content(), "cat");
}
#[test]
fn clipboard_bindings_support_sequences_suppression_and_refuse_partial_composition() {
    let stroke =
        |ch| StrokePattern::new(KeyCode::Char(ch), ModifierPattern::exact(Modifiers::ctrl()));
    let event = |ch| KeyEvent::new(KeyCode::Char(ch), Modifiers::ctrl());
    let mut user = Keymap::new("test");
    user.push(KeyBinding::new(stroke('k'), &[stroke('c')], CLIPBOARD_COPY));
    user.push(KeyBinding::unbound(stroke('v'), &[]));
    user.push(KeyBinding::new(stroke('x'), &[], CLIPBOARD_CUT).then_command(CLIPBOARD_PASTE));
    let mut keys = Keys::default();
    keys.set_user_keymap(&user);
    assert!(matches!(keys.resolve(&event('k')), Resolved::Handled));
    assert!(matches!(
        keys.resolve(&event('c')),
        Resolved::Command(id) if id == CLIPBOARD_COPY
    ));
    assert!(matches!(keys.resolve(&event('v')), Resolved::Handled));
    assert!(matches!(keys.resolve(&event('x')), Resolved::Invalid));
    assert!(matches!(keys.resolve(&event('k')), Resolved::Handled));
    assert!(matches!(
        keys.resolve(&KeyEvent::new(KeyCode::Escape, Modifiers::none())),
        Resolved::Handled
    ));
    assert!(keys.pending.is_empty());
}
