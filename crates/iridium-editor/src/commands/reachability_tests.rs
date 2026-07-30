//! Tests for [`KeymapStack::binding_is_reachable`].
//!
//! The method answers one question — *does pressing this binding's own keys run
//! this binding?* — and everything here is a way for the answer to be "no": a
//! higher layer unbinds the sequence, a higher layer steals it, a sibling in the
//! same layer outranks it, or a shorter binding claims one of its prefixes and
//! fires before the sequence can finish. Each case gets a test, because each is a
//! distinct reason a palette would otherwise print a key that does nothing.
//!
//! The prefix case is the one that does not follow from `exact_match` alone, and
//! it is why `a_leader_bound_in_a_higher_layer_strands_the_chord_below_it` exists:
//! it failed against the first implementation, which compared whole sequences and
//! so could not see a shorter binding cutting in.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    CommandId, KeyBinding, Keymap, KeymapStack, ModeName, ModifierPattern, ModifierState,
    StrokeCapture, StrokePattern,
};
use crate::input::KeyCode;

/// `Ctrl` held, every other modifier released.
fn ctrl(key: KeyCode) -> StrokePattern {
    StrokePattern::new(
        key,
        ModifierPattern::NONE.with_ctrl(ModifierState::Required),
    )
}

/// `Ctrl` held, `Shift` irrelevant — the shape the real default keymap uses and
/// the one whose text form is unreadable.
fn ctrl_any_shift(key: KeyCode) -> StrokePattern {
    StrokePattern::new(
        key,
        ModifierPattern::NONE
            .with_ctrl(ModifierState::Required)
            .with_shift(ModifierState::Any),
    )
}

fn binding(stroke: StrokePattern, command: &'static str) -> KeyBinding {
    KeyBinding::new(stroke, &[], CommandId::from_static(command))
}

/// The single binding of the single layer of `stack`.
fn sole_binding(stack: &KeymapStack) -> &KeyBinding {
    &stack.layers()[0].bindings()[0]
}

/// The binding at `index` in layer `layer`.
fn binding_at(stack: &KeymapStack, layer: usize, index: usize) -> &KeyBinding {
    &stack.layers()[layer].bindings()[index]
}

#[test]
fn a_lone_binding_is_reachable() {
    let mut keymap = Keymap::new("base");
    keymap.push(binding(ctrl(KeyCode::Char('c')), "clipboard.copy"));
    let stack = KeymapStack::with_base(keymap);

    assert!(stack.binding_is_reachable(sole_binding(&stack)));
}

#[test]
fn a_binding_from_another_stack_is_never_reachable() {
    // Identity is by pointer, so a structurally identical binding that this
    // stack does not own must answer `false` rather than accidentally matching.
    let mut keymap = Keymap::new("base");
    keymap.push(binding(ctrl(KeyCode::Char('c')), "clipboard.copy"));
    let stack = KeymapStack::with_base(keymap);

    let outsider = binding(ctrl(KeyCode::Char('c')), "clipboard.copy");
    assert!(!stack.binding_is_reachable(&outsider));
}

#[test]
fn a_higher_layer_suppression_makes_a_binding_unreachable() {
    let mut base = Keymap::new("base");
    base.push(binding(ctrl(KeyCode::Char('c')), "clipboard.copy"));
    let mut stack = KeymapStack::with_base(base);

    assert!(stack.binding_is_reachable(sole_binding(&stack)));

    let mut user = Keymap::new("user");
    user.push(KeyBinding::unbound(ctrl(KeyCode::Char('c')), &[]));
    stack.push(user);

    assert!(!stack.binding_is_reachable(binding_at(&stack, 0, 0)));
}

#[test]
fn a_higher_layer_stealing_the_sequence_makes_the_lower_binding_unreachable() {
    let mut base = Keymap::new("base");
    base.push(binding(ctrl(KeyCode::Char('c')), "clipboard.copy"));
    let mut user = Keymap::new("user");
    user.push(binding(ctrl(KeyCode::Char('c')), "comment.toggleLine"));

    let mut stack = KeymapStack::with_base(base);
    stack.push(user);

    // The overridden binding is gone; the overriding one is what fires.
    assert!(!stack.binding_is_reachable(binding_at(&stack, 0, 0)));
    assert!(stack.binding_is_reachable(binding_at(&stack, 1, 0)));
}

#[test]
fn a_lower_layer_binding_survives_a_higher_layer_that_does_not_mention_it() {
    let mut base = Keymap::new("base");
    base.push(binding(ctrl(KeyCode::Char('c')), "clipboard.copy"));
    let mut user = Keymap::new("user");
    user.push(binding(ctrl(KeyCode::Char('x')), "clipboard.cut"));

    let mut stack = KeymapStack::with_base(base);
    stack.push(user);

    assert!(stack.binding_is_reachable(binding_at(&stack, 0, 0)));
    assert!(stack.binding_is_reachable(binding_at(&stack, 1, 0)));
}

#[test]
fn intra_layer_rank_loss_makes_the_outranked_binding_unreachable() {
    // Both fire on a bare `Ctrl+C`. `Keymap::exact_match` ranks by required
    // specificity first, so the one that pins `Shift` off beats the one that
    // ignores `Shift` — and only one of the two may be reported as the key that
    // runs its command.
    let mut keymap = Keymap::new("base");
    keymap.push(binding(
        ctrl_any_shift(KeyCode::Char('c')),
        "clipboard.copy",
    ));
    keymap.push(binding(ctrl(KeyCode::Char('c')), "clipboard.cut"));
    let stack = KeymapStack::with_base(keymap);

    let loose = binding_at(&stack, 0, 0);
    let tight = binding_at(&stack, 0, 1);
    assert_eq!(loose.command().unwrap().as_str(), "clipboard.copy");
    assert_eq!(tight.command().unwrap().as_str(), "clipboard.cut");

    assert!(!stack.binding_is_reachable(loose));
    assert!(stack.binding_is_reachable(tight));
}

#[test]
fn a_mode_scoped_binding_is_reachable_in_its_own_mode() {
    // The witness carries the binding's mode, so a modal binding is not reported
    // unreachable merely because the non-modal resolution ignores it.
    let mut keymap = Keymap::new("modal");
    keymap.push(
        binding(ctrl(KeyCode::Char('c')), "clipboard.copy")
            .in_mode(ModeName::from_static("normal")),
    );
    let stack = KeymapStack::with_base(keymap);

    assert!(stack.binding_is_reachable(sole_binding(&stack)));
}

#[test]
fn a_multi_stroke_binding_is_reachable() {
    let mut keymap = Keymap::new("base");
    keymap.push(KeyBinding::new(
        ctrl(KeyCode::Char('k')),
        &[ctrl(KeyCode::Char('d'))],
        CommandId::from_static("multiCursor.skipLastOccurrence"),
    ));
    let stack = KeymapStack::with_base(keymap);

    assert!(stack.binding_is_reachable(sole_binding(&stack)));
}

#[test]
fn a_leader_bound_in_a_higher_layer_strands_the_chord_below_it() {
    // The exact customization `KeymapStack`'s own documentation warns about:
    // binding a bare `Ctrl+K` makes every `Ctrl+K …` chord unreachable, because
    // the leader fires the instant it completes. `validate` rejects this, but a
    // stack assembled without validation must still report the truth.
    let mut base = Keymap::new("base");
    base.push(KeyBinding::new(
        ctrl(KeyCode::Char('k')),
        &[ctrl(KeyCode::Char('d'))],
        CommandId::from_static("multiCursor.skipLastOccurrence"),
    ));
    let mut user = Keymap::new("user");
    user.push(binding(ctrl(KeyCode::Char('k')), "palette.open"));

    let mut stack = KeymapStack::with_base(base);
    stack.push(user);

    assert!(!stack.binding_is_reachable(binding_at(&stack, 0, 0)));
    assert!(stack.binding_is_reachable(binding_at(&stack, 1, 0)));
}

#[test]
fn a_wildcard_binding_is_reachable_when_some_character_reaches_it() {
    let mut keymap = Keymap::new("modal");
    keymap.push(KeyBinding::new(
        StrokePattern::plain(KeyCode::Char('f')),
        &[StrokePattern::any_char(ModifierPattern::NONE)],
        CommandId::from_static("motion.findChar"),
    ));
    let stack = KeymapStack::with_base(keymap);

    let wildcard = sole_binding(&stack);
    assert_eq!(wildcard.sequence()[1].capture, StrokeCapture::AnyChar);
    assert!(stack.binding_is_reachable(wildcard));
}

#[test]
fn a_wildcard_survives_a_literal_binding_on_one_of_its_characters() {
    // A literal binding outranks a wildcard on the character it claims, so a
    // single fixed witness character would report this wildcard as dead. It is
    // not: every other character still reaches it.
    let mut keymap = Keymap::new("modal");
    keymap.push(KeyBinding::new(
        StrokePattern::plain(KeyCode::Char('f')),
        &[StrokePattern::any_char(ModifierPattern::NONE)],
        CommandId::from_static("motion.findChar"),
    ));
    keymap.push(KeyBinding::new(
        StrokePattern::plain(KeyCode::Char('f')),
        &[StrokePattern::plain(KeyCode::Char('a'))],
        CommandId::from_static("motion.findA"),
    ));
    let stack = KeymapStack::with_base(keymap);

    assert!(stack.binding_is_reachable(binding_at(&stack, 0, 0)));
    assert!(stack.binding_is_reachable(binding_at(&stack, 0, 1)));
}

#[test]
fn a_wildcard_smothered_on_every_character_is_unreachable() {
    // The complement of the test above, and the reason the witness list ends in
    // a character no realistic keymap binds: when every candidate *is* claimed,
    // the wildcard genuinely cannot fire and must be reported that way.
    let mut keymap = Keymap::new("modal");
    keymap.push(KeyBinding::new(
        StrokePattern::plain(KeyCode::Char('f')),
        &[StrokePattern::any_char(ModifierPattern::NONE)],
        CommandId::from_static("motion.findChar"),
    ));
    let mut smother = Keymap::new("smother");
    for candidate in ('a'..='z').chain('0'..='9').chain(std::iter::once('~')) {
        smother.push(KeyBinding::new(
            StrokePattern::plain(KeyCode::Char('f')),
            &[StrokePattern::plain(KeyCode::Char(candidate))],
            CommandId::from_static("command.noOp"),
        ));
    }

    let mut stack = KeymapStack::with_base(keymap);
    stack.push(smother);

    assert!(!stack.binding_is_reachable(binding_at(&stack, 0, 0)));
}

#[test]
fn every_binding_of_the_default_stack_is_reachable() {
    // The property that makes the whole index trustworthy: the shipped keymap
    // contains no binding that cannot fire. If this ever fails, a default
    // binding has been silently lost.
    let stack = super::default_keymap_stack();
    for (layer_index, layer) in stack.layers().iter().enumerate() {
        for entry in layer.bindings() {
            assert!(
                stack.binding_is_reachable(entry),
                "layer {layer_index} binding `{}` cannot fire",
                entry.display_sequence()
            );
        }
    }
}
