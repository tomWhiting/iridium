//! Search selections must survive real movement, editing and history replay.

use crate::commands::builtin::{
    CURSOR_LINE_END, CURSOR_WORD_RIGHT_SELECT, SEARCH_SELECT_ALL_MATCHES,
};
use crate::search::SearchOptions;
use crate::{CommandArgs, CursorState, Editor, KeyCode, KeyEvent, Modifiers, Position, Selection};

fn searching(text: &str, query: &str, options: &SearchOptions) -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content(text);
    editor.find(query, options).expect("valid query");
    editor
}

fn type_character(editor: &mut Editor, c: char) {
    editor.handle_key(&KeyEvent {
        key: KeyCode::Char(c),
        modifiers: Modifiers::none(),
        is_repeat: false,
    });
}

#[test]
fn regex_results_become_selections_then_edit_and_undo_as_one() {
    let mut editor = searching(
        "item_1 tail\nitem_22 tail\nitem_333 tail",
        "item_[0-9]+",
        &SearchOptions::regex_mode(),
    );
    editor.goto_next_match();
    editor
        .run_command(SEARCH_SELECT_ALL_MATCHES.as_str(), CommandArgs::NONE)
        .expect("registered command");
    let selected = editor.state().cursor.clone();
    assert_eq!(selected.cursor_count(), 3);
    assert_eq!(
        selected.primary,
        Selection::new(Position::new(1, 0), Position::new(1, 7))
    );
    type_character(&mut editor, 'X');
    assert_eq!(editor.content(), "X tail\nX tail\nX tail");
    assert!(editor.undo());
    assert_eq!(editor.content(), "item_1 tail\nitem_22 tail\nitem_333 tail");
    assert_eq!(editor.state().cursor, selected);
    assert!(editor.redo());
    assert_eq!(editor.content(), "X tail\nX tail\nX tail");
}

#[test]
fn word_and_line_motions_preserve_every_search_cursor() {
    let mut editor = searching(
        "tag apple end\ntag banana end\ntag pear end",
        "tag",
        &SearchOptions::default(),
    );
    assert_eq!(editor.select_search_matches(), Ok(3));
    editor.close_search();
    editor
        .run_command(CURSOR_WORD_RIGHT_SELECT.as_str(), CommandArgs::NONE)
        .expect("word motion");
    assert_eq!(editor.state().cursor.cursor_count(), 3);
    assert!(
        editor
            .state()
            .cursor
            .all_selections()
            .all(|s| !s.is_collapsed())
    );
    editor
        .run_command(CURSOR_LINE_END.as_str(), CommandArgs::NONE)
        .expect("line motion");
    type_character(&mut editor, '!');
    assert_eq!(
        editor.content(),
        "tag apple end!\ntag banana end!\ntag pear end!"
    );
    assert!(editor.undo());
    assert_eq!(
        editor.content(),
        "tag apple end\ntag banana end\ntag pear end"
    );
    assert_eq!(editor.state().cursor.cursor_count(), 3);
}

#[test]
fn case_and_word_options_match_the_preview() {
    let mut editor = searching(
        "cat Cat scatter cat",
        "cat",
        &SearchOptions {
            case_sensitive: true,
            whole_word: true,
            regex: false,
        },
    );
    assert_eq!(editor.select_search_matches(), Ok(2));
    type_character(&mut editor, 'X');
    assert_eq!(editor.content(), "X Cat scatter X");
}

#[test]
fn adjacent_unicode_matches_stay_separate() {
    let mut editor = searching("éé😀é", "é", &SearchOptions::default());
    assert_eq!(editor.select_search_matches(), Ok(3));
    type_character(&mut editor, 'X');
    assert_eq!(editor.content(), "XX😀X");
}

#[test]
fn zero_width_regex_results_become_insertion_carets() {
    let mut editor = searching("one\ntwo\nthree", "(?m)^", &SearchOptions::regex_mode());
    assert_eq!(editor.select_search_matches(), Ok(3));
    type_character(&mut editor, '>');
    assert_eq!(editor.content(), ">one\n>two\n>three");
    assert!(editor.undo());
    assert_eq!(editor.content(), "one\ntwo\nthree");
}

#[test]
fn no_matches_and_invalid_patterns_preserve_existing_selections() {
    let mut editor = searching("one two", "missing", &SearchOptions::default());
    editor.set_selection(Position::new(0, 0), Position::new(0, 3));
    let before = editor.state().cursor.clone();
    assert_eq!(editor.select_search_matches(), Ok(0));
    assert_eq!(editor.state().cursor, before);
    assert!(editor.find("[", &SearchOptions::regex_mode()).is_err());
    assert!(editor.select_search_matches().is_err());
    assert_eq!(editor.state().cursor, before);
    assert_eq!(editor.content(), "one two");
    assert!(!editor.undo());
}

#[test]
fn stale_ranges_are_recomputed_before_selection() {
    let mut editor = searching("a a", "a", &SearchOptions::default());
    editor
        .state_mut()
        .document
        .insert(Position::zero(), "prefix ")
        .expect("valid insertion");
    assert_eq!(editor.select_search_matches(), Ok(2));
    type_character(&mut editor, 'X');
    assert_eq!(editor.content(), "prefix X X");
}

#[test]
fn read_only_allows_selection_but_refuses_typing() {
    let mut editor = searching("a a", "a", &SearchOptions::default());
    editor.state_mut().read_only = true;
    assert_eq!(editor.select_search_matches(), Ok(2));
    type_character(&mut editor, 'X');
    assert_eq!(editor.content(), "a a");
    assert!(!editor.undo());
}

#[test]
fn batch_cursor_construction_keeps_the_existing_merge_rules() {
    let primary = Selection::new(Position::new(0, 1), Position::new(0, 3));
    let cursors = CursorState::from_selections(
        primary,
        vec![
            Selection::collapsed(Position::new(0, 3)),
            Selection::new(Position::new(0, 3), Position::new(0, 5)),
            Selection::collapsed(Position::new(0, 8)),
            Selection::collapsed(Position::new(0, 8)),
        ],
    );
    assert_eq!(cursors.cursor_count(), 3);
    assert_eq!(cursors.primary, primary);
}

#[test]
fn thousands_of_matches_are_all_selected() {
    let mut editor = searching(&"x ".repeat(10_000), "x", &SearchOptions::default());
    assert_eq!(editor.select_search_matches(), Ok(10_000));
}
