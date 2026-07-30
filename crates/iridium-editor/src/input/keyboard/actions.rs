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

use crate::commands::{CommandArgs, CommandId, builtin};
use crate::document::{CursorState, Document};
use crate::editor::EditorConfig;
use crate::history::{Command, UndoTree};

use super::motions;
use super::motions::VerticalDirection::{Down, Up};
use super::types::{ClipboardOperation, KeyCode, KeyEvent, KeyResult, SearchAction};
use super::{KeyboardHandler, line_ops};

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
    /// The undo history, read by the history commands.
    pub history: &'a UndoTree,
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

impl KeyboardHandler {
    /// Runs one command against `ctx` and returns what the caller must do.
    ///
    /// This is a pure routing table: every arm delegates to the same handler the
    /// pre-registry `match` on `(KeyCode, Modifiers)` called, with the modifier
    /// interpretation that used to live in the guards now expressed by the
    /// binding that selected the command. `Ctrl+Left` no longer means "word
    /// left because `ctrl` is set"; it means
    /// [`builtin::CURSOR_WORD_LEFT`], and the keymap decided that.
    pub(super) fn run_action(
        &mut self,
        action: KeyboardAction,
        ctx: &CommandContext<'_>,
    ) -> KeyResult {
        let document = ctx.document;
        let cursor = ctx.cursor;
        match action {
            // ----- Navigation and selection -----
            Action::CharLeft => {
                Self::apply_motion(cursor, false, |sel| motions::char_left(document, sel.head))
            },
            Action::CharLeftSelect => {
                Self::apply_motion(cursor, true, |sel| motions::char_left(document, sel.head))
            },
            Action::CharRight => {
                Self::apply_motion(cursor, false, |sel| motions::char_right(document, sel.head))
            },
            Action::CharRightSelect => {
                Self::apply_motion(cursor, true, |sel| motions::char_right(document, sel.head))
            },
            Action::WordLeft => {
                Self::apply_motion(cursor, false, |sel| motions::word_left(document, sel.head))
            },
            Action::WordLeftSelect => {
                Self::apply_motion(cursor, true, |sel| motions::word_left(document, sel.head))
            },
            Action::WordRight => {
                Self::apply_motion(cursor, false, |sel| motions::word_right(document, sel.head))
            },
            Action::WordRightSelect => {
                Self::apply_motion(cursor, true, |sel| motions::word_right(document, sel.head))
            },
            Action::LineStart => Self::apply_motion(cursor, false, |sel| {
                motions::line_start_smart(document, sel.head)
            }),
            Action::LineStartSelect => Self::apply_motion(cursor, true, |sel| {
                motions::line_start_smart(document, sel.head)
            }),
            Action::LineEnd => {
                Self::apply_motion(cursor, false, |sel| motions::line_end(document, sel.head))
            },
            Action::LineEndSelect => {
                Self::apply_motion(cursor, true, |sel| motions::line_end(document, sel.head))
            },
            Action::DocumentStart => {
                Self::apply_motion(cursor, false, |_| motions::document_start())
            },
            Action::DocumentStartSelect => {
                Self::apply_motion(cursor, true, |_| motions::document_start())
            },
            Action::DocumentEnd => {
                Self::apply_motion(cursor, false, |_| motions::document_end(document))
            },
            Action::DocumentEndSelect => {
                Self::apply_motion(cursor, true, |_| motions::document_end(document))
            },
            Action::LineUp => self.vertical_motion(document, cursor, false, Up),
            Action::LineUpSelect => self.vertical_motion(document, cursor, true, Up),
            Action::LineDown => self.vertical_motion(document, cursor, false, Down),
            Action::LineDownSelect => self.vertical_motion(document, cursor, true, Down),
            Action::SelectAll => Self::handle_select_all(document, cursor),
            Action::CollapseToPrimary => Self::handle_collapse_to_primary(cursor),

            // ----- Editing -----
            // The only command that reads the raw event. Invoked by id, with no
            // keystroke to take a character from, there is nothing to insert.
            Action::InsertCharacter => match ctx.event.map(|event| event.key) {
                Some(KeyCode::Char(c)) => Self::handle_char_input(c, document, cursor, ctx.config),
                _ => KeyResult::Ignored,
            },
            Action::InsertNewline => Self::handle_enter(document, cursor, ctx.config),
            Action::Tab => Self::handle_tab(document, cursor, ctx.config),
            Action::Outdent => Self::handle_outdent(document, cursor, ctx.config),
            Action::DeleteBackward => Self::handle_delete_backward(document, cursor, ctx.config),
            Action::DeleteWordBackward => Self::handle_delete_word_backward(document, cursor),
            Action::DeleteForward => Self::handle_delete_forward(document, cursor, false),
            Action::DeleteWordForward => Self::handle_delete_forward(document, cursor, true),

            // ----- Whole-line operations -----
            Action::LinesMoveUp => Self::line_command(line_ops::move_lines(document, cursor, Up)),
            Action::LinesMoveDown => {
                Self::line_command(line_ops::move_lines(document, cursor, Down))
            },
            Action::LinesDuplicateUp => {
                Self::line_command(line_ops::duplicate_lines(document, cursor, Up))
            },
            Action::LinesDuplicateDown => {
                Self::line_command(line_ops::duplicate_lines(document, cursor, Down))
            },
            Action::LinesDelete => Self::line_command(line_ops::delete_lines(document, cursor)),
            Action::LinesJoin => Self::line_command(line_ops::join_lines(document, cursor)),

            // ----- Comments -----
            Action::ToggleLineComment => {
                Self::handle_toggle_line_comment(document, cursor, ctx.config)
            },
            Action::ToggleBlockComment => {
                Self::handle_toggle_block_comment(document, cursor, ctx.config)
            },

            // ----- Clipboard -----
            Action::ClipboardCopy => Self::handle_copy(document, cursor),
            Action::ClipboardCut => Self::handle_cut(document, cursor),
            Action::ClipboardPaste => KeyResult::Clipboard(ClipboardOperation::Paste),

            // ----- History -----
            Action::Undo => Self::handle_undo(ctx.history),
            Action::Redo => Self::handle_redo(ctx.history),

            // ----- Multi-cursor -----
            Action::AddSelectionToNextMatch => {
                self.handle_add_selection_next_match(document, cursor)
            },
            Action::SelectAllOccurrences => self.handle_select_all_occurrences(document, cursor),
            Action::RemoveLastCursor => self.handle_undo_last_cursor(document, cursor),
            Action::AddCursorAbove => self.handle_add_cursor_vertical(document, cursor, Up),
            Action::AddCursorBelow => self.handle_add_cursor_vertical(document, cursor, Down),
            Action::SkipLastOccurrence => self.skip_last_added_occurrence(document, cursor),

            // ----- General -----
            Action::NoOp => KeyResult::Handled,

            // ----- Search -----
            Action::SearchOpen => KeyResult::Search(SearchAction::OpenSearch),
            Action::SearchNextMatch => KeyResult::Search(SearchAction::NextMatch),
            Action::SearchPreviousMatch => KeyResult::Search(SearchAction::PreviousMatch),
        }
    }

    /// Turns an optional line-operation command into a [`KeyResult`].
    ///
    /// A line operation that has nothing to do (an empty document, a join on the
    /// last line) produces no command; the key is still consumed, exactly as
    /// before the registry.
    fn line_command(command: Option<Command>) -> KeyResult {
        command.map_or(KeyResult::Handled, KeyResult::Command)
    }
}
