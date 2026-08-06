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
const CHORD_LEADER: KeyEvent = KeyEvent::new(KeyCode::Char('b'), CTRL);

/// A handler whose keymap adds a `Ctrl+B Ctrl+D` chord in a host layer.
///
/// The default keymap binds no multi-stroke sequence — a bare `Ctrl+K` opens the
/// palette, and a complete binding forecloses every chord beneath it — so the
/// sequence machinery needs a chord supplied to be tested at all. Supplying it
/// here keeps those tests about the *resolver*, which is where the behaviour
/// lives, instead of coupling them to whichever binding happens to be a chord
/// this month. The leader is [`CHORD_LEADER`]: any key the default leaves free.
fn handler_with_chord() -> KeyboardHandler {
    let mut handler = KeyboardHandler::new();
    let mut layer = Keymap::new("host-chord");
    layer.push(KeyBinding::new(
        chord_stroke(CHORD_LEADER.key),
        &[chord_stroke(KeyCode::Char('d'))],
        crate::commands::builtin::MULTI_CURSOR_SKIP_LAST_OCCURRENCE,
    ));
    handler.push_keymap(layer);
    handler
}

/// A `Ctrl`+letter stroke with `Shift` forbidden, as the default keymap spells
/// one.
fn chord_stroke(key: KeyCode) -> StrokePattern {
    use ModifierState::{Any, Forbidden, Required};
    StrokePattern::new(
        key,
        ModifierPattern::new(Forbidden, Required, Forbidden, Forbidden, Any),
    )
}

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
fn every_default_binding_names_a_command_the_kernel_or_a_host_owns() {
    // A default binding may name a command the kernel does not implement *at the
    // editor level*, but only in one of two declared cases:
    //
    //   - a **host command**, which the kernel cannot implement at all — it cannot
    //     draw a palette — and so names and reports for each face to run;
    //   - a **workspace command**, which the kernel does implement, in
    //     `Workspace::run_command`, just not on `Editor`. It has no
    //     `KeyboardAction` because the action table acts on one editor and a
    //     workspace command acts on the thing that owns the editors.
    //
    // Both resolve to `KeyResult::HostCommand`, so "no action" alone cannot tell
    // either of them from a typo — and a typo surfaces as a key that does nothing
    // at all. The three cases are therefore separated here rather than the check
    // being loosened to "not implemented is fine".
    let unimplemented_by_design: Vec<&str> = crate::commands::builtin::host_command_metas()
        .iter()
        .chain(crate::commands::builtin::workspace_command_metas())
        .map(|meta| meta.id().as_str())
        .collect();

    for layer in KeyboardHandler::new().keymap().layers() {
        for binding in layer.bindings() {
            let Some(id) = binding.command() else {
                continue;
            };
            if unimplemented_by_design.contains(&id.as_str()) {
                assert!(
                    action_for(id.as_str()).is_none(),
                    "`{id}` is declared a host or workspace command but the kernel \
                     implements it as an editor action"
                );
                continue;
            }
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

/// Adding a cursor vertically **by id** must do exactly what the key does.
///
/// Reported from the running web demo: "add cursor above and below isn't wired
/// up" from the palette. Both ids are registered *and* present in the action
/// table, so nothing static catches a divergence — only running both routes over
/// the same document and comparing every resulting caret does.
///
/// This is the failure mode a palette makes easy to ship: a command that works
/// perfectly from its key and does nothing from its name is invisible to every
/// test that only presses keys.
#[test]
fn adding_a_cursor_vertically_by_id_matches_the_key() {
    let text = "Hello World\nSecond Line\nThird Line";
    // Bound to `let` first: `CommandId::as_str` borrows, so calling it inline
    // inside the array would drop the temporary at the end of the statement.
    let below = crate::commands::builtin::MULTI_CURSOR_ADD_CURSOR_BELOW;
    let above = crate::commands::builtin::MULTI_CURSOR_ADD_CURSOR_ABOVE;
    let cases = [
        (
            below.as_str(),
            KeyEvent::new(KeyCode::Down, CTRL_ALT),
            (1_usize, 4_usize),
        ),
        (above.as_str(), KeyEvent::new(KeyCode::Up, CTRL_ALT), (1, 4)),
    ];

    for (id, event, start) in cases {
        // The key route.
        let mut key_doc = Document::new(text);
        let mut key_cursor = cursors_at(&[start]);
        let mut key_handler = KeyboardHandler::new();
        let key_result = key_handler.handle_key(
            &event,
            &key_doc,
            &key_cursor,
            &UndoTree::new(),
            &EditorConfig::default(),
        );
        if let KeyResult::Command(cmd) = key_result {
            cmd.apply(&mut key_doc, &mut key_cursor)
                .expect("command must apply");
        }

        // The palette route: same command, named instead of typed.
        let mut id_doc = Document::new(text);
        let mut id_cursor = cursors_at(&[start]);
        let mut id_handler = KeyboardHandler::new();
        let id_result = id_handler
            .run_command(
                id,
                CommandArgs::NONE,
                &id_doc,
                &id_cursor,
                &UndoTree::new(),
                &EditorConfig::default(),
            )
            .expect("the kernel implements this command");
        if let KeyResult::Command(cmd) = id_result {
            cmd.apply(&mut id_doc, &mut id_cursor)
                .expect("command must apply");
        }

        assert_eq!(
            heads(&id_cursor),
            heads(&key_cursor),
            "`{id}` disagreed between the key route and the palette route"
        );
        assert_eq!(
            heads(&id_cursor).len(),
            2,
            "`{id}` must leave two carets, not {:?}",
            heads(&id_cursor)
        );
    }
}
