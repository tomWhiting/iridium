//! A fold stays on its header as the text around it changes.
//!
//! Folds are remembered by the line their header is on. These drive real
//! edits through the editor — paste, Enter, delete, undo — and check that the
//! fold is still on the same function afterwards, and that no other block
//! became folded by taking over its old line number.
#![cfg(feature = "syntax")]

use super::Editor;
use crate::document::Position;
use crate::input::{KeyCode, KeyEvent};
use iridium_lang::Language;

/// Two functions with bodies, the second starting on line 4.
const TWO_FUNCTIONS: &str = "fn first() {\n    let a = 1;\n    let b = 2;\n}\nfn second() {\n    let c = 3;\n    let d = 4;\n}\n";

/// An editor on [`TWO_FUNCTIONS`] with the second function folded.
fn second_folded() -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content(TWO_FUNCTIONS);
    editor.set_language(Language::Rust);
    editor.settle_syntax();
    assert!(editor.fold_at(4), "fn second() should fold");
    editor
}

/// The line `text` is on, which must be exactly one.
fn line_of(editor: &Editor, text: &str) -> usize {
    let document = &editor.state().document;
    let lines: Vec<usize> = (0..document.line_count())
        .filter(|&line| document.line(line).is_some_and(|line| line == text))
        .collect();
    assert_eq!(lines.len(), 1, "{text:?} is on lines {lines:?}");
    lines[0]
}

/// The folded lines, sorted.
fn folded(editor: &Editor) -> Vec<usize> {
    let mut lines: Vec<usize> = editor.state().fold_state.folded_lines().collect();
    lines.sort_unstable();
    lines
}

#[test]
fn lines_pasted_above_a_fold_move_it_down_with_its_header() {
    let mut editor = second_folded();
    editor.set_cursor(Position::new(0, 0));
    editor.paste("// one\n// two\n// three\n");
    editor.settle_syntax();

    let header = line_of(&editor, "fn second() {");
    assert_eq!(header, 7);
    assert_eq!(
        folded(&editor),
        vec![header],
        "the fold followed its header"
    );
    assert!(editor.state().fold_state.is_line_hidden(header + 1));
    assert!(
        !editor
            .state()
            .fold_state
            .is_folded(line_of(&editor, "fn first() {")),
        "the function that moved onto line 4 is not folded"
    );
}

#[test]
fn enter_at_the_start_of_the_header_pushes_the_fold_down() {
    let mut editor = second_folded();
    editor.set_cursor(Position::new(4, 0));
    editor.paste("\n");
    editor.settle_syntax();
    assert_eq!(folded(&editor), vec![5]);
    assert_eq!(line_of(&editor, "fn second() {"), 5);
}

#[test]
fn typing_after_the_start_of_the_header_leaves_the_fold_where_it_is() {
    let mut editor = second_folded();
    editor.set_cursor(Position::new(4, 3));
    editor.paste("x");
    editor.settle_syntax();
    assert_eq!(folded(&editor), vec![4]);
    assert_eq!(line_of(&editor, "fn xsecond() {"), 4);
}

#[test]
fn deleting_lines_above_a_fold_moves_it_up() {
    let mut editor = second_folded();
    // Select the whole first function and delete it.
    editor.set_selection(Position::new(0, 0), Position::new(4, 0));
    editor.handle_key(&KeyEvent::simple(KeyCode::Backspace));
    editor.settle_syntax();
    assert_eq!(line_of(&editor, "fn second() {"), 0);
    assert_eq!(folded(&editor), vec![0]);
}

#[test]
fn undoing_the_paste_brings_the_fold_back_up() {
    let mut editor = second_folded();
    editor.set_cursor(Position::new(0, 0));
    editor.paste("// above\n");
    editor.settle_syntax();
    assert_eq!(folded(&editor), vec![5]);
    assert!(editor.undo());
    editor.settle_syntax();
    assert_eq!(folded(&editor), vec![4]);
    assert_eq!(line_of(&editor, "fn second() {"), 4);
}
