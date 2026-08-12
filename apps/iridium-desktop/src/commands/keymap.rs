//! The keys this face binds, and the modifier patterns behind them.
//!
//! Everything here is a *spelling*: which chord reaches which id. The ids
//! themselves are [`ids`](super::ids)' and the kernel's. See the module doc on
//! [`commands`](super) for the two tables and the reasoning behind every row.

use iridium_editor::commands::builtin::{
    AST_EXPAND_SELECTION, AST_SHRINK_SELECTION, CLIPBOARD_COPY, CLIPBOARD_CUT, CLIPBOARD_PASTE,
    COMMENT_TOGGLE_LINE, CONFIG_RELOAD, CURSOR_DOCUMENT_END, CURSOR_DOCUMENT_END_SELECT,
    CURSOR_DOCUMENT_START, CURSOR_DOCUMENT_START_SELECT, CURSOR_LINE_END, CURSOR_LINE_END_SELECT,
    CURSOR_LINE_START, CURSOR_LINE_START_SELECT, CURSOR_WORD_LEFT, CURSOR_WORD_LEFT_SELECT,
    CURSOR_WORD_RIGHT, CURSOR_WORD_RIGHT_SELECT, EDIT_DELETE_TO_LINE_END,
    EDIT_DELETE_TO_LINE_START, EDIT_DELETE_WORD_BACKWARD, EDIT_DELETE_WORD_FORWARD,
    EXPLORER_TOGGLE_PANEL, EXPLORER_TOGGLE_PLACEMENT, HISTORY_NEXT_BRANCH, HISTORY_PREVIOUS_BRANCH,
    HISTORY_REDO, HISTORY_TOGGLE_PANEL, HISTORY_UNDO, LINES_DELETE, LINES_JOIN,
    MULTI_CURSOR_ADD_CURSOR_ABOVE, MULTI_CURSOR_ADD_CURSOR_BELOW,
    MULTI_CURSOR_ADD_SELECTION_TO_NEXT_MATCH, MULTI_CURSOR_REMOVE_LAST_CURSOR,
    MULTI_CURSOR_SELECT_ALL_OCCURRENCES, PALETTE_OPEN, SEARCH_OPEN, SELECTION_SELECT_ALL,
    VIEW_TOGGLE_THEME, WORKSPACE_CLOSE_TAB, WORKSPACE_NEXT_TAB, WORKSPACE_PREVIOUS_TAB,
};
use iridium_editor::{
    CommandId, KeyBinding, KeyCode, Keymap, ModifierPattern, ModifierState, StrokePattern,
};

use ModifierState::{Any, Forbidden, Required};

use super::ids::{FILE_NEW, FILE_OPEN, FILE_SAVE, FILE_SAVE_AS, FILE_SAVE_FORCE, PROJECT_OPEN};

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

/// A `Ctrl+Alt` chord. `AltGraph` is forbidden for the reason the kernel's
/// add-cursor chord forbids it: on many layouts `AltGr` is reported as
/// `Ctrl+Alt` while composing a character, and a forced save is not something
/// to do by accident while typing `@`.
const CTRL_ALT: ModifierPattern = pattern(Any, Required, Required, Forbidden, Forbidden);

/// A bare `Ctrl` chord with `Shift` absent — the unshifted half of a pair
/// whose shifted spelling is a *different verb*.
///
/// A pattern spelling `Shift` [`Any`] matches the shifted chord as well, so the
/// unshifted row would swallow it or be swallowed by it depending on which
/// outranked the other. Every letter on this face that carries two verbs — `O`
/// (open file / open folder), `S` (save / save as), `N` (new file, holding
/// `⌘⇧N` open for a new window) — is split with this and [`CTRL_SHIFT`] rather
/// than left to that ordering.
///
/// ⚠️ The `Ctrl+S` row used the `Shift`-ignoring shape until 12 Aug 2026, which
/// is exactly how `Ctrl+⇧S` came to save rather than save-as.
const CTRL_NO_SHIFT: ModifierPattern = pattern(Forbidden, Required, Forbidden, Forbidden, Any);

/// A `Ctrl+⇧` chord: the shifted half of [`CTRL_NO_SHIFT`].
const CTRL_SHIFT: ModifierPattern = pattern(Required, Required, Forbidden, Forbidden, Any);

/// A bare ⌘ chord: `Shift` ignored — ⌘C copies with or without it, exactly as
/// the default keymap's `Ctrl+C` does — `Ctrl` and `Alt` absent.
const META: ModifierPattern = pattern(Any, Forbidden, Forbidden, Required, Any);

/// A ⌘ chord that requires `Shift` to be absent: `⌘Z` must not swallow `⌘⇧Z`.
const META_NO_SHIFT: ModifierPattern = pattern(Forbidden, Forbidden, Forbidden, Required, Any);

/// A `⌘⇧` chord.
const META_SHIFT: ModifierPattern = pattern(Required, Forbidden, Forbidden, Required, Any);

/// A `⌘⌥` chord, with the `AltGraph` guard [`CTRL_ALT`] carries.
const META_ALT: ModifierPattern = pattern(Any, Forbidden, Required, Required, Forbidden);

/// A bare `⌥` chord on a navigation key: nothing else held.
///
/// `Shift` is [`Forbidden`] rather than [`Any`] deliberately — see the note
/// above the `⌥⇧` gap in [`MAC_CHORDS`]. `AltGraph` is forbidden for the reason
/// [`CTRL_ALT`] forbids it, and on this face it is always reported absent
/// anyway: a mac keyboard has no `AltGr`.
const ALT_NAV: ModifierPattern = pattern(Forbidden, Forbidden, Required, Forbidden, Forbidden);

/// An `⌥⇧` chord on a navigation key: the selecting half of [`ALT_NAV`].
const ALT_SHIFT_NAV: ModifierPattern = pattern(Required, Forbidden, Required, Forbidden, Forbidden);

/// A `⌃⇧⌘` chord on a navigation key — where the syntax expand/shrink verbs
/// live on this face, matching VS Code's mac spelling.
///
/// Nothing else on the box wants this combination, and the kernel's own
/// `Left`/`Right` patterns spell `meta` as [`Any`], so a [`Required`] `meta`
/// here outranks them by the stack's ordinary precedence — the same mechanism
/// every other row in [`MAC_CHORDS`] relies on.
const CTRL_SHIFT_META_NAV: ModifierPattern =
    pattern(Required, Required, Forbidden, Required, Forbidden);

/// A bare `⌘` chord on a navigation key, `Shift` absent.
const META_NAV: ModifierPattern = pattern(Forbidden, Forbidden, Forbidden, Required, Forbidden);

/// A `⌘⇧` chord on a navigation key: the selecting half of [`META_NAV`].
const META_SHIFT_NAV: ModifierPattern =
    pattern(Required, Forbidden, Forbidden, Required, Forbidden);

/// A `⌘⌥` chord on a navigation key — the mac spelling of the kernel's
/// `Ctrl+Alt`+arrow add-cursor pair.
///
/// `Shift` is [`Forbidden`], as the kernel's `ADD_CURSOR` forbids it: growing
/// a selection and spawning a cursor are different verbs and must not share a
/// chord. `AltGraph` is forbidden for the reason `ADD_CURSOR` forbids it,
/// though a mac keyboard has no `AltGr` to report.
const META_ALT_NAV: ModifierPattern = pattern(Forbidden, Forbidden, Required, Required, Forbidden);

/// A bare `⌥` chord on a delete key, `Shift` ignored.
///
/// `Shift` is [`Any`] here for the reason the default keymap's `Backspace`
/// patterns declare it so: a delete key deletes whatever `Shift` is doing, and
/// no `⌥⇧⌫` verb exists for it to swallow.
const ALT_DELETE: ModifierPattern = pattern(Any, Forbidden, Required, Forbidden, Forbidden);

/// A bare `⌘` chord on a delete key, `Shift` ignored.
const META_DELETE: ModifierPattern = pattern(Any, Forbidden, Forbidden, Required, Forbidden);

/// No continuation: every binding here is a single chord.
const CHORD: &[StrokePattern] = &[];

/// The binding table: `(stroke, command)`.
pub(super) const BINDINGS: &[(StrokePattern, CommandId)] = &[
    // ⚠️ **The save rows forbid `Shift` — they used to ignore it.** `⌘⇧S` and
    // `Ctrl+⇧S` reached `file.save` until 12 Aug 2026 simply because nothing
    // else claimed them; they are Save As on every editor a hand arrives from,
    // and the rows below now say so. The split is the same one the kernel makes
    // between `Ctrl+K` and `Ctrl+Shift+K`, and it is declared rather than left
    // to rank order: a [`Required`] `Shift` does outrank an [`Any`] one inside
    // a layer, but a binding that only works because of the ordering is one an
    // edit somewhere else can quietly break.
    (
        StrokePattern::new(KeyCode::Char('s'), CTRL_NO_SHIFT),
        FILE_SAVE,
    ),
    (
        StrokePattern::new(KeyCode::Char('s'), META_NO_SHIFT),
        FILE_SAVE,
    ),
    (
        StrokePattern::new(KeyCode::Char('s'), CTRL_SHIFT),
        FILE_SAVE_AS,
    ),
    (
        StrokePattern::new(KeyCode::Char('s'), META_SHIFT),
        FILE_SAVE_AS,
    ),
    (
        StrokePattern::new(KeyCode::Char('s'), CTRL_ALT),
        FILE_SAVE_FORCE,
    ),
    (
        StrokePattern::new(KeyCode::Char('s'), META_ALT),
        FILE_SAVE_FORCE,
    ),
    // `⌘N` / `Ctrl+N`, `Shift` forbidden — leaving `⌘⇧N` free for a new
    // *window* rather than silently claiming it, which is the reasoning `⌘W`
    // already follows for `⌘⇧W`. Nothing in any layer bound `n` before this:
    // `Char('n')` over the kernel's default keymap and every source file of
    // both native faces returned nothing but tests answering `n` to a
    // yes-or-no prompt, checked 12 Aug 2026 immediately before these two rows.
    (
        StrokePattern::new(KeyCode::Char('n'), CTRL_NO_SHIFT),
        FILE_NEW,
    ),
    (
        StrokePattern::new(KeyCode::Char('n'), META_NO_SHIFT),
        FILE_NEW,
    ),
    (StrokePattern::new(KeyCode::Char('c'), META), CLIPBOARD_COPY),
    (StrokePattern::new(KeyCode::Char('x'), META), CLIPBOARD_CUT),
    (
        StrokePattern::new(KeyCode::Char('v'), META),
        CLIPBOARD_PASTE,
    ),
    (
        StrokePattern::new(KeyCode::Char('z'), META_NO_SHIFT),
        HISTORY_UNDO,
    ),
    (
        StrokePattern::new(KeyCode::Char('z'), META_SHIFT),
        HISTORY_REDO,
    ),
    (
        StrokePattern::new(KeyCode::Char('a'), META),
        SELECTION_SELECT_ALL,
    ),
    (StrokePattern::new(KeyCode::Char('f'), META), SEARCH_OPEN),
    // `Shift` forbidden for the reason the default keymap's `Ctrl+K` forbids
    // it: the shifted spelling must stay free to mean something else.
    (
        StrokePattern::new(KeyCode::Char('k'), META_NO_SHIFT),
        PALETTE_OPEN,
    ),
    (
        StrokePattern::new(KeyCode::Char('h'), META_ALT),
        HISTORY_TOGGLE_PANEL,
    ),
    // `⌘⌥T`, the ⌘ spelling of the kernel's `Ctrl+Alt+T`. Same id, so the
    // palette shows one row and a `[keys]` line names one thing, whichever
    // hand the user reaches with.
    (
        StrokePattern::new(KeyCode::Char('t'), META_ALT),
        VIEW_TOGGLE_THEME,
    ),
    // `⌘⌥R`, the ⌘ spelling of the kernel's `Ctrl+Alt+R`. Same id, for the same
    // reason `⌘⌥T` is: one palette row, and one thing for a `[keys]` line to
    // name, whichever hand the user reaches with.
    (
        StrokePattern::new(KeyCode::Char('r'), META_ALT),
        CONFIG_RELOAD,
    ),
    // ----- The rows the enumerating test found, 9 Aug 2026 -----
    //
    // ⚠️ **Every one of these was missing while the table above looked
    // complete.** `⌘⌥E` is the one that was noticed, and it was noticed by Tom
    // pressing it — the panel he most wanted had no ⌘ chord at all while the
    // three panels beside it did. The rest came out of
    // `every_ctrl_chord_a_mac_hand_reaches_for_has_a_meta_spelling`, which
    // enumerates rather than reads, and which is the actual fix here: the rows
    // are what was missing, the test is what stops the next one going missing.
    //
    // The spellings are not invented. Each is what VS Code and Zed bind on
    // macOS, so a hand that has used either arrives knowing them.
    (
        StrokePattern::new(KeyCode::Char('e'), META_ALT),
        EXPLORER_TOGGLE_PANEL,
    ),
    // `⌘B` for the sidebar, **without `⌥`** — the one chord here that is not
    // the ⌘ spelling of a `Ctrl+Alt` kernel binding, because it is a chord a
    // mac hand already has: VS Code and Zed both bind the sidebar to plain
    // `⌘B`. The kernel keeps `Ctrl+Alt+B` for the faces that have no ⌘.
    (
        StrokePattern::new(KeyCode::Char('b'), META),
        EXPLORER_TOGGLE_PLACEMENT,
    ),
    // `⌘/`, the mac spelling of `Ctrl+/`. `Shift` ignored, matching the
    // kernel's `CTRL_ANY_SHIFT`: on several layouts `/` is a shifted key.
    (
        StrokePattern::new(KeyCode::Char('/'), META),
        COMMENT_TOGGLE_LINE,
    ),
    // The undo-tree branch pair, keeping the kernel's reasoning: `⌥` reads as
    // "the sideways version of", so these sit where `⌘Z` and `⌘⇧Z` already are
    // in the hand. `META_NO_SHIFT` forbids `Alt` on `⌘Z`, so nothing collides.
    (
        StrokePattern::new(KeyCode::Char('z'), META_ALT),
        HISTORY_PREVIOUS_BRANCH,
    ),
    (
        StrokePattern::new(KeyCode::Char('y'), META_ALT),
        HISTORY_NEXT_BRANCH,
    ),
    // `⌘⇧K` deletes a line and `⌘K` opens the palette — the same split the
    // kernel makes between `Ctrl+Shift+K` and `Ctrl+K`, and it works here for
    // the same reason: the palette row forbids `Shift`.
    (
        StrokePattern::new(KeyCode::Char('k'), META_SHIFT),
        LINES_DELETE,
    ),
    (
        StrokePattern::new(KeyCode::Char('j'), META_NO_SHIFT),
        LINES_JOIN,
    ),
    (
        StrokePattern::new(KeyCode::Char('d'), META),
        MULTI_CURSOR_ADD_SELECTION_TO_NEXT_MATCH,
    ),
    (
        StrokePattern::new(KeyCode::Char('u'), META_NO_SHIFT),
        MULTI_CURSOR_REMOVE_LAST_CURSOR,
    ),
    (
        StrokePattern::new(KeyCode::Char('l'), META_SHIFT),
        MULTI_CURSOR_SELECT_ALL_OCCURRENCES,
    ),
    // The tab strip. The kernel's own comment says `Ctrl+Shift+]` exists
    // *because* the web face receives it from macOS's `⌘⇧]`; on this face the
    // ⌘ chord arrives as itself and needs a row of its own.
    (
        StrokePattern::new(KeyCode::Char(']'), META_SHIFT),
        WORKSPACE_NEXT_TAB,
    ),
    (
        StrokePattern::new(KeyCode::Char('['), META_SHIFT),
        WORKSPACE_PREVIOUS_TAB,
    ),
    // `⌘W`, `Shift` forbidden — leaving `⌘⇧W` free for a close-window rather
    // than silently claiming it, which is the kernel's reasoning for
    // `Ctrl+W`, unchanged.
    (
        StrokePattern::new(KeyCode::Char('w'), META_NO_SHIFT),
        WORKSPACE_CLOSE_TAB,
    ),
    // ----- The two ways in that have keys, added 12 Aug 2026 -----
    //
    // `⌘O` opens a file, `⌘⇧O` opens a folder as the project. Both are what
    // VS Code and Zed bind on macOS, so a hand arriving from either already
    // knows them. `project.set` gets no chord and is on the palette-only
    // list; [`ids`](super::ids) carries the reasoning for that.
    //
    // Two spellings each, per `FILE_SAVE`'s precedent at the top of this
    // table: one id, a `Ctrl` row and a `⌘` row.
    //
    // The unshifted rows forbid `Shift` rather than ignoring it, following
    // `⌘K` and `⌘⇧K` above — one letter carrying two verbs, split the way the
    // kernel splits `Ctrl+K` from `Ctrl+Shift+K`.
    //
    // ⚠️ **This is explicitness, not necessity, and the difference was
    // measured rather than assumed.** Spelling `Shift` as [`Any`] on the file
    // rows was tried on 12 Aug 2026 and `⌘⇧O` still reached `project.open`:
    // the folder rows declare `Shift` [`Required`], which outranks a loose
    // pattern inside a layer — the same mechanism `MAC_CHORDS` below leans on
    // deliberately. So the forbid is not what makes these four rows work.
    // What it does is stop them depending on rank order to work, which is a
    // thing to state plainly rather than a hazard to claim.
    //
    // Nothing else binds this key in any layer: `Char('o')` and `Char('O')`
    // over the kernel's default keymap and every source file of both native
    // faces returns nothing, re-checked 12 Aug 2026 immediately before these
    // rows were written.
    (
        StrokePattern::new(KeyCode::Char('o'), CTRL_NO_SHIFT),
        FILE_OPEN,
    ),
    (
        StrokePattern::new(KeyCode::Char('o'), META_NO_SHIFT),
        FILE_OPEN,
    ),
    (
        StrokePattern::new(KeyCode::Char('o'), CTRL_SHIFT),
        PROJECT_OPEN,
    ),
    (
        StrokePattern::new(KeyCode::Char('o'), META_SHIFT),
        PROJECT_OPEN,
    ),
];

/// The macOS chord table: `(stroke, command)`, for the arrows and the delete
/// keys.
///
/// Kept apart from [`BINDINGS`] because these rows and only these rows
/// deliberately override the default keymap. Every pattern here declares `Alt`
/// or `Meta` [`Required`], which is both what makes the chord distinguishable
/// from the unmodified key and what outranks the default's loose patterns
/// within a layer; the rows in [`BINDINGS`] override nothing, and the test
/// below holds them to it.
pub(super) const MAC_CHORDS: &[(StrokePattern, CommandId)] = &[
    (StrokePattern::new(KeyCode::Left, ALT_NAV), CURSOR_WORD_LEFT),
    (
        StrokePattern::new(KeyCode::Right, ALT_NAV),
        CURSOR_WORD_RIGHT,
    ),
    // ⌥⇧← / ⌥⇧→ were the one contested pair, and the owner has ruled: they go
    // to word-select, the chord every other mac editor puts it on. The syntax
    // verbs that held them are not dropped — they move to ⌃⇧⌘ below. This is
    // why `ALT_NAV` forbids `Shift` rather than ignoring it: the two ⌥ rows
    // above must not swallow the selecting variant.
    (
        StrokePattern::new(KeyCode::Left, ALT_SHIFT_NAV),
        CURSOR_WORD_LEFT_SELECT,
    ),
    (
        StrokePattern::new(KeyCode::Right, ALT_SHIFT_NAV),
        CURSOR_WORD_RIGHT_SELECT,
    ),
    // The displaced syntax verbs, rehoused rather than unbound. Left shrinks
    // and Right expands, keeping the direction sense the default keymap gave
    // them on ⌥⇧.
    (
        StrokePattern::new(KeyCode::Left, CTRL_SHIFT_META_NAV),
        AST_SHRINK_SELECTION,
    ),
    (
        StrokePattern::new(KeyCode::Right, CTRL_SHIFT_META_NAV),
        AST_EXPAND_SELECTION,
    ),
    (
        StrokePattern::new(KeyCode::Left, META_NAV),
        CURSOR_LINE_START,
    ),
    (
        StrokePattern::new(KeyCode::Right, META_NAV),
        CURSOR_LINE_END,
    ),
    (
        StrokePattern::new(KeyCode::Left, META_SHIFT_NAV),
        CURSOR_LINE_START_SELECT,
    ),
    (
        StrokePattern::new(KeyCode::Right, META_SHIFT_NAV),
        CURSOR_LINE_END_SELECT,
    ),
    (
        StrokePattern::new(KeyCode::Up, META_NAV),
        CURSOR_DOCUMENT_START,
    ),
    (
        StrokePattern::new(KeyCode::Down, META_NAV),
        CURSOR_DOCUMENT_END,
    ),
    (
        StrokePattern::new(KeyCode::Up, META_SHIFT_NAV),
        CURSOR_DOCUMENT_START_SELECT,
    ),
    (
        StrokePattern::new(KeyCode::Down, META_SHIFT_NAV),
        CURSOR_DOCUMENT_END_SELECT,
    ),
    (
        StrokePattern::new(KeyCode::Backspace, ALT_DELETE),
        EDIT_DELETE_WORD_BACKWARD,
    ),
    (
        StrokePattern::new(KeyCode::Delete, ALT_DELETE),
        EDIT_DELETE_WORD_FORWARD,
    ),
    (
        StrokePattern::new(KeyCode::Backspace, META_DELETE),
        EDIT_DELETE_TO_LINE_START,
    ),
    (
        StrokePattern::new(KeyCode::Delete, META_DELETE),
        EDIT_DELETE_TO_LINE_END,
    ),
    // `⌘⌥↑` / `⌘⌥↓`, the mac spelling of the kernel's `Ctrl+Alt`+arrow. These
    // belong in *this* table rather than beside the letter rows above: the
    // default keymap's `Up` and `Down` patterns read only `Shift`, so a chord
    // on those keys overrides plain caret motion by construction — which is
    // exactly what this table is for.
    (
        StrokePattern::new(KeyCode::Up, META_ALT_NAV),
        MULTI_CURSOR_ADD_CURSOR_ABOVE,
    ),
    (
        StrokePattern::new(KeyCode::Down, META_ALT_NAV),
        MULTI_CURSOR_ADD_CURSOR_BELOW,
    ),
];

/// The number of bindings this face adds.
///
/// Derived from the two tables so it cannot drift from either.
pub const BINDING_COUNT: usize = BINDINGS.len() + MAC_CHORDS.len();

/// Builds the keymap layer this face pushes onto the kernel's default.
///
/// A layer rather than a replacement: everything the default keymap binds
/// keeps working, and a user keymap pushed later still overrides both.
#[must_use]
pub fn keymap() -> Keymap {
    let mut keymap = Keymap::new("iridium-desktop");
    for (stroke, command) in BINDINGS.iter().chain(MAC_CHORDS) {
        keymap.push(KeyBinding::new(*stroke, CHORD, command.clone()));
    }
    keymap
}
