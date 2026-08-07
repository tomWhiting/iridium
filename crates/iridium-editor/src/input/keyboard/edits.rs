//! Text-editing, clipboard, comment and history command implementations.
//!
//! Every implementation here is multi-cursor aware and produces at most one
//! reversible [`Command`], which the caller applies: nothing in this module
//! mutates a [`Document`].
//!
//! Behaviours that depend on configuration — tab stops, indent/outdent,
//! auto-indent on Enter, auto-closing pairs — live in [`super::behaviors`], and
//! comment syntax resolution lives in [`super::comments`]; these functions
//! choose between them and package the result.

use crate::commands::CommandArgs;
use crate::document::{CursorState, Document};
use crate::editor::EditorConfig;
use crate::history::Command;

use super::types::{ClipboardOperation, HistoryRequest, KeyResult};
use super::{KeyboardHandler, behaviors, comments, editing};

impl KeyboardHandler {
    /// Toggles line comments over every line any cursor touches.
    ///
    /// Each cursor's touched lines toggle as a group with VS Code semantics, with
    /// groups sharing a line merged so the outcome never depends on which cursor
    /// is primary (see [`comments::toggle_line_comment_edits`]); the whole toggle
    /// is one undoable command that remaps every cursor and selection. When no
    /// comment syntax is available (no language, a commentless language like
    /// JSON, and no configured fallback token) the key is acknowledged without
    /// editing.
    pub(super) fn handle_toggle_line_comment(
        document: &Document,
        cursor: &CursorState,
        config: &EditorConfig,
    ) -> KeyResult {
        let Some(syntax) = comments::resolve_comment_syntax(document, config) else {
            return KeyResult::Handled;
        };
        let edits = comments::toggle_line_comment_edits(document, cursor, &syntax);
        editing::build_line_edits_command(document, cursor, edits)
            .map_or(KeyResult::Handled, KeyResult::Command)
    }

    /// Toggles a block comment around every selection.
    ///
    /// Each selection is wrapped in the language's block pair or unwrapped when
    /// it is exactly wrapped already (see
    /// [`comments::toggle_block_comment_edits`]). Languages without a block pair
    /// fall back to the line toggle; with no comment syntax at all the key is
    /// acknowledged without editing.
    pub(super) fn handle_toggle_block_comment(
        document: &Document,
        cursor: &CursorState,
        config: &EditorConfig,
    ) -> KeyResult {
        let Some(syntax) = comments::resolve_comment_syntax(document, config) else {
            return KeyResult::Handled;
        };
        if let Some((open, close)) = &syntax.block {
            let edits = comments::toggle_block_comment_edits(document, cursor, open, close);
            editing::build_multi_cursor_command_placed(document, cursor, edits)
                .map_or(KeyResult::Handled, KeyResult::Command)
        } else {
            let edits = comments::toggle_line_comment_edits(document, cursor, &syntax);
            editing::build_line_edits_command(document, cursor, edits)
                .map_or(KeyResult::Handled, KeyResult::Command)
        }
    }

    /// Inserts the typed character at every cursor.
    ///
    /// When `config.auto_pairs` is enabled and the character participates in
    /// bracket/quote pairing, each cursor independently inserts the pair, skips
    /// over an existing closer, or wraps its selection (see
    /// [`behaviors::auto_pair_char_edits`]). All other characters are plain
    /// per-cursor insertions.
    pub(super) fn handle_char_input(
        c: char,
        document: &Document,
        cursor: &CursorState,
        config: &EditorConfig,
    ) -> KeyResult {
        if config.auto_pairs {
            // Built once here rather than inside the per-cursor loop: it costs
            // a manifest lookup, and the answer is the document's, not the
            // cursor's.
            let pairs = behaviors::AutoPairs::for_document(document);
            if pairs.is_trigger(c) {
                let edits = behaviors::auto_pair_char_edits(document, cursor, c, pairs);
                return editing::build_multi_cursor_command_placed(document, cursor, edits)
                    .map_or(KeyResult::Handled, KeyResult::Command);
            }
        }

        Self::insert_text(&c.to_string(), document, cursor)
    }

    /// Inserts a line break at every cursor.
    ///
    /// With `config.auto_indent`, each cursor's new line inherits its own line's
    /// leading whitespace, expands bracket blocks, and expands opening code
    /// fences (see [`behaviors::enter_edits`]). Without it, every cursor inserts
    /// the bare document line ending.
    pub(super) fn handle_enter(
        document: &Document,
        cursor: &CursorState,
        config: &EditorConfig,
    ) -> KeyResult {
        let edits = behaviors::enter_edits(document, cursor, config);
        editing::build_multi_cursor_command_placed(document, cursor, edits)
            .map_or(KeyResult::Handled, KeyResult::Command)
    }

    /// Indents or inserts a tab.
    ///
    /// - **With any non-collapsed selection**, indents every touched line by one
    ///   level, keeping selections covering the same text.
    /// - **With only collapsed cursors**, inserts a tab, or pads with spaces to
    ///   the next tab stop from each cursor's column when `insert_spaces` is set.
    pub(super) fn handle_tab(
        document: &Document,
        cursor: &CursorState,
        config: &EditorConfig,
    ) -> KeyResult {
        if cursor.all_selections().any(|sel| !sel.is_collapsed()) {
            let edits = behaviors::indent_edits(document, cursor, config);
            editing::build_line_edits_command(document, cursor, edits)
                .map_or(KeyResult::Handled, KeyResult::Command)
        } else {
            let edits = behaviors::tab_insert_edits(cursor, config);
            editing::build_multi_cursor_command(document, cursor, edits)
                .map_or(KeyResult::Handled, KeyResult::Command)
        }
    }

    /// Outdents every line touched by a cursor or selection by up to one level
    /// (a leading tab or up to `tab_width` leading spaces), never removing
    /// non-whitespace.
    pub(super) fn handle_outdent(
        document: &Document,
        cursor: &CursorState,
        config: &EditorConfig,
    ) -> KeyResult {
        let edits = behaviors::outdent_edits(document, cursor, config);
        editing::build_line_edits_command(document, cursor, edits)
            .map_or(KeyResult::Handled, KeyResult::Command)
    }

    /// Deletes the selection, or the character before each caret.
    ///
    /// Each cursor acts independently: cursors with a selection delete the
    /// selection, collapsed cursors delete the character before the caret. With
    /// `config.auto_pairs`, a collapsed cursor between the two halves of an empty
    /// pair deletes both halves. Converging cursors are merged.
    pub(super) fn handle_delete_backward(
        document: &Document,
        cursor: &CursorState,
        config: &EditorConfig,
    ) -> KeyResult {
        let edits = if config.auto_pairs {
            behaviors::backspace_edits_with_pairs(document, cursor)
        } else {
            editing::backspace_edits(document, cursor, false)
        };
        editing::build_multi_cursor_command(document, cursor, edits)
            .map_or(KeyResult::Handled, KeyResult::Command)
    }

    /// Deletes the selection, or the word before each caret.
    ///
    /// The auto-pair deletion of an empty pair is deliberately not applied to the
    /// word-wise variant: a word-wise delete already spans more than the two
    /// characters that rule is about.
    pub(super) fn handle_delete_word_backward(
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        let edits = editing::backspace_edits(document, cursor, true);
        editing::build_multi_cursor_command(document, cursor, edits)
            .map_or(KeyResult::Handled, KeyResult::Command)
    }

    /// Deletes the selection, or the character (`word` false) or word (`word`
    /// true) after each caret.
    ///
    /// Each cursor acts independently and converging cursors are merged.
    pub(super) fn handle_delete_forward(
        document: &Document,
        cursor: &CursorState,
        word: bool,
    ) -> KeyResult {
        let edits = editing::delete_forward_edits(document, cursor, word);
        editing::build_multi_cursor_command(document, cursor, edits)
            .map_or(KeyResult::Handled, KeyResult::Command)
    }

    /// Deletes the selection, or the text between each caret and the start of
    /// its own line (macOS `Cmd+Backspace`).
    ///
    /// Every cursor acts on *its own* line. The web face used to implement
    /// this verb by hand against the primary cursor alone, which both spared
    /// the other carets' lines and collapsed the multi-cursor state; routing it
    /// through the same builder as every other delete is what makes the three
    /// faces agree.
    pub(super) fn handle_delete_to_line_start(
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        let edits = editing::delete_to_line_start_edits(cursor);
        editing::build_multi_cursor_command(document, cursor, edits)
            .map_or(KeyResult::Handled, KeyResult::Command)
    }

    /// Deletes the selection, or the text between each caret and the end of its
    /// own line (macOS `Cmd+Delete`).
    ///
    /// The line ending survives, so the line is emptied rather than joined to
    /// the next.
    pub(super) fn handle_delete_to_line_end(
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        let edits = editing::delete_to_line_end_edits(document, cursor);
        editing::build_multi_cursor_command(document, cursor, edits)
            .map_or(KeyResult::Handled, KeyResult::Command)
    }

    /// Inserts text at every cursor, replacing any selections.
    ///
    /// Per-cursor position accounting (same-line byte shifts, inserted newlines,
    /// selection replacement) is handled by
    /// [`editing::build_multi_cursor_command`].
    pub(super) fn insert_text(text: &str, document: &Document, cursor: &CursorState) -> KeyResult {
        let edits = editing::replace_all_edits(cursor, text);
        editing::build_multi_cursor_command(document, cursor, edits)
            .map_or(KeyResult::Handled, KeyResult::Command)
    }

    /// Copies the selections to the host clipboard.
    ///
    /// With selections, the per-cursor selected texts are joined with the
    /// document line ending; with only collapsed cursors, each cursor's line is
    /// copied.
    pub(super) fn handle_copy(document: &Document, cursor: &CursorState) -> KeyResult {
        KeyResult::Clipboard(ClipboardOperation::Copy(editing::copy_text(
            document, cursor,
        )))
    }

    /// Cuts the selections: a copy plus a multi-cursor delete.
    ///
    /// The clipboard text always matches what copy would produce, even when
    /// nothing can be removed from the document (e.g. an empty document): the
    /// host clipboard is still updated, with a no-op command.
    pub(super) fn handle_cut(document: &Document, cursor: &CursorState) -> KeyResult {
        let text = editing::copy_text(document, cursor);
        let edits = editing::cut_edits(document, cursor);
        let command = editing::build_multi_cursor_command(document, cursor, edits).unwrap_or(
            Command::Compound {
                commands: Vec::new(),
            },
        );
        KeyResult::Clipboard(ClipboardOperation::Cut { text, command })
    }

    /// Requests an undo.
    ///
    /// The handler cannot perform it: it is handed the history by shared
    /// reference and never mutates editor state. Naming the request is the
    /// whole fix for the defect that used to live here — this returned
    /// [`KeyResult::Handled`], a bare acknowledgement, and each face undid by
    /// its own private route, so *Undo* run from a command palette did nothing
    /// at all and no keymap could rebind it.
    pub(super) const fn handle_undo() -> KeyResult {
        KeyResult::History(HistoryRequest::Undo)
    }

    /// Requests a redo along the tree's active path.
    pub(super) const fn handle_redo() -> KeyResult {
        KeyResult::History(HistoryRequest::Redo)
    }

    /// Requests a redo into a specific branch of the current node.
    ///
    /// The branch is the command's count argument, one-based as a count always
    /// is — `2` in a keymap means "the second branch" — and defaults to the
    /// first. An out-of-range index is not an error here: the tree reports it
    /// by leaving the document alone.
    pub(super) fn handle_redo_branch(args: &CommandArgs) -> KeyResult {
        let index = args.count().unwrap_or(1).max(1) - 1;
        KeyResult::History(HistoryRequest::RedoBranch(index as usize))
    }

    /// Requests that redo point at the next branch, without moving.
    pub(super) const fn handle_next_branch() -> KeyResult {
        KeyResult::History(HistoryRequest::NextBranch)
    }

    /// Requests that redo point at the previous branch, without moving.
    pub(super) const fn handle_previous_branch() -> KeyResult {
        KeyResult::History(HistoryRequest::PreviousBranch)
    }
}
