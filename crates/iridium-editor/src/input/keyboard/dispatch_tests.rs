//! Tests for keymap-driven dispatch.
//!
//! Three things are checked here that no other test module can:
//!
//! - **the command table is exhaustive** — every registered built-in has an
//!   implementation and every implementation is a registered built-in, so a
//!   command can never be listed in a palette while doing nothing;
//! - **bindings are data** — a user keymap layer rebinds, unbinds, and adds
//!   multi-key sequences without this crate changing;
//! - **the round-trip trap stays closed** — a selection change that returns to a
//!   byte-identical cursor state must still invalidate the sticky columns and the
//!   multi-cursor addition-order stack. That defect has been reintroduced twice
//!   in this codebase, both times by moving *where* the invalidation happens;
//!   routing dispatch through the registry moved it again, so it is pinned here
//!   explicitly rather than left to the verb-specific suites.
//!
//! Every behavioural test drives the real handler and applies the produced
//! command to a real [`Document`], then asserts the resulting text and every
//! cursor position.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::actions::{IMPLEMENTED_COMMAND_COUNT, action_for, implemented_ids};
use super::tests::{cursors_at, heads};
use super::*;
use crate::commands::builtin::{
    BUILTIN_COMMAND_COUNT, SELECTION_SELECT_ALL, builtin_commands, builtin_registry,
};
use crate::commands::{
    CommandArgs, CommandId, KeyBinding, ModifierPattern, ModifierState, StrokePattern,
};

const CTRL: Modifiers = Modifiers {
    shift: false,
    ctrl: true,
    alt: false,
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
const CTRL_ALT: Modifiers = Modifiers {
    shift: false,
    ctrl: true,
    alt: true,
    meta: false,
    alt_graph: false,
};
const CTRL_ALT_GRAPH: Modifiers = Modifiers {
    shift: false,
    ctrl: true,
    alt: true,
    meta: false,
    alt_graph: true,
};

const ADD_NEXT: KeyEvent = KeyEvent::new(KeyCode::Char('d'), CTRL);
const REMOVE_LAST: KeyEvent = KeyEvent::new(KeyCode::Char('u'), CTRL);
const ADD_BELOW: KeyEvent = KeyEvent::new(KeyCode::Down, CTRL_ALT);
const CHORD_LEADER: KeyEvent = KeyEvent::new(KeyCode::Char('k'), CTRL);

/// Lines of length 10 / 2 / 10, for exercising sticky columns across a short
/// middle line.
const STICKY_DOC: &str = "aaaaaaaaaa\nbb\ncccccccccc";

/// Runs a key through the handler and applies any produced command.
fn press(
    handler: &mut KeyboardHandler,
    event: &KeyEvent,
    document: &mut Document,
    cursor: &mut CursorState,
) -> KeyResult {
    let result = handler.handle_key(
        event,
        document,
        cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    );
    if let KeyResult::Command(cmd) = &result {
        cmd.apply(document, cursor).expect("command must apply");
    }
    result
}

/// Runs a key without applying anything, for asserting the raw result.
fn probe(
    handler: &mut KeyboardHandler,
    event: &KeyEvent,
    document: &Document,
    cursor: &CursorState,
) -> KeyResult {
    handler.handle_key(
        event,
        document,
        cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
    )
}

/// A modifier pattern requiring `Ctrl` and forbidding everything else.
fn ctrl_pattern(key: KeyCode) -> StrokePattern {
    StrokePattern::new(
        key,
        ModifierPattern::NONE.with_ctrl(ModifierState::Required),
    )
}

// ========== The command table is exhaustive ==========

#[test]
fn every_registered_builtin_command_has_an_implementation() {
    for meta in builtin_commands() {
        assert!(
            action_for(meta.id().as_str()).is_some(),
            "built-in command `{}` is registered but the keyboard handler cannot run it",
            meta.id()
        );
    }
}

#[test]
fn every_implemented_command_is_a_registered_builtin() {
    let registry = builtin_registry().expect("built-in registry must build");
    for id in implemented_ids() {
        assert!(
            registry.contains(id.as_str()),
            "the action table implements `{id}`, which is not a registered command"
        );
    }
}

#[test]
fn the_action_table_covers_the_registry_exactly_once() {
    assert_eq!(
        IMPLEMENTED_COMMAND_COUNT, BUILTIN_COMMAND_COUNT,
        "the action table and the built-in registry must have the same size"
    );

    let mut ids: Vec<&str> = implemented_ids().map(CommandId::as_str).collect();
    ids.sort_unstable();
    let unique = ids.len();
    ids.dedup();
    assert_eq!(unique, ids.len(), "an id appears twice in the action table");
}

#[test]
fn every_default_binding_names_an_implemented_command() {
    for layer in KeyboardHandler::new().keymap().layers() {
        for binding in layer.bindings() {
            let Some(id) = binding.command() else {
                continue;
            };
            assert!(
                action_for(id.as_str()).is_some(),
                "default keymap binds `{}` to `{id}`, which has no implementation",
                binding.display_sequence()
            );
        }
    }
}

// ========== The round-trip trap ==========

#[test]
fn a_horizontal_round_trip_does_not_revive_the_sticky_column() {
    // The trap: sticky columns are validated by exact cursor-state identity, so
    // a Left-then-Right round trip returns to a byte-identical state and would
    // resurrect the column captured before it. Dispatch must still invalidate on
    // the *horizontal* keys, which are now separate commands rather than
    // modifier-guarded branches.
    let mut doc = Document::new(STICKY_DOC);
    let mut cursor = cursors_at(&[(0, 8)]);
    let mut handler = KeyboardHandler::new();

    // Down clamps onto the short middle line while remembering column 8.
    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Down),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(heads(&cursor), vec![(1, 2)]);

    // Left then Right lands back on exactly (1, 2).
    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Left),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(heads(&cursor), vec![(1, 1)]);
    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Right),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(heads(&cursor), vec![(1, 2)]);

    // The stale column 8 must be gone: the next Down seeds from the live
    // column 2. A revived snapshot would land on (2, 8).
    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Down),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(
        heads(&cursor),
        vec![(2, 2)],
        "a byte-identical round trip revived the stale sticky column"
    );
    assert_eq!(doc.text(), STICKY_DOC, "navigation must not edit the text");
}

#[test]
fn a_selection_extending_round_trip_does_not_revive_the_sticky_column() {
    // The same trap through the *selecting* variants, which are now distinct
    // commands: Shift+Down captures a column, Shift+Left then Shift+Right
    // restores a byte-identical selection.
    let mut doc = Document::new(STICKY_DOC);
    let mut cursor = cursors_at(&[(0, 8)]);
    let mut handler = KeyboardHandler::new();

    let shift = Modifiers::shift();
    press(
        &mut handler,
        &KeyEvent::new(KeyCode::Down, shift),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(heads(&cursor), vec![(1, 2)]);

    press(
        &mut handler,
        &KeyEvent::new(KeyCode::Left, shift),
        &mut doc,
        &mut cursor,
    );
    press(
        &mut handler,
        &KeyEvent::new(KeyCode::Right, shift),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(heads(&cursor), vec![(1, 2)]);
    assert_eq!(cursor.primary.anchor, Position::new(0, 8));

    press(
        &mut handler,
        &KeyEvent::new(KeyCode::Down, shift),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(
        heads(&cursor),
        vec![(2, 2)],
        "a byte-identical selection round trip revived the stale sticky column"
    );
}

#[test]
fn a_horizontal_round_trip_does_not_revive_the_addition_order_stack() {
    // The same trap on the multi-cursor stack: after a round trip the stack must
    // be empty, so remove-last-cursor no-ops instead of dropping an
    // arbitrarily-ordered cursor.
    let mut doc = Document::new("aaa\nbbb\nccc");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor);
    assert_eq!(heads(&cursor), vec![(0, 1), (1, 1)]);

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Left),
        &mut doc,
        &mut cursor,
    );
    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Right),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(
        heads(&cursor),
        vec![(0, 1), (1, 1)],
        "the round trip must return to the same two cursors"
    );

    assert_eq!(
        probe(&mut handler, &REMOVE_LAST, &doc, &cursor),
        KeyResult::Handled,
        "a byte-identical round trip revived the stale addition-order stack"
    );
}

#[test]
fn an_edit_that_round_trips_does_not_revive_the_addition_order_stack() {
    // Typing a character and deleting it again returns the cursors to identical
    // coordinates, but the document revision moved on. Both guards must hold.
    let mut doc = Document::new("aaa\nbbb\nccc");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor);
    assert_eq!(heads(&cursor), vec![(0, 1), (1, 1)]);

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Char('x')),
        &mut doc,
        &mut cursor,
    );
    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Backspace),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(doc.text(), "aaa\nbbb\nccc");
    assert_eq!(heads(&cursor), vec![(0, 1), (1, 1)]);

    assert_eq!(
        probe(&mut handler, &REMOVE_LAST, &doc, &cursor),
        KeyResult::Handled,
        "an edit that round-tripped revived the stale addition-order stack"
    );
}

// ========== Multi-key sequences ==========

#[cfg(test)]
mod sequence_tests;
