use super::*;
use crate::Position;
#[cfg(feature = "syntax")]
use crate::commands::builtin::AST_SELECT_NEXT_SIBLING;
use crate::commands::builtin::{
    AST_SELECT_NODE, CLIPBOARD_COPY, EDIT_DELETE_FORWARD, SELECTION_SELECT_ALL,
    TRANSFORM_UPPER_CASE,
};
use crate::commands::{CommandArgs, CommandId};
#[cfg(feature = "syntax")]
use crate::{KeyCode, KeyEvent, Selection};

fn step(command: CommandId) -> CommandInvocation {
    CommandInvocation::bare(command)
}

fn editor(text: &str) -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content(text);
    editor
}

#[test]
fn edits_and_selections_commit_as_one_undo_unit() {
    let mut editor = editor("héllo\nworld");
    editor.set_cursor(Position::new(1, 2));
    let cursor = editor.state.cursor.clone();
    editor
        .run_command_sequence(&[step(SELECTION_SELECT_ALL), step(TRANSFORM_UPPER_CASE)])
        .expect("composed edit");
    assert_eq!(editor.content(), "HÉLLO\nWORLD");
    assert!(editor.undo());
    assert_eq!(editor.content(), "héllo\nworld");
    assert_eq!(editor.state.cursor, cursor);
    assert!(!editor.undo());
    assert!(editor.redo());
    assert_eq!(editor.content(), "HÉLLO\nWORLD");
}

#[test]
fn late_host_operation_rolls_back_edits_cursors_history_and_revision() {
    let mut editor = editor("héllo");
    editor.set_cursor(Position::new(0, 2));
    let cursor = editor.state.cursor.clone();
    let revision = editor.state.document.revision();
    let error = editor
        .run_command_sequence(&[
            step(SELECTION_SELECT_ALL),
            step(TRANSFORM_UPPER_CASE),
            step(CLIPBOARD_COPY),
        ])
        .expect_err("external operation rejected");
    assert!(error.contains("step 3 (clipboard.copy)"), "{error}");
    assert_eq!(editor.content(), "héllo");
    assert_eq!(editor.state.cursor, cursor);
    assert_eq!(editor.state.document.revision(), revision);
    assert!(!editor.undo());
    assert!(editor.repeat_command_sequence(1).is_err());
}

#[test]
fn readonly_rejection_keeps_prior_selection() {
    let mut editor = editor("read only");
    editor.state.read_only = true;
    let cursor = editor.state.cursor.clone();
    let error = editor
        .run_command_sequence(&[step(SELECTION_SELECT_ALL), step(TRANSFORM_UPPER_CASE)])
        .expect_err("read only");
    assert!(error.contains("read-only"));
    assert_eq!(editor.state.cursor, cursor);
    assert_eq!(editor.content(), "read only");
}

#[test]
fn excessive_count_is_rejected_before_any_selection_changes() {
    let mut editor = editor("hello");
    let command = CommandInvocation::new(SELECTION_SELECT_ALL, CommandArgs::with_count(u32::MAX));
    assert!(editor.run_command_sequence(&[command]).is_err());
    assert!(editor.state.cursor.primary.is_collapsed());
    let zero = CommandInvocation::new(SELECTION_SELECT_ALL, CommandArgs::with_count(0));
    assert!(
        editor
            .run_command_sequence(&[zero, step(EDIT_DELETE_FORWARD)])
            .is_err()
    );
    assert_eq!(editor.content(), "hello");
}

#[test]
fn counted_repeat_is_one_undo_unit_and_does_not_multiply_the_saved_recipe() {
    use crate::commands::builtin::{COMMAND_REPEAT_SEQUENCE, CURSOR_WORD_RIGHT_SELECT};
    let mut editor = editor("one two three four five");
    editor
        .run_command_sequence(&[step(CURSOR_WORD_RIGHT_SELECT), step(EDIT_DELETE_FORWARD)])
        .expect("delete first word");
    assert_eq!(editor.content(), "two three four five");
    editor
        .run_command(COMMAND_REPEAT_SEQUENCE.as_str(), CommandArgs::with_count(2))
        .expect("repeat command");
    assert_eq!(editor.content(), "four five");
    assert!(editor.undo());
    assert_eq!(editor.content(), "two three four five");
    editor.repeat_command_sequence(1).expect("one repetition");
    assert_eq!(editor.content(), "three four five");
}

#[test]
fn composition_publishes_only_the_final_content_and_selection() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let mut editor = editor("hello");
    let selections = Arc::new(AtomicUsize::new(0));
    let changes = Arc::new(AtomicUsize::new(0));
    let (seen_selections, seen_changes) = (selections.clone(), changes.clone());
    editor.add_listener(move |event| match event {
        crate::EditorEvent::SelectionChanged { .. } => {
            seen_selections.fetch_add(1, Ordering::SeqCst);
        },
        crate::EditorEvent::ContentChanged { .. } => {
            seen_changes.fetch_add(1, Ordering::SeqCst);
        },
        _ => {},
    });
    editor
        .run_command_sequence(&[step(SELECTION_SELECT_ALL), step(TRANSFORM_UPPER_CASE)])
        .expect("composition");
    assert_eq!(selections.load(Ordering::SeqCst), 1);
    assert_eq!(changes.load(Ordering::SeqCst), 1);
}

#[test]
fn parser_free_target_cannot_fall_through_to_delete() {
    let mut editor = editor("untouched");
    assert!(
        editor
            .run_command_sequence(&[step(AST_SELECT_NODE), step(EDIT_DELETE_FORWARD)])
            .is_err()
    );
    assert_eq!(editor.content(), "untouched");
    assert!(!editor.undo());
}

#[test]
fn repeat_uses_current_targets_and_keeps_its_recipe_after_failure() {
    let mut editor = editor("one two");
    editor.set_selection(Position::new(0, 0), Position::new(0, 3));
    editor
        .run_command_sequence(&[step(TRANSFORM_UPPER_CASE)])
        .expect("first edit");
    editor.set_selection(Position::new(0, 4), Position::new(0, 7));
    editor.state.read_only = true;
    assert!(editor.repeat_command_sequence(1).is_err());
    editor.state.read_only = false;
    editor
        .repeat_command_sequence(1)
        .expect("repeat after failure");
    assert_eq!(editor.content(), "ONE TWO");
    assert!(editor.undo());
    assert_eq!(editor.content(), "ONE two");
}

#[test]
#[cfg(feature = "syntax")]
fn structural_count_moves_then_deletes_once_and_undo_restores_original_target() {
    let mut editor = editor("[11,22,33]");
    editor.set_language(crate::Language::Json);
    editor.set_selection(Position::new(0, 1), Position::new(0, 3));
    let before = editor.state.cursor.clone();
    let steps = [
        CommandInvocation::new(AST_SELECT_NEXT_SIBLING, CommandArgs::with_count(2)),
        step(EDIT_DELETE_FORWARD),
    ];
    editor
        .run_command_sequence(&steps)
        .expect("counted selection plus deletion");
    assert_eq!(editor.content(), "[11,22,]");
    assert!(editor.undo());
    assert_eq!(editor.state.cursor, before);
    assert_eq!(editor.content(), "[11,22,33]");
    assert!(!editor.undo());
    editor.set_selection(Position::new(0, 7), Position::new(0, 9));
    assert!(editor.run_command_sequence(&steps).is_err());
    assert_eq!(editor.content(), "[11,22,33]");
}

#[test]
#[cfg(feature = "syntax")]
fn operator_prefix_cancels_counts_and_delays_mode_transition_until_success() {
    use crate::commands::structural_keymap::{
        STRUCTURAL_CHANGE, STRUCTURAL_VERB, structural_keymap,
    };
    let mut editor = editor("[11,22,33]");
    editor.set_language(crate::Language::Json);
    editor.set_cursor(Position::new(0, 1));
    editor.push_keymap(structural_keymap()).expect("preset");
    for key in [
        KeyCode::F7,
        KeyCode::Char('d'),
        KeyCode::Char('2'),
        KeyCode::Escape,
    ] {
        editor.handle_key(&KeyEvent::simple(key));
    }
    assert_eq!(editor.content(), "[11,22,33]");
    assert_eq!(editor.mode(), Some(&STRUCTURAL_VERB));
    for key in [KeyCode::Char('c'), KeyCode::Char('2'), KeyCode::Down] {
        editor.handle_key(&KeyEvent::simple(key));
    }
    assert_eq!(editor.content(), "[11,22,]");
    assert_eq!(editor.mode(), Some(&STRUCTURAL_CHANGE));
    assert!(editor.undo());
    editor.set_selection(Position::new(0, 7), Position::new(0, 9));
    for key in [KeyCode::F7, KeyCode::Char('c')] {
        editor.handle_key(&KeyEvent::simple(key));
    }
    assert!(matches!(
        editor.handle_key(&KeyEvent::simple(KeyCode::Down)),
        crate::EditorKeyResult::Error(_)
    ));
    assert_eq!(editor.mode(), Some(&STRUCTURAL_VERB));
    assert_eq!(
        editor.state.cursor.primary,
        Selection::new(Position::new(0, 7), Position::new(0, 9))
    );
}

#[test]
#[cfg(feature = "syntax")]
fn merged_structural_targets_are_edited_once_and_undo_restores_all_cursors() {
    use crate::commands::builtin::AST_EXPAND_SELECTION;
    let mut editor = editor("[11,22,33]");
    editor.set_language(crate::Language::Json);
    let original = CursorState::from_selections(
        Selection::new(Position::new(0, 3), Position::new(0, 1)),
        vec![Selection::new(Position::new(0, 4), Position::new(0, 6))],
    );
    editor.set_selections(original.clone());
    let depth = editor.state.syntax.expansion_depth();
    assert!(
        editor
            .run_command_sequence(&[step(AST_EXPAND_SELECTION), step(CLIPBOARD_COPY)])
            .is_err()
    );
    assert_eq!(editor.state.syntax.expansion_depth(), depth);
    assert_eq!(editor.state.cursor, original);
    editor
        .run_command_sequence(&[step(AST_EXPAND_SELECTION), step(EDIT_DELETE_FORWARD)])
        .expect("merged parent deletion");
    assert_eq!(editor.content(), "");
    assert!(editor.undo());
    assert_eq!(editor.content(), "[11,22,33]");
    assert_eq!(editor.state.cursor, original);
}

#[test]
#[cfg(feature = "syntax")]
fn rust_function_navigation_composes_with_the_same_delete_command() {
    use crate::commands::builtin::AST_EXPAND_SELECTION;
    let mut editor = editor("fn first() {}\nfn second() {}");
    editor.set_language(crate::Language::Rust);
    editor.set_selection(Position::new(0, 3), Position::new(0, 8));
    editor
        .run_command_sequence(&[
            step(AST_EXPAND_SELECTION),
            step(AST_SELECT_NEXT_SIBLING),
            step(EDIT_DELETE_FORWARD),
        ])
        .expect("parent then next function then ordinary delete");
    assert_eq!(editor.content(), "fn first() {}\n");
    assert!(editor.undo());
    assert_eq!(editor.content(), "fn first() {}\nfn second() {}");
}

#[test]
fn composed_bindings_roundtrip_and_do_not_advertise_destructive_selection_shortcuts() {
    use crate::commands::{KeyBinding, Keymap, StrokePattern};
    let binding = KeyBinding::new(
        StrokePattern::plain(crate::KeyCode::F8),
        &[],
        SELECTION_SELECT_ALL,
    )
    .then_command(EDIT_DELETE_FORWARD);
    let json = serde_json::to_string(&binding).expect("serialized binding");
    let decoded: KeyBinding = serde_json::from_str(&json).expect("roundtrip");
    assert_eq!(decoded, binding);
    let mut keys = Keymap::new("composed");
    keys.push(decoded);
    let mut editor = editor("hello");
    editor.push_keymap(keys).expect("valid composition");
    assert!(
        editor
            .key_hints()
            .hints_for(SELECTION_SELECT_ALL.as_str())
            .iter()
            .all(|hint| !hint.binding_text().contains("f8"))
    );
}
