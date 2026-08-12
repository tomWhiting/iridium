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
//! Two of the ids this face contributes are **the terminal face's ids,
//! character for character**: `file.save` and `file.saveForce`. One id and
//! one meaning across every face is the whole argument of the kernel's
//! `builtin::host` module, and two faces spelling the same verb differently
//! is exactly the drift it warns about.
//!
//! # ⚠️ Two ids this face contributes that the terminal face does not
//!
//! `file.saveAs` and `file.new` are **desktop-only today**, and that is stated
//! here rather than left to be noticed, because the paragraph above is a
//! promise about drift and these are the two places it is not yet kept.
//!
//! * **`file.saveAs`** is a verb the terminal face *has* — `App::save_as` is
//!   written and tested there — with no way in, exactly the gap this face just
//!   closed. What is not transferable is the **key**. The split below is
//!   `Ctrl+S` against `Ctrl+⇧S`, and the terminal face's own `CTRL` pattern
//!   ignores `Shift` for a stated reason: *a terminal cannot always report it*.
//!   So the chord that opens this door on a window is not a chord that face can
//!   rely on, and choosing what replaces it — another chord, or a palette-only
//!   row like `commands.list` — is a decision about that face, taken with the
//!   modal work, not transcribed from here.
//! * **`file.new`** is a different verb on the two faces rather than the same
//!   one missing. This face has tabs, so a new file is additive and asks
//!   nothing; the terminal face holds one document for the session, so the same
//!   verb would have to discard it and would need the confirmation a reload
//!   already carries. Same word, different stakes — which is precisely the
//!   drift the paragraph above warns about, so the id stays unclaimed there
//!   until that face has an answer of its own.
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
//! | `Ctrl+⇧S` / `⌘⇧S` | [`FILE_SAVE_AS`] | asks where, starting from the current name |
//! | `Ctrl+Alt+S` / `⌘⌥S` | [`FILE_SAVE_FORCE`] | saves anyway |
//! | `Ctrl+N` / `⌘N` | [`FILE_NEW`] | a fresh untitled tab |
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

mod ids;
mod keymap;
#[cfg(test)]
mod tests;

pub use ids::{
    COMMAND_COUNT, COMMANDS, COMMANDS_LIST, CONFIG_EDIT, FILE_NEW, FILE_OPEN, FILE_SAVE,
    FILE_SAVE_AS, FILE_SAVE_FORCE, PROJECT_OPEN, PROJECT_SET, command_metas,
};
pub use keymap::{BINDING_COUNT, keymap};
