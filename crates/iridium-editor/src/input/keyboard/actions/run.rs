//! The dispatch half of the action table: running one [`KeyboardAction`].
//!
//! Split from [`super`] because the two halves change for different reasons and
//! for different people. The parent is *declarative* — the enum and the id
//! table, which is what you read to learn what commands exist. This file is the
//! routing `match`, which is what you edit to change what one of them does.
//!
//! The `match` is exhaustive over the enum on purpose: adding a variant without
//! an arm here is a compile error, so a command can never be declared and then
//! silently do nothing.

use crate::history::Command;
use crate::text::case::CaseStyle;

use super::super::motions;
use super::super::motions::VerticalDirection::{Down, Up};
use super::super::transform::{CaseVerb, LineVerb};
use super::super::types::{AstRequest, ClipboardOperation, KeyCode, KeyResult, SearchAction};
use super::super::{KeyboardHandler, line_ops};
use super::{CommandContext, KeyboardAction};

use KeyboardAction as Action;

impl KeyboardHandler {
    /// Runs one command against `ctx` and returns what the caller must do.
    ///
    /// This is a pure routing table: every arm delegates to the same handler the
    /// pre-registry `match` on `(KeyCode, Modifiers)` called, with the modifier
    /// interpretation that used to live in the guards now expressed by the
    /// binding that selected the command. `Ctrl+Left` no longer means "word
    /// left because `ctrl` is set"; it means
    /// [`builtin::CURSOR_WORD_LEFT`](crate::commands::builtin::CURSOR_WORD_LEFT),
    /// and the keymap decided that.
    pub(in crate::input::keyboard) fn run_action(
        &mut self,
        action: KeyboardAction,
        ctx: &CommandContext<'_>,
    ) -> KeyResult {
        let document = ctx.document;
        let cursor = ctx.cursor;
        // The numeric prefix a key sequence carried, or 1. Only a keymap that
        // declares `with_count_prefix` on a binding can ever make this anything
        // else — no binding in the non-modal default does, so every arm below
        // behaves exactly as it did before counts existed unless a modal keymap
        // asked for one.
        let count = ctx.args.repeat_count();
        // The same figure as a hop size for the motions that take one as a
        // parameter rather than repeating. Saturating rather than wrapping: a
        // count larger than a `usize` is one hop off the end of the document,
        // which every vertical motion already clamps.
        let hops = usize::try_from(count).unwrap_or(usize::MAX);
        match action {
            // ----- Navigation and selection -----
            Action::CharLeft => Self::apply_repeated_motion(cursor, false, count, |head| {
                motions::char_left(document, head)
            }),
            Action::CharLeftSelect => Self::apply_repeated_motion(cursor, true, count, |head| {
                motions::char_left(document, head)
            }),
            Action::CharRight => Self::apply_repeated_motion(cursor, false, count, |head| {
                motions::char_right(document, head)
            }),
            Action::CharRightSelect => Self::apply_repeated_motion(cursor, true, count, |head| {
                motions::char_right(document, head)
            }),
            Action::WordLeft => Self::apply_repeated_motion(cursor, false, count, |head| {
                motions::word_left(document, head)
            }),
            Action::WordLeftSelect => Self::apply_repeated_motion(cursor, true, count, |head| {
                motions::word_left(document, head)
            }),
            Action::WordRight => Self::apply_repeated_motion(cursor, false, count, |head| {
                motions::word_right(document, head)
            }),
            Action::WordRightSelect => Self::apply_repeated_motion(cursor, true, count, |head| {
                motions::word_right(document, head)
            }),
            Action::LineStart => Self::apply_motion(cursor, false, |head| {
                motions::line_start_smart(document, head)
            }),
            Action::LineStartSelect => Self::apply_motion(cursor, true, |head| {
                motions::line_start_smart(document, head)
            }),
            Action::LineEnd => {
                Self::apply_motion(cursor, false, |head| motions::line_end(document, head))
            },
            Action::LineEndSelect => {
                Self::apply_motion(cursor, true, |head| motions::line_end(document, head))
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
            Action::LineUp => self.vertical_motion(document, cursor, false, Up, hops),
            Action::LineUpSelect => self.vertical_motion(document, cursor, true, Up, hops),
            Action::LineDown => self.vertical_motion(document, cursor, false, Down, hops),
            Action::LineDownSelect => self.vertical_motion(document, cursor, true, Down, hops),
            Action::PageUp => self.page_motion(document, cursor, false, Up, hops),
            Action::PageUpSelect => self.page_motion(document, cursor, true, Up, hops),
            Action::PageDown => self.page_motion(document, cursor, false, Down, hops),
            Action::PageDownSelect => self.page_motion(document, cursor, true, Down, hops),
            Action::SelectAll => Self::handle_select_all(document, cursor),
            Action::CollapseToPrimary => Self::handle_collapse_to_primary(cursor),

            // ----- Editing -----
            // The only command that reads the raw event. Invoked by id, with no
            // keystroke to take a character from, there is nothing to insert.
            Action::InsertCharacter => match ctx.event.map(|event| event.key) {
                Some(KeyCode::Char(c)) => {
                    self.handle_char_input(c, document, cursor, ctx.config, ctx.scopes)
                },
                _ => KeyResult::Ignored,
            },
            Action::InsertNewline => Self::handle_enter(document, cursor, ctx.config),
            Action::Tab => Self::handle_tab(document, cursor, ctx.config),
            Action::Outdent => Self::handle_outdent(document, cursor, ctx.config),
            Action::DeleteBackward => self.handle_delete_backward(document, cursor, ctx.config),
            Action::DeleteWordBackward => Self::handle_delete_word_backward(document, cursor),
            Action::DeleteForward => Self::handle_delete_forward(document, cursor, false),
            Action::DeleteWordForward => Self::handle_delete_forward(document, cursor, true),
            Action::DeleteToLineStart => Self::handle_delete_to_line_start(document, cursor),
            Action::DeleteToLineEnd => Self::handle_delete_to_line_end(document, cursor),

            // ----- Text transformations -----
            Action::TransformUpperCase => {
                Self::handle_case_transform(document, cursor, CaseVerb::Upper)
            },
            Action::TransformLowerCase => {
                Self::handle_case_transform(document, cursor, CaseVerb::Lower)
            },
            Action::TransformTitleCase => {
                Self::handle_case_transform(document, cursor, CaseVerb::Style(CaseStyle::Title))
            },
            Action::TransformToggleCase => {
                Self::handle_case_transform(document, cursor, CaseVerb::Toggle)
            },
            Action::TransformSwapCase => {
                Self::handle_case_transform(document, cursor, CaseVerb::Swap)
            },
            Action::TransformCamelCase => {
                Self::handle_case_transform(document, cursor, CaseVerb::Style(CaseStyle::Camel))
            },
            Action::TransformPascalCase => {
                Self::handle_case_transform(document, cursor, CaseVerb::Style(CaseStyle::Pascal))
            },
            Action::TransformSnakeCase => {
                Self::handle_case_transform(document, cursor, CaseVerb::Style(CaseStyle::Snake))
            },
            Action::TransformScreamingSnakeCase => Self::handle_case_transform(
                document,
                cursor,
                CaseVerb::Style(CaseStyle::ScreamingSnake),
            ),
            Action::TransformKebabCase => {
                Self::handle_case_transform(document, cursor, CaseVerb::Style(CaseStyle::Kebab))
            },
            Action::TransformSortLines => {
                Self::handle_line_transform(document, cursor, LineVerb::Sort)
            },
            Action::TransformSortLinesReverse => {
                Self::handle_line_transform(document, cursor, LineVerb::SortReverse)
            },
            Action::TransformReverseLines => {
                Self::handle_line_transform(document, cursor, LineVerb::Reverse)
            },
            Action::TransformDedupeLines => {
                Self::handle_line_transform(document, cursor, LineVerb::Dedupe)
            },
            Action::TransformTrimTrailingWhitespace => {
                Self::handle_line_transform(document, cursor, LineVerb::TrimTrailing)
            },

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
            Action::Undo => Self::handle_undo(),
            Action::Redo => Self::handle_redo(),
            Action::HistoryRedoBranch => Self::handle_redo_branch(&ctx.args),
            Action::HistoryNextBranch => Self::handle_next_branch(),
            Action::HistoryPreviousBranch => Self::handle_previous_branch(),

            // ----- Multi-cursor -----
            Action::AddSelectionToNextMatch => {
                self.handle_add_selection_next_match(document, cursor)
            },
            Action::SelectAllOccurrences => self.handle_select_all_occurrences(document, cursor),
            Action::RemoveLastCursor => self.handle_undo_last_cursor(document, cursor),
            Action::AddCursorAbove => self.handle_add_cursor_vertical(document, cursor, Up),
            Action::AddCursorBelow => self.handle_add_cursor_vertical(document, cursor, Down),
            Action::SkipLastOccurrence => self.skip_last_added_occurrence(document, cursor),

            // ----- Syntax -----
            //
            // Named, not performed: the answer lives in the editor's parse tree
            // and its expansion stack, neither of which the keyboard layer can
            // see. See `AstRequest`.
            Action::AstSelectNode => KeyResult::Ast(AstRequest::SelectNode),
            Action::AstExpandSelection => KeyResult::Ast(AstRequest::ExpandSelection),
            Action::AstShrinkSelection => KeyResult::Ast(AstRequest::ShrinkSelection),
            Action::AstSelectNextSibling => KeyResult::Ast(AstRequest::SelectNextSibling),
            Action::AstSelectPreviousSibling => KeyResult::Ast(AstRequest::SelectPreviousSibling),
            Action::AstSelectFirstChild => KeyResult::Ast(AstRequest::SelectFirstChild),
            Action::AstSelectLastChild => KeyResult::Ast(AstRequest::SelectLastChild),
            Action::AstExtendNextSibling => KeyResult::Ast(AstRequest::ExtendNextSibling),
            Action::AstExtendPreviousSibling => KeyResult::Ast(AstRequest::ExtendPreviousSibling),
            Action::AstCursorNodeStart => KeyResult::Ast(AstRequest::CursorNodeStart),
            Action::AstCursorNodeEnd => KeyResult::Ast(AstRequest::CursorNodeEnd),
            Action::AstCursorOnEverySibling => KeyResult::Ast(AstRequest::CursorOnEverySibling),
            Action::AstCursorOnEveryChild => KeyResult::Ast(AstRequest::CursorOnEveryChild),
            Action::AstSelectFunctionInside => KeyResult::Ast(AstRequest::SelectFunctionInside),
            Action::AstSelectFunctionAround => KeyResult::Ast(AstRequest::SelectFunctionAround),
            Action::AstSelectClassInside => KeyResult::Ast(AstRequest::SelectClassInside),
            Action::AstSelectClassAround => KeyResult::Ast(AstRequest::SelectClassAround),
            Action::AstSelectCommentAround => KeyResult::Ast(AstRequest::SelectCommentAround),
            Action::AstNextFunction => KeyResult::Ast(AstRequest::NextFunction),
            Action::AstPreviousFunction => KeyResult::Ast(AstRequest::PreviousFunction),
            Action::AstNextClass => KeyResult::Ast(AstRequest::NextClass),
            Action::AstPreviousClass => KeyResult::Ast(AstRequest::PreviousClass),

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
