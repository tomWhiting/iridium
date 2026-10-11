//! User journeys across independent documents, driven through actual host keys.
use super::{App, Flow};
use crate::app::modal_tests::legacy_options;
use crate::cli::Options;
use crate::file::test_support::TempDir;
use iridium_editor::{KeyCode, KeyEvent, Modifiers, Position};
use iridium_tui::cell::{CellContent, ColorDepth, Surface};
use iridium_tui::input::TerminalInput;
use std::fs;
use std::path::Path;

fn app(path: &Path) -> App {
    App::with_config(
        Options {
            path: Some(path.to_owned()),
            ..legacy_options()
        },
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("session")
}
fn key(app: &mut App, character: char, modifiers: Modifiers) -> Flow {
    app.handle_input(&TerminalInput::Key(KeyEvent {
        key: KeyCode::Char(character),
        modifiers,
        is_repeat: false,
    }))
}
fn paste(app: &mut App, text: &str) {
    assert_eq!(
        app.handle_input(&TerminalInput::Paste(text.to_owned())),
        Flow::Running
    );
}
fn write(dir: &TempDir, name: &str, text: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, text).expect("fixture");
    path
}
fn contents(app: &App) -> String {
    app.editor().expect("active").content()
}
fn row(surface: &Surface, index: usize) -> String {
    let mut text = String::new();
    for cell in surface.back().row(index).expect("row") {
        if let CellContent::Grapheme(value) = cell.content() {
            value.push_to(&mut text);
        }
    }
    text
}

#[test]
fn three_files_keep_selections_history_viewport_and_independent_saves() {
    let dir = TempDir::new("terminal-tabs-three");
    let a = write(&dir, "a.rs", &"let a = 1;\n".repeat(100));
    let b = write(&dir, "b.rs", "let b = 2;\n");
    let c = write(&dir, "c.rs", "let c = 3;\n");
    let mut app = app(&a);
    app.resize(80, 12);
    app.goto(70);
    paste(&mut app, "first ");
    let state = app.editor().expect("active").state();
    let cursor = state.cursor.clone();
    let viewport = state.viewport.clone();
    let history = state.history.current_node_id();
    app.open_file(&b);
    paste(&mut app, "second ");
    app.open_file(&c);
    assert_eq!(app.workspace.document_count(), 3);
    key(
        &mut app,
        ',',
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
    );
    assert!(contents(&app).starts_with("second "));
    key(
        &mut app,
        ',',
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
    );
    let state = app.editor().expect("active").state();
    assert_eq!(state.cursor, cursor);
    assert_eq!(state.viewport.first_line, viewport.first_line);
    assert_eq!(state.history.current_node_id(), history);
    key(&mut app, 'z', Modifiers::ctrl());
    assert!(!app.is_dirty());
    key(
        &mut app,
        '.',
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
    );
    assert!(contents(&app).starts_with("second "));
    key(&mut app, 's', Modifiers::ctrl());
    assert_eq!(
        fs::read_to_string(&b).expect("saved"),
        "second let b = 2;\n"
    );
    assert!(!app.is_dirty());
    assert_eq!(
        fs::read_to_string(&a).expect("unchanged"),
        "let a = 1;\n".repeat(100)
    );
}

#[test]
fn quit_checks_dirty_background_tabs_and_cancel_keeps_them() {
    let dir = TempDir::new("terminal-tabs-quit");
    let a = write(&dir, "a", "one");
    let b = write(&dir, "b", "two");
    let mut app = app(&a);
    paste(&mut app, "edit ");
    app.open_file(&b);
    assert!(!app.is_dirty());
    assert_eq!(key(&mut app, 'q', Modifiers::ctrl()), Flow::Running);
    assert!(app.prompt().is_some());
    key(&mut app, 'n', Modifiers::none());
    assert_eq!(app.workspace.document_count(), 2);
    key(
        &mut app,
        ',',
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
    );
    assert_eq!(contents(&app), "edit one");
}

#[test]
fn close_confirms_one_document_and_last_close_leaves_usable_scratch() {
    let dir = TempDir::new("terminal-tabs-close");
    let a = write(&dir, "a", "one");
    let mut app = app(&a);
    paste(&mut app, "edit ");
    key(&mut app, 'w', Modifiers::ctrl());
    assert!(app.prompt().is_some());
    key(&mut app, 'n', Modifiers::none());
    assert_eq!(contents(&app), "edit one");
    key(&mut app, 'w', Modifiers::ctrl());
    key(&mut app, 'y', Modifiers::none());
    assert_eq!(app.workspace.document_count(), 1);
    assert!(app.file().is_none());
    paste(&mut app, "new");
    assert_eq!(contents(&app), "new");
    assert_eq!(fs::read_to_string(a).expect("disk"), "one");
}

#[test]
fn duplicate_paths_and_symlinks_activate_existing_unsaved_document() {
    let dir = TempDir::new("terminal-tabs-alias");
    let a = write(&dir, "a.rs", "one");
    let mut app = app(&a);
    paste(&mut app, "edit ");
    app.open_file(&dir.path().join(".").join("a.rs"));
    assert_eq!(app.workspace.document_count(), 1);
    assert_eq!(contents(&app), "edit one");
    #[cfg(unix)]
    {
        let alias = dir.path().join("alias.rs");
        std::os::unix::fs::symlink(&a, &alias).expect("symlink");
        app.open_file(&alias);
        assert_eq!(app.workspace.document_count(), 1);
        assert_eq!(contents(&app), "edit one");
    }
}

#[test]
fn failed_open_preserves_the_active_tab_and_reports_the_error() {
    let dir = TempDir::new("terminal-tabs-error");
    let a = write(&dir, "a", "one");
    let mut app = app(&a);
    paste(&mut app, "edit ");
    let invalid = dir.path().join("binary");
    fs::write(&invalid, [0xff]).expect("fixture");
    let active = app.workspace.active();
    app.open_file(&invalid);
    assert_eq!(app.workspace.active(), active);
    assert_eq!(contents(&app), "edit one");
    assert!(app.message().expect("error").is_error());
}

#[test]
fn save_conflicts_and_bom_crlf_remain_per_document() {
    let dir = TempDir::new("terminal-tabs-save");
    let a = write(&dir, "a", "\u{feff}one\r\n");
    let b = write(&dir, "b", "two\n");
    let mut app = app(&a);
    paste(&mut app, "A");
    app.open_file(&b);
    paste(&mut app, "B");
    fs::write(&b, "external\n").expect("external edit");
    key(&mut app, 's', Modifiers::ctrl());
    assert!(app.message().expect("refused").is_error());
    assert!(app.is_dirty());
    key(
        &mut app,
        ',',
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
    );
    key(&mut app, 's', Modifiers::ctrl());
    assert_eq!(fs::read(&a).expect("saved"), "\u{feff}Aone\r\n".as_bytes());
    assert_eq!(fs::read_to_string(b).expect("external"), "external\n");
}

#[test]
fn save_as_cannot_claim_another_open_tab_even_when_its_file_is_deleted() {
    let dir = TempDir::new("terminal-tabs-save-as");
    let a = write(&dir, "a", "one");
    let mut app = app(&a);
    key(&mut app, 'n', Modifiers::ctrl());
    paste(&mut app, "scratch");
    fs::remove_file(&a).expect("external deletion");
    app.write_as(&a);
    assert!(app.message().expect("refusal").is_error());
    assert!(app.file().is_none());
    assert!(!a.exists());
    assert_eq!(contents(&app), "scratch");
}

#[test]
fn read_only_and_user_keys_apply_to_new_tabs() {
    let dir = TempDir::new("terminal-tabs-config");
    let a = write(&dir, "a", "one");
    let b = write(&dir, "b", "two");
    let mut user = iridium_config::UserConfig::default();
    user.bindings.push(
        iridium_editor::KeyBinding::parse(
            "f9",
            iridium_editor::commands::builtin::WORKSPACE_PREVIOUS_TAB,
        )
        .expect("binding"),
    );
    let mut app = App::with_config(
        Options {
            path: Some(a),
            read_only: true,
            ..legacy_options()
        },
        user,
        None,
    )
    .expect("session");
    app.open_file(&b);
    paste(&mut app, "no");
    assert_eq!(contents(&app), "two");
    app.handle_input(&TerminalInput::Key(KeyEvent {
        key: KeyCode::F9,
        modifiers: Modifiers::none(),
        is_repeat: false,
    }));
    assert_eq!(contents(&app), "one");
    key(&mut app, 'n', Modifiers::ctrl());
    paste(&mut app, "no");
    assert_eq!(contents(&app), "");
}

#[test]
fn tab_strip_marks_dirty_and_keeps_active_name_visible_after_resize() {
    let dir = TempDir::new("terminal-tabs-paint");
    let a = write(&dir, "first", "one");
    let b = write(&dir, "漢字", "two");
    let mut app = app(&a);
    app.open_file(&b);
    paste(&mut app, "edit ");
    let mut surface = Surface::new(60, 8, ColorDepth::TrueColor);
    app.render(&mut surface);
    let tabs = row(&surface, 6);
    assert!(tabs.contains("1:first"), "{tabs}");
    assert!(tabs.contains("2:*漢字"), "{tabs}");
    let mut narrow = Surface::new(12, 8, ColorDepth::TrueColor);
    app.render(&mut narrow);
    assert!(row(&narrow, 6).contains("2:*漢字"));
    assert_eq!(
        app.editor().expect("active").state().viewport.visible_lines,
        6
    );
    assert_eq!(app.editor().expect("active").cursor(), Position::new(0, 5));
}

mod state;

#[test]
fn option_shortcuts_open_panels_without_changing_unsaved_document() {
    let dir = TempDir::new("terminal-option-panels");
    let first = write(&dir, "first.rs", "one");
    let second = write(&dir, "second.rs", "two");
    let mut app = app(&first);
    paste(&mut app, "changed");
    let saved = contents(&app);
    app.open_file(&second);
    key(
        &mut app,
        'k',
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
    );
    assert!(app.is_palette_open());
    let mut surface = Surface::new(80, 10, ColorDepth::TrueColor);
    app.render(&mut surface);
    let tabs = row(&surface, 8);
    assert!(
        tabs.contains("first.rs") && tabs.contains("second.rs"),
        "{tabs}"
    );
    app.handle_input(&TerminalInput::Key(KeyEvent {
        key: KeyCode::Escape,
        modifiers: Modifiers::none(),
        is_repeat: false,
    }));
    key(
        &mut app,
        'o',
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
    );
    assert!(app.is_explorer_open());
    app.close_explorer();
    app.open_file(&first);
    assert_eq!(contents(&app), saved);
    assert_eq!(app.workspace.tabs().len(), 2);
}
