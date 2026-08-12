//! Tests for the minimal modal keymap.
//!
//! These drive the **real** [`KeyboardHandler`] against a real
//! [`Document`](crate::document::Document), because the point of this keymap is
//! that the whole modal path works end to end: mode-scoped resolution, a mode
//! switch, the typing gate, and the four commands the motions name. Asserting
//! the keymap's contents instead would prove only that a table was typed out
//! correctly.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::builtin::default_registry;
use super::{
    INSERT, KeymapStack, MODAL_KEYMAP_BINDING_COUNT, NORMAL, default_non_modal_keymap,
    minimal_modal_keymap,
};
use crate::document::{CursorState, Document, Position};
use crate::editor::{CaretScopes, EditorConfig};
use crate::history::UndoTree;
use crate::input::keyboard::KeyboardHandler;
use crate::input::{KeyCode, KeyEvent, KeyResult, Modifiers};

/// A handler with the modal keymap pushed over the default one, in the mode the
/// keymap says a session begins in.
///
/// The initial mode is read off the stack rather than named here: a test that
/// hard-codes `"normal"` would still pass if the keymap forgot to declare it,
/// and the declaration is half of what makes the keymap usable.
fn modal_handler() -> KeyboardHandler {
    let mut handler = KeyboardHandler::new();
    handler.push_keymap(minimal_modal_keymap());
    let initial = handler.keymap().initial_mode().cloned();
    handler.set_mode(initial);
    handler
}

/// Runs a key through the handler and applies any command it produced.
fn press(handler: &mut KeyboardHandler, key: KeyCode, document: &mut Document) -> KeyResult {
    let mut cursor = CursorState::at(Position::new(0, 0));
    press_at(handler, key, document, &mut cursor)
}

/// Runs a key with a cursor state the caller keeps between presses.
fn press_at(
    handler: &mut KeyboardHandler,
    key: KeyCode,
    document: &mut Document,
    cursor: &mut CursorState,
) -> KeyResult {
    let result = handler.handle_key(
        &KeyEvent::new(key, Modifiers::none()),
        document,
        cursor,
        &UndoTree::new(),
        &EditorConfig::default(),
        &CaretScopes::none(),
    );
    if let KeyResult::Command(command) = &result {
        command.apply(document, cursor).expect("command must apply");
    }
    result
}

#[test]
fn the_binding_count_matches_the_documented_constant() {
    assert_eq!(minimal_modal_keymap().len(), MODAL_KEYMAP_BINDING_COUNT);
}

#[test]
fn the_modal_keymap_names_only_commands_the_kernel_registers() {
    // The same guard the default keymap has: a typo here would be a key that
    // silently does nothing, which in a mode where nothing else types is
    // indistinguishable from the editor having hung.
    let registry = default_registry().expect("the default registry must build");
    minimal_modal_keymap()
        .validate(&registry)
        .expect("every binding must name a registered command");
}

#[test]
fn the_modal_keymap_stacks_over_the_default_without_shadowing_it() {
    // Bare letters are free in the default keymap, so the modal layer takes
    // nothing away from it. If that ever stops being true this fails at the
    // stack's own validation rather than as a key that quietly changed meaning.
    let mut stack = KeymapStack::with_base(default_non_modal_keymap());
    stack.push(minimal_modal_keymap());
    // `default_registry`, not `builtin_registry`: the default keymap binds host
    // commands the built-in table alone does not know, which its own tests
    // record as a statement rather than a quirk.
    let registry = default_registry().expect("the default registry must build");
    stack.validate(&registry).expect("the stack must validate");
}

#[test]
fn a_session_begins_in_the_mode_the_keymap_names() {
    let mut handler = KeyboardHandler::new();
    assert_eq!(handler.keymap().initial_mode(), None);

    handler.push_keymap(minimal_modal_keymap());
    assert_eq!(handler.keymap().initial_mode(), Some(&NORMAL));
}

#[test]
fn the_default_keymap_alone_names_no_mode() {
    // The invariant that keeps every face that exists today non-modal: the
    // keymap they run declares no mode to begin in, so nothing can put them in
    // one by accident.
    assert_eq!(default_non_modal_keymap().initial_mode(), None);
    assert_eq!(KeyboardHandler::new().keymap().initial_mode(), None);
}

#[test]
fn normal_mode_moves_the_caret_and_types_nothing() {
    let mut document = Document::new("abc\ndef");
    let mut cursor = CursorState::at(Position::new(0, 0));
    let mut handler = modal_handler();

    for (key, expected) in [
        (KeyCode::Char('l'), Position::new(0, 1)),
        (KeyCode::Char('j'), Position::new(1, 1)),
        (KeyCode::Char('k'), Position::new(0, 1)),
        (KeyCode::Char('h'), Position::new(0, 0)),
    ] {
        press_at(&mut handler, key, &mut document, &mut cursor);
        assert_eq!(cursor.primary.head, expected, "{key:?} moved the wrong way");
    }

    assert_eq!(
        document.text(),
        "abc\ndef",
        "a motion key wrote into the document"
    );
}

#[test]
fn a_letter_normal_mode_does_not_bind_is_swallowed() {
    // The whole reason the typing gate had to exist: `q` is bound to nothing in
    // any layer, and without the gate it would be inserted.
    let mut document = Document::new("abc");
    let mut handler = modal_handler();

    let result = press(&mut handler, KeyCode::Char('q'), &mut document);

    assert_eq!(document.text(), "abc");
    assert!(matches!(result, KeyResult::Handled));
}

#[test]
fn i_enters_insert_and_the_same_letter_then_types() {
    let mut document = Document::new("abc");
    let mut cursor = CursorState::at(Position::new(0, 0));
    let mut handler = modal_handler();

    press_at(&mut handler, KeyCode::Char('q'), &mut document, &mut cursor);
    assert_eq!(
        document.text(),
        "abc",
        "typing before `i` reached the document"
    );

    press_at(&mut handler, KeyCode::Char('i'), &mut document, &mut cursor);
    assert_eq!(handler.mode(), Some(&INSERT));

    press_at(&mut handler, KeyCode::Char('q'), &mut document, &mut cursor);
    assert_eq!(document.text(), "qabc", "insert mode did not type");
}

#[test]
fn escape_returns_to_normal_and_typing_stops_again() {
    let mut document = Document::new("abc");
    let mut cursor = CursorState::at(Position::new(0, 0));
    let mut handler = modal_handler();

    press_at(&mut handler, KeyCode::Char('i'), &mut document, &mut cursor);
    press_at(&mut handler, KeyCode::Char('q'), &mut document, &mut cursor);
    assert_eq!(document.text(), "qabc");

    press_at(&mut handler, KeyCode::Escape, &mut document, &mut cursor);
    assert_eq!(handler.mode(), Some(&NORMAL));

    press_at(&mut handler, KeyCode::Char('q'), &mut document, &mut cursor);
    assert_eq!(
        document.text(),
        "qabc",
        "leaving insert mode did not stop typing"
    );
}

#[test]
fn a_enters_insert_one_column_further_right() {
    // `a` is the case `then_enter_mode` exists for: a command *and* a mode
    // switch from one binding. If only the mode switch happened, the caret would
    // still be at column 0 and the text would come out "qabc".
    let mut document = Document::new("abc");
    let mut cursor = CursorState::at(Position::new(0, 0));
    let mut handler = modal_handler();

    press_at(&mut handler, KeyCode::Char('a'), &mut document, &mut cursor);
    assert_eq!(handler.mode(), Some(&INSERT));
    assert_eq!(cursor.primary.head, Position::new(0, 1));

    press_at(&mut handler, KeyCode::Char('q'), &mut document, &mut cursor);
    assert_eq!(document.text(), "aqbc");
}

#[test]
fn every_binding_in_the_modal_keymap_is_reachable() {
    // The oracle the hint index uses: a binding is reachable when typing its
    // sequence in its mode resolves to that exact binding. Applied here it
    // catches a mode-scoped binding that a mode-free binding in the default
    // layer would outrank — the failure mode that would make a modal key
    // silently keep its non-modal meaning.
    let mut stack = KeymapStack::with_base(default_non_modal_keymap());
    stack.push(minimal_modal_keymap());
    // Read the bindings back out of the stack rather than from a second copy of
    // the keymap: reachability is settled by pointer identity, so a clone would
    // report every binding unreachable no matter what the stack resolves.
    let modal = stack.layers().last().expect("the modal layer was pushed");

    for binding in modal.bindings() {
        assert!(
            stack.binding_is_reachable(binding),
            "a modal binding is unreachable: {:?}",
            binding.sequence()
        );
    }
}
