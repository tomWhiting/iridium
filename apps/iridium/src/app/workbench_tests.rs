//! Real input and rendered geometry for the terminal workspace controls.
use super::{App, workbench};
use crate::cli::Options;
use iridium_editor::{KeyCode, KeyEvent, Modifiers};
use iridium_file::test_support::TempDir;
use iridium_tui::cell::{ColorDepth, Surface};
use iridium_tui::input::{MouseButton, MouseEvent, MouseEventKind, MouseModifiers, TerminalInput};
use std::fs;

pub(super) fn app(dir: &TempDir) -> App {
    let path = dir.path().join("config.toml");
    fs::write(&path, "[editor]\nword_wrap = true\n[keys]\n").expect("config");
    let mut app = App::with_config(
        Options::default(),
        iridium_config::UserConfig::load(Some(&path)),
        Some(&path),
    )
    .expect("app");
    app.project = Some(dir.path().to_path_buf());
    app.resize(100, 24);
    app
}
pub(super) fn key(app: &mut App, key: KeyCode, modifiers: Modifiers) {
    app.handle_input(&TerminalInput::Key(KeyEvent::new(key, modifiers)));
}
fn render(app: &mut App) {
    let mut surface = Surface::new(100, 24, ColorDepth::TrueColor);
    app.render(&mut surface);
}
pub(super) fn settle(app: &mut App) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        render(app);
        if app.explorer.as_ref().is_none_or(|p| !p.is_waiting()) {
            break;
        }
        assert!(std::time::Instant::now() < end, "reader stalled");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
fn click(app: &mut App, column: u16, row: u16) {
    app.handle_input(&TerminalInput::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: MouseModifiers::empty(),
    }));
}
#[test]
fn builtin_sidebar_palette_command_opens_and_closes_the_sidebar() {
    use iridium_editor::commands::builtin::EXPLORER_TOGGLE_SIDEBAR;

    let dir = TempDir::new("sidebar-palette-command");
    let mut app = app(&dir);
    app.run_palette_command(&EXPLORER_TOGGLE_SIDEBAR);
    settle(&mut app);
    assert!(app.sidebar_columns() > 0);
    assert!(app.message.is_none());
    app.run_palette_command(&EXPLORER_TOGGLE_SIDEBAR);
    assert!(app.explorer.is_none());
    assert_eq!(app.sidebar_columns(), 0);
    assert!(app.message.is_none());
}

#[test]
fn sidebar_stays_visible_while_editing_and_opens_files_in_tabs() {
    let dir = TempDir::new("sidebar");
    fs::write(dir.path().join("hello.rs"), "fn hello() {}\n").expect("file");
    let mut app = app(&dir);
    key(
        &mut app,
        KeyCode::Char('b'),
        Modifiers {
            ctrl: true,
            ..Modifiers::none()
        },
    );
    settle(&mut app);
    assert!(app.sidebar_columns() > 0);
    app.handle_input(&TerminalInput::Paste("hello.rs".into()));
    settle(&mut app);
    key(&mut app, KeyCode::Enter, Modifiers::none());
    assert!(app.explorer.is_some());
    assert!(app.focus != crate::app::sidebar::Focus::Explorer);
    assert_eq!(app.workspace.document_count(), 2);
    key(&mut app, KeyCode::Char('i'), Modifiers::none());
    app.handle_input(&TerminalInput::Paste("// document\n".into()));
    assert!(
        app.editor()
            .expect("editor")
            .content()
            .contains("// document")
    );
    key(
        &mut app,
        KeyCode::Char('e'),
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
    );
    assert!(app.focus == crate::app::sidebar::Focus::Explorer);
    key(&mut app, KeyCode::Escape, Modifiers::none());
    assert!(app.focus != crate::app::sidebar::Focus::Explorer);
    assert!(app.sidebar_columns() > 0);
    key(
        &mut app,
        KeyCode::Char('b'),
        Modifiers {
            ctrl: true,
            ..Modifiers::none()
        },
    );
    assert!(app.explorer.is_none());
    assert_eq!(app.sidebar_columns(), 0);
}
#[test]
fn sidebar_mouse_opens_file_and_document_click_returns_focus() {
    let dir = TempDir::new("sidebar-mouse");
    fs::write(dir.path().join("hello.rs"), "fn hello() {}\n").expect("file");
    let mut app = app(&dir);
    app.run_host_command(&workbench::SIDEBAR);
    settle(&mut app);
    app.handle_input(&TerminalInput::Paste("hello.rs".into()));
    settle(&mut app);
    click(&mut app, 4, 1);
    assert!(app.editor().expect("editor").content().contains("fn hello"));
    assert!(app.focus != crate::app::sidebar::Focus::Explorer);
    click(&mut app, 1, 0);
    assert!(app.focus == crate::app::sidebar::Focus::Explorer);
    click(&mut app, 45, 0);
    assert!(app.focus != crate::app::sidebar::Focus::Explorer);
}
#[test]
fn tab_clicks_and_keyboard_switch_without_changing_documents() {
    let dir = TempDir::new("tab-clicks");
    let mut app = app(&dir);
    app.editor_mut_for_test("one");
    let first = app.workspace.active();
    app.new_scratch();
    app.editor_mut_for_test("two");
    let second = app.workspace.active();
    render(&mut app);
    let row =
        u16::try_from(app.mouse_layout().expect("layout").tab_row.expect("tabs")).expect("row");
    click(&mut app, 2, row);
    assert_eq!(app.workspace.active(), first);
    key(
        &mut app,
        KeyCode::Char('.'),
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
    );
    assert_eq!(app.workspace.active(), second);
    key(&mut app, KeyCode::PageUp, Modifiers::ctrl());
    assert_eq!(app.workspace.active(), first);
    assert_eq!(app.editor().expect("editor").content(), "one");
}
#[test]
fn generated_bindings_round_trip_modes_transitions_and_composed_commands() {
    let text = super::binding_reference::generate().expect("reference");
    let mut total = 0;
    for line in text.lines().filter_map(|line| line.strip_prefix("# \"")) {
        let user = iridium_config::UserConfig::parse(&format!("[keys]\n\"{line}\n"));
        assert!(user.problems.is_empty(), "{line}: {:?}", user.problems);
        assert_eq!(user.bindings.len(), 1, "{line}");
        total += 1;
    }
    assert!(total > 200);
    assert!(text.contains("terminal.reloadConfig"));
    assert!(text.contains("ast.navigateFiner"));
    assert!(text.contains("record_input = true"));
}
#[test]
fn reload_replaces_keys_on_all_tabs_and_rejects_bad_config_atomically() {
    let dir = TempDir::new("reload-config");
    let mut app = app(&dir);
    app.new_scratch();
    let path = app.configuration_path().expect("path");
    fs::write(
        &path,
        "[editor]\ntab_width = 7\n[keys]\nf9 = 'terminal.toggleSidebar'\n",
    )
    .expect("config");
    app.reload_configuration().expect("reload");
    for tab in app.workspace.tabs() {
        let id = app
            .workspace
            .node(tab)
            .expect("tab")
            .document()
            .expect("document");
        assert_eq!(
            app.workspace
                .editor(id)
                .expect("editor")
                .state()
                .config
                .tab_width,
            7
        );
    }
    key(&mut app, KeyCode::F9, Modifiers::none());
    assert!(app.explorer.is_some());
    fs::write(
        &path,
        "[editor]\ntab_width = 2\n[keys]\nf9 = 'no.such.command'\n",
    )
    .expect("config");
    assert!(app.reload_configuration().is_err());
    assert_eq!(app.editor().expect("editor").state().config.tab_width, 7);
    fs::write(&path, "[editor]\ntab_width = 3\n[keys]\n").expect("config");
    app.reload_configuration().expect("reload");
    app.close_explorer();
    key(&mut app, KeyCode::F9, Modifiers::none());
    assert!(app.explorer.is_none(), "removed binding still active");
}
#[test]
fn config_open_and_reference_do_not_overwrite_user_settings() {
    let dir = TempDir::new("config-reference");
    let mut app = app(&dir);
    let path = app.configuration_path().expect("path");
    let before = fs::read_to_string(&path).expect("read");
    app.open_configuration(false);
    assert_eq!(
        app.file().expect("file").path(),
        path.canonicalize().expect("canonical path")
    );
    app.open_configuration(true);
    assert!(app.editor().expect("editor").state().read_only);
    assert_eq!(fs::read_to_string(path).expect("read"), before);
    assert!(dir.path().join("default-keybindings.toml").exists());
}
impl App {
    fn editor_mut_for_test(&mut self, text: &str) {
        self.workspace
            .active_editor_mut()
            .expect("editor")
            .set_content(text);
    }
}

#[test]
fn sidebar_focus_changes_keep_unapplied_oil_edits_and_hide_refuses_them() {
    let dir = TempDir::new("sidebar-oil-focus");
    fs::write(dir.path().join("hello.rs"), "original").expect("file");
    let mut app = app(&dir);
    app.run_host_command(&workbench::SIDEBAR);
    settle(&mut app);
    app.handle_input(&TerminalInput::Paste("hello.rs".into()));
    settle(&mut app);
    key(&mut app, KeyCode::Tab, Modifiers::none());
    app.handle_input(&TerminalInput::Paste("_rename".into()));
    assert!(
        app.explorer
            .as_ref()
            .expect("browser")
            .has_unapplied_edits()
    );
    key(
        &mut app,
        KeyCode::Char('e'),
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
    );
    assert!(app.focus != crate::app::sidebar::Focus::Explorer);
    key(
        &mut app,
        KeyCode::Char('b'),
        Modifiers {
            ctrl: true,
            ..Modifiers::none()
        },
    );
    assert!(
        app.explorer
            .as_ref()
            .expect("browser")
            .has_unapplied_edits()
    );
    assert!(app.message().is_some_and(super::Message::is_error));
    key(
        &mut app,
        KeyCode::Char('e'),
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
    );
    assert!(app.focus == crate::app::sidebar::Focus::Explorer);
    assert_eq!(
        fs::read_to_string(dir.path().join("hello.rs")).expect("file"),
        "original"
    );
}
