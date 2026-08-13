//! Undoing a multi-cursor edit restores every caret, not just the primary.
//!
//! Split out of a 1,142-line `tests.rs` for #92, on the rule the file
//! already carried. The harness and the cursor builders are in [`super`].

use super::*;

// ========== Undo of multi-cursor edits ==========

#[test]
fn undo_multi_cursor_edit_restores_text_and_cursors() {
    let mut doc = Document::new("abc def");
    let original_cursor = cursors_at(&[(0, 0), (0, 4)]);
    let mut cursor = original_cursor.clone();
    let mut handler = KeyboardHandler::new();

    let cmd = press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Char('x')),
        &mut doc,
        &mut cursor,
    )
    .expect("typing must produce a command");

    assert_eq!(doc.text(), "xabc xdef");
    assert_eq!(heads(&cursor), vec![(0, 1), (0, 6)]);

    cmd.inverse()
        .apply(&mut doc, &mut cursor)
        .expect("inverse command must apply");

    assert_eq!(doc.text(), "abc def");
    assert_eq!(cursor, original_cursor);
}

#[test]
fn undo_multi_cursor_backspace_restores_text_and_cursors() {
    let mut doc = Document::new("abc def ghi");
    let original_cursor = cursors_at(&[(0, 3), (0, 7), (0, 11)]);
    let mut cursor = original_cursor.clone();
    let mut handler = KeyboardHandler::new();

    let cmd = press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Backspace),
        &mut doc,
        &mut cursor,
    )
    .expect("backspace must produce a command");

    assert_eq!(doc.text(), "ab de gh");

    cmd.inverse()
        .apply(&mut doc, &mut cursor)
        .expect("inverse command must apply");

    assert_eq!(doc.text(), "abc def ghi");
    assert_eq!(cursor, original_cursor);
}

/// The early break in [`KeyboardHandler::apply_repeated_motion`], guarded by
/// counting calls rather than by timing them.
///
/// ⚠️ **Measured, not assumed.** The behavioural test in the modal keymap suite
/// — a count of 900,000,000 landing at the end of a three-character line —
/// still *passes* with the break removed. It takes **174 seconds** instead of
/// microseconds, but it passes, so it guards the clamping and nothing else. A
/// test that can only pass proves nothing about the thing it claims to guard.
///
/// A wall-clock assertion would close the gap and open a worse one: a timing
/// test on a loaded machine is a test that fails for reasons that have nothing
/// to do with the code. Counting the calls is exact, deterministic and costs
/// nothing.
#[test]
fn a_repeated_motion_stops_calling_the_motion_once_it_stops_moving() {
    use std::cell::Cell;

    let document = Document::new("abc");
    let cursor = cursors_at(&[(0, 0)]);
    let calls = Cell::new(0_u32);

    KeyboardHandler::apply_repeated_motion(&cursor, false, 1_000_000, |head| {
        calls.set(calls.get() + 1);
        motions::char_right(&document, head)
    });

    // Three moves reach the end of the line, and the fourth call is the one that
    // returns its own argument and ends the loop.
    assert_eq!(
        calls.get(),
        4,
        "the loop ran past the boundary instead of stopping at it"
    );
}

#[test]
fn a_repeated_motion_that_keeps_moving_runs_exactly_the_count() {
    use std::cell::Cell;

    let document = Document::new("abcdefghij");
    let cursor = cursors_at(&[(0, 0)]);
    let calls = Cell::new(0_u32);

    let result = KeyboardHandler::apply_repeated_motion(&cursor, false, 3, |head| {
        calls.set(calls.get() + 1);
        motions::char_right(&document, head)
    });

    assert_eq!(calls.get(), 3);
    assert!(
        matches!(result, KeyResult::Command(_)),
        "a motion that moved must emit exactly one command"
    );
}
