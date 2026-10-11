use crate::commands::structural_keymap::{STRUCTURAL_VERB, structural_keymap};
use crate::{Editor, KeyCode, KeyEvent, Language, Position};

fn press(editor: &mut Editor, key: KeyCode) {
    editor.handle_key(&KeyEvent::simple(key));
}

#[test]
fn change_typing_finishes_as_one_undoable_repeatable_edit() {
    let mut editor = Editor::with_defaults();
    editor.set_content("[11,22,33,44]");
    editor.set_language(Language::Json);
    editor.set_cursor(Position::new(0, 1));
    editor.push_keymap(structural_keymap()).expect("preset");
    for key in [
        KeyCode::F7,
        KeyCode::Char('c'),
        KeyCode::Down,
        KeyCode::Char('9'),
        KeyCode::Char('8'),
        KeyCode::Backspace,
        KeyCode::Char('9'),
        KeyCode::Escape,
    ] {
        press(&mut editor, key);
    }
    assert_eq!(editor.content(), "[11,99,33,44]");
    assert_eq!(editor.mode(), Some(&STRUCTURAL_VERB));
    assert!(editor.undo());
    assert_eq!(editor.content(), "[11,22,33,44]");
    assert!(!editor.undo());
    assert!(editor.redo());
    editor.set_selection(Position::new(0, 4), Position::new(0, 6));
    press(&mut editor, KeyCode::Char('.'));
    assert_eq!(editor.content(), "[11,99,99,44]");
    assert!(editor.undo());
    assert_eq!(editor.content(), "[11,99,33,44]");
}

fn selection_first() -> Result<Editor, crate::commands::KeymapError> {
    let mut editor = Editor::with_defaults();
    editor.set_content("[11,22,33]");
    editor.set_language(Language::Json);
    editor.set_cursor(Position::new(0, 1));
    editor.set_undo_group_timeout_ms(0);
    editor.push_keymap(structural_keymap())?;
    press(&mut editor, KeyCode::F6);
    press(&mut editor, KeyCode::Char('c'));
    Ok(editor)
}

#[test]
fn paste_and_cursor_motions_record_with_typing_without_timeout_grouping() {
    let mut editor = selection_first().expect("selection-first fixture");
    editor.paste("456");
    press(&mut editor, KeyCode::Left);
    press(&mut editor, KeyCode::Backspace);
    press(&mut editor, KeyCode::Char('8'));
    press(&mut editor, KeyCode::Escape);
    assert_eq!(editor.content(), "[486,22,33]");
    let final_cursor = editor.state().cursor.clone();
    assert!(editor.undo());
    assert_eq!(editor.content(), "[11,22,33]");
    assert!(!editor.undo());
    assert!(editor.redo());
    assert_eq!(editor.state().cursor, final_cursor);
    editor.set_selection(Position::new(0, 5), Position::new(0, 7));
    press(&mut editor, KeyCode::Char('.'));
    assert_eq!(editor.content(), "[486,486,33]");
}

#[test]
fn repeated_typing_uses_auto_pairs_and_skips_the_generated_closer() {
    let mut editor = selection_first().expect("selection-first fixture");
    for key in [
        KeyCode::Char('"'),
        KeyCode::Char('x'),
        KeyCode::Char('"'),
        KeyCode::Escape,
    ] {
        press(&mut editor, key);
    }
    assert_eq!(editor.content(), "[\"x\",22,33]");
    editor.set_selection(Position::new(0, 5), Position::new(0, 7));
    press(&mut editor, KeyCode::Char('.'));
    assert_eq!(editor.content(), "[\"x\",\"x\",33]");
}

#[test]
fn change_fans_out_to_children_and_ime_commit_keeps_every_cursor() {
    let mut editor = Editor::with_defaults();
    editor.set_content("[11,22]");
    editor.set_language(Language::Json);
    editor.push_keymap(structural_keymap()).expect("preset");
    for key in [KeyCode::F6, KeyCode::Char('a'), KeyCode::Char('c')] {
        press(&mut editor, key);
    }
    editor.handle_ime(&crate::input::ImeEvent::Commit { text: "7".into() });
    press(&mut editor, KeyCode::Escape);
    assert_eq!(editor.content(), "[7,7]");
    assert_eq!(editor.state().cursor.cursor_count(), 2);
    assert!(editor.undo());
    assert_eq!(editor.content(), "[11,22]");
    assert!(!editor.undo());
    assert!(editor.redo());
    editor.set_selection(Position::new(0, 1), Position::new(0, 2));
    press(&mut editor, KeyCode::Char('.'));
    assert_eq!(editor.content(), "[7,7]");
}

#[test]
fn undo_during_input_finishes_the_recipe_and_restores_the_origin_mode() {
    let mut editor = selection_first().expect("selection-first fixture");
    press(&mut editor, KeyCode::Char('9'));
    assert!(editor.undo());
    assert_eq!(editor.content(), "[11,22,33]");
    assert_eq!(
        editor.mode(),
        Some(&crate::commands::structural_keymap::STRUCTURAL)
    );
    press(&mut editor, KeyCode::Char('.'));
    assert_eq!(editor.content(), "[9,22,33]");
    assert!(editor.undo());
    assert!(!editor.undo());
}

#[test]
fn oversized_paste_is_refused_before_mutation_and_recording_can_continue() {
    let mut editor = selection_first().expect("selection-first fixture");
    let before = editor.state().cursor.clone();
    let revision = editor.state().document.revision();
    assert!(matches!(
        editor.paste_with_result(&"x".repeat(1_048_577)),
        crate::EditorKeyResult::Error(_)
    ));
    assert_eq!(editor.content(), "[,22,33]");
    assert_eq!(editor.state().document.revision(), revision);
    assert_eq!(editor.state().cursor, before);
    press(&mut editor, KeyCode::Char('9'));
    press(&mut editor, KeyCode::Escape);
    assert!(editor.undo());
    assert_eq!(editor.content(), "[11,22,33]");
    press(&mut editor, KeyCode::Char('.'));
    assert_eq!(editor.content(), "[9,22,33]");
}

#[test]
fn failed_start_and_read_only_replay_preserve_the_previous_recipe() {
    use crate::commands::{CommandInvocation, builtin::SELECTION_SELECT_ALL};
    let mut editor = selection_first().expect("selection-first fixture");
    editor.paste("9");
    editor.finish_change();
    let cursor = editor.state().cursor.clone();
    assert!(
        editor
            .run_input_sequence(
                &[CommandInvocation::bare(SELECTION_SELECT_ALL)],
                Some(crate::commands::ModeName::new("custom-input")),
                true
            )
            .is_err()
    );
    assert_eq!(editor.state().cursor, cursor);
    editor.set_selection(Position::new(0, 3), Position::new(0, 5));
    editor.state_mut().read_only = true;
    assert!(editor.repeat_command_sequence(1).is_err());
    assert_eq!(editor.content(), "[9,22,33]");
    editor.state_mut().read_only = false;
    editor.repeat_command_sequence(1).expect("recipe retained");
    assert_eq!(editor.content(), "[9,9,33]");
}
