//! The command palette's default keymap — the same chords, now as data.
//!
//! Every key the palette answers is a [`KeyBinding`] in this layer, scoped to
//! [`PALETTE_MODE`]. What used to be a `match` on `(Chord, KeyCode)` is now a
//! table a user's `[keys]` layer can be pushed on top of.
//!
//! # One mode, because there is one screen
//!
//! Unlike the explorer's four, the palette has a single screen: a query field
//! over a ranked list. Nothing it shows disagrees about what a printable
//! character means, so there is nothing for a second mode to separate.
//!
//! # ⛔ Shift is `Any` on every binding, and that is a faithfulness requirement
//!
//! The `chord` function these bindings replaced read `ctrl`, `alt` and `meta`
//! and **never looked at shift**. So `Shift+Down` moved the selection and
//! `Ctrl+Shift+K` closed the panel — not by design, but because nothing asked.
//! A [`KeyPress`](iridium_editor::KeyPress) carries shift, and a pattern that
//! does not spell it [`Any`] *requires it absent*.
//!
//! Spelling shift `Any` throughout is therefore what makes this conversion
//! change nothing. #91 measured that **nothing else catches a binding that
//! forgot**: with the shift state tightened, its whole workspace suite went on
//! passing. So this table carries its own ratchet —
//! `every_default_binding_ignores_shift_exactly_as_the_chord_table_did` presses
//! all of them a second time with shift held.
//!
//! `AltGraph` is `Any` for the same reason and with the same evidence: the old
//! function did not read it either.
//!
//! # What is deliberately not in this table
//!
//! The printable-character fall-through. `(Plain, Char(_))` was the last arm of
//! the old table and it is not a binding — it is "every character nothing
//! claimed goes into the query". Turning it into twenty-six bindings would make
//! a keymap that could not express a *field*. It stays below resolution, in
//! [`super::keys`], and a user who binds a bare letter to a command takes that
//! letter out of the query, which is the honest consequence of what they asked
//! for.
//!
//! [`Any`]: ModifierState::Any

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
pub const KEYMAP_NAME: &str = "palette";

/// The number of bindings in the palette's default keymap.
///
/// Asserted in the module tests so the documented count cannot drift.
pub const DEFAULT_BINDING_COUNT: usize = 16;

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
/// Meta alone — the old `Chord::Meta`, the ⌘ spelling of the toggle.
const META: ModifierPattern = pattern(Any, Forbidden, Forbidden, Required, Any);

/// Every modifier pattern this table uses, for the shift ratchet to walk.
///
/// A `const` list rather than one written out in the test, so that a pattern
/// added here without `Any` shift is caught by the ratchet rather than by
/// somebody noticing.
#[cfg(test)]
pub(super) const EVERY_PATTERN: &[ModifierPattern] = &[PLAIN, CTRL, META];

/// One binding: a chord, in the palette's mode, running a command.
fn bind(modifiers: ModifierPattern, key: KeyCode, command: CommandId) -> KeyBinding {
    KeyBinding::new(StrokePattern::new(key, modifiers), &[], command).in_mode(PALETTE_MODE)
}

/// The palette's default bindings, as a layer.
///
/// Built rather than `static` because [`KeyBinding`] owns its stroke list, which
/// has drop glue; the cost is one allocation per panel, once, at construction.
#[must_use]
pub fn default_keymap() -> Keymap {
    let mut keymap = Keymap::new(KEYMAP_NAME);
    keymap.extend(vec![
        // The three ways out. `Ctrl+K` and `⌘K` close because they open —
        // a toggle is what the finger expects, and the palette owns both
        // halves: `palette.open` is the editor's, this is the panel's.
        bind(PLAIN, KeyCode::Escape, PALETTE_DISMISS),
        bind(CTRL, KeyCode::Char('k'), PALETTE_DISMISS),
        bind(META, KeyCode::Char('k'), PALETTE_DISMISS),
        bind(PLAIN, KeyCode::Enter, PALETTE_ACCEPT),
        // The selection. `Ctrl+P` / `Ctrl+N` beside the arrows, as everywhere
        // else in this editor that has a list.
        bind(PLAIN, KeyCode::Up, PALETTE_SELECT_PREVIOUS),
        bind(CTRL, KeyCode::Char('p'), PALETTE_SELECT_PREVIOUS),
        bind(PLAIN, KeyCode::Down, PALETTE_SELECT_NEXT),
        bind(CTRL, KeyCode::Char('n'), PALETTE_SELECT_NEXT),
        bind(PLAIN, KeyCode::PageUp, PALETTE_SELECT_PAGE_UP),
        bind(PLAIN, KeyCode::PageDown, PALETTE_SELECT_PAGE_DOWN),
        // The query field. The four motions go to the *field*, not the list:
        // a field that took two of them and gave the others to the list would
        // be a field with no rule.
        bind(PLAIN, KeyCode::Left, PALETTE_CARET_LEFT),
        bind(PLAIN, KeyCode::Right, PALETTE_CARET_RIGHT),
        bind(PLAIN, KeyCode::Home, PALETTE_CARET_HOME),
        bind(PLAIN, KeyCode::End, PALETTE_CARET_END),
        bind(PLAIN, KeyCode::Backspace, PALETTE_QUERY_BACKSPACE),
        bind(PLAIN, KeyCode::Delete, PALETTE_QUERY_DELETE),
    ]);
    keymap
}
