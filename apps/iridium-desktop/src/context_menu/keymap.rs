//! The context menu's default keymap — the same chords, now as data.
//!
//! Every key the menu answers is a [`KeyBinding`] in this layer, scoped to
//! [`CONTEXT_MENU_MODE`]. What used to be a `match` on `event.key` with two
//! boolean guards is now a table a user's `[keys]` layer can be pushed on top
//! of.
//!
//! # ⛔ Shift is `Any` on every binding, and that is a faithfulness requirement
//!
//! The guards these bindings replaced were computed from `ctrl`, `alt` and
//! `meta` and **never read shift** — the code said so, and for a good reason:
//! shift decides which character a printable key produced, and none of the
//! menu's keys are printable. A pattern that does not spell shift [`Any`]
//! *requires it absent*, so a binding that forgot would silently drop chords
//! that work today.
//!
//! #91 measured that nothing else catches this. This panel therefore carries its
//! own ratchets, over its own table — see [`super::keymap_tests`].
//!
//! `AltGraph` is `Any` for the same reason and with the same evidence: the old
//! guards did not read it either.
//!
//! # There is no fall-through
//!
//! The menu has no field: it is a list and nothing else, so an unclaimed key is
//! swallowed and that is the whole of it. A user who binds a bare letter here
//! takes nothing away, because nothing else wanted it.
//!
//! [`Any`]: ModifierState::Any

use iridium_editor::commands::builtin::{
    CONTEXT_MENU_ACCEPT, CONTEXT_MENU_DISMISS, CONTEXT_MENU_MODE, CONTEXT_MENU_SELECT_FIRST,
    CONTEXT_MENU_SELECT_LAST, CONTEXT_MENU_SELECT_NEXT, CONTEXT_MENU_SELECT_PREVIOUS,
};
use iridium_editor::{
    CommandId, KeyBinding, KeyCode, Keymap, ModifierPattern, ModifierState, StrokePattern,
};

use ModifierState::{Any, Forbidden, Required};

/// The name this layer is pushed under, quoted in every keymap diagnostic.
pub const KEYMAP_NAME: &str = "context-menu";

/// The number of bindings in the menu's default keymap.
///
/// Eight bindings for six verbs: the two readline hands duplicate the selection
/// verbs. Asserted in the module tests so the documented count cannot drift.
///
/// `#[cfg(test)]` because the ratchet is its only reader: nothing at runtime
/// sizes anything by it, and a `pub` constant no build outside the test harness
/// can reach is dead weight the compiler is right to name.
#[cfg(test)]
pub(super) const DEFAULT_BINDING_COUNT: usize = 8;

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

/// No modifier that changes the meaning of the key — the old `plain` guard.
const PLAIN: ModifierPattern = pattern(Any, Forbidden, Forbidden, Forbidden, Any);
/// Control alone — the old `ctrl_only` guard.
const CTRL: ModifierPattern = pattern(Any, Required, Forbidden, Forbidden, Any);

/// Every modifier pattern this table uses, for the shift ratchet to walk.
///
/// A `const` list rather than one written out in the test, so that a pattern
/// added here without `Any` shift is caught by the ratchet rather than by
/// somebody noticing.
#[cfg(test)]
pub(super) const EVERY_PATTERN: &[ModifierPattern] = &[PLAIN, CTRL];

/// One binding: a chord, in the menu's mode, running a command.
fn bind(modifiers: ModifierPattern, key: KeyCode, command: CommandId) -> KeyBinding {
    KeyBinding::new(StrokePattern::new(key, modifiers), &[], command).in_mode(CONTEXT_MENU_MODE)
}

/// The menu's default bindings, as a layer.
///
/// Built rather than `static` because [`KeyBinding`] owns its stroke list, which
/// has drop glue; the cost is one allocation per menu, once, at construction —
/// and a context menu is constructed on a right-click, not per frame.
#[must_use]
pub fn default_keymap() -> Keymap {
    let mut keymap = Keymap::new(KEYMAP_NAME);
    keymap.extend(vec![
        // The two ways out.
        bind(PLAIN, KeyCode::Escape, CONTEXT_MENU_DISMISS),
        bind(PLAIN, KeyCode::Enter, CONTEXT_MENU_ACCEPT),
        // The selection. The `Ctrl` spellings are the readline hands.
        //
        // ⚠️ One binding covers `p` and `P` both: `StrokePattern::matches`
        // ASCII-lowercases a character keycode before comparing, so the
        // `Char('p' | 'P')` arms these replaced only looked like they were
        // giving extra coverage.
        bind(PLAIN, KeyCode::Up, CONTEXT_MENU_SELECT_PREVIOUS),
        bind(CTRL, KeyCode::Char('p'), CONTEXT_MENU_SELECT_PREVIOUS),
        bind(PLAIN, KeyCode::Down, CONTEXT_MENU_SELECT_NEXT),
        bind(CTRL, KeyCode::Char('n'), CONTEXT_MENU_SELECT_NEXT),
        bind(PLAIN, KeyCode::Home, CONTEXT_MENU_SELECT_FIRST),
        bind(PLAIN, KeyCode::End, CONTEXT_MENU_SELECT_LAST),
    ]);
    keymap
}
