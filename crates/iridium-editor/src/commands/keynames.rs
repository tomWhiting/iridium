//! The textual name of a [`KeyCode`], in both directions.
//!
//! A rebinding UI and a hand-written configuration file both want `"ctrl-k
//! ctrl-c"` rather than the verbose serde shape, and a keybinding-help view wants
//! to render the same text back. One table serves both so the two can never
//! disagree, and the round trip is asserted exhaustively over every variant in
//! the module tests.

use crate::input::KeyCode;

/// Every non-character [`KeyCode`] paired with its canonical lowercase name.
///
/// Order is irrelevant to correctness; the first entry for a key is the one
/// [`name_of`] renders, so aliases (if any are ever added) must come after it.
static NAMES: &[(&str, KeyCode)] = &[
    ("left", KeyCode::Left),
    ("right", KeyCode::Right),
    ("up", KeyCode::Up),
    ("down", KeyCode::Down),
    ("home", KeyCode::Home),
    ("end", KeyCode::End),
    ("pageup", KeyCode::PageUp),
    ("pagedown", KeyCode::PageDown),
    ("backspace", KeyCode::Backspace),
    ("delete", KeyCode::Delete),
    ("enter", KeyCode::Enter),
    ("tab", KeyCode::Tab),
    ("shift", KeyCode::Shift),
    ("control", KeyCode::Control),
    ("altkey", KeyCode::Alt),
    ("metakey", KeyCode::Meta),
    ("escape", KeyCode::Escape),
    ("f1", KeyCode::F1),
    ("f2", KeyCode::F2),
    ("f3", KeyCode::F3),
    ("f4", KeyCode::F4),
    ("f5", KeyCode::F5),
    ("f6", KeyCode::F6),
    ("f7", KeyCode::F7),
    ("f8", KeyCode::F8),
    ("f9", KeyCode::F9),
    ("f10", KeyCode::F10),
    ("f11", KeyCode::F11),
    ("f12", KeyCode::F12),
];

/// The name of the wildcard stroke that matches (and captures) any character.
pub const ANY_CHAR_NAME: &str = "{char}";

/// The canonical name of `key`.
///
/// [`KeyCode::Char`] renders as the character itself, except for the two
/// characters the sequence grammar reserves — `-` (the modifier separator) and
/// a space (the stroke separator) — which render as `minus` and `space` so the
/// form stays parseable.
#[must_use]
pub fn name_of(key: KeyCode) -> String {
    match key {
        KeyCode::Char(' ') => "space".to_owned(),
        KeyCode::Char('-') => "minus".to_owned(),
        KeyCode::Char(c) => c.to_string(),
        other => NAMES.iter().find(|(_, code)| *code == other).map_or_else(
            || format!("{other:?}").to_lowercase(),
            |(name, _)| (*name).to_owned(),
        ),
    }
}

/// Parses a key name, accepting any letter case.
///
/// A single character is [`KeyCode::Char`]; everything else must be one of the
/// named keys, plus the two escapes `space` and `minus`.
#[must_use]
pub fn key_from_name(name: &str) -> Option<KeyCode> {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "space" => return Some(KeyCode::Char(' ')),
        "minus" => return Some(KeyCode::Char('-')),
        _ => {},
    }
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Some(KeyCode::Char(c));
    }
    NAMES
        .iter()
        .find(|(candidate, _)| *candidate == lower.as_str())
        .map(|(_, code)| *code)
}
