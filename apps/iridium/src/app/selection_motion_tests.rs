//! Vertical motion retains the selected object, rather than its exclusive end.
use super::App;
use crate::cli::Options;
use iridium_editor::{CursorState, KeyCode, KeyEvent, Language, Modifiers, Position, Selection};
use iridium_tui::input::TerminalInput;
fn app(text: &str, language: Language, wrap: bool) -> App {
    let config = iridium_config::UserConfig::parse(&format!(
        "[editor]\nword_wrap = {wrap}\nshow_line_numbers = false\n"
    ));
    assert!(config.problems.is_empty());
    let mut app = App::with_config(Options::default(), config, None).expect("app");
    let editor = app.workspace.active_editor_mut().expect("editor");
    editor.set_content(text);
    editor.set_language(language);
    app.resize(40, 12);
    app
}
fn select(app: &mut App, line: usize, start: usize, end: usize) {
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_selection(Position::new(line, start), Position::new(line, end));
}
fn key(app: &mut App, key: KeyCode) {
    app.handle_input(&TerminalInput::Key(KeyEvent::simple(key)));
}
fn selection(app: &App) -> Selection {
    app.editor().expect("editor").state().cursor.primary
}
fn text(app: &App) -> String {
    let editor = app.editor().expect("editor");
    editor.state().document.slice(selection(app).range())
}
#[test]
fn number_selection_moves_to_numbers_on_next_rows_with_both_wrap_settings() {
    for wrap in [false, true] {
        let mut app = app("[\n  11,\n  222,\n  3\n]", Language::Json, wrap);
        select(&mut app, 1, 2, 4);
        key(&mut app, KeyCode::Down);
        assert_eq!(text(&app), "222");
        key(&mut app, KeyCode::Down);
        assert_eq!(text(&app), "3");
        key(&mut app, KeyCode::Up);
        assert_eq!(text(&app), "222");
        key(&mut app, KeyCode::Up);
        assert_eq!(text(&app), "11");
        key(&mut app, KeyCode::Up);
        assert_eq!(text(&app), "11");
    }
}
#[test]
fn comma_descends_and_period_ascends_then_vertical_preserves_the_parent_kind() {
    let mut app = app("[\n  [11,22],\n  [33,44]\n]", Language::Json, true);
    select(&mut app, 1, 3, 5);
    key(&mut app, KeyCode::Char('.'));
    assert_eq!(text(&app), "[11,22]");
    key(&mut app, KeyCode::Down);
    assert_eq!(text(&app), "[33,44]");
    key(&mut app, KeyCode::Char(','));
    assert_eq!(text(&app), "33");
    key(&mut app, KeyCode::Up);
    assert_eq!(text(&app), "11");
}
#[test]
fn prose_word_selection_keeps_its_start_column_and_skips_empty_lines() {
    let mut app = app(
        "// hello world\n\n// cat thing\n// longer word",
        Language::Rust,
        true,
    );
    select(&mut app, 0, 0, 14);
    key(&mut app, KeyCode::Char(','));
    assert_eq!(text(&app), "hello");
    key(&mut app, KeyCode::Down);
    assert_eq!(text(&app), "cat");
    key(&mut app, KeyCode::Down);
    assert_eq!(text(&app), "longer");
    key(&mut app, KeyCode::Up);
    assert_eq!(text(&app), "cat");
    key(&mut app, KeyCode::Up);
    assert_eq!(text(&app), "hello");
}
#[test]
fn shift_down_extends_and_insert_down_is_still_a_caret_motion() {
    let mut app = app("[\n  11,\n  222\n]", Language::Json, true);
    select(&mut app, 1, 2, 4);
    app.handle_input(&TerminalInput::Key(KeyEvent::new(
        KeyCode::Down,
        Modifiers::shift(),
    )));
    assert_eq!(selection(&app).anchor, Position::new(1, 2));
    assert_eq!(selection(&app).head, Position::new(2, 4));
    select(&mut app, 1, 2, 4);
    key(&mut app, KeyCode::Char('i'));
    key(&mut app, KeyCode::Down);
    assert!(selection(&app).is_collapsed());
}
#[test]
fn multiple_syntax_selections_move_and_change_together() {
    let mut app = app("[\n [11,22],\n [333,444]\n]", Language::Json, true);
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_selections(CursorState::from_selections(
            Selection::new(Position::new(1, 2), Position::new(1, 4)),
            vec![Selection::new(Position::new(1, 5), Position::new(1, 7))],
        ));
    key(&mut app, KeyCode::Down);
    assert_eq!(
        app.editor().expect("editor").state().cursor.cursor_count(),
        2
    );
    key(&mut app, KeyCode::Char('c'));
    key(&mut app, KeyCode::Char('9'));
    key(&mut app, KeyCode::Escape);
    assert_eq!(
        app.editor().expect("editor").content(),
        "[\n [11,22],\n [9,9]\n]"
    );
    key(&mut app, KeyCode::Char('u'));
    assert_eq!(
        app.editor().expect("editor").content(),
        "[\n [11,22],\n [333,444]\n]"
    );
}
#[test]
fn multiline_nodes_move_past_their_own_body() {
    let mut app = app("[\n [\n  11\n ],\n [\n  22\n ]\n]", Language::Json, true);
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_selection(Position::new(1, 1), Position::new(3, 2));
    key(&mut app, KeyCode::Down);
    assert_eq!(text(&app), "[\n  22\n ]");
    key(&mut app, KeyCode::Up);
    assert_eq!(text(&app), "[\n  11\n ]");
}

#[test]
fn converging_cursors_merge_after_resolving_the_syntax_target() {
    let mut app = app("[\n     [1,2],\n[3]\n]", Language::Json, true);
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_selections(CursorState::from_selections(
            Selection::new(Position::new(1, 6), Position::new(1, 7)),
            vec![Selection::new(Position::new(1, 8), Position::new(1, 9))],
        ));
    key(&mut app, KeyCode::Down);
    assert_eq!(text(&app), "3");
    assert_eq!(
        app.editor().expect("editor").state().cursor.cursor_count(),
        1
    );
}
#[test]
fn grapheme_selections_stay_whole_between_comment_lines() {
    let mut app = app("// e\u{301}arth\n// 漢字", Language::Rust, true);
    select(&mut app, 0, 0, 9);
    key(&mut app, KeyCode::Char(','));
    assert_eq!(text(&app), "e\u{301}arth");
    key(&mut app, KeyCode::Char(','));
    assert_eq!(text(&app), "e\u{301}");
    key(&mut app, KeyCode::Down);
    assert_eq!(text(&app), "漢");
    key(&mut app, KeyCode::Up);
    assert_eq!(text(&app), "e\u{301}");
}

#[test]
fn horizontal_arrows_preserve_nodes_and_cross_enclosing_branches() {
    let mut app = app("[[11,22],[333,444]]", Language::Json, true);
    select(&mut app, 0, 2, 4);
    key(&mut app, KeyCode::Right);
    assert_eq!(text(&app), "22");
    key(&mut app, KeyCode::Right);
    assert_eq!(text(&app), "333");
    key(&mut app, KeyCode::Left);
    assert_eq!(text(&app), "22");
    key(&mut app, KeyCode::Char('.'));
    assert_eq!(text(&app), "[11,22]");
    key(&mut app, KeyCode::Right);
    assert_eq!(text(&app), "[333,444]");
    key(&mut app, KeyCode::Right);
    assert_eq!(text(&app), "[333,444]");
    key(&mut app, KeyCode::Left);
    assert_eq!(text(&app), "[11,22]");
    key(&mut app, KeyCode::Char(','));
    assert_eq!(text(&app), "11");
}

#[test]
fn horizontal_arrows_keep_words_and_graphemes_after_vertical_motion() {
    let mut app = app("// hello world\n// e\u{301}arth moon", Language::Rust, true);
    select(&mut app, 0, 0, 14);
    key(&mut app, KeyCode::Char(','));
    key(&mut app, KeyCode::Down);
    assert_eq!(text(&app), "e\u{301}arth");
    key(&mut app, KeyCode::Right);
    assert_eq!(text(&app), "moon");
    key(&mut app, KeyCode::Left);
    assert_eq!(text(&app), "e\u{301}arth");
    key(&mut app, KeyCode::Char(','));
    assert_eq!(text(&app), "e\u{301}");
    key(&mut app, KeyCode::Right);
    assert_eq!(text(&app), "a");
    key(&mut app, KeyCode::Left);
    assert_eq!(text(&app), "e\u{301}");
}

#[test]
fn horizontal_counts_preserve_backward_selection_and_stop_at_boundary() {
    let mut app = app("[11,22,333]", Language::Json, false);
    select(&mut app, 0, 3, 1);
    key(&mut app, KeyCode::Char('2'));
    key(&mut app, KeyCode::Right);
    assert_eq!(text(&app), "333");
    assert!(selection(&app).is_backward());
    key(&mut app, KeyCode::Right);
    assert_eq!(text(&app), "333");
    key(&mut app, KeyCode::Left);
    assert_eq!(text(&app), "22");
}

#[test]
fn horizontal_multiselections_move_independently_then_change_and_undo() {
    let mut app = app("[11,22,333,444]", Language::Json, true);
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_selections(CursorState::from_selections(
            Selection::new(Position::new(0, 1), Position::new(0, 3)),
            vec![Selection::new(Position::new(0, 7), Position::new(0, 10))],
        ));
    key(&mut app, KeyCode::Right);
    key(&mut app, KeyCode::Char('c'));
    key(&mut app, KeyCode::Char('9'));
    key(&mut app, KeyCode::Escape);
    assert_eq!(app.editor().expect("editor").content(), "[11,9,333,9]");
    key(&mut app, KeyCode::Char('u'));
    assert_eq!(app.editor().expect("editor").content(), "[11,22,333,444]");
}

#[test]
fn horizontal_shift_and_insert_arrows_keep_character_motions() {
    let mut app = app("[11,22]", Language::Json, true);
    select(&mut app, 0, 1, 3);
    app.handle_input(&TerminalInput::Key(KeyEvent::new(
        KeyCode::Right,
        Modifiers::shift(),
    )));
    assert_eq!(
        selection(&app),
        Selection::new(Position::new(0, 1), Position::new(0, 4))
    );
    select(&mut app, 0, 1, 3);
    key(&mut app, KeyCode::Char('i'));
    key(&mut app, KeyCode::Right);
    assert!(selection(&app).is_collapsed());
    assert_eq!(selection(&app).head, Position::new(0, 4));
}
