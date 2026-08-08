//! Tests for the `transform.*` verbs.
//!
//! The transformations themselves are tested in [`crate::text`] against plain
//! strings. What is tested here is the part that only exists once the verb
//! meets a document: which range each caret acts on, what happens to the
//! carets afterwards, and that the whole thing is one undoable step.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::tests::{cursors_at, cursors_with, heads};
use super::*;

use crate::commands::builtin::{
    TRANSFORM_CAMEL_CASE, TRANSFORM_DEDUPE_LINES, TRANSFORM_KEBAB_CASE, TRANSFORM_LOWER_CASE,
    TRANSFORM_PASCAL_CASE, TRANSFORM_REVERSE_LINES, TRANSFORM_SNAKE_CASE, TRANSFORM_SORT_LINES,
    TRANSFORM_SORT_LINES_REVERSE, TRANSFORM_SWAP_CASE, TRANSFORM_TITLE_CASE, TRANSFORM_TOGGLE_CASE,
    TRANSFORM_TRIM_TRAILING_WHITESPACE, TRANSFORM_UPPER_CASE,
};
use crate::editor::CaretScopes;

/// Runs a command by id — the only route the `transform.*` verbs have — and
/// applies whatever it produces. Returns the command, for undo tests.
fn run(id: &str, document: &mut Document, cursor: &mut CursorState) -> Option<Command> {
    let mut handler = KeyboardHandler::new();
    let result = handler
        .run_command(
            id,
            CommandArgs::NONE,
            document,
            cursor,
            &UndoTree::new(),
            &EditorConfig::default(),
            &CaretScopes::none(),
        )
        .expect("the kernel implements this command");
    match result {
        KeyResult::Command(cmd) => {
            cmd.apply(document, cursor).expect("command must apply");
            Some(cmd)
        },
        KeyResult::Handled => None,
        other => panic!("expected a command or Handled, got {other:?}"),
    }
}

// ========== Case verbs: which range each caret acts on ==========

#[test]
fn a_collapsed_caret_transforms_the_word_it_sits_in() {
    // The whole point of the word fallback: Upper Case is useful without
    // selecting first.
    let mut document = Document::new("alpha beta gamma");
    let mut cursor = cursors_at(&[(0, 7)]);

    run(TRANSFORM_UPPER_CASE.as_str(), &mut document, &mut cursor);

    assert_eq!(document.text(), "alpha BETA gamma");
    // The caret keeps its offset into the word it transformed.
    assert_eq!(heads(&cursor), vec![(0, 7)]);
}

#[test]
fn a_caret_off_a_word_changes_nothing() {
    // Sitting on a space must not reach out and mangle a neighbour.
    let mut document = Document::new("alpha beta");
    let mut cursor = cursors_at(&[(0, 5)]);

    let command = run(TRANSFORM_UPPER_CASE.as_str(), &mut document, &mut cursor);

    assert_eq!(document.text(), "alpha beta");
    assert!(
        command.is_none_or(|cmd| !cmd.modifies_content()),
        "a no-op verb must not push a content change onto the undo tree"
    );
}

#[test]
fn a_selection_wins_over_the_word_under_the_caret() {
    let mut document = Document::new("alpha beta gamma");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 10))]);

    run(TRANSFORM_UPPER_CASE.as_str(), &mut document, &mut cursor);

    assert_eq!(document.text(), "ALPHA BETA gamma");
}

#[test]
fn a_transformed_selection_stays_selected_so_verbs_chain() {
    // Select once, then press snake, then upper, then kebab.
    let mut document = Document::new("userIdValue");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 11))]);

    run(TRANSFORM_SNAKE_CASE.as_str(), &mut document, &mut cursor);
    assert_eq!(document.text(), "user_id_value");

    run(TRANSFORM_KEBAB_CASE.as_str(), &mut document, &mut cursor);
    assert_eq!(document.text(), "user-id-value");

    run(TRANSFORM_PASCAL_CASE.as_str(), &mut document, &mut cursor);
    assert_eq!(document.text(), "UserIdValue");
}

#[test]
fn a_reversed_selection_stays_reversed() {
    // Head before anchor: the selection must not silently flip direction, or
    // the next Shift+Left would extend the wrong end.
    let mut document = Document::new("alpha");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 5), Position::new(0, 0))]);

    run(TRANSFORM_UPPER_CASE.as_str(), &mut document, &mut cursor);

    assert_eq!(document.text(), "ALPHA");
    let selection = cursor.primary;
    assert_eq!(
        (selection.anchor.column, selection.head.column),
        (5, 0),
        "the selection reversed"
    );
}

#[test]
fn every_caret_transforms_its_own_word() {
    let mut document = Document::new("alpha\nbeta\ngamma");
    let mut cursor = cursors_at(&[(0, 1), (1, 1), (2, 1)]);

    run(TRANSFORM_UPPER_CASE.as_str(), &mut document, &mut cursor);

    assert_eq!(document.text(), "ALPHA\nBETA\nGAMMA");
    assert_eq!(heads(&cursor), vec![(0, 1), (1, 1), (2, 1)]);
}

#[test]
fn a_length_changing_transform_keeps_every_later_caret_aligned() {
    // snake_case is longer than camelCase, so cursor two's edit starts at a
    // position that has already shifted. Getting this wrong is the classic
    // multi-cursor off-by-N.
    let mut document = Document::new("userId and itemId here");
    let mut cursor = cursors_at(&[(0, 2), (0, 13)]);

    run(TRANSFORM_SNAKE_CASE.as_str(), &mut document, &mut cursor);

    assert_eq!(document.text(), "user_id and item_id here");
}

#[test]
fn each_case_verb_does_what_it_says() {
    for (id, expected) in [
        (TRANSFORM_UPPER_CASE.as_str(), "USERIDVALUE"),
        (TRANSFORM_LOWER_CASE.as_str(), "useridvalue"),
        (TRANSFORM_SNAKE_CASE.as_str(), "user_id_value"),
        (TRANSFORM_KEBAB_CASE.as_str(), "user-id-value"),
        (TRANSFORM_CAMEL_CASE.as_str(), "userIdValue"),
        (TRANSFORM_PASCAL_CASE.as_str(), "UserIdValue"),
        (TRANSFORM_TITLE_CASE.as_str(), "User Id Value"),
        (TRANSFORM_SWAP_CASE.as_str(), "USERiDvALUE"),
    ] {
        let mut document = Document::new("userIdValue");
        let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 11))]);
        run(id, &mut document, &mut cursor);
        assert_eq!(document.text(), expected, "`{id}` produced the wrong text");
    }
}

#[test]
fn toggle_case_cycles_lower_upper_title() {
    let mut document = Document::new("hello world");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 11))]);

    run(TRANSFORM_TOGGLE_CASE.as_str(), &mut document, &mut cursor);
    assert_eq!(document.text(), "HELLO WORLD");

    run(TRANSFORM_TOGGLE_CASE.as_str(), &mut document, &mut cursor);
    assert_eq!(document.text(), "Hello World");

    run(TRANSFORM_TOGGLE_CASE.as_str(), &mut document, &mut cursor);
    assert_eq!(document.text(), "hello world");
}

#[test]
fn title_case_keeps_the_sentences_punctuation() {
    // Unlike the identifier-shaped Title style, this must not eat the comma or
    // collapse the spacing.
    let mut document = Document::new("HELLO, BIG WORLD!");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 17))]);

    run(TRANSFORM_TOGGLE_CASE.as_str(), &mut document, &mut cursor);

    assert_eq!(document.text(), "Hello, Big World!");
}

// ========== Line verbs: which lines, and how they merge ==========

#[test]
fn sorting_covers_exactly_the_lines_the_selection_touches() {
    let mut document = Document::new("keep\ngamma\nalpha\nbeta\nkeep");
    let mut cursor = cursors_with(&[Selection::new(Position::new(1, 2), Position::new(3, 1))]);

    run(TRANSFORM_SORT_LINES.as_str(), &mut document, &mut cursor);

    assert_eq!(document.text(), "keep\nalpha\nbeta\ngamma\nkeep");
}

#[test]
fn a_selection_ending_at_column_zero_has_not_entered_that_line() {
    // Dragging down to the start of a line selects up to it, not into it —
    // matching what the highlight on screen shows.
    //
    // The untouched line sorts *first*, deliberately: with a line that would
    // sort last anyway, including it and excluding it give the same answer and
    // the test proves nothing.
    let mut document = Document::new("gamma\nalpha\naaa");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(2, 0))]);

    run(TRANSFORM_SORT_LINES.as_str(), &mut document, &mut cursor);

    assert_eq!(document.text(), "alpha\ngamma\naaa");
}

#[test]
fn a_collapsed_caret_sorts_its_own_line_alone() {
    // One line sorted is one line: the verb must not quietly widen to the file.
    let mut document = Document::new("gamma\nalpha");
    let mut cursor = cursors_at(&[(0, 0)]);

    let command = run(TRANSFORM_SORT_LINES.as_str(), &mut document, &mut cursor);

    assert_eq!(document.text(), "gamma\nalpha");
    assert!(
        command.is_none_or(|cmd| !cmd.modifies_content()),
        "sorting one line changed the document"
    );
}

#[test]
fn two_carets_inside_one_block_sort_it_once() {
    let mut document = Document::new("gamma\nalpha\nbeta");
    let mut cursor = cursors_with(&[
        Selection::new(Position::new(0, 0), Position::new(2, 4)),
        Selection::collapsed(Position::new(1, 2)),
    ]);

    run(TRANSFORM_SORT_LINES.as_str(), &mut document, &mut cursor);

    assert_eq!(document.text(), "alpha\nbeta\ngamma");
}

#[test]
fn adjacent_blocks_merge_rather_than_sorting_separately() {
    // Two selections on lines 0-1 and 2-3 share no line, but they do share the
    // boundary between them. Sorting them apart would leave that boundary
    // unsorted, which is not what "sort the selected lines" means.
    let mut document = Document::new("d\nc\nb\na\nkeep");
    let mut cursor = cursors_with(&[
        Selection::new(Position::new(0, 0), Position::new(1, 1)),
        Selection::new(Position::new(2, 0), Position::new(3, 1)),
    ]);

    run(TRANSFORM_SORT_LINES.as_str(), &mut document, &mut cursor);

    assert_eq!(document.text(), "a\nb\nc\nd\nkeep");
}

#[test]
fn disjoint_blocks_sort_independently() {
    let mut document = Document::new("d\nc\nkeep\nb\na");
    let mut cursor = cursors_with(&[
        Selection::new(Position::new(0, 0), Position::new(1, 1)),
        Selection::new(Position::new(3, 0), Position::new(4, 1)),
    ]);

    run(TRANSFORM_SORT_LINES.as_str(), &mut document, &mut cursor);

    assert_eq!(document.text(), "c\nd\nkeep\na\nb");
}

#[test]
fn a_block_reaching_the_last_line_keeps_the_file_ending_intact() {
    // The last line carries no trailing ending; a verb that invented one would
    // grow the file by a blank line on every press.
    let mut document = Document::new("keep\ngamma\nalpha");
    let mut cursor = cursors_with(&[Selection::new(Position::new(1, 0), Position::new(2, 5))]);

    run(TRANSFORM_SORT_LINES.as_str(), &mut document, &mut cursor);

    assert_eq!(document.text(), "keep\nalpha\ngamma");
    assert_eq!(document.line_count(), 3);
}

#[test]
fn a_blank_line_at_the_end_of_a_block_is_still_a_line() {
    // The block must run to the *start of the line after it*, not to the end of
    // its own last line's content: those two agree everywhere except when that
    // last line is empty, where stopping at the content silently drops the
    // blank line out of the block entirely.
    let mut document = Document::new("b\na\n\nkeep");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(3, 0))]);

    run(TRANSFORM_SORT_LINES.as_str(), &mut document, &mut cursor);

    // The blank line is line 2, inside the block, and sorts to the front.
    assert_eq!(document.text(), "\na\nb\nkeep");
}

#[test]
fn each_line_verb_does_what_it_says() {
    for (id, expected) in [
        (TRANSFORM_SORT_LINES.as_str(), "a\nb\nb\nc"),
        (TRANSFORM_SORT_LINES_REVERSE.as_str(), "c\nb\nb\na"),
        (TRANSFORM_REVERSE_LINES.as_str(), "b\na\nb\nc"),
        (TRANSFORM_DEDUPE_LINES.as_str(), "c\nb\na"),
    ] {
        let mut document = Document::new("c\nb\na\nb");
        let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(3, 1))]);
        run(id, &mut document, &mut cursor);
        assert_eq!(document.text(), expected, "`{id}` produced the wrong text");
    }
}

#[test]
fn trimming_empties_blank_lines_rather_than_removing_them() {
    let mut document = Document::new("a  \n\t\nb\t");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(2, 2))]);

    run(
        TRANSFORM_TRIM_TRAILING_WHITESPACE.as_str(),
        &mut document,
        &mut cursor,
    );

    assert_eq!(document.text(), "a\n\nb");
    assert_eq!(document.line_count(), 3);
}

// ========== Undo ==========

#[test]
fn a_multi_caret_transform_is_one_undoable_step() {
    let text = "alpha\nbeta\ngamma";
    let mut document = Document::new(text);
    let mut cursor = cursors_at(&[(0, 1), (1, 1), (2, 1)]);
    let before = heads(&cursor);

    let command = run(TRANSFORM_UPPER_CASE.as_str(), &mut document, &mut cursor)
        .expect("the transform produced a command");
    assert_eq!(document.text(), "ALPHA\nBETA\nGAMMA");

    command
        .inverse()
        .apply(&mut document, &mut cursor)
        .expect("inverse must apply");

    assert_eq!(document.text(), text);
    assert_eq!(heads(&cursor), before);
}

#[test]
fn a_line_verb_is_one_undoable_step() {
    let text = "gamma\nalpha\nbeta";
    let mut document = Document::new(text);
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(2, 4))]);

    let command = run(TRANSFORM_SORT_LINES.as_str(), &mut document, &mut cursor)
        .expect("the sort produced a command");
    assert_eq!(document.text(), "alpha\nbeta\ngamma");

    command
        .inverse()
        .apply(&mut document, &mut cursor)
        .expect("inverse must apply");

    assert_eq!(document.text(), text);
}

// ========== Idempotence ==========

#[test]
fn running_a_verb_twice_pushes_nothing_the_second_time() {
    // Pressing Upper Case on already-uppercase text must not fill the undo
    // tree with entries that undo nothing.
    let mut document = Document::new("ALPHA");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 5))]);

    let command = run(TRANSFORM_UPPER_CASE.as_str(), &mut document, &mut cursor);

    assert_eq!(document.text(), "ALPHA");
    assert!(
        command.is_none_or(|cmd| !cmd.modifies_content()),
        "an already-transformed selection produced a content change"
    );
}
