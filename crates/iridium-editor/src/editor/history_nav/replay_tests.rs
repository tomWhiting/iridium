//! Successful replay must preserve syntax accuracy and normal undo grouping.

use crate::{Command, Editor, EditorConfig, Position};

fn insert(editor: &mut Editor, line: usize, column: usize, text: &str) {
    editor.apply_command(Command::Insert {
        position: Position::new(line, column),
        text: text.to_owned(),
    });
}

#[test]
fn jumping_to_current_node_does_not_split_the_typing_group() {
    let mut editor = Editor::new(EditorConfig {
        undo_group_timeout_ms: u64::MAX,
        ..EditorConfig::default()
    });
    insert(&mut editor, 0, 0, "a");
    let here = editor.current_history_node();
    assert!(editor.jump_to_history_node(here));
    insert(&mut editor, 0, 1, "b");
    assert_eq!(editor.current_history_node(), here);
    assert!(editor.undo());
    assert_eq!(editor.content(), "");
}

#[cfg(feature = "syntax")]
fn parsed_shape(editor: &mut Editor) -> Vec<(String, usize, usize)> {
    let state = editor.state_mut();
    let tree = state
        .syntax
        .sync(&state.document)
        .expect("Rust syntax tree");
    let mut nodes = vec![tree.root_node()];
    let mut shape = Vec::new();
    while let Some(node) = nodes.pop() {
        shape.push((node.kind().to_owned(), node.start_byte(), node.end_byte()));
        for index in 0..node.child_count() {
            let index = u32::try_from(index).expect("node child index");
            nodes.push(node.child(index).expect("child in range"));
        }
    }
    shape
}

#[cfg(feature = "syntax")]
fn assert_fresh_parse(editor: &mut Editor) {
    let mut fresh = Editor::with_defaults();
    fresh.set_content(&editor.content());
    fresh.set_language(crate::Language::Rust);
    assert_eq!(parsed_shape(editor), parsed_shape(&mut fresh));
    assert_eq!(
        editor.state().fold_state.regions(),
        fresh.state().fold_state.regions()
    );
}

#[cfg(feature = "syntax")]
#[test]
fn multiline_unicode_jump_keeps_incremental_syntax_and_correct_coordinates() {
    let mut editor = Editor::new(EditorConfig {
        undo_group_timeout_ms: 0,
        ..EditorConfig::default()
    });
    editor.set_content("fn main() {\n}\n");
    editor.set_language(crate::Language::Rust);
    let root = editor.current_history_node();
    insert(&mut editor, 1, 0, "    let café = 1;\n");
    insert(&mut editor, 2, 0, "    let tea = café + 1;\n");
    let target = editor.current_history_node();
    let text = editor.content();
    assert!(editor.jump_to_history_node(root));
    let full = editor.state().syntax.full_parses();
    let incremental = editor.state().syntax.incremental_parses();
    assert!(editor.jump_to_history_node(target));
    assert_eq!(editor.content(), text);
    assert_eq!(editor.state().syntax.full_parses(), full);
    assert_eq!(editor.state().syntax.incremental_parses(), incremental + 1);
    assert_fresh_parse(&mut editor);
}

#[cfg(feature = "syntax")]
#[test]
fn a_later_known_edit_cannot_mask_an_earlier_missing_incremental_span() {
    let mut editor = Editor::new(EditorConfig {
        undo_group_timeout_ms: 0,
        ..EditorConfig::default()
    });
    editor.set_content("fn main() {}");
    editor.set_language(crate::Language::Rust);
    let root = editor.current_history_node();
    editor.apply_command(Command::Compound {
        commands: vec![
            Command::Insert {
                position: Position::new(0, 12),
                text: "\n".to_owned(),
            },
            Command::Insert {
                position: Position::new(1, 0),
                text: "fn other() {}".to_owned(),
            },
        ],
    });
    insert(&mut editor, 1, 11, " /* comment */ ");
    let target = editor.current_history_node();
    assert!(editor.jump_to_history_node(root));
    let full = editor.state().syntax.full_parses();
    assert!(editor.jump_to_history_node(target));
    assert_eq!(editor.state().syntax.full_parses(), full + 1);
    assert_fresh_parse(&mut editor);
}
