//! The explorer's default keymap — the same chords, now as data.
//!
//! Every key the file explorer answers is a [`KeyBinding`] in this layer,
//! scoped to the mode of the screen it belongs to. What used to be a `match` on
//! `(Chord, KeyCode)` is now a table a user's `[keys]` layer can be pushed on
//! top of, which is the whole of #91.
//!
//! # The four modes are the four screens
//!
//! | mode | what is on screen |
//! | --- | --- |
//! | [`EXPLORER_MODE`] | the query field and the rows |
//! | [`EXPLORER_EDIT_MODE`] | the rows, as editable text |
//! | [`EXPLORER_CONFIRM_MODE`] | the operations a plan will perform |
//! | [`EXPLORER_REFUSED_MODE`] | why a buffer cannot be applied |
//!
//! A mode per screen rather than a flag, because the screens disagree about
//! what a key *is*: `Escape` leaves an editing session, cancels a plan and
//! dismisses a refusal, and those are three commands rather than one command
//! with three meanings.
//!
//! # ⛔ Shift is `Any` on every binding, and that is a faithfulness requirement
//!
//! The `chord` function these bindings replaced read `ctrl`, `alt` and `meta`
//! and **never looked at shift**. So `Shift+Down` moved the selection,
//! `Ctrl+Shift+D` struck a row through and `Ctrl+Shift+N` moved down — not by
//! design, but because nothing asked. A [`KeyPress`](iridium_editor::KeyPress)
//! carries shift, and a pattern that does not spell it [`Any`] *requires it
//! absent*.
//!
//! Spelling shift `Any` throughout is therefore what makes this conversion
//! change nothing. A binding that forgot would compile, would pass every test
//! written against the unshifted spelling, and would silently drop chords that
//! work today — which is why
//! `every_default_binding_ignores_shift_exactly_as_the_chord_table_did` presses
//! all of them a second time with shift held.
//!
//! `AltGraph` is `Any` for the same reason and with the same evidence: the old
//! function did not read it either. ⚠️ Note this differs from the kernel's
//! `ADD_CURSOR`, which forbids `AltGraph` because `AltGr` is reported as
//! `Ctrl+Alt` on many layouts. The explorer has no `Ctrl+Alt`+character binding
//! that a composing key could collide with except the toggle chord, and
//! *tightening* it here would be a behaviour change smuggled in under a
//! refactor. Named as a known difference rather than silently harmonized.
//!
//! # What is deliberately not in this table
//!
//! The printable-character fall-through. `(Plain, Char(_))` was the last arm of
//! both old tables and it is not a binding — it is "every character nothing
//! claimed goes into the field on this screen", the query while browsing and
//! the filename while editing. Turning it into twenty-six bindings would make a
//! keymap that could not express a *field*. It stays below resolution, in
//! [`super::keys`], and a user who binds a bare letter to a command takes that
//! letter out of the field, which is the honest consequence of what they asked
//! for.
//!
//! [`Any`]: ModifierState::Any

use iridium_editor::commands::builtin::{
    EXPLORER_ACTIVATE, EXPLORER_BEGIN_EDIT, EXPLORER_COLLAPSE, EXPLORER_CONFIRM_APPLY,
    EXPLORER_CONFIRM_CANCEL, EXPLORER_CONFIRM_MODE, EXPLORER_DISMISS, EXPLORER_EDIT_ASK_APPLY,
    EXPLORER_EDIT_BACKSPACE, EXPLORER_EDIT_CARET_END, EXPLORER_EDIT_CARET_HOME,
    EXPLORER_EDIT_CARET_LEFT, EXPLORER_EDIT_CARET_RIGHT, EXPLORER_EDIT_CURSOR_DOWN,
    EXPLORER_EDIT_CURSOR_UP, EXPLORER_EDIT_DELETE, EXPLORER_EDIT_LEAVE, EXPLORER_EDIT_MODE,
    EXPLORER_EDIT_NEW_ROW, EXPLORER_EDIT_STRIKE_ROW, EXPLORER_EXPAND, EXPLORER_MODE,
    EXPLORER_MOVE_DOWN, EXPLORER_MOVE_TO_FIRST, EXPLORER_MOVE_TO_LAST, EXPLORER_MOVE_UP,
    EXPLORER_QUERY_BACKSPACE, EXPLORER_REFUSED_DISMISS, EXPLORER_REFUSED_MODE, EXPLORER_ROOT_ABOVE,
    EXPLORER_ROOT_AT_SELECTION, EXPLORER_TOGGLE_HIDDEN, EXPLORER_TOGGLE_PANEL,
    EXPLORER_TOGGLE_SIDEBAR,
};
use iridium_editor::{
    CommandId, KeyBinding, KeyCode, Keymap, ModeName, ModifierPattern, ModifierState, StrokePattern,
};

use ModifierState::{Any, Forbidden, Required};

/// The name this layer is pushed under, quoted in every keymap diagnostic.
pub const KEYMAP_NAME: &str = "explorer";

/// The number of bindings in the explorer's default keymap.
///
/// Asserted in the module tests so the documented count cannot drift.
pub const DEFAULT_BINDING_COUNT: usize = 42;

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
/// Meta alone — the old `Chord::Meta`.
const META: ModifierPattern = pattern(Any, Forbidden, Forbidden, Required, Any);
/// Control and Alt together — the old `Chord::CtrlAlt`.
const CTRL_ALT: ModifierPattern = pattern(Any, Required, Required, Forbidden, Any);
/// Meta and Alt together — the old `Chord::MetaAlt`.
const META_ALT: ModifierPattern = pattern(Any, Forbidden, Required, Required, Any);

/// Every modifier pattern this table uses, for the shift ratchet to walk.
///
/// A `const` list rather than one written out in the test, so that a pattern
/// added here without `Any` shift is caught by the ratchet rather than by
/// somebody noticing.
#[cfg(test)]
pub(super) const EVERY_PATTERN: &[ModifierPattern] = &[PLAIN, CTRL, META, CTRL_ALT, META_ALT];

/// One binding: a chord, in a mode, running a command.
fn bind(
    modifiers: ModifierPattern,
    key: KeyCode,
    command: CommandId,
    mode: ModeName,
) -> KeyBinding {
    KeyBinding::new(StrokePattern::new(key, modifiers), &[], command).in_mode(mode)
}

/// The explorer's default bindings, as a layer.
///
/// Built rather than `static` because [`KeyBinding`] owns its stroke list, which
/// has drop glue; the cost is one allocation per panel, once, at construction.
#[must_use]
pub fn default_keymap() -> Keymap {
    let mut keymap = Keymap::new(KEYMAP_NAME);
    keymap.extend(browsing());
    keymap.extend(editing());
    keymap.extend(confirming());
    keymap.extend(refused());
    keymap
}

/// The bindings that answer keys while the panel is browsing a tree.
fn browsing() -> Vec<KeyBinding> {
    let mode = || EXPLORER_MODE;
    vec![
        // The two toggle chords are named here rather than left to the host,
        // because the panel is modal: a chord it does not name is swallowed by
        // the fall-through and never reaches the command it belongs to.
        bind(CTRL_ALT, KeyCode::Char('e'), EXPLORER_TOGGLE_PANEL, mode()),
        bind(META_ALT, KeyCode::Char('e'), EXPLORER_TOGGLE_PANEL, mode()),
        // `⌘B` without `⌥`, because that is what VS Code and Zed both bind.
        bind(
            CTRL_ALT,
            KeyCode::Char('b'),
            EXPLORER_TOGGLE_SIDEBAR,
            mode(),
        ),
        bind(META, KeyCode::Char('b'), EXPLORER_TOGGLE_SIDEBAR, mode()),
        bind(PLAIN, KeyCode::Escape, EXPLORER_DISMISS, mode()),
        bind(PLAIN, KeyCode::Enter, EXPLORER_ACTIVATE, mode()),
        // Tab is the ruled key into edit mode. Nothing else in this panel uses
        // it, and there is no focus ring for it to walk.
        bind(PLAIN, KeyCode::Tab, EXPLORER_BEGIN_EDIT, mode()),
        bind(PLAIN, KeyCode::Up, EXPLORER_MOVE_UP, mode()),
        bind(CTRL, KeyCode::Char('p'), EXPLORER_MOVE_UP, mode()),
        bind(PLAIN, KeyCode::Down, EXPLORER_MOVE_DOWN, mode()),
        bind(CTRL, KeyCode::Char('n'), EXPLORER_MOVE_DOWN, mode()),
        // `⌘↓` and `⌘↑` are what macOS itself binds for "open this folder" and
        // "enclosing folder"; `Ctrl` is the same pair for a keyboard with no
        // command key.
        bind(META, KeyCode::Down, EXPLORER_ROOT_AT_SELECTION, mode()),
        bind(CTRL, KeyCode::Down, EXPLORER_ROOT_AT_SELECTION, mode()),
        bind(META, KeyCode::Up, EXPLORER_ROOT_ABOVE, mode()),
        bind(CTRL, KeyCode::Up, EXPLORER_ROOT_ABOVE, mode()),
        // `.` for dotfiles — the mnemonic every file manager with this key
        // uses, and unshifted on every layout this face runs on. **Not `⌘H`**,
        // which macOS takes to hide the application before any window sees it.
        bind(META, KeyCode::Char('.'), EXPLORER_TOGGLE_HIDDEN, mode()),
        bind(CTRL, KeyCode::Char('.'), EXPLORER_TOGGLE_HIDDEN, mode()),
        bind(PLAIN, KeyCode::Home, EXPLORER_MOVE_TO_FIRST, mode()),
        bind(PLAIN, KeyCode::End, EXPLORER_MOVE_TO_LAST, mode()),
        // `←` and `→` are the tree's. A filtered view has no expansion state to
        // walk, and the panel refuses them there — that guard is a property of
        // the screen rather than of the binding, so it stays in `keys`.
        bind(PLAIN, KeyCode::Right, EXPLORER_EXPAND, mode()),
        bind(PLAIN, KeyCode::Left, EXPLORER_COLLAPSE, mode()),
        bind(PLAIN, KeyCode::Backspace, EXPLORER_QUERY_BACKSPACE, mode()),
    ]
}

/// The bindings that answer keys while the rows are being edited.
fn editing() -> Vec<KeyBinding> {
    let mode = || EXPLORER_EDIT_MODE;
    vec![
        bind(CTRL_ALT, KeyCode::Char('e'), EXPLORER_TOGGLE_PANEL, mode()),
        bind(META_ALT, KeyCode::Char('e'), EXPLORER_TOGGLE_PANEL, mode()),
        bind(PLAIN, KeyCode::Escape, EXPLORER_EDIT_LEAVE, mode()),
        bind(CTRL, KeyCode::Char('d'), EXPLORER_EDIT_STRIKE_ROW, mode()),
        // `Ctrl+Enter` rather than `Ctrl+N`, because `Ctrl+N` was the movement
        // pair long before the oil buffer existed.
        bind(CTRL, KeyCode::Enter, EXPLORER_EDIT_NEW_ROW, mode()),
        bind(META, KeyCode::Char('s'), EXPLORER_EDIT_ASK_APPLY, mode()),
        bind(PLAIN, KeyCode::Up, EXPLORER_EDIT_CURSOR_UP, mode()),
        bind(CTRL, KeyCode::Char('p'), EXPLORER_EDIT_CURSOR_UP, mode()),
        bind(PLAIN, KeyCode::Down, EXPLORER_EDIT_CURSOR_DOWN, mode()),
        bind(CTRL, KeyCode::Char('n'), EXPLORER_EDIT_CURSOR_DOWN, mode()),
        // The four motions go to the *name*, not the list. A field that took
        // two of them and gave the others to the tree would be a field with no
        // rule.
        bind(PLAIN, KeyCode::Left, EXPLORER_EDIT_CARET_LEFT, mode()),
        bind(PLAIN, KeyCode::Right, EXPLORER_EDIT_CARET_RIGHT, mode()),
        bind(PLAIN, KeyCode::Home, EXPLORER_EDIT_CARET_HOME, mode()),
        bind(PLAIN, KeyCode::End, EXPLORER_EDIT_CARET_END, mode()),
        bind(PLAIN, KeyCode::Backspace, EXPLORER_EDIT_BACKSPACE, mode()),
        bind(PLAIN, KeyCode::Delete, EXPLORER_EDIT_DELETE, mode()),
    ]
}

/// The three keys the confirmation screen prints at the bottom of itself.
///
/// ⛔ `⌘S` is deliberately absent: it is what got the user *to* this screen, and
/// a held key repeating into an apply is the one way a confirmation can be
/// answered by an accident of timing rather than by a decision.
fn confirming() -> Vec<KeyBinding> {
    let mode = || EXPLORER_CONFIRM_MODE;
    vec![
        bind(PLAIN, KeyCode::Char('y'), EXPLORER_CONFIRM_APPLY, mode()),
        bind(PLAIN, KeyCode::Char('n'), EXPLORER_CONFIRM_CANCEL, mode()),
        bind(PLAIN, KeyCode::Escape, EXPLORER_CONFIRM_CANCEL, mode()),
    ]
}

/// The one key a refusal screen answers.
fn refused() -> Vec<KeyBinding> {
    vec![bind(
        PLAIN,
        KeyCode::Escape,
        EXPLORER_REFUSED_DISMISS,
        EXPLORER_REFUSED_MODE,
    )]
}
