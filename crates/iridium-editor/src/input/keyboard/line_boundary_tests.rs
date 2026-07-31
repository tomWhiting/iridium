//! Tests for `edit.deleteToLineStart` and `edit.deleteToLineEnd`.
//!
//! These two verbs exist because the web face had implemented them by hand,
//! against the primary cursor alone, outside the kernel — so on macOS
//! `Cmd+Backspace` with four carets deleted one line's prefix and silently
//! collapsed the other three. Nothing compiled that code on a native target,
//! so nothing could test it. The tests below are therefore mostly about the
//! *other* carets: that every one of them acts, and that all of them survive.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::tests::{cursors_at, cursors_with, heads};
use super::*;

use crate::commands::builtin::{EDIT_DELETE_TO_LINE_END, EDIT_DELETE_TO_LINE_START};

/// Runs a command by id — the only route these verbs have, since the
/// platform-neutral default keymap deliberately leaves them unbound — and
/// applies whatever it produces.
fn run(id: &str, document: &mut Document, cursor: &mut CursorState) {
    let mut handler = KeyboardHandler::new();
    let result = handler
        .run_command(
            id,
            CommandArgs::NONE,
            document,
            cursor,
            &UndoTree::new(),
            &EditorConfig::default(),
        )
        .expect("the kernel implements this command");
    match result {
        KeyResult::Command(cmd) => {
            cmd.apply(document, cursor).expect("command must apply");
        },
        KeyResult::Handled => {},
        other => panic!("expected a command or Handled, got {other:?}"),
    }
}

#[test]
fn delete_to_line_start_acts_on_every_caret() {
    let mut document = Document::new("alpha beta\ngamma delta\nepsilon zeta");
    let mut cursor = cursors_at(&[(0, 6), (1, 6), (2, 8)]);

    run(
        EDIT_DELETE_TO_LINE_START.as_str(),
        &mut document,
        &mut cursor,
    );

    assert_eq!(document.text(), "beta\ndelta\nzeta");
    // Every caret lands at its own line start, and all three still exist.
    assert_eq!(heads(&cursor), vec![(0, 0), (1, 0), (2, 0)]);
}

#[test]
fn delete_to_line_end_acts_on_every_caret() {
    let mut document = Document::new("alpha beta\ngamma delta\nepsilon zeta");
    let mut cursor = cursors_at(&[(0, 5), (1, 5), (2, 7)]);

    run(EDIT_DELETE_TO_LINE_END.as_str(), &mut document, &mut cursor);

    assert_eq!(document.text(), "alpha\ngamma\nepsilon");
    assert_eq!(heads(&cursor), vec![(0, 5), (1, 5), (2, 7)]);
}

#[test]
fn delete_to_line_end_empties_the_line_without_joining_it() {
    // The distinction from `lines.join`: the line ending survives, so three
    // lines stay three lines.
    let mut document = Document::new("alpha\nbeta\ngamma");
    let mut cursor = cursors_at(&[(1, 0)]);

    run(EDIT_DELETE_TO_LINE_END.as_str(), &mut document, &mut cursor);

    assert_eq!(document.text(), "alpha\n\ngamma");
    assert_eq!(document.line_count(), 3);
}

#[test]
fn a_selection_is_deleted_instead_of_the_line_prefix() {
    // Consistency with every other delete verb: a caret that owns a selection
    // deletes that selection, not the text behind it.
    let mut document = Document::new("alpha beta gamma");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 6), Position::new(0, 10))]);

    run(
        EDIT_DELETE_TO_LINE_START.as_str(),
        &mut document,
        &mut cursor,
    );

    assert_eq!(document.text(), "alpha  gamma");
    assert_eq!(heads(&cursor), vec![(0, 6)]);
}

#[test]
fn mixed_carets_each_do_their_own_thing() {
    // One caret with a selection, one collapsed: the selection goes, and the
    // collapsed caret still deletes its own line prefix. Neither disturbs the
    // other, and both survive.
    //
    // The selection deliberately does *not* start at column zero, so deleting
    // the selection and deleting that caret's line prefix give different
    // answers — otherwise this test would pass against an implementation that
    // ignores selections entirely.
    let mut document = Document::new("alpha beta\ngamma delta");
    let mut cursor = cursors_with(&[
        Selection::new(Position::new(0, 6), Position::new(0, 10)),
        Selection::collapsed(Position::new(1, 6)),
    ]);

    run(
        EDIT_DELETE_TO_LINE_START.as_str(),
        &mut document,
        &mut cursor,
    );

    assert_eq!(document.text(), "alpha \ndelta");
    assert_eq!(heads(&cursor), vec![(0, 6), (1, 0)]);
}

#[test]
fn two_carets_on_one_line_remove_that_prefix_once() {
    // Overlapping deletes are clamped, so the longer one wins and the two
    // carets converge rather than double-deleting or corrupting offsets.
    let mut document = Document::new("alpha beta gamma");
    let mut cursor = cursors_at(&[(0, 6), (0, 11)]);

    run(
        EDIT_DELETE_TO_LINE_START.as_str(),
        &mut document,
        &mut cursor,
    );

    assert_eq!(document.text(), "gamma");
    assert_eq!(heads(&cursor), vec![(0, 0)]);
}

#[test]
fn a_caret_already_at_the_line_start_leaves_its_line_alone() {
    // The no-op caret must not stop the others, and must not vanish.
    let mut document = Document::new("alpha\nbeta gamma");
    let mut cursor = cursors_at(&[(0, 0), (1, 5)]);

    run(
        EDIT_DELETE_TO_LINE_START.as_str(),
        &mut document,
        &mut cursor,
    );

    assert_eq!(document.text(), "alpha\ngamma");
    assert_eq!(heads(&cursor), vec![(0, 0), (1, 0)]);
}

#[test]
fn a_caret_already_at_the_line_end_leaves_its_line_alone() {
    let mut document = Document::new("alpha\nbeta gamma");
    let mut cursor = cursors_at(&[(0, 5), (1, 4)]);

    run(EDIT_DELETE_TO_LINE_END.as_str(), &mut document, &mut cursor);

    assert_eq!(document.text(), "alpha\nbeta");
    assert_eq!(heads(&cursor), vec![(0, 5), (1, 4)]);
}

#[test]
fn every_caret_is_restored_by_a_single_undo() {
    // The whole edit is one reversible command carrying both text and cursor
    // state, so undoing it brings back all three carets, not just the primary.
    let text = "alpha beta\ngamma delta\nepsilon zeta";
    let mut document = Document::new(text);
    let mut cursor = cursors_at(&[(0, 6), (1, 6), (2, 8)]);
    let before = heads(&cursor);

    let mut handler = KeyboardHandler::new();
    let result = handler
        .run_command(
            EDIT_DELETE_TO_LINE_START.as_str(),
            CommandArgs::NONE,
            &document,
            &cursor,
            &UndoTree::new(),
            &EditorConfig::default(),
        )
        .expect("the kernel implements this command");
    let KeyResult::Command(command) = result else {
        panic!("expected a command, got {result:?}");
    };
    command
        .apply(&mut document, &mut cursor)
        .expect("command must apply");
    command
        .inverse()
        .apply(&mut document, &mut cursor)
        .expect("inverse must apply");

    assert_eq!(document.text(), text);
    assert_eq!(heads(&cursor), before);
}

#[test]
fn multi_byte_text_is_measured_in_columns_not_bytes() {
    // A caret past a multi-byte character must delete exactly the characters
    // behind it — a byte-indexed implementation would slice mid-codepoint or
    // leave a fragment behind.
    let mut document = Document::new("héllo wörld\nnaïve café");
    let mut cursor = cursors_at(&[(0, 6), (1, 6)]);

    run(
        EDIT_DELETE_TO_LINE_START.as_str(),
        &mut document,
        &mut cursor,
    );

    assert_eq!(document.text(), "wörld\ncafé");
    assert_eq!(heads(&cursor), vec![(0, 0), (1, 0)]);
}

#[test]
fn multi_byte_lines_are_deleted_to_their_true_end() {
    // Each line here is longer in bytes than in columns. Note what this does
    // and does not prove: an implementation that measured the line in *bytes*
    // would still pass, because `clamp_edit_ranges` pulls an over-long column
    // back to the real line end. What it catches is the other direction — any
    // end that stops short and leaves a fragment behind — and that the
    // multi-byte characters survive the round trip intact.
    let mut document = Document::new("héllo wörld\nnaïve café");
    let mut cursor = cursors_at(&[(0, 5), (1, 5)]);

    run(EDIT_DELETE_TO_LINE_END.as_str(), &mut document, &mut cursor);

    assert_eq!(document.text(), "héllo\nnaïve");
    assert_eq!(heads(&cursor), vec![(0, 5), (1, 5)]);
}
