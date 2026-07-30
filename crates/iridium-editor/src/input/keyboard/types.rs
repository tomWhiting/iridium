//! Keyboard input types: key codes, modifiers, events, and handler results.

use serde::{Deserialize, Serialize};

use crate::commands::{CommandArgs, CommandId};
use crate::history::Command;

/// Key codes for keyboard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyCode {
    /// Alphanumeric and symbol keys
    Char(char),

    /// Left arrow key
    Left,
    /// Right arrow key
    Right,
    /// Up arrow key
    Up,
    /// Down arrow key
    Down,

    /// Home key (go to start of line)
    Home,
    /// End key (go to end of line)
    End,
    /// Page Up key
    PageUp,
    /// Page Down key
    PageDown,

    /// Backspace key (delete character before cursor)
    Backspace,
    /// Delete key (delete character after cursor)
    Delete,
    /// Enter/Return key
    Enter,
    /// Tab key
    Tab,

    /// Shift modifier key
    Shift,
    /// Control modifier key
    Control,
    /// Alt/Option modifier key
    Alt,
    /// Meta/Windows/Command modifier key
    Meta,

    /// Escape key
    Escape,

    /// Function key F1
    F1,
    /// Function key F2
    F2,
    /// Function key F3
    F3,
    /// Function key F4
    F4,
    /// Function key F5
    F5,
    /// Function key F6
    F6,
    /// Function key F7
    F7,
    /// Function key F8
    F8,
    /// Function key F9
    F9,
    /// Function key F10
    F10,
    /// Function key F11
    F11,
    /// Function key F12
    F12,
}

/// Keyboard modifier state.
#[allow(clippy::struct_excessive_bools)] // Mirrors the hardware modifier keys.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Modifiers {
    /// Shift key is held
    pub shift: bool,
    /// Control key is held (Cmd on Mac)
    pub ctrl: bool,
    /// Alt key is held (Option on Mac)
    pub alt: bool,
    /// Meta key is held (Win key, or Cmd on Mac)
    pub meta: bool,
    /// `AltGraph` (`AltGr`) is active.
    ///
    /// On many non-US layouts `AltGr` is reported by the browser as
    /// `ctrl` + `alt` held together while composing a character (e.g. `@`, `€`).
    /// That coincides exactly with the Ctrl+Alt chord used for add-cursor
    /// above/below, so the two are indistinguishable from the `ctrl`/`alt` bits
    /// alone. Hosts that can observe `getModifierState("AltGraph")` set this bit
    /// so modifier chords (see [`super::KeyboardHandler`]) can exclude `AltGr`
    /// and let the character compose untouched. Defaults to `false`.
    #[serde(default)]
    pub alt_graph: bool,
}

impl Modifiers {
    /// No modifiers pressed.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
            alt_graph: false,
        }
    }

    /// Shift only.
    #[must_use]
    pub const fn shift() -> Self {
        Self {
            shift: true,
            ctrl: false,
            alt: false,
            meta: false,
            alt_graph: false,
        }
    }

    /// Ctrl only.
    #[must_use]
    pub const fn ctrl() -> Self {
        Self {
            shift: false,
            ctrl: true,
            alt: false,
            meta: false,
            alt_graph: false,
        }
    }

    /// Ctrl+Shift.
    #[must_use]
    pub const fn ctrl_shift() -> Self {
        Self {
            shift: true,
            ctrl: true,
            alt: false,
            meta: false,
            alt_graph: false,
        }
    }
}

/// A keyboard event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyEvent {
    /// The key that was pressed
    pub key: KeyCode,
    /// Active modifiers
    pub modifiers: Modifiers,
    /// True if this is a key repeat event
    pub is_repeat: bool,
}

impl KeyEvent {
    /// Creates a new key event.
    #[must_use]
    pub const fn new(key: KeyCode, modifiers: Modifiers) -> Self {
        Self {
            key,
            modifiers,
            is_repeat: false,
        }
    }

    /// Creates a new key event with no modifiers.
    #[must_use]
    pub const fn simple(key: KeyCode) -> Self {
        Self::new(key, Modifiers::none())
    }
}

/// Search-related actions triggered by keyboard input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchAction {
    /// Open the search panel (Ctrl+F)
    OpenSearch,
    /// Go to next match (F3 or Enter in search)
    NextMatch,
    /// Go to previous match (Shift+F3)
    PreviousMatch,
    /// Close search panel (Escape when search is open)
    CloseSearch,
}

/// Result of handling a keyboard event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyResult {
    /// The event was handled, no further action needed
    Handled,

    /// The event was handled and a command was produced
    Command(Command),

    /// The event triggered a clipboard operation
    Clipboard(ClipboardOperation),

    /// The event triggered a search action (T121, T122)
    Search(SearchAction),

    /// A binding resolved to a command this kernel does not implement.
    ///
    /// The keypress was **consumed** and the command named: it is the caller's to
    /// run. This is the whole extensibility path for commands contributed from
    /// outside the kernel — a host registers a [`CommandMeta`](crate::CommandMeta)
    /// so the palette lists it, binds it in a pushed
    /// [`Keymap`](crate::Keymap), and receives this when the binding fires.
    ///
    /// Reporting it is not optional politeness. Before this variant existed the id
    /// was dropped and the key reported as [`Self::Ignored`], which is
    /// indistinguishable from a meaningless keypress; for a multi-stroke sequence
    /// the earlier strokes had already returned [`Self::Handled`], so the host
    /// could not even reconstruct which sequence had completed.
    HostCommand {
        /// The resolved command id.
        command: CommandId,
        /// The count and captured characters the key sequence carried.
        args: CommandArgs,
    },

    /// The event was not handled (pass to next handler)
    Ignored,
}

/// Why a command named by id could not be run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandRunError {
    /// The kernel implements no command with this id.
    ///
    /// Not necessarily a mistake: the id may be a host command, registered in the
    /// [`CommandRegistry`](crate::CommandRegistry) and implemented outside the
    /// kernel. The caller is the one that knows, which is why this is an error
    /// value rather than a silent no-op.
    #[error("the editing kernel implements no command with id `{id}`")]
    Unimplemented {
        /// The id that has no kernel implementation.
        id: String,
    },
}

/// Clipboard operation requested by keyboard handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardOperation {
    /// Copy selected text
    Copy(String),
    /// Cut selected text (returns text to copy and command to delete)
    Cut {
        /// The text to copy to clipboard
        text: String,
        /// The command to apply to delete the selection
        command: Command,
    },
    /// Request paste from clipboard
    Paste,
}
