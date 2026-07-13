//! Keyboard input handling.
//!
//! This module provides keyboard event handling for the editor, including:
//! - Arrow key navigation (with Ctrl for word-level, Shift for selection)
//! - Home/End keys (with Shift for selection)
//! - Character insertion
//! - Backspace/Delete
//! - Clipboard operations (Ctrl+C/X/V)
//! - Undo/Redo (Ctrl+Z, Ctrl+Y/Ctrl+Shift+Z)
//! - Line operations: move lines (Alt+Up/Down), duplicate lines
//!   (Shift+Alt+Up/Down), delete lines (Ctrl+Shift+K), join lines (Ctrl+J)
//! - Comment toggling: line comments (Ctrl+/), block comments (Shift+Alt+A)
//!
//! All editing and navigation operations are multi-cursor aware: every
//! cursor (primary and secondary) is edited or moved independently, and
//! cursors that converge on the same position are merged. The per-cursor
//! primitives live in [`editing`] and [`motions`] so other input paths (e.g.
//! paste handling in the editor core) can reuse them.
//!
//! Configuration-dependent behaviors — tab width and spaces-vs-tabs,
//! indent/outdent, auto-indent on Enter (including bracket-block and
//! code-fence expansion), and auto-closing pairs — live in [`behaviors`] and
//! are driven by the [`EditorConfig`] passed to [`KeyboardHandler::handle_key`].
//!
//! Comment toggling lives in [`comments`]: the comment syntax is resolved
//! from the document's language identifier ([`Document::language`]), falling
//! back to [`EditorConfig::line_comment_token`]; when neither is available
//! the toggle keys are acknowledged without editing.

mod behaviors;
mod comments;
pub mod editing;
mod line_ops;
pub mod motions;
mod multi_cursor;
mod types;

#[cfg(test)]
mod behavior_tests;
#[cfg(test)]
mod comment_tests;
#[cfg(test)]
mod line_ops_tests;
#[cfg(test)]
mod multi_cursor_tests;
#[cfg(test)]
mod tests;

pub use types::{ClipboardOperation, KeyCode, KeyEvent, KeyResult, Modifiers, SearchAction};

use std::collections::HashMap;

use crate::document::{CursorState, Document, Position, Range, Selection};
use crate::editor::EditorConfig;
use crate::history::{Command, UndoTree};

use motions::VerticalDirection;

/// Keyboard event handler for the editor.
///
/// This handler processes keyboard input and produces commands that can be
/// applied to the document. It handles:
/// - Navigation (arrow keys, Home/End, Page Up/Down)
/// - Selection (Shift+navigation)
/// - Text editing (character input, Backspace, Delete)
/// - Clipboard operations (Ctrl+C/X/V)
/// - Undo/Redo (Ctrl+Z, Ctrl+Y)
/// - Line operations (Alt+Up/Down move, Shift+Alt+Up/Down duplicate,
///   Ctrl+Shift+K delete, Ctrl+J join)
/// - Comment toggling (Ctrl+/ line comments, Shift+Alt+A block comments)
///
/// All operations act on every cursor of a multi-cursor [`CursorState`].
#[derive(Debug, Default)]
pub struct KeyboardHandler {
    /// Per-cursor preferred ("sticky") columns for vertical movement.
    ///
    /// Aligned with [`CursorState::all_selections`] order (primary first,
    /// then secondary cursors in document order) as captured by the last
    /// vertical move or vertical add. The vector lives on the handler rather
    /// than on `Selection` so the persisted cursor state stays free of
    /// transient input concerns.
    ///
    /// The columns are valid *only* for the exact cursor state recorded in
    /// [`Self::sticky_state`]. A vertical operation reuses them when the
    /// incoming cursor state equals that snapshot and rebuilds them from the
    /// current head columns otherwise. Validating by full cursor-state
    /// identity (rather than by cursor count) is what makes any intervening
    /// path self-invalidate the sticky column without an explicit reset: a
    /// horizontal motion, an edit, or a select-all produces a different
    /// cursor state, so the stale columns are ignored — while undo/redo that
    /// restores the *exact* prior cursor state also restores its sticky
    /// columns (they were never destroyed, only left dormant).
    preferred_columns: Option<Vec<usize>>,

    /// The cursor state that [`Self::preferred_columns`] describes, or `None`
    /// when no sticky columns are held. A vertical move/add trusts
    /// `preferred_columns` only while this equals the incoming cursor state.
    sticky_state: Option<CursorState>,

    /// The document content revision ([`Document::revision`]) the sticky
    /// columns were captured at, or `None` when untracked.
    ///
    /// A host-driven bare content edit can shift text under cursors *without*
    /// moving them (leaving [`Self::sticky_state`] matching), so validating the
    /// sticky columns also against the content revision discards them whenever
    /// the document changed — a vertical move after such an edit re-seeds from
    /// the live head column instead of a stale preferred column.
    sticky_revision: Option<u64>,

    /// Set when a non-vertical cursor mutation has happened since the sticky
    /// columns were captured, marking them stale without destroying them.
    ///
    /// Validity by cursor-state identity alone is defeated by any action that
    /// returns the cursor to the same coordinates (a left-then-right round
    /// trip, an edit that lands the caret back where it started): the columns
    /// would be wrongly resurrected. A horizontal motion, an edit, Escape, or a
    /// line operation sets this flag (see [`Self::note_operation`]); a vertical
    /// move/add ignores dirty columns and re-seeds from the live head. The flag
    /// is cleared only when new columns are stored, on a full reset, or when
    /// undo/redo restores the exact prior state (see
    /// [`Self::revalidate_vertical_columns`]) — so genuine history replay still
    /// revives the columns while a live round trip does not.
    sticky_dirty: bool,

    /// Secondary cursors in the order they were added, most recent last.
    ///
    /// Populated by the multi-cursor add verbs (Ctrl+D add-next-match,
    /// Ctrl+Alt+Up/Down add-above/below, Ctrl+Shift+L select-all-occurrences)
    /// and consumed by "undo last cursor" (Ctrl+U) and
    /// [`Self::skip_last_added_occurrence`], which pop the most recently added
    /// cursor. Each entry is the exact [`Selection`] of a secondary cursor.
    ///
    /// The stack is only meaningful while it describes the *current* cursor
    /// state; [`Self::add_order_anchor`] records the state it was last
    /// consistent with so any cursor change from another path (a motion, an
    /// edit, Escape, a host-driven `set_cursor`/`set_selection`, undo/redo)
    /// self-invalidates the stack the next time a multi-cursor verb runs. See
    /// [`Self::sync_add_order`].
    cursor_add_order: Vec<Selection>,

    /// The cursor state that [`Self::cursor_add_order`] was last consistent
    /// with (the state each add verb produced), or `None` when the stack is
    /// empty and untracked.
    ///
    /// Before a multi-cursor verb trusts the stack it compares the incoming
    /// cursor state against this snapshot; any mismatch means something else
    /// moved, added, or removed a cursor since the last verb, so the stack is
    /// cleared. This is what makes edits and motions unable to corrupt the
    /// addition order: they never update the anchor, so the very next verb
    /// discards the now-stale stack instead of removing or skipping the wrong
    /// cursor.
    add_order_anchor: Option<CursorState>,

    /// The document content revision ([`Document::revision`]) the stack was
    /// last consistent with, or `None` when untracked.
    ///
    /// A host-driven bare `Insert`/`Delete`/`Replace` can move text under the
    /// cursors *without* changing any cursor position, leaving
    /// [`Self::add_order_anchor`] matching while the bytes each stack entry
    /// points at have changed. Recording the revision alongside the anchor
    /// means the next verb also compares content revisions and discards the
    /// stack whenever the document changed, so skip/undo-cursor never slice a
    /// stale range as the search term.
    add_order_revision: Option<u64>,
}

impl KeyboardHandler {
    /// Creates a new keyboard handler.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            preferred_columns: None,
            sticky_state: None,
            sticky_revision: None,
            sticky_dirty: false,
            cursor_add_order: Vec::new(),
            add_order_anchor: None,
            add_order_revision: None,
        }
    }

    /// Handles a keyboard event.
    ///
    /// Returns a `KeyResult` indicating what action should be taken.
    /// The caller is responsible for applying any resulting commands
    /// and handling clipboard operations.
    ///
    /// `config` drives the editing behaviors: `tab_width`/`insert_spaces`
    /// for Tab, indent, and outdent; `auto_indent` for Enter (indent
    /// inheritance, bracket-block and code-fence expansion); `auto_pairs`
    /// for bracket/quote pairing; and `line_comment_token` as the comment
    /// syntax fallback for Ctrl+/ and Shift+Alt+A when the document's
    /// language provides none.
    pub fn handle_key(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
        history: &UndoTree,
        config: &EditorConfig,
    ) -> KeyResult {
        let result = self.dispatch_key(event, document, cursor, history, config);
        self.note_operation(event, &result);
        result
    }

    /// Dispatches a keyboard event to the matching handler.
    ///
    /// Split out from [`Self::handle_key`] so the post-dispatch bookkeeping in
    /// [`Self::note_operation`] runs uniformly for every key.
    fn dispatch_key(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
        history: &UndoTree,
        config: &EditorConfig,
    ) -> KeyResult {
        match event.key {
            // Alt+Up/Down move the cursors' line blocks; Shift+Alt+Up/Down
            // duplicate them. Plain and Shift arrows fall through to cursor
            // movement below.
            KeyCode::Up if Self::is_line_op_modifiers(event.modifiers) => {
                Self::handle_line_vertical(event, document, cursor, VerticalDirection::Up)
            },
            KeyCode::Down if Self::is_line_op_modifiers(event.modifiers) => {
                Self::handle_line_vertical(event, document, cursor, VerticalDirection::Down)
            },

            // Ctrl+Alt+Up/Down add a cursor above/below every existing cursor.
            // The line-op guard above deliberately excludes Ctrl, so these
            // never collide with Alt (move) or Shift+Alt (duplicate).
            KeyCode::Up if Self::is_add_cursor_modifiers(event.modifiers) => {
                self.handle_add_cursor_vertical(document, cursor, VerticalDirection::Up)
            },
            KeyCode::Down if Self::is_add_cursor_modifiers(event.modifiers) => {
                self.handle_add_cursor_vertical(document, cursor, VerticalDirection::Down)
            },

            // Arrow key navigation
            KeyCode::Left => Self::handle_left(event, document, cursor),
            KeyCode::Right => Self::handle_right(event, document, cursor),
            KeyCode::Up => self.handle_up(event, document, cursor),
            KeyCode::Down => self.handle_down(event, document, cursor),

            // Home/End
            KeyCode::Home => Self::handle_home(event, document, cursor),
            KeyCode::End => Self::handle_end(event, document, cursor),

            // Shift+Alt+A toggles block comments. Ctrl and Meta must be
            // absent: AltGr layouts report Ctrl+Alt and compose characters,
            // which must keep passing through untouched.
            //
            // Contract: hosts forward the BASE logical letter for modifier
            // chords (a platform where Alt resolves the logical key to a
            // special character, e.g. macOS Shift+Option+A -> "Å", must
            // normalize to the physical base letter before dispatch — the
            // web controller does this in translateKeyEvent).
            KeyCode::Char(c)
                if c.eq_ignore_ascii_case(&'a')
                    && event.modifiers.alt
                    && event.modifiers.shift
                    && !event.modifiers.ctrl
                    && !event.modifiers.meta =>
            {
                Self::handle_toggle_block_comment(document, cursor, config)
            },

            // Character input. Ctrl+character shortcuts require that
            // neither Alt nor Meta is also held: compound chords like
            // Ctrl+Alt+J or Ctrl+Meta+K are reserved for the host/OS, and
            // layouts reporting AltGr as Ctrl+Alt compose characters with
            // those modifiers — both must pass through untouched (they fall
            // to the `Char(_)` arm below), never mutate the document.
            KeyCode::Char(c)
                if event.modifiers.ctrl && !event.modifiers.alt && !event.modifiers.meta =>
            {
                self.handle_ctrl_char(c, event.modifiers.shift, document, cursor, history, config)
            },
            KeyCode::Char(c) if !event.modifiers.alt && !event.modifiers.meta => {
                Self::handle_char_input(c, document, cursor, config)
            },

            // Enter key
            KeyCode::Enter => Self::handle_enter(document, cursor, config),

            // Tab key
            KeyCode::Tab => Self::handle_tab(event, document, cursor, config),

            // Delete operations
            KeyCode::Backspace => Self::handle_backspace(event, document, cursor, config),
            KeyCode::Delete => Self::handle_delete(event, document, cursor),

            // Escape - collapse to primary cursor
            KeyCode::Escape => Self::handle_escape(cursor),

            // F3 - Next match (T122)
            KeyCode::F3 if !event.modifiers.shift => KeyResult::Search(SearchAction::NextMatch),
            // Shift+F3 - Previous match (T122)
            KeyCode::F3 if event.modifiers.shift => KeyResult::Search(SearchAction::PreviousMatch),

            // Everything else: Page Up/Down are handled by the viewport,
            // function keys and modifier-only keys are not handled, and
            // characters with Alt or Meta modifiers — including Ctrl+Alt
            // and Ctrl+Meta chords — pass through. (The bare F3 arm is
            // unreachable due to the guards above but keeps the match
            // exhaustive.)
            KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::F1
            | KeyCode::F2
            | KeyCode::F3
            | KeyCode::F4
            | KeyCode::F5
            | KeyCode::F6
            | KeyCode::F7
            | KeyCode::F8
            | KeyCode::F9
            | KeyCode::F10
            | KeyCode::F11
            | KeyCode::F12
            | KeyCode::Shift
            | KeyCode::Control
            | KeyCode::Alt
            | KeyCode::Meta
            | KeyCode::Char(_) => KeyResult::Ignored,
        }
    }

    /// Handles pasting text from clipboard.
    ///
    /// This is called by the editor when clipboard content is available.
    /// The text is inserted at every cursor.
    pub fn handle_paste(&self, text: &str, document: &Document, cursor: &CursorState) -> KeyResult {
        Self::insert_text(text, document, cursor)
    }

    /// Clears the sticky-column state used for vertical (Up/Down) movement.
    ///
    /// Sticky columns are validated against the exact cursor state they were
    /// captured for (see [`Self::preferred_columns`]), so most cursor changes
    /// self-invalidate them. This explicit reset is for host-driven cursor
    /// jumps that bypass [`Self::handle_key`] — `set_cursor`/`set_selection`,
    /// mouse-applied commands, IME commits, paste — where the caller wants a
    /// fresh sticky column even if the new state happened to coincide with a
    /// dormant one. Undo/redo deliberately does **not** call this: restoring
    /// the exact prior cursor state must also restore its sticky columns.
    pub fn reset_vertical_state(&mut self) {
        self.preferred_columns = None;
        self.sticky_state = None;
        self.sticky_revision = None;
        self.sticky_dirty = false;
    }

    /// Revives the sticky columns after an undo/redo replay that restored the
    /// exact cursor state they describe.
    ///
    /// History navigation that returns the cursor to the state the sticky
    /// columns were captured for should also revive them (the enclosing content
    /// edit had marked them dirty). This clears the dirty flag and, when the
    /// stored [`Self::sticky_state`] still equals the restored `cursor`,
    /// re-stamps [`Self::sticky_revision`] to the replayed document's
    /// `revision` — because undo/redo bump the revision even when they restore
    /// identical content, the columns' original revision would otherwise no
    /// longer match. When the stored state does not match the restored cursor,
    /// only the flag is cleared and the (now mismatched) columns are ignored by
    /// [`Self::take_sticky_columns_for`], so a wrong set can never be revived.
    pub fn revalidate_vertical_columns(&mut self, cursor: &CursorState, revision: u64) {
        self.sticky_dirty = false;
        if self.sticky_state.as_ref() == Some(cursor) {
            self.sticky_revision = Some(revision);
        }
    }

    /// Discards the multi-cursor addition-order stack.
    ///
    /// Called by the editor from every path that mutates the cursor without
    /// going through an add verb — host `set_cursor`/`set_selection`, mouse
    /// clicks, IME commits, paste, search navigation, and undo/redo replay.
    /// Unlike the value-equality snapshot in [`Self::sync_add_order`] (which a
    /// sequence of mutations returning to the same state can defeat), this
    /// eager clear cannot be fooled by a round trip: once any such path runs,
    /// "undo last cursor" (Ctrl+U) and skip find an empty stack and no-op
    /// rather than removing an arbitrarily-ordered cursor.
    pub fn invalidate_cursor_order(&mut self) {
        self.cursor_add_order.clear();
        self.add_order_anchor = None;
        self.add_order_revision = None;
    }

    // ========== Navigation handlers ==========

    /// Handles left arrow key.
    fn handle_left(event: &KeyEvent, document: &Document, cursor: &CursorState) -> KeyResult {
        let new_cursor = if event.modifiers.ctrl {
            motions::apply_to_all(cursor, event.modifiers.shift, |_, sel| {
                motions::word_left(document, sel.head)
            })
        } else {
            motions::apply_to_all(cursor, event.modifiers.shift, |_, sel| {
                motions::char_left(document, sel.head)
            })
        };

        Self::create_selection_command(cursor, &new_cursor)
    }

    /// Handles right arrow key.
    fn handle_right(event: &KeyEvent, document: &Document, cursor: &CursorState) -> KeyResult {
        let new_cursor = if event.modifiers.ctrl {
            motions::apply_to_all(cursor, event.modifiers.shift, |_, sel| {
                motions::word_right(document, sel.head)
            })
        } else {
            motions::apply_to_all(cursor, event.modifiers.shift, |_, sel| {
                motions::char_right(document, sel.head)
            })
        };

        Self::create_selection_command(cursor, &new_cursor)
    }

    /// Handles up arrow key.
    fn handle_up(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        let new_cursor = self.move_vertically(
            document,
            cursor,
            event.modifiers.shift,
            VerticalDirection::Up,
        );
        Self::create_selection_command(cursor, &new_cursor)
    }

    /// Handles down arrow key.
    fn handle_down(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        let new_cursor = self.move_vertically(
            document,
            cursor,
            event.modifiers.shift,
            VerticalDirection::Down,
        );
        Self::create_selection_command(cursor, &new_cursor)
    }

    /// Handles Home key.
    fn handle_home(event: &KeyEvent, document: &Document, cursor: &CursorState) -> KeyResult {
        let new_cursor = if event.modifiers.ctrl {
            // Move all cursors to the start of the document (they merge into one).
            motions::apply_to_all(cursor, event.modifiers.shift, |_, _| {
                motions::document_start()
            })
        } else {
            // Move each cursor to the start of its line (smart home).
            motions::apply_to_all(cursor, event.modifiers.shift, |_, sel| {
                motions::line_start_smart(document, sel.head)
            })
        };

        Self::create_selection_command(cursor, &new_cursor)
    }

    /// Handles End key.
    fn handle_end(event: &KeyEvent, document: &Document, cursor: &CursorState) -> KeyResult {
        let new_cursor = if event.modifiers.ctrl {
            // Move all cursors to the end of the document (they merge into one).
            motions::apply_to_all(cursor, event.modifiers.shift, |_, _| {
                motions::document_end(document)
            })
        } else {
            // Move each cursor to the end of its line.
            motions::apply_to_all(cursor, event.modifiers.shift, |_, sel| {
                motions::line_end(document, sel.head)
            })
        };

        Self::create_selection_command(cursor, &new_cursor)
    }

    /// Moves every cursor one line up or down with per-cursor sticky columns.
    fn move_vertically(
        &mut self,
        document: &Document,
        cursor: &CursorState,
        extend_selection: bool,
        direction: VerticalDirection,
    ) -> CursorState {
        let count = cursor.cursor_count();
        let revision = document.revision();

        // Reuse the sticky columns from the previous vertical move only when
        // they were captured for this exact cursor state; otherwise start from
        // the current head columns.
        let mut preferred = self.take_sticky_columns_for(cursor, revision);

        let new_cursor = motions::apply_to_all(cursor, extend_selection, |index, sel| {
            let target = preferred.get(index).copied().unwrap_or(sel.head.column);
            let (head, sticky) = motions::vertical_move(document, sel.head, target, direction);
            if let Some(slot) = preferred.get_mut(index) {
                *slot = sticky;
            }
            head
        });

        // If cursors merged, the per-cursor mapping is gone; rebuild from the
        // actual columns on the next vertical move. Otherwise remember the
        // columns keyed to the state they now describe.
        if new_cursor.cursor_count() == count {
            self.store_sticky_columns(&new_cursor, preferred, revision);
        } else {
            self.preferred_columns = None;
            self.sticky_state = None;
            self.sticky_revision = None;
            self.sticky_dirty = false;
        }

        new_cursor
    }

    /// Returns the sticky columns to seed a vertical operation for `cursor`.
    ///
    /// When [`Self::preferred_columns`] was captured for exactly this cursor
    /// state (and still lines up with it), those columns are taken; otherwise
    /// a fresh vector of the current head columns is returned. Either way the
    /// stored sticky state is consumed, to be re-stored against the resulting
    /// state by [`Self::store_sticky_columns`].
    fn take_sticky_columns_for(&mut self, cursor: &CursorState, revision: u64) -> Vec<usize> {
        let reusable = !self.sticky_dirty
            && self.sticky_state.as_ref() == Some(cursor)
            && self.sticky_revision == Some(revision)
            && self
                .preferred_columns
                .as_ref()
                .is_some_and(|columns| columns.len() == cursor.cursor_count());
        self.sticky_state = None;
        self.sticky_revision = None;
        self.sticky_dirty = false;
        if reusable {
            if let Some(columns) = self.preferred_columns.take() {
                return columns;
            }
        }
        self.preferred_columns = None;
        cursor.all_selections().map(|sel| sel.head.column).collect()
    }

    /// Records `columns` as the sticky columns for `state` (the cursor state a
    /// vertical operation just produced) at content revision `revision`,
    /// aligned to its [`CursorState::all_selections`] order. The freshly stored
    /// columns are clean (not dirty).
    fn store_sticky_columns(&mut self, state: &CursorState, columns: Vec<usize>, revision: u64) {
        self.preferred_columns = Some(columns);
        self.sticky_state = Some(state.clone());
        self.sticky_revision = Some(revision);
        self.sticky_dirty = false;
    }

    /// Returns true when the modifier set selects a line operation on
    /// Up/Down: Alt (move lines) or Shift+Alt (duplicate lines), without
    /// Ctrl or Meta (reserved for other bindings such as add-cursor).
    const fn is_line_op_modifiers(modifiers: Modifiers) -> bool {
        modifiers.alt && !modifiers.ctrl && !modifiers.meta
    }

    /// Handles Alt+Up/Down (move line blocks) and Shift+Alt+Up/Down
    /// (duplicate lines/selections).
    ///
    /// Both operations act on every cursor and produce a single undoable
    /// command; see [`line_ops::move_lines`] and [`line_ops::duplicate_lines`].
    fn handle_line_vertical(
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
        direction: VerticalDirection,
    ) -> KeyResult {
        let command = if event.modifiers.shift {
            line_ops::duplicate_lines(document, cursor, direction)
        } else {
            line_ops::move_lines(document, cursor, direction)
        };
        command.map_or(KeyResult::Handled, KeyResult::Command)
    }

    /// Handles Escape key - collapse to primary cursor.
    fn handle_escape(cursor: &CursorState) -> KeyResult {
        if cursor.secondary.is_empty() && cursor.primary.is_collapsed() {
            return KeyResult::Handled;
        }

        let mut new_cursor = cursor.clone();
        new_cursor.collapse_to_primary();
        new_cursor.primary = Selection::collapsed(cursor.primary.head);

        Self::create_selection_command(cursor, &new_cursor)
    }

    // ========== Editing handlers ==========

    /// Handles Ctrl+key combinations.
    ///
    /// `shift` distinguishes the shifted bindings: Ctrl+Shift+K deletes each
    /// cursor's lines, while Ctrl+J (unshifted) joins each cursor's line
    /// with the next. `config` drives comment toggling (Ctrl+/), which falls
    /// back to `config.line_comment_token` when the document's language
    /// provides no comment syntax.
    fn handle_ctrl_char(
        &mut self,
        c: char,
        shift: bool,
        document: &Document,
        cursor: &CursorState,
        history: &UndoTree,
        config: &EditorConfig,
    ) -> KeyResult {
        match c.to_ascii_lowercase() {
            'c' => Self::handle_copy(document, cursor),
            'x' => Self::handle_cut(document, cursor),
            'v' => KeyResult::Clipboard(ClipboardOperation::Paste),
            'z' => Self::handle_undo(history),
            'y' => Self::handle_redo(history),
            'a' => Self::handle_select_all(document, cursor),
            'd' => self.handle_add_selection_next_match(document, cursor), // T107
            'l' if shift => self.handle_select_all_occurrences(document, cursor),
            'u' if !shift => self.handle_undo_last_cursor(document, cursor),
            'f' => KeyResult::Search(SearchAction::OpenSearch), // T121
            'k' if shift => line_ops::delete_lines(document, cursor)
                .map_or(KeyResult::Handled, KeyResult::Command),
            'j' if !shift => line_ops::join_lines(document, cursor)
                .map_or(KeyResult::Handled, KeyResult::Command),
            '/' => Self::handle_toggle_line_comment(document, cursor, config),
            _ => KeyResult::Ignored,
        }
    }

    /// Handles Ctrl+/ — toggle line comment.
    ///
    /// Each cursor's touched lines toggle as a group with VS Code
    /// semantics, with groups sharing a line merged so the outcome never
    /// depends on which cursor is primary (see
    /// [`comments::toggle_line_comment_edits`]); the whole toggle is one
    /// undoable command that remaps every cursor and selection. When no comment syntax is available (no language, a
    /// commentless language like JSON, and no configured fallback token)
    /// the key is acknowledged without editing.
    fn handle_toggle_line_comment(
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

    /// Handles Shift+Alt+A — toggle block comment.
    ///
    /// Each selection is wrapped in the language's block pair or unwrapped
    /// when it is exactly wrapped already (see
    /// [`comments::toggle_block_comment_edits`]). Languages without a block
    /// pair fall back to the line toggle; with no comment syntax at all the
    /// key is acknowledged without editing.
    fn handle_toggle_block_comment(
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

    /// Handles character input.
    ///
    /// When `config.auto_pairs` is enabled and the character participates in
    /// bracket/quote pairing, each cursor independently inserts the pair,
    /// skips over an existing closer, or wraps its selection (see
    /// [`behaviors::auto_pair_char_edits`]). All other characters are plain
    /// per-cursor insertions.
    fn handle_char_input(
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

    /// Handles Enter key.
    ///
    /// With `config.auto_indent`, each cursor's new line inherits its own
    /// line's leading whitespace, expands bracket blocks, and expands opening
    /// code fences (see [`behaviors::enter_edits`]). Without it, every cursor
    /// inserts the bare document line ending.
    fn handle_enter(document: &Document, cursor: &CursorState, config: &EditorConfig) -> KeyResult {
        let edits = behaviors::enter_edits(document, cursor, config);
        editing::build_multi_cursor_command_placed(document, cursor, edits)
            .map_or(KeyResult::Handled, KeyResult::Command)
    }

    /// Handles Tab and Shift+Tab.
    ///
    /// - **Shift+Tab** outdents every line touched by any cursor or
    ///   selection by up to one level (a leading tab or up to `tab_width`
    ///   leading spaces), never removing non-whitespace.
    /// - **Tab with any non-collapsed selection** indents every touched line
    ///   by one level, keeping selections covering the same text.
    /// - **Tab with only collapsed cursors** inserts a tab, or pads with
    ///   spaces to the next tab stop from each cursor's column when
    ///   `insert_spaces` is set.
    fn handle_tab(
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
        config: &EditorConfig,
    ) -> KeyResult {
        if event.modifiers.shift {
            let edits = behaviors::outdent_edits(document, cursor, config);
            return editing::build_line_edits_command(document, cursor, edits)
                .map_or(KeyResult::Handled, KeyResult::Command);
        }

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

    /// Handles Backspace key.
    ///
    /// Each cursor acts independently: cursors with a selection delete the
    /// selection, collapsed cursors delete the character (or word with Ctrl)
    /// before the caret. With `config.auto_pairs`, a collapsed cursor between
    /// the two halves of an empty pair deletes both halves. Converging
    /// cursors are merged.
    fn handle_backspace(
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
        config: &EditorConfig,
    ) -> KeyResult {
        let edits = if config.auto_pairs && !event.modifiers.ctrl {
            behaviors::backspace_edits_with_pairs(document, cursor)
        } else {
            editing::backspace_edits(document, cursor, event.modifiers.ctrl)
        };
        editing::build_multi_cursor_command(document, cursor, edits)
            .map_or(KeyResult::Handled, KeyResult::Command)
    }

    /// Handles Delete key.
    ///
    /// Each cursor acts independently: cursors with a selection delete the
    /// selection, collapsed cursors delete the character (or word with Ctrl)
    /// after the caret. Converging cursors are merged.
    fn handle_delete(event: &KeyEvent, document: &Document, cursor: &CursorState) -> KeyResult {
        let edits = editing::delete_forward_edits(document, cursor, event.modifiers.ctrl);
        editing::build_multi_cursor_command(document, cursor, edits)
            .map_or(KeyResult::Handled, KeyResult::Command)
    }

    /// Inserts text at every cursor, replacing any selections.
    ///
    /// Per-cursor position accounting (same-line byte shifts, inserted
    /// newlines, selection replacement) is handled by
    /// [`editing::build_multi_cursor_command`].
    fn insert_text(text: &str, document: &Document, cursor: &CursorState) -> KeyResult {
        let edits = editing::replace_all_edits(cursor, text);
        editing::build_multi_cursor_command(document, cursor, edits)
            .map_or(KeyResult::Handled, KeyResult::Command)
    }

    // ========== Clipboard operations ==========

    /// Handles copy operation.
    ///
    /// With selections, the per-cursor selected texts are joined with the
    /// document line ending; with only collapsed cursors, each cursor's line
    /// is copied.
    fn handle_copy(document: &Document, cursor: &CursorState) -> KeyResult {
        KeyResult::Clipboard(ClipboardOperation::Copy(editing::copy_text(
            document, cursor,
        )))
    }

    /// Handles cut operation (copy plus multi-cursor delete).
    ///
    /// The clipboard text always matches what copy would produce, even when
    /// nothing can be removed from the document (e.g. an empty document):
    /// the host clipboard is still updated, with a no-op command.
    fn handle_cut(document: &Document, cursor: &CursorState) -> KeyResult {
        let text = editing::copy_text(document, cursor);
        let edits = editing::cut_edits(document, cursor);
        let command = editing::build_multi_cursor_command(document, cursor, edits).unwrap_or(
            Command::Compound {
                commands: Vec::new(),
            },
        );
        KeyResult::Clipboard(ClipboardOperation::Cut { text, command })
    }

    /// Handles undo operation.
    ///
    /// The undo itself is executed at the editor level; the handler only
    /// acknowledges the key so it is not treated as unhandled input.
    const fn handle_undo(_history: &UndoTree) -> KeyResult {
        KeyResult::Handled
    }

    /// Handles redo operation.
    ///
    /// The redo itself is executed at the editor level; the handler only
    /// acknowledges the key so it is not treated as unhandled input.
    const fn handle_redo(_history: &UndoTree) -> KeyResult {
        KeyResult::Handled
    }

    // ========== Multi-selection (Ctrl+A / Ctrl+D) ==========

    /// Handles select all operation.
    fn handle_select_all(document: &Document, cursor: &CursorState) -> KeyResult {
        let start = motions::document_start();
        let end = motions::document_end(document);

        let new_cursor = CursorState::new(Selection::new(start, end));

        KeyResult::Command(Command::SetSelection {
            old_state: cursor.clone(),
            new_state: new_cursor,
        })
    }

    /// Handles Ctrl+D - Add Selection to Next Match (T107).
    ///
    /// If no selection exists, selects the word at cursor.
    /// If a selection exists, finds the next occurrence and adds a cursor there.
    ///
    /// A genuinely added cursor is recorded on the addition-order stack (see
    /// [`Self::record_cursor_change`]) so "undo last cursor" (Ctrl+U) and
    /// [`Self::skip_last_added_occurrence`] can later drop it.
    fn handle_add_selection_next_match(
        &mut self,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        self.sync_add_order(cursor, document.revision());

        // Get the search text - either from selection or select word at cursor
        let (search_text, initial_selection) = if cursor.primary.is_collapsed() {
            // No selection - select word at cursor first
            let word_sel = motions::select_word_at(document, cursor.primary.head);
            if word_sel.is_collapsed() {
                return KeyResult::Handled; // No word to select
            }
            let text = document.slice(word_sel.range());
            (text, Some(word_sel))
        } else {
            // Use existing selection
            let text = document.slice(cursor.primary.range());
            (text, None)
        };

        if search_text.is_empty() {
            return KeyResult::Handled;
        }

        let mut new_cursor = cursor.clone();

        // If we had to select a word first, do that (no cursor is added yet).
        if let Some(sel) = initial_selection {
            new_cursor.primary = sel;
            return self.finish_cursor_command(cursor, &new_cursor);
        }

        // Find the next occurrence that is not already selected, starting after
        // the last cursor and wrapping. Using the shared occurrence scan (which
        // skips ranges already `taken`) rather than a bare first-match search
        // means Ctrl+D composes with skip: after skip leaves an earlier
        // occurrence unselected, the next Ctrl+D fills that gap instead of
        // stopping on the first (already selected) match.
        let search_start = Self::last_selection_end(&new_cursor);
        let after = document.position_to_offset(search_start).unwrap_or(0);
        let taken: Vec<Range> = new_cursor.all_selections().map(Selection::range).collect();

        let Some(next) = multi_cursor::next_occurrence(document, &search_text, after, &taken)
        else {
            // Every occurrence is already selected (or there is only one).
            return KeyResult::Handled;
        };

        new_cursor.add_cursor(next);
        self.finish_cursor_command(cursor, &new_cursor)
    }

    /// Gets the position after the last selection (for Ctrl+D search).
    fn last_selection_end(cursor: &CursorState) -> Position {
        let mut last_end = cursor.primary.end();
        for sel in &cursor.secondary {
            if sel.end() > last_end {
                last_end = sel.end();
            }
        }
        last_end
    }

    /// Handles Ctrl+Shift+L - select all occurrences of the primary
    /// selection's text (or the word under a collapsed caret) as cursors.
    ///
    /// Matching is case-sensitive and exact; occurrences are non-overlapping
    /// (as with Ctrl+D). The reference selection stays primary, keeping its
    /// exact range **and direction** (a backward selection stays backward, so
    /// the active head does not jump). Every other occurrence that does not
    /// overlap the primary becomes its own distinct secondary cursor —
    /// including occurrences adjacent to each other, which stay separate edit
    /// targets rather than collapsing into one. A source that is empty (an
    /// empty selection or no word under the caret) is a no-op; an explicit
    /// non-empty selection is always honored, whitespace included, matching
    /// Ctrl+D's contract.
    fn handle_select_all_occurrences(
        &mut self,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        self.sync_add_order(cursor, document.revision());

        // Resolve the reference occurrence: the primary selection (kept
        // verbatim, direction included), or the word under a collapsed caret
        // (reusing the Ctrl+D word logic).
        let reference = if cursor.primary.is_collapsed() {
            let word = motions::select_word_at(document, cursor.primary.head);
            if word.is_collapsed() {
                return KeyResult::Handled;
            }
            word
        } else {
            cursor.primary
        };

        let source = document.slice(reference.range());
        if source.is_empty() {
            return KeyResult::Handled;
        }

        // Seed the occupied set with the exact primary range so the reference
        // selection is preserved verbatim (range and direction) while every
        // other occurrence that does not overlap it is returned as a distinct,
        // document-ordered secondary. The occurrence scanner keeps adjacent
        // matches separate (e.g. the two "foo" halves of "foofoo") and anchors
        // to the primary rather than a zero-based partition, so a primary that
        // does not fall on a non-overlapping boundary (columns 1..3 of "aaa")
        // still yields the correct set. Building the secondary list directly —
        // never via `add_cursor`, whose adjacency merge would fuse touching
        // occurrences — keeps every match its own edit target. The occurrences
        // are non-overlapping by construction, so the state upholds the
        // `CursorState` invariants.
        let secondary =
            multi_cursor::occurrences_excluding(document, &source, &[reference.range()]);

        // Always construct the desired state and route it through
        // finish_cursor_command; never short-circuit on an empty secondary.
        // When the reference is a uniquely occurring word under a collapsed
        // caret, there are no other matches yet the caret must still expand to
        // select that single occurrence. And when an unrelated secondary cursor
        // exists (added by a vertical/mouse path), select-all must replace the
        // whole set with just the reference occurrence rather than preserving
        // that unrelated cursor as a stray edit target. `create_selection_command`
        // still collapses to a no-op when the constructed state equals the
        // incoming one (e.g. the reference already is the sole cursor and has no
        // other occurrences).
        let new_state = CursorState {
            primary: reference,
            secondary,
        };

        self.finish_cursor_command(cursor, &new_state)
    }

    /// Handles Ctrl+U - remove the most recently added secondary cursor.
    ///
    /// The cursor to remove is the top of the addition-order stack (see
    /// [`Self::cursor_add_order`]). With a single cursor, or when the stack
    /// has been invalidated by an intervening motion or edit, this is a no-op.
    fn handle_undo_last_cursor(&mut self, document: &Document, cursor: &CursorState) -> KeyResult {
        self.sync_add_order(cursor, document.revision());

        let Some(target) = self.cursor_add_order.pop() else {
            // Nothing tracked to remove (single cursor, or the stack was
            // invalidated since the last add verb).
            return KeyResult::Handled;
        };

        let new_state = Self::remove_secondary(cursor, &target);
        self.add_order_anchor = Some(new_state.clone());
        Self::create_selection_command(cursor, &new_state)
    }

    /// Handles Ctrl+Alt+Up / Ctrl+Alt+Down - add a cursor above/below every
    /// existing cursor.
    ///
    /// Each existing cursor is cloned one line in `direction` at its sticky
    /// preferred column (clamped to the target line's length); cloning every
    /// cursor and merging collisions means repeated presses extend each
    /// contiguous block by one line. Cursors at the document edge (the first
    /// line going up, the last line going down) contribute nothing — there is
    /// no wraparound. Integrates with the same [`Self::preferred_columns`]
    /// sticky-column state as plain vertical movement.
    fn handle_add_cursor_vertical(
        &mut self,
        document: &Document,
        cursor: &CursorState,
        direction: VerticalDirection,
    ) -> KeyResult {
        let revision = document.revision();
        self.sync_add_order(cursor, revision);

        // Reuse the sticky columns from a previous vertical move/add only when
        // they were captured for this exact cursor state; otherwise seed from
        // the current head columns.
        let preferred: Vec<usize> = self.take_sticky_columns_for(cursor, revision);

        // Clone each cursor one line in `direction`, keeping its sticky column.
        let last_line = document.line_count().saturating_sub(1);
        let mut clones: Vec<(Selection, usize)> = Vec::new();
        for (index, sel) in cursor.all_selections().enumerate() {
            let sticky = preferred.get(index).copied().unwrap_or(sel.head.column);
            let clone_line = match direction {
                VerticalDirection::Up => {
                    if sel.head.line == 0 {
                        continue; // At the top: no cursor above.
                    }
                    sel.head.line - 1
                },
                VerticalDirection::Down => {
                    if sel.head.line >= last_line {
                        continue; // At the bottom: no cursor below.
                    }
                    sel.head.line + 1
                },
            };
            let line_len = document.line_len(clone_line).unwrap_or(0);
            let clone_pos = Position::new(clone_line, sticky.min(line_len));
            clones.push((Selection::collapsed(clone_pos), sticky));
        }

        if clones.is_empty() {
            // Every cursor was already at the document edge: nothing to add,
            // but the cursor state is unchanged so its sticky columns remain
            // valid for a subsequent vertical operation.
            self.store_sticky_columns(cursor, preferred, revision);
            return KeyResult::Handled;
        }

        // Build the new cursor state: all originals plus the clones, merged.
        let mut new_state = CursorState::new(cursor.primary);
        for sel in &cursor.secondary {
            new_state.add_cursor(*sel);
        }
        for (sel, _) in &clones {
            new_state.add_cursor(*sel);
        }

        // Rebuild the sticky columns aligned to the new cursor order. Clones
        // seed their source's sticky column; an original at the same position
        // (a collision) wins, so its sticky column is preserved.
        let mut sticky_by_pos: HashMap<Position, usize> = HashMap::new();
        for (sel, sticky) in &clones {
            sticky_by_pos.insert(sel.head, *sticky);
        }
        for (index, sel) in cursor.all_selections().enumerate() {
            let sticky = preferred.get(index).copied().unwrap_or(sel.head.column);
            sticky_by_pos.insert(sel.head, sticky);
        }
        let new_preferred: Vec<usize> = new_state
            .all_selections()
            .map(|sel| {
                sticky_by_pos
                    .get(&sel.head)
                    .copied()
                    .unwrap_or(sel.head.column)
            })
            .collect();
        self.store_sticky_columns(&new_state, new_preferred, revision);

        self.record_cursor_change(cursor, &new_state);
        Self::create_selection_command(cursor, &new_state)
    }

    /// Drops the most recently added occurrence cursor and adds the next
    /// occurrence of its text instead, wrapping around the document.
    ///
    /// VS Code binds this to the chord Ctrl+K Ctrl+D ("Move Last Selection to
    /// Next Find Match"). This codebase has no chord infrastructure (the
    /// keymap layer is a pending design decision), so this public method is
    /// the complete deliverable — call it directly (it is threaded through
    /// [`crate::Editor`]). It never inserts a chord binding of its own.
    ///
    /// Behavior:
    /// - With one or more added occurrence cursors, the most recently added
    ///   one is removed and the next occurrence after it that is not already
    ///   selected is added (wrapping); the added cursor becomes the new top of
    ///   the addition-order stack.
    /// - With no added cursors and a collapsed caret, the word under the caret
    ///   is selected (the Ctrl+D bootstrap), so a following call can advance.
    /// - With no added cursors and a non-empty primary selection, the primary
    ///   is moved to the next occurrence (wrapping).
    ///
    /// Returns the resulting [`KeyResult`]; the caller applies any command.
    pub fn skip_last_added_occurrence(
        &mut self,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        let result = self.skip_occurrence_inner(document, cursor);
        if matches!(result, KeyResult::Command(_)) {
            // Skip changed the selection through a non-vertical path, so any
            // held sticky columns describe a cursor set the user has since
            // navigated away from — even when the skip wraps back to a
            // byte-identical selection, the live head's column is what a
            // following vertical verb must seed from. Destroy (not merely
            // dirty) the columns, exactly as `note_operation` treats the
            // keyboard-bound occurrence verbs (Ctrl+D, Ctrl+Shift+L, Ctrl+U).
            // The addition-order stack is left intact: skip maintains it
            // itself and `reset_vertical_state` does not touch it.
            self.reset_vertical_state();
        }
        result
    }

    /// The body of [`Self::skip_last_added_occurrence`], separated so the
    /// public entry point can uniformly invalidate sticky columns on every
    /// branch that produced a selection change.
    fn skip_occurrence_inner(&mut self, document: &Document, cursor: &CursorState) -> KeyResult {
        self.sync_add_order(cursor, document.revision());

        // Find the most recently added *occurrence* cursor: a non-collapsed
        // selection on the stack. Collapsed additions (vertical add-above/below
        // cursors) are not find matches, so they are skipped over rather than
        // treated as the search source — a vertical add on top of the stack
        // must never block advancing the last occurrence.
        let occ_index = self
            .cursor_add_order
            .iter()
            .rposition(|sel| !sel.is_collapsed());

        let Some(idx) = occ_index else {
            // No occurrence cursor to drop: bootstrap or move the primary.
            if cursor.primary.is_collapsed() {
                let word = motions::select_word_at(document, cursor.primary.head);
                if word.is_collapsed() {
                    return KeyResult::Handled;
                }
                let mut new_state = cursor.clone();
                new_state.primary = word;
                return self.finish_cursor_command(cursor, &new_state);
            }
            return self.skip_move_primary(document, cursor);
        };

        let dropped = self.cursor_add_order[idx];
        let source = document.slice(dropped.range());
        if source.is_empty() {
            // A non-collapsed selection should always carry text; nothing safe
            // to advance if it somehow does not.
            return KeyResult::Handled;
        }

        let remaining = Self::remove_secondary(cursor, &dropped);
        let taken: Vec<Range> = remaining.all_selections().map(Selection::range).collect();
        let after = document
            .position_to_offset(dropped.range().end)
            .unwrap_or(0);

        match multi_cursor::next_occurrence(document, &source, after, &taken) {
            Some(next) if next.range() != dropped.range() => {
                let mut new_state = remaining;
                new_state.add_cursor(next);
                // Replace the dropped occurrence entry with the newly added
                // one, keeping it the most recent occurrence for a following
                // skip while leaving any collapsed (vertical) entries in place.
                self.cursor_add_order.remove(idx);
                self.cursor_add_order.push(next);
                self.add_order_anchor = Some(new_state.clone());
                Self::create_selection_command(cursor, &new_state)
            },
            _ => {
                // Only the dropped occurrence exists (or every other is already
                // selected): leave the cursor state and stack unchanged.
                KeyResult::Handled
            },
        }
    }

    /// Moves the primary selection to the next occurrence of its text
    /// (wrapping), used by [`Self::skip_last_added_occurrence`] when there are
    /// no added cursors to drop.
    fn skip_move_primary(&mut self, document: &Document, cursor: &CursorState) -> KeyResult {
        let source = document.slice(cursor.primary.range());
        if source.is_empty() {
            return KeyResult::Handled;
        }
        let taken: Vec<Range> = cursor.secondary.iter().map(Selection::range).collect();
        let after = document
            .position_to_offset(cursor.primary.range().end)
            .unwrap_or(0);

        match multi_cursor::next_occurrence(document, &source, after, &taken) {
            Some(next) if next.range() != cursor.primary.range() => {
                let mut new_state = cursor.clone();
                new_state.primary = next;
                self.finish_cursor_command(cursor, &new_state)
            },
            _ => KeyResult::Handled,
        }
    }

    /// Removes the secondary cursor whose selection equals `target`, rebuilding
    /// the cursor state from the primary and remaining secondaries.
    fn remove_secondary(cursor: &CursorState, target: &Selection) -> CursorState {
        let mut new_state = CursorState::new(cursor.primary);
        for sel in &cursor.secondary {
            if sel != target {
                new_state.add_cursor(*sel);
            }
        }
        new_state
    }

    /// Ensures the addition-order stack is consistent with `cursor`.
    ///
    /// If the incoming cursor state differs from the one the stack was last
    /// consistent with ([`Self::add_order_anchor`]), or the document content
    /// changed since ([`Self::add_order_revision`] vs `revision`), some other
    /// path moved, added, or removed a cursor — or shifted the text under
    /// otherwise-unchanged cursors — so the stack is stale and is cleared.
    /// Otherwise the stack is left intact (and any entry that somehow no longer
    /// corresponds to a current secondary is dropped defensively).
    fn sync_add_order(&mut self, cursor: &CursorState, revision: u64) {
        if self.add_order_anchor.as_ref() != Some(cursor)
            || self.add_order_revision != Some(revision)
        {
            self.cursor_add_order.clear();
            self.add_order_anchor = Some(cursor.clone());
            self.add_order_revision = Some(revision);
            return;
        }
        let secondaries: Vec<Selection> = cursor.secondary.clone();
        self.cursor_add_order
            .retain(|sel| secondaries.contains(sel));
    }

    /// Records the cursor transition produced by a multi-cursor add verb.
    ///
    /// Every secondary cursor in `new_state` that was not already a selection
    /// in `old` is pushed onto the addition-order stack in document order, and
    /// the anchor is advanced to `new_state` so the next verb trusts the stack.
    /// The comparison is by selection identity, not cursor *count*: select-all
    /// can create fresh occurrence selections while *reducing* the count (five
    /// vertical cursors collapsing to three matches), and those new selections
    /// must still be recorded so undo-cursor/skip can peel them. Verbs that
    /// only reshape the primary (the Ctrl+D word bootstrap) add no new
    /// secondary and so push nothing.
    ///
    /// [`Self::add_order_revision`] is intentionally left untouched here: an
    /// add verb never changes document content, so the revision recorded by
    /// the preceding [`Self::sync_add_order`] still describes `new_state`.
    fn record_cursor_change(&mut self, old: &CursorState, new_state: &CursorState) {
        let old_selections: Vec<Selection> = old.all_selections().copied().collect();
        for sel in &new_state.secondary {
            if !old_selections.contains(sel) {
                self.cursor_add_order.push(*sel);
            }
        }
        self.add_order_anchor = Some(new_state.clone());
    }

    /// Records the cursor change and emits the resulting selection command.
    ///
    /// The sticky vertical column is not touched: the cursor set changed
    /// through a non-vertical path, so any held sticky columns no longer match
    /// the new state and are ignored on the next vertical move without being
    /// destroyed (which keeps them recoverable if undo restores the prior
    /// state).
    fn finish_cursor_command(&mut self, old: &CursorState, new_state: &CursorState) -> KeyResult {
        self.record_cursor_change(old, new_state);
        Self::create_selection_command(old, new_state)
    }

    /// Returns true when the modifier set selects "add cursor above/below":
    /// Ctrl+Alt (Cmd+Alt on macOS reports as Ctrl+Alt here) without Shift or
    /// Meta.
    ///
    /// `AltGraph` must be absent: on many non-US layouts `AltGr` is reported as
    /// Ctrl+Alt while composing a character, which would otherwise be
    /// indistinguishable from this chord and spawn cursors on an arrow press.
    /// Requiring `!alt_graph` keeps `AltGr`+Arrow out of the add-cursor
    /// dispatch.
    const fn is_add_cursor_modifiers(modifiers: Modifiers) -> bool {
        modifiers.ctrl
            && modifiers.alt
            && !modifiers.shift
            && !modifiers.meta
            && !modifiers.alt_graph
    }

    // ========== Utility methods ==========

    /// Post-dispatch bookkeeping: invalidates transient handler state after a
    /// cursor-mutating operation that is not the corresponding verb.
    ///
    /// - A cursor mutation that is not a sticky-column vertical op (plain
    ///   Up/Down or Ctrl+Alt+Up/Down) invalidates the sticky columns, but *how*
    ///   depends on whether the operation is undoable:
    ///   - A **content edit** only marks the columns dirty (dormant). The edit
    ///     may be undone, and an undo that restores the exact pre-edit cursor
    ///     state should also revive the columns captured before it (see
    ///     [`Self::revalidate_vertical_columns`]).
    ///   - A **selection-only** move (a horizontal motion, Home/End, Escape)
    ///     permanently *discards* the columns. Such moves are never recorded in
    ///     the undo history, so no later undo can legitimately revive them; a
    ///     Left-then-Right round trip that returns the caret to the captured
    ///     coordinates must not leave a dirty snapshot for a subsequent undo of
    ///     an *unrelated* content edit to resurrect. Destroying (not merely
    ///     dirtying) is what closes that resurrection window.
    /// - Any cursor mutation that is not a multi-cursor add verb clears the
    ///   addition-order stack, so a motion or edit between add verbs cannot let
    ///   Ctrl+U/skip remove the wrong cursor even if the state round-trips.
    ///
    /// Operations that leave the cursor untouched (copy, undo/redo
    /// acknowledgements, search requests, ignored keys) invalidate nothing.
    fn note_operation(&mut self, event: &KeyEvent, result: &KeyResult) {
        if !Self::result_mutates_cursor(result) {
            return;
        }
        if !Self::uses_sticky_columns(event) {
            if Self::result_modifies_content(result) {
                // Undoable edit: keep the columns dormant so an undo that
                // restores the exact prior state can revive them.
                self.sticky_dirty = true;
            } else {
                // Non-undoable selection-only move: destroy the columns so no
                // later undo (of an unrelated edit) can resurrect a snapshot a
                // horizontal round trip already invalidated.
                self.reset_vertical_state();
            }
        }
        if !Self::is_add_order_verb(event) {
            self.invalidate_cursor_order();
        }
    }

    /// Returns true when the result of a keyboard operation modifies document
    /// content (as opposed to a selection-only change or a no-op).
    fn result_modifies_content(result: &KeyResult) -> bool {
        match result {
            KeyResult::Command(command) => command.modifies_content(),
            KeyResult::Clipboard(ClipboardOperation::Cut { command, .. }) => {
                command.modifies_content()
            },
            KeyResult::Handled
            | KeyResult::Ignored
            | KeyResult::Clipboard(_)
            | KeyResult::Search(_) => false,
        }
    }

    /// Returns true when the result of a keyboard operation changes the cursor
    /// or selection state (a selection command, a content edit, or a cut whose
    /// deletion mutates the document).
    fn result_mutates_cursor(result: &KeyResult) -> bool {
        match result {
            KeyResult::Command(command) => {
                command.modifies_selection() || command.modifies_content()
            },
            KeyResult::Clipboard(ClipboardOperation::Cut { command, .. }) => {
                command.modifies_content()
            },
            KeyResult::Handled
            | KeyResult::Ignored
            | KeyResult::Clipboard(_)
            | KeyResult::Search(_) => false,
        }
    }

    /// Returns true when the event is a vertical operation that consumes and
    /// re-establishes the sticky preferred columns: plain (or Shift-extended)
    /// Up/Down navigation, or Ctrl+Alt+Up/Down add-cursor. Alt/Shift+Alt line
    /// operations are excluded — they move whole lines and must not preserve
    /// sticky columns.
    const fn uses_sticky_columns(event: &KeyEvent) -> bool {
        matches!(event.key, KeyCode::Up | KeyCode::Down)
            && !Self::is_line_op_modifiers(event.modifiers)
    }

    /// Returns true when the event is a multi-cursor add-order verb: Ctrl+D
    /// (add next match, shift-agnostic), Ctrl+Shift+L (select all occurrences),
    /// Ctrl+U (undo last cursor), or Ctrl+Alt+Up/Down (add cursor
    /// above/below). These maintain the addition-order stack themselves, so
    /// [`Self::note_operation`] must not clear it under them.
    ///
    /// The predicate mirrors the dispatch guards in [`Self::dispatch_key`] and
    /// [`Self::handle_ctrl_char`]; keep the two in sync.
    const fn is_add_order_verb(event: &KeyEvent) -> bool {
        let modifiers = event.modifiers;
        match event.key {
            KeyCode::Up | KeyCode::Down => Self::is_add_cursor_modifiers(modifiers),
            KeyCode::Char(c) if modifiers.ctrl && !modifiers.alt && !modifiers.meta => {
                let lower = c.to_ascii_lowercase();
                lower == 'd'
                    || (lower == 'l' && modifiers.shift)
                    || (lower == 'u' && !modifiers.shift)
            },
            _ => false,
        }
    }

    /// Creates a selection command if the cursor changed.
    fn create_selection_command(old: &CursorState, new: &CursorState) -> KeyResult {
        if old == new {
            KeyResult::Handled
        } else {
            KeyResult::Command(Command::SetSelection {
                old_state: old.clone(),
                new_state: new.clone(),
            })
        }
    }
}
