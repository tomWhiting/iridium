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

use super::builtin::default_registry;
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
    let registry = default_registry().unwrap();
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
    let registry = default_registry().unwrap();
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
    let registry = default_registry().unwrap();
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
    let registry = default_registry().unwrap();
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
    let registry = default_registry().unwrap();
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
    let registry = default_registry().unwrap();
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

/// ⛔ **The trap #91 opened.** Registering the panel's vocabulary in the kernel
/// made `"j" = "explorer.moveDown"` a *valid* binding — and a binding with no
/// mode applies in every mode, so a configuration file could bind a plain letter
/// to a command that can only ever run inside the file explorer. The letter then
/// stops typing in the document, and the command it names never runs, because
/// the editor is never in `explorer` mode.
///
/// Canonicalization is where a loaded layer is made to mean what it says, so it
/// is where the command's own declaration of its mode is applied.
#[test]
fn canonicalize_scopes_a_binding_to_the_mode_its_command_declares() {
    let registry = default_registry().unwrap();
    let mut keymap = Keymap::new("user");
    keymap.push(KeyBinding::new(
        StrokePattern::new(KeyCode::Char('j'), ModifierPattern::NONE),
        &[],
        CommandId::new(String::from("explorer.moveDown")),
    ));
    assert!(
        keymap.bindings()[0].mode().is_none(),
        "as the file wrote it"
    );

    keymap.canonicalize(&registry).unwrap();
    assert_eq!(
        keymap.bindings()[0].mode().map(ToString::to_string),
        Some("explorer".to_owned()),
        "a command that exists only in a mode may only be bound inside it"
    );
}

/// The other direction, and the reason this is not simply "always set a mode":
/// an editor command belongs to no mode, so a binding to one is left alone and
/// goes on applying everywhere — including inside a modal keymap that scopes it
/// deliberately.
#[test]
fn canonicalize_leaves_a_mode_free_command_mode_free() {
    let registry = default_registry().unwrap();
    let mut keymap = Keymap::new("user");
    keymap.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('c')),
        &[],
        CommandId::new(String::from("clipboard.copy")),
    ));

    keymap.canonicalize(&registry).unwrap();
    assert!(keymap.bindings()[0].mode().is_none());
}

/// ⚠️ **A mode the author wrote is never overwritten.** #113's modal keymaps
/// scope editor commands — which declare no mode of their own — into `normal`
/// and `insert`. Adopting the command's declaration must therefore fill a gap,
/// never replace an answer, or every such binding would silently escape its mode
/// the moment the layer was canonicalized.
#[test]
fn canonicalize_never_overwrites_a_mode_the_binding_already_names() {
    let registry = default_registry().unwrap();
    let normal = ModeName::from_static("normal");
    let mut keymap = Keymap::new("user");
    keymap.push(
        KeyBinding::new(
            StrokePattern::new(KeyCode::Char('d'), ModifierPattern::NONE),
            &[],
            CommandId::new(String::from("edit.deleteToLineEnd")),
        )
        .in_mode(normal.clone()),
    );
    // And a panel command scoped somewhere other than where its meta says, which
    // is the case where "fill a gap" and "replace an answer" actually differ.
    keymap.push(
        KeyBinding::new(
            StrokePattern::new(KeyCode::Char('k'), ModifierPattern::NONE),
            &[],
            CommandId::new(String::from("explorer.moveUp")),
        )
        .in_mode(normal.clone()),
    );

    keymap.canonicalize(&registry).unwrap();
    for binding in keymap.bindings() {
        assert_eq!(
            binding.mode(),
            Some(&normal),
            "`{}` was scoped by its author and must stay there",
            binding.display_sequence()
        );
    }
}

#[test]
fn canonicalize_skips_suppressions() {
    let registry = default_registry().unwrap();
    let mut keymap = Keymap::new("user");
    keymap.push(KeyBinding::unbound(ctrl_pattern(KeyCode::Char('c')), &[]));
    keymap
        .canonicalize(&registry)
        .expect("suppressions carry no id");
    assert!(keymap.bindings()[0].command().is_none());
}

#[test]
fn stack_validate_and_canonicalize_cover_every_layer() {
    let registry = default_registry().unwrap();
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
    let registry = default_registry().unwrap();
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
fn a_host_chord_under_the_defaults_ctrl_k_is_a_load_time_error() {
    // The price of binding the palette to a bare `Ctrl+K`, stated where someone
    // hitting it will find it. The default's complete binding fires the instant
    // the key goes down, so a host chord beneath it could never complete — and
    // that is reported at load rather than felt as a key that does nothing.
    let registry = default_registry().unwrap();
    let mut stack = KeymapStack::with_base(super::default_non_modal_keymap());
    let mut host = Keymap::new("host");
    host.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('k')),
        &[ctrl_pattern(KeyCode::Char('d'))],
        CommandId::from_static("multiCursor.skipLastOccurrence"),
    ));
    stack.push(host);

    match stack.validate(&registry) {
        Err(KeymapError::CrossLayerShadowedSequence {
            prefix_keymap,
            shadowed_keymap,
            shadowed,
            ..
        }) => {
            assert_eq!(prefix_keymap, "default");
            assert_eq!(shadowed_keymap, "host");
            assert_eq!(shadowed, "ctrl+k ctrl+d");
        },
        other => panic!("expected the palette binding to strand the chord, got {other:?}"),
    }
}

#[test]
fn a_host_may_take_ctrl_k_back_by_unbinding_it_first() {
    // The escape hatch, and the reason the error above is a diagnostic rather than
    // a wall: a host that wants the leader suppresses the default's binding in its
    // own layer, and the chord beneath it then validates and resolves.
    let registry = default_registry().unwrap();
    let mut stack = KeymapStack::with_base(super::default_non_modal_keymap());
    let mut host = Keymap::new("host");
    // A suppression cancels a binding it spells *identically* — `Keymap::suppresses`
    // compares whole sequences, patterns included. So unbinding the default's
    // `Ctrl+K` means reproducing its modifier pattern, `AltGraph: Any` and all,
    // rather than the tighter `NONE.with_ctrl(Required)` used elsewhere here.
    let palette_key = StrokePattern::new(
        KeyCode::Char('k'),
        ModifierPattern::new(
            ModifierState::Forbidden,
            ModifierState::Required,
            ModifierState::Forbidden,
            ModifierState::Forbidden,
            ModifierState::Any,
        ),
    );
    host.push(KeyBinding::unbound(palette_key, &[]));
    host.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('k')),
        &[ctrl_pattern(KeyCode::Char('d'))],
        CommandId::from_static("multiCursor.skipLastOccurrence"),
    ));
    stack.push(host);

    stack
        .validate(&registry)
        .expect("the suppression clears the default's claim on the leader");

    // And it genuinely resolves, rather than merely passing validation.
    let mut resolver = super::KeymapResolver::new();
    let ctrl = |key| {
        super::KeyPress::new(
            key,
            crate::input::Modifiers {
                shift: false,
                ctrl: true,
                alt: false,
                meta: false,
                alt_graph: false,
            },
        )
    };
    assert_eq!(
        resolver.resolve(&stack, ctrl(KeyCode::Char('k'))),
        super::Resolution::Pending
    );
    assert_eq!(
        resolver.resolve(&stack, ctrl(KeyCode::Char('d'))),
        super::Resolution::matched(CommandId::from_static("multiCursor.skipLastOccurrence"))
    );
}

#[test]
fn a_lower_layer_prefix_shadowing_a_higher_layer_sequence_is_reported_too() {
    // The defect is symmetric: the base layer's complete `Ctrl+K` fires before the
    // user layer's longer sequence can complete, so the user's binding is the dead
    // one. Both directions must be caught, or half the failures stay silent.
    let registry = default_registry().unwrap();
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
    let registry = default_registry().unwrap();
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
    let registry = default_registry().unwrap();
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
    let registry = default_registry().unwrap();
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
