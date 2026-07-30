//! The default non-modal keymap, expressed as data.
//!
//! Every binding the keyboard handler's `match` statement implements today is
//! transcribed here, including the modifier bits that dispatch deliberately does
//! *not* consult. Where the current dispatch ignores a modifier — `Backspace`
//! deletes backwards whatever `Shift` is doing, `Ctrl+C` copies with or without
//! `Shift`, `Enter` and `Escape` ignore every modifier — the binding says
//! [`ModifierState::Any`] rather than pretending the chord is exact. That is what
//! makes this table a faithful transcription rather than a subtly stricter
//! reimplementation.
//!
//! Two entries deserve calling out explicitly:
//!
//! - **`Ctrl+Shift+Z` is bound to undo, not redo.** The current dispatch matches
//!   `'z'` regardless of `Shift`, so that is what it does. This is a known defect
//!   (see [`builtin::HISTORY_REDO`](super::builtin::HISTORY_REDO)); it is
//!   transcribed rather than fixed, because this phase must not change observable
//!   behaviour.
//! - **`Ctrl+K Ctrl+D` is the one additive binding, and it is a deliberate
//!   behaviour change at the host boundary.** The skip-occurrence verb exists and
//!   is tested, but had no key because the codebase had no chord support. Binding
//!   it here gives the multi-key machinery a real user from day one and makes a
//!   command that was previously reachable only from Rust reachable from the
//!   keyboard. No *pre-existing* binding changes meaning — but plain `Ctrl+K`
//!   changes from `KeyResult::Ignored` to `KeyResult::Handled`, and that is
//!   observable:
//!
//!   - the web host repaints and calls `preventDefault()` on `Ctrl+K`, where
//!     before it passed the key through to the browser;
//!   - on macOS the web host forwards both `Cmd` and `Ctrl` as the kernel's
//!     `ctrl`, so `Cmd+K` *and* Cocoa's `Ctrl+K` kill-to-end-of-line both arm the
//!     leader, and the latter no longer reaches the OS. This matches VS Code on
//!     macOS, where `Cmd+K` is likewise a chord leader.
//!
//!   What the leader must never do is *destroy* a keystroke. It does not: a stroke
//!   that cannot continue the sequence is retried from scratch, so `Ctrl+K` then
//!   `h` types `h`, and `Ctrl+K` then `Ctrl+Shift+D` runs the `Ctrl+Shift+D`
//!   binding. Hosts should also surface `KeyboardHandler::pending_sequence` so the
//!   consumed leader is visible rather than looking like a dropped key.

use super::builtin::{
    CLIPBOARD_COPY, CLIPBOARD_CUT, CLIPBOARD_PASTE, COMMENT_TOGGLE_BLOCK, COMMENT_TOGGLE_LINE,
    CURSOR_CHAR_LEFT, CURSOR_CHAR_LEFT_SELECT, CURSOR_CHAR_RIGHT, CURSOR_CHAR_RIGHT_SELECT,
    CURSOR_DOCUMENT_END, CURSOR_DOCUMENT_END_SELECT, CURSOR_DOCUMENT_START,
    CURSOR_DOCUMENT_START_SELECT, CURSOR_LINE_DOWN, CURSOR_LINE_DOWN_SELECT, CURSOR_LINE_END,
    CURSOR_LINE_END_SELECT, CURSOR_LINE_START, CURSOR_LINE_START_SELECT, CURSOR_LINE_UP,
    CURSOR_LINE_UP_SELECT, CURSOR_WORD_LEFT, CURSOR_WORD_LEFT_SELECT, CURSOR_WORD_RIGHT,
    CURSOR_WORD_RIGHT_SELECT, EDIT_DELETE_BACKWARD, EDIT_DELETE_FORWARD, EDIT_DELETE_WORD_BACKWARD,
    EDIT_DELETE_WORD_FORWARD, EDIT_INSERT_NEWLINE, EDIT_OUTDENT, EDIT_TAB, HISTORY_REDO,
    HISTORY_UNDO, LINES_DELETE, LINES_DUPLICATE_DOWN, LINES_DUPLICATE_UP, LINES_JOIN,
    LINES_MOVE_DOWN, LINES_MOVE_UP, MULTI_CURSOR_ADD_CURSOR_ABOVE, MULTI_CURSOR_ADD_CURSOR_BELOW,
    MULTI_CURSOR_ADD_SELECTION_TO_NEXT_MATCH, MULTI_CURSOR_REMOVE_LAST_CURSOR,
    MULTI_CURSOR_SELECT_ALL_OCCURRENCES, MULTI_CURSOR_SKIP_LAST_OCCURRENCE, SEARCH_NEXT_MATCH,
    SEARCH_OPEN, SEARCH_PREVIOUS_MATCH, SELECTION_COLLAPSE_TO_PRIMARY, SELECTION_SELECT_ALL,
};
use super::{
    CommandId, KeyBinding, Keymap, KeymapStack, ModifierPattern, ModifierState, StrokePattern,
};
use crate::input::KeyCode;

/// The number of bindings in the default keymap.
///
/// Asserted in the module tests so the documented count cannot drift.
pub const DEFAULT_KEYMAP_BINDING_COUNT: usize = 51;

use ModifierState::{Any, Forbidden, Required};

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

/// Every modifier ignored: `Enter` and `Escape`, which today consult none.
const ANY_MODS: ModifierPattern = ModifierPattern::ANY;

/// `Shift` must be absent, everything else ignored: plain `Up`/`Down`, `Tab`, `F3`.
const NO_SHIFT: ModifierPattern = pattern(Forbidden, Any, Any, Any, Any);
/// `Shift` must be held, everything else ignored: `Shift+Up`, `Shift+Tab`, `Shift+F3`.
const WITH_SHIFT: ModifierPattern = pattern(Required, Any, Any, Any, Any);

/// `Ctrl` must be absent, everything else ignored: `Backspace`, `Delete`.
const NO_CTRL: ModifierPattern = pattern(Any, Forbidden, Any, Any, Any);
/// `Ctrl` must be held, everything else ignored: `Ctrl+Backspace`, `Ctrl+Delete`.
const WITH_CTRL: ModifierPattern = pattern(Any, Required, Any, Any, Any);

/// Horizontal navigation: `Ctrl` and `Shift` are read, `Alt`/`Meta` ignored.
const NAV: ModifierPattern = pattern(Forbidden, Forbidden, Any, Any, Any);
/// Horizontal navigation with `Shift`.
const NAV_SHIFT: ModifierPattern = pattern(Required, Forbidden, Any, Any, Any);
/// Horizontal navigation with `Ctrl`.
const NAV_CTRL: ModifierPattern = pattern(Forbidden, Required, Any, Any, Any);
/// Horizontal navigation with `Ctrl+Shift`.
const NAV_CTRL_SHIFT: ModifierPattern = pattern(Required, Required, Any, Any, Any);

/// `Alt` line move: `Alt` held, `Ctrl`/`Meta`/`Shift` absent, `AltGraph` ignored.
const LINE_MOVE: ModifierPattern = pattern(Forbidden, Forbidden, Required, Forbidden, Any);
/// `Shift+Alt` line duplicate.
const LINE_DUPLICATE: ModifierPattern = pattern(Required, Forbidden, Required, Forbidden, Any);

/// `Ctrl+Alt` add-cursor chord, with the `AltGraph` guard.
///
/// `AltGraph` is [`ModifierState::Forbidden`] because on many non-US layouts
/// `AltGr` is reported as `Ctrl+Alt` held together while composing a character;
/// without the guard, `AltGr`+Arrow would spawn cursors instead of typing `@`.
const ADD_CURSOR: ModifierPattern = pattern(Forbidden, Required, Required, Forbidden, Forbidden);

/// `Shift+Alt` letter chord: the block-comment toggle.
const SHIFT_ALT: ModifierPattern = pattern(Required, Forbidden, Required, Forbidden, Any);

/// A `Ctrl`+letter chord that fires with or without `Shift`.
const CTRL_ANY_SHIFT: ModifierPattern = pattern(Any, Required, Forbidden, Forbidden, Any);
/// A `Ctrl`+letter chord that requires `Shift` to be absent.
const CTRL_NO_SHIFT: ModifierPattern = pattern(Forbidden, Required, Forbidden, Forbidden, Any);
/// A `Ctrl+Shift`+letter chord.
const CTRL_SHIFT: ModifierPattern = pattern(Required, Required, Forbidden, Forbidden, Any);

/// No continuation: a single-chord binding.
const CHORD: &[StrokePattern] = &[];
/// The second stroke of `Ctrl+K Ctrl+D`.
const THEN_CTRL_D: &[StrokePattern] = &[StrokePattern::new(KeyCode::Char('d'), CTRL_NO_SHIFT)];

/// The default binding table: `(first stroke, continuation, command)`.
const BINDINGS: &[(StrokePattern, &[StrokePattern], CommandId)] = &[
    // ----- Horizontal navigation and selection -----
    (
        StrokePattern::new(KeyCode::Left, NAV),
        CHORD,
        CURSOR_CHAR_LEFT,
    ),
    (
        StrokePattern::new(KeyCode::Left, NAV_SHIFT),
        CHORD,
        CURSOR_CHAR_LEFT_SELECT,
    ),
    (
        StrokePattern::new(KeyCode::Left, NAV_CTRL),
        CHORD,
        CURSOR_WORD_LEFT,
    ),
    (
        StrokePattern::new(KeyCode::Left, NAV_CTRL_SHIFT),
        CHORD,
        CURSOR_WORD_LEFT_SELECT,
    ),
    (
        StrokePattern::new(KeyCode::Right, NAV),
        CHORD,
        CURSOR_CHAR_RIGHT,
    ),
    (
        StrokePattern::new(KeyCode::Right, NAV_SHIFT),
        CHORD,
        CURSOR_CHAR_RIGHT_SELECT,
    ),
    (
        StrokePattern::new(KeyCode::Right, NAV_CTRL),
        CHORD,
        CURSOR_WORD_RIGHT,
    ),
    (
        StrokePattern::new(KeyCode::Right, NAV_CTRL_SHIFT),
        CHORD,
        CURSOR_WORD_RIGHT_SELECT,
    ),
    (
        StrokePattern::new(KeyCode::Home, NAV),
        CHORD,
        CURSOR_LINE_START,
    ),
    (
        StrokePattern::new(KeyCode::Home, NAV_SHIFT),
        CHORD,
        CURSOR_LINE_START_SELECT,
    ),
    (
        StrokePattern::new(KeyCode::Home, NAV_CTRL),
        CHORD,
        CURSOR_DOCUMENT_START,
    ),
    (
        StrokePattern::new(KeyCode::Home, NAV_CTRL_SHIFT),
        CHORD,
        CURSOR_DOCUMENT_START_SELECT,
    ),
    (
        StrokePattern::new(KeyCode::End, NAV),
        CHORD,
        CURSOR_LINE_END,
    ),
    (
        StrokePattern::new(KeyCode::End, NAV_SHIFT),
        CHORD,
        CURSOR_LINE_END_SELECT,
    ),
    (
        StrokePattern::new(KeyCode::End, NAV_CTRL),
        CHORD,
        CURSOR_DOCUMENT_END,
    ),
    (
        StrokePattern::new(KeyCode::End, NAV_CTRL_SHIFT),
        CHORD,
        CURSOR_DOCUMENT_END_SELECT,
    ),
    // ----- Vertical navigation -----
    //
    // The plain patterns read only `Shift`, mirroring dispatch: `Ctrl+Up` and
    // `Meta+Up` fall through to caret movement today. The line and add-cursor
    // chords below are more specific, so they win where they apply.
    (
        StrokePattern::new(KeyCode::Up, NO_SHIFT),
        CHORD,
        CURSOR_LINE_UP,
    ),
    (
        StrokePattern::new(KeyCode::Up, WITH_SHIFT),
        CHORD,
        CURSOR_LINE_UP_SELECT,
    ),
    (
        StrokePattern::new(KeyCode::Down, NO_SHIFT),
        CHORD,
        CURSOR_LINE_DOWN,
    ),
    (
        StrokePattern::new(KeyCode::Down, WITH_SHIFT),
        CHORD,
        CURSOR_LINE_DOWN_SELECT,
    ),
    // ----- Line operations -----
    (
        StrokePattern::new(KeyCode::Up, LINE_MOVE),
        CHORD,
        LINES_MOVE_UP,
    ),
    (
        StrokePattern::new(KeyCode::Down, LINE_MOVE),
        CHORD,
        LINES_MOVE_DOWN,
    ),
    (
        StrokePattern::new(KeyCode::Up, LINE_DUPLICATE),
        CHORD,
        LINES_DUPLICATE_UP,
    ),
    (
        StrokePattern::new(KeyCode::Down, LINE_DUPLICATE),
        CHORD,
        LINES_DUPLICATE_DOWN,
    ),
    (
        StrokePattern::new(KeyCode::Char('k'), CTRL_SHIFT),
        CHORD,
        LINES_DELETE,
    ),
    (
        StrokePattern::new(KeyCode::Char('j'), CTRL_NO_SHIFT),
        CHORD,
        LINES_JOIN,
    ),
    // ----- Multi-cursor -----
    (
        StrokePattern::new(KeyCode::Up, ADD_CURSOR),
        CHORD,
        MULTI_CURSOR_ADD_CURSOR_ABOVE,
    ),
    (
        StrokePattern::new(KeyCode::Down, ADD_CURSOR),
        CHORD,
        MULTI_CURSOR_ADD_CURSOR_BELOW,
    ),
    (
        StrokePattern::new(KeyCode::Char('d'), CTRL_ANY_SHIFT),
        CHORD,
        MULTI_CURSOR_ADD_SELECTION_TO_NEXT_MATCH,
    ),
    (
        StrokePattern::new(KeyCode::Char('l'), CTRL_SHIFT),
        CHORD,
        MULTI_CURSOR_SELECT_ALL_OCCURRENCES,
    ),
    (
        StrokePattern::new(KeyCode::Char('u'), CTRL_NO_SHIFT),
        CHORD,
        MULTI_CURSOR_REMOVE_LAST_CURSOR,
    ),
    (
        StrokePattern::new(KeyCode::Char('k'), CTRL_NO_SHIFT),
        THEN_CTRL_D,
        MULTI_CURSOR_SKIP_LAST_OCCURRENCE,
    ),
    // ----- Editing -----
    (
        StrokePattern::new(KeyCode::Enter, ANY_MODS),
        CHORD,
        EDIT_INSERT_NEWLINE,
    ),
    (StrokePattern::new(KeyCode::Tab, NO_SHIFT), CHORD, EDIT_TAB),
    (
        StrokePattern::new(KeyCode::Tab, WITH_SHIFT),
        CHORD,
        EDIT_OUTDENT,
    ),
    (
        StrokePattern::new(KeyCode::Backspace, NO_CTRL),
        CHORD,
        EDIT_DELETE_BACKWARD,
    ),
    (
        StrokePattern::new(KeyCode::Backspace, WITH_CTRL),
        CHORD,
        EDIT_DELETE_WORD_BACKWARD,
    ),
    (
        StrokePattern::new(KeyCode::Delete, NO_CTRL),
        CHORD,
        EDIT_DELETE_FORWARD,
    ),
    (
        StrokePattern::new(KeyCode::Delete, WITH_CTRL),
        CHORD,
        EDIT_DELETE_WORD_FORWARD,
    ),
    // ----- Selection verbs -----
    (
        StrokePattern::new(KeyCode::Char('a'), CTRL_ANY_SHIFT),
        CHORD,
        SELECTION_SELECT_ALL,
    ),
    (
        StrokePattern::new(KeyCode::Escape, ANY_MODS),
        CHORD,
        SELECTION_COLLAPSE_TO_PRIMARY,
    ),
    // ----- Comments -----
    (
        StrokePattern::new(KeyCode::Char('/'), CTRL_ANY_SHIFT),
        CHORD,
        COMMENT_TOGGLE_LINE,
    ),
    (
        StrokePattern::new(KeyCode::Char('a'), SHIFT_ALT),
        CHORD,
        COMMENT_TOGGLE_BLOCK,
    ),
    // ----- Clipboard -----
    (
        StrokePattern::new(KeyCode::Char('c'), CTRL_ANY_SHIFT),
        CHORD,
        CLIPBOARD_COPY,
    ),
    (
        StrokePattern::new(KeyCode::Char('x'), CTRL_ANY_SHIFT),
        CHORD,
        CLIPBOARD_CUT,
    ),
    (
        StrokePattern::new(KeyCode::Char('v'), CTRL_ANY_SHIFT),
        CHORD,
        CLIPBOARD_PASTE,
    ),
    // ----- History -----
    //
    // `Ctrl+Shift+Z` maps to undo, faithfully reproducing the current dispatch.
    (
        StrokePattern::new(KeyCode::Char('z'), CTRL_ANY_SHIFT),
        CHORD,
        HISTORY_UNDO,
    ),
    (
        StrokePattern::new(KeyCode::Char('y'), CTRL_ANY_SHIFT),
        CHORD,
        HISTORY_REDO,
    ),
    // ----- Search -----
    (
        StrokePattern::new(KeyCode::Char('f'), CTRL_ANY_SHIFT),
        CHORD,
        SEARCH_OPEN,
    ),
    (
        StrokePattern::new(KeyCode::F3, NO_SHIFT),
        CHORD,
        SEARCH_NEXT_MATCH,
    ),
    (
        StrokePattern::new(KeyCode::F3, WITH_SHIFT),
        CHORD,
        SEARCH_PREVIOUS_MATCH,
    ),
];

/// Builds the default non-modal keymap.
///
/// Mode-free throughout: every binding applies in every mode, which is what makes
/// this keymap the base layer a modal keymap can be stacked on top of.
#[must_use]
pub fn default_non_modal_keymap() -> Keymap {
    let mut keymap = Keymap::new("default");
    for (first, rest, command) in BINDINGS {
        keymap.push(KeyBinding::new(*first, rest, command.clone()));
    }
    keymap
}

/// Builds a stack containing only the default non-modal keymap.
///
/// A host adds its user keymap with [`KeymapStack::push`], which then overrides
/// the default without editing it.
#[must_use]
pub fn default_keymap_stack() -> KeymapStack {
    KeymapStack::with_base(default_non_modal_keymap())
}
