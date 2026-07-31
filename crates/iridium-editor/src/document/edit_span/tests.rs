//! Tests for edit-span extraction.
//!
//! These moved here with the code, from the wasm bindings crate. That is the
//! point of the move: the kernel's retained syntax tree and the web face both
//! ask "which bytes did that command change?", and the answer must be one
//! implementation with one set of tests rather than two that agree until they
//! do not.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::document::{Position, Range};

fn doc(text: &str) -> Document {
    Document::new(text)
}

fn insert(line: usize, column: usize, text: &str) -> Command {
    Command::Insert {
        position: Position::new(line, column),
        text: text.to_string(),
    }
}

fn delete(start: (usize, usize), end: (usize, usize), deleted: &str) -> Command {
    Command::Delete {
        range: Range::new(Position::new(start.0, start.1), Position::new(end.0, end.1)),
        deleted_text: deleted.to_string(),
    }
}

// ===== byte_point =====

#[test]
fn byte_point_ascii() {
    let d = doc("hello\nworld");
    assert_eq!(byte_point(&d, 0), Some((0, 0)));
    assert_eq!(byte_point(&d, 5), Some((0, 5)));
    assert_eq!(byte_point(&d, 6), Some((1, 0)));
    assert_eq!(byte_point(&d, 11), Some((1, 5)));
}

#[test]
fn byte_point_returns_byte_columns_for_multibyte_text() {
    // 'é' is 2 bytes in UTF-8.
    let d = doc("aé\nb");
    assert_eq!(byte_point(&d, 3), Some((0, 3))); // after 'é' (byte column)
    assert_eq!(byte_point(&d, 4), Some((1, 0)));
}

#[test]
fn byte_point_out_of_bounds_is_none() {
    let d = doc("ab");
    assert_eq!(byte_point(&d, 3), None);
}

// ===== compute_edit_span =====

#[test]
fn span_for_insert() {
    let d = doc("hello\nworld");
    let span = compute_edit_span(&d, &insert(1, 0, "xy"))
        .expect("valid command")
        .expect("content edit");
    assert_eq!(
        span,
        EditSpan {
            start_byte: 6,
            old_end_byte: 6,
            new_end_byte: 8,
            old_end_row: 1,
            old_end_column: 0,
        }
    );
}

#[test]
fn span_for_delete() {
    let d = doc("hello\nworld");
    let span = compute_edit_span(&d, &delete((0, 1), (0, 3), "el"))
        .expect("valid command")
        .expect("content edit");
    assert_eq!(
        span,
        EditSpan {
            start_byte: 1,
            old_end_byte: 3,
            new_end_byte: 1,
            old_end_row: 0,
            old_end_column: 3,
        }
    );
}

#[test]
fn span_for_replace() {
    let d = doc("hello");
    let cmd = Command::Replace {
        range: Range::new(Position::new(0, 1), Position::new(0, 3)),
        old_text: "el".to_string(),
        new_text: "ELLO".to_string(),
    };
    let span = compute_edit_span(&d, &cmd)
        .expect("valid command")
        .expect("content edit");
    assert_eq!(
        span,
        EditSpan {
            start_byte: 1,
            old_end_byte: 3,
            new_end_byte: 5,
            old_end_row: 0,
            old_end_column: 3,
        }
    );
}

#[test]
fn span_for_selection_only_command_is_none() {
    let d = doc("hello");
    let cmd = Command::Compound {
        commands: Vec::new(),
    };
    assert_eq!(compute_edit_span(&d, &cmd), Ok(None));
}

#[test]
fn span_for_multi_cursor_compound_covers_all_edits() {
    // Two cursors inserting "ab" at (0,0) and (1,0) — the core emits the
    // commands in reverse document order with pre-edit coordinates.
    let d = doc("one\ntwo");
    let cmd = Command::Compound {
        commands: vec![insert(1, 0, "ab"), insert(0, 0, "ab")],
    };
    let span = compute_edit_span(&d, &cmd)
        .expect("valid command")
        .expect("content edit");
    // start = min(0, 4) = 0; old_end = max(0, 4) = 4; delta = +4.
    assert_eq!(
        span,
        EditSpan {
            start_byte: 0,
            old_end_byte: 4,
            new_end_byte: 8,
            old_end_row: 1,
            old_end_column: 0,
        }
    );
}

#[test]
fn span_with_invalid_position_is_err() {
    let d = doc("hi");
    assert_eq!(
        compute_edit_span(&d, &insert(5, 0, "x")),
        Err(EditSpanError)
    );
}
