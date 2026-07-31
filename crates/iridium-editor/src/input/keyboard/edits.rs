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

use crate::document::{CursorState, Document};
use crate::editor::EditorConfig;
use crate::history::{Command, UndoTree};

use super::types::{ClipboardOperation, KeyResult};
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
        if config.auto_pairs && behaviors::is_auto_pair_trigger(c) {
            let edits = behaviors::auto_pair_char_edits(document, cursor, c);
            return editing::build_multi_cursor_command_placed(document, cursor, edits)
                .map_or(KeyResult::Handled, KeyResult::Command);
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

    /// Acknowledges an undo request.
    ///
    /// # Known defect, preserved deliberately
    ///
    /// This does not undo anything. The undo itself is executed at the editor
    /// level, and the only host that reaches it — the web binding — intercepts
    /// the undo chord *before* dispatch and calls the editor directly, so the
    /// handler only has to avoid reporting the key as unhandled input. A native
    /// host that routes the chord here gets nothing. Wiring this to
    /// [`UndoTree`] is a behaviour change and therefore out of scope for the
    /// keymap migration; it is recorded here rather than silently fixed.
    pub(super) const fn handle_undo(_history: &UndoTree) -> KeyResult {
        KeyResult::Handled
    }

    /// Acknowledges a redo request.
    ///
    /// Carries the same known defect as [`Self::handle_undo`]: the redo is
    /// executed at the editor level and this only acknowledges the key.
    pub(super) const fn handle_redo(_history: &UndoTree) -> KeyResult {
        KeyResult::Handled
    }
}
