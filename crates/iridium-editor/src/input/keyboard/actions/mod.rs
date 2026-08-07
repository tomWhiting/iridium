//! The command table: one [`CommandId`](crate::commands::CommandId) to one
//! implementation.
//!
//! Dispatch resolves a keypress to a command id (see [`super::dispatch`]) and
//! then needs to *run* it. That last hop is this module, and it is deliberately
//! two steps rather than one:
//!
//! 1. [`ACTIONS`](table::ACTIONS) maps every built-in `CommandId` to a
//!    [`KeyboardAction`] — a `Copy` token with one variant per command. The
//!    table is the only place an id string is tied to behaviour, and it names
//!    the ids through the constants in [`crate::commands::builtin`], so there is
//!    no second transcription of the id text to drift.
//! 2. [`KeyboardHandler`](super::KeyboardHandler)'s `run_action` matches on the
//!    token. Because the match is
//!    exhaustive over an enum, a command added to [`ACTIONS`](table::ACTIONS)
//!    without an implementation is a compile error rather than a key that
//!    silently does nothing.
//!
//! The token also carries the post-dispatch bookkeeping: the sticky-column and
//! addition-order predicates in [`super::dispatch`] are `matches!` over these
//! variants, so they cost nothing on the hot path and cannot fall out of step
//! with the bindings the way a modifier-sniffing predicate could.
//!
//! # Variant naming
//!
//! Each variant is the 1:1 counterpart of the identically-named constant in
//! [`crate::commands::builtin`], which is the authoritative description of what
//! the command does; the pairing is asserted exhaustively in the module's
//! `dispatch_tests`. Grouping comments below mirror the sections of that module.

mod run;
mod table;

pub(super) use table::action_for;
#[cfg(test)]
pub(super) use table::{IMPLEMENTED_COMMAND_COUNT, implemented_ids};

use crate::commands::CommandArgs;
use crate::document::{CursorState, Document};
use crate::editor::EditorConfig;

use super::types::KeyEvent;

/// The editor state one command implementation may read.
///
/// Bundled into a struct rather than passed as five parameters because every
/// implementation reads a different subset and threading all five through each
/// call site obscured which ones mattered.
pub(super) struct CommandContext<'a> {
    /// The original, un-normalized key event, or `None` when the command was
    /// invoked by id rather than by a keystroke.
    ///
    /// Read by exactly one command —
    /// [`EDIT_INSERT_CHARACTER`](crate::commands::builtin::EDIT_INSERT_CHARACTER)
    /// — which needs the character the user actually typed. Keymap resolution
    /// works on the *normalized* keypress, and that normalized form must never
    /// reach the document: it would turn `A` into `a`.
    ///
    /// `Option` rather than a required field because a command palette, a macro
    /// and an AI host all invoke commands with no keypress at all, and fabricating
    /// a fake `KeyEvent` for them would be a lie the one event-reading command
    /// could act on.
    pub event: Option<&'a KeyEvent>,
    /// The count and captured characters the key sequence carried, empty for an
    /// invocation that named no arguments.
    pub args: CommandArgs,
    /// The document the command reads; it is never mutated here. A command that
    /// changes text returns a reversible [`crate::history::Command`] instead.
    pub document: &'a Document,
    /// The cursor state the command acts on.
    pub cursor: &'a CursorState,
    /// The behaviour configuration (tab width, auto-indent, auto-pairs, comment
    /// token fallback).
    pub config: &'a EditorConfig,
}

/// One built-in command, as a `Copy` token.
///
/// See the module docs for why this exists rather than dispatching on the id
/// string directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum KeyboardAction {
    // ----- Navigation -----
    /// Move every caret one character left.
    CharLeft,
    /// Move every caret one character right.
    CharRight,
    /// Move every caret one word left.
    WordLeft,
    /// Move every caret one word right.
    WordRight,
    /// Move every caret one line up, honouring its sticky column.
    LineUp,
    /// Move every caret one line down, honouring its sticky column.
    LineDown,
    /// Move every caret one viewport-height up, honouring its sticky column.
    PageUp,
    /// Move every caret one viewport-height down, honouring its sticky column.
    PageDown,
    /// Move every caret to the start of its line (smart home).
    LineStart,
    /// Move every caret to the end of its line.
    LineEnd,
    /// Move to the start of the document, merging all cursors.
    DocumentStart,
    /// Move to the end of the document, merging all cursors.
    DocumentEnd,

    // ----- Selection -----
    /// Extend every selection one character left.
    CharLeftSelect,
    /// Extend every selection one character right.
    CharRightSelect,
    /// Extend every selection one word left.
    WordLeftSelect,
    /// Extend every selection one word right.
    WordRightSelect,
    /// Extend every selection one line up.
    LineUpSelect,
    /// Extend every selection one line down.
    LineDownSelect,
    /// Extend every selection one viewport-height up.
    PageUpSelect,
    /// Extend every selection one viewport-height down.
    PageDownSelect,
    /// Extend every selection to the start of its line (smart home).
    LineStartSelect,
    /// Extend every selection to the end of its line.
    LineEndSelect,
    /// Extend the selection to the start of the document.
    DocumentStartSelect,
    /// Extend the selection to the end of the document.
    DocumentEndSelect,
    /// Select the whole document as one selection.
    SelectAll,
    /// Drop every secondary cursor and collapse the primary selection.
    CollapseToPrimary,

    // ----- Editing -----
    /// Insert the typed character at every cursor.
    InsertCharacter,
    /// Insert a line break at every cursor, applying auto-indent behaviours.
    InsertNewline,
    /// Indent the touched lines, or insert a tab / pad to the next tab stop.
    Tab,
    /// Outdent every line touched by a cursor or selection by one level.
    Outdent,
    /// Delete the selection, or the character before each caret.
    DeleteBackward,
    /// Delete the selection, or the word before each caret.
    DeleteWordBackward,
    /// Delete the selection, or the character after each caret.
    DeleteForward,
    /// Delete the selection, or the word after each caret.
    DeleteWordForward,
    /// Delete the selection, or from each caret back to the start of its line.
    DeleteToLineStart,
    /// Delete the selection, or from each caret to the end of its line.
    DeleteToLineEnd,

    // ----- Text transformations -----
    //
    // One variant per verb rather than one carrying a `CaseVerb`/`LineVerb`
    // payload: the enum must stay `Copy` and exhaustively matched against the
    // id table, and a payload-carrying variant would let two ids map to the
    // same variant without a compile error.
    /// Uppercase each caret's selection, or the word under it.
    TransformUpperCase,
    /// Lowercase each caret's selection, or the word under it.
    TransformLowerCase,
    /// Capitalise each word, keeping spacing and punctuation.
    TransformTitleCase,
    /// Cycle each caret's text through lower, upper and title case.
    TransformToggleCase,
    /// Invert the case of every cased character.
    TransformSwapCase,
    /// Re-join the words as `camelCase`.
    TransformCamelCase,
    /// Re-join the words as `PascalCase`.
    TransformPascalCase,
    /// Re-join the words as `snake_case`.
    TransformSnakeCase,
    /// Re-join the words as `SCREAMING_SNAKE_CASE`.
    TransformScreamingSnakeCase,
    /// Re-join the words as `kebab-case`.
    TransformKebabCase,
    /// Sort the selected lines ascending.
    TransformSortLines,
    /// Sort the selected lines descending.
    TransformSortLinesReverse,
    /// Reverse the order of the selected lines.
    TransformReverseLines,
    /// Remove repeated lines, keeping the first of each.
    TransformDedupeLines,
    /// Strip trailing whitespace from the selected lines.
    TransformTrimTrailingWhitespace,

    // ----- Whole-line operations -----
    /// Move each cursor's line block up one line.
    LinesMoveUp,
    /// Move each cursor's line block down one line.
    LinesMoveDown,
    /// Duplicate each cursor's lines upwards.
    LinesDuplicateUp,
    /// Duplicate each cursor's lines downwards.
    LinesDuplicateDown,
    /// Delete every line touched by a cursor.
    LinesDelete,
    /// Join each cursor's line with the one below it.
    LinesJoin,

    // ----- Comments -----
    /// Toggle line comments over every touched line.
    ToggleLineComment,
    /// Toggle a block comment around every selection.
    ToggleBlockComment,

    // ----- Clipboard -----
    /// Copy the selections, or the whole lines of collapsed carets.
    ClipboardCopy,
    /// Copy and then delete, in one undoable step.
    ClipboardCut,
    /// Request a paste from the host clipboard.
    ClipboardPaste,

    // ----- History -----
    /// Undo the last change.
    Undo,
    /// Redo the last undone change.
    Redo,
    /// Redo into a chosen branch of the current node, by count.
    HistoryRedoBranch,
    /// Point redo at the next branch, without moving.
    HistoryNextBranch,
    /// Point redo at the previous branch, without moving.
    HistoryPreviousBranch,

    // ----- Multi-cursor -----
    /// Select the word at the caret, or add a cursor at the next match.
    AddSelectionToNextMatch,
    /// Turn every occurrence of the selected text into its own cursor.
    SelectAllOccurrences,
    /// Remove the most recently added cursor.
    RemoveLastCursor,
    /// Add a cursor one line above every existing cursor.
    AddCursorAbove,
    /// Add a cursor one line below every existing cursor.
    AddCursorBelow,
    /// Move the most recently added occurrence cursor to the next match.
    SkipLastOccurrence,

    // ----- Syntax -----
    /// Snap every selection to the smallest node covering it.
    AstSelectNode,
    /// Widen every selection to the smallest node strictly containing it.
    AstExpandSelection,
    /// Undo one expansion, restoring the previous selections exactly.
    AstShrinkSelection,
    /// Select the next node beside the current one.
    AstSelectNextSibling,
    /// Select the previous node beside the current one.
    AstSelectPreviousSibling,
    /// Select the first child of the node under each selection.
    AstSelectFirstChild,
    /// Select the last child of the node under each selection.
    AstSelectLastChild,
    /// Grow each selection to also cover the node after it.
    AstExtendNextSibling,
    /// Grow each selection to also cover the node before it.
    AstExtendPreviousSibling,
    /// Move each caret to the start of the node it sits in.
    AstCursorNodeStart,
    /// Move each caret to the end of the node it sits in.
    AstCursorNodeEnd,
    /// Put a cursor on every node beside the current one, including it.
    AstCursorOnEverySibling,
    /// Put a cursor on every child of the node under each selection.
    AstCursorOnEveryChild,
    /// Select the body of the function each selection sits in.
    AstSelectFunctionInside,
    /// Select the whole function each selection sits in.
    AstSelectFunctionAround,
    /// Select the body of the class each selection sits in.
    AstSelectClassInside,
    /// Select the whole class each selection sits in.
    AstSelectClassAround,
    /// Select the whole comment each selection sits in.
    AstSelectCommentAround,
    /// Move each caret to the start of the next function.
    AstNextFunction,
    /// Move each caret to the start of the previous function.
    AstPreviousFunction,
    /// Move each caret to the start of the next class.
    AstNextClass,
    /// Move each caret to the start of the previous class.
    AstPreviousClass,

    // ----- General -----
    /// Consume the keypress and do nothing.
    NoOp,

    // ----- Search -----
    /// Open the search panel.
    SearchOpen,
    /// Go to the next search match.
    SearchNextMatch,
    /// Go to the previous search match.
    SearchPreviousMatch,
}
