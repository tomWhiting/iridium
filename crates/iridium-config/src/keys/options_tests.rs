use iridium_editor::commands::structural_keymap::{STRUCTURAL, structural_keymap};
use iridium_editor::{Editor, KeyCode, KeyEvent, Position};

use super::{bindings, keymap};

#[test]
fn composed_binding_loads_runs_and_undoes_as_one_edit() {
    let (bindings, problems) =
        load("[keys]\nf8 = { command = 'selection.selectAll', then = ['transform.upperCase'] }\n");
    assert!(problems.is_empty(), "{problems:?}");
    let mut editor = Editor::with_defaults();
    editor.set_content("one two");
    editor
        .push_keymap(keymap(bindings))
        .expect("composition installed");
    editor.handle_key(&KeyEvent::simple(KeyCode::F8));
    assert_eq!(editor.content(), "ONE TWO");
    assert!(editor.undo());
    assert_eq!(editor.content(), "one two");
    assert!(!editor.undo());
}

#[test]
fn following_command_typo_drops_only_its_binding() {
    let (bindings, problems) = load(
        "[keys]\nf8 = { command = 'selection.selectAll', then = ['not.a.command'] }\nf9 = 'selection.selectAll'\n",
    );
    assert!(problems.is_empty());
    let mut editor = Editor::with_defaults();
    let problems = crate::install(bindings, |keys| {
        editor.push_keymap(keys).map_err(crate::Refusal::Keymap)
    });
    assert_eq!(problems.len(), 1);
    editor.set_content("one two");
    editor.handle_key(&KeyEvent::simple(KeyCode::F9));
    assert!(!editor.state().cursor.primary.is_collapsed());
}

fn load(text: &str) -> (Vec<iridium_editor::KeyBinding>, Vec<crate::Problem>) {
    let document = text.parse().expect("TOML fixture");
    let mut problems = Vec::new();
    let bindings = bindings(&document, &mut problems);
    (bindings, problems)
}

#[test]
fn configured_entry_exit_and_counted_keys_run_through_the_resolver() {
    let (bindings, problems) = load(
        r#"
[keys]
f7 = { command = "ast.selectNode", enter_mode = "structural" }
f8 = { enter_mode = "insert", mode = "structural" }
right = { command = "cursor.charRight", mode = "structural", count = true }
"#,
    );
    assert!(problems.is_empty(), "{problems:?}");
    let mut editor = Editor::with_defaults();
    editor.set_content("abcdef");
    editor.push_keymap(structural_keymap()).expect("preset");
    editor
        .push_keymap(keymap(bindings))
        .expect("configured keys");
    editor.handle_key(&KeyEvent::simple(KeyCode::F7));
    assert_eq!(editor.mode(), Some(&STRUCTURAL));
    editor.handle_key(&KeyEvent::simple(KeyCode::Char('3')));
    assert_eq!(editor.cursor(), Position::zero());
    editor.handle_key(&KeyEvent::simple(KeyCode::Right));
    assert_eq!(editor.cursor(), Position::new(0, 3));
    editor.handle_key(&KeyEvent::simple(KeyCode::F8));
    editor.handle_key(&KeyEvent::simple(KeyCode::Char('X')));
    assert_eq!(editor.content(), "abcXdef");
}

#[test]
fn malformed_options_are_reported_without_losing_other_bindings() {
    for options in [
        "{ commmand = 'noop' }",
        "{ command = 4 }",
        "{ command = 'noop', count = 'yes' }",
        "{ mode = 'structural' }",
        "{ enter_mode = '' }",
        "{ enter_mode = ' structural' }",
        "{ command = '', enter_mode = 'insert' }",
        "{ command = '', count = true }",
        "{ enter_mode = 'insert', count = true }",
    ] {
        let (bindings, problems) = load(&format!("[keys]\nf7 = {options}\nf8 = 'noop'\n"));
        assert_eq!(problems.len(), 1, "{options}: {problems:?}");
        assert_eq!(bindings.len(), 1, "{options}");
        assert_eq!(bindings[0].display_sequence(), "f8");
    }
}

#[test]
fn mode_scoped_unbinding_preserves_the_scope() {
    let (bindings, problems) = load("[keys]\nf6 = { command = '', mode = 'insert' }\n");
    assert!(problems.is_empty());
    assert!(bindings[0].command().is_none());
    assert_eq!(
        bindings[0].mode().map(ToString::to_string).as_deref(),
        Some("insert")
    );
}

#[test]
fn custom_change_mode_records_input_and_returns_to_its_origin() {
    let (bindings, problems) = load(
        r#"
[keys]
f8 = { command = "selection.selectAll", then = ["edit.deleteForward"], enter_mode = "replacement", record_input = true }
escape = { command = "command.finishChange", mode = "replacement" }
"#,
    );
    assert!(problems.is_empty(), "{problems:?}");
    let mut editor = Editor::with_defaults();
    editor.set_content("before");
    editor.set_mode(Some(iridium_editor::commands::ModeName::new(
        "custom-origin",
    )));
    editor.push_keymap(keymap(bindings)).expect("bindings");
    editor.handle_key(&KeyEvent::simple(KeyCode::F8));
    editor.handle_key(&KeyEvent::simple(KeyCode::Char('x')));
    editor.handle_key(&KeyEvent::simple(KeyCode::Escape));
    assert_eq!(
        editor
            .mode()
            .map(iridium_editor::commands::ModeName::as_str),
        Some("custom-origin")
    );
    assert_eq!(editor.content(), "x");
    assert!(editor.undo());
    assert_eq!(editor.content(), "before");
    assert!(!editor.undo());
    editor
        .repeat_command_sequence(1)
        .expect("repeat custom change");
    assert_eq!(editor.content(), "x");
}

#[test]
fn recording_binding_requires_both_a_command_and_an_input_mode() {
    for value in [
        "{ enter_mode = 'replacement', record_input = true }",
        "{ command = 'edit.deleteForward', record_input = true }",
    ] {
        let (bindings, problems) = load(&format!("[keys]\nf8 = {value}\n"));
        assert!(bindings.is_empty());
        assert_eq!(problems.len(), 1);
    }
}
