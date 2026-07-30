//! Key presses and the binding patterns that match them.
//!
//! The human-readable chord form (`ctrl+shift+k`) lives in `stroke_text`, and
//! [`KeyBinding`](super::KeyBinding) in `binding` — three concerns that grew
//! one file past the module cap between them.

use serde::{Deserialize, Serialize};

use super::ModifierPattern;
use crate::input::{KeyCode, KeyEvent, Modifiers};

/// One concrete keypress, as observed from the host.
///
/// This is the *input* to keymap resolution: a real [`KeyCode`] with the real
/// [`Modifiers`] that were held. Contrast [`StrokePattern`], which is what a
/// binding stores and can be looser.
///
/// # Normalization
///
/// [`Self::normalized`] ASCII-lowercases [`KeyCode::Char`], because a host
/// reports `Ctrl+Shift+K` as `Char('K')` on one layout and `Char('k')` on
/// another while the same binding must match both. Matching normalizes both
/// sides ([`StrokePattern::matches`]), and bindings are normalized when they are
/// added to a [`Keymap`](super::Keymap), so the two sides always agree.
///
/// Normalization is only ever applied to the *comparison*. The keypresses the
/// resolver buffers stay exactly as the host reported them, because two things
/// need the original character: text insertion on fall-through, and the character
/// captured by a [`StrokeCapture::AnyChar`] stroke — lowercasing either would
/// turn `A` into `a`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyPress {
    /// The key that was pressed.
    pub key: KeyCode,
    /// The modifiers that were held.
    pub modifiers: Modifiers,
}

impl KeyPress {
    /// Creates a keypress. Not normalized; see [`Self::normalized`].
    #[must_use]
    pub const fn new(key: KeyCode, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }

    /// Creates a keypress with no modifiers held.
    #[must_use]
    pub const fn plain(key: KeyCode) -> Self {
        Self::new(key, Modifiers::none())
    }

    /// Extracts the keypress from a keyboard event, dropping the repeat flag.
    ///
    /// The repeat flag is deliberately not part of the identity of a keypress: it
    /// is a property of the *event*, not of the chord. The resolver takes it as a
    /// separate argument — see
    /// [`KeymapResolver::resolve_repeat`](super::KeymapResolver::resolve_repeat).
    #[must_use]
    pub const fn from_event(event: &KeyEvent) -> Self {
        Self::new(event.key, event.modifiers)
    }

    /// Returns the keypress with [`KeyCode::Char`] ASCII-lowercased.
    #[must_use]
    pub const fn normalized(self) -> Self {
        Self {
            key: normalize_key(self.key),
            modifiers: self.modifiers,
        }
    }

    /// The character this keypress carries, if any.
    #[must_use]
    pub const fn character(self) -> Option<char> {
        match self.key {
            KeyCode::Char(c) => Some(c),
            _ => None,
        }
    }
}

/// ASCII-lowercases a character key code, leaving every other code alone.
const fn normalize_key(key: KeyCode) -> KeyCode {
    match key {
        KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
        other => other,
    }
}

/// How widely one stroke of a binding matches its key.
///
/// The default, [`Self::None`], matches exactly one [`KeyCode`] — every binding
/// in the default keymap. [`Self::AnyChar`] is what makes an operator grammar
/// expressible: `f{char}`, `r{char}`, `m{mark}`, `"{register}` and `di{delim}`
/// are one binding each, and the character the user actually typed reaches the
/// command through [`CommandArgs::captures`](super::CommandArgs::captures)
/// instead of needing one binding and one command id per character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StrokeCapture {
    /// Match [`StrokePattern::key`] exactly, capturing nothing.
    #[default]
    None,
    /// Match any [`KeyCode::Char`], capturing the character.
    ///
    /// [`StrokePattern::key`] is ignored for matching. A wildcard binding is
    /// scanned separately from the exact-key index, so it costs a short extra
    /// scan rather than widening every lookup.
    AnyChar,
}

impl StrokeCapture {
    /// Returns `true` for [`Self::None`].
    ///
    /// Used to omit the field when serializing a stroke, so the configuration
    /// shape of an ordinary binding is unchanged.
    #[must_use]
    pub const fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

/// One element of a binding's key sequence: a key plus modifier requirements.
///
/// The key is matched exactly (after the [`KeyPress`] normalization described
/// there) unless [`Self::capture`] widens it; the modifiers are matched through a
/// [`ModifierPattern`], so one pattern can express "`Ctrl` held, `Alt` absent,
/// `Shift` irrelevant".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StrokePattern {
    /// The key that must be pressed; ignored when [`Self::capture`] is
    /// [`StrokeCapture::AnyChar`].
    pub key: KeyCode,
    /// The modifier requirements.
    #[serde(default)]
    pub modifiers: ModifierPattern,
    /// How widely the key is matched, and whether it is captured.
    #[serde(default, skip_serializing_if = "StrokeCapture::is_none")]
    pub capture: StrokeCapture,
}

impl StrokePattern {
    /// Builds a stroke pattern that matches `key` exactly.
    #[must_use]
    pub const fn new(key: KeyCode, modifiers: ModifierPattern) -> Self {
        Self {
            key,
            modifiers,
            capture: StrokeCapture::None,
        }
    }

    /// Builds a pattern for `key` with no modifier held.
    #[must_use]
    pub const fn plain(key: KeyCode) -> Self {
        Self::new(key, ModifierPattern::NONE)
    }

    /// Builds a wildcard pattern matching any character key under `modifiers`.
    ///
    /// The matched character is captured and reaches the command through
    /// [`CommandArgs`](super::CommandArgs).
    #[must_use]
    pub const fn any_char(modifiers: ModifierPattern) -> Self {
        Self {
            // Never consulted for matching; a placeholder that keeps the struct
            // total and gives the wildcard a stable serialized shape.
            key: KeyCode::Char('\0'),
            modifiers,
            capture: StrokeCapture::AnyChar,
        }
    }

    /// Returns the pattern with [`KeyCode::Char`] ASCII-lowercased.
    #[must_use]
    pub const fn normalized(self) -> Self {
        Self {
            key: normalize_key(self.key),
            modifiers: self.modifiers,
            capture: self.capture,
        }
    }

    /// Returns `true` when this stroke captures the character it matched.
    #[must_use]
    pub const fn captures(&self) -> bool {
        matches!(self.capture, StrokeCapture::AnyChar)
    }

    /// Returns `true` when `press` satisfies this pattern.
    ///
    /// Both sides are compared in normalized form.
    #[must_use]
    pub fn matches(&self, press: KeyPress) -> bool {
        self.key_matches(press.key) && self.modifiers.matches(press.modifiers)
    }

    /// Returns the character `press` contributes to the command arguments, if
    /// this stroke captures one.
    ///
    /// The character is taken **un-normalized**, so `f` followed by `Shift+A`
    /// captures `'A'`.
    #[must_use]
    pub const fn capture_of(&self, press: KeyPress) -> Option<char> {
        match (self.capture, press.key) {
            (StrokeCapture::AnyChar, KeyCode::Char(c)) => Some(c),
            _ => None,
        }
    }

    /// Returns `true` when this pattern's key requirement admits `key`.
    fn key_matches(&self, key: KeyCode) -> bool {
        match self.capture {
            StrokeCapture::AnyChar => matches!(key, KeyCode::Char(_)),
            StrokeCapture::None => normalize_key(self.key) == normalize_key(key),
        }
    }

    /// Returns `true` when some keypress could satisfy both patterns.
    ///
    /// Used by keymap validation to decide whether a shorter binding can actually
    /// shadow a longer one, rather than assuming any two bindings on the same key
    /// collide.
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        let keys_overlap = match (self.capture, other.capture) {
            (StrokeCapture::AnyChar, StrokeCapture::AnyChar) => true,
            (StrokeCapture::AnyChar, StrokeCapture::None) => {
                matches!(other.key, KeyCode::Char(_))
            },
            (StrokeCapture::None, StrokeCapture::AnyChar) => {
                matches!(self.key, KeyCode::Char(_))
            },
            (StrokeCapture::None, StrokeCapture::None) => {
                normalize_key(self.key) == normalize_key(other.key)
            },
        };
        keys_overlap && self.modifiers.overlaps(&other.modifiers)
    }
}
