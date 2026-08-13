//! The search overlay's default keymap — the same chords, now as data.
//!
//! Every key the overlay answers is a [`KeyBinding`] in this layer, scoped to
//! [`SEARCH_MODE`]. What used to be a `match` on `(Chord, KeyCode)` is now a
//! table a user's `[keys]` layer can be pushed on top of.
//!
//! # ⛔ Shift is `Any` on every binding **except the two `Enter` spellings**
//!
//! This is the one panel of the seven where shift carries meaning. The `chord`
//! function these bindings replaced classified the modifiers *without* looking
//! at shift, and then the `Enter` arm **re-read `event.modifiers.shift`** to
//! decide between next match and previous match. Reproducing that faithfully
//! means two bindings on one key:
//!
//! | binding | shift |
//! |---|---|
//! | `enter` → next match | [`Forbidden`] |
//! | `shift+enter` → previous match | [`Required`] |
//! | everything else | [`Any`] |
//!
//! Everywhere else `Any` is a faithfulness requirement in the usual way: a
//! pattern that does not spell shift `Any` *requires it absent*, so a binding
//! that forgot would silently drop chords that work today. #91 measured that
//! nothing else catches this, so this panel carries its own ratchets over its
//! own table — see [`super::keymap_tests`].
//!
//! `AltGraph` is `Any` on **every** binding including the two `Enter` ones: the
//! old function did not read it either, and on a great many layouts it is how a
//! character is typed at all.
//!
//! # ⚠️ `Up` and `Down` are bound to the same two verbs, and must stay that way
//!
//! The legacy xterm encoding **cannot express `Shift+Enter` at all** (see
//! [`crate::input`]). `Up` and `Down` exist so that nothing is unreachable on a
//! terminal without the kitty protocol. Four bindings, two verbs, on purpose.
//!
//! # There is a fall-through, and it is below this table
//!
//! A printable key nothing here claims becomes text in the focused field, which
//! is what makes the overlay typable. A user who binds a bare letter to a verb
//! gets the verb, and every other letter still types. Anything else unclaimed is
//! [`SearchOutcome::Ignored`](super::SearchOutcome) and stays the host's, so a
//! binding such as save is not dead while the overlay is open.
//!
//! [`Any`]: ModifierState::Any
//! [`Forbidden`]: ModifierState::Forbidden
//! [`Required`]: ModifierState::Required

use iridium_editor::commands::builtin::{
    SEARCH_CARET_END, SEARCH_CARET_HOME, SEARCH_CARET_LEFT, SEARCH_CARET_RIGHT, SEARCH_DISMISS,
    SEARCH_FIELD_BACKSPACE, SEARCH_FIELD_DELETE, SEARCH_MODE, SEARCH_NEXT_MATCH,
    SEARCH_PREVIOUS_MATCH, SEARCH_REPLACE_ALL, SEARCH_REPLACE_CURRENT,
    SEARCH_TOGGLE_CASE_SENSITIVE, SEARCH_TOGGLE_FIELD, SEARCH_TOGGLE_REGEX,
    SEARCH_TOGGLE_WHOLE_WORD,
};
use iridium_editor::{
    CommandId, KeyBinding, KeyCode, Keymap, ModifierPattern, ModifierState, StrokePattern,
};

use ModifierState::{Any, Forbidden, Required};

/// The name this layer is pushed under, quoted in every keymap diagnostic.
pub const KEYMAP_NAME: &str = "terminal-search";

/// The number of bindings in the overlay's default keymap.
///
/// Seventeen bindings for fifteen verbs: `Up`/`Down` duplicate the two match
/// verbs so the legacy encoding can reach them. Asserted in the module tests so
/// the documented count cannot drift, and `#[cfg(test)]` because that is the
/// only thing it is for: this face hands no `FaceKeys` to `iridium_config` yet,
/// so nothing outside the tests has asked what the count is.
#[cfg(test)]
pub(super) const DEFAULT_BINDING_COUNT: usize = 17;

/// Shorthand for a modifier pattern, in the field order of [`ModifierPattern`].
const fn pattern(
    shift: ModifierState,
    ctrl: ModifierState,
    alt: ModifierState,
    meta: ModifierState,
    alt_graph: ModifierState,
) -> ModifierPattern {
    ModifierPattern::new(shift, ctrl, alt, meta, alt_graph)
}

/// No modifier that changes the meaning of the key — the old `Chord::Plain`.
const PLAIN: ModifierPattern = pattern(Any, Forbidden, Forbidden, Forbidden, Any);
/// Alt alone — the old `Chord::Alt`, which carries the three option toggles.
const ALT: ModifierPattern = pattern(Any, Forbidden, Required, Forbidden, Any);
/// Control alone — the old `Chord::Ctrl`.
const CTRL: ModifierPattern = pattern(Any, Required, Forbidden, Forbidden, Any);
/// Control and Alt together — the old `Chord::CtrlAlt`.
const CTRL_ALT: ModifierPattern = pattern(Any, Required, Required, Forbidden, Any);

/// `Enter` with shift **released** — next match.
///
/// ⚠️ The only two patterns in this table that constrain shift are this and
/// [`ENTER_SHIFT`], and they do so because the code they replaced read shift on
/// exactly this key. See the module documentation.
const ENTER_PLAIN: ModifierPattern = pattern(Forbidden, Forbidden, Forbidden, Forbidden, Any);
/// `Enter` with shift **held** — previous match.
const ENTER_SHIFT: ModifierPattern = pattern(Required, Forbidden, Forbidden, Forbidden, Any);

/// Every modifier pattern that ignores shift, for the shift ratchet to walk.
///
/// A `const` list rather than one written out in the test, so that a pattern
/// added here without `Any` shift is caught by the ratchet rather than by
/// somebody noticing.
#[cfg(test)]
pub(super) const SHIFT_BLIND_PATTERNS: &[ModifierPattern] = &[PLAIN, ALT, CTRL, CTRL_ALT];

/// The two patterns that deliberately constrain shift.
///
/// Listed so the ratchet can assert they are the **only** exceptions, and that
/// between them they partition shift rather than leaving a state unreachable.
#[cfg(test)]
pub(super) const ENTER_PATTERNS: &[ModifierPattern] = &[ENTER_PLAIN, ENTER_SHIFT];

/// One binding: a chord, in the overlay's mode, running a command.
fn bind(modifiers: ModifierPattern, key: KeyCode, command: CommandId) -> KeyBinding {
    KeyBinding::new(StrokePattern::new(key, modifiers), &[], command).in_mode(SEARCH_MODE)
}

/// The overlay's default bindings, as a layer.
///
/// Built rather than `static` because [`KeyBinding`] owns its stroke list, which
/// has drop glue; the cost is one allocation per overlay, once, at construction.
#[must_use]
pub fn default_keymap() -> Keymap {
    let mut keymap = Keymap::new(KEYMAP_NAME);
    keymap.extend(vec![
        // The way out.
        bind(PLAIN, KeyCode::Escape, SEARCH_DISMISS),
        // Walking the matches. Two spellings each, because the legacy xterm
        // encoding cannot express `Shift+Enter`.
        bind(ENTER_PLAIN, KeyCode::Enter, SEARCH_NEXT_MATCH),
        bind(ENTER_SHIFT, KeyCode::Enter, SEARCH_PREVIOUS_MATCH),
        bind(PLAIN, KeyCode::Down, SEARCH_NEXT_MATCH),
        bind(PLAIN, KeyCode::Up, SEARCH_PREVIOUS_MATCH),
        // The two fields.
        bind(PLAIN, KeyCode::Tab, SEARCH_TOGGLE_FIELD),
        bind(PLAIN, KeyCode::Left, SEARCH_CARET_LEFT),
        bind(PLAIN, KeyCode::Right, SEARCH_CARET_RIGHT),
        bind(PLAIN, KeyCode::Home, SEARCH_CARET_HOME),
        bind(PLAIN, KeyCode::End, SEARCH_CARET_END),
        bind(PLAIN, KeyCode::Backspace, SEARCH_FIELD_BACKSPACE),
        bind(PLAIN, KeyCode::Delete, SEARCH_FIELD_DELETE),
        // Replacing. `CTRL` forbids alt, which is what keeps these two apart.
        //
        // ⚠️ One binding covers `r` and `R` both: `StrokePattern::matches`
        // ASCII-lowercases a character keycode before comparing, so the
        // `Char('r' | 'R')` arms these replaced only looked like they were
        // giving extra coverage.
        bind(CTRL, KeyCode::Char('r'), SEARCH_REPLACE_CURRENT),
        bind(CTRL_ALT, KeyCode::Char('r'), SEARCH_REPLACE_ALL),
        // The options.
        bind(ALT, KeyCode::Char('c'), SEARCH_TOGGLE_CASE_SENSITIVE),
        bind(ALT, KeyCode::Char('w'), SEARCH_TOGGLE_WHOLE_WORD),
        bind(ALT, KeyCode::Char('r'), SEARCH_TOGGLE_REGEX),
    ]);
    keymap
}
