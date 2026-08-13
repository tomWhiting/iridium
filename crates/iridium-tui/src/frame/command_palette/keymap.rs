//! The terminal palette's default keymap — the same chords, now as data.
//!
//! Every key the panel answers is a [`KeyBinding`] in this layer, scoped to
//! [`PALETTE_MODE`]. What used to be a `match` on `(Chord, KeyCode)` is now a
//! table a user's `[keys]` layer can be pushed on top of.
//!
//! # ⛔ Shift is `Any` on every binding, and that is a faithfulness requirement
//!
//! The `chord` function these bindings replaced read `ctrl`, `alt` and `meta`
//! and **never looked at shift** — it said so, and for a good reason: shift
//! decides which character a printable key produced, and the input adapter has
//! already resolved that. A pattern that does not spell shift [`Any`]
//! *requires it absent*, so a binding that forgot would silently drop chords
//! that work today.
//!
//! #91 measured that nothing else catches this. This panel therefore carries its
//! own ratchets, over its own table — see [`super::keymap_tests`].
//!
//! `AltGraph` is `Any` for the same reason and with the same evidence: the old
//! function did not read it either.
//!
//! # This face's table is not the desktop's, and should not be
//!
//! There is no `⌘` in a terminal, so the `meta` spellings the desktop palette
//! carries have no meaning here and are absent. The two tables answer the same
//! twelve verbs by different chords, which is what a face-owned keymap is for.

use iridium_editor::commands::builtin::{
    PALETTE_ACCEPT, PALETTE_CARET_END, PALETTE_CARET_HOME, PALETTE_CARET_LEFT, PALETTE_CARET_RIGHT,
    PALETTE_DISMISS, PALETTE_MODE, PALETTE_QUERY_BACKSPACE, PALETTE_QUERY_DELETE,
    PALETTE_SELECT_NEXT, PALETTE_SELECT_PAGE_DOWN, PALETTE_SELECT_PAGE_UP, PALETTE_SELECT_PREVIOUS,
};
use iridium_editor::{
    CommandId, KeyBinding, KeyCode, Keymap, ModifierPattern, ModifierState, StrokePattern,
};

use ModifierState::{Any, Forbidden, Required};

/// The name this layer is pushed under, quoted in every keymap diagnostic.
pub const KEYMAP_NAME: &str = "terminal-palette";

/// The number of bindings in the panel's default keymap.
///
/// Asserted in the module tests so the documented count cannot drift, and
/// `#[cfg(test)]` because that is the only thing it is for: unlike the desktop
/// palette's, this face hands no `FaceKeys` to `iridium_config` yet, so nothing
/// outside the tests has asked what the count is.
#[cfg(test)]
pub(super) const DEFAULT_BINDING_COUNT: usize = 15;

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
/// Control alone — the old `Chord::Ctrl`.
const CTRL: ModifierPattern = pattern(Any, Required, Forbidden, Forbidden, Any);

/// Every modifier pattern this table uses, for the shift ratchet to walk.
///
/// A `const` list rather than one written out in the test, so that a pattern
/// added here without `Any` shift is caught by the ratchet rather than by
/// somebody noticing.
#[cfg(test)]
pub(super) const EVERY_PATTERN: &[ModifierPattern] = &[PLAIN, CTRL];

/// One binding: a chord, in the panel's mode, running a command.
fn bind(modifiers: ModifierPattern, key: KeyCode, command: CommandId) -> KeyBinding {
    KeyBinding::new(StrokePattern::new(key, modifiers), &[], command).in_mode(PALETTE_MODE)
}

/// The panel's default bindings, as a layer.
///
/// Built rather than `static` because [`KeyBinding`] owns its stroke list, which
/// has drop glue; the cost is one allocation per panel, once, at construction.
#[must_use]
pub fn default_keymap() -> Keymap {
    let mut keymap = Keymap::new(KEYMAP_NAME);
    keymap.extend(vec![
        // The ways out. `Ctrl+K` closes because it opens — and it is named
        // *here*, not left to the host, because the panel is modal: a chord it
        // does not name is swallowed and never reaches the command it belongs
        // to.
        bind(PLAIN, KeyCode::Escape, PALETTE_DISMISS),
        bind(CTRL, KeyCode::Char('k'), PALETTE_DISMISS),
        bind(PLAIN, KeyCode::Enter, PALETTE_ACCEPT),
        // The selection. The `Ctrl` spellings are the readline hands.
        bind(PLAIN, KeyCode::Up, PALETTE_SELECT_PREVIOUS),
        bind(CTRL, KeyCode::Char('p'), PALETTE_SELECT_PREVIOUS),
        bind(PLAIN, KeyCode::Down, PALETTE_SELECT_NEXT),
        bind(CTRL, KeyCode::Char('n'), PALETTE_SELECT_NEXT),
        bind(PLAIN, KeyCode::PageUp, PALETTE_SELECT_PAGE_UP),
        bind(PLAIN, KeyCode::PageDown, PALETTE_SELECT_PAGE_DOWN),
        // The query field.
        bind(PLAIN, KeyCode::Left, PALETTE_CARET_LEFT),
        bind(PLAIN, KeyCode::Right, PALETTE_CARET_RIGHT),
        bind(PLAIN, KeyCode::Home, PALETTE_CARET_HOME),
        bind(PLAIN, KeyCode::End, PALETTE_CARET_END),
        bind(PLAIN, KeyCode::Backspace, PALETTE_QUERY_BACKSPACE),
        bind(PLAIN, KeyCode::Delete, PALETTE_QUERY_DELETE),
    ]);
    keymap
}
