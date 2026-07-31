//! Stable ids for every command the keyboard layer exposes today.
//!
//! These constants are the compatibility surface described on
//! [`CommandId`]: keymaps, macros and scripting hosts reference them by string.
//! Add freely; never rename or remove without an alias.
//!
//! The list is an exhaustive transcription of the dispatch in
//! `crate::input::keyboard` as it stands, so the migration onto the registry can
//! be checked binding-for-binding.

use crate::commands::CommandId;

// ===== Navigation: motions that move the caret without extending =====

/// Move every caret one character left.
pub const CURSOR_CHAR_LEFT: CommandId = CommandId::from_static("cursor.charLeft");
/// Move every caret one character right.
pub const CURSOR_CHAR_RIGHT: CommandId = CommandId::from_static("cursor.charRight");
/// Move every caret one word left.
pub const CURSOR_WORD_LEFT: CommandId = CommandId::from_static("cursor.wordLeft");
/// Move every caret one word right.
pub const CURSOR_WORD_RIGHT: CommandId = CommandId::from_static("cursor.wordRight");
/// Move every caret one line up, honouring its sticky column.
pub const CURSOR_LINE_UP: CommandId = CommandId::from_static("cursor.lineUp");
/// Move every caret one line down, honouring its sticky column.
pub const CURSOR_LINE_DOWN: CommandId = CommandId::from_static("cursor.lineDown");
/// Move every caret to the start of its line (smart home).
pub const CURSOR_LINE_START: CommandId = CommandId::from_static("cursor.lineStart");
/// Move every caret to the end of its line.
pub const CURSOR_LINE_END: CommandId = CommandId::from_static("cursor.lineEnd");
/// Move the caret to the start of the document, merging all cursors.
pub const CURSOR_DOCUMENT_START: CommandId = CommandId::from_static("cursor.documentStart");
/// Move the caret to the end of the document, merging all cursors.
pub const CURSOR_DOCUMENT_END: CommandId = CommandId::from_static("cursor.documentEnd");

// ===== Selection: the extending variants, plus whole-selection verbs =====

/// Extend every selection one character left.
pub const CURSOR_CHAR_LEFT_SELECT: CommandId = CommandId::from_static("cursor.charLeftSelect");
/// Extend every selection one character right.
pub const CURSOR_CHAR_RIGHT_SELECT: CommandId = CommandId::from_static("cursor.charRightSelect");
/// Extend every selection one word left.
pub const CURSOR_WORD_LEFT_SELECT: CommandId = CommandId::from_static("cursor.wordLeftSelect");
/// Extend every selection one word right.
pub const CURSOR_WORD_RIGHT_SELECT: CommandId = CommandId::from_static("cursor.wordRightSelect");
/// Extend every selection one line up.
pub const CURSOR_LINE_UP_SELECT: CommandId = CommandId::from_static("cursor.lineUpSelect");
/// Extend every selection one line down.
pub const CURSOR_LINE_DOWN_SELECT: CommandId = CommandId::from_static("cursor.lineDownSelect");
/// Extend every selection to the start of its line (smart home).
pub const CURSOR_LINE_START_SELECT: CommandId = CommandId::from_static("cursor.lineStartSelect");
/// Extend every selection to the end of its line.
pub const CURSOR_LINE_END_SELECT: CommandId = CommandId::from_static("cursor.lineEndSelect");
/// Extend the selection to the start of the document.
pub const CURSOR_DOCUMENT_START_SELECT: CommandId =
    CommandId::from_static("cursor.documentStartSelect");
/// Extend the selection to the end of the document.
pub const CURSOR_DOCUMENT_END_SELECT: CommandId =
    CommandId::from_static("cursor.documentEndSelect");
/// Select the whole document as one selection.
pub const SELECTION_SELECT_ALL: CommandId = CommandId::from_static("selection.selectAll");
/// Drop every secondary cursor and collapse the primary selection.
pub const SELECTION_COLLAPSE_TO_PRIMARY: CommandId =
    CommandId::from_static("selection.collapseToPrimary");

// ===== Editing =====

/// Insert the typed character at every cursor.
///
/// Intentionally **not bound** in any keymap: no key sequence can stand for
/// "whatever character the user typed". It is registered because it is a real,
/// palette-listable, host-invocable command and because the migration needs a
/// name for the fall-through path — a keypress resolving to
/// [`Resolution::NoMatch`](crate::commands::Resolution) that carries text runs
/// this command with the character from the original key event.
pub const EDIT_INSERT_CHARACTER: CommandId = CommandId::from_static("edit.insertCharacter");
/// Insert a line break at every cursor, applying auto-indent behaviours.
pub const EDIT_INSERT_NEWLINE: CommandId = CommandId::from_static("edit.insertNewline");
/// Tab: indent every touched line when anything is selected, otherwise insert a
/// tab or pad to the next tab stop.
pub const EDIT_TAB: CommandId = CommandId::from_static("edit.tab");
/// Outdent every line touched by a cursor or selection by one level.
pub const EDIT_OUTDENT: CommandId = CommandId::from_static("edit.outdent");
/// Delete the selection, or the character before each caret.
pub const EDIT_DELETE_BACKWARD: CommandId = CommandId::from_static("edit.deleteBackward");
/// Delete the selection, or the word before each caret.
pub const EDIT_DELETE_WORD_BACKWARD: CommandId = CommandId::from_static("edit.deleteWordBackward");
/// Delete the selection, or the character after each caret.
pub const EDIT_DELETE_FORWARD: CommandId = CommandId::from_static("edit.deleteForward");
/// Delete the selection, or the word after each caret.
pub const EDIT_DELETE_WORD_FORWARD: CommandId = CommandId::from_static("edit.deleteWordForward");
/// Delete the selection, or from each caret back to the start of its line.
pub const EDIT_DELETE_TO_LINE_START: CommandId = CommandId::from_static("edit.deleteToLineStart");
/// Delete the selection, or from each caret to the end of its line.
pub const EDIT_DELETE_TO_LINE_END: CommandId = CommandId::from_static("edit.deleteToLineEnd");

// ===== Whole-line operations =====

/// Move each cursor's line block up one line.
pub const LINES_MOVE_UP: CommandId = CommandId::from_static("lines.moveUp");
/// Move each cursor's line block down one line.
pub const LINES_MOVE_DOWN: CommandId = CommandId::from_static("lines.moveDown");
/// Duplicate each cursor's lines upwards.
pub const LINES_DUPLICATE_UP: CommandId = CommandId::from_static("lines.duplicateUp");
/// Duplicate each cursor's lines downwards.
pub const LINES_DUPLICATE_DOWN: CommandId = CommandId::from_static("lines.duplicateDown");
/// Delete every line touched by a cursor or selection.
pub const LINES_DELETE: CommandId = CommandId::from_static("lines.delete");
/// Join each cursor's line with the following line.
pub const LINES_JOIN: CommandId = CommandId::from_static("lines.join");

// ===== Comments =====

/// Toggle line comments over every line touched by a cursor.
pub const COMMENT_TOGGLE_LINE: CommandId = CommandId::from_static("comment.toggleLine");
/// Toggle a block comment around every selection, falling back to line comments.
pub const COMMENT_TOGGLE_BLOCK: CommandId = CommandId::from_static("comment.toggleBlock");

// ===== Clipboard =====

/// Copy the selections, or the caret lines when nothing is selected.
pub const CLIPBOARD_COPY: CommandId = CommandId::from_static("clipboard.copy");
/// Copy and then delete, as one undoable edit.
pub const CLIPBOARD_CUT: CommandId = CommandId::from_static("clipboard.cut");
/// Request a paste from the host clipboard.
pub const CLIPBOARD_PASTE: CommandId = CommandId::from_static("clipboard.paste");

// ===== History =====

/// Undo the last change.
///
/// **Known defect, faithfully modelled:** the keyboard handler's `handle_undo` is
/// a `const fn` that ignores the history and merely acknowledges the key, so undo
/// works today only because the WASM host intercepts the chord before dispatch.
/// The command is named here because it is a real command; the observable
/// behaviour is unchanged by this phase.
pub const HISTORY_UNDO: CommandId = CommandId::from_static("history.undo");
/// Redo the last undone change.
///
/// Carries the same known defect as [`HISTORY_UNDO`], and additionally the
/// current dispatch matches `Ctrl+Z` regardless of `Shift`, so `Ctrl+Shift+Z`
/// means undo rather than redo. The default keymap reproduces that exactly; see
/// [`default_non_modal_keymap`](crate::commands::default_non_modal_keymap).
pub const HISTORY_REDO: CommandId = CommandId::from_static("history.redo");

// ===== Multi-cursor =====

/// Select the word at the caret, or add a cursor at the next occurrence.
pub const MULTI_CURSOR_ADD_SELECTION_TO_NEXT_MATCH: CommandId =
    CommandId::from_static("multiCursor.addSelectionToNextMatch");
/// Add a cursor at every occurrence of the current selection.
pub const MULTI_CURSOR_SELECT_ALL_OCCURRENCES: CommandId =
    CommandId::from_static("multiCursor.selectAllOccurrences");
/// Remove the most recently added cursor.
pub const MULTI_CURSOR_REMOVE_LAST_CURSOR: CommandId =
    CommandId::from_static("multiCursor.removeLastCursor");
/// Add a cursor one line above every existing cursor.
pub const MULTI_CURSOR_ADD_CURSOR_ABOVE: CommandId =
    CommandId::from_static("multiCursor.addCursorAbove");
/// Add a cursor one line below every existing cursor.
pub const MULTI_CURSOR_ADD_CURSOR_BELOW: CommandId =
    CommandId::from_static("multiCursor.addCursorBelow");
/// Move the most recently added occurrence cursor to the next match.
///
/// Deliberately **unbound** by the default keymap: it briefly held `Ctrl+K
/// Ctrl+D`, which was given up so that bare `Ctrl+K` could open the command
/// palette — binding a prefix forecloses every chord beneath it. Reach it from
/// the palette, from a host layer that rebinds the chord, or in Rust as
/// [`KeyboardHandler::skip_last_added_occurrence`](crate::input::KeyboardHandler::skip_last_added_occurrence).
pub const MULTI_CURSOR_SKIP_LAST_OCCURRENCE: CommandId =
    CommandId::from_static("multiCursor.skipLastOccurrence");

// ===== Search =====

/// Open the search panel.
pub const SEARCH_OPEN: CommandId = CommandId::from_static("search.open");
/// Go to the next search match.
pub const SEARCH_NEXT_MATCH: CommandId = CommandId::from_static("search.nextMatch");
/// Go to the previous search match.
pub const SEARCH_PREVIOUS_MATCH: CommandId = CommandId::from_static("search.previousMatch");

// ===== General =====

/// Consume the keypress and do nothing.
///
/// Not bound by the default keymap, and deliberately so: it exists for keymaps
/// that must *swallow* a key rather than unbind it. A
/// [`suppression`](crate::commands::KeyBinding::unbound) falls through to text
/// insertion, which is right for "this chord is not mine" and wrong for a modal
/// keymap's normal mode, where an unhandled letter must not be typed into the
/// document. Binding that letter — or a
/// [`capture wildcard`](crate::commands::StrokeCapture::AnyChar) covering every
/// letter — to this command is how a mode stops typing without the kernel
/// knowing what a mode is.
pub const COMMAND_NO_OP: CommandId = CommandId::from_static("command.noOp");
