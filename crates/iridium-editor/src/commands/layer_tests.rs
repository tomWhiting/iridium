//! Layer-stack tests: overriding, suppression, and prefix continuations across
//! layers.

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
    CommandId, KeyBinding, KeyPress, Keymap, KeymapStack, ModifierPattern, ModifierState,
    StrokePattern,
};
use crate::input::{KeyCode, Modifiers};

const fn mods(shift: bool, ctrl: bool, alt: bool, meta: bool, alt_graph: bool) -> Modifiers {
    Modifiers {
        shift,
        ctrl,
        alt,
        meta,
        alt_graph,
    }
}

fn press(key: KeyCode, modifiers: Modifiers) -> Vec<KeyPress> {
    vec![KeyPress::new(key, modifiers)]
}

fn ctrl_pattern(key: KeyCode) -> StrokePattern {
    StrokePattern::new(
        key,
        ModifierPattern::NONE.with_ctrl(ModifierState::Required),
    )
}

// ===== Layering =====

fn base_layer() -> Keymap {
    let mut keymap = Keymap::new("default");
    keymap.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('c')),
        &[],
        CommandId::from_static("clipboard.copy"),
    ));
    keymap.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('k')),
        &[ctrl_pattern(KeyCode::Char('d'))],
        CommandId::from_static("multiCursor.skipLastOccurrence"),
    ));
    keymap
}

#[test]
fn a_user_layer_overrides_only_the_sequences_it_mentions() {
    let mut stack = KeymapStack::with_base(base_layer());
    let mut user = Keymap::new("user");
    user.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('c')),
        &[],
        CommandId::from_static("comment.toggleLine"),
    ));
    stack.push(user);

    assert_eq!(stack.len(), 2);
    assert_eq!(
        stack
            .exact_match(
                &press(KeyCode::Char('c'), mods(false, true, false, false, false)),
                None
            )
            .unwrap()
            .command()
            .unwrap()
            .as_str(),
        "comment.toggleLine"
    );
    // The untouched sequence still resolves through the base layer.
    let sequence = vec![
        KeyPress::new(KeyCode::Char('k'), mods(false, true, false, false, false)),
        KeyPress::new(KeyCode::Char('d'), mods(false, true, false, false, false)),
    ];
    assert_eq!(
        stack
            .exact_match(&sequence, None)
            .unwrap()
            .command()
            .unwrap()
            .as_str(),
        "multiCursor.skipLastOccurrence"
    );
}

#[test]
fn a_higher_layer_can_suppress_a_default_binding() {
    let mut stack = KeymapStack::with_base(base_layer());
    let mut user = Keymap::new("user");
    user.push(KeyBinding::unbound(ctrl_pattern(KeyCode::Char('c')), &[]));
    stack.push(user);

    let binding = stack
        .exact_match(
            &press(KeyCode::Char('c'), mods(false, true, false, false, false)),
            None,
        )
        .expect("the suppression itself matches");
    assert!(
        binding.command().is_none(),
        "a suppression must not fall through to the lower layer"
    );
}

#[test]
fn a_pending_prefix_is_kept_alive_by_any_layer() {
    let mut stack = KeymapStack::with_base(base_layer());
    let mut user = Keymap::new("user");
    user.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('k')),
        &[ctrl_pattern(KeyCode::Char('z'))],
        CommandId::from_static("user.chord"),
    ));
    stack.push(user);

    let ctrl_k = press(KeyCode::Char('k'), mods(false, true, false, false, false));
    assert!(stack.has_continuation(&ctrl_k, None));

    // Both continuations remain reachable: the user layer did not destroy the
    // default's Ctrl+K Ctrl+D by binding a different second key.
    for (second, expected) in [('d', "multiCursor.skipLastOccurrence"), ('z', "user.chord")] {
        let sequence = vec![
            KeyPress::new(KeyCode::Char('k'), mods(false, true, false, false, false)),
            KeyPress::new(
                KeyCode::Char(second),
                mods(false, true, false, false, false),
            ),
        ];
        assert_eq!(
            stack
                .exact_match(&sequence, None)
                .unwrap()
                .command()
                .unwrap()
                .as_str(),
            expected
        );
    }
}

#[test]
fn stack_pop_and_clear_restore_lower_layers() {
    let mut stack = KeymapStack::new();
    assert!(stack.is_empty());
    stack.push(base_layer());
    let mut user = Keymap::new("user");
    user.push(KeyBinding::unbound(ctrl_pattern(KeyCode::Char('c')), &[]));
    stack.push(user);

    let popped = stack.pop().expect("user layer");
    assert_eq!(popped.name(), "user");
    assert_eq!(
        stack
            .exact_match(
                &press(KeyCode::Char('c'), mods(false, true, false, false, false)),
                None
            )
            .unwrap()
            .command()
            .unwrap()
            .as_str(),
        "clipboard.copy"
    );
    stack.clear();
    assert!(stack.is_empty());
    assert!(stack.layers().is_empty());
}

// ===== Continuations =====

#[test]
fn has_continuation_is_true_only_for_a_live_prefix() {
    let keymap = base_layer();
    let ctrl_k = press(KeyCode::Char('k'), mods(false, true, false, false, false));
    let ctrl_c = press(KeyCode::Char('c'), mods(false, true, false, false, false));
    assert!(keymap.has_continuation(&ctrl_k, None));
    assert!(!keymap.has_continuation(&ctrl_c, None));
}

#[test]
fn a_suppressed_longer_sequence_does_not_hold_a_prefix_open() {
    let mut keymap = Keymap::new("test");
    keymap.push(KeyBinding::unbound(
        ctrl_pattern(KeyCode::Char('k')),
        &[ctrl_pattern(KeyCode::Char('d'))],
    ));
    let ctrl_k = press(KeyCode::Char('k'), mods(false, true, false, false, false));
    assert!(!keymap.has_continuation(&ctrl_k, None));
}

#[test]
fn unbinding_a_chord_releases_its_prefix_across_layers() {
    // REGRESSION: `has_continuation` consulted every layer while suppression only
    // took effect in `exact_match`, so unbinding `Ctrl+K Ctrl+D` removed the leaf
    // and left `Ctrl+K` pending forever — a leader that could never complete and
    // that swallowed whatever the user typed next. `KeyBinding::unbound` documents
    // the sequence as treated "exactly as if it had never been bound", and for a
    // multi-stroke sequence that has to include the prefix.
    let mut stack = KeymapStack::with_base(base_layer());
    let ctrl_k = press(KeyCode::Char('k'), mods(false, true, false, false, false));
    assert!(stack.has_continuation(&ctrl_k, None));

    let mut user = Keymap::new("user");
    user.push(KeyBinding::unbound(
        ctrl_pattern(KeyCode::Char('k')),
        &[ctrl_pattern(KeyCode::Char('d'))],
    ));
    stack.push(user);

    assert!(
        !stack.has_continuation(&ctrl_k, None),
        "the only continuation was unbound, so the prefix must not stay live"
    );
}

#[test]
fn unbinding_one_chord_leaves_a_sibling_chord_holding_the_prefix() {
    // Suppression matches a sequence exactly, so it must not release a prefix that
    // another live sequence still needs.
    let mut base = base_layer();
    base.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('k')),
        &[ctrl_pattern(KeyCode::Char('u'))],
        CommandId::from_static("multiCursor.removeLastCursor"),
    ));
    let mut stack = KeymapStack::with_base(base);
    let mut user = Keymap::new("user");
    user.push(KeyBinding::unbound(
        ctrl_pattern(KeyCode::Char('k')),
        &[ctrl_pattern(KeyCode::Char('d'))],
    ));
    stack.push(user);

    let ctrl_k = press(KeyCode::Char('k'), mods(false, true, false, false, false));
    assert!(
        stack.has_continuation(&ctrl_k, None),
        "ctrl+k ctrl+u is still bound, so the prefix stays live"
    );
}

#[test]
fn a_suppression_in_a_lower_layer_does_not_silence_a_higher_one() {
    // Suppression flows downwards only. A layer pushed *after* a suppression is
    // more specific and must win, or a keymap could not be re-enabled by layering.
    let mut stack = KeymapStack::with_base(base_layer());
    let mut disable = Keymap::new("disable");
    disable.push(KeyBinding::unbound(
        ctrl_pattern(KeyCode::Char('k')),
        &[ctrl_pattern(KeyCode::Char('d'))],
    ));
    stack.push(disable);
    let mut reenable = Keymap::new("reenable");
    reenable.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('k')),
        &[ctrl_pattern(KeyCode::Char('d'))],
        CommandId::from_static("comment.toggleLine"),
    ));
    stack.push(reenable);

    let ctrl_k = press(KeyCode::Char('k'), mods(false, true, false, false, false));
    assert!(stack.has_continuation(&ctrl_k, None));
    let sequence = vec![
        KeyPress::new(KeyCode::Char('k'), mods(false, true, false, false, false)),
        KeyPress::new(KeyCode::Char('d'), mods(false, true, false, false, false)),
    ];
    assert_eq!(
        stack
            .exact_match(&sequence, None)
            .unwrap()
            .command()
            .unwrap()
            .as_str(),
        "comment.toggleLine"
    );
}

// ===== Replacing a layer in place — the configuration-reload primitive =====

/// A layer of `name` binding `Ctrl+<key>` to `command`.
fn named_layer(name: &'static str, key: KeyCode, command: &'static str) -> Keymap {
    let mut keymap = Keymap::new(name);
    keymap.push(KeyBinding::new(
        ctrl_pattern(key),
        &[],
        CommandId::from_static(command),
    ));
    keymap
}

/// The command a stack resolves `Ctrl+<key>` to, if any.
fn resolves(stack: &KeymapStack, key: KeyCode) -> Option<String> {
    stack
        .exact_match(&press(key, mods(false, true, false, false, false)), None)
        .and_then(|binding| binding.command().map(|id| id.as_str().to_owned()))
}

#[test]
fn a_replaced_layer_keeps_its_position_in_the_stack() {
    // Three layers, and the one replaced is the *middle* one — the case that
    // separates a real replacement from pop-and-push, which would move it to
    // the top and hand it precedence it never had.
    let mut stack = KeymapStack::new();
    stack.push(named_layer("default", KeyCode::Char('a'), "kernel.a"));
    stack.push(named_layer("face", KeyCode::Char('b'), "face.b"));
    stack.push(named_layer("user", KeyCode::Char('b'), "user.b"));

    let previous = stack
        .replace_named(named_layer("face", KeyCode::Char('b'), "face.edited"))
        .expect("there is a layer called `face`");

    assert_eq!(previous.name(), "face", "the old layer came back");
    assert_eq!(stack.len(), 3, "replacing must not add a layer");
    assert_eq!(
        stack.layers()[1].name(),
        "face",
        "and it is still in the middle"
    );
    assert_eq!(
        resolves(&stack, KeyCode::Char('b')).as_deref(),
        Some("user.b"),
        "the user layer still outranks the face layer it sits above"
    );
}

/// ⭐ The behaviour a reload exists for: a binding deleted from the file goes.
#[test]
fn a_binding_missing_from_the_replacement_stops_resolving() {
    let mut stack = KeymapStack::new();
    stack.push(named_layer("user", KeyCode::Char('s'), "user.save"));
    assert_eq!(
        resolves(&stack, KeyCode::Char('s')).as_deref(),
        Some("user.save")
    );

    stack.replace_named(named_layer("user", KeyCode::Char('q'), "user.quit"));

    assert_eq!(
        resolves(&stack, KeyCode::Char('q')).as_deref(),
        Some("user.quit")
    );
    assert_eq!(
        resolves(&stack, KeyCode::Char('s')),
        None,
        "a push would have left the old version of the layer underneath"
    );
}

#[test]
fn replacing_a_name_no_layer_bears_changes_nothing() {
    let mut stack = KeymapStack::new();
    stack.push(named_layer("default", KeyCode::Char('a'), "kernel.a"));

    assert!(
        stack
            .replace_named(named_layer("user", KeyCode::Char('b'), "user.b"))
            .is_none(),
        "there is no layer called `user` to replace"
    );
    assert_eq!(stack.len(), 1, "and nothing was installed in its place");
    assert_eq!(resolves(&stack, KeyCode::Char('b')), None);
}

/// Duplicate names are a caller's bug; the behaviour under one is still
/// defined rather than incidental. The **highest-precedence** match is the one
/// replaced, because that is the layer deciding what those chords do today.
#[test]
fn the_highest_precedence_match_is_the_one_replaced() {
    let mut stack = KeymapStack::new();
    stack.push(named_layer("user", KeyCode::Char('a'), "lower.a"));
    stack.push(named_layer("user", KeyCode::Char('b'), "upper.b"));

    let previous = stack
        .replace_named(named_layer("user", KeyCode::Char('c'), "fresh.c"))
        .expect("a layer called `user` is there twice");

    assert_eq!(
        resolves(&stack, KeyCode::Char('b')),
        None,
        "the top layer is the one that went"
    );
    assert_eq!(
        resolves(&stack, KeyCode::Char('a')).as_deref(),
        Some("lower.a"),
        "and the one beneath it is untouched"
    );
    assert_eq!(
        previous.bindings().len(),
        1,
        "the layer handed back is the one that was replaced"
    );
}
