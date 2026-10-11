//! Continuous granular navigation is exercised through the terminal's bindings.
use super::App;
use crate::cli::Options;
use iridium_editor::{KeyCode, KeyEvent, Position};
use iridium_tui::input::TerminalInput;

fn key(app: &mut App, code: KeyCode) {
    app.handle_input(&TerminalInput::Key(KeyEvent::simple(code)));
}
fn selected(app: &App) -> String {
    let editor = app.editor().expect("editor");
    editor
        .state()
        .document
        .slice(editor.state().cursor.primary.range())
}

#[test]
fn prose_descends_from_document_to_line_word_and_whole_graphemes() {
    let mut app = App::with_config(
        Options::default(),
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("app");
    let editor = app.workspace.active_editor_mut().expect("editor");
    editor.set_content("hello e\u{301}arth 👩‍💻!\nnext line");
    editor.set_selection(Position::new(0, 0), Position::new(1, 9));
    key(&mut app, KeyCode::Char(','));
    assert_eq!(selected(&app), "hello e\u{301}arth 👩‍💻!");
    key(&mut app, KeyCode::Char(','));
    assert_eq!(selected(&app), "hello");
    key(&mut app, KeyCode::Char('j'));
    assert_eq!(selected(&app), "e\u{301}arth");
    key(&mut app, KeyCode::Char(','));
    assert_eq!(selected(&app), "e\u{301}");
    key(&mut app, KeyCode::Char(','));
    assert_eq!(selected(&app), "a");
    key(&mut app, KeyCode::Char('k'));
    assert_eq!(selected(&app), "e\u{301}");
    key(&mut app, KeyCode::Char('.'));
    assert_eq!(selected(&app), "e\u{301}arth");
    key(&mut app, KeyCode::Char(','));
    assert_eq!(selected(&app), "e\u{301}");
    for _ in 0..6 {
        key(&mut app, KeyCode::Char(','));
    }
    assert_eq!(selected(&app), "👩‍💻");
}

#[test]
fn comment_leaf_descends_to_words_and_letters_then_traverses() {
    let mut app = App::with_config(
        Options::default(),
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("app");
    let editor = app.workspace.active_editor_mut().expect("editor");
    editor.set_content("// hello world\nfn main() {}\n");
    editor.set_language(iridium_editor::Language::Rust);
    editor.set_selection(Position::new(0, 0), Position::new(0, 14));
    key(&mut app, KeyCode::Char(','));
    assert_eq!(selected(&app), "hello");
    key(&mut app, KeyCode::Char('j'));
    assert_eq!(selected(&app), "world");
    key(&mut app, KeyCode::Char(','));
    assert_eq!(selected(&app), "w");
    key(&mut app, KeyCode::Char(','));
    assert_eq!(selected(&app), "o");
    key(&mut app, KeyCode::Char('.'));
    assert_eq!(selected(&app), "world");
    key(&mut app, KeyCode::Char('.'));
    assert_eq!(selected(&app), "// hello world");
}

#[test]
fn crossing_branches_keeps_leaf_detail_and_eof_does_not_wrap() {
    let mut app = App::with_config(
        Options::default(),
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("app");
    let editor = app.workspace.active_editor_mut().expect("editor");
    editor.set_content("[[11],[22,33]]");
    editor.set_language(iridium_editor::Language::Json);
    editor.set_selection(Position::new(0, 2), Position::new(0, 4));
    key(&mut app, KeyCode::Char('j'));
    assert_eq!(selected(&app), "22");
    key(&mut app, KeyCode::Char('k'));
    assert_eq!(selected(&app), "11");
    for _ in 0..30 {
        key(&mut app, KeyCode::Char(','));
    }
    assert_eq!(selected(&app), "]");
    let end = app.editor().expect("editor").state().cursor.clone();
    key(&mut app, KeyCode::Char(','));
    assert_eq!(app.editor().expect("editor").state().cursor, end);
}

#[test]
fn custom_axes_use_the_same_granular_commands_and_settings_are_loaded() {
    let config = iridium_config::UserConfig::parse(
        r#"[editor]
 tab_width = 8
 [keys]
 right = { command = "ast.navigateNext", mode = "structural", count = true }
 left = { command = "ast.navigatePrevious", mode = "structural", count = true }
 down = { command = "ast.navigateFiner", mode = "structural", count = true }
 up = { command = "ast.navigateCoarser", mode = "structural", count = true }
 "#,
    );
    assert!(config.problems.is_empty(), "{:?}", config.problems);
    let mut app = App::with_config(Options::default(), config, None).expect("app");
    let editor = app.workspace.active_editor_mut().expect("editor");
    assert_eq!(editor.state().config.tab_width, 8);
    editor.set_content("hello world");
    editor.set_selection(Position::new(0, 0), Position::new(0, 11));
    key(&mut app, KeyCode::Down);
    assert_eq!(selected(&app), "hello");
    key(&mut app, KeyCode::Right);
    assert_eq!(selected(&app), "world");
    key(&mut app, KeyCode::Down);
    assert_eq!(selected(&app), "w");
    key(&mut app, KeyCode::Right);
    assert_eq!(selected(&app), "o");
}

#[test]
fn selecting_then_changing_words_is_one_undo_and_repeats_after_navigation() {
    let mut app = App::with_config(
        Options::default(),
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("app");
    let editor = app.workspace.active_editor_mut().expect("editor");
    editor.set_content("hello world");
    editor.set_selection(Position::new(0, 0), Position::new(0, 11));
    key(&mut app, KeyCode::Char(','));
    key(&mut app, KeyCode::Char('c'));
    for character in "bye".chars() {
        key(&mut app, KeyCode::Char(character));
    }
    key(&mut app, KeyCode::Escape);
    assert_eq!(app.editor().expect("editor").content(), "bye world");
    key(&mut app, KeyCode::Char('u'));
    assert_eq!(app.editor().expect("editor").content(), "hello world");
    app.handle_input(&TerminalInput::Key(KeyEvent::new(
        KeyCode::Char('.'),
        iridium_editor::Modifiers::ctrl(),
    )));
    assert_eq!(app.editor().expect("editor").content(), "bye world");
}
