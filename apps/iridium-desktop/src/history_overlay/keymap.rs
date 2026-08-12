//! The undo-tree panel's default keymap — the same chords, now as data.
//!
//! Every key the panel answers is a [`KeyBinding`] in this layer, scoped to
//! [`HISTORY_MODE`]. What used to be a `match` on `(Chord, KeyCode)` is now a
//! table a user's `[keys]` layer can be pushed on top of.
//!
//! # ⛔ Shift is `Any` on every binding, and that is a faithfulness requirement
//!
//! The `chord` function these bindings replaced read `ctrl`, `alt` and `meta`
//! and **never looked at shift**, so `Shift+Down` moved the selection. A pattern
//! that does not spell shift [`Any`] *requires it absent*, so a binding that
//! forgot would silently drop chords that work today.
//!
//! #91 measured that nothing else catches this. This panel therefore carries its
//! own ratchets, over its own table — see [`super::keymap_tests`].
//!
//! `AltGraph` is `Any` for the same reason and with the same evidence: the old
//! function did not read it either.
//!
//! # There is no fall-through, and that is the difference from the palette
//!
//! The palette and the file explorer both have a *field*, so a printable
//! character nothing claimed is text. This panel has no field: it is a list and
//! nothing else, so an unclaimed key is swallowed and that is the whole of it.
//! A user who binds a bare letter here takes nothing away, because nothing else
//! wanted it.
//!
//! [`Any`]: ModifierState::Any

use iridium_editor::commands::builtin::{
    HISTORY_DISMISS, HISTORY_JUMP, HISTORY_MODE, HISTORY_SELECT_FIRST, HISTORY_SELECT_LAST,
    HISTORY_SELECT_NEXT, HISTORY_SELECT_PAGE_DOWN, HISTORY_SELECT_PAGE_UP, HISTORY_SELECT_PREVIOUS,
};
use iridium_editor::{
    CommandId, KeyBinding, KeyCode, Keymap, ModifierPattern, ModifierState, StrokePattern,
};

use ModifierState::{Any, Forbidden, Required};

/// The name this layer is pushed under, quoted in every keymap diagnostic.
pub const KEYMAP_NAME: &str = "history";

/// The number of bindings in the panel's default keymap.
///
/// Asserted in the module tests so the documented count cannot drift.
pub const DEFAULT_BINDING_COUNT: usize = 10;

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
/// Control and Alt together — the old `Chord::CtrlAlt`.
const CTRL_ALT: ModifierPattern = pattern(Any, Required, Required, Forbidden, Any);
/// Meta and Alt together — the old `Chord::MetaAlt`, the ⌘⌥ spelling.
const META_ALT: ModifierPattern = pattern(Any, Forbidden, Required, Required, Any);

/// Every modifier pattern this table uses, for the shift ratchet to walk.
///
/// A `const` list rather than one written out in the test, so that a pattern
/// added here without `Any` shift is caught by the ratchet rather than by
/// somebody noticing.
#[cfg(test)]
pub(super) const EVERY_PATTERN: &[ModifierPattern] = &[PLAIN, CTRL_ALT, META_ALT];

/// One binding: a chord, in the panel's mode, running a command.
fn bind(modifiers: ModifierPattern, key: KeyCode, command: CommandId) -> KeyBinding {
    KeyBinding::new(StrokePattern::new(key, modifiers), &[], command).in_mode(HISTORY_MODE)
}

/// The panel's default bindings, as a layer.
///
/// Built rather than `static` because [`KeyBinding`] owns its stroke list, which
/// has drop glue; the cost is one allocation per panel, once, at construction.
#[must_use]
pub fn default_keymap() -> Keymap {
    let mut keymap = Keymap::new(KEYMAP_NAME);
    keymap.extend(vec![
        // The ways out. The two toggle spellings close because they open —
        // and they are named *here*, not left to the host, because the panel
        // is modal: a chord it does not name is swallowed and never reaches
        // the command it belongs to.
        bind(PLAIN, KeyCode::Escape, HISTORY_DISMISS),
        bind(CTRL_ALT, KeyCode::Char('h'), HISTORY_DISMISS),
        bind(META_ALT, KeyCode::Char('h'), HISTORY_DISMISS),
        // ⚠️ `Enter` leaves the panel open on purpose. See the kernel's
        // `HISTORY_JUMP` for why that is one verb rather than two.
        bind(PLAIN, KeyCode::Enter, HISTORY_JUMP),
        // The selection.
        bind(PLAIN, KeyCode::Up, HISTORY_SELECT_PREVIOUS),
        bind(PLAIN, KeyCode::Down, HISTORY_SELECT_NEXT),
        bind(PLAIN, KeyCode::PageUp, HISTORY_SELECT_PAGE_UP),
        bind(PLAIN, KeyCode::PageDown, HISTORY_SELECT_PAGE_DOWN),
        bind(PLAIN, KeyCode::Home, HISTORY_SELECT_FIRST),
        bind(PLAIN, KeyCode::End, HISTORY_SELECT_LAST),
    ]);
    keymap
}
