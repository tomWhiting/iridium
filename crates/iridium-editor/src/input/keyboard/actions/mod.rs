//! The command table: one [`CommandId`] to one implementation.
//!
//! Dispatch resolves a keypress to a [`CommandId`] (see [`super::dispatch`]) and
//! then needs to *run* it. That last hop is this module, and it is deliberately
//! two steps rather than one:
//!
//! 1. [`ACTIONS`] maps every built-in [`CommandId`] to a [`KeyboardAction`] —
//!    a `Copy` token with one variant per command. The table is the only place
//!    an id string is tied to behaviour, and it names the ids through the
//!    constants in [`crate::commands::builtin`], so there is no second
//!    transcription of the id text to drift.
//! 2. [`KeyboardHandler::run_action`] matches on the token. Because the match is
//!    exhaustive over an enum, a command added to [`ACTIONS`] without an
//!    implementation is a compile error rather than a key that silently does
//!    nothing.
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

use crate::commands::{CommandArgs, CommandId, builtin};
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
    /// Read by exactly one command — [`builtin::EDIT_INSERT_CHARACTER`] — which
    /// needs the character the user actually typed. Keymap resolution works on the
    /// *normalized* keypress, and that normalized form must never reach the
    /// document: it would turn `A` into `a`.
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

/// Short local alias, so one table entry and one dispatch arm each fit on a line.
use KeyboardAction as Action;

/// Every built-in command id paired with its implementation token.
///
/// Exhaustive over [`crate::commands::builtin`]: the module tests assert that
/// every registered built-in appears here exactly once, and that every entry
/// here is a registered built-in. Order is irrelevant to correctness; it follows
/// the id declaration order for readability.
static ACTIONS: &[(CommandId, KeyboardAction)] = &[
    // Navigation.
    (builtin::CURSOR_CHAR_LEFT, Action::CharLeft),
    (builtin::CURSOR_CHAR_RIGHT, Action::CharRight),
    (builtin::CURSOR_WORD_LEFT, Action::WordLeft),
    (builtin::CURSOR_WORD_RIGHT, Action::WordRight),
    (builtin::CURSOR_LINE_UP, Action::LineUp),
    (builtin::CURSOR_LINE_DOWN, Action::LineDown),
    (builtin::CURSOR_LINE_START, Action::LineStart),
    (builtin::CURSOR_LINE_END, Action::LineEnd),
    (builtin::CURSOR_DOCUMENT_START, Action::DocumentStart),
    (builtin::CURSOR_DOCUMENT_END, Action::DocumentEnd),
    // Selection.
    (builtin::CURSOR_CHAR_LEFT_SELECT, Action::CharLeftSelect),
    (builtin::CURSOR_CHAR_RIGHT_SELECT, Action::CharRightSelect),
    (builtin::CURSOR_WORD_LEFT_SELECT, Action::WordLeftSelect),
    (builtin::CURSOR_WORD_RIGHT_SELECT, Action::WordRightSelect),
    (builtin::CURSOR_LINE_UP_SELECT, Action::LineUpSelect),
    (builtin::CURSOR_LINE_DOWN_SELECT, Action::LineDownSelect),
    (builtin::CURSOR_LINE_START_SELECT, Action::LineStartSelect),
    (builtin::CURSOR_LINE_END_SELECT, Action::LineEndSelect),
    (
        builtin::CURSOR_DOCUMENT_START_SELECT,
        Action::DocumentStartSelect,
    ),
    (
        builtin::CURSOR_DOCUMENT_END_SELECT,
        Action::DocumentEndSelect,
    ),
    (builtin::SELECTION_SELECT_ALL, Action::SelectAll),
    (
        builtin::SELECTION_COLLAPSE_TO_PRIMARY,
        Action::CollapseToPrimary,
    ),
    // Editing.
    (builtin::EDIT_INSERT_CHARACTER, Action::InsertCharacter),
    (builtin::EDIT_INSERT_NEWLINE, Action::InsertNewline),
    (builtin::EDIT_TAB, Action::Tab),
    (builtin::EDIT_OUTDENT, Action::Outdent),
    (builtin::EDIT_DELETE_BACKWARD, Action::DeleteBackward),
    (
        builtin::EDIT_DELETE_WORD_BACKWARD,
        Action::DeleteWordBackward,
    ),
    (builtin::EDIT_DELETE_FORWARD, Action::DeleteForward),
    (builtin::EDIT_DELETE_WORD_FORWARD, Action::DeleteWordForward),
    (
        builtin::EDIT_DELETE_TO_LINE_START,
        Action::DeleteToLineStart,
    ),
    (builtin::EDIT_DELETE_TO_LINE_END, Action::DeleteToLineEnd),
    // Text transformations.
    (builtin::TRANSFORM_UPPER_CASE, Action::TransformUpperCase),
    (builtin::TRANSFORM_LOWER_CASE, Action::TransformLowerCase),
    (builtin::TRANSFORM_TITLE_CASE, Action::TransformTitleCase),
    (builtin::TRANSFORM_TOGGLE_CASE, Action::TransformToggleCase),
    (builtin::TRANSFORM_SWAP_CASE, Action::TransformSwapCase),
    (builtin::TRANSFORM_CAMEL_CASE, Action::TransformCamelCase),
    (builtin::TRANSFORM_PASCAL_CASE, Action::TransformPascalCase),
    (builtin::TRANSFORM_SNAKE_CASE, Action::TransformSnakeCase),
    (
        builtin::TRANSFORM_SCREAMING_SNAKE_CASE,
        Action::TransformScreamingSnakeCase,
    ),
    (builtin::TRANSFORM_KEBAB_CASE, Action::TransformKebabCase),
    (builtin::TRANSFORM_SORT_LINES, Action::TransformSortLines),
    (
        builtin::TRANSFORM_SORT_LINES_REVERSE,
        Action::TransformSortLinesReverse,
    ),
    (
        builtin::TRANSFORM_REVERSE_LINES,
        Action::TransformReverseLines,
    ),
    (
        builtin::TRANSFORM_DEDUPE_LINES,
        Action::TransformDedupeLines,
    ),
    (
        builtin::TRANSFORM_TRIM_TRAILING_WHITESPACE,
        Action::TransformTrimTrailingWhitespace,
    ),
    // Whole-line operations.
    (builtin::LINES_MOVE_UP, Action::LinesMoveUp),
    (builtin::LINES_MOVE_DOWN, Action::LinesMoveDown),
    (builtin::LINES_DUPLICATE_UP, Action::LinesDuplicateUp),
    (builtin::LINES_DUPLICATE_DOWN, Action::LinesDuplicateDown),
    (builtin::LINES_DELETE, Action::LinesDelete),
    (builtin::LINES_JOIN, Action::LinesJoin),
    // Comments.
    (builtin::COMMENT_TOGGLE_LINE, Action::ToggleLineComment),
    (builtin::COMMENT_TOGGLE_BLOCK, Action::ToggleBlockComment),
    // Clipboard.
    (builtin::CLIPBOARD_COPY, Action::ClipboardCopy),
    (builtin::CLIPBOARD_CUT, Action::ClipboardCut),
    (builtin::CLIPBOARD_PASTE, Action::ClipboardPaste),
    // History.
    (builtin::HISTORY_UNDO, Action::Undo),
    (builtin::HISTORY_REDO, Action::Redo),
    (builtin::HISTORY_REDO_BRANCH, Action::HistoryRedoBranch),
    (builtin::HISTORY_NEXT_BRANCH, Action::HistoryNextBranch),
    (
        builtin::HISTORY_PREVIOUS_BRANCH,
        Action::HistoryPreviousBranch,
    ),
    // Multi-cursor.
    (
        builtin::MULTI_CURSOR_ADD_SELECTION_TO_NEXT_MATCH,
        Action::AddSelectionToNextMatch,
    ),
    (
        builtin::MULTI_CURSOR_SELECT_ALL_OCCURRENCES,
        Action::SelectAllOccurrences,
    ),
    (
        builtin::MULTI_CURSOR_REMOVE_LAST_CURSOR,
        Action::RemoveLastCursor,
    ),
    (
        builtin::MULTI_CURSOR_ADD_CURSOR_ABOVE,
        Action::AddCursorAbove,
    ),
    (
        builtin::MULTI_CURSOR_ADD_CURSOR_BELOW,
        Action::AddCursorBelow,
    ),
    (
        builtin::MULTI_CURSOR_SKIP_LAST_OCCURRENCE,
        Action::SkipLastOccurrence,
    ),
    // General.
    (builtin::COMMAND_NO_OP, Action::NoOp),
    // Syntax.
    (builtin::AST_SELECT_NODE, Action::AstSelectNode),
    (builtin::AST_EXPAND_SELECTION, Action::AstExpandSelection),
    (builtin::AST_SHRINK_SELECTION, Action::AstShrinkSelection),
    (
        builtin::AST_SELECT_NEXT_SIBLING,
        Action::AstSelectNextSibling,
    ),
    (
        builtin::AST_SELECT_PREVIOUS_SIBLING,
        Action::AstSelectPreviousSibling,
    ),
    (builtin::AST_SELECT_FIRST_CHILD, Action::AstSelectFirstChild),
    (builtin::AST_SELECT_LAST_CHILD, Action::AstSelectLastChild),
    (
        builtin::AST_EXTEND_NEXT_SIBLING,
        Action::AstExtendNextSibling,
    ),
    (
        builtin::AST_EXTEND_PREVIOUS_SIBLING,
        Action::AstExtendPreviousSibling,
    ),
    (builtin::AST_CURSOR_NODE_START, Action::AstCursorNodeStart),
    (builtin::AST_CURSOR_NODE_END, Action::AstCursorNodeEnd),
    (
        builtin::AST_CURSOR_ON_EVERY_SIBLING,
        Action::AstCursorOnEverySibling,
    ),
    (
        builtin::AST_CURSOR_ON_EVERY_CHILD,
        Action::AstCursorOnEveryChild,
    ),
    (
        builtin::AST_SELECT_FUNCTION_INSIDE,
        Action::AstSelectFunctionInside,
    ),
    (
        builtin::AST_SELECT_FUNCTION_AROUND,
        Action::AstSelectFunctionAround,
    ),
    (
        builtin::AST_SELECT_CLASS_INSIDE,
        Action::AstSelectClassInside,
    ),
    (
        builtin::AST_SELECT_CLASS_AROUND,
        Action::AstSelectClassAround,
    ),
    (
        builtin::AST_SELECT_COMMENT_AROUND,
        Action::AstSelectCommentAround,
    ),
    (builtin::AST_NEXT_FUNCTION, Action::AstNextFunction),
    (builtin::AST_PREVIOUS_FUNCTION, Action::AstPreviousFunction),
    (builtin::AST_NEXT_CLASS, Action::AstNextClass),
    (builtin::AST_PREVIOUS_CLASS, Action::AstPreviousClass),
    // Search.
    (builtin::SEARCH_OPEN, Action::SearchOpen),
    (builtin::SEARCH_NEXT_MATCH, Action::SearchNextMatch),
    (builtin::SEARCH_PREVIOUS_MATCH, Action::SearchPreviousMatch),
];

/// Returns the implementation token for `id`, or `None` when this handler does
/// not implement it.
///
/// `None` is a legitimate outcome, not an error: a host may register commands of
/// its own and bind them in a user keymap, and those are the host's to run. The
/// keyboard handler reports such a key as [`KeyResult::Ignored`] so the host
/// sees it.
///
/// # Cost
///
/// A linear scan over the table's `'static` string comparisons, each rejected on
/// length before any byte is read. That is tens of nanoseconds against an 8 ms
/// input budget, and it buys a table with no second copy of the id text and no
/// lazily initialized global. It allocates nothing.
pub(super) fn action_for(id: &str) -> Option<KeyboardAction> {
    ACTIONS
        .iter()
        .find(|(command, _)| command.as_str() == id)
        .map(|entry| entry.1)
}

/// The number of commands this handler implements.
///
/// Asserted against [`crate::commands::builtin::BUILTIN_COMMAND_COUNT`] in the
/// module tests, so a command cannot be registered without an implementation.
#[cfg(test)]
pub(super) const IMPLEMENTED_COMMAND_COUNT: usize = ACTIONS.len();

/// Every implemented command id, for the exhaustiveness tests.
#[cfg(test)]
pub(super) fn implemented_ids() -> impl Iterator<Item = &'static CommandId> {
    ACTIONS.iter().map(|entry| &entry.0)
}
