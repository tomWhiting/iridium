//! Translation from winit keyboard events to the kernel's [`KeyEvent`].
//!
//! The kernel binds presses of its own [`KeyCode`] under its own
//! [`Modifiers`]; winit reports logical [`Key`] values under a
//! [`ModifiersState`]. This module is the whole seam between the two — one
//! pure function a test can drive without a window — and it deliberately
//! translates *less* than winit can say:
//!
//! - **Standalone modifier presses are dropped.** Modifiers reach the kernel
//!   on the chord they modify; a bare Shift or Command press carries no verb,
//!   and feeding one to the keymap mid-sequence would tick over a pending
//!   chord that the next real keystroke was about to complete.
//! - **Multi-character clusters are dropped.** The kernel's `Char` holds one
//!   `char`; a logical key carrying a multi-scalar cluster has no honest
//!   single-character reading, and inventing one would corrupt the text. Full
//!   cluster input is IME territory, which is not this slice's.
//! - **Named keys past the kernel's vocabulary are dropped.** F13 and the
//!   media keys have no [`KeyCode`]; only the kernel may grow that enum.
//!
//! Cmd maps to `meta` — the kernel's `Modifiers` already carries the bit —
//! Ctrl to `ctrl`, Option to `alt`, Shift to `shift`. The `alt_graph` bit
//! stays `false`: winit's [`ModifiersState`] does not report `AltGraph`, and
//! the kernel documents the bit as "hosts that can observe it set it".
//!
//! ⚠️ **Which means this face reads its chords with
//! [`KeyLabelStyle::MacGlyphsCommandAsMeta`], never the other mac style.**
//! This paragraph used to claim the label style "already labels it ⌘"; it did
//! not, and had never been checked. The only mac style at the time was written
//! for the web face, which forwards Command as the kernel's `ctrl`, so it
//! spelled `ctrl` as ⌘ and `meta` as ⌃ — and this face, which does the
//! opposite, showed **every chord in its context menu and command palette with
//! ⌘ and ⌃ swapped**. Found 13 Aug 2026 while building the menu bar, by a test
//! that resolved a chord and read what came back rather than trusting a
//! sentence.
//!
//! [`KeyLabelStyle::MacGlyphsCommandAsMeta`]: iridium_editor::KeyLabelStyle::MacGlyphsCommandAsMeta

use iridium_editor::{KeyCode, KeyEvent, Modifiers};
use winit::keyboard::{Key, ModifiersState, NamedKey};

/// Translates one pressed winit key into a kernel key event.
///
/// Answers `None` for a key the kernel has no name for — see the module
/// documentation for what is dropped and why. `repeat` travels through
/// unchanged as `is_repeat`.
pub fn translate(key: &Key, repeat: bool, modifiers: ModifiersState) -> Option<KeyEvent> {
    Some(KeyEvent {
        key: key_code(key)?,
        modifiers: kernel_modifiers(modifiers),
        is_repeat: repeat,
    })
}

/// Maps winit's modifier state onto the kernel's, bit for bit.
///
/// Shift to `shift`, Ctrl to `ctrl`, Option to `alt`, Cmd (Super) to `meta`.
/// `alt_graph` is always `false`; winit cannot observe it.
pub fn kernel_modifiers(state: ModifiersState) -> Modifiers {
    Modifiers {
        shift: state.shift_key(),
        ctrl: state.control_key(),
        alt: state.alt_key(),
        meta: state.super_key(),
        alt_graph: false,
    }
}

/// The kernel key code for a logical key, or `None` for one it cannot name.
fn key_code(key: &Key) -> Option<KeyCode> {
    match key {
        Key::Character(text) => {
            let mut chars = text.chars();
            let first = chars.next()?;
            // A second scalar means a cluster, not a character; see the
            // module documentation.
            chars.next().is_none().then_some(KeyCode::Char(first))
        },
        Key::Named(named) => named_key_code(*named),
        // A dead key composes the *next* press and an unidentified key has no
        // logical meaning; neither is a keystroke the kernel can bind.
        Key::Unidentified(_) | Key::Dead(_) => None,
    }
}

/// The kernel key code for a named key, or `None` for names outside the
/// kernel's vocabulary — standalone modifiers included, on purpose.
const fn named_key_code(named: NamedKey) -> Option<KeyCode> {
    match named {
        // Space is a named key to winit and a character to the kernel.
        NamedKey::Space => Some(KeyCode::Char(' ')),
        NamedKey::ArrowLeft => Some(KeyCode::Left),
        NamedKey::ArrowRight => Some(KeyCode::Right),
        NamedKey::ArrowUp => Some(KeyCode::Up),
        NamedKey::ArrowDown => Some(KeyCode::Down),
        NamedKey::Home => Some(KeyCode::Home),
        NamedKey::End => Some(KeyCode::End),
        NamedKey::PageUp => Some(KeyCode::PageUp),
        NamedKey::PageDown => Some(KeyCode::PageDown),
        NamedKey::Backspace => Some(KeyCode::Backspace),
        NamedKey::Delete => Some(KeyCode::Delete),
        NamedKey::Enter => Some(KeyCode::Enter),
        NamedKey::Tab => Some(KeyCode::Tab),
        NamedKey::Escape => Some(KeyCode::Escape),
        NamedKey::F1 => Some(KeyCode::F1),
        NamedKey::F2 => Some(KeyCode::F2),
        NamedKey::F3 => Some(KeyCode::F3),
        NamedKey::F4 => Some(KeyCode::F4),
        NamedKey::F5 => Some(KeyCode::F5),
        NamedKey::F6 => Some(KeyCode::F6),
        NamedKey::F7 => Some(KeyCode::F7),
        NamedKey::F8 => Some(KeyCode::F8),
        NamedKey::F9 => Some(KeyCode::F9),
        NamedKey::F10 => Some(KeyCode::F10),
        NamedKey::F11 => Some(KeyCode::F11),
        NamedKey::F12 => Some(KeyCode::F12),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use iridium_editor::{KeyCode, Modifiers};
    use winit::keyboard::{Key, ModifiersState, NamedKey, SmolStr};

    use super::{kernel_modifiers, translate};

    /// Every named key the kernel can name, in one table the tests below
    /// share, so a mapping cannot be dropped without a test noticing.
    const NAMED: [(NamedKey, KeyCode); 26] = [
        (NamedKey::Space, KeyCode::Char(' ')),
        (NamedKey::ArrowLeft, KeyCode::Left),
        (NamedKey::ArrowRight, KeyCode::Right),
        (NamedKey::ArrowUp, KeyCode::Up),
        (NamedKey::ArrowDown, KeyCode::Down),
        (NamedKey::Home, KeyCode::Home),
        (NamedKey::End, KeyCode::End),
        (NamedKey::PageUp, KeyCode::PageUp),
        (NamedKey::PageDown, KeyCode::PageDown),
        (NamedKey::Backspace, KeyCode::Backspace),
        (NamedKey::Delete, KeyCode::Delete),
        (NamedKey::Enter, KeyCode::Enter),
        (NamedKey::Tab, KeyCode::Tab),
        (NamedKey::Escape, KeyCode::Escape),
        (NamedKey::F1, KeyCode::F1),
        (NamedKey::F2, KeyCode::F2),
        (NamedKey::F3, KeyCode::F3),
        (NamedKey::F4, KeyCode::F4),
        (NamedKey::F5, KeyCode::F5),
        (NamedKey::F6, KeyCode::F6),
        (NamedKey::F7, KeyCode::F7),
        (NamedKey::F8, KeyCode::F8),
        (NamedKey::F9, KeyCode::F9),
        (NamedKey::F10, KeyCode::F10),
        (NamedKey::F11, KeyCode::F11),
        (NamedKey::F12, KeyCode::F12),
    ];

    #[test]
    fn every_named_key_maps_to_its_kernel_code() {
        for (named, expected) in NAMED {
            let event = translate(&Key::Named(named), false, ModifiersState::empty());
            let event = event.unwrap_or_else(|| panic!("{named:?} should translate"));
            assert_eq!(event.key, expected, "{named:?} maps to the wrong code");
            assert_eq!(event.modifiers, Modifiers::none());
            assert!(!event.is_repeat);
        }
    }

    #[test]
    fn characters_arrive_as_they_were_typed() {
        for (text, expected) in [("a", 'a'), ("A", 'A'), ("ö", 'ö'), ("{", '{'), ("7", '7')] {
            let key = Key::Character(SmolStr::new(text));
            let event = translate(&key, false, ModifiersState::empty());
            let event = event.unwrap_or_else(|| panic!("{text:?} should translate"));
            assert_eq!(event.key, KeyCode::Char(expected));
        }
    }

    #[test]
    fn multi_scalar_clusters_are_dropped() {
        for text in ["ab", "é\u{301}", ""] {
            let key = Key::Character(SmolStr::new(text));
            assert_eq!(
                translate(&key, false, ModifiersState::empty()),
                None,
                "{text:?} has no single-character reading"
            );
        }
    }

    #[test]
    fn standalone_modifiers_and_unknown_names_are_dropped() {
        for named in [
            NamedKey::Shift,
            NamedKey::Control,
            NamedKey::Alt,
            NamedKey::Super,
            NamedKey::CapsLock,
            NamedKey::F13,
            NamedKey::MediaPlayPause,
        ] {
            assert_eq!(
                translate(&Key::Named(named), false, ModifiersState::empty()),
                None,
                "{named:?} should not reach the kernel"
            );
        }
        assert_eq!(
            translate(&Key::Dead(None), false, ModifiersState::empty()),
            None
        );
    }

    #[test]
    fn each_modifier_bit_maps_to_its_kernel_bit() {
        let cases: [(ModifiersState, Modifiers); 4] = [
            (ModifiersState::SHIFT, Modifiers::shift()),
            (ModifiersState::CONTROL, Modifiers::ctrl()),
            (
                ModifiersState::ALT,
                Modifiers {
                    alt: true,
                    ..Modifiers::none()
                },
            ),
            (
                ModifiersState::SUPER,
                Modifiers {
                    meta: true,
                    ..Modifiers::none()
                },
            ),
        ];
        for (state, expected) in cases {
            assert_eq!(kernel_modifiers(state), expected, "{state:?}");
        }
    }

    #[test]
    fn modifier_chords_arrive_intact() {
        let state = ModifiersState::SHIFT | ModifiersState::CONTROL;
        assert_eq!(kernel_modifiers(state), Modifiers::ctrl_shift());

        let everything = ModifiersState::SHIFT
            | ModifiersState::CONTROL
            | ModifiersState::ALT
            | ModifiersState::SUPER;
        assert_eq!(
            kernel_modifiers(everything),
            Modifiers {
                shift: true,
                ctrl: true,
                alt: true,
                meta: true,
                alt_graph: false,
            }
        );

        // The chord the desktop face exists for: ⌘ plus a character.
        let key = Key::Character(SmolStr::new("c"));
        let event = translate(&key, false, ModifiersState::SUPER);
        let event = event.unwrap_or_else(|| panic!("cmd+c should translate"));
        assert_eq!(event.key, KeyCode::Char('c'));
        assert!(event.modifiers.meta);
        assert!(!event.modifiers.ctrl);
    }

    #[test]
    fn repeat_travels_through() {
        let key = Key::Named(NamedKey::ArrowDown);
        let event = translate(&key, true, ModifiersState::empty());
        assert!(event.is_some_and(|event| event.is_repeat));
    }
}
