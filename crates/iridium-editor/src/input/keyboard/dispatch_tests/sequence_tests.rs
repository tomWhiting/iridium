//! Multi-stroke sequences, user keymap layers, and the fall-through.
//!
//! Split from the parent module for size. These are the tests that exercise
//! bindings *as data*: chords held pending, a dead sequence replaying its final
//! stroke rather than eating it, a user layer rebinding and unbinding without
//! this crate changing, and a bound command the kernel does not implement being
//! handed to the host by name.

use super::*;

#[test]
fn the_chord_leader_is_consumed_and_held_pending() {
    let doc = Document::new("foo foo foo");
    let cursor = cursors_at(&[(0, 1)]);
    let mut handler = handler_with_chord();

    assert_eq!(
        probe(&mut handler, &CHORD_LEADER, &doc, &cursor),
        KeyResult::Handled,
        "a live prefix must be consumed, never inserted as text"
    );
    assert_eq!(handler.pending_sequence().len(), 1);
    assert_eq!(doc.text(), "foo foo foo");

    // Escape leaves the sequence without collapsing the cursor.
    assert_eq!(
        probe(
            &mut handler,
            &KeyEvent::simple(KeyCode::Escape),
            &doc,
            &cursor
        ),
        KeyResult::Handled
    );
    assert!(handler.pending_sequence().is_empty());
}

#[test]
fn the_skip_occurrence_chord_advances_the_most_recently_added_cursor() {
    let mut doc = Document::new("foo foo foo");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = handler_with_chord();

    // Bootstrap onto the word, then add the second occurrence.
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor);
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor);
    assert_eq!(heads(&cursor), vec![(0, 3), (0, 7)]);

    // Ctrl+K Ctrl+D moves the added cursor from the second to the third match.
    press(&mut handler, &CHORD_LEADER, &mut doc, &mut cursor);
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor);
    assert_eq!(heads(&cursor), vec![(0, 3), (0, 11)]);
    assert!(handler.pending_sequence().is_empty());

    // A second skip wraps back onto the middle match, which is only possible if
    // the addition-order stack survived the first skip.
    press(&mut handler, &CHORD_LEADER, &mut doc, &mut cursor);
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor);
    assert_eq!(heads(&cursor), vec![(0, 3), (0, 7)]);
    assert_eq!(doc.text(), "foo foo foo", "skip must not edit the text");
}

#[test]
fn a_dead_ended_sequence_replays_the_final_stroke_into_the_document() {
    // REGRESSION: this used to return `Handled` and drop the keystroke, so a user
    // who armed a chord leader and then typed lost one character of their
    // document, with nothing on screen to explain it. The chord is abandoned and
    // the character typed.
    //
    // The chord comes from a host layer because the default keymap no longer
    // binds one; the defect was in the resolver, so any live chord proves it.
    let mut doc = Document::new("");
    let mut cursor = cursors_at(&[(0, 0)]);
    let mut handler = handler_with_chord();

    probe(&mut handler, &CHORD_LEADER, &doc, &cursor);
    assert_eq!(handler.pending_sequence().len(), 1);

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Char('h')),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(
        doc.text(),
        "h",
        "the character that killed the chord was eaten"
    );
    assert!(handler.pending_sequence().is_empty());

    // And the next character types normally, so nothing is left half-pending.
    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Char('i')),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(doc.text(), "hi");
}

#[test]
fn holding_the_chord_leader_down_does_not_change_what_the_next_stroke_means() {
    // REGRESSION: auto-repeat used to flip the sequence between pending and dead on
    // alternate repeats, so after an even number of repeats `Ctrl+D` ran
    // add-selection-to-next-match (which spawns a cursor) instead of the intended
    // skip-occurrence. A repeat of the stroke already held is now swallowed.
    //
    // Held-down keys are a property of the resolver, not of any binding, so the
    // chord is supplied by a host layer now that the default keymap has none.
    let mut doc = Document::new("foo foo foo");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = handler_with_chord();

    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor);
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor);
    assert_eq!(heads(&cursor), vec![(0, 3), (0, 7)]);

    let repeat = KeyEvent {
        key: CHORD_LEADER.key,
        modifiers: CTRL,
        is_repeat: true,
    };
    press(&mut handler, &CHORD_LEADER, &mut doc, &mut cursor);
    for _ in 0..4 {
        press(&mut handler, &repeat, &mut doc, &mut cursor);
        assert_eq!(
            handler.pending_sequence().len(),
            1,
            "an auto-repeat of the leader must leave the sequence exactly as it was"
        );
    }
    press(&mut handler, &ADD_NEXT, &mut doc, &mut cursor);
    assert_eq!(
        heads(&cursor),
        vec![(0, 3), (0, 11)],
        "the chord must still mean skip-occurrence after the leader was held"
    );
    assert_eq!(
        cursor.cursor_count(),
        2,
        "skip must not spawn a third cursor"
    );
}

// ========== Bindings are data ==========

#[test]
fn a_user_layer_rebinds_and_unbinds_without_editing_the_default() {
    let doc = Document::new("foo foo");
    let cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    let mut user = Keymap::new("user");
    // Suppress add-selection-to-next-match, and move select-all onto Ctrl+B.
    user.push(KeyBinding::unbound(ctrl_pattern(KeyCode::Char('d')), &[]));
    user.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('b')),
        &[],
        SELECTION_SELECT_ALL,
    ));
    handler.push_keymap(user);

    // An unbound single chord falls through exactly as if it had never been
    // bound: a Ctrl+character carries no text, so it is ignored.
    assert_eq!(
        probe(&mut handler, &ADD_NEXT, &doc, &cursor),
        KeyResult::Ignored
    );

    let result = probe(
        &mut handler,
        &KeyEvent::new(KeyCode::Char('b'), CTRL),
        &doc,
        &cursor,
    );
    let KeyResult::Command(Command::SetSelection { new_state, .. }) = result else {
        panic!("Ctrl+B must select all, got {result:?}");
    };
    assert_eq!(new_state.primary.anchor, Position::new(0, 0));
    assert_eq!(new_state.primary.head, Position::new(0, 7));

    // Popping the layer restores the default meaning of Ctrl+D.
    handler.pop_keymap();
    assert!(matches!(
        probe(&mut handler, &ADD_NEXT, &doc, &cursor),
        KeyResult::Command(_)
    ));
    assert_eq!(doc.text(), "foo foo");
}

#[test]
fn a_binding_naming_an_unimplemented_command_reports_the_command() {
    // REGRESSION: this used to report `KeyResult::Ignored`, dropping the resolved
    // id. That made a host command indistinguishable from a meaningless keypress —
    // and for a multi-stroke sequence unrecoverable, because the earlier strokes had
    // already returned `Handled`. The id and its arguments are now reported, which
    // is the whole path by which a command implemented outside the kernel runs.
    let doc = Document::new("x");
    let cursor = cursors_at(&[(0, 0)]);
    let mut handler = KeyboardHandler::new();

    let mut host = Keymap::new("host");
    host.push(KeyBinding::new(
        StrokePattern::new(KeyCode::F5, ModifierPattern::NONE),
        &[],
        CommandId::from_static("host.doThing"),
    ));
    host.push(KeyBinding::new(
        StrokePattern::new(KeyCode::F6, ModifierPattern::NONE),
        &[StrokePattern::new(KeyCode::F7, ModifierPattern::NONE)],
        CommandId::from_static("host.twoStroke"),
    ));
    handler.push_keymap(host);

    assert_eq!(
        probe(&mut handler, &KeyEvent::simple(KeyCode::F5), &doc, &cursor),
        KeyResult::HostCommand {
            command: CommandId::from_static("host.doThing"),
            args: CommandArgs::NONE,
        }
    );

    // A multi-stroke host binding is reported when it completes, not lost.
    assert_eq!(
        probe(&mut handler, &KeyEvent::simple(KeyCode::F6), &doc, &cursor),
        KeyResult::Handled
    );
    assert_eq!(
        probe(&mut handler, &KeyEvent::simple(KeyCode::F7), &doc, &cursor),
        KeyResult::HostCommand {
            command: CommandId::from_static("host.twoStroke"),
            args: CommandArgs::NONE,
        }
    );
    assert_eq!(doc.text(), "x", "a host command must not edit the document");
}

// ========== Fall-through, guards and known defects ==========

#[test]
fn an_unbound_character_inserts_the_original_case() {
    // Resolution lowercases `Char` so `Ctrl+Shift+K` matches either layout, but
    // the normalized form must never reach the document.
    let mut doc = Document::new("");
    let mut cursor = cursors_at(&[(0, 0)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::new(KeyCode::Char('A'), Modifiers::shift()),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(doc.text(), "A");
}

#[test]
fn altgraph_does_not_satisfy_the_add_cursor_chord() {
    // On many non-US layouts AltGr is reported as Ctrl+Alt while composing a
    // character. The binding forbids AltGraph, so the chord cannot fire.
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_at(&[(1, 1)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &KeyEvent::new(KeyCode::Down, CTRL_ALT_GRAPH),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(cursor.cursor_count(), 1, "AltGr+Down must not add a cursor");
    assert_eq!(heads(&cursor), vec![(2, 1)]);

    // The genuine chord still adds one.
    press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor);
    assert_eq!(cursor.cursor_count(), 1, "already on the last line");
    let mut cursor = cursors_at(&[(0, 1)]);
    press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor);
    assert_eq!(cursor.cursor_count(), 2);
}

#[test]
fn history_chords_are_acknowledged_without_running_history() {
    // KNOWN PRE-EXISTING DEFECT, preserved deliberately: the handler does not
    // execute undo or redo — the web host intercepts the chord before dispatch
    // and drives the editor directly. `Ctrl+Shift+Z` also resolves to *undo*,
    // not redo, because the pre-registry dispatch matched 'z' regardless of
    // Shift and the default keymap transcribes that faithfully.
    let doc = Document::new("hello");
    let cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    for event in [
        KeyEvent::new(KeyCode::Char('z'), CTRL),
        KeyEvent::new(KeyCode::Char('z'), CTRL_SHIFT),
        KeyEvent::new(KeyCode::Char('y'), CTRL),
    ] {
        assert_eq!(
            probe(&mut handler, &event, &doc, &cursor),
            KeyResult::Handled,
            "history chord {event:?} must be acknowledged"
        );
    }
    assert_eq!(doc.text(), "hello");
    assert_eq!(heads(&cursor), vec![(0, 1)]);
}

// ===== Key-hint cache =====

/// Asserts the handler's cached hint index equals a fresh build from its own
/// live keymap.
fn assert_hints_fresh(handler: &KeyboardHandler, at: &str) {
    let fresh = crate::commands::KeyHintIndex::build(handler.keymap());
    assert_eq!(
        handler.key_hints().len(),
        fresh.len(),
        "the cached hint index is stale after {at}"
    );
    for id in fresh.command_ids() {
        let cached: Vec<&str> = handler
            .key_hints()
            .hints_for(id.as_str())
            .iter()
            .map(|hint| hint.label(crate::commands::KeyLabelStyle::Portable))
            .collect();
        let expected: Vec<&str> = fresh
            .hints_for(id.as_str())
            .iter()
            .map(|hint| hint.label(crate::commands::KeyLabelStyle::Portable))
            .collect();
        assert_eq!(cached, expected, "`{id}` is stale after {at}");
    }
}

#[test]
fn key_hints_are_rebuilt_by_every_keymap_mutator() {
    // `key_hints` is state derived from the keymap, and four separate methods
    // mutate that keymap. Derived state refreshed by four call sites is derived
    // state that will eventually be stale at one of them — and a stale hint index
    // shows a palette a key that no longer runs the command beside it, which is a
    // wrong answer rather than a missing one. All four are exercised here, at the
    // handler, because two of them are unreachable through `Editor`.
    let mut handler = KeyboardHandler::new();
    assert_hints_fresh(&handler, "construction");

    let mut user = Keymap::new("user");
    user.push(KeyBinding::new(
        chord_stroke(KeyCode::Char('b')),
        &[],
        crate::commands::builtin::LINES_DELETE,
    ));

    handler.push_keymap(user.clone());
    assert_hints_fresh(&handler, "push_keymap");

    handler.pop_keymap().expect("the user layer is on top");
    assert_hints_fresh(&handler, "pop_keymap");

    handler
        .push_validated_keymap(
            user,
            &crate::commands::builtin::default_registry().expect("the registry builds"),
        )
        .expect("Ctrl+B is free");
    assert_hints_fresh(&handler, "push_validated_keymap");

    handler.set_keymap(crate::commands::default_keymap_stack());
    assert_hints_fresh(&handler, "set_keymap");
}
