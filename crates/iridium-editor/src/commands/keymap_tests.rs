//! Keymap tests: single-layer matching, precedence, the `AltGr` guard and modes.
//!
//! Layer stacking lives in [`super::layer_tests`]; validation, canonicalization
//! and the modifier-pattern algebra in [`super::validation_tests`].

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
    CommandId, KeyBinding, KeyPress, Keymap, ModeName, ModifierPattern, ModifierState,
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

// ===== Basic matching =====

#[test]
fn exact_chord_matches_and_reports_its_command() {
    let mut keymap = Keymap::new("test");
    keymap.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('c')),
        &[],
        CommandId::from_static("clipboard.copy"),
    ));

    let binding = keymap
        .exact_match(
            &press(KeyCode::Char('c'), mods(false, true, false, false, false)),
            None,
        )
        .expect("ctrl+c matches");
    assert_eq!(binding.command().unwrap().as_str(), "clipboard.copy");
    assert_eq!(keymap.len(), 1);
    assert!(!keymap.is_empty());
}

#[test]
fn character_keys_match_case_insensitively_in_both_directions() {
    let mut keymap = Keymap::new("test");
    // Bound with an upper-case letter; must still match a lower-case report.
    keymap.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('K')),
        &[],
        CommandId::from_static("lines.delete"),
    ));

    for reported in ['k', 'K'] {
        assert!(
            keymap
                .exact_match(
                    &press(
                        KeyCode::Char(reported),
                        mods(false, true, false, false, false)
                    ),
                    None
                )
                .is_some(),
            "`{reported}` should match"
        );
    }
    assert_eq!(keymap.bindings()[0].sequence()[0].key, KeyCode::Char('k'));
}

#[test]
fn a_forbidden_modifier_blocks_the_match() {
    let mut keymap = Keymap::new("test");
    keymap.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('c')),
        &[],
        CommandId::from_static("clipboard.copy"),
    ));

    // The default pattern forbids every other modifier, so Ctrl+Alt+C misses.
    assert!(
        keymap
            .exact_match(
                &press(KeyCode::Char('c'), mods(false, true, true, false, false)),
                None
            )
            .is_none()
    );
}

#[test]
fn an_any_modifier_matches_both_ways() {
    let mut keymap = Keymap::new("test");
    keymap.push(KeyBinding::new(
        StrokePattern::new(
            KeyCode::Char('c'),
            ModifierPattern::NONE
                .with_ctrl(ModifierState::Required)
                .with_shift(ModifierState::Any),
        ),
        &[],
        CommandId::from_static("clipboard.copy"),
    ));

    for shift in [false, true] {
        assert!(
            keymap
                .exact_match(
                    &press(KeyCode::Char('c'), mods(shift, true, false, false, false)),
                    None
                )
                .is_some(),
            "shift={shift} should match"
        );
    }
}

// ===== Precedence inside one layer =====

#[test]
fn the_more_specific_pattern_wins() {
    let mut keymap = Keymap::new("test");
    // Loose: Up with Shift absent, everything else ignored.
    keymap.push(KeyBinding::new(
        StrokePattern::new(
            KeyCode::Up,
            ModifierPattern::ANY.with_shift(ModifierState::Forbidden),
        ),
        &[],
        CommandId::from_static("cursor.lineUp"),
    ));
    // Specific: Alt held, Ctrl/Meta/Shift absent.
    keymap.push(KeyBinding::new(
        StrokePattern::new(
            KeyCode::Up,
            ModifierPattern::NONE.with_alt(ModifierState::Required),
        ),
        &[],
        CommandId::from_static("lines.moveUp"),
    ));

    let alt_up = press(KeyCode::Up, mods(false, false, true, false, false));
    assert_eq!(
        keymap
            .exact_match(&alt_up, None)
            .unwrap()
            .command()
            .unwrap()
            .as_str(),
        "lines.moveUp"
    );
    let plain_up = press(KeyCode::Up, mods(false, false, false, false, false));
    assert_eq!(
        keymap
            .exact_match(&plain_up, None)
            .unwrap()
            .command()
            .unwrap()
            .as_str(),
        "cursor.lineUp"
    );
}

#[test]
fn the_later_binding_wins_on_an_equally_specific_tie() {
    let mut keymap = Keymap::new("test");
    keymap.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('c')),
        &[],
        CommandId::from_static("first"),
    ));
    keymap.push(KeyBinding::new(
        ctrl_pattern(KeyCode::Char('c')),
        &[],
        CommandId::from_static("second"),
    ));

    let binding = keymap
        .exact_match(
            &press(KeyCode::Char('c'), mods(false, true, false, false, false)),
            None,
        )
        .unwrap();
    assert_eq!(binding.command().unwrap().as_str(), "second");
}

// ===== The AltGr guard =====

#[test]
fn alt_graph_is_forbidden_by_default_so_the_add_cursor_guard_survives() {
    let chord = ModifierPattern::NONE
        .with_ctrl(ModifierState::Required)
        .with_alt(ModifierState::Required);
    assert_eq!(chord.alt_graph, ModifierState::Forbidden);

    let mut keymap = Keymap::new("test");
    keymap.push(KeyBinding::new(
        StrokePattern::new(KeyCode::Up, chord),
        &[],
        CommandId::from_static("multiCursor.addCursorAbove"),
    ));

    // Ctrl+Alt+Up adds a cursor.
    assert!(
        keymap
            .exact_match(
                &press(KeyCode::Up, mods(false, true, true, false, false)),
                None
            )
            .is_some()
    );
    // AltGr, reported as Ctrl+Alt plus the AltGraph bit, must not.
    assert!(
        keymap
            .exact_match(
                &press(KeyCode::Up, mods(false, true, true, false, true)),
                None
            )
            .is_none()
    );
}

#[test]
fn a_binding_can_opt_out_of_the_alt_graph_guard() {
    let pattern = ModifierPattern::NONE
        .with_ctrl(ModifierState::Required)
        .with_alt(ModifierState::Required)
        .with_alt_graph(ModifierState::Any);
    let mut keymap = Keymap::new("test");
    keymap.push(KeyBinding::new(
        StrokePattern::new(KeyCode::Up, pattern),
        &[],
        CommandId::from_static("test.command"),
    ));

    assert!(
        keymap
            .exact_match(
                &press(KeyCode::Up, mods(false, true, true, false, true)),
                None
            )
            .is_some()
    );
}

// ===== Modes =====

#[test]
fn mode_free_bindings_apply_in_every_mode() {
    let mut keymap = Keymap::new("test");
    keymap.push(KeyBinding::new(
        StrokePattern::plain(KeyCode::Char('d')),
        &[],
        CommandId::from_static("mode.free"),
    ));

    let d = press(KeyCode::Char('d'), Modifiers::none());
    let normal = ModeName::from_static("normal");
    assert!(keymap.exact_match(&d, None).is_some());
    assert!(keymap.exact_match(&d, Some(&normal)).is_some());
}

#[test]
fn mode_scoped_bindings_only_apply_in_their_mode() {
    let normal = ModeName::from_static("normal");
    let insert = ModeName::from_static("insert");
    let mut keymap = Keymap::new("modal");
    keymap.push(
        KeyBinding::new(
            StrokePattern::plain(KeyCode::Char('d')),
            &[],
            CommandId::from_static("lines.delete"),
        )
        .in_mode(normal.clone()),
    );

    let d = press(KeyCode::Char('d'), Modifiers::none());
    assert_eq!(
        keymap
            .exact_match(&d, Some(&normal))
            .unwrap()
            .command()
            .unwrap()
            .as_str(),
        "lines.delete"
    );
    assert!(keymap.exact_match(&d, Some(&insert)).is_none());
    assert!(keymap.exact_match(&d, None).is_none());
}

#[test]
fn a_mode_scoped_binding_outranks_a_mode_free_one_for_the_same_keys() {
    let normal = ModeName::from_static("normal");
    let mut keymap = Keymap::new("modal");
    // Registered first and mode-free; must still lose to the mode-scoped one.
    keymap.push(KeyBinding::new(
        StrokePattern::plain(KeyCode::Char('d')),
        &[],
        CommandId::from_static("edit.insertCharacter"),
    ));
    keymap.push(
        KeyBinding::new(
            StrokePattern::plain(KeyCode::Char('d')),
            &[],
            CommandId::from_static("lines.delete"),
        )
        .in_mode(normal.clone()),
    );

    let d = press(KeyCode::Char('d'), Modifiers::none());
    assert_eq!(
        keymap
            .exact_match(&d, Some(&normal))
            .unwrap()
            .command()
            .unwrap()
            .as_str(),
        "lines.delete"
    );
    // Outside the mode, the mode-free binding is what remains.
    assert_eq!(
        keymap
            .exact_match(&d, None)
            .unwrap()
            .command()
            .unwrap()
            .as_str(),
        "edit.insertCharacter"
    );
}
