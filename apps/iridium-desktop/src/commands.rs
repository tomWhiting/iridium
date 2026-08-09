//! The commands this face adds, and the keys they answer to.
//!
//! # These are host commands, not a second dispatch
//!
//! Every verb here is a real entry in the kernel's `CommandRegistry`, bound in
//! a [`Keymap`] layer pushed on top of the default one — the identical
//! mechanism the terminal face uses, described at length in its
//! `app::commands`. A key resolves through the kernel's resolver like any
//! other, the kernel reports it as
//! [`EditorKeyResult::HostCommand`](iridium_editor::EditorKeyResult::HostCommand),
//! and [`DesktopApp`](crate::app::DesktopApp) runs it.
//!
//! The two ids this face contributes are **the terminal face's ids,
//! character for character**: `file.save` and `file.saveForce`. One id and
//! one meaning across every face is the whole argument of the kernel's
//! `builtin::host` module, and two faces spelling the same verb differently
//! is exactly the drift it warns about.
//!
//! # The ⌘ layer
//!
//! The kernel's default keymap binds only `Ctrl` chords — a terminal cannot
//! reliably see ⌘ — and every one of its patterns forbids `meta`, so a bare
//! ⌘C on this face would fall through dead. This layer is where the desktop
//! plan's "⌘C/⌘V/⌘Z work natively" line is honored: the ⌘ rows below bind
//! **kernel-implemented** commands (clipboard, undo, redo, select-all,
//! search) and **kernel-named host commands** (the palette, the undo-tree
//! panel) to their macOS chords. Nothing is reimplemented — the rows route
//! the mac chord to the verb the kernel already resolves for the `Ctrl`
//! spelling, so the two spellings cannot disagree.
//!
//! | Key | Command | Note |
//! |---|---|---|
//! | `Ctrl+S` / `⌘S` | [`FILE_SAVE`] | refuses a file that changed on disk |
//! | `Ctrl+Alt+S` / `⌘⌥S` | [`FILE_SAVE_FORCE`] | saves anyway |
//! | `⌘C` | `clipboard.copy` | kernel verb, mac chord |
//! | `⌘X` | `clipboard.cut` | kernel verb, mac chord |
//! | `⌘V` | `clipboard.paste` | kernel verb, mac chord |
//! | `⌘Z` | `history.undo` | kernel verb, mac chord |
//! | `⌘⇧Z` | `history.redo` | kernel verb, mac chord |
//! | `⌘A` | `selection.selectAll` | kernel verb, mac chord |
//! | `⌘F` | `search.open` | kernel verb, mac chord |
//! | `⌘K` | `palette.open` | kernel-named host command, mac chord |
//! | `⌘⌥H` | `history.togglePanel` | kernel-named host command, mac chord |
//! | `⌘⌥T` | `view.toggleTheme` | kernel-named host command, mac chord |
//! | `⌘⌥R` | `config.reload` | kernel-named host command, mac chord |
//! | `⌘⌥E` | `explorer.togglePanel` | kernel-named host command, mac chord |
//! | `⌘/` | `comment.toggleLine` | kernel verb, mac chord |
//! | `⌘⌥Z` / `⌘⌥Y` | `history.previousBranch` / `nextBranch` | kernel verbs |
//! | `⌘⇧K` | `lines.delete` | kernel verb, mac chord |
//! | `⌘J` | `lines.join` | kernel verb, mac chord |
//! | `⌘D` | `multiCursor.addSelectionToNextMatch` | kernel verb, mac chord |
//! | `⌘U` | `multiCursor.removeLastCursor` | kernel verb, mac chord |
//! | `⌘⇧L` | `multiCursor.selectAllOccurrences` | kernel verb, mac chord |
//! | `⌘⇧]` / `⌘⇧[` | `workspace.nextTab` / `previousTab` | workspace verbs |
//! | `⌘W` | `workspace.closeTab` | workspace verb, mac chord |
//!
//! The `Ctrl` spellings stay bound by the default keymap underneath this
//! layer — `Ctrl+F`, `Ctrl+K`, `Ctrl+P` and `Ctrl+Alt+H` included; both
//! spellings work.
//!
//! # ⚠️ The bottom half of that table was missing, and a person found it
//!
//! Every row from `⌘⌥E` down landed on **9 Aug 2026**, after Tom pressed
//! `⌘⌥E` — documented as working — and nothing happened. The file explorer,
//! the panel he most wanted, had no ⌘ chord at all while the three panels
//! beside it did, and a page asserting otherwise had been written by reading
//! this table by eye.
//!
//! So the fix is not the rows. It is
//! `every_ctrl_chord_a_mac_hand_reaches_for_has_a_meta_spelling`, which
//! **enumerates** every verb the kernel binds under `Ctrl` and fails unless
//! some key reaches it without `Ctrl` held. Note the predicate: not "has a ⌘
//! chord". Word motion is `⌥←` on macOS and that is right; `⌃⌥E` holds `Alt`
//! and is still wrong. What a mac hand objects to is `Ctrl`, so `Ctrl` is what
//! is asked about. A verb that genuinely should stay `Ctrl`-only goes on
//! `CTRL_ONLY` with its reason, and that list is empty.
//!
//! # The ⌥ and ⌘ chords on the arrows and the delete keys
//!
//! The kernel's default keymap reads only `Ctrl` and `Shift` on the arrows and
//! only `Ctrl` on `Backspace`/`Delete`, declaring `Alt` and `Meta`
//! `Any` — a terminal cannot see either, so the kernel neither reads nor
//! forbids them. On a window it can: winit reports `⌥←` as `Left` with `alt`
//! held and `⌘←` as `Left` with `meta` held. Without the rows in
//! `MAC_CHORDS` those chords match the loose default patterns and arrive as
//! plain character motion and a plain backspace — the whole macOS chord set
//! silently doing the unmodified thing.
//!
//! | Key | Command |
//! |---|---|
//! | `⌥←` / `⌥→` | `cursor.wordLeft` / `cursor.wordRight` |
//! | `⌥⇧←` / `⌥⇧→` | `cursor.wordLeftSelect` / `cursor.wordRightSelect` |
//! | `⌃⇧⌘←` / `⌃⇧⌘→` | `ast.shrinkSelection` / `ast.expandSelection` |
//! | `⌘←` / `⌘→` | `cursor.lineStart` / `cursor.lineEnd` |
//! | `⌘⇧←` / `⌘⇧→` | `cursor.lineStartSelect` / `cursor.lineEndSelect` |
//! | `⌘↑` / `⌘↓` | `cursor.documentStart` / `cursor.documentEnd` |
//! | `⌘⇧↑` / `⌘⇧↓` | `cursor.documentStartSelect` / `cursor.documentEndSelect` |
//! | `⌥⌫` / `⌥⌦` | `edit.deleteWordBackward` / `edit.deleteWordForward` |
//! | `⌘⌫` / `⌘⌦` | `edit.deleteToLineStart` / `edit.deleteToLineEnd` |
//!
//! `edit.deleteToLineStart` and `edit.deleteToLineEnd` are the two verbs the
//! kernel implements and no keymap bound: they have no `Ctrl` spelling to
//! inherit, and `⌘⌫`/`⌘⌦` is where a mac hand looks for them.
//!
//! Every row here overrides *something* — that is what a `Required` mac
//! spelling inside a loose pattern does, and a row that overrode nothing would
//! be a mistake, which is why a test asserts each one does. Nearly always what
//! it overrides is a fall-through the displaced verb does not need: `⌥←`
//! outranks the loose pattern behind `cursor.left`, and `cursor.left` still
//! answers to a bare `←`.
//!
//! **The `⌥⇧←`/`⌥⇧→` row is the one that takes a chord away from a verb's only
//! home.** Those chords reached
//! `ast.shrinkSelection`/`ast.expandSelection` through the kernel's loose
//! patterns, and word-by-word selection had nowhere a mac hand would look for
//! it. The verbs displaced are rehoused on `⌃⇧⌘`, VS Code's mac spelling,
//! rather than left unbound — a rebinding that silently deletes a feature is
//! a worse bug than the one it fixes.
//!
//! That last sentence is a promise, so it is machine-checked rather than left
//! to whoever adds the next row:
//! `no_verb_the_kernel_could_reach_is_stranded_by_this_layer` compares
//! *reachability* — `KeyHintIndex` over the defaults against the same over the
//! session stack — for every command the default keymap binds. A stroke-level
//! overlap check cannot tell a harmless override from a stranding; this can.

use iridium_editor::commands::builtin::{
    AST_EXPAND_SELECTION, AST_SHRINK_SELECTION, CLIPBOARD_COPY, CLIPBOARD_CUT, CLIPBOARD_PASTE,
    COMMENT_TOGGLE_LINE, CONFIG_RELOAD, CURSOR_DOCUMENT_END, CURSOR_DOCUMENT_END_SELECT,
    CURSOR_DOCUMENT_START, CURSOR_DOCUMENT_START_SELECT, CURSOR_LINE_END, CURSOR_LINE_END_SELECT,
    CURSOR_LINE_START, CURSOR_LINE_START_SELECT, CURSOR_WORD_LEFT, CURSOR_WORD_LEFT_SELECT,
    CURSOR_WORD_RIGHT, CURSOR_WORD_RIGHT_SELECT, EDIT_DELETE_TO_LINE_END,
    EDIT_DELETE_TO_LINE_START, EDIT_DELETE_WORD_BACKWARD, EDIT_DELETE_WORD_FORWARD,
    EXPLORER_TOGGLE_PANEL, HISTORY_NEXT_BRANCH, HISTORY_PREVIOUS_BRANCH, HISTORY_REDO,
    HISTORY_TOGGLE_PANEL, HISTORY_UNDO, LINES_DELETE, LINES_JOIN, MULTI_CURSOR_ADD_CURSOR_ABOVE,
    MULTI_CURSOR_ADD_CURSOR_BELOW, MULTI_CURSOR_ADD_SELECTION_TO_NEXT_MATCH,
    MULTI_CURSOR_REMOVE_LAST_CURSOR, MULTI_CURSOR_SELECT_ALL_OCCURRENCES, PALETTE_OPEN,
    SEARCH_OPEN, SELECTION_SELECT_ALL, VIEW_TOGGLE_THEME, WORKSPACE_CLOSE_TAB, WORKSPACE_NEXT_TAB,
    WORKSPACE_PREVIOUS_TAB,
};
use iridium_editor::{
    CommandCategory, CommandId, CommandMeta, KeyBinding, KeyCode, Keymap, ModifierPattern,
    ModifierState, StrokePattern,
};

use ModifierState::{Any, Forbidden, Required};

/// Write the document to its file.
pub const FILE_SAVE: CommandId = CommandId::from_static("file.save");
/// Write the document to its file even though the file changed on disk.
pub const FILE_SAVE_FORCE: CommandId = CommandId::from_static("file.saveForce");
/// Open a tab listing every command by the id a configuration file names it by.
pub const COMMANDS_LIST: CommandId = CommandId::from_static("commands.list");
/// Open the user's `config.toml` as an ordinary, savable tab.
pub const CONFIG_EDIT: CommandId = CommandId::from_static("config.edit");

/// Every command this face contributes, in declaration order.
pub static COMMANDS: &[CommandMeta] = &[
    CommandMeta::described(
        FILE_SAVE,
        "Save",
        "Writes the document to its file, refusing if the file changed on disk.",
        CommandCategory::from_static("File"),
    )
    .with_aliases(&["write", "w"]),
    CommandMeta::described(
        FILE_SAVE_FORCE,
        "Save Anyway",
        "Writes the document to its file even though the file changed on disk.",
        CommandCategory::from_static("File"),
    )
    .with_aliases(&["overwrite", "force write", "w!"]),
    // Deliberately unbound. It is read once while writing a configuration
    // file and then not again for months, so it is worth a palette entry and
    // not worth a chord — and every chord spent here is one a user cannot
    // have.
    CommandMeta::described(
        COMMANDS_LIST,
        "List Every Command",
        "Opens a tab listing every command by the id to write in config.toml, with its key.",
        CommandCategory::from_static("Help"),
    )
    .with_aliases(&["keybindings", "shortcuts", "command ids", "config"]),
    // Deliberately unbound, and on the palette-only list with `commands.list`
    // for the same reason: it is the door into a file edited once and then not
    // for months. The point of it is that "where do I change my keys" has an
    // answer you can *search for* — Tom asked that question on 9 Aug 2026 and
    // the honest answer at the time was a path he had to be told.
    CommandMeta::described(
        CONFIG_EDIT,
        "Edit Configuration",
        "Opens ~/.config/iridium/config.toml in a tab, writing the default file if none exists.",
        CommandCategory::from_static("Help"),
    )
    .with_aliases(&[
        "settings",
        "preferences",
        "keybindings",
        "keymap",
        "config.toml",
    ]),
];

/// The number of commands this face contributes.
///
/// Derived from [`COMMANDS`] so it cannot drift from the table.
pub const COMMAND_COUNT: usize = COMMANDS.len();

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

/// A bare `Ctrl` chord: `Shift` ignored, `Alt` and `Meta` absent — the same
/// shape the terminal face binds its save under, so a keymap reads the same
/// across both.
const CTRL: ModifierPattern = pattern(Any, Required, Forbidden, Forbidden, Any);

/// A `Ctrl+Alt` chord. `AltGraph` is forbidden for the reason the kernel's
/// add-cursor chord forbids it: on many layouts `AltGr` is reported as
/// `Ctrl+Alt` while composing a character, and a forced save is not something
/// to do by accident while typing `@`.
const CTRL_ALT: ModifierPattern = pattern(Any, Required, Required, Forbidden, Forbidden);

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
const BINDINGS: &[(StrokePattern, CommandId)] = &[
    (StrokePattern::new(KeyCode::Char('s'), CTRL), FILE_SAVE),
    (StrokePattern::new(KeyCode::Char('s'), META), FILE_SAVE),
    (
        StrokePattern::new(KeyCode::Char('s'), CTRL_ALT),
        FILE_SAVE_FORCE,
    ),
    (
        StrokePattern::new(KeyCode::Char('s'), META_ALT),
        FILE_SAVE_FORCE,
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
const MAC_CHORDS: &[(StrokePattern, CommandId)] = &[
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

/// Every command this face contributes, cloned for registration.
#[must_use]
pub fn command_metas() -> Vec<CommandMeta> {
    COMMANDS.to_vec()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use iridium_editor::commands::builtin::{
        AST_EXPAND_SELECTION, AST_SHRINK_SELECTION, CURSOR_WORD_LEFT_SELECT,
        CURSOR_WORD_RIGHT_SELECT,
    };
    use iridium_editor::commands::{default_keymap_stack, default_non_modal_keymap};
    use iridium_editor::{Editor, KeyHintIndex, KeyPress, KeymapResolver, KeymapStack, Modifiers};

    use super::*;

    /// The whole stack a session resolves against: the kernel's default layer
    /// with this face's layer pushed on top, exactly as `DesktopApp::new`
    /// builds it. A chord is only bound if it resolves *here* — a row in the
    /// table proves nothing on its own, because the layer underneath binds the
    /// same keys under looser patterns.
    fn session_stack() -> KeymapStack {
        let mut stack = default_keymap_stack();
        stack.push(keymap());
        stack
    }

    /// Modifiers with only the named bits held.
    const fn held(shift: bool, alt: bool, meta: bool) -> Modifiers {
        Modifiers {
            shift,
            ctrl: false,
            alt,
            meta,
            alt_graph: false,
        }
    }

    /// The `⌃⇧⌘` chord's modifiers — the one combination [`held`] cannot
    /// spell, named rather than given a fourth `bool` parameter.
    const fn ctrl_shift_meta() -> Modifiers {
        Modifiers {
            shift: true,
            ctrl: true,
            alt: false,
            meta: true,
            alt_graph: false,
        }
    }

    /// Asserts one keypress resolves to `expected` through the full stack.
    fn resolves_to(stack: &KeymapStack, key: KeyCode, modifiers: Modifiers, expected: &CommandId) {
        let mut resolver = KeymapResolver::new();
        let outcome = resolver.resolve(stack, KeyPress::new(key, modifiers));
        assert_eq!(
            outcome.command().map(CommandId::as_str),
            Some(expected.as_str()),
            "{key:?} with {modifiers:?} did not resolve to {expected}"
        );
    }

    #[test]
    fn the_mac_navigation_chords_resolve_through_the_whole_stack() {
        let stack = session_stack();
        let cases: &[(KeyCode, Modifiers, &CommandId)] = &[
            (KeyCode::Left, held(false, true, false), &CURSOR_WORD_LEFT),
            (KeyCode::Right, held(false, true, false), &CURSOR_WORD_RIGHT),
            (KeyCode::Left, held(false, false, true), &CURSOR_LINE_START),
            (KeyCode::Right, held(false, false, true), &CURSOR_LINE_END),
            (
                KeyCode::Left,
                held(true, false, true),
                &CURSOR_LINE_START_SELECT,
            ),
            (
                KeyCode::Right,
                held(true, false, true),
                &CURSOR_LINE_END_SELECT,
            ),
            (
                KeyCode::Up,
                held(false, false, true),
                &CURSOR_DOCUMENT_START,
            ),
            (
                KeyCode::Down,
                held(false, false, true),
                &CURSOR_DOCUMENT_END,
            ),
            (
                KeyCode::Up,
                held(true, false, true),
                &CURSOR_DOCUMENT_START_SELECT,
            ),
            (
                KeyCode::Down,
                held(true, false, true),
                &CURSOR_DOCUMENT_END_SELECT,
            ),
        ];
        for &(key, modifiers, expected) in cases {
            resolves_to(&stack, key, modifiers, expected);
        }
    }

    #[test]
    fn the_mac_deletion_chords_resolve_through_the_whole_stack() {
        let stack = session_stack();
        let cases: &[(KeyCode, Modifiers, &CommandId)] = &[
            (
                KeyCode::Backspace,
                held(false, true, false),
                &EDIT_DELETE_WORD_BACKWARD,
            ),
            (
                KeyCode::Delete,
                held(false, true, false),
                &EDIT_DELETE_WORD_FORWARD,
            ),
            (
                KeyCode::Backspace,
                held(false, false, true),
                &EDIT_DELETE_TO_LINE_START,
            ),
            (
                KeyCode::Delete,
                held(false, false, true),
                &EDIT_DELETE_TO_LINE_END,
            ),
        ];
        for &(key, modifiers, expected) in cases {
            resolves_to(&stack, key, modifiers, expected);
        }
    }

    #[test]
    fn the_word_select_chords_belong_to_word_select() {
        // The contested pair, ruled. ⌥⇧← / ⌥⇧→ were left to the default
        // keymap's syntax verbs while the collision was unresolved; Tom asked
        // for word-by-word selection on the chord every other mac editor puts
        // it on, so this face takes it back. The predecessor of this test
        // pinned the opposite resolution under the name
        // `the_word_select_chords_are_left_to_the_syntax_verbs`, and the
        // rename is the record that a deliberate decision replaced a
        // deliberate decision rather than drifting into one.
        let stack = session_stack();
        resolves_to(
            &stack,
            KeyCode::Left,
            held(true, true, false),
            &CURSOR_WORD_LEFT_SELECT,
        );
        resolves_to(
            &stack,
            KeyCode::Right,
            held(true, true, false),
            &CURSOR_WORD_RIGHT_SELECT,
        );
    }

    #[test]
    fn the_syntax_verbs_keep_a_home_of_their_own() {
        // Expand/shrink did not lose the chord, it moved: ⌃⇧⌘← / ⌃⇧⌘→, where
        // VS Code puts them on mac. A verb displaced by a rebinding and left
        // unbound would be the rebinding quietly deleting a feature.
        let stack = session_stack();
        resolves_to(
            &stack,
            KeyCode::Left,
            ctrl_shift_meta(),
            &AST_SHRINK_SELECTION,
        );
        resolves_to(
            &stack,
            KeyCode::Right,
            ctrl_shift_meta(),
            &AST_EXPAND_SELECTION,
        );
    }

    /// Verbs the kernel binds under `Ctrl` that deliberately have no
    /// Ctrl-free chord on this face.
    ///
    /// A list with a reason against each, so "Ctrl only" is a decision written
    /// down rather than a row nobody got to. Empty is the goal.
    const CTRL_ONLY: &[(&str, &str)] = &[];

    /// Whether any key that reaches `id` needs no `Ctrl` held.
    ///
    /// ⚠️ **Not "has a ⌘ chord", and the difference is the whole test.** The
    /// mac spelling of a `Ctrl` chord is sometimes ⌘ and sometimes ⌥ — word
    /// motion is `⌥←` on every mac editor, not `⌘←`, and `⌥⌫` deletes a word
    /// — so demanding ⌘ specifically would fail on four verbs this face binds
    /// correctly. And demanding "⌘ *or* ⌥" would *pass* `⌃⌥E`, which holds
    /// `Alt` and is exactly the chord that sent Tom looking for a ⌘ one.
    ///
    /// What a mac hand actually objects to is `Ctrl`. So that is what is
    /// asked.
    fn reachable_without_ctrl(hints: &KeyHintIndex, id: &str) -> bool {
        hints.hints_for(id).iter().any(|hint| {
            hint.sequence()
                .first()
                .is_some_and(|stroke| !stroke.modifiers.required_modifiers().ctrl)
        })
    }

    #[test]
    fn every_ctrl_chord_a_mac_hand_reaches_for_has_a_meta_spelling() {
        // ⚠️ **This test exists because reading the table by eye failed.**
        // The walkthrough written on 9 Aug 2026 asserted `⌘⌥E` opened the file
        // explorer. It did not — the kernel binds `Ctrl+Alt+E` and this face's
        // table had `⌘⌥` rows for H, T and R and no E — and Tom found that by
        // pressing the key, which is the worst possible place to find it.
        //
        // The question is asked **per verb, not per stroke**, and that
        // distinction is the whole reason this passes for the arrows.
        // `cursor.documentStart` is bound to `Ctrl+Home`, and there is no
        // `⌘Home` row; a stroke-level check would demand one. But a mac hand
        // does not reach for `⌘Home`, it reaches for `⌘↑`, and that IS bound —
        // so the verb has a ⌘ spelling and the stroke never needs one.
        //
        // Scoped to the non-modal defaults, which is the surface this face's
        // layer can reach, and to bindings that *require* `Ctrl`: a plain
        // arrow needs no mac spelling because it already is one.
        let hints = KeyHintIndex::build(&session_stack());
        let excused: BTreeSet<&str> = CTRL_ONLY.iter().map(|(id, _)| *id).collect();

        let mut stranded: Vec<String> = Vec::new();
        for binding in default_non_modal_keymap().bindings() {
            let Some(command) = binding.command() else {
                continue;
            };
            let requires_ctrl = binding
                .sequence()
                .first()
                .is_some_and(|stroke| stroke.modifiers.required_modifiers().ctrl);
            if !requires_ctrl || excused.contains(command.as_str()) {
                continue;
            }
            if !reachable_without_ctrl(&hints, command.as_str()) {
                stranded.push(command.to_string());
            }
        }

        stranded.sort_unstable();
        stranded.dedup();
        assert!(
            stranded.is_empty(),
            "these verbs answer only to a chord holding Ctrl, on a face whose users \
             reach for ⌘ and ⌥: {stranded:?} — add the row, or put the id on \
             CTRL_ONLY with the reason"
        );
    }

    #[test]
    fn nothing_on_the_ctrl_only_list_has_a_meta_spelling_after_all() {
        // The same staleness guard `nothing_on_the_palette_only_list_is_also_bound`
        // applies to its list: an excuse that outlived its reason is worse than
        // no excuse, because the next verb added inherits it.
        let hints = KeyHintIndex::build(&session_stack());
        for (id, reason) in CTRL_ONLY {
            assert!(
                !reason.is_empty(),
                "{id} is excused from a Ctrl-free chord with no reason given"
            );
            assert!(
                !reachable_without_ctrl(&hints, id),
                "{id} has a Ctrl-free chord after all — take it off CTRL_ONLY"
            );
        }
    }

    #[test]
    fn no_verb_the_kernel_could_reach_is_stranded_by_this_layer() {
        // The general form of the test above it. That one names the two verbs
        // whose displacement someone noticed; this one asks the question of
        // every verb, so the next row added to `MAC_CHORDS` cannot strand one
        // quietly.
        //
        // The claim being checked is the module doc's: this face takes chords
        // *away* from the default keymap, and every verb so displaced keeps a
        // home. Most rows displace nothing that matters — `⌥←` outranks the
        // loose pattern behind `cursor.left`, which still answers to a bare
        // `←` — and the distinction between that and stranding a verb outright
        // is exactly what a stroke-level overlap check cannot see. So the
        // comparison is at the level of *reachability*, which is what
        // `KeyHintIndex` already computes: an empty hint list means there is
        // no key for this command, suppression, shadowing and rank loss all
        // accounted for.
        //
        // A command already unreachable under the defaults alone is skipped
        // rather than asserted about: this layer did not do that, and holding
        // it responsible would fail the moment the kernel gained a
        // palette-only verb.
        //
        // Scoped to the non-modal defaults, which is the surface this face's
        // layer can reach — every row it pushes is a `CHORD`-mode binding.
        let before = KeyHintIndex::build(&default_keymap_stack());
        let after = KeyHintIndex::build(&session_stack());

        for binding in default_non_modal_keymap().bindings() {
            let Some(command) = binding.command() else {
                continue;
            };
            let id = command.as_str();
            if before.hints_for(id).is_empty() {
                continue;
            }
            assert!(
                !after.hints_for(id).is_empty(),
                "{id} answered to a key under the default keymap and answers to none \
                 with this face's layer pushed — rehouse it, as the syntax verbs were, \
                 or say in the table that it is deliberately gone"
            );
        }
    }

    #[test]
    fn no_host_command_this_face_binds_carries_arguments() {
        // The face half of the kernel's
        // `no_host_command_is_bound_to_a_sequence_that_carries_arguments`.
        // Every verb this face contributes is a host command by construction —
        // the test above asserts the kernel implements none of them — and it
        // reaches `dispatch_host_command`, which takes an id and nothing else.
        //
        // `EditorKeyResult::HostCommand` carries `args` beside the id, and this
        // face drops them. Harmless while no binding declares a count prefix
        // or a capturing stroke; a silent wrong answer the moment one does,
        // since the browser face forwards both.
        //
        // ⚠️ This covers this face's own layer only. A **user keymap** can
        // carry a capturing stroke — `{char}` parses to
        // `StrokePattern::any_char` — and nothing checks that layer. It costs
        // nothing today because no host command anywhere reads its arguments,
        // so the character dropped here is one the browser hands to a host
        // that ignores it too.
        //
        // **When this fails, thread `args` through `run_host_command` rather
        // than relaxing it.**
        //
        // ⚠️ **This note used to say `implements_command` admits the five
        // `workspace.*` ids. It does not** — it is
        // `actions::action_for(id).is_some()`, the keyboard action table, and
        // no `workspace.*` id is in it. Proven on 9 Aug 2026 by binding `⌘⇧]`
        // to `workspace.nextTab`, which made
        // `every_borrowed_kernel_verb_is_still_a_kernel_verb` fail with
        // "the kernel neither implements nor names it".
        //
        // The consequence for *this* test is that a `workspace.*` id falls
        // through the `continue` above and gets asserted about, which is
        // correct and was correct before: those ids reach
        // `Workspace::run_command`, which takes an id and nothing else, so
        // they discard arguments exactly as host commands do.
        for binding in keymap().bindings() {
            let Some(command) = binding.command() else {
                continue;
            };
            if Editor::implements_command(command.as_str()) {
                continue;
            }
            assert!(
                !binding.accepts_count() && !binding.has_capture_stroke(),
                "{command} is a command the kernel does not implement, bound to \
                 a sequence that carries arguments, and this face discards them"
            );
        }
    }

    #[test]
    fn every_command_this_face_adds_is_one_the_kernel_does_not_implement() {
        // A face command that shadows a kernel one would be the exact mistake
        // this crate is written to avoid: two implementations of one id, and
        // whichever the dispatch reaches first wins.
        for meta in COMMANDS {
            assert!(
                !Editor::implements_command(meta.id().as_str()),
                "{} is already implemented by the kernel",
                meta.id()
            );
        }
    }

    #[test]
    fn every_borrowed_kernel_verb_is_still_a_kernel_verb() {
        // The ⌘ layer binds only verbs the kernel implements or ids the kernel
        // *names* for someone else to run; a row here for an id the kernel
        // dropped would be a mac chord that consumes the key and does nothing.
        //
        // **Three families, not two.** `host_command_metas` was the only one
        // this test knew about until `⌘⇧]` was bound to `workspace.nextTab` on
        // 9 Aug 2026 and it failed — the `workspace.*` ids are named by
        // `workspace_command_metas` and dispatched by `Workspace::run_command`,
        // which is a third destination and was always a legitimate one for this
        // face to bind. The test was narrower than the code it guarded.
        let ours: BTreeSet<&str> = COMMANDS.iter().map(|meta| meta.id().as_str()).collect();
        let named: BTreeSet<&str> = iridium_editor::commands::builtin::host_command_metas()
            .iter()
            .chain(iridium_editor::commands::builtin::workspace_command_metas())
            .map(|meta| meta.id().as_str())
            .collect();
        for (_, command) in BINDINGS.iter().chain(MAC_CHORDS) {
            if ours.contains(command.as_str()) {
                continue;
            }
            assert!(
                Editor::implements_command(command.as_str()) || named.contains(command.as_str()),
                "{command} is bound here but the kernel neither implements it, nor names \
                 it as a host command, nor names it as a workspace command"
            );
        }
    }

    #[test]
    fn no_two_commands_share_an_id() {
        let ids: BTreeSet<&str> = COMMANDS.iter().map(|meta| meta.id().as_str()).collect();
        assert_eq!(ids.len(), COMMAND_COUNT, "a command id is duplicated");
    }

    #[test]
    fn every_command_carries_a_title_and_a_description() {
        for meta in COMMANDS {
            assert!(!meta.title().is_empty(), "{} has no title", meta.id());
            assert!(
                meta.description().is_some_and(|text| !text.is_empty()),
                "{} has no description",
                meta.id()
            );
        }
    }

    /// Commands this face contributes and deliberately leaves unbound.
    ///
    /// A list rather than a relaxed assertion, so leaving a verb unbound is a
    /// decision written down with its reason next to it — and an *accidentally*
    /// unbound verb still fails, which is the case this test exists for.
    ///
    /// - `commands.list` opens a reference read once while writing a
    ///   configuration file and then not again for months. Every chord spent
    ///   is one the user cannot have, and the palette is exactly the right
    ///   surface for something wanted by name and rarely.
    /// - `config.edit` opens that same configuration file, at the same
    ///   cadence, and is found the same way. `⌘,` is the mac chord for it and
    ///   is deliberately **not** taken: this face has no preferences window,
    ///   and claiming the key that everyone's hand expects one from — to open
    ///   a text file instead — is a promise it cannot keep.
    const PALETTE_ONLY: &[&str] = &["commands.list", "config.edit"];

    #[test]
    fn every_command_this_face_adds_is_bound_or_deliberately_is_not() {
        let bound: BTreeSet<&str> = BINDINGS
            .iter()
            .chain(MAC_CHORDS)
            .map(|(_, command)| command.as_str())
            .collect();
        for meta in COMMANDS {
            let id = meta.id().as_str();
            assert!(
                bound.contains(id) || PALETTE_ONLY.contains(&id),
                "{id} has no key and is not on the palette-only list, and this \
                 face's own verbs otherwise all deserve one"
            );
        }
    }

    #[test]
    fn nothing_on_the_palette_only_list_is_also_bound() {
        // The list is a record of a decision, and a stale record is worse than
        // none: a verb that later gained a chord would keep its excuse and the
        // next unbound one would inherit it.
        let bound: BTreeSet<&str> = BINDINGS
            .iter()
            .chain(MAC_CHORDS)
            .map(|(_, command)| command.as_str())
            .collect();
        for id in PALETTE_ONLY {
            assert!(
                !bound.contains(id),
                "{id} is bound after all — take it off the palette-only list"
            );
            assert!(
                COMMANDS.iter().any(|meta| meta.id().as_str() == *id),
                "{id} is on the palette-only list but this face does not add it"
            );
        }
    }

    #[test]
    fn the_save_ids_are_the_terminal_faces_ids() {
        // One id, one meaning, across every face. The strings are asserted
        // here because the terminal face declares its own constants and
        // nothing else would notice the two crates drifting apart.
        assert_eq!(FILE_SAVE.as_str(), "file.save");
        assert_eq!(FILE_SAVE_FORCE.as_str(), "file.saveForce");
    }

    /// Every single-stroke binding of the default keymap that `stroke` could
    /// fire instead of, named by its command.
    fn defaults_overlapped(defaults: &Keymap, stroke: &StrokePattern) -> Vec<String> {
        defaults
            .bindings()
            .iter()
            .filter(|binding| {
                let Some((first, rest)) = binding.sequence().split_first() else {
                    return false;
                };
                rest.is_empty() && first.overlaps(stroke)
            })
            .filter_map(|binding| binding.command().map(ToString::to_string))
            .collect()
    }

    #[test]
    fn no_binding_collides_with_the_default_keymap_except_the_mac_chords() {
        // Two layers may legitimately bind one stroke — that is what a layer
        // is for — but the letter rows of this face are adding verbs and mac
        // spellings, not rebinding the kernel's `Ctrl` chords, so an overlap
        // there means a key silently stopped doing what it did.
        let defaults = default_non_modal_keymap();
        for (stroke, command) in BINDINGS {
            let shadowed = defaults_overlapped(&defaults, stroke);
            assert!(
                shadowed.is_empty(),
                "{command} shadows the default bindings for {shadowed:?}"
            );
        }

        // `MAC_CHORDS` is the allowlist, and it overlaps by construction: the
        // default keymap's arrow and delete patterns declare `Alt` and `Meta`
        // `Any`, so the only way to give `⌥←` or `⌘⌫` a verb of its own is a
        // `Required` mac spelling that matches inside the loose pattern and
        // outranks it. A row that overrides *nothing* is the mistake this half
        // catches: it would mean the chord was already bound elsewhere, or that
        // the pattern is not the one the default keymap actually uses.
        for (stroke, command) in MAC_CHORDS {
            assert!(
                !defaults_overlapped(&defaults, stroke).is_empty(),
                "{command} is filed as a deliberate override but overrides nothing"
            );
        }
    }

    #[test]
    fn the_keymap_binds_every_row_of_the_table() {
        let keymap = keymap();
        assert_eq!(keymap.len(), BINDING_COUNT);
        let bound: BTreeSet<String> = keymap
            .bindings()
            .iter()
            .filter_map(|binding| binding.command().map(ToString::to_string))
            .collect();
        for (_, command) in BINDINGS.iter().chain(MAC_CHORDS) {
            assert!(bound.contains(command.as_str()), "{command} is not bound");
        }
    }

    #[test]
    fn the_keymap_validates_against_a_registry_holding_these_commands() {
        // `push_keymap` runs this check for real at start-up; running it here
        // means a mistyped id fails in a unit test rather than at launch.
        let mut editor = Editor::with_defaults();
        for meta in command_metas() {
            editor
                .register_command(meta)
                .expect("the kernel accepted the command");
        }
        editor.push_keymap(keymap()).expect("the keymap validates");
    }
}
