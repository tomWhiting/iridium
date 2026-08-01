//! The key-code and modifier mapping, from `terminput` to the kernel.
//!
//! Split out of the parent module so the mapping tables sit on their own and
//! the event routing stays readable. Nothing here is stateful; both functions
//! are `const` and total.

use iridium_editor::{KeyCode, Modifiers};
use terminput::{KeyModifiers, ModifierKeyCode};

/// The terminal modifiers that collapse onto the kernel's single meta bit.
///
/// The kernel models four hardware modifiers, matching what a browser reports:
/// shift, control, alt and meta. The kitty protocol carries six, adding *super*
/// and *hyper*. Super is the Windows/Command key, which is exactly what the
/// kernel calls meta; hyper is a modifier almost no keyboard has a key for and
/// which the kernel has no bit for at all.
///
/// All three are merged rather than any being dropped. Dropping one would make
/// `Hyper+X` arrive as a bare `X` and insert a character the user did not type;
/// merging makes it a meta chord, which is wrong only in that it cannot be told
/// apart from `Super+X`. A modifier that is merged is still a modifier; one
/// that is dropped is a typo in the document.
const META_LIKE: KeyModifiers = KeyModifiers::SUPER
    .union(KeyModifiers::META)
    .union(KeyModifiers::HYPER);

/// Converts terminal modifiers to the kernel's modifier set.
///
/// [`Modifiers::alt_graph`] is always `false`, and that is a statement about
/// terminals rather than an omission. The bit exists because a browser can ask
/// `getModifierState("AltGraph")` and so distinguish an `AltGr`-composed
/// character from the `Ctrl+Alt` chord it looks identical to. No terminal
/// protocol reports it: the kitty protocol has no `AltGr` modifier bit, and a
/// terminal that composes `€` from `AltGr+E` sends the composed character with
/// no modifiers at all. Setting the bit on a guess would suppress the
/// `Ctrl+Alt` chords the kernel uses for add-cursor-above/below.
pub const fn modifiers(modifiers: KeyModifiers) -> Modifiers {
    Modifiers {
        shift: modifiers.intersects(KeyModifiers::SHIFT),
        ctrl: modifiers.intersects(KeyModifiers::CTRL),
        alt: modifiers.intersects(KeyModifiers::ALT),
        meta: modifiers.intersects(META_LIKE),
        alt_graph: false,
    }
}

/// Converts a terminal key code to the kernel's, or `None` when the kernel has
/// no name for it.
///
/// `None` is not a failure and not a silence: the caller reports it as
/// [`TerminalInput::Unmapped`](super::TerminalInput::Unmapped). The keys it
/// covers are Insert, F13 and above, the lock keys, Print Screen, Pause, Menu,
/// the keypad Begin key, the media keys, and the ISO level-3/5 shift keys —
/// every one of them a key the kernel's [`KeyCode`] cannot express. Adding one
/// means adding it to the kernel first; inventing a code here is the parallel
/// key type this crate exists not to have.
pub const fn key_code(code: terminput::KeyCode) -> Option<KeyCode> {
    use terminput::KeyCode as Tc;

    Some(match code {
        Tc::Char(c) => KeyCode::Char(c),

        Tc::Left => KeyCode::Left,
        Tc::Right => KeyCode::Right,
        Tc::Up => KeyCode::Up,
        Tc::Down => KeyCode::Down,
        Tc::Home => KeyCode::Home,
        Tc::End => KeyCode::End,
        Tc::PageUp => KeyCode::PageUp,
        Tc::PageDown => KeyCode::PageDown,

        Tc::Backspace => KeyCode::Backspace,
        Tc::Delete => KeyCode::Delete,
        Tc::Enter => KeyCode::Enter,
        Tc::Tab => KeyCode::Tab,
        Tc::Esc => KeyCode::Escape,

        Tc::F(n) => return function_key(n),

        // A modifier reported as a key in its own right, which only the kitty
        // protocol does. Super and hyper join meta for the reason given on
        // `META_LIKE`: the kernel has one bit and one key code for the family.
        Tc::Modifier(ModifierKeyCode::Shift, _) => KeyCode::Shift,
        Tc::Modifier(ModifierKeyCode::Control, _) => KeyCode::Control,
        Tc::Modifier(ModifierKeyCode::Alt, _) => KeyCode::Alt,
        Tc::Modifier(
            ModifierKeyCode::Super | ModifierKeyCode::Meta | ModifierKeyCode::Hyper,
            _,
        ) => KeyCode::Meta,

        Tc::Insert
        | Tc::CapsLock
        | Tc::ScrollLock
        | Tc::NumLock
        | Tc::PrintScreen
        | Tc::Pause
        | Tc::Menu
        | Tc::KeypadBegin
        | Tc::Media(_)
        | Tc::Modifier(ModifierKeyCode::IsoLevel3Shift | ModifierKeyCode::IsoLevel5Shift, _) => {
            return None;
        },
    })
}

/// Maps a function-key number to the kernel's code.
///
/// The kernel names F1 to F12. The kitty protocol can report up to F35, and a
/// terminal reports F13 and above for shifted function keys on some keyboards;
/// those have no kernel code and are reported unmapped rather than folded onto
/// a lower key, which would fire the wrong binding.
const fn function_key(number: u8) -> Option<KeyCode> {
    Some(match number {
        1 => KeyCode::F1,
        2 => KeyCode::F2,
        3 => KeyCode::F3,
        4 => KeyCode::F4,
        5 => KeyCode::F5,
        6 => KeyCode::F6,
        7 => KeyCode::F7,
        8 => KeyCode::F8,
        9 => KeyCode::F9,
        10 => KeyCode::F10,
        11 => KeyCode::F11,
        12 => KeyCode::F12,
        _ => return None,
    })
}
