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
//! - **`Ctrl+Z` undoes and `Ctrl+Shift+Z` redoes.** The registry migration
//!   transcribed the pre-registry dispatch faithfully, and that dispatch matched
//!   `'z'` regardless of `Shift` — so `Ctrl+Shift+Z` undid. That was correct for
//!   a phase forbidden from changing behaviour, and is corrected here.
//! - **`Ctrl+K` opens the command palette**, as do `Ctrl+P` and `Ctrl+Shift+P`.
//!   It briefly held a `Ctrl+K Ctrl+D` chord for the skip-occurrence verb — the
//!   multi-key machinery's first real user — and that chord was given up for this,
//!   knowingly: a bare binding on a sequence forecloses every chord sharing its
//!   prefix, because an exact match fires the instant it completes, so `Ctrl+K`
//!   and `Ctrl+K …` cannot coexist (see [`KeymapStack`] on cross-layer
//!   shadowing).
//!
//!   Two consequences follow, and both are the accepted price rather than
//!   oversights. [`MULTI_CURSOR_SKIP_LAST_OCCURRENCE`](crate::commands::builtin::MULTI_CURSOR_SKIP_LAST_OCCURRENCE)
//!   has no default key: it is
//!   still registered, still implemented, and now reachable *through the palette*,
//!   which is the whole reason the palette was worth the key. And no layer — the
//!   default's or a host's — can put a chord under `Ctrl+K` without unbinding it
//!   first; [`KeymapStack::validate`] reports the attempt rather than letting the
//!   chord silently never fire.
//!
//!   `palette.open` is a **host command**: the kernel names it, binds it and
//!   reports it, and the face opens the UI. See
//!   [`builtin::host`](super::builtin) for why the id lives in the kernel.

use super::builtin::{
    AST_EXPAND_SELECTION, AST_SHRINK_SELECTION, CLIPBOARD_COPY, CLIPBOARD_CUT, CLIPBOARD_PASTE,
    COMMENT_TOGGLE_BLOCK, COMMENT_TOGGLE_LINE, CURSOR_CHAR_LEFT, CURSOR_CHAR_LEFT_SELECT,
    CURSOR_CHAR_RIGHT, CURSOR_CHAR_RIGHT_SELECT, CURSOR_DOCUMENT_END, CURSOR_DOCUMENT_END_SELECT,
    CURSOR_DOCUMENT_START, CURSOR_DOCUMENT_START_SELECT, CURSOR_LINE_DOWN, CURSOR_LINE_DOWN_SELECT,
    CURSOR_LINE_END, CURSOR_LINE_END_SELECT, CURSOR_LINE_START, CURSOR_LINE_START_SELECT,
    CURSOR_LINE_UP, CURSOR_LINE_UP_SELECT, CURSOR_PAGE_DOWN, CURSOR_PAGE_DOWN_SELECT,
    CURSOR_PAGE_UP, CURSOR_PAGE_UP_SELECT, CURSOR_WORD_LEFT, CURSOR_WORD_LEFT_SELECT,
    CURSOR_WORD_RIGHT, CURSOR_WORD_RIGHT_SELECT, EDIT_DELETE_BACKWARD, EDIT_DELETE_FORWARD,
    EDIT_DELETE_WORD_BACKWARD, EDIT_DELETE_WORD_FORWARD, EDIT_INSERT_NEWLINE, EDIT_OUTDENT,
    EDIT_TAB, EXPLORER_TOGGLE_PANEL, HISTORY_NEXT_BRANCH, HISTORY_PREVIOUS_BRANCH, HISTORY_REDO,
    HISTORY_TOGGLE_PANEL, HISTORY_UNDO, LINES_DELETE, LINES_DUPLICATE_DOWN, LINES_DUPLICATE_UP,
    LINES_JOIN, LINES_MOVE_DOWN, LINES_MOVE_UP, MULTI_CURSOR_ADD_CURSOR_ABOVE,
    MULTI_CURSOR_ADD_CURSOR_BELOW, MULTI_CURSOR_ADD_SELECTION_TO_NEXT_MATCH,
    MULTI_CURSOR_REMOVE_LAST_CURSOR, MULTI_CURSOR_SELECT_ALL_OCCURRENCES, PALETTE_OPEN,
    SEARCH_NEXT_MATCH, SEARCH_OPEN, SEARCH_PREVIOUS_MATCH, SELECTION_COLLAPSE_TO_PRIMARY,
    SELECTION_SELECT_ALL, VIEW_TOGGLE_THEME, WORKSPACE_CLOSE_TAB, WORKSPACE_NEXT_TAB,
    WORKSPACE_PREVIOUS_TAB,
};
use super::{
    CommandId, KeyBinding, Keymap, KeymapStack, ModifierPattern, ModifierState, StrokePattern,
};
use crate::input::KeyCode;

/// The number of bindings in the default keymap.
///
/// Asserted in the module tests so the documented count cannot drift.
pub const DEFAULT_KEYMAP_BINDING_COUNT: usize = 67;

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

/// `Shift+Alt`+arrow: grow and shrink the selection by syntax node.
///
/// Deliberately the same shape as [`LINE_DUPLICATE`], one axis over. `Shift+Alt`
/// already reads as "the structural version of" on the vertical arrows, where it
/// duplicates whole lines; on the horizontal arrows it widens and narrows the
/// selection by node, with right meaning outward. That is also what VS Code
/// binds *Expand Selection* and *Shrink Selection* to, so the muscle memory is
/// not invented here.
///
/// More specific than the plain horizontal patterns — which ignore `Alt` — so it
/// wins where it applies and character motion keeps `Alt+Left` elsewhere.
const SYNTAX_SELECT: ModifierPattern = pattern(Required, Forbidden, Required, Forbidden, Any);

/// A `Ctrl`+letter chord that fires with or without `Shift`.
const CTRL_ANY_SHIFT: ModifierPattern = pattern(Any, Required, Forbidden, Forbidden, Any);
/// A `Ctrl`+letter chord that requires `Shift` to be absent.
const CTRL_NO_SHIFT: ModifierPattern = pattern(Forbidden, Required, Forbidden, Forbidden, Any);
/// A `Ctrl+Shift`+letter chord.
const CTRL_SHIFT: ModifierPattern = pattern(Required, Required, Forbidden, Forbidden, Any);
/// A `Ctrl+Alt`+letter chord, Shift ignored: the undo-tree branch verbs.
///
/// `Alt` reads as "the sideways version of", which is what walking between two
/// alternative futures is next to stepping along one — so `Ctrl+Alt+Z` and
/// `Ctrl+Alt+Y` sit exactly where `Ctrl+Z` and `Ctrl+Y` already are in the hand.
const CTRL_ALT: ModifierPattern = pattern(Any, Required, Required, Forbidden, Any);

/// No continuation: a single-chord binding.
///
/// Every entry in the table below uses this. The continuation slot is kept — and
/// exercised by the multi-stroke tests — because the resolver supports chords and
/// a host keymap layer is expected to use them; the *default* keymap simply binds
/// none, now that `Ctrl+K` is reserved (see the module documentation).
const CHORD: &[StrokePattern] = &[];

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
    // The page keys read only `Shift`, exactly as the plain arrows do: a
    // terminal that decorates them with Ctrl or Alt must still page rather
    // than deliver a dead key.
    (
        StrokePattern::new(KeyCode::PageUp, NO_SHIFT),
        CHORD,
        CURSOR_PAGE_UP,
    ),
    (
        StrokePattern::new(KeyCode::PageUp, WITH_SHIFT),
        CHORD,
        CURSOR_PAGE_UP_SELECT,
    ),
    (
        StrokePattern::new(KeyCode::PageDown, NO_SHIFT),
        CHORD,
        CURSOR_PAGE_DOWN,
    ),
    (
        StrokePattern::new(KeyCode::PageDown, WITH_SHIFT),
        CHORD,
        CURSOR_PAGE_DOWN_SELECT,
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
    // `MULTI_CURSOR_SKIP_LAST_OCCURRENCE` deliberately has no binding here: it
    // held `Ctrl+K Ctrl+D`, and `Ctrl+K` is now reserved as the command-palette
    // leader. See the module documentation.
    // ----- Syntax selection -----
    //
    // `AST_SELECT_NODE` is deliberately palette-only: from a caret it lands
    // where one expansion lands, so it earns a key only for the "snap this
    // selection to node boundaries" case, and that is a question about daily use
    // rather than a technical one.
    (
        StrokePattern::new(KeyCode::Right, SYNTAX_SELECT),
        CHORD,
        AST_EXPAND_SELECTION,
    ),
    (
        StrokePattern::new(KeyCode::Left, SYNTAX_SELECT),
        CHORD,
        AST_SHRINK_SELECTION,
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
    // `Ctrl+Z` undoes and `Ctrl+Shift+Z` redoes, which is what every editor a
    // user has ever used does. This was transcribed the other way during the
    // registry migration — the pre-registry dispatch matched `'z'` regardless of
    // `Shift`, so `Ctrl+Shift+Z` undid — and that transcription was correct at
    // the time, because that phase was forbidden from changing behaviour.
    // Correcting it is this phase's job.
    (
        StrokePattern::new(KeyCode::Char('z'), CTRL_NO_SHIFT),
        CHORD,
        HISTORY_UNDO,
    ),
    (
        StrokePattern::new(KeyCode::Char('z'), CTRL_SHIFT),
        CHORD,
        HISTORY_REDO,
    ),
    (
        StrokePattern::new(KeyCode::Char('y'), CTRL_ANY_SHIFT),
        CHORD,
        HISTORY_REDO,
    ),
    // Branch selection changes nothing in the document — it only points redo at
    // a different future — so these are safe to hold down while watching the
    // panel, and are bound rather than left palette-only for exactly that
    // reason: they are a *browsing* verb, and browsing by name is not browsing.
    (
        StrokePattern::new(KeyCode::Char('z'), CTRL_ALT),
        CHORD,
        HISTORY_PREVIOUS_BRANCH,
    ),
    (
        StrokePattern::new(KeyCode::Char('y'), CTRL_ALT),
        CHORD,
        HISTORY_NEXT_BRANCH,
    ),
    // The panel joins its two navigation verbs on `Ctrl+Alt`, so the whole
    // undo-tree family is one chord shape rather than three unrelated keys.
    (
        StrokePattern::new(KeyCode::Char('h'), CTRL_ALT),
        CHORD,
        HISTORY_TOGGLE_PANEL,
    ),
    // ----- File explorer -----
    //
    // `Ctrl+Alt+E`, on the same shape as the undo tree: both are panels a
    // toggle puts up and takes away, and grouping them means one chord to
    // remember rather than two unrelated keys.
    (
        StrokePattern::new(KeyCode::Char('e'), CTRL_ALT),
        CHORD,
        EXPLORER_TOGGLE_PANEL,
    ),
    // ----- Theme -----
    //
    // `Ctrl+Alt+T`, joining the panel toggles on the same shape. It is not a
    // panel, but it is the third thing in the editor that a single chord turns
    // from one state to the other, and `T` is the letter nobody has to be told.
    //
    // ⚠️ `CTRL_ALT` forbids `Meta` and ignores `Shift`, so this cannot be
    // reached by the desktop face's ⌘ layer — that face binds `⌘⌥T` to the
    // same id separately, which is the point of the id being in the kernel.
    (
        StrokePattern::new(KeyCode::Char('t'), CTRL_ALT),
        CHORD,
        VIEW_TOGGLE_THEME,
    ),
    // ----- Command palette -----
    //
    // `Ctrl+K` must forbid `Shift`, or it would swallow the `Ctrl+Shift+K` that
    // deletes a line. `Ctrl+P` takes either, so one binding serves both the
    // `Ctrl+P` and the `Ctrl+Shift+P` muscle memory.
    (
        StrokePattern::new(KeyCode::Char('k'), CTRL_NO_SHIFT),
        CHORD,
        PALETTE_OPEN,
    ),
    (
        StrokePattern::new(KeyCode::Char('p'), CTRL_ANY_SHIFT),
        CHORD,
        PALETTE_OPEN,
    ),
    // ----- Tabs -----
    //
    // `Ctrl+Shift+]` and `Ctrl+Shift+[`, which the web face receives from
    // macOS's `Cmd+Shift+]` / `Cmd+Shift+[` — the bracket pair every editor
    // on this platform already uses to walk a tab strip. `Ctrl+PageDown`
    // was the obvious alternative and is **not available**: the paging
    // bindings above take `Any` for control, so `Ctrl+PageDown` already
    // resolves to page-down and would be shadowed rather than added.
    (
        StrokePattern::new(KeyCode::Char(']'), CTRL_SHIFT),
        CHORD,
        WORKSPACE_NEXT_TAB,
    ),
    (
        StrokePattern::new(KeyCode::Char('['), CTRL_SHIFT),
        CHORD,
        WORKSPACE_PREVIOUS_TAB,
    ),
    // Closing takes `Ctrl+W` with shift forbidden, leaving `Ctrl+Shift+W`
    // free for a future close-window rather than silently claiming it.
    (
        StrokePattern::new(KeyCode::Char('w'), CTRL_NO_SHIFT),
        CHORD,
        WORKSPACE_CLOSE_TAB,
    ),
    // `workspace.firstTab` and `workspace.lastTab` are deliberately left
    // unbound: they are palette verbs. Binding every command is what
    // exhausts the chord space, and jumping to the ends of a tab strip is
    // not something anyone reaches for by key.
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
