//! Optional syntax navigation, using the shared command and mode machinery.
use super::builtin::{
    AST_CURSOR_ON_EVERY_CHILD, AST_CURSOR_ON_EVERY_SIBLING, AST_DESCEND_SELECTION,
    AST_EXPAND_SELECTION, AST_EXTEND_NEXT_SIBLING, AST_EXTEND_PREVIOUS_SIBLING,
    AST_SELECT_NEXT_SIBLING, AST_SELECT_NODE, AST_SELECT_PREVIOUS_SIBLING, CLIPBOARD_COPY,
    COMMAND_FINISH_CHANGE, COMMAND_REPEAT_SEQUENCE, EDIT_DELETE_FORWARD,
};
use super::{KeyBinding, Keymap, ModeName, ModifierPattern, StrokePattern};
use crate::{KeyCode, Modifiers};

/// Arrow keys traverse named syntax nodes; unbound typing is suppressed.
pub const STRUCTURAL: ModeName = ModeName::from_static("structural");
/// Operator-first syntax navigation: `d`/`c` waits for a node motion.
pub const STRUCTURAL_VERB: ModeName = ModeName::from_static("structural-verb");
/// Replacement input captured as part of one structural change.
pub const STRUCTURAL_CHANGE: ModeName = ModeName::from_static("structural-change");
/// Ordinary typing and text navigation after leaving structural navigation.
pub const STRUCTURAL_INSERT: ModeName = ModeName::from_static("insert");

/// Builds a rebindable preset that leaves ordinary editing enabled at startup.
///
/// Ctrl+Space or F6 selects the current node and enters structural navigation. Up/Down
/// select siblings; Left expands; Right retraces that expansion or selects a
/// child. `a` selects every child, `s` every sibling, `d` deletes, `c` changes,
/// and `y` copies. `i` or Escape returns to typing, retaining the selections.
#[must_use]
pub fn structural_keymap() -> Keymap {
    let mut keys = Keymap::new("structural-navigation");
    let plain = StrokePattern::plain;
    for stroke in [
        StrokePattern::new(
            KeyCode::Char(' '),
            ModifierPattern::exact(Modifiers::ctrl()),
        ),
        StrokePattern::plain(KeyCode::F6),
    ] {
        keys.push(KeyBinding::new(stroke, &[], AST_SELECT_NODE).then_enter_mode(STRUCTURAL));
    }
    for (key, command) in [
        (KeyCode::Up, AST_SELECT_PREVIOUS_SIBLING),
        (KeyCode::Down, AST_SELECT_NEXT_SIBLING),
        (KeyCode::Left, AST_EXPAND_SELECTION),
        (KeyCode::Right, AST_DESCEND_SELECTION),
        (KeyCode::Char('a'), AST_CURSOR_ON_EVERY_CHILD),
        (KeyCode::Char('s'), AST_CURSOR_ON_EVERY_SIBLING),
        (KeyCode::Char('d'), EDIT_DELETE_FORWARD),
        (KeyCode::Char('y'), CLIPBOARD_COPY),
    ] {
        let binding = KeyBinding::new(plain(key), &[], command).in_mode(STRUCTURAL);
        keys.push(
            if matches!(
                key,
                KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right
            ) {
                binding.with_count_prefix()
            } else {
                binding
            },
        );
    }
    for (key, command) in [
        (KeyCode::Up, AST_EXTEND_PREVIOUS_SIBLING),
        (KeyCode::Down, AST_EXTEND_NEXT_SIBLING),
    ] {
        keys.push(
            KeyBinding::new(
                StrokePattern::new(key, ModifierPattern::exact(Modifiers::shift())),
                &[],
                command,
            )
            .in_mode(STRUCTURAL)
            .with_count_prefix(),
        );
    }
    keys.push(
        KeyBinding::new(plain(KeyCode::Char('c')), &[], EDIT_DELETE_FORWARD)
            .in_mode(STRUCTURAL)
            .then_record_input(STRUCTURAL_CHANGE),
    );
    for key in [KeyCode::Char('i'), KeyCode::Escape] {
        keys.push(KeyBinding::enter_mode(plain(key), &[], STRUCTURAL_INSERT).in_mode(STRUCTURAL));
    }
    keys.silence_typing_in(STRUCTURAL);
    append_verb_first(&mut keys);
    keys.push(
        KeyBinding::new(plain(KeyCode::Escape), &[], COMMAND_FINISH_CHANGE)
            .in_mode(STRUCTURAL_CHANGE),
    );
    keys
}

fn append_verb_first(keys: &mut Keymap) {
    let shared: Vec<_> = keys
        .bindings()
        .iter()
        .filter(|binding| {
            binding.mode() == Some(&STRUCTURAL) && binding.command() != Some(&EDIT_DELETE_FORWARD)
        })
        .cloned()
        .map(|binding| binding.in_mode(STRUCTURAL_VERB))
        .collect();
    for binding in shared {
        keys.push(binding);
    }
    keys.push(
        KeyBinding::new(StrokePattern::plain(KeyCode::F7), &[], AST_SELECT_NODE)
            .then_enter_mode(STRUCTURAL_VERB),
    );
    for verb in ['d', 'c'] {
        for (key, motion) in [
            (KeyCode::Up, AST_SELECT_PREVIOUS_SIBLING),
            (KeyCode::Down, AST_SELECT_NEXT_SIBLING),
            (KeyCode::Left, AST_EXPAND_SELECTION),
            (KeyCode::Right, AST_DESCEND_SELECTION),
            (KeyCode::Char(verb), AST_SELECT_NODE),
            (KeyCode::Char('a'), AST_CURSOR_ON_EVERY_CHILD),
            (KeyCode::Char('s'), AST_CURSOR_ON_EVERY_SIBLING),
        ] {
            let mut binding = KeyBinding::new(
                StrokePattern::plain(KeyCode::Char(verb)),
                &[StrokePattern::plain(key)],
                motion,
            )
            .then_command(EDIT_DELETE_FORWARD)
            .in_mode(STRUCTURAL_VERB)
            .with_count_prefix();
            if verb == 'c' {
                binding = binding.then_record_input(STRUCTURAL_CHANGE);
            }
            keys.push(binding);
        }
    }
    for mode in [STRUCTURAL, STRUCTURAL_VERB] {
        keys.push(
            KeyBinding::new(
                StrokePattern::plain(KeyCode::Char('.')),
                &[],
                COMMAND_REPEAT_SEQUENCE,
            )
            .in_mode(mode)
            .with_count_prefix(),
        );
    }
    keys.silence_typing_in(STRUCTURAL_VERB);
}
