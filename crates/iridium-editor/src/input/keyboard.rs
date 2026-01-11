//! Keyboard event handling.
//!
//! This module handles keyboard events and translates them into editor actions.
//! It supports standard key bindings and modifier combinations.

use super::types::{InputAction, InputResult};

/// Keyboard modifiers state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    /// Shift key is pressed.
    pub shift: bool,
    /// Control key (Ctrl on Windows/Linux, Cmd on macOS) is pressed.
    pub ctrl: bool,
    /// Alt key (Alt on Windows/Linux, Option on macOS) is pressed.
    pub alt: bool,
    /// Super/Meta key (Windows key on Windows, Cmd on macOS) is pressed.
    pub meta: bool,
}

impl Modifiers {
    /// Creates a new modifiers state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
        }
    }

    /// Creates modifiers with shift pressed.
    #[must_use]
    pub const fn shift() -> Self {
        Self {
            shift: true,
            ctrl: false,
            alt: false,
            meta: false,
        }
    }

    /// Creates modifiers with ctrl pressed.
    #[must_use]
    pub const fn ctrl() -> Self {
        Self {
            shift: false,
            ctrl: true,
            alt: false,
            meta: false,
        }
    }

    /// Creates modifiers with ctrl and shift pressed.
    #[must_use]
    pub const fn ctrl_shift() -> Self {
        Self {
            shift: true,
            ctrl: true,
            alt: false,
            meta: false,
        }
    }

    /// Returns true if no modifiers are pressed.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        !self.shift && !self.ctrl && !self.alt && !self.meta
    }

    /// Returns true if only shift is pressed.
    #[must_use]
    pub const fn only_shift(&self) -> bool {
        self.shift && !self.ctrl && !self.alt && !self.meta
    }

    /// Returns true if only ctrl is pressed.
    #[must_use]
    pub const fn only_ctrl(&self) -> bool {
        !self.shift && self.ctrl && !self.alt && !self.meta
    }

    /// Returns true if ctrl+shift are pressed.
    #[must_use]
    pub const fn is_ctrl_shift(&self) -> bool {
        self.shift && self.ctrl && !self.alt && !self.meta
    }
}

/// Virtual key codes for special keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    // Navigation
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,

    // Editing
    Backspace,
    Delete,
    Enter,
    Tab,

    // Other
    Escape,
    Space,

    // Letters (for shortcuts)
    A,
    C,
    V,
    X,
    Z,
    Y,

    // Function keys (for future use)
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,

    // Character input
    Char(char),

    // Unknown key
    Unknown,
}

impl Key {
    /// Creates a Key from a character.
    #[must_use]
    pub fn from_char(c: char) -> Self {
        match c.to_ascii_lowercase() {
            'a' => Key::A,
            'c' => Key::C,
            'v' => Key::V,
            'x' => Key::X,
            'z' => Key::Z,
            'y' => Key::Y,
            _ => Key::Char(c),
        }
    }
}

/// A keyboard event.
#[derive(Debug, Clone)]
pub struct KeyEvent {
    /// The key that was pressed.
    pub key: Key,
    /// The modifier keys state.
    pub modifiers: Modifiers,
    /// Whether this is a key repeat event.
    pub is_repeat: bool,
}

impl KeyEvent {
    /// Creates a new key event.
    #[must_use]
    pub fn new(key: Key, modifiers: Modifiers) -> Self {
        Self {
            key,
            modifiers,
            is_repeat: false,
        }
    }

    /// Creates a key event with repeat flag.
    #[must_use]
    pub fn with_repeat(mut self, is_repeat: bool) -> Self {
        self.is_repeat = is_repeat;
        self
    }

    /// Creates a key event from a character without modifiers.
    #[must_use]
    pub fn char(c: char) -> Self {
        Self {
            key: Key::Char(c),
            modifiers: Modifiers::new(),
            is_repeat: false,
        }
    }
}

/// Handles keyboard input and translates to editor actions.
#[derive(Debug, Default)]
pub struct KeyHandler {
    /// Whether we're in an IME composition.
    ime_active: bool,
}

impl KeyHandler {
    /// Creates a new key handler.
    #[must_use]
    pub fn new() -> Self {
        Self { ime_active: false }
    }

    /// Sets whether IME composition is active.
    pub fn set_ime_active(&mut self, active: bool) {
        self.ime_active = active;
    }

    /// Returns whether IME composition is active.
    #[must_use]
    pub fn is_ime_active(&self) -> bool {
        self.ime_active
    }

    /// Handles a key event and returns the resulting action.
    #[must_use]
    pub fn handle_key(&self, event: &KeyEvent) -> InputResult {
        // Don't process keys during IME composition (except for cancel)
        if self.ime_active && event.key != Key::Escape {
            return InputResult::not_handled();
        }

        let modifiers = &event.modifiers;
        let key = &event.key;

        // Handle shortcuts first (Ctrl/Cmd + key)
        if modifiers.ctrl || modifiers.meta {
            return self.handle_shortcut(key, modifiers);
        }

        // Handle special keys
        match key {
            // Navigation without modifiers
            Key::Left => {
                if modifiers.only_shift() {
                    InputResult::action(InputAction::SelectLeft)
                } else if modifiers.is_empty() {
                    InputResult::action(InputAction::MoveLeft)
                } else {
                    InputResult::not_handled()
                }
            }
            Key::Right => {
                if modifiers.only_shift() {
                    InputResult::action(InputAction::SelectRight)
                } else if modifiers.is_empty() {
                    InputResult::action(InputAction::MoveRight)
                } else {
                    InputResult::not_handled()
                }
            }
            Key::Up => {
                if modifiers.only_shift() {
                    InputResult::action(InputAction::SelectUp)
                } else if modifiers.is_empty() {
                    InputResult::action(InputAction::MoveUp)
                } else {
                    InputResult::not_handled()
                }
            }
            Key::Down => {
                if modifiers.only_shift() {
                    InputResult::action(InputAction::SelectDown)
                } else if modifiers.is_empty() {
                    InputResult::action(InputAction::MoveDown)
                } else {
                    InputResult::not_handled()
                }
            }
            Key::Home => {
                if modifiers.only_shift() {
                    InputResult::action(InputAction::SelectToLineStart)
                } else if modifiers.is_empty() {
                    InputResult::action(InputAction::MoveToLineStart)
                } else {
                    InputResult::not_handled()
                }
            }
            Key::End => {
                if modifiers.only_shift() {
                    InputResult::action(InputAction::SelectToLineEnd)
                } else if modifiers.is_empty() {
                    InputResult::action(InputAction::MoveToLineEnd)
                } else {
                    InputResult::not_handled()
                }
            }
            Key::PageUp => {
                if modifiers.only_shift() {
                    InputResult::action(InputAction::SelectPageUp)
                } else if modifiers.is_empty() {
                    InputResult::action(InputAction::PageUp)
                } else {
                    InputResult::not_handled()
                }
            }
            Key::PageDown => {
                if modifiers.only_shift() {
                    InputResult::action(InputAction::SelectPageDown)
                } else if modifiers.is_empty() {
                    InputResult::action(InputAction::PageDown)
                } else {
                    InputResult::not_handled()
                }
            }

            // Editing keys
            Key::Backspace => {
                if modifiers.is_empty() || modifiers.only_shift() {
                    InputResult::action(InputAction::Backspace)
                } else {
                    InputResult::not_handled()
                }
            }
            Key::Delete => {
                if modifiers.is_empty() || modifiers.only_shift() {
                    InputResult::action(InputAction::Delete)
                } else {
                    InputResult::not_handled()
                }
            }
            Key::Enter => {
                if modifiers.is_empty() || modifiers.only_shift() {
                    InputResult::action(InputAction::InsertNewline)
                } else {
                    InputResult::not_handled()
                }
            }
            Key::Tab => {
                if modifiers.is_empty() {
                    InputResult::action(InputAction::InsertChar('\t'))
                } else {
                    InputResult::not_handled()
                }
            }
            Key::Space => {
                if modifiers.is_empty() {
                    InputResult::action(InputAction::InsertChar(' '))
                } else {
                    InputResult::not_handled()
                }
            }

            // Escape cancels IME or clears selection
            Key::Escape => {
                if self.ime_active {
                    InputResult::action(InputAction::ImeCompositionCancel)
                } else {
                    InputResult::consumed() // Clear selection handled by editor
                }
            }

            // Character input
            Key::Char(c) => {
                if modifiers.is_empty() || modifiers.only_shift() {
                    InputResult::action(InputAction::InsertChar(*c))
                } else {
                    InputResult::not_handled()
                }
            }

            // Letter keys without ctrl/cmd handled as characters
            Key::A | Key::C | Key::V | Key::X | Key::Z | Key::Y => {
                if modifiers.is_empty() || modifiers.only_shift() {
                    let c = match key {
                        Key::A => 'a',
                        Key::C => 'c',
                        Key::V => 'v',
                        Key::X => 'x',
                        Key::Z => 'z',
                        Key::Y => 'y',
                        _ => return InputResult::not_handled(),
                    };
                    let c = if modifiers.shift {
                        c.to_ascii_uppercase()
                    } else {
                        c
                    };
                    InputResult::action(InputAction::InsertChar(c))
                } else {
                    InputResult::not_handled()
                }
            }

            _ => InputResult::not_handled(),
        }
    }

    /// Handles keyboard shortcuts (Ctrl/Cmd + key).
    fn handle_shortcut(&self, key: &Key, modifiers: &Modifiers) -> InputResult {
        // Check for Ctrl+Shift combinations first
        if modifiers.is_ctrl_shift() {
            return match key {
                Key::Left => InputResult::action(InputAction::SelectWordLeft),
                Key::Right => InputResult::action(InputAction::SelectWordRight),
                Key::Home => InputResult::action(InputAction::SelectToDocumentStart),
                Key::End => InputResult::action(InputAction::SelectToDocumentEnd),
                Key::Z | Key::Y => InputResult::action(InputAction::Redo),
                _ => InputResult::not_handled(),
            };
        }

        // Ctrl/Cmd only combinations
        if modifiers.only_ctrl() || (modifiers.meta && !modifiers.shift && !modifiers.alt) {
            return match key {
                // Navigation
                Key::Left => InputResult::action(InputAction::MoveWordLeft),
                Key::Right => InputResult::action(InputAction::MoveWordRight),
                Key::Home => InputResult::action(InputAction::MoveToDocumentStart),
                Key::End => InputResult::action(InputAction::MoveToDocumentEnd),

                // Selection
                Key::A => InputResult::action(InputAction::SelectAll),

                // Clipboard
                Key::C => InputResult::action(InputAction::Copy),
                Key::X => InputResult::action(InputAction::Cut),
                Key::V => InputResult::action(InputAction::Paste),

                // History
                Key::Z => InputResult::action(InputAction::Undo),
                Key::Y => InputResult::action(InputAction::Redo),

                // Delete word
                Key::Backspace => InputResult::action(InputAction::DeleteWordBefore),
                Key::Delete => InputResult::action(InputAction::DeleteWordAfter),

                _ => InputResult::not_handled(),
            };
        }

        InputResult::not_handled()
    }

    /// Handles text input (for direct character input, bypassing key events).
    #[must_use]
    pub fn handle_text_input(&self, text: &str) -> InputResult {
        if self.ime_active {
            return InputResult::not_handled();
        }

        if text.is_empty() {
            return InputResult::consumed();
        }

        // For single characters, use InsertChar
        let chars: Vec<char> = text.chars().collect();
        if chars.len() == 1 {
            InputResult::action(InputAction::InsertChar(chars[0]))
        } else {
            InputResult::action(InputAction::InsertText(text.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modifiers_new() {
        let m = Modifiers::new();
        assert!(m.is_empty());
        assert!(!m.shift);
        assert!(!m.ctrl);
    }

    #[test]
    fn test_modifiers_shift() {
        let m = Modifiers::shift();
        assert!(m.only_shift());
        assert!(m.shift);
        assert!(!m.ctrl);
    }

    #[test]
    fn test_modifiers_ctrl() {
        let m = Modifiers::ctrl();
        assert!(m.only_ctrl());
        assert!(m.ctrl);
        assert!(!m.shift);
    }

    #[test]
    fn test_modifiers_ctrl_shift() {
        let m = Modifiers::ctrl_shift();
        assert!(m.is_ctrl_shift());
        assert!(m.ctrl);
        assert!(m.shift);
    }

    #[test]
    fn test_key_from_char() {
        assert_eq!(Key::from_char('a'), Key::A);
        assert_eq!(Key::from_char('A'), Key::A);
        assert_eq!(Key::from_char('z'), Key::Z);
        assert_eq!(Key::from_char('1'), Key::Char('1'));
    }

    #[test]
    fn test_key_event_new() {
        let event = KeyEvent::new(Key::Left, Modifiers::new());
        assert_eq!(event.key, Key::Left);
        assert!(event.modifiers.is_empty());
        assert!(!event.is_repeat);
    }

    #[test]
    fn test_key_handler_arrow_keys() {
        let handler = KeyHandler::new();

        // Left arrow without modifiers
        let event = KeyEvent::new(Key::Left, Modifiers::new());
        let result = handler.handle_key(&event);
        assert!(matches!(result.actions[0], InputAction::MoveLeft));

        // Left arrow with shift
        let event = KeyEvent::new(Key::Left, Modifiers::shift());
        let result = handler.handle_key(&event);
        assert!(matches!(result.actions[0], InputAction::SelectLeft));

        // Left arrow with ctrl
        let event = KeyEvent::new(Key::Left, Modifiers::ctrl());
        let result = handler.handle_key(&event);
        assert!(matches!(result.actions[0], InputAction::MoveWordLeft));

        // Left arrow with ctrl+shift
        let event = KeyEvent::new(Key::Left, Modifiers::ctrl_shift());
        let result = handler.handle_key(&event);
        assert!(matches!(result.actions[0], InputAction::SelectWordLeft));
    }

    #[test]
    fn test_key_handler_home_end() {
        let handler = KeyHandler::new();

        // Home without modifiers
        let event = KeyEvent::new(Key::Home, Modifiers::new());
        let result = handler.handle_key(&event);
        assert!(matches!(result.actions[0], InputAction::MoveToLineStart));

        // Home with shift
        let event = KeyEvent::new(Key::Home, Modifiers::shift());
        let result = handler.handle_key(&event);
        assert!(matches!(result.actions[0], InputAction::SelectToLineStart));

        // Home with ctrl
        let event = KeyEvent::new(Key::Home, Modifiers::ctrl());
        let result = handler.handle_key(&event);
        assert!(matches!(
            result.actions[0],
            InputAction::MoveToDocumentStart
        ));
    }

    #[test]
    fn test_key_handler_backspace_delete() {
        let handler = KeyHandler::new();

        // Backspace
        let event = KeyEvent::new(Key::Backspace, Modifiers::new());
        let result = handler.handle_key(&event);
        assert!(matches!(result.actions[0], InputAction::Backspace));

        // Delete
        let event = KeyEvent::new(Key::Delete, Modifiers::new());
        let result = handler.handle_key(&event);
        assert!(matches!(result.actions[0], InputAction::Delete));

        // Ctrl+Backspace
        let event = KeyEvent::new(Key::Backspace, Modifiers::ctrl());
        let result = handler.handle_key(&event);
        assert!(matches!(result.actions[0], InputAction::DeleteWordBefore));
    }

    #[test]
    fn test_key_handler_clipboard_shortcuts() {
        let handler = KeyHandler::new();

        // Ctrl+C
        let event = KeyEvent::new(Key::C, Modifiers::ctrl());
        let result = handler.handle_key(&event);
        assert!(matches!(result.actions[0], InputAction::Copy));

        // Ctrl+X
        let event = KeyEvent::new(Key::X, Modifiers::ctrl());
        let result = handler.handle_key(&event);
        assert!(matches!(result.actions[0], InputAction::Cut));

        // Ctrl+V
        let event = KeyEvent::new(Key::V, Modifiers::ctrl());
        let result = handler.handle_key(&event);
        assert!(matches!(result.actions[0], InputAction::Paste));
    }

    #[test]
    fn test_key_handler_undo_redo() {
        let handler = KeyHandler::new();

        // Ctrl+Z
        let event = KeyEvent::new(Key::Z, Modifiers::ctrl());
        let result = handler.handle_key(&event);
        assert!(matches!(result.actions[0], InputAction::Undo));

        // Ctrl+Y
        let event = KeyEvent::new(Key::Y, Modifiers::ctrl());
        let result = handler.handle_key(&event);
        assert!(matches!(result.actions[0], InputAction::Redo));

        // Ctrl+Shift+Z
        let event = KeyEvent::new(Key::Z, Modifiers::ctrl_shift());
        let result = handler.handle_key(&event);
        assert!(matches!(result.actions[0], InputAction::Redo));
    }

    #[test]
    fn test_key_handler_select_all() {
        let handler = KeyHandler::new();

        let event = KeyEvent::new(Key::A, Modifiers::ctrl());
        let result = handler.handle_key(&event);
        assert!(matches!(result.actions[0], InputAction::SelectAll));
    }

    #[test]
    fn test_key_handler_character_input() {
        let handler = KeyHandler::new();

        // Regular character
        let event = KeyEvent::char('a');
        let result = handler.handle_key(&event);
        assert!(matches!(result.actions[0], InputAction::InsertChar('a')));

        // Character with shift (uppercase)
        let event = KeyEvent::new(Key::Char('A'), Modifiers::shift());
        let result = handler.handle_key(&event);
        assert!(matches!(result.actions[0], InputAction::InsertChar('A')));
    }

    #[test]
    fn test_key_handler_enter_tab() {
        let handler = KeyHandler::new();

        // Enter
        let event = KeyEvent::new(Key::Enter, Modifiers::new());
        let result = handler.handle_key(&event);
        assert!(matches!(result.actions[0], InputAction::InsertNewline));

        // Tab
        let event = KeyEvent::new(Key::Tab, Modifiers::new());
        let result = handler.handle_key(&event);
        assert!(matches!(result.actions[0], InputAction::InsertChar('\t')));
    }

    #[test]
    fn test_key_handler_text_input() {
        let handler = KeyHandler::new();

        // Single character
        let result = handler.handle_text_input("a");
        assert!(matches!(result.actions[0], InputAction::InsertChar('a')));

        // Multiple characters
        let result = handler.handle_text_input("hello");
        assert!(matches!(
            result.actions[0],
            InputAction::InsertText(ref s) if s == "hello"
        ));

        // Empty string
        let result = handler.handle_text_input("");
        assert!(result.consumed);
        assert!(result.actions.is_empty());
    }

    #[test]
    fn test_key_handler_ime_active() {
        let mut handler = KeyHandler::new();
        handler.set_ime_active(true);

        // Keys should not be handled during IME (except Escape)
        let event = KeyEvent::char('a');
        let result = handler.handle_key(&event);
        assert!(!result.consumed);

        // Escape should still work to cancel IME
        let event = KeyEvent::new(Key::Escape, Modifiers::new());
        let result = handler.handle_key(&event);
        assert!(matches!(
            result.actions[0],
            InputAction::ImeCompositionCancel
        ));
    }
}
