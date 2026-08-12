//! Tests for the command-to-key reverse index.
//!
//! Two properties carry most of the weight, and both are asserted over the real
//! shipped keymap rather than a fixture:
//!
//! - **The oracle** — every hint, typed as its label describes, resolves to the
//!   binding it came from. That single check subsumes suppression, cross-layer
//!   override, intra-layer rank loss and prefix stranding, so a new way to lose a
//!   binding is caught without a new test.
//! - **Readability** — no label contains a `~`, the marker of the round-trippable
//!   form leaking into a human-facing string.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{KeyHint, KeyHintIndex, KeyLabelStyle};
use crate::commands::builtin::{
    EDIT_INSERT_CHARACTER, MULTI_CURSOR_SKIP_LAST_OCCURRENCE, SELECTION_SELECT_ALL,
};
use crate::commands::{
    CommandId, KeyBinding, KeyPress, Keymap, KeymapStack, ModeName, ModifierPattern, ModifierState,
    StrokePattern, default_keymap_stack,
};
use crate::input::{KeyCode, Modifiers};

/// `Ctrl` held, nothing else.
fn ctrl(key: KeyCode) -> StrokePattern {
    StrokePattern::new(
        key,
        ModifierPattern::NONE.with_ctrl(ModifierState::Required),
    )
}

fn binding(stroke: StrokePattern, command: &'static str) -> KeyBinding {
    KeyBinding::new(stroke, &[], CommandId::from_static(command))
}

fn layer_with(name: &'static str, bindings: Vec<KeyBinding>) -> Keymap {
    let mut keymap = Keymap::new(name);
    for entry in bindings {
        keymap.push(entry);
    }
    keymap
}

/// The label a command's best hint renders to, in the portable style.
fn label_of(index: &KeyHintIndex, id: &str) -> String {
    index
        .primary_hint(id)
        .unwrap_or_else(|| panic!("`{id}` has no hint"))
        .label(KeyLabelStyle::Portable)
        .to_owned()
}

/// The keypresses a hint's own strokes describe: required modifiers held, the
/// rest released.
fn presses_for(hint: &KeyHint) -> Vec<KeyPress> {
    hint.sequence()
        .iter()
        .map(|stroke| {
            KeyPress::new(
                stroke.key,
                Modifiers {
                    shift: stroke.modifiers.shift == ModifierState::Required,
                    ctrl: stroke.modifiers.ctrl == ModifierState::Required,
                    alt: stroke.modifiers.alt == ModifierState::Required,
                    meta: stroke.modifiers.meta == ModifierState::Required,
                    alt_graph: stroke.modifiers.alt_graph == ModifierState::Required,
                },
            )
        })
        .collect()
}

// ===== The oracle =====

#[test]
fn every_hint_resolves_to_the_command_it_claims() {
    let stack = default_keymap_stack();
    let index = KeyHintIndex::build(&stack);

    for id in index.command_ids() {
        for hint in index.hints_for(id.as_str()) {
            // A wildcard's key is a placeholder, so its literal strokes are not a
            // typeable sequence; those are covered in `reachability_tests`.
            if hint.is_wildcard() {
                continue;
            }
            let presses = presses_for(hint);
            let winner = stack.exact_match(&presses, hint.mode()).unwrap_or_else(|| {
                panic!(
                    "hint `{}` for `{id}` resolves to nothing",
                    hint.label(KeyLabelStyle::Portable)
                )
            });
            assert_eq!(
                winner.command(),
                Some(id),
                "hint `{}` claims `{id}` but runs something else",
                hint.label(KeyLabelStyle::Portable)
            );
        }
    }
}

#[test]
fn no_prefix_of_a_hint_fires_before_it() {
    // The failure mode a whole-sequence check cannot see: a shorter binding on a
    // prefix consumes the first keystroke and the chord never completes.
    let stack = default_keymap_stack();
    let index = KeyHintIndex::build(&stack);

    for id in index.command_ids() {
        for hint in index.hints_for(id.as_str()) {
            let presses = presses_for(hint);
            for length in 1..presses.len() {
                assert!(
                    stack.exact_match(&presses[..length], hint.mode()).is_none(),
                    "`{}` for `{id}` is stranded: its {length}-stroke prefix already fires",
                    hint.label(KeyLabelStyle::Portable)
                );
            }
        }
    }
}

#[test]
fn every_bound_command_of_the_default_keymap_has_a_hint() {
    let stack = default_keymap_stack();
    let index = KeyHintIndex::build(&stack);

    for layer in stack.layers() {
        for entry in layer.bindings() {
            let Some(command) = entry.command() else {
                continue;
            };
            assert!(
                !index.hints_for(command.as_str()).is_empty(),
                "`{command}` is bound but has no hint"
            );
        }
    }
}

// ===== Readability =====

#[test]
fn labels_drop_ignored_modifiers() {
    // The regression this module exists to prevent. `Select All` is bound with
    // `Shift` and `AltGraph` both ignored, so its round-trippable form is
    // `ctrl+~shift+~altgraph+a` — which is what a palette would have shown.
    let stack = default_keymap_stack();
    let index = KeyHintIndex::build(&stack);

    let hint = index
        .primary_hint(SELECTION_SELECT_ALL.as_str())
        .expect("select all is bound by default");

    assert_eq!(hint.label(KeyLabelStyle::Portable), "Ctrl+A");
    assert!(
        hint.binding_text().contains('~'),
        "this test is only meaningful while the binding really does ignore a \
         modifier; if it no longer does, pick another binding that still does"
    );
}

#[test]
fn no_label_in_the_default_keymap_leaks_the_round_trippable_form() {
    let stack = default_keymap_stack();
    let index = KeyHintIndex::build(&stack);

    for id in index.command_ids() {
        for hint in index.hints_for(id.as_str()) {
            for style in [
                KeyLabelStyle::Portable,
                KeyLabelStyle::MacGlyphsCommandAsCtrl,
                KeyLabelStyle::MacGlyphsCommandAsMeta,
            ] {
                let label = hint.label(style);
                assert!(
                    !label.contains('~'),
                    "`{id}` renders as `{label}`, which is the parseable form, not a label"
                );
            }
        }
    }
}

/// ⭐ The two mac conventions are opposites, and this is the test that says
/// so in one place.
///
/// The kernel's default keymap binds Select All to `ctrl`. A face that
/// forwards Command as `ctrl` (the web) must show that as ⌘A; a face that
/// forwards Command as `meta` and physical Control as `ctrl` (the desktop)
/// must show the same binding as ⌃A, because on that face it really is the
/// Control key.
///
/// ⚠️ Asserting **both** rather than one: a single style with the other
/// implied is exactly the state that let the desktop face ship every chord
/// with the glyphs swapped, found 13 Aug 2026.
#[test]
fn the_two_mac_styles_disagree_about_which_glyph_ctrl_wears() {
    let index = KeyHintIndex::build(&default_keymap_stack());

    let hint = index
        .primary_hint(SELECTION_SELECT_ALL.as_str())
        .expect("select all is bound by default");

    assert_eq!(hint.label(KeyLabelStyle::MacGlyphsCommandAsCtrl), "⌘A");
    assert_eq!(hint.label(KeyLabelStyle::MacGlyphsCommandAsMeta), "⌃A");
}

#[test]
fn a_multi_stroke_label_separates_chords_with_a_space() {
    let keymap = layer_with(
        "base",
        vec![KeyBinding::new(
            ctrl(KeyCode::Char('k')),
            &[ctrl(KeyCode::Char('d'))],
            MULTI_CURSOR_SKIP_LAST_OCCURRENCE,
        )],
    );
    let index = KeyHintIndex::build(&KeymapStack::with_base(keymap));
    let skip = MULTI_CURSOR_SKIP_LAST_OCCURRENCE;

    assert_eq!(label_of(&index, skip.as_str()), "Ctrl+K Ctrl+D");
    assert_eq!(
        index
            .primary_hint(skip.as_str())
            .unwrap()
            .label(KeyLabelStyle::MacGlyphsCommandAsCtrl),
        "⌘K ⌘D"
    );
}

#[test]
fn named_keys_read_as_words() {
    let keymap = layer_with(
        "base",
        vec![
            binding(StrokePattern::plain(KeyCode::PageDown), "cursor.lineDown"),
            binding(StrokePattern::plain(KeyCode::Escape), "search.close"),
            binding(StrokePattern::plain(KeyCode::F3), "search.nextMatch"),
        ],
    );
    let index = KeyHintIndex::build(&KeymapStack::with_base(keymap));

    assert_eq!(label_of(&index, "cursor.lineDown"), "Page Down");
    assert_eq!(label_of(&index, "search.close"), "Esc");
    assert_eq!(label_of(&index, "search.nextMatch"), "F3");
}

// ===== Layering =====

#[test]
fn a_suppressed_binding_leaves_no_hint() {
    let base = layer_with(
        "base",
        vec![binding(ctrl(KeyCode::Char('c')), "clipboard.copy")],
    );
    let mut stack = KeymapStack::with_base(base);
    assert_eq!(
        KeyHintIndex::build(&stack)
            .hints_for("clipboard.copy")
            .len(),
        1
    );

    stack.push(layer_with(
        "user",
        vec![KeyBinding::unbound(ctrl(KeyCode::Char('c')), &[])],
    ));

    assert!(
        KeyHintIndex::build(&stack)
            .hints_for("clipboard.copy")
            .is_empty()
    );
}

#[test]
fn a_rebinding_moves_the_hint_to_the_new_command() {
    let base = layer_with(
        "base",
        vec![binding(ctrl(KeyCode::Char('c')), "clipboard.copy")],
    );
    let mut stack = KeymapStack::with_base(base);
    stack.push(layer_with(
        "user",
        vec![binding(ctrl(KeyCode::Char('c')), "comment.toggleLine")],
    ));
    let index = KeyHintIndex::build(&stack);

    assert!(index.hints_for("clipboard.copy").is_empty());
    assert_eq!(label_of(&index, "comment.toggleLine"), "Ctrl+C");
}

#[test]
fn a_command_bound_in_two_layers_prefers_the_higher_one() {
    let base = layer_with(
        "base",
        vec![binding(ctrl(KeyCode::Char('p')), "palette.open")],
    );
    let mut stack = KeymapStack::with_base(base);
    stack.push(layer_with(
        "user",
        vec![binding(ctrl(KeyCode::Char('k')), "palette.open")],
    ));
    let index = KeyHintIndex::build(&stack);

    let hints = index.hints_for("palette.open");
    assert_eq!(hints.len(), 2, "both bindings still fire");
    assert_eq!(hints[0].label(KeyLabelStyle::Portable), "Ctrl+K");
    assert_eq!(hints[0].layer(), 1);
    assert_eq!(hints[1].label(KeyLabelStyle::Portable), "Ctrl+P");
}

#[test]
fn within_one_layer_the_simpler_chord_is_preferred() {
    let keymap = layer_with(
        "base",
        vec![
            binding(
                StrokePattern::new(
                    KeyCode::Char('p'),
                    ModifierPattern::NONE
                        .with_ctrl(ModifierState::Required)
                        .with_shift(ModifierState::Required),
                ),
                "palette.open",
            ),
            binding(ctrl(KeyCode::Char('k')), "palette.open"),
        ],
    );
    let index = KeyHintIndex::build(&KeymapStack::with_base(keymap));

    let hints = index.hints_for("palette.open");
    assert_eq!(hints.len(), 2);
    assert_eq!(hints[0].label(KeyLabelStyle::Portable), "Ctrl+K");
    assert_eq!(hints[1].label(KeyLabelStyle::Portable), "Ctrl+Shift+P");
}

#[test]
fn a_stranded_chord_leaves_no_hint() {
    let base = layer_with(
        "base",
        vec![KeyBinding::new(
            ctrl(KeyCode::Char('k')),
            &[ctrl(KeyCode::Char('d'))],
            MULTI_CURSOR_SKIP_LAST_OCCURRENCE,
        )],
    );
    let mut stack = KeymapStack::with_base(base);
    stack.push(layer_with(
        "user",
        vec![binding(ctrl(KeyCode::Char('k')), "palette.open")],
    ));
    let index = KeyHintIndex::build(&stack);

    assert!(
        index
            .hints_for(MULTI_CURSOR_SKIP_LAST_OCCURRENCE.as_str())
            .is_empty(),
        "the leader fires first, so the chord can never complete"
    );
    assert_eq!(label_of(&index, "palette.open"), "Ctrl+K");
}

// ===== Absence =====

#[test]
fn an_unbound_command_has_no_hint() {
    let index = KeyHintIndex::build(&default_keymap_stack());

    assert!(index.hints_for(EDIT_INSERT_CHARACTER.as_str()).is_empty());
    assert!(index.primary_hint(EDIT_INSERT_CHARACTER.as_str()).is_none());
}

#[test]
fn an_unknown_command_has_no_hint() {
    let index = KeyHintIndex::build(&default_keymap_stack());

    assert!(index.hints_for("nothing.likeThis").is_empty());
    assert!(index.primary_hint("nothing.likeThis").is_none());
}

#[test]
fn an_empty_stack_yields_an_empty_index() {
    let index = KeyHintIndex::build(&KeymapStack::new());

    assert!(index.is_empty());
    assert_eq!(index.len(), 0);
}

#[test]
fn a_mode_entering_binding_with_no_command_contributes_nothing() {
    let keymap = layer_with(
        "modal",
        vec![KeyBinding::enter_mode(
            StrokePattern::plain(KeyCode::Escape),
            &[],
            ModeName::from_static("normal"),
        )],
    );
    let index = KeyHintIndex::build(&KeymapStack::with_base(keymap));

    assert!(index.is_empty(), "a mode switch is not a command hint");
}
