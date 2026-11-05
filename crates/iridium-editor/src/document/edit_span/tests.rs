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

// ===== compounds whose parts address the document their predecessors left =====

/// Applies `command` to `before` and checks `span` is a true description of
/// what changed: the text before its start and after its ends is untouched.
fn assert_describes(before: &str, command: &Command, span: &EditSpan) {
    let mut document = doc(before);
    let mut cursor = crate::document::CursorState::at(Position::zero());
    command
        .apply(&mut document, &mut cursor)
        .expect("the command applies");
    let after = document.text();
    assert!(
        span.start_byte <= span.old_end_byte && span.start_byte <= span.new_end_byte,
        "{span:?} is inverted"
    );
    assert_eq!(
        before.get(..span.start_byte),
        after.get(..span.start_byte),
        "{span:?} claims the text before it is untouched by {command:?}"
    );
    assert_eq!(
        before.get(span.old_end_byte..),
        after.get(span.new_end_byte..),
        "{span:?} claims the text after it is untouched by {command:?}"
    );
}

#[test]
fn undoing_a_multi_cursor_insert_reports_what_it_changed() {
    // Two carets typing "x", emitted last caret first; then its inverse, which
    // reverses the order and so addresses the document between the two parts.
    let before = "abcdef\n";
    let typed = Command::Compound {
        commands: vec![insert(0, 4, "x"), insert(0, 1, "x")],
    };
    let mut document = doc(before);
    let mut cursor = crate::document::CursorState::at(Position::zero());
    typed
        .apply(&mut document, &mut cursor)
        .expect("the insert applies");
    let typed_text = document.text();
    assert_eq!(typed_text, "axbcdxef\n");

    let undo = typed.inverse();
    let span = compute_edit_span(&document, &undo)
        .expect("the undo resolves")
        .expect("the undo edits content");
    assert_describes(&typed_text, &undo, &span);
}

#[test]
fn a_later_part_landing_before_an_earlier_ones_end_is_reported_whole() {
    // Delete the first five bytes, then insert where the deletion left the
    // fifth byte of what remains: the extremes of the parts' own coordinates
    // would describe an edit that did not happen.
    let before = "0123456789\n";
    let command = Command::Compound {
        commands: vec![delete((0, 0), (0, 5), "01234"), insert(0, 4, "X")],
    };
    let span = compute_edit_span(&doc(before), &command)
        .expect("the command resolves")
        .expect("the command edits content");
    assert_describes(before, &command, &span);
}

#[test]
fn random_sequential_compounds_are_reported_truly() {
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let mut next = |bound: usize| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        usize::try_from(state % bound.max(1) as u64).unwrap_or(0)
    };
    let pieces = ["x", "yz", "\n", "é", "{\n}", ""];

    for round in 0..400 {
        let before = "fn main() {\n    let a = 1;\n    café();\n}\n// tail\n".to_owned();
        let mut document = doc(&before);
        let mut cursor = crate::document::CursorState::at(Position::zero());
        let mut parts = Vec::new();
        for _ in 0..2 + next(4) {
            let text = document.text();
            let mut at = next(text.len() + 1);
            while !text.is_char_boundary(at) {
                at -= 1;
            }
            let mut end = (at + next(6)).min(text.len());
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            let start = document.offset_to_position(at).expect("in bounds");
            let stop = document.offset_to_position(end).expect("in bounds");
            let removed = text.get(at..end).unwrap_or_default().to_owned();
            let inserted = pieces[next(pieces.len())].to_owned();
            let part = match round % 3 {
                0 => Command::Insert {
                    position: start,
                    text: inserted,
                },
                1 => Command::Delete {
                    range: Range::new(start, stop),
                    deleted_text: removed,
                },
                _ => Command::Replace {
                    range: Range::new(start, stop),
                    old_text: removed,
                    new_text: inserted,
                },
            };
            part.apply(&mut document, &mut cursor)
                .expect("each part addresses the document the last one left");
            parts.push(part);
        }
        let command = Command::Compound { commands: parts };
        let span = compute_edit_span(&doc(&before), &command)
            .expect("the command resolves")
            .expect("the command edits content");
        assert_describes(&before, &command, &span);
        // And the inverse, as an undo replays it.
        let after = document.text();
        let undo = command.inverse();
        let span = compute_edit_span(&doc(&after), &undo)
            .expect("the undo resolves")
            .expect("the undo edits content");
        assert_describes(&after, &undo, &span);
    }
}
