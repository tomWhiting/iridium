//! Failed host edits must not escape the command transaction.

use std::sync::{Arc, Mutex};

use crate::search::SearchOptions;
use crate::{
    Command, CursorState, Document, Editor, EditorEvent, KeyCode, KeyEvent, Modifiers, Position,
    Range, Selection,
};

fn insert(line: usize, column: usize, text: &str) -> Command {
    Command::Insert {
        position: Position::new(line, column),
        text: text.to_owned(),
    }
}

fn invalid() -> Command {
    insert(999, 0, "never")
}

#[test]
fn onboarding_reproducer_leaves_no_partial_edit_or_undo_entry() {
    let mut editor = Editor::with_defaults();
    editor.set_content("abc");
    editor.apply_command(Command::Compound {
        commands: vec![insert(0, 0, "X"), invalid()],
    });
    assert_eq!(editor.content(), "abc");
    assert!(!editor.undo());
}

#[test]
fn failed_compound_restores_text_revision_identity_metadata_and_all_cursors() {
    for failing_index in 0..4 {
        let mut document = Document::new("abc\r\ndef");
        document.set_language(Some("rust".to_owned()));
        let mut cursor = CursorState::from_selections(
            Selection::new(Position::new(0, 2), Position::zero()),
            vec![Selection::collapsed(Position::new(1, 2))],
        );
        let before = cursor.clone();
        let (id, revision, ending) = (document.id(), document.revision(), document.line_ending());
        let mut commands = vec![
            insert(0, 0, "é"),
            Command::SetSelection {
                old_state: before.clone(),
                new_state: CursorState::default(),
            },
            Command::Delete {
                range: Range::new(Position::zero(), Position::new(0, 1)),
                deleted_text: "é".to_owned(),
            },
        ];
        commands.insert(failing_index, invalid());
        assert!(
            Command::Compound { commands }
                .apply(&mut document, &mut cursor)
                .is_err()
        );
        assert_eq!(document.text(), "abc\r\ndef");
        assert_eq!(document.id(), id);
        assert_eq!(document.revision(), revision);
        assert_eq!(document.line_ending(), ending);
        assert_eq!(document.language(), Some("rust"));
        assert_eq!(cursor, before);
    }
}

#[test]
fn failure_in_a_nested_compound_rolls_back_its_parent_too() {
    let mut document = Document::new("abc");
    let mut cursor = CursorState::default();
    let command = Command::Compound {
        commands: vec![
            insert(0, 0, "parent"),
            Command::Compound {
                commands: vec![insert(0, 0, "child"), invalid()],
            },
            insert(0, 0, "unreachable"),
        ],
    };
    assert!(command.apply(&mut document, &mut cursor).is_err());
    assert_eq!(document.text(), "abc");
    assert_eq!(document.revision(), 0);
}

#[test]
fn successful_nested_commands_use_the_preceding_childs_document() {
    let mut document = Document::new("a");
    let mut cursor = CursorState::default();
    let command = Command::Compound {
        commands: vec![
            insert(0, 1, "\n"),
            Command::Compound {
                commands: vec![insert(1, 0, "é😀")],
            },
        ],
    };
    command
        .apply(&mut document, &mut cursor)
        .expect("sequential positions");
    assert_eq!(document.text(), "a\né😀");
    command
        .inverse()
        .apply(&mut document, &mut cursor)
        .expect("valid inverse");
    assert_eq!(document.text(), "a");
}

#[test]
fn rejected_host_edit_emits_only_error_and_preserves_search_and_history() {
    let mut editor = Editor::with_defaults();
    editor.set_content("abc");
    editor.apply_command(insert(0, 3, "!"));
    editor
        .find("abc", &SearchOptions::default())
        .expect("valid search");
    let before = editor.state().cursor.clone();
    let revision = editor.state().document.revision();
    let history = editor.current_history_node();
    let matches = editor.search_state().matches.clone();
    let events = Arc::new(Mutex::new(Vec::new()));
    let listener = Arc::clone(&events);
    editor.add_listener(move |event| listener.lock().expect("event log").push(event.clone()));
    editor.apply_command(Command::Compound {
        commands: vec![insert(0, 0, "X"), invalid()],
    });
    assert_eq!(editor.content(), "abc!");
    assert_eq!(editor.state().document.revision(), revision);
    assert_eq!(editor.state().cursor, before);
    assert_eq!(editor.search_state().matches, matches);
    assert_eq!(editor.current_history_node(), history);
    assert!(matches!(events.lock().expect("event log").as_slice(),
        [EditorEvent::Error { code, .. }] if code == "COMMAND_FAILED"));
    assert!(editor.undo());
    assert_eq!(editor.content(), "abc");
    assert!(editor.redo());
    assert_eq!(editor.content(), "abc!");
}

#[test]
fn failed_host_edit_preserves_the_preferred_vertical_column() {
    let mut editor = Editor::with_defaults();
    editor.set_content("abcdef\nx\nabcdef");
    editor.set_cursor(Position::new(0, 5));
    let down = KeyEvent {
        key: KeyCode::Down,
        modifiers: Modifiers::none(),
        is_repeat: false,
    };
    editor.handle_key(&down);
    assert_eq!(editor.cursor(), Position::new(1, 1));
    editor.apply_command(Command::Compound {
        commands: vec![insert(0, 0, "X"), invalid()],
    });
    editor.handle_key(&down);
    assert_eq!(editor.cursor(), Position::new(2, 5));
}

#[test]
fn reversed_serialized_range_is_an_error_and_leaves_the_document_unchanged() {
    let mut document = Document::new("abc");
    let mut cursor = CursorState::default();
    // Public fields and deserialization can bypass Range::new's normalization.
    let range = Range {
        start: Position::new(0, 2),
        end: Position::zero(),
    };
    let command = Command::Compound {
        commands: vec![
            insert(0, 0, "X"),
            Command::Delete {
                range,
                deleted_text: String::new(),
            },
        ],
    };
    assert!(command.apply(&mut document, &mut cursor).is_err());
    assert_eq!(document.text(), "abc");
}

#[cfg(feature = "syntax")]
#[test]
fn rejected_edit_preserves_syntax_folds_and_the_expansion_trail() {
    use crate::commands::builtin::{AST_EXPAND_SELECTION, AST_SHRINK_SELECTION};
    use crate::{CommandArgs, Language};

    let mut editor = Editor::with_defaults();
    let text = "fn main() {\n    let alpha = 1;\n    let beta = 2;\n}\n";
    editor.set_content(text);
    editor.set_language(Language::Rust);
    editor.set_cursor(Position::new(1, 9));
    editor
        .run_command(AST_EXPAND_SELECTION.as_str(), CommandArgs::NONE)
        .expect("expand identifier");
    let inner = editor.state().cursor.clone();
    editor
        .run_command(AST_EXPAND_SELECTION.as_str(), CommandArgs::NONE)
        .expect("expand parent");
    let outer = editor.state().cursor.clone();
    assert_ne!(inner, outer);
    let folds = editor.state().fold_state.generation();
    let full = editor.state().syntax.full_parses();
    let incremental = editor.state().syntax.incremental_parses();
    assert!(full > 0);
    editor.apply_command(Command::Compound {
        commands: vec![insert(0, 0, "\n"), invalid()],
    });
    assert_eq!(editor.content(), text);
    assert_eq!(editor.state().cursor, outer);
    assert_eq!(editor.state().fold_state.generation(), folds);
    assert_eq!(editor.state().syntax.full_parses(), full);
    assert_eq!(editor.state().syntax.incremental_parses(), incremental);
    editor
        .run_command(AST_SHRINK_SELECTION.as_str(), CommandArgs::NONE)
        .expect("shrink back");
    assert_eq!(editor.state().cursor, inner);
}
