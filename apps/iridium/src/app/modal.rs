//! Terminal defaults use structural targets with familiar modal entry and exit.
use iridium_editor::commands::builtin::PALETTE_OPEN;
use iridium_editor::commands::builtin::{
    AST_NAVIGATE_COARSER, AST_NAVIGATE_FINER, AST_NAVIGATE_NEXT, AST_NAVIGATE_PREVIOUS,
    AST_SELECT_NODE, CLIPBOARD_CUT, CLIPBOARD_PASTE, COMMAND_NO_OP, COMMAND_REPEAT_SEQUENCE,
    CURSOR_CHAR_LEFT, CURSOR_CHAR_LEFT_SELECT, CURSOR_CHAR_RIGHT, CURSOR_CHAR_RIGHT_SELECT,
    CURSOR_LINE_DOWN, CURSOR_LINE_DOWN_SELECT, CURSOR_LINE_UP, CURSOR_LINE_UP_SELECT,
    EDIT_DELETE_FORWARD, HISTORY_REDO, HISTORY_UNDO,
};
use iridium_editor::commands::structural_keymap::{
    STRUCTURAL, STRUCTURAL_CHANGE, STRUCTURAL_INSERT, STRUCTURAL_VERB,
};
use iridium_editor::{KeyBinding, KeyCode, Keymap, ModifierPattern, ModifierState, StrokePattern};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum StartupMode {
    Structural,
    Insert,
}

pub(super) fn keymap() -> Keymap {
    let mut keys = Keymap::new("terminal-modal");
    for mode in [STRUCTURAL, STRUCTURAL_VERB] {
        for (key, command) in [
            (KeyCode::Up, CURSOR_LINE_UP),
            (KeyCode::Down, CURSOR_LINE_DOWN),
            (KeyCode::Left, CURSOR_CHAR_LEFT),
            (KeyCode::Right, CURSOR_CHAR_RIGHT),
            (KeyCode::Char(','), AST_NAVIGATE_FINER),
            (KeyCode::Char('.'), AST_NAVIGATE_COARSER),
            (KeyCode::Char(':'), PALETTE_OPEN),
            (KeyCode::Char('u'), HISTORY_UNDO),
            (KeyCode::Char('h'), AST_NAVIGATE_COARSER),
            (KeyCode::Char('j'), AST_NAVIGATE_NEXT),
            (KeyCode::Char('k'), AST_NAVIGATE_PREVIOUS),
            (KeyCode::Char('l'), AST_NAVIGATE_FINER),
            (KeyCode::Escape, COMMAND_NO_OP),
        ] {
            keys.push(
                KeyBinding::new(
                    StrokePattern::new(
                        key,
                        ModifierPattern {
                            shift: if matches!(
                                key,
                                KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right
                            ) {
                                ModifierState::Forbidden
                            } else {
                                ModifierState::Any
                            },
                            ..ModifierPattern::NONE
                        },
                    ),
                    &[],
                    command,
                )
                .in_mode(mode.clone())
                .with_count_prefix(),
            );
        }
        for (key, command) in [
            (KeyCode::Up, CURSOR_LINE_UP_SELECT),
            (KeyCode::Down, CURSOR_LINE_DOWN_SELECT),
            (KeyCode::Left, CURSOR_CHAR_LEFT_SELECT),
            (KeyCode::Right, CURSOR_CHAR_RIGHT_SELECT),
        ] {
            keys.push(
                KeyBinding::new(
                    StrokePattern::new(
                        key,
                        ModifierPattern::exact(iridium_editor::Modifiers::shift()),
                    ),
                    &[],
                    command,
                )
                .in_mode(mode.clone()),
            );
        }
        keys.push(
            KeyBinding::new(
                StrokePattern::new(
                    KeyCode::Char('.'),
                    ModifierPattern::exact(iridium_editor::Modifiers::ctrl()),
                ),
                &[],
                COMMAND_REPEAT_SEQUENCE,
            )
            .in_mode(mode.clone()),
        );
        keys.push(
            KeyBinding::new(
                StrokePattern::new(
                    KeyCode::Char('r'),
                    ModifierPattern::exact(iridium_editor::Modifiers::ctrl()),
                ),
                &[],
                HISTORY_REDO,
            )
            .in_mode(mode),
        );
    }
    keys.push(
        KeyBinding::new(StrokePattern::plain(KeyCode::Escape), &[], AST_SELECT_NODE)
            .in_mode(STRUCTURAL_INSERT)
            .then_enter_mode(STRUCTURAL),
    );
    for (character, modifiers) in [
        ('s', iridium_editor::Modifiers::ctrl()),
        ('q', iridium_editor::Modifiers::ctrl()),
        ('w', iridium_editor::Modifiers::ctrl()),
        (
            's',
            iridium_editor::Modifiers {
                ctrl: true,
                alt: true,
                ..iridium_editor::Modifiers::none()
            },
        ),
    ] {
        keys.push(KeyBinding::unbound(
            StrokePattern::new(KeyCode::Char(character), ModifierPattern::exact(modifiers)),
            &[],
        ));
    }
    keys.push(
        KeyBinding::new(StrokePattern::plain(KeyCode::Char('d')), &[], CLIPBOARD_CUT)
            .in_mode(STRUCTURAL),
    );
    for mode in [STRUCTURAL, STRUCTURAL_VERB] {
        keys.push(
            KeyBinding::new(
                StrokePattern::plain(KeyCode::Char('p')),
                &[],
                CLIPBOARD_PASTE,
            )
            .in_mode(mode),
        );
    }
    for verb in ['c', 'd'] {
        for (key, motion) in [
            (KeyCode::Up, CURSOR_LINE_UP_SELECT),
            (KeyCode::Down, CURSOR_LINE_DOWN_SELECT),
            (KeyCode::Left, CURSOR_CHAR_LEFT_SELECT),
            (KeyCode::Right, CURSOR_CHAR_RIGHT_SELECT),
            (KeyCode::Char('k'), AST_NAVIGATE_PREVIOUS),
            (KeyCode::Char('j'), AST_NAVIGATE_NEXT),
            (KeyCode::Char('h'), AST_NAVIGATE_COARSER),
            (KeyCode::Char('l'), AST_NAVIGATE_FINER),
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
    keys
}
