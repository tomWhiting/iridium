use super::{App, Flow};
use crate::cli::Options;
use crate::file::test_support::TempDir;
use iridium_editor::commands::structural_keymap::{STRUCTURAL, STRUCTURAL_INSERT};
use iridium_editor::{KeyCode, KeyEvent};
use iridium_tui::input::TerminalInput;

pub(super) fn legacy_options() -> Options {
    Options {
        non_modal: true,
        ..Options::default()
    }
}
fn key(app: &mut App, code: KeyCode) -> Flow {
    app.handle_input(&TerminalInput::Key(KeyEvent::simple(code)))
}
fn ex(app: &mut App, command: &str) -> Flow {
    assert_eq!(key(app, KeyCode::Char(':')), Flow::Running);
    assert!(app.is_palette_open());
    for character in command.chars() {
        key(app, KeyCode::Char(character));
    }
    key(app, KeyCode::Enter)
}

#[test]
fn modal_default_enters_typing_returns_with_escape_and_writes_via_colon() {
    let directory = TempDir::new("modal-default");
    let path = directory.path().join("data.json");
    std::fs::write(&path, "[11,22]").expect("fixture");
    let mut app = App::with_config(
        Options {
            path: Some(path.clone()),
            ..Options::default()
        },
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("app");
    assert_eq!(app.editor().expect("editor").mode(), Some(&STRUCTURAL));
    key(&mut app, KeyCode::Char(','));
    key(&mut app, KeyCode::Char('j'));
    key(&mut app, KeyCode::Char('i'));
    assert_eq!(
        app.editor().expect("editor").mode(),
        Some(&STRUCTURAL_INSERT)
    );
    key(&mut app, KeyCode::Char('9'));
    key(&mut app, KeyCode::Escape);
    assert_eq!(app.editor().expect("editor").mode(), Some(&STRUCTURAL));
    assert_eq!(
        app.handle_input(&TerminalInput::Key(KeyEvent::new(
            KeyCode::Char('s'),
            iridium_editor::Modifiers::ctrl()
        ))),
        Flow::Running
    );
    assert_eq!(
        app.handle_input(&TerminalInput::Key(KeyEvent::new(
            KeyCode::Char('q'),
            iridium_editor::Modifiers::ctrl()
        ))),
        Flow::Running
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("unchanged disk"),
        "[11,22]"
    );
    assert_eq!(ex(&mut app, "q"), Flow::Running);
    assert!(app.message().expect("refusal").is_error());
    assert_eq!(ex(&mut app, "w"), Flow::Running);
    assert_eq!(std::fs::read_to_string(&path).expect("saved"), "[11,9]");
    assert_eq!(ex(&mut app, "q"), Flow::Exit);
}

#[test]
fn write_quit_refuses_external_changes_without_quitting_or_overwriting() {
    let directory = TempDir::new("modal-write-quit");
    let path = directory.path().join("data.json");
    std::fs::write(&path, "[11]").expect("fixture");
    let mut app = App::with_config(
        Options {
            path: Some(path.clone()),
            ..Options::default()
        },
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("app");
    std::fs::write(&path, "[22]").expect("external change");
    assert_eq!(ex(&mut app, "wq"), Flow::Running);
    assert!(app.message().expect("refusal").is_error());
    assert_eq!(std::fs::read_to_string(path).expect("disk"), "[22]");
}

#[test]
fn write_quit_waits_for_a_filename_and_cancel_leaves_the_editor_open() {
    let directory = TempDir::new("modal-save-as");
    let path = directory.path().join("new.json");
    let mut app = App::with_config(
        Options::default(),
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("app");
    key(&mut app, KeyCode::Char('i'));
    app.handle_input(&TerminalInput::Paste("[1]".into()));
    key(&mut app, KeyCode::Escape);
    assert_eq!(ex(&mut app, "wq"), Flow::Running);
    assert!(app.prompt().is_some());
    assert_eq!(key(&mut app, KeyCode::Escape), Flow::Running);
    assert!(app.pending_write_quit.is_none());
    assert_eq!(ex(&mut app, "wq"), Flow::Running);
    app.handle_input(&TerminalInput::Paste(path.to_string_lossy().into_owned()));
    assert_eq!(key(&mut app, KeyCode::Enter), Flow::Exit);
    assert_eq!(std::fs::read_to_string(path).expect("saved"), "[1]");
}

#[test]
fn quit_bang_affects_one_tab_and_quit_all_checks_background_edits() {
    let mut app = App::with_config(
        Options::default(),
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("app");
    key(&mut app, KeyCode::Char('i'));
    key(&mut app, KeyCode::Char('x'));
    key(&mut app, KeyCode::Escape);
    app.new_scratch();
    assert_eq!(ex(&mut app, "qa"), Flow::Running);
    assert!(app.message().expect("unsaved background").is_error());
    assert_eq!(ex(&mut app, "q!"), Flow::Running);
    assert_eq!(app.workspace.tabs().len(), 1);
    assert_eq!(app.editor().expect("remaining").content(), "x");
    assert_eq!(ex(&mut app, "qa!"), Flow::Exit);
}

#[test]
fn insert_colon_is_literal_and_delete_yanks_source_for_paste() {
    let mut app = App::with_config(
        Options::default(),
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("app");
    key(&mut app, KeyCode::Char('i'));
    key(&mut app, KeyCode::Char(':'));
    assert!(!app.is_palette_open());
    assert_eq!(app.editor().expect("editor").content(), ":");
    key(&mut app, KeyCode::Escape);
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_selection(
            iridium_editor::Position::new(0, 0),
            iridium_editor::Position::new(0, 1),
        );
    key(&mut app, KeyCode::Char('d'));
    assert_eq!(app.clipboard, ":");
    assert_eq!(app.editor().expect("editor").content(), "");
    key(&mut app, KeyCode::Char('p'));
    assert_eq!(app.editor().expect("editor").content(), ":");
}

#[test]
fn insert_completion_uses_buffer_words_and_one_undo() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::with_config(Options::default(), iridium_config::UserConfig::default(), None)?;
    let editor = app.workspace.active_editor_mut().ok_or("no editor")?;
    editor.set_content("alphabet al");
    editor.set_cursor(iridium_editor::Position::new(0, 11));
    editor.set_mode(Some(STRUCTURAL_INSERT));
    app.handle_input(&TerminalInput::Key(KeyEvent {
        key: KeyCode::Char('n'),
        modifiers: iridium_editor::Modifiers::ctrl(),
        is_repeat: false,
    }));
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.editor().ok_or("no editor")?.content(), "alphabet alphabet");
    key(&mut app, KeyCode::Escape);
    key(&mut app, KeyCode::Char('u'));
    assert_eq!(app.editor().ok_or("no editor")?.content(), "alphabet al");
    Ok(())
}
