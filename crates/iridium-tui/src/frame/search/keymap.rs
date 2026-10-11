//! Rebindable search-field and match commands, scoped to `SEARCH_MODE`.
//! Shift extends field selections on arrows/Home/End and reverses Enter.
//! Other legacy bindings retain their shift-blind spelling. Up/Down provide
//! previous/next match on terminals unable to encode Shift+Enter.
//! Unclaimed host commands, including save and undo, remain reachable.

use iridium_editor::commands::builtin::{
    CLIPBOARD_COPY, CLIPBOARD_CUT, CLIPBOARD_PASTE, CURSOR_CHAR_LEFT_SELECT,
    CURSOR_CHAR_RIGHT_SELECT, CURSOR_LINE_END_SELECT, CURSOR_LINE_START_SELECT, CURSOR_WORD_LEFT,
    CURSOR_WORD_LEFT_SELECT, CURSOR_WORD_RIGHT, CURSOR_WORD_RIGHT_SELECT,
    EDIT_DELETE_WORD_BACKWARD, EDIT_DELETE_WORD_FORWARD, SEARCH_CARET_END, SEARCH_CARET_HOME,
    SEARCH_CARET_LEFT, SEARCH_CARET_RIGHT, SEARCH_CLEAR_SCOPE, SEARCH_DISMISS,
    SEARCH_FIELD_BACKSPACE, SEARCH_FIELD_DELETE, SEARCH_MODE, SEARCH_NEXT_MATCH,
    SEARCH_PREVIOUS_MATCH, SEARCH_REPLACE_ALL, SEARCH_REPLACE_CURRENT, SEARCH_SELECT_ALL_MATCHES,
    SEARCH_TOGGLE_CASE_SENSITIVE, SEARCH_TOGGLE_FIELD, SEARCH_TOGGLE_REGEX,
    SEARCH_TOGGLE_WHOLE_WORD, SELECTION_SELECT_ALL,
};
use iridium_editor::{
    CommandId, KeyBinding, KeyCode, Keymap, ModifierPattern, ModifierState, StrokePattern,
};

use ModifierState::{Any, Forbidden, Required};

/// The name this layer is pushed under, quoted in every keymap diagnostic.
pub const KEYMAP_NAME: &str = "terminal-search";

/// The number of bindings in the overlay's default keymap.
///
/// Thirty-three bindings for thirty-one verbs: `Up`/`Down` duplicate the two match
/// verbs so the legacy encoding can reach them. Asserted in the module tests so
/// the documented count cannot drift, and `#[cfg(test)]` because that is the
/// only thing it is for: this face hands no `FaceKeys` to `iridium_config` yet,
/// so nothing outside the tests has asked what the count is.
#[cfg(test)]
pub(super) const DEFAULT_BINDING_COUNT: usize = 33;

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

/// Plain field movement and next match.
const ENTER_PLAIN: ModifierPattern = pattern(Forbidden, Forbidden, Forbidden, Forbidden, Any);
/// Selection extension and previous match.
const ENTER_SHIFT: ModifierPattern = pattern(Required, Forbidden, Forbidden, Forbidden, Any);
/// Word movement.
const WORD_PLAIN: ModifierPattern = pattern(Forbidden, Required, Forbidden, Forbidden, Any);
/// Word selection extension.
const WORD_SHIFT: ModifierPattern = pattern(Required, Required, Forbidden, Forbidden, Any);

/// Every modifier pattern that ignores shift, for the shift ratchet to walk.
///
/// A `const` list rather than one written out in the test, so that a pattern
/// added here without `Any` shift is caught by the ratchet rather than by
/// somebody noticing.
#[cfg(test)]
pub(super) const SHIFT_BLIND_PATTERNS: &[ModifierPattern] = &[PLAIN, ALT, CTRL, CTRL_ALT];

/// Plain and shifted navigation patterns.
///
/// Listed so the ratchet verifies that they partition shift. Word movement
/// uses the same shift distinction with Control required.
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
        bind(CTRL, KeyCode::Char('a'), SELECTION_SELECT_ALL),
        bind(CTRL, KeyCode::Char('c'), CLIPBOARD_COPY),
        bind(CTRL, KeyCode::Char('x'), CLIPBOARD_CUT),
        bind(ENTER_SHIFT, KeyCode::Left, CURSOR_CHAR_LEFT_SELECT),
        bind(ENTER_SHIFT, KeyCode::Right, CURSOR_CHAR_RIGHT_SELECT),
        bind(ENTER_SHIFT, KeyCode::Home, CURSOR_LINE_START_SELECT),
        bind(ENTER_SHIFT, KeyCode::End, CURSOR_LINE_END_SELECT),
        bind(WORD_PLAIN, KeyCode::Left, CURSOR_WORD_LEFT),
        bind(WORD_PLAIN, KeyCode::Right, CURSOR_WORD_RIGHT),
        bind(WORD_SHIFT, KeyCode::Left, CURSOR_WORD_LEFT_SELECT),
        bind(WORD_SHIFT, KeyCode::Right, CURSOR_WORD_RIGHT_SELECT),
        bind(CTRL, KeyCode::Backspace, EDIT_DELETE_WORD_BACKWARD),
        bind(CTRL, KeyCode::Delete, EDIT_DELETE_WORD_FORWARD),
        // The way out.
        bind(PLAIN, KeyCode::Escape, SEARCH_DISMISS),
        // Walking the matches. Two spellings each, because the legacy xterm
        // encoding cannot express `Shift+Enter`.
        bind(ENTER_PLAIN, KeyCode::Enter, SEARCH_NEXT_MATCH),
        bind(ENTER_SHIFT, KeyCode::Enter, SEARCH_PREVIOUS_MATCH),
        bind(PLAIN, KeyCode::Down, SEARCH_NEXT_MATCH),
        bind(PLAIN, KeyCode::Up, SEARCH_PREVIOUS_MATCH),
        // Alt+Enter is representable as an escape prefix in legacy terminals.
        bind(ALT, KeyCode::Enter, SEARCH_SELECT_ALL_MATCHES),
        bind(ALT, KeyCode::Char('s'), SEARCH_CLEAR_SCOPE),
        // Clipboard text is supplied by the host, including in read-only documents.
        bind(CTRL, KeyCode::Char('v'), CLIPBOARD_PASTE),
        // The two fields.
        bind(PLAIN, KeyCode::Tab, SEARCH_TOGGLE_FIELD),
        bind(ENTER_PLAIN, KeyCode::Left, SEARCH_CARET_LEFT),
        bind(ENTER_PLAIN, KeyCode::Right, SEARCH_CARET_RIGHT),
        bind(ENTER_PLAIN, KeyCode::Home, SEARCH_CARET_HOME),
        bind(ENTER_PLAIN, KeyCode::End, SEARCH_CARET_END),
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
