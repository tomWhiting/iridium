//! The keystroke path with no grammar.
//!
//! The configuration the browser ships. There is no tree, so folds come off a
//! brace scan of the document text — and that scan read every byte of it for
//! every keystroke. These go through `Editor::apply_command` for the same reason
//! the tree-sitter ones do: the cost was only ever reachable from there.

use crate::editor::{Editor, EditorConfig};
use crate::{Command, Language, Position};

/// A document of `blocks` three-line functions, loaded with folds built.
fn editor_for(blocks: usize) -> Editor {
    use std::fmt::Write as _;

    let mut source = String::new();
    for index in 0..blocks {
        // Writing into a `String` is infallible.
        let _ = write!(source, "fn f{index}() {{\n    body();\n}}\n");
    }

    let mut editor = Editor::new(EditorConfig::default());
    editor.set_content(&source);
    editor.set_language(Language::Rust);
    editor
}

/// How many lines the brace scanner reads for one typed character.
fn lines_for_one_keystroke(blocks: usize) -> u64 {
    let mut editor = editor_for(blocks);

    // Type once first, so the measured keystroke is a steady-state one.
    editor.apply_command(Command::Insert {
        position: Position::new(1, 8),
        text: "a".to_owned(),
    });
    let before = editor.state().fold_state.fold_lines_scanned();

    editor.apply_command(Command::Insert {
        position: Position::new(1, 8),
        text: "b".to_owned(),
    });
    editor.state().fold_state.fold_lines_scanned() - before
}

/// The bug, stated as a test.
#[test]
fn typing_does_not_read_the_whole_document() {
    let small = lines_for_one_keystroke(20);
    let large = lines_for_one_keystroke(2_000);

    assert_eq!(
        small, large,
        "one keystroke read {small} lines of a 60-line document and {large} \
         of a 6,000-line one; the cost of typing must not depend on how much \
         is already typed"
    );
}

/// Typing must leave the folds exactly where a fresh load would.
///
/// Real typing, including newlines, which move every fold after them.
#[test]
fn folds_after_typing_match_a_fresh_load() {
    let mut editor = editor_for(6);

    for (step, text) in ["x", "\n", "    let extra = 3;", "\n", "}", "{"]
        .into_iter()
        .cycle()
        .take(24)
        .enumerate()
    {
        editor.apply_command(Command::Insert {
            position: Position::new(1, 4),
            text: text.to_owned(),
        });

        let mut oracle = Editor::new(EditorConfig::default());
        oracle.set_content(&editor.content());
        oracle.set_language(Language::Rust);

        assert_eq!(
            editor.state().fold_state.regions(),
            oracle.state().fold_state.regions(),
            "step {step}: the editor's folds drifted from a fresh scan of \
             its own text"
        );
    }
}
