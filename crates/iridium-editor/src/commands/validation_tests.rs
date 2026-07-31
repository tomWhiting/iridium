//! Keymap validation tests: unknown commands, unreachable bindings, id
//! canonicalization, and the modifier-pattern algebra those rest on.

// `fn_params_excessive_bools` is allowed for the `mods` helper only: its five
// parameters mirror the five hardware modifier bits of `Modifiers` one for one,
// exactly as that struct's own documented allow does.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::fn_params_excessive_bools
)]

use super::builtin::builtin_registry;
use super::{
    CommandId, CommandRegistry, KeyBinding, Keymap, KeymapError, KeymapStack, ModeName,
    ModifierPattern, ModifierState, StrokePattern,
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

fn ctrl_pattern(key: KeyCode) -> StrokePattern {
    StrokePattern::new(
        key,
        ModifierPattern::NONE.with_ctrl(ModifierState::Required),
    )
}

// ===== Validation =====

#[test]
fn validate_rejects_an_unknown_command_id() {
    let registry = builtin_registry().unwrap();
    let mut keymap = Keymap::new("user");
    keymap.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('q')),
        &[],
        CommandId::from_static("does.notExist"),
    ));

    let error = keymap.validate(&registry).unwrap_err();
    assert_eq!(
        error,
        KeymapError::UnknownCommand {
            keymap: "user".to_owned(),
            id: "does.notExist".to_owned()
        }
    );
}

#[test]
fn validate_rejects_a_sequence_shadowed_by_a_complete_prefix() {
    let mut registry = CommandRegistry::new();
    registry
        .register_all(super::builtin::builtin_commands())
        .unwrap();

    let mut keymap = Keymap::new("user");
    keymap.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('k')),
        &[],
        CommandId::from_static("lines.delete"),
    ));
    keymap.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('k')),
        &[ctrl_pattern(KeyCode::Char('d'))],
        CommandId::from_static("multiCursor.skipLastOccurrence"),
    ));

    let error = keymap.validate(&registry).unwrap_err();
    match error {
        KeymapError::ShadowedSequence {
            keymap: name,
            prefix,
            shadowed,
        } => {
            assert_eq!(name, "user");
            assert_eq!(prefix, "ctrl+k");
            assert_eq!(shadowed, "ctrl+k ctrl+d");
        },
        other => panic!("expected a shadowing error, got {other:?}"),
    }
}

#[test]
fn incompatible_modifiers_do_not_count_as_shadowing() {
    let registry = builtin_registry().unwrap();
    let mut keymap = Keymap::new("user");
    // Ctrl+Shift+K is complete; Ctrl+K (Shift forbidden) starts a sequence.
    // They cannot both match one keypress, so neither shadows the other.
    keymap.push(KeyBinding::new(
        StrokePattern::new(
            KeyCode::Char('k'),
            ModifierPattern::NONE
                .with_ctrl(ModifierState::Required)
                .with_shift(ModifierState::Required),
        ),
        &[],
        CommandId::from_static("lines.delete"),
    ));
    keymap.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('k')),
        &[ctrl_pattern(KeyCode::Char('d'))],
        CommandId::from_static("multiCursor.skipLastOccurrence"),
    ));

    keymap.validate(&registry).expect("no shadowing");
}

#[test]
fn a_suppression_never_shadows() {
    let registry = builtin_registry().unwrap();
    let mut keymap = Keymap::new("user");
    keymap.push(KeyBinding::unbound(ctrl_pattern(KeyCode::Char('k')), &[]));
    keymap.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('k')),
        &[ctrl_pattern(KeyCode::Char('d'))],
        CommandId::from_static("multiCursor.skipLastOccurrence"),
    ));
    keymap
        .validate(&registry)
        .expect("suppressions are not prefixes");
}

#[test]
fn a_mode_scoped_prefix_only_shadows_inside_its_mode() {
    let registry = builtin_registry().unwrap();
    let normal = ModeName::from_static("normal");
    let insert = ModeName::from_static("insert");

    let mut clashing = Keymap::new("clashing");
    clashing.push(
        KeyBinding::new(
            ctrl_pattern(KeyCode::Char('k')),
            &[],
            CommandId::from_static("lines.delete"),
        )
        .in_mode(normal.clone()),
    );
    clashing.push(
        KeyBinding::new(
            ctrl_pattern(KeyCode::Char('k')),
            &[ctrl_pattern(KeyCode::Char('d'))],
            CommandId::from_static("multiCursor.skipLastOccurrence"),
        )
        .in_mode(normal),
    );
    assert!(matches!(
        clashing.validate(&registry),
        Err(KeymapError::ShadowedSequence { .. })
    ));

    let mut separated = Keymap::new("separated");
    separated.push(
        KeyBinding::new(
            ctrl_pattern(KeyCode::Char('k')),
            &[],
            CommandId::from_static("lines.delete"),
        )
        .in_mode(insert),
    );
    separated.push(
        KeyBinding::new(
            ctrl_pattern(KeyCode::Char('k')),
            &[ctrl_pattern(KeyCode::Char('d'))],
            CommandId::from_static("multiCursor.skipLastOccurrence"),
        )
        .in_mode(ModeName::from_static("normal")),
    );
    separated
        .validate(&registry)
        .expect("different modes do not clash");
}

// ===== Canonicalization =====

#[test]
fn canonicalize_interns_owned_ids_against_the_registry() {
    let registry = builtin_registry().unwrap();
    let mut keymap = Keymap::new("user");
    keymap.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('c')),
        &[],
        CommandId::new(String::from("clipboard.copy")),
    ));
    assert!(!keymap.bindings()[0].command().unwrap().is_static());

    keymap.canonicalize(&registry).unwrap();
    let id = keymap.bindings()[0].command().unwrap();
    assert!(id.is_static(), "canonicalized ids must be allocation free");
    assert_eq!(id.as_str(), "clipboard.copy");
}

#[test]
fn canonicalize_reports_an_unknown_id() {
    let registry = builtin_registry().unwrap();
    let mut keymap = Keymap::new("user");
    keymap.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('c')),
        &[],
        CommandId::new(String::from("nope.missing")),
    ));

    let error = keymap.canonicalize(&registry).unwrap_err();
    assert_eq!(
        error,
        KeymapError::UnknownCommand {
            keymap: "user".to_owned(),
            id: "nope.missing".to_owned()
        }
    );
}

#[test]
fn canonicalize_skips_suppressions() {
    let registry = builtin_registry().unwrap();
    let mut keymap = Keymap::new("user");
    keymap.push(KeyBinding::unbound(ctrl_pattern(KeyCode::Char('c')), &[]));
    keymap
        .canonicalize(&registry)
        .expect("suppressions carry no id");
    assert!(keymap.bindings()[0].command().is_none());
}

#[test]
fn stack_validate_and_canonicalize_cover_every_layer() {
    let registry = builtin_registry().unwrap();
    let mut stack = KeymapStack::with_base(super::default_non_modal_keymap());
    let mut user = Keymap::new("user");
    user.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('q')),
        &[],
        CommandId::new(String::from("clipboard.copy")),
    ));
    stack.push(user);

    stack.validate(&registry).expect("both layers are valid");
    stack.canonicalize(&registry).unwrap();
    for layer in stack.layers() {
        for binding in layer.bindings() {
            if let Some(id) = binding.command() {
                assert!(
                    id.is_static(),
                    "`{id}` not interned in layer `{}`",
                    layer.name()
                );
            }
        }
    }
}

// ===== Modifier pattern algebra =====

#[test]
fn specificity_counts_constrained_modifiers() {
    assert_eq!(ModifierPattern::ANY.specificity(), 0);
    assert_eq!(ModifierPattern::NONE.specificity(), 5);
    assert_eq!(
        ModifierPattern::ANY
            .with_ctrl(ModifierState::Required)
            .specificity(),
        1
    );
}

#[test]
fn exact_pattern_round_trips_a_modifier_set() {
    let held = mods(true, true, false, false, false);
    let pattern = ModifierPattern::exact(held);
    assert!(pattern.matches(held));
    assert!(!pattern.matches(mods(false, true, false, false, false)));
    assert_eq!(pattern.specificity(), 5);
}

#[test]
fn overlaps_is_false_only_where_required_meets_forbidden() {
    let required = ModifierPattern::ANY.with_shift(ModifierState::Required);
    let forbidden = ModifierPattern::ANY.with_shift(ModifierState::Forbidden);
    let ignored = ModifierPattern::ANY;
    assert!(!required.overlaps(&forbidden));
    assert!(required.overlaps(&ignored));
    assert!(forbidden.overlaps(&ignored));
    assert!(required.overlaps(&required));
}

// ===== Cross-layer validation =====

#[test]
fn stack_validate_reports_a_base_chord_stranded_by_a_user_prefix() {
    // REGRESSION: `KeymapStack::validate` validated each layer in isolation, so the
    // single commonest user customization — binding the bare chord leader — passed
    // validation while permanently stranding every sequence under it. Resolution
    // tests `exact_match` before `has_continuation`, so the shorter binding wins
    // from the first keypress and the longer one can never run. This is exactly what
    // `ShadowedSequence` exists to report; it was unreachable for the one place a
    // user keymap actually lives.
    //
    // The base layer is constructed here rather than taken from
    // `default_non_modal_keymap`, which no longer binds any chord: `Ctrl+K` is now
    // reserved as the command-palette leader, and a bare binding cannot coexist
    // with a sequence under it — which is the very rule this test pins. Building
    // the pair explicitly keeps the test about `validate` instead of about
    // whichever binding happens to be a chord.
    let registry = builtin_registry().unwrap();
    let mut base = Keymap::new("base");
    base.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('k')),
        &[ctrl_pattern(KeyCode::Char('d'))],
        CommandId::from_static("multiCursor.skipLastOccurrence"),
    ));
    let mut stack = KeymapStack::with_base(base);
    stack.validate(&registry).expect("the base alone is valid");

    let mut user = Keymap::new("user");
    user.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('k')),
        &[],
        CommandId::from_static("lines.delete"),
    ));
    stack.push(user);

    match stack.validate(&registry) {
        Err(KeymapError::CrossLayerShadowedSequence {
            prefix_keymap,
            prefix,
            shadowed_keymap,
            shadowed,
        }) => {
            assert_eq!(prefix_keymap, "user");
            assert_eq!(prefix, "ctrl+k");
            assert_eq!(shadowed_keymap, "base");
            assert_eq!(shadowed, "ctrl+k ctrl+d");
        },
        other => panic!("expected a cross-layer shadowing error, got {other:?}"),
    }
}

#[test]
fn the_default_keymap_leaves_ctrl_k_free_for_a_host_chord() {
    // The other half of reserving `Ctrl+K`: because the default binds nothing on
    // it, a host layer may now bind either a bare `Ctrl+K` *or* a chord under it
    // and validation accepts both. Before, the default's own chord made the bare
    // binding a load-time error.
    let registry = builtin_registry().unwrap();

    for continuation in [Vec::new(), vec![ctrl_pattern(KeyCode::Char('d'))]] {
        let mut stack = KeymapStack::with_base(super::default_non_modal_keymap());
        let mut host = Keymap::new("host");
        host.push(KeyBinding::new(
            ctrl_pattern(KeyCode::Char('k')),
            &continuation,
            CommandId::from_static("multiCursor.skipLastOccurrence"),
        ));
        stack.push(host);

        stack
            .validate(&registry)
            .expect("nothing in the default claims the Ctrl+K prefix");
    }
}

#[test]
fn a_lower_layer_prefix_shadowing_a_higher_layer_sequence_is_reported_too() {
    // The defect is symmetric: the base layer's complete `Ctrl+K` fires before the
    // user layer's longer sequence can complete, so the user's binding is the dead
    // one. Both directions must be caught, or half the failures stay silent.
    let registry = builtin_registry().unwrap();
    let mut base = Keymap::new("base");
    base.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('k')),
        &[],
        CommandId::from_static("lines.delete"),
    ));
    let mut stack = KeymapStack::with_base(base);
    let mut user = Keymap::new("user");
    user.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('k')),
        &[ctrl_pattern(KeyCode::Char('d'))],
        CommandId::from_static("multiCursor.skipLastOccurrence"),
    ));
    stack.push(user);

    match stack.validate(&registry) {
        Err(KeymapError::CrossLayerShadowedSequence {
            prefix_keymap,
            shadowed_keymap,
            ..
        }) => {
            assert_eq!(prefix_keymap, "base");
            assert_eq!(shadowed_keymap, "user");
        },
        other => panic!("expected a cross-layer shadowing error, got {other:?}"),
    }
}

#[test]
fn unbinding_the_stranded_sequence_clears_the_cross_layer_diagnostic() {
    // The documented remedy has to actually work, or the diagnostic is a wall.
    let registry = builtin_registry().unwrap();
    let mut stack = KeymapStack::with_base(super::default_non_modal_keymap());
    let mut user = Keymap::new("user");
    user.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('k')),
        &[],
        CommandId::from_static("lines.delete"),
    ));
    // The diagnostic's own text names the sequence to unbind, and parses back.
    let (first, rest) =
        KeyBinding::parse_sequence("ctrl+~altgraph+k ctrl+~altgraph+d").expect("parses");
    user.push(KeyBinding::unbound(first, &rest));
    stack.push(user);

    stack
        .validate(&registry)
        .expect("the stranded sequence was removed, so nothing is stranded");
}

#[test]
fn incompatible_modifiers_do_not_count_as_cross_layer_shadowing() {
    // The same overlap rule as within a layer: `Ctrl+Shift+K` and a `Ctrl+K` prefix
    // that forbids Shift cannot both match one keypress.
    let registry = builtin_registry().unwrap();
    let mut base = Keymap::new("base");
    base.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('k')),
        &[ctrl_pattern(KeyCode::Char('d'))],
        CommandId::from_static("multiCursor.skipLastOccurrence"),
    ));
    let mut stack = KeymapStack::with_base(base);
    let mut user = Keymap::new("user");
    user.push(KeyBinding::new(
        StrokePattern::new(
            KeyCode::Char('k'),
            ModifierPattern::NONE
                .with_ctrl(ModifierState::Required)
                .with_shift(ModifierState::Required),
        ),
        &[],
        CommandId::from_static("lines.delete"),
    ));
    stack.push(user);

    stack.validate(&registry).expect("no overlap, no shadowing");
}

#[test]
fn a_mode_scoped_prefix_does_not_shadow_across_layers_outside_its_mode() {
    let registry = builtin_registry().unwrap();
    let mut base = Keymap::new("base");
    base.push(
        KeyBinding::new(
            ctrl_pattern(KeyCode::Char('k')),
            &[ctrl_pattern(KeyCode::Char('d'))],
            CommandId::from_static("multiCursor.skipLastOccurrence"),
        )
        .in_mode(ModeName::from_static("normal")),
    );
    let mut stack = KeymapStack::with_base(base);
    let mut user = Keymap::new("user");
    user.push(
        KeyBinding::new(
            ctrl_pattern(KeyCode::Char('k')),
            &[],
            CommandId::from_static("lines.delete"),
        )
        .in_mode(ModeName::from_static("insert")),
    );
    stack.push(user);

    stack
        .validate(&registry)
        .expect("different modes never collide");
}
