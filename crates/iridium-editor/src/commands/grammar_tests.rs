//! Grammar tests: the pieces an operator/count/motion keymap needs.
//!
//! Each test here pins one thing that was previously *inexpressible*, so a
//! modal keymap had to enumerate the operator × motion × text-object × count
//! product as bindings — which for counts is infinite. The keymaps below are
//! written the way a real Vim layer would be, as data, with no kernel support for
//! any Vim concept.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    CommandArgs, CommandId, CommandInvocation, KeyBinding, KeyPress, Keymap, KeymapResolver,
    KeymapStack, ModeName, ModifierPattern, Resolution, StrokeCapture, StrokePattern,
};
use crate::input::KeyCode;

const NORMAL: ModeName = ModeName::from_static("normal");
const INSERT: ModeName = ModeName::from_static("insert");
const OPERATOR: ModeName = ModeName::from_static("operator-pending");

const DELETE_WORD: CommandId = CommandId::from_static("edit.deleteWordForward");
const DELETE_LINE: CommandId = CommandId::from_static("lines.delete");
const NO_OP: CommandId = CommandId::from_static("command.noOp");
const LINE_START: CommandId = CommandId::from_static("cursor.lineStart");
const FIND_CHAR: CommandId = CommandId::from_static("cursor.findChar");

/// A bare character stroke, as a modal keymap writes them.
fn key(c: char) -> StrokePattern {
    StrokePattern::plain(KeyCode::Char(c))
}

/// A wildcard stroke capturing whatever character is typed next.
fn any_char() -> StrokePattern {
    StrokePattern::any_char(ModifierPattern::NONE)
}

fn press(c: char) -> KeyPress {
    KeyPress::plain(KeyCode::Char(c))
}

fn normal_mode_resolver() -> KeymapResolver {
    let mut resolver = KeymapResolver::new();
    resolver.set_mode(Some(NORMAL));
    resolver
}

/// Feeds `text` one character at a time and returns the final resolution.
fn type_chars(resolver: &mut KeymapResolver, stack: &KeymapStack, text: &str) -> Resolution {
    let mut outcome = Resolution::NoMatch;
    for c in text.chars() {
        outcome = resolver.resolve(stack, press(c));
    }
    outcome
}

// ===== Counts =====

/// `dw` / `dd` with a count, exactly as a Vim layer would declare them.
fn counted_operator_keymap() -> KeymapStack {
    let mut keymap = Keymap::new("vim");
    keymap.push(
        KeyBinding::new(key('d'), &[key('w')], DELETE_WORD)
            .in_mode(NORMAL)
            .with_count_prefix(),
    );
    keymap.push(
        KeyBinding::new(key('d'), &[key('d')], DELETE_LINE)
            .in_mode(NORMAL)
            .with_count_prefix(),
    );
    KeymapStack::with_base(keymap)
}

#[test]
fn a_count_typed_before_the_operator_reaches_the_command() {
    let stack = counted_operator_keymap();
    let mut resolver = normal_mode_resolver();

    assert_eq!(
        type_chars(&mut resolver, &stack, "3dw"),
        Resolution::Matched(CommandInvocation::new(
            DELETE_WORD,
            CommandArgs::with_count(3)
        )),
        "`3dw` must be one binding carrying a count, not a binding per count"
    );
    assert!(!resolver.is_pending());
}

#[test]
fn a_count_typed_inside_the_sequence_reaches_the_command() {
    let stack = counted_operator_keymap();
    let mut resolver = normal_mode_resolver();

    let outcome = type_chars(&mut resolver, &stack, "d2w");
    assert_eq!(outcome.args().and_then(CommandArgs::count), Some(2));
    assert_eq!(outcome.command(), Some(&DELETE_WORD));
}

#[test]
fn a_multi_digit_count_accumulates_and_two_groups_multiply() {
    let stack = counted_operator_keymap();

    let mut resolver = normal_mode_resolver();
    assert_eq!(
        type_chars(&mut resolver, &stack, "22dw")
            .args()
            .and_then(CommandArgs::count),
        Some(22),
        "digits in one group accumulate decimally"
    );

    // Vim multiplies a count before the operator by one before the motion.
    let mut resolver = normal_mode_resolver();
    assert_eq!(
        type_chars(&mut resolver, &stack, "2d2w")
            .args()
            .and_then(CommandArgs::count),
        Some(4)
    );
}

#[test]
fn no_count_leaves_the_arguments_empty_and_the_repeat_count_at_one() {
    let stack = counted_operator_keymap();
    let mut resolver = normal_mode_resolver();

    let outcome = type_chars(&mut resolver, &stack, "dd");
    let args = outcome.args().expect("a match carries arguments");
    assert!(args.is_empty());
    assert_eq!(args.count(), None);
    assert_eq!(args.repeat_count(), 1, "an absent count means once");
}

#[test]
fn a_digit_is_ordinary_text_when_no_binding_in_reach_accepts_a_count() {
    // The guard that keeps the non-modal default keymap typing `3` as `3`.
    let stack = super::default_keymap_stack();
    let mut resolver = KeymapResolver::new();
    for c in ['0', '3', '9'] {
        assert_eq!(
            resolver.resolve(&stack, press(c)),
            Resolution::NoMatch,
            "`{c}` must fall through to the typing path"
        );
        assert!(!resolver.is_pending());
    }

    // Same keymap, but in a mode where the count-accepting binding does not apply.
    let counted = counted_operator_keymap();
    let mut insert = KeymapResolver::new();
    insert.set_mode(Some(INSERT));
    assert_eq!(insert.resolve(&counted, press('3')), Resolution::NoMatch);
}

#[test]
fn a_bound_digit_beats_the_count_register() {
    // Vim's `0` is line-start, and a keymap must be able to bind it even while
    // counts are live. A leading `0` is therefore never a count digit.
    let mut keymap = Keymap::new("vim");
    keymap.push(
        KeyBinding::new(key('d'), &[key('w')], DELETE_WORD)
            .in_mode(NORMAL)
            .with_count_prefix(),
    );
    keymap.push(KeyBinding::new(key('0'), &[], LINE_START).in_mode(NORMAL));
    let stack = KeymapStack::with_base(keymap);

    let mut resolver = normal_mode_resolver();
    assert_eq!(
        resolver.resolve(&stack, press('0')),
        Resolution::matched(LINE_START)
    );

    // A `0` *after* a digit is part of the count, so `10dw` still works.
    let mut resolver = normal_mode_resolver();
    assert_eq!(
        type_chars(&mut resolver, &stack, "10dw")
            .args()
            .and_then(CommandArgs::count),
        Some(10)
    );
}

#[test]
fn an_abandoned_count_does_not_leak_into_the_next_command() {
    let stack = counted_operator_keymap();
    let mut resolver = normal_mode_resolver();

    assert_eq!(resolver.resolve(&stack, press('7')), Resolution::Pending);
    assert_eq!(resolver.pending_count(), Some(7));
    assert_eq!(
        resolver.resolve(&stack, KeyPress::plain(KeyCode::Escape)),
        Resolution::Aborted
    );
    assert_eq!(resolver.pending_count(), None);

    let outcome = type_chars(&mut resolver, &stack, "dw");
    assert_eq!(outcome.args().and_then(CommandArgs::count), None);
}

// ===== Captured characters =====

#[test]
fn a_wildcard_stroke_captures_the_character_the_user_typed() {
    let mut keymap = Keymap::new("vim");
    keymap.push(
        KeyBinding::new(key('f'), &[any_char()], FIND_CHAR)
            .in_mode(NORMAL)
            .with_count_prefix(),
    );
    let stack = KeymapStack::with_base(keymap);

    let mut resolver = normal_mode_resolver();
    assert_eq!(resolver.resolve(&stack, press('f')), Resolution::Pending);
    let outcome = resolver.resolve(&stack, press('x'));
    assert_eq!(outcome.command(), Some(&FIND_CHAR));
    assert_eq!(outcome.args().map(CommandArgs::captures), Some(&['x'][..]));

    // The captured character keeps its case: normalization is for lookup only.
    let mut resolver = normal_mode_resolver();
    resolver.resolve(&stack, press('f'));
    let outcome = resolver.resolve(&stack, press('Q'));
    assert_eq!(outcome.args().and_then(|args| args.capture(0)), Some('Q'));

    // And a count still rides along: `3fx`.
    let mut resolver = normal_mode_resolver();
    let outcome = type_chars(&mut resolver, &stack, "3fx");
    assert_eq!(outcome.args().and_then(CommandArgs::count), Some(3));
    assert_eq!(outcome.args().and_then(|args| args.capture(0)), Some('x'));
}

#[test]
fn a_text_object_captures_its_delimiter_for_any_delimiter() {
    // `di{delim}` — one binding, every delimiter, which is the case that could not
    // be enumerated as data at all.
    let mut keymap = Keymap::new("vim");
    keymap.push(
        KeyBinding::new(
            key('d'),
            &[key('i'), any_char()],
            CommandId::from_static("edit.deleteInside"),
        )
        .in_mode(NORMAL),
    );
    let stack = KeymapStack::with_base(keymap);

    for delimiter in ['"', '\'', '(', '[', '{', '<', '|', 'w'] {
        let mut resolver = normal_mode_resolver();
        assert_eq!(resolver.resolve(&stack, press('d')), Resolution::Pending);
        assert_eq!(resolver.resolve(&stack, press('i')), Resolution::Pending);
        let outcome = resolver.resolve(&stack, press(delimiter));
        assert_eq!(
            outcome.args().and_then(|args| args.capture(0)),
            Some(delimiter),
            "di{delimiter} must resolve with the delimiter captured"
        );
    }
}

#[test]
fn an_exact_binding_outranks_a_wildcard_on_the_same_prefix() {
    // `dd` and `d{char}` must coexist: the exact sequence wins where it applies.
    let mut keymap = Keymap::new("vim");
    keymap.push(
        KeyBinding::new(
            key('d'),
            &[any_char()],
            CommandId::from_static("edit.deleteMotion"),
        )
        .in_mode(NORMAL),
    );
    keymap.push(KeyBinding::new(key('d'), &[key('d')], DELETE_LINE).in_mode(NORMAL));
    let stack = KeymapStack::with_base(keymap);

    let mut resolver = normal_mode_resolver();
    assert_eq!(
        type_chars(&mut resolver, &stack, "dd").command(),
        Some(&DELETE_LINE)
    );

    let mut resolver = normal_mode_resolver();
    let outcome = type_chars(&mut resolver, &stack, "dj");
    assert_eq!(
        outcome.command(),
        Some(&CommandId::from_static("edit.deleteMotion"))
    );
    assert_eq!(outcome.args().and_then(|args| args.capture(0)), Some('j'));
}

#[test]
fn a_wildcard_led_binding_is_reachable_even_though_no_key_indexes_it() {
    // Bindings are indexed by their first stroke's key code; a wildcard has none,
    // so it lives in a separate list that lookup must also consult.
    let mut keymap = Keymap::new("vim");
    keymap.push(KeyBinding::new(any_char(), &[], NO_OP).in_mode(NORMAL));
    let stack = KeymapStack::with_base(keymap);
    let mut resolver = normal_mode_resolver();

    for c in ['a', 'Z', '7', 'é'] {
        let outcome = resolver.resolve(&stack, press(c));
        assert_eq!(
            outcome.command(),
            Some(&NO_OP),
            "a wildcard-led binding must match `{c}`"
        );
        assert_eq!(outcome.args().and_then(|args| args.capture(0)), Some(c));
    }
    // A non-character key is not a character, so the wildcard leaves it alone.
    assert_eq!(
        resolver.resolve(&stack, KeyPress::plain(KeyCode::Left)),
        Resolution::NoMatch
    );
}

#[test]
fn the_wildcard_capture_survives_a_serde_round_trip() {
    let binding = KeyBinding::new(key('f'), &[any_char()], FIND_CHAR)
        .in_mode(NORMAL)
        .with_count_prefix();
    let json = serde_json::to_string(&binding).expect("serializable");
    let restored: KeyBinding = serde_json::from_str(&json).expect("deserializable");

    assert_eq!(restored.sequence()[1].capture, StrokeCapture::AnyChar);
    assert!(restored.accepts_count());
    assert_eq!(restored.mode(), Some(&NORMAL));
    assert_eq!(restored, binding);
}

// ===== Modes =====

/// A minimal but complete modal keymap: enter insert, type, leave insert.
fn modal_keymap() -> KeymapStack {
    let mut vim = Keymap::new("vim");
    // `i` enters insert mode and runs nothing.
    vim.push(KeyBinding::enter_mode(key('i'), &[], INSERT).in_mode(NORMAL));
    // `Escape` in insert mode returns to normal.
    vim.push(
        KeyBinding::enter_mode(StrokePattern::plain(KeyCode::Escape), &[], NORMAL).in_mode(INSERT),
    );
    // `d` runs the operator and switches to operator-pending.
    vim.push(
        KeyBinding::new(key('d'), &[], DELETE_LINE)
            .in_mode(NORMAL)
            .then_enter_mode(OPERATOR),
    );
    // In normal mode, every other character is swallowed rather than typed.
    vim.push(KeyBinding::new(any_char(), &[], NO_OP).in_mode(NORMAL));
    KeymapStack::with_base(vim)
}

#[test]
fn a_keymap_can_switch_modes_with_no_help_from_the_kernel() {
    // REGRESSION: `set_mode` existed but nothing could call it, so a modal keymap
    // could name modes and never enter one. Mode transitions are now binding data.
    let stack = modal_keymap();
    let mut resolver = normal_mode_resolver();

    assert_eq!(
        resolver.resolve(&stack, press('i')),
        Resolution::ModeEntered(INSERT)
    );
    assert_eq!(resolver.mode(), Some(&INSERT));

    // In insert mode nothing is bound, so characters reach the typing path.
    assert_eq!(resolver.resolve(&stack, press('x')), Resolution::NoMatch);

    // And Escape gets you back out, which is the half that was unreachable.
    assert_eq!(
        resolver.resolve(&stack, KeyPress::plain(KeyCode::Escape)),
        Resolution::ModeEntered(NORMAL)
    );
    assert_eq!(resolver.mode(), Some(&NORMAL));
}

#[test]
fn a_binding_can_run_a_command_and_change_mode_in_one_stroke() {
    let stack = modal_keymap();
    let mut resolver = normal_mode_resolver();

    let outcome = resolver.resolve(&stack, press('d'));
    assert_eq!(outcome.command(), Some(&DELETE_LINE));
    assert_eq!(
        resolver.mode(),
        Some(&OPERATOR),
        "the operator must leave the resolver in operator-pending"
    );
}

#[test]
fn normal_mode_swallows_a_character_instead_of_typing_it() {
    // A suppression falls *through* to typing, which is right for "not my chord"
    // and catastrophic in a modal normal mode. `command.noOp` is the difference.
    let stack = modal_keymap();
    let mut resolver = normal_mode_resolver();

    let outcome = resolver.resolve(&stack, press('q'));
    assert_eq!(outcome.command(), Some(&NO_OP));
    assert!(
        outcome.consumed(),
        "an unhandled key in normal mode must not become text"
    );
}

#[test]
fn a_mode_only_binding_is_not_a_suppression() {
    let entering = KeyBinding::enter_mode(key('i'), &[], INSERT);
    assert!(entering.command().is_none());
    assert!(
        !entering.is_suppression(),
        "a binding that switches mode runs no command but is still live"
    );
    assert!(KeyBinding::unbound(key('i'), &[]).is_suppression());
}
