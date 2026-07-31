//! The id-to-implementation table.
//!
//! Split from [`super`], which owns the enum this maps *into*. That file is the
//! catalogue of what a command is; this one is the wiring that pairs each
//! [`CommandId`] with its token, plus the lookup over it. See the module docs of
//! [`super`] for why dispatch is two steps rather than one.

use crate::commands::{CommandId, builtin};

use super::KeyboardAction;

/// Short local alias, so one table entry and one dispatch arm each fit on a line.
use KeyboardAction as Action;

/// Every built-in command id paired with its implementation token.
///
/// Exhaustive over [`crate::commands::builtin`]: the module tests assert that
/// every registered built-in appears here exactly once, and that every entry
/// here is a registered built-in. Order is irrelevant to correctness; it follows
/// the id declaration order for readability.
pub(super) static ACTIONS: &[(CommandId, KeyboardAction)] = &[
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
pub(in crate::input::keyboard) fn action_for(id: &str) -> Option<KeyboardAction> {
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
pub(in crate::input::keyboard) const IMPLEMENTED_COMMAND_COUNT: usize = ACTIONS.len();

/// Every implemented command id, for the exhaustiveness tests.
#[cfg(test)]
pub(in crate::input::keyboard) fn implemented_ids() -> impl Iterator<Item = &'static CommandId> {
    ACTIONS.iter().map(|entry| &entry.0)
}
