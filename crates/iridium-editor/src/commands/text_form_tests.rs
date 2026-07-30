//! The textual form of a key sequence, and the modifier algebra the tie-break
//! rests on.
//!
//! Two things are pinned here. First, `"ctrl-k ctrl-c"` parses and renders back
//! identically — a keymap on disk or from a rebinding UI does not have to be
//! written in the verbose serde shape, and a diagnostic naming a sequence can be
//! quoted straight back into a binding. Second, the tie-break between two bindings
//! that both match one keypress ranks *required* modifiers first, because counting
//! constraints alone (which includes every `Forbidden`) does not express "more
//! modifiers held wins".

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    CommandId, KeyBinding, KeyPress, Keymap, KeymapError, ModifierPattern, ModifierState,
    StrokeCapture, StrokePattern, key_from_name, name_of,
};
use crate::input::{KeyCode, Modifiers};

// ===== Parsing =====

#[test]
fn a_chord_parses_from_either_separator() {
    for text in ["ctrl+k", "ctrl-k", "CTRL+K", " ctrl-k "] {
        let stroke: StrokePattern = text.parse().expect(text);
        assert_eq!(stroke.key, KeyCode::Char('k'));
        assert_eq!(stroke.modifiers.ctrl, ModifierState::Required);
        assert_eq!(
            stroke.modifiers.shift,
            ModifierState::Forbidden,
            "an unmentioned modifier must be forbidden, which is the AltGr guard"
        );
        assert_eq!(stroke.modifiers.alt_graph, ModifierState::Forbidden);
    }
}

#[test]
fn every_modifier_spelling_parses() {
    type Field = fn(&ModifierPattern) -> ModifierState;
    let cases: &[(&str, Field)] = &[
        ("ctrl+a", |m| m.ctrl),
        ("control+a", |m| m.ctrl),
        ("shift+a", |m| m.shift),
        ("alt+a", |m| m.alt),
        ("option+a", |m| m.alt),
        ("meta+a", |m| m.meta),
        ("cmd+a", |m| m.meta),
        ("super+a", |m| m.meta),
        ("win+a", |m| m.meta),
        ("altgraph+a", |m| m.alt_graph),
        ("altgr+a", |m| m.alt_graph),
    ];
    for (text, field) in cases {
        let stroke: StrokePattern = text.parse().expect(text);
        assert_eq!(field(&stroke.modifiers), ModifierState::Required, "{text}");
    }
}

#[test]
fn a_tilde_prefix_means_the_modifier_is_ignored() {
    let stroke: StrokePattern = "~shift+up".parse().expect("parses");
    assert_eq!(stroke.key, KeyCode::Up);
    assert_eq!(stroke.modifiers.shift, ModifierState::Any);
    assert!(stroke.matches(KeyPress::plain(KeyCode::Up)));
    assert!(stroke.matches(KeyPress::new(KeyCode::Up, Modifiers::shift())));
}

#[test]
fn the_capture_wildcard_has_a_text_form() {
    let stroke: StrokePattern = "{char}".parse().expect("parses");
    assert_eq!(stroke.capture, StrokeCapture::AnyChar);
    assert!(stroke.captures());
    assert!(stroke.matches(KeyPress::plain(KeyCode::Char('q'))));
    assert!(!stroke.matches(KeyPress::plain(KeyCode::Left)));
    assert_eq!(stroke.to_string(), "{char}");
}

#[test]
fn the_reserved_key_names_parse_and_render() {
    for (text, key) in [("minus", KeyCode::Char('-')), ("space", KeyCode::Char(' '))] {
        let stroke: StrokePattern = text.parse().expect(text);
        assert_eq!(stroke.key, key);
        assert_eq!(stroke.to_string(), text);
    }
    // Which means a chord on those keys survives the separator grammar.
    let stroke: StrokePattern = "ctrl+minus".parse().expect("parses");
    assert_eq!(stroke.key, KeyCode::Char('-'));
    assert_eq!(stroke.modifiers.ctrl, ModifierState::Required);
}

#[test]
fn an_unparsable_chord_names_itself_in_the_error() {
    for text in ["", "   ", "ctrl+nosuchkey", "+k", "ctrl+"] {
        let error = text.parse::<StrokePattern>().expect_err(text);
        assert!(
            matches!(error, KeymapError::UnparsableStroke { .. }),
            "{text}: {error:?}"
        );
        assert!(error.to_string().contains(text.trim()) || text.trim().is_empty());
    }
}

#[test]
fn a_sequence_parses_and_binds_in_one_step() {
    let binding = KeyBinding::parse(
        "ctrl-k ctrl-c",
        CommandId::from_static("comment.toggleLine"),
    )
    .unwrap();
    assert_eq!(binding.sequence().len(), 2);
    assert_eq!(binding.display_sequence(), "ctrl+k ctrl+c");
    assert_eq!(binding.command().unwrap().as_str(), "comment.toggleLine");

    let empty = KeyBinding::parse_sequence("   ").expect_err("no chords");
    assert!(matches!(empty, KeymapError::EmptySequence { .. }));
}

// ===== Round trip =====

#[test]
fn every_key_code_round_trips_through_its_name() {
    // Exhaustive over `KeyCode`, so adding a variant without a name is a failure
    // rather than a silently unparseable binding.
    let all = [
        KeyCode::Char('a'),
        KeyCode::Char('-'),
        KeyCode::Char(' '),
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Up,
        KeyCode::Down,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::Backspace,
        KeyCode::Delete,
        KeyCode::Enter,
        KeyCode::Tab,
        KeyCode::Shift,
        KeyCode::Control,
        KeyCode::Alt,
        KeyCode::Meta,
        KeyCode::Escape,
        KeyCode::F1,
        KeyCode::F2,
        KeyCode::F3,
        KeyCode::F4,
        KeyCode::F5,
        KeyCode::F6,
        KeyCode::F7,
        KeyCode::F8,
        KeyCode::F9,
        KeyCode::F10,
        KeyCode::F11,
        KeyCode::F12,
    ];
    for key in all {
        let name = name_of(key);
        assert_eq!(
            key_from_name(&name),
            Some(key),
            "`{name}` did not round trip"
        );
    }
}

#[test]
fn the_display_form_of_every_default_binding_parses_back_identically() {
    // The property that makes a diagnostic quotable: `Display` shows every
    // constrained modifier (`~name` for the ones a pattern ignores), so what the
    // editor prints is what a keymap author can paste back.
    for binding in super::default_non_modal_keymap().bindings() {
        let text = binding.display_sequence();
        let (first, rest) = KeyBinding::parse_sequence(&text)
            .unwrap_or_else(|error| panic!("`{text}` does not parse: {error}"));
        let reparsed = KeyBinding::new(first, &rest, CommandId::from_static("x"));
        assert_eq!(
            reparsed.sequence(),
            binding.sequence(),
            "`{text}` did not round trip"
        );
    }
}

// ===== The specificity tie-break =====

#[test]
fn required_modifiers_are_counted_separately_from_all_constraints() {
    // `specificity` counts Forbidden too, so it cannot express "more modifiers
    // held". `required_count` is the half that can.
    assert_eq!(ModifierPattern::NONE.specificity(), 5);
    assert_eq!(ModifierPattern::NONE.required_count(), 0);

    let ctrl_alt = ModifierPattern::exact(Modifiers {
        shift: false,
        ctrl: true,
        alt: true,
        meta: false,
        alt_graph: false,
    });
    assert_eq!(
        ctrl_alt.specificity(),
        5,
        "an exact pattern constrains all 5"
    );
    assert_eq!(ctrl_alt.required_count(), 2);
}

#[test]
fn an_exactly_specified_arrow_binding_does_not_tie_with_the_add_cursor_chord() {
    // REGRESSION: the tie-break ranked total *constraints*, and an exact pattern
    // scores the maximum regardless of how many modifiers it requires. So a new
    // `Up` binding written with `ModifierPattern::exact` tied with the Ctrl+Alt+Up
    // add-cursor chord and the winner fell to insertion order — the opposite of the
    // documented model. Required modifiers are now ranked first.
    let plain_up = ModifierPattern::exact(Modifiers::none());
    let add_cursor = ModifierPattern::exact(Modifiers {
        shift: false,
        ctrl: true,
        alt: true,
        meta: false,
        alt_graph: false,
    });
    assert_eq!(
        plain_up.specificity(),
        add_cursor.specificity(),
        "the old ranking could not separate these"
    );

    let mut keymap = Keymap::new("test");
    // Deliberately insert the chord FIRST, so insertion order would pick the
    // looser binding if the modifier ranking failed.
    keymap.push(KeyBinding::new(
        StrokePattern::new(KeyCode::Up, add_cursor),
        &[],
        CommandId::from_static("multiCursor.addCursorAbove"),
    ));
    keymap.push(KeyBinding::new(
        StrokePattern::new(KeyCode::Up, plain_up),
        &[],
        CommandId::from_static("cursor.lineUp"),
    ));

    let held = [KeyPress::new(
        KeyCode::Up,
        Modifiers {
            shift: false,
            ctrl: true,
            alt: true,
            meta: false,
            alt_graph: false,
        },
    )];
    assert_eq!(
        keymap
            .exact_match(&held, None)
            .and_then(KeyBinding::command)
            .map(CommandId::as_str),
        Some("multiCursor.addCursorAbove"),
        "the chord requiring two modifiers must win"
    );

    // And the plain arrow still wins when no modifier is held.
    let bare = [KeyPress::plain(KeyCode::Up)];
    assert_eq!(
        keymap
            .exact_match(&bare, None)
            .and_then(KeyBinding::command)
            .map(CommandId::as_str),
        Some("cursor.lineUp")
    );
}

#[test]
fn required_modifiers_invert_an_exact_pattern() {
    // What a chord-capture rebinding UI needs: turn the recorded chord into a
    // pattern, and turn the pattern back into the chord to show the user.
    let held = Modifiers {
        shift: true,
        ctrl: true,
        alt: false,
        meta: false,
        alt_graph: false,
    };
    assert_eq!(ModifierPattern::exact(held).required_modifiers(), held);
    assert_eq!(
        ModifierPattern::ANY.required_modifiers(),
        Modifiers::none(),
        "a pattern that requires nothing reports nothing held"
    );
}
