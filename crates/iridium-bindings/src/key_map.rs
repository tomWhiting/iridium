//! Pure mapping from DOM `KeyboardEvent.key` strings to editor key codes.
//!
//! This module is the single translation point between the browser's
//! keyboard event vocabulary and [`iridium_editor::KeyCode`]. It is kept
//! free of `wasm-bindgen` so it compiles (and its unit tests run) on every
//! target, not just `wasm32`.
//!
//! Mapping rules:
//!
//! - A key value consisting of exactly one Unicode scalar (`"a"`, `"Ж"`,
//!   `" "`, `"("`, …) maps to [`KeyCode::Char`].
//! - Named editing/navigation keys (`"Enter"`, `"Tab"`, `"Backspace"`,
//!   `"Delete"`, `"Escape"`, the arrow keys, `"Home"`, `"End"`,
//!   `"PageUp"`, `"PageDown"`) map to their dedicated variants.
//! - Function keys `"F1"` through `"F12"` map to their variants.
//! - Everything else — bare modifier presses (`"Shift"`, `"Control"`,
//!   `"Alt"`, `"Meta"`, `"CapsLock"`), IME intermediates (`"Dead"`,
//!   `"Process"`), media keys, `"Unidentified"`, … — maps to `None` and
//!   must be left to the browser without ever reaching the Rust core.

use iridium_editor::KeyCode;

/// Converts a DOM `KeyboardEvent.key` string to an editor [`KeyCode`].
///
/// Returns `None` for keys the editor core has no representation for;
/// callers must treat those as unhandled (browser default behavior).
#[must_use]
pub fn key_code_from_dom_key(key: &str) -> Option<KeyCode> {
    // Exactly one Unicode scalar value → printable character key.
    // (All named DOM key values are multi-character ASCII strings, so this
    // cannot shadow them.)
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Some(KeyCode::Char(c));
    }

    let code = match key {
        "Enter" => KeyCode::Enter,
        "Tab" => KeyCode::Tab,
        "Backspace" => KeyCode::Backspace,
        "Delete" => KeyCode::Delete,
        "Escape" => KeyCode::Escape,
        "ArrowLeft" => KeyCode::Left,
        "ArrowRight" => KeyCode::Right,
        "ArrowUp" => KeyCode::Up,
        "ArrowDown" => KeyCode::Down,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "PageUp" => KeyCode::PageUp,
        "PageDown" => KeyCode::PageDown,
        "F1" => KeyCode::F1,
        "F2" => KeyCode::F2,
        "F3" => KeyCode::F3,
        "F4" => KeyCode::F4,
        "F5" => KeyCode::F5,
        "F6" => KeyCode::F6,
        "F7" => KeyCode::F7,
        "F8" => KeyCode::F8,
        "F9" => KeyCode::F9,
        "F10" => KeyCode::F10,
        "F11" => KeyCode::F11,
        "F12" => KeyCode::F12,
        _ => return None,
    };
    Some(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_characters_map_to_char() {
        assert_eq!(key_code_from_dom_key("a"), Some(KeyCode::Char('a')));
        assert_eq!(key_code_from_dom_key("Z"), Some(KeyCode::Char('Z')));
        assert_eq!(key_code_from_dom_key("0"), Some(KeyCode::Char('0')));
        assert_eq!(key_code_from_dom_key("("), Some(KeyCode::Char('(')));
        assert_eq!(key_code_from_dom_key("{"), Some(KeyCode::Char('{')));
        assert_eq!(key_code_from_dom_key("\""), Some(KeyCode::Char('"')));
    }

    #[test]
    fn space_is_a_character() {
        assert_eq!(key_code_from_dom_key(" "), Some(KeyCode::Char(' ')));
    }

    #[test]
    fn non_ascii_single_scalars_map_to_char() {
        assert_eq!(key_code_from_dom_key("é"), Some(KeyCode::Char('é')));
        assert_eq!(key_code_from_dom_key("Ж"), Some(KeyCode::Char('Ж')));
        assert_eq!(key_code_from_dom_key("ø"), Some(KeyCode::Char('ø')));
    }

    #[test]
    fn named_editing_keys_map_to_variants() {
        assert_eq!(key_code_from_dom_key("Enter"), Some(KeyCode::Enter));
        assert_eq!(key_code_from_dom_key("Tab"), Some(KeyCode::Tab));
        assert_eq!(key_code_from_dom_key("Backspace"), Some(KeyCode::Backspace));
        assert_eq!(key_code_from_dom_key("Delete"), Some(KeyCode::Delete));
        assert_eq!(key_code_from_dom_key("Escape"), Some(KeyCode::Escape));
    }

    #[test]
    fn navigation_keys_map_to_variants() {
        assert_eq!(key_code_from_dom_key("ArrowLeft"), Some(KeyCode::Left));
        assert_eq!(key_code_from_dom_key("ArrowRight"), Some(KeyCode::Right));
        assert_eq!(key_code_from_dom_key("ArrowUp"), Some(KeyCode::Up));
        assert_eq!(key_code_from_dom_key("ArrowDown"), Some(KeyCode::Down));
        assert_eq!(key_code_from_dom_key("Home"), Some(KeyCode::Home));
        assert_eq!(key_code_from_dom_key("End"), Some(KeyCode::End));
        assert_eq!(key_code_from_dom_key("PageUp"), Some(KeyCode::PageUp));
        assert_eq!(key_code_from_dom_key("PageDown"), Some(KeyCode::PageDown));
    }

    #[test]
    fn function_keys_map_to_variants() {
        assert_eq!(key_code_from_dom_key("F1"), Some(KeyCode::F1));
        assert_eq!(key_code_from_dom_key("F2"), Some(KeyCode::F2));
        assert_eq!(key_code_from_dom_key("F3"), Some(KeyCode::F3));
        assert_eq!(key_code_from_dom_key("F4"), Some(KeyCode::F4));
        assert_eq!(key_code_from_dom_key("F5"), Some(KeyCode::F5));
        assert_eq!(key_code_from_dom_key("F6"), Some(KeyCode::F6));
        assert_eq!(key_code_from_dom_key("F7"), Some(KeyCode::F7));
        assert_eq!(key_code_from_dom_key("F8"), Some(KeyCode::F8));
        assert_eq!(key_code_from_dom_key("F9"), Some(KeyCode::F9));
        assert_eq!(key_code_from_dom_key("F10"), Some(KeyCode::F10));
        assert_eq!(key_code_from_dom_key("F11"), Some(KeyCode::F11));
        assert_eq!(key_code_from_dom_key("F12"), Some(KeyCode::F12));
    }

    #[test]
    fn bare_modifier_presses_are_unmapped() {
        // The Rust core has KeyCode variants for these, but a bare modifier
        // press carries no editing intent; it must never reach the handler.
        assert_eq!(key_code_from_dom_key("Shift"), None);
        assert_eq!(key_code_from_dom_key("Control"), None);
        assert_eq!(key_code_from_dom_key("Alt"), None);
        assert_eq!(key_code_from_dom_key("Meta"), None);
        assert_eq!(key_code_from_dom_key("CapsLock"), None);
    }

    #[test]
    fn ime_and_special_keys_are_unmapped() {
        assert_eq!(key_code_from_dom_key("Dead"), None);
        assert_eq!(key_code_from_dom_key("Process"), None);
        assert_eq!(key_code_from_dom_key("Unidentified"), None);
        assert_eq!(key_code_from_dom_key("Compose"), None);
        assert_eq!(key_code_from_dom_key("ContextMenu"), None);
        assert_eq!(key_code_from_dom_key("Insert"), None);
        assert_eq!(key_code_from_dom_key("NumLock"), None);
        assert_eq!(key_code_from_dom_key("ScrollLock"), None);
        assert_eq!(key_code_from_dom_key("MediaPlayPause"), None);
        assert_eq!(key_code_from_dom_key("AudioVolumeUp"), None);
        assert_eq!(key_code_from_dom_key("F13"), None);
        assert_eq!(key_code_from_dom_key(""), None);
    }

    #[test]
    fn multi_scalar_strings_are_unmapped() {
        // Some layouts can report multi-character key values; they are not
        // single character insertions and must not be treated as such.
        assert_eq!(key_code_from_dom_key("ab"), None);
    }
}
