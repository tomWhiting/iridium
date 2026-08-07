//! The human-readable chord form: `ctrl+shift+k`, `~alt+f4`, `f`, `<any>`.
//!
//! Kept apart from [`StrokePattern`] itself because parsing and rendering a
//! chord is a different job from matching one, and because this is the only
//! part of the type that a user's keymap file touches directly.

use std::fmt;
use std::str::FromStr;

use super::keynames::{ANY_CHAR_NAME, key_from_name, name_of};
use super::{KeymapError, ModifierPattern, ModifierState, StrokePattern};

impl fmt::Display for StrokePattern {
    /// Renders the chord in the form [`FromStr`] parses, e.g. `ctrl+shift+k`.
    ///
    /// Every constrained modifier is shown: a required one as its bare name, one
    /// the pattern *ignores* as `~name`. [`ModifierState::Forbidden`] is omitted
    /// because it is what parsing an unmentioned modifier yields, which is what
    /// makes the form round-trippable rather than merely readable.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        self.modifiers.write_display(&mut out);
        f.write_str(&out)?;
        if self.captures() {
            f.write_str(ANY_CHAR_NAME)
        } else {
            f.write_str(&name_of(self.key))
        }
    }
}

impl FromStr for StrokePattern {
    type Err = KeymapError;

    /// Parses one chord, e.g. `ctrl+k`, `ctrl-shift-f3`, `~shift+up`, `{char}`.
    ///
    /// Modifier names and the key name are separated by `+` or `-`; the last
    /// element is the key. Accepted modifier spellings are `ctrl`/`control`,
    /// `shift`, `alt`/`option`, `meta`/`cmd`/`super`/`win` and
    /// `altgraph`/`altgr`, each optionally prefixed with `~` to mean "ignore this
    /// modifier" ([`ModifierState::Any`]). An unmentioned modifier is
    /// [`ModifierState::Forbidden`], so `ctrl+k` is an exact chord and carries the
    /// `AltGr` guard without the author writing it. The key `{char}` is the
    /// capture wildcard; the literal keys `-` and space are spelled `minus` and
    /// `space`. [`KeyCode::Char`](crate::input::KeyCode::Char) is normalized, so
    /// letter case is irrelevant on
    /// both sides.
    ///
    /// # Errors
    ///
    /// [`KeymapError::UnparsableStroke`] when the text is empty, names an unknown
    /// key, or ends with a separator.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let unparsable = || KeymapError::UnparsableStroke {
            stroke: text.to_owned(),
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(unparsable());
        }
        let mut modifiers = ModifierPattern::NONE;
        let mut rest = trimmed;
        // The last element is the key, so a modifier is only a modifier when a
        // separator follows it and the name is recognized. Anything else ends the
        // modifier prefix and is parsed as the key.
        while let Some(split) = rest.find(['+', '-']) {
            if split == 0 {
                return Err(unparsable());
            }
            let (head, tail) = rest.split_at(split);
            let (state, name) = head
                .strip_prefix('~')
                .map_or((ModifierState::Required, head), |bare| {
                    (ModifierState::Any, bare)
                });
            if !modifiers.apply_named(&name.to_ascii_lowercase(), state) {
                break;
            }
            rest = &tail[1..];
        }
        if rest == ANY_CHAR_NAME {
            return Ok(Self::any_char(modifiers));
        }
        // Normalized on the way in, exactly as a binding's strokes are, so
        // `"CTRL+K"` and `"ctrl+k"` are the same pattern rather than two that
        // happen to match the same keypress.
        key_from_name(rest)
            .map(|key| Self::new(key, modifiers).normalized())
            .ok_or_else(unparsable)
    }
}
