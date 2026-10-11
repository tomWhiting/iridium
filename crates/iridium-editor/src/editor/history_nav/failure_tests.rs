//! Navigation failures must leave the complete source state available to retry.

use std::sync::{Arc, Mutex};

use crate::search::SearchOptions;
use crate::{Command, Editor, EditorConfig, EditorEvent, Position, Range};

fn editor() -> Editor {
    let mut editor = Editor::new(EditorConfig {
        undo_group_timeout_ms: 0,
        ..EditorConfig::default()
    });
    editor.set_content("abcdef");
    editor.set_language(crate::Language::Rust);
    editor
}

fn insert(editor: &mut Editor, column: usize, text: &str) {
    editor.apply_command(Command::Insert {
        position: Position::new(0, column),
        text: text.to_owned(),
    });
}

fn change_without_history(editor: &mut Editor, text: &str) {
    let end = editor
        .state()
        .document
        .offset_to_position(editor.content().len())
        .expect("document end");
    editor
        .state_mut()
        .document
        .replace(Range::new(Position::zero(), end), text)
        .expect("replace fixture");
    editor
        .find("b", &SearchOptions::default())
        .expect("valid query");
}

fn assert_refused(editor: &mut Editor, code: &str, action: impl FnOnce(&mut Editor) -> bool) {
    let text = editor.content();
    let revision = editor.state().document.revision();
    let cursor = editor.state().cursor.clone();
    let history = serde_json::to_value(editor.history_snapshot()).expect("snapshot");
    let search = editor.search_state().matches.clone();
    let folds = editor.state().fold_state.generation();
    let parses = (
        editor.state().syntax.full_parses(),
        editor.state().syntax.incremental_parses(),
    );
    let events = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&events);
    editor.add_listener(move |event| recorded.lock().expect("event log").push(event.clone()));
    assert!(!action(editor));
    assert_eq!(editor.content(), text);
    assert_eq!(editor.state().document.revision(), revision);
    assert_eq!(editor.state().cursor, cursor);
    assert_eq!(
        serde_json::to_value(editor.history_snapshot()).expect("snapshot"),
        history
    );
    assert_eq!(editor.search_state().matches, search);
    assert_eq!(editor.state().fold_state.generation(), folds);
    assert_eq!(
        (
            editor.state().syntax.full_parses(),
            editor.state().syntax.incremental_parses()
        ),
        parses
    );
    assert!(matches!(events.lock().expect("event log").as_slice(),
        [EditorEvent::Error { code: emitted, .. }] if emitted == code));
}

#[test]
fn failed_undo_keeps_its_source_node_and_can_retry_after_repair() {
    let mut editor = editor();
    insert(&mut editor, 6, "!");
    change_without_history(&mut editor, "b");
    assert_refused(&mut editor, "UNDO_FAILED", Editor::undo);
    change_without_history(&mut editor, "abcdef!");
    assert!(editor.undo());
    assert_eq!(editor.content(), "abcdef");
}

#[test]
fn failed_redo_keeps_its_source_node() {
    let mut editor = editor();
    insert(&mut editor, 6, "!");
    assert!(editor.undo());
    change_without_history(&mut editor, "b");
    assert_refused(&mut editor, "REDO_FAILED", Editor::redo);
}

#[test]
fn failed_branch_entry_keeps_the_previously_preferred_branch() {
    let mut editor = editor();
    insert(&mut editor, 6, "!");
    assert!(editor.undo());
    insert(&mut editor, 6, "?");
    assert!(editor.undo());
    assert!(editor.cycle_history_branch(false));
    assert_eq!(editor.state().history.active_branch_index(), Some(0));
    change_without_history(&mut editor, "b");
    assert_refused(&mut editor, "REDO_BRANCH_FAILED", |editor| {
        editor.redo_branch(1)
    });
}

#[test]
fn failed_jump_rolls_back_successful_leading_undo_steps() {
    let mut editor = editor();
    let root = editor.current_history_node();
    insert(&mut editor, 5, "X");
    insert(&mut editor, 0, "Y");
    change_without_history(&mut editor, "abc");
    assert_refused(&mut editor, "HISTORY_JUMP_FAILED", |editor| {
        editor.jump_to_history_node(root)
    });
}

#[test]
fn failed_cross_branch_jump_rolls_back_undo_before_a_failed_redo() {
    let mut editor = editor();
    insert(&mut editor, 5, "X");
    let target = editor.current_history_node();
    assert!(editor.undo());
    insert(&mut editor, 0, "Y");
    change_without_history(&mut editor, "abc");
    assert_refused(&mut editor, "HISTORY_JUMP_FAILED", |editor| {
        editor.jump_to_history_node(target)
    });
}
