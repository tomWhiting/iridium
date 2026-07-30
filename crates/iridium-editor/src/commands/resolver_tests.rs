//! Resolver tests: the pending-sequence state machine.

// `fn_params_excessive_bools` is allowed for the `mods` helper only: its five
// parameters mirror the five hardware modifier bits of `Modifiers` one for one,
// exactly as that struct's own documented allow does.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::fn_params_excessive_bools
)]

use super::{
    CommandId, KeyBinding, KeyPress, Keymap, KeymapResolver, KeymapStack, ModeName,
    ModifierPattern, ModifierState, Resolution, StrokePattern,
};
use crate::input::{KeyCode, KeyEvent, Modifiers};

const fn mods(shift: bool, ctrl: bool, alt: bool, meta: bool, alt_graph: bool) -> Modifiers {
    Modifiers {
        shift,
        ctrl,
        alt,
        meta,
        alt_graph,
    }
}

fn ctrl(key: KeyCode) -> StrokePattern {
    StrokePattern::new(
        key,
        ModifierPattern::NONE.with_ctrl(ModifierState::Required),
    )
}

fn ctrl_press(c: char) -> KeyPress {
    KeyPress::new(KeyCode::Char(c), mods(false, true, false, false, false))
}

/// Base layer: one chord, one two-stroke sequence, and `Escape`.
fn stack() -> KeymapStack {
    let mut keymap = Keymap::new("test");
    keymap.push(KeyBinding::new(
        ctrl(KeyCode::Char('c')),
        &[],
        CommandId::from_static("clipboard.copy"),
    ));
    keymap.push(KeyBinding::new(
        ctrl(KeyCode::Char('k')),
        &[ctrl(KeyCode::Char('d'))],
        CommandId::from_static("multiCursor.skipLastOccurrence"),
    ));
    keymap.push(KeyBinding::new(
        ctrl(KeyCode::Char('k')),
        &[ctrl(KeyCode::Char('c'))],
        CommandId::from_static("comment.toggleLine"),
    ));
    keymap.push(KeyBinding::new(
        StrokePattern::new(KeyCode::Escape, ModifierPattern::ANY),
        &[],
        CommandId::from_static("selection.collapseToPrimary"),
    ));
    KeymapStack::with_base(keymap)
}

#[test]
fn a_single_chord_matches_immediately() {
    let keymap = stack();
    let mut resolver = KeymapResolver::new();

    let outcome = resolver.resolve(&keymap, ctrl_press('c'));
    assert_eq!(
        outcome,
        Resolution::matched(CommandId::from_static("clipboard.copy"))
    );
    assert!(outcome.consumed());
    assert_eq!(outcome.command().unwrap().as_str(), "clipboard.copy");
    assert!(!resolver.is_pending());
    assert!(resolver.pending().is_empty());
}

#[test]
fn an_unbound_chord_falls_through_without_consuming() {
    let keymap = stack();
    let mut resolver = KeymapResolver::new();

    let outcome = resolver.resolve(&keymap, ctrl_press('q'));
    assert_eq!(outcome, Resolution::NoMatch);
    assert!(!outcome.consumed());
    assert!(outcome.command().is_none());
    assert!(!resolver.is_pending());
}

#[test]
fn a_prefix_goes_pending_then_completes() {
    let keymap = stack();
    let mut resolver = KeymapResolver::new();

    assert_eq!(
        resolver.resolve(&keymap, ctrl_press('k')),
        Resolution::Pending
    );
    assert!(resolver.is_pending());
    assert_eq!(resolver.pending().len(), 1);
    assert_eq!(resolver.pending()[0].key, KeyCode::Char('k'));

    assert_eq!(
        resolver.resolve(&keymap, ctrl_press('d')),
        Resolution::matched(CommandId::from_static("multiCursor.skipLastOccurrence"))
    );
    assert!(!resolver.is_pending());
}

#[test]
fn a_prefix_disambiguates_between_two_continuations() {
    let keymap = stack();
    let mut resolver = KeymapResolver::new();

    assert_eq!(
        resolver.resolve(&keymap, ctrl_press('k')),
        Resolution::Pending
    );
    assert_eq!(
        resolver.resolve(&keymap, ctrl_press('c')),
        Resolution::matched(CommandId::from_static("comment.toggleLine"))
    );
}

#[test]
fn a_chord_key_that_is_also_a_chord_start_is_not_swallowed_afterwards() {
    let keymap = stack();
    let mut resolver = KeymapResolver::new();

    // Ctrl+K Ctrl+C runs the sequence, then a bare Ctrl+C runs the chord again:
    // the pending buffer must be genuinely cleared, not merely truncated.
    assert_eq!(
        resolver.resolve(&keymap, ctrl_press('k')),
        Resolution::Pending
    );
    assert!(matches!(
        resolver.resolve(&keymap, ctrl_press('c')),
        Resolution::Matched(_)
    ));
    assert_eq!(
        resolver.resolve(&keymap, ctrl_press('c')),
        Resolution::matched(CommandId::from_static("clipboard.copy"))
    );
}

#[test]
fn escape_aborts_a_pending_sequence_without_running_its_own_binding() {
    let keymap = stack();
    let mut resolver = KeymapResolver::new();

    assert_eq!(
        resolver.resolve(&keymap, ctrl_press('k')),
        Resolution::Pending
    );
    let outcome = resolver.resolve(&keymap, KeyPress::plain(KeyCode::Escape));
    assert_eq!(outcome, Resolution::Aborted);
    assert!(
        outcome.consumed(),
        "an aborted chord key must not become text"
    );
    assert!(!resolver.is_pending());

    // With nothing pending, Escape resolves normally.
    assert_eq!(
        resolver.resolve(&keymap, KeyPress::plain(KeyCode::Escape)),
        Resolution::matched(CommandId::from_static("selection.collapseToPrimary"))
    );
}

#[test]
fn escape_aborts_whatever_modifiers_accompany_it() {
    let keymap = stack();
    let mut resolver = KeymapResolver::new();
    assert_eq!(
        resolver.resolve(&keymap, ctrl_press('k')),
        Resolution::Pending
    );
    let shifted = KeyPress::new(KeyCode::Escape, mods(true, false, false, false, false));
    assert_eq!(resolver.resolve(&keymap, shifted), Resolution::Aborted);
}

#[test]
fn a_custom_abort_stroke_replaces_escape() {
    let keymap = stack();
    let abort = StrokePattern::new(
        KeyCode::Char('g'),
        ModifierPattern::NONE.with_ctrl(ModifierState::Required),
    );
    let mut resolver = KeymapResolver::new().with_abort_stroke(abort);
    assert_eq!(resolver.abort_stroke().key, KeyCode::Char('g'));

    assert_eq!(
        resolver.resolve(&keymap, ctrl_press('k')),
        Resolution::Pending
    );
    assert_eq!(
        resolver.resolve(&keymap, ctrl_press('g')),
        Resolution::Aborted
    );

    // Escape is no longer the abort key, so it now dead-ends the sequence. The
    // dead-end path replays the stroke, and this keymap binds Escape, so the
    // replay runs that binding instead of discarding the keypress.
    resolver.set_abort_stroke(StrokePattern::new(KeyCode::F12, ModifierPattern::ANY));
    assert_eq!(
        resolver.resolve(&keymap, ctrl_press('k')),
        Resolution::Pending
    );
    assert_eq!(
        resolver.resolve(&keymap, KeyPress::plain(KeyCode::Escape)),
        Resolution::matched(CommandId::from_static("selection.collapseToPrimary"))
    );
    assert!(!resolver.is_pending());
}

#[test]
fn a_dead_end_replays_the_final_stroke_instead_of_eating_it() {
    // REGRESSION: a dead-ended sequence used to return `Aborted`, which consumed
    // the keystroke. With `Ctrl+K` a live leader in the *default* keymap, that made
    // `Ctrl+K` followed by an ordinary character silently destroy that character —
    // data loss with no host-visible cause. The sequence is now abandoned and the
    // final keypress retried from scratch, so it resolves exactly as if the leader
    // had never been pressed.
    let keymap = stack();
    let mut resolver = KeymapResolver::new();

    assert_eq!(
        resolver.resolve(&keymap, ctrl_press('k')),
        Resolution::Pending
    );
    // 'z' continues nothing and is bound to nothing: it falls through to typing.
    let outcome = resolver.resolve(&keymap, KeyPress::plain(KeyCode::Char('z')));
    assert_eq!(outcome, Resolution::NoMatch);
    assert!(
        !outcome.consumed(),
        "the stroke that killed the chord must reach the typing fall-through"
    );
    assert!(!resolver.is_pending());

    // And a stroke that *is* bound on its own runs that binding rather than
    // vanishing, so a mistyped chord costs the leader and nothing else.
    assert_eq!(
        resolver.resolve(&keymap, ctrl_press('k')),
        Resolution::Pending
    );
    assert_eq!(
        resolver.resolve(&keymap, ctrl_press('c')),
        Resolution::matched(CommandId::from_static("comment.toggleLine")),
        "ctrl+k ctrl+c is a real sequence here"
    );
    assert_eq!(
        resolver.resolve(&keymap, ctrl_press('k')),
        Resolution::Pending
    );
    let mut deeper = KeymapResolver::new();
    assert_eq!(
        deeper.resolve(&keymap, ctrl_press('k')),
        Resolution::Pending
    );
    assert_eq!(
        deeper.resolve(&keymap, KeyPress::plain(KeyCode::Escape)),
        Resolution::Aborted,
        "the abort stroke still aborts rather than replaying"
    );
}

#[test]
fn abort_pending_reports_whether_it_cancelled_anything() {
    let keymap = stack();
    let mut resolver = KeymapResolver::new();

    assert!(!resolver.abort_pending());
    assert_eq!(
        resolver.resolve(&keymap, ctrl_press('k')),
        Resolution::Pending
    );
    assert!(resolver.abort_pending());
    assert!(!resolver.is_pending());
    assert!(!resolver.abort_pending());
}

#[test]
fn an_exact_match_wins_over_a_longer_continuation() {
    // Ctrl+K is bound both as a complete chord and as a sequence prefix. The
    // documented policy is that the exact match fires at once, with no timeout;
    // `Keymap::validate` is what reports the unreachable longer binding.
    let mut keymap = Keymap::new("clashing");
    keymap.push(KeyBinding::new(
        ctrl(KeyCode::Char('k')),
        &[],
        CommandId::from_static("lines.delete"),
    ));
    keymap.push(KeyBinding::new(
        ctrl(KeyCode::Char('k')),
        &[ctrl(KeyCode::Char('d'))],
        CommandId::from_static("multiCursor.skipLastOccurrence"),
    ));
    let stack = KeymapStack::with_base(keymap);
    let mut resolver = KeymapResolver::new();

    assert_eq!(
        resolver.resolve(&stack, ctrl_press('k')),
        Resolution::matched(CommandId::from_static("lines.delete"))
    );
    assert!(!resolver.is_pending());
}

#[test]
fn a_suppressed_chord_falls_through_to_typing() {
    let mut base = Keymap::new("default");
    base.push(KeyBinding::new(
        ctrl(KeyCode::Char('c')),
        &[],
        CommandId::from_static("clipboard.copy"),
    ));
    let mut user = Keymap::new("user");
    user.push(KeyBinding::unbound(ctrl(KeyCode::Char('c')), &[]));
    let mut stack = KeymapStack::with_base(base);
    stack.push(user);

    let mut resolver = KeymapResolver::new();
    assert_eq!(
        resolver.resolve(&stack, ctrl_press('c')),
        Resolution::NoMatch
    );
}

#[test]
fn a_suppressed_sequence_aborts_rather_than_falling_through() {
    let mut base = Keymap::new("default");
    base.push(KeyBinding::new(
        ctrl(KeyCode::Char('k')),
        &[ctrl(KeyCode::Char('d'))],
        CommandId::from_static("multiCursor.skipLastOccurrence"),
    ));
    let mut user = Keymap::new("user");
    user.push(KeyBinding::unbound(
        ctrl(KeyCode::Char('k')),
        &[ctrl(KeyCode::Char('d'))],
    ));
    let mut stack = KeymapStack::with_base(base);
    stack.push(user);

    let mut resolver = KeymapResolver::new();
    // REGRESSION: the base layer's `Ctrl+K Ctrl+D` used to keep `Ctrl+K` pending
    // even though the user layer had unbound that very sequence, so the leader ate
    // the next keystroke and nothing could ever complete. Unbinding the leaf now
    // also releases the prefix: `Ctrl+K` is a plain unmatched chord.
    assert_eq!(
        resolver.resolve(&stack, ctrl_press('k')),
        Resolution::NoMatch
    );
    assert!(!resolver.is_pending());
    // A following key is therefore untouched, rather than swallowed by an
    // uncompletable sequence.
    assert_eq!(
        resolver.resolve(&stack, KeyPress::plain(KeyCode::Char('x'))),
        Resolution::NoMatch
    );
}

// ===== Modes =====

#[test]
fn resolution_is_scoped_to_the_active_mode() {
    let normal = ModeName::from_static("normal");
    let mut keymap = Keymap::new("modal");
    keymap.push(
        KeyBinding::new(
            StrokePattern::plain(KeyCode::Char('d')),
            &[StrokePattern::plain(KeyCode::Char('d'))],
            CommandId::from_static("lines.delete"),
        )
        .in_mode(normal.clone()),
    );
    let stack = KeymapStack::with_base(keymap);

    let mut resolver = KeymapResolver::new();
    // No mode active: the binding does not apply at all.
    assert_eq!(
        resolver.resolve(&stack, KeyPress::plain(KeyCode::Char('d'))),
        Resolution::NoMatch
    );

    resolver.set_mode(Some(normal.clone()));
    assert_eq!(resolver.mode(), Some(&normal));
    assert_eq!(
        resolver.resolve(&stack, KeyPress::plain(KeyCode::Char('d'))),
        Resolution::Pending
    );
    assert_eq!(
        resolver.resolve(&stack, KeyPress::plain(KeyCode::Char('d'))),
        Resolution::matched(CommandId::from_static("lines.delete"))
    );
}

#[test]
fn changing_mode_discards_a_pending_sequence() {
    let keymap = stack();
    let mut resolver = KeymapResolver::new();
    assert_eq!(
        resolver.resolve(&keymap, ctrl_press('k')),
        Resolution::Pending
    );

    resolver.set_mode(Some(ModeName::from_static("insert")));
    assert!(
        !resolver.is_pending(),
        "a prefix resolved in the old mode must not complete in the new one"
    );
}

#[test]
fn reset_clears_both_pending_state_and_mode() {
    let keymap = stack();
    let mut resolver = KeymapResolver::new();
    resolver.set_mode(Some(ModeName::from_static("normal")));
    assert_eq!(
        resolver.resolve(&keymap, ctrl_press('k')),
        Resolution::Pending
    );

    resolver.reset();
    assert!(!resolver.is_pending());
    assert!(resolver.mode().is_none());
}

// ===== Normalization =====

#[test]
fn resolution_normalizes_the_reported_character() {
    let keymap = stack();
    let mut resolver = KeymapResolver::new();
    // A host that reports the shifted letter for a Ctrl chord still matches.
    let upper = KeyPress::new(KeyCode::Char('C'), mods(false, true, false, false, false));
    assert_eq!(
        resolver.resolve(&keymap, upper),
        Resolution::matched(CommandId::from_static("clipboard.copy"))
    );
}

#[test]
fn key_press_from_event_drops_the_repeat_flag_only() {
    let event = KeyEvent {
        key: KeyCode::Char('C'),
        modifiers: mods(true, true, false, false, false),
        is_repeat: true,
    };
    let press = KeyPress::from_event(&event);
    assert_eq!(press.key, KeyCode::Char('C'));
    assert_eq!(press.modifiers, event.modifiers);
    assert_eq!(press.normalized().key, KeyCode::Char('c'));
    assert_eq!(
        press.normalized().modifiers,
        event.modifiers,
        "normalization must never touch modifiers"
    );
}

#[test]
fn a_default_resolver_matches_a_new_one() {
    let mut from_default = KeymapResolver::default();
    let keymap = stack();
    assert_eq!(
        from_default.resolve(&keymap, ctrl_press('c')),
        Resolution::matched(CommandId::from_static("clipboard.copy"))
    );
    assert_eq!(from_default.abort_stroke().key, KeyCode::Escape);
}
