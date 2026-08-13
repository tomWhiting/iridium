//! Tests for the keyboard handler, including document-state-level
//! multi-cursor editing tests that apply the produced commands to a real
//! document and assert both the resulting text and every cursor position.
//!
//! # One file per subject
//!
//! ⚠️ This was a single 1,142-line file against a 1,000-line hard limit (#92),
//! split on the `// ========== … ==========` rules it already carried, so no
//! test moved between neighbours.
//!
//! What stays here is the harness and the three cursor builders. [`cursors_at`],
//! [`cursors_with`] and [`heads`] are `pub(super)` because **nine** sibling
//! modules import them, which is why they cannot live in one of the files
//! below.
//!
//! # ⛔ A file missing from the list below is silently ignored
//!
//! `cargo` reads this list, not the directory: an orphaned `.rs` beside it
//! compiles clean, takes its tests with it, and leaves the suite green — none
//! of the ten gates reports it. Measured, with numbers, in
//! `docs/CODING_STANDARDS.md`. Take a test count either side of any add,
//! remove or rename here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod clipboard;
mod editing;
mod navigation;
mod single_cursor;
mod undo;

use super::*;
use crate::editor::CaretScopes;

fn create_test_document() -> Document {
    Document::new("Hello World\nSecond Line\nThird Line")
}

/// Builds a multi-cursor state from collapsed positions (first is primary).
pub(super) fn cursors_at(positions: &[(usize, usize)]) -> CursorState {
    let (first, rest) = positions
        .split_first()
        .expect("at least one cursor position");
    let mut state = CursorState::at(Position::new(first.0, first.1));
    for &(line, column) in rest {
        state.add_cursor(Selection::collapsed(Position::new(line, column)));
    }
    state
}

/// Builds a multi-cursor state from selections (first is primary).
pub(super) fn cursors_with(selections: &[Selection]) -> CursorState {
    let (first, rest) = selections.split_first().expect("at least one selection");
    let mut state = CursorState::new(*first);
    for sel in rest {
        state.add_cursor(*sel);
    }
    state
}

/// All cursor head positions in `all_selections` order.
pub(super) fn heads(cursor: &CursorState) -> Vec<(usize, usize)> {
    cursor
        .all_selections()
        .map(|sel| (sel.head.line, sel.head.column))
        .collect()
}

/// Handles a key event and applies the resulting command (if any) to the
/// document and cursor, returning the command for undo tests.
fn press(
    handler: &mut KeyboardHandler,
    event: &KeyEvent,
    document: &mut Document,
    cursor: &mut CursorState,
) -> Option<Command> {
    let result = handler.handle_key(
        event,
        document,
        cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
        &CaretScopes::none(),
    );
    match result {
        KeyResult::Command(cmd) => {
            cmd.apply(document, cursor).expect("command must apply");
            Some(cmd)
        },
        KeyResult::Handled => None,
        other => panic!("expected a command or Handled, got {other:?}"),
    }
}
