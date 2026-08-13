//! Multi-cursor: the chords that make carets, the ones that take them away,
//! and the regressions two reviews found.
//!
//! # One file per subject
//!
//! ⚠️ This was a single 1,122-line file against a 1,000-line hard limit (#92),
//! split on the `// ========== … ==========` rules it already carried. The nine
//! rules were grouped into four files by subject rather than one file each —
//! three of them were under forty lines — but no test moved past a neighbour,
//! so the reading order is unchanged.
//!
//! What stays here is the harness, the six chords and the sticky-column
//! fixture: everything more than one of the files below needs.
//!
//! # ⛔ A file missing from the list below is silently ignored
//!
//! `cargo` reads this list, not the directory: an orphaned `.rs` beside it
//! compiles clean, takes its tests with it, and leaves the suite green — none
//! of the ten gates reports it. Measured, with numbers, in
//! `docs/CODING_STANDARDS.md`. Take a test count either side of any add,
//! remove or rename here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod adding;
mod history;
mod narrowing;
mod regressions;

use super::tests::{cursors_at, cursors_with, heads};
use super::*;
use crate::editor::CaretScopes;

/// All cursor selections in `all_selections` order, as
/// `((anchor.line, anchor.column), (head.line, head.column))` pairs.
fn selections(cursor: &CursorState) -> Vec<((usize, usize), (usize, usize))> {
    cursor
        .all_selections()
        .map(|sel| {
            (
                (sel.anchor.line, sel.anchor.column),
                (sel.head.line, sel.head.column),
            )
        })
        .collect()
}

/// Handles a key event and applies the resulting command (if any), returning
/// the command for undo assertions. `None` means a no-op (Handled).
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

/// Calls the skip verb directly (it has no key binding) and applies the
/// resulting command, returning it. `None` means a no-op (Handled).
fn skip(
    handler: &mut KeyboardHandler,
    document: &mut Document,
    cursor: &mut CursorState,
) -> Option<Command> {
    let result = handler.skip_last_added_occurrence(document, cursor);
    match result {
        KeyResult::Command(cmd) => {
            cmd.apply(document, cursor).expect("command must apply");
            Some(cmd)
        },
        KeyResult::Handled => None,
        other => panic!("expected a command or Handled, got {other:?}"),
    }
}

const CTRL: Modifiers = Modifiers {
    shift: false,
    ctrl: true,
    alt: false,
    meta: false,
    alt_graph: false,
};
const CTRL_ALT: Modifiers = Modifiers {
    shift: false,
    ctrl: true,
    alt: true,
    meta: false,
    alt_graph: false,
};
const CTRL_SHIFT: Modifiers = Modifiers {
    shift: true,
    ctrl: true,
    alt: false,
    meta: false,
    alt_graph: false,
};

const ADD_ABOVE: KeyEvent = KeyEvent::new(KeyCode::Up, CTRL_ALT);
const ADD_BELOW: KeyEvent = KeyEvent::new(KeyCode::Down, CTRL_ALT);
const ADD_NEXT: KeyEvent = KeyEvent::new(KeyCode::Char('d'), CTRL);
const SELECT_ALL_OCC: KeyEvent = KeyEvent::new(KeyCode::Char('l'), CTRL_SHIFT);
const UNDO_CURSOR: KeyEvent = KeyEvent::new(KeyCode::Char('u'), CTRL);
const ESCAPE: KeyEvent = KeyEvent::simple(KeyCode::Escape);

/// Lines of length 10 / 2 / 10 for exercising sticky columns across a short
/// middle line.
const STICKY_DOC: &str = "aaaaaaaaaa\nbb\ncccccccccc";
