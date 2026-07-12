//! Keyboard input handling.
//!
//! This module provides keyboard event handling for the editor, including:
//! - Arrow key navigation (with Ctrl for word-level, Shift for selection)
//! - Home/End keys (with Shift for selection)
//! - Character insertion
//! - Backspace/Delete
//! - Clipboard operations (Ctrl+C/X/V)
//! - Undo/Redo (Ctrl+Z, Ctrl+Y/Ctrl+Shift+Z)
//!
//! All editing and navigation operations are multi-cursor aware: every
//! cursor (primary and secondary) is edited or moved independently, and
//! cursors that converge on the same position are merged. The per-cursor
//! primitives live in [`editing`] and [`motions`] so other input paths (e.g.
//! paste handling in the editor core) can reuse them.

pub mod editing;
pub mod motions;
mod types;

#[cfg(test)]
mod tests;

pub use types::{ClipboardOperation, KeyCode, KeyEvent, KeyResult, Modifiers, SearchAction};

use crate::document::{CursorState, Document, Position, Range, Selection};
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
///
/// All operations act on every cursor of a multi-cursor [`CursorState`].
#[derive(Debug, Default)]
pub struct KeyboardHandler {
    /// Per-cursor preferred ("sticky") columns for vertical movement.
    ///
    /// Aligned with [`CursorState::all_selections`] order (primary first,
    /// then secondary cursors in document order) as captured by the last
    /// vertical move. The vector lives on the handler rather than on
    /// `Selection` so the persisted cursor state stays free of transient
    /// input concerns; it is rebuilt from the current cursor columns whenever
    /// its length no longer matches the cursor count (cursors added, removed,
    /// or merged) and cleared by any horizontal motion or edit.
    preferred_columns: Option<Vec<usize>>,
}

impl KeyboardHandler {
    /// Creates a new keyboard handler.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            preferred_columns: None,
        }
    }

    /// Handles a keyboard event.
    ///
    /// Returns a `KeyResult` indicating what action should be taken.
    /// The caller is responsible for applying any resulting commands
    /// and handling clipboard operations.
    pub fn handle_key(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
        history: &UndoTree,
    ) -> KeyResult {
        match event.key {
            // Arrow key navigation
            KeyCode::Left => self.handle_left(event, document, cursor),
            KeyCode::Right => self.handle_right(event, document, cursor),
            KeyCode::Up => self.handle_up(event, document, cursor),
            KeyCode::Down => self.handle_down(event, document, cursor),

            // Home/End
            KeyCode::Home => self.handle_home(event, document, cursor),
            KeyCode::End => self.handle_end(event, document, cursor),

            // Character input
            KeyCode::Char(c) if event.modifiers.ctrl => {
                self.handle_ctrl_char(c, document, cursor, history)
            },
            KeyCode::Char(c) if !event.modifiers.alt && !event.modifiers.meta => {
                self.handle_char_input(c, document, cursor)
            },

            // Enter key
            KeyCode::Enter => self.handle_enter(document, cursor),

            // Tab key
            KeyCode::Tab => self.handle_tab(event, document, cursor),

            // Delete operations
            KeyCode::Backspace => self.handle_backspace(event, document, cursor),
            KeyCode::Delete => self.handle_delete(event, document, cursor),

            // Escape - collapse to primary cursor
            KeyCode::Escape => self.handle_escape(cursor),

            // F3 - Next match (T122)
            KeyCode::F3 if !event.modifiers.shift => KeyResult::Search(SearchAction::NextMatch),
            // Shift+F3 - Previous match (T122)
            KeyCode::F3 if event.modifiers.shift => KeyResult::Search(SearchAction::PreviousMatch),

            // Everything else: Page Up/Down are handled by the viewport,
            // function keys and modifier-only keys are not handled, and
            // characters with Alt/Meta modifiers pass through. (The bare F3
            // arm is unreachable due to the guards above but keeps the match
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

    // ========== Navigation handlers ==========

    /// Handles left arrow key.
    fn handle_left(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        self.preferred_columns = None;

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
    fn handle_right(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        self.preferred_columns = None;

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
    fn handle_home(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        self.preferred_columns = None;

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
    fn handle_end(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        self.preferred_columns = None;

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

        // Reuse the sticky columns from the previous vertical move when they
        // still line up with the cursors; otherwise start from the current
        // head columns.
        let mut preferred = match self.preferred_columns.take() {
            Some(columns) if columns.len() == count => columns,
            _ => cursor.all_selections().map(|sel| sel.head.column).collect(),
        };

        let new_cursor = motions::apply_to_all(cursor, extend_selection, |index, sel| {
            let target = preferred.get(index).copied().unwrap_or(sel.head.column);
            let (head, sticky) = motions::vertical_move(document, sel.head, target, direction);
            if let Some(slot) = preferred.get_mut(index) {
                *slot = sticky;
            }
            head
        });

        // If cursors merged, the per-cursor mapping is gone; rebuild from the
        // actual columns on the next vertical move.
        self.preferred_columns = if new_cursor.cursor_count() == count {
            Some(preferred)
        } else {
            None
        };

        new_cursor
    }

    /// Handles Escape key - collapse to primary cursor.
    fn handle_escape(&mut self, cursor: &CursorState) -> KeyResult {
        self.preferred_columns = None;

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
    fn handle_ctrl_char(
        &mut self,
        c: char,
        document: &Document,
        cursor: &CursorState,
        history: &UndoTree,
    ) -> KeyResult {
        match c.to_ascii_lowercase() {
            'c' => Self::handle_copy(document, cursor),
            'x' => {
                self.preferred_columns = None;
                Self::handle_cut(document, cursor)
            },
            'v' => {
                self.preferred_columns = None;
                KeyResult::Clipboard(ClipboardOperation::Paste)
            },
            'z' => Self::handle_undo(history),
            'y' => Self::handle_redo(history),
            'a' => Self::handle_select_all(document, cursor),
            'd' => Self::handle_add_selection_next_match(document, cursor), // T107
            'f' => KeyResult::Search(SearchAction::OpenSearch),             // T121
            _ => KeyResult::Ignored,
        }
    }

    /// Handles character input.
    fn handle_char_input(
        &mut self,
        c: char,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        self.preferred_columns = None;
        Self::insert_text(&c.to_string(), document, cursor)
    }

    /// Handles Enter key.
    fn handle_enter(&mut self, document: &Document, cursor: &CursorState) -> KeyResult {
        self.preferred_columns = None;
        let line_ending = document.line_ending().as_str();
        Self::insert_text(line_ending, document, cursor)
    }

    /// Handles Tab key.
    fn handle_tab(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        self.preferred_columns = None;

        if event.modifiers.shift {
            // Outdent - not implemented in basic editing
            KeyResult::Ignored
        } else {
            // Insert tab character (or spaces if configured)
            // For now, insert a tab character
            Self::insert_text("\t", document, cursor)
        }
    }

    /// Handles Backspace key.
    ///
    /// Each cursor acts independently: cursors with a selection delete the
    /// selection, collapsed cursors delete the character (or word with Ctrl)
    /// before the caret. Converging cursors are merged.
    fn handle_backspace(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        self.preferred_columns = None;

        let edits = editing::backspace_edits(document, cursor, event.modifiers.ctrl);
        editing::build_multi_cursor_command(document, cursor, edits)
            .map_or(KeyResult::Handled, KeyResult::Command)
    }

    /// Handles Delete key.
    ///
    /// Each cursor acts independently: cursors with a selection delete the
    /// selection, collapsed cursors delete the character (or word with Ctrl)
    /// after the caret. Converging cursors are merged.
    fn handle_delete(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        self.preferred_columns = None;

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
    fn handle_add_selection_next_match(document: &Document, cursor: &CursorState) -> KeyResult {
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

        // If we had to select a word first, do that
        if let Some(sel) = initial_selection {
            new_cursor.primary = sel;
            return KeyResult::Command(Command::SetSelection {
                old_state: cursor.clone(),
                new_state: new_cursor,
            });
        }

        // Find the next occurrence after the last cursor
        let search_start = Self::last_selection_end(&new_cursor);
        let doc_text = document.text();
        let search_offset = document.position_to_offset(search_start).unwrap_or(0);

        // Search for next occurrence
        if let Some(match_offset) = doc_text[search_offset..].find(&search_text) {
            let abs_offset = search_offset + match_offset;
            let Some(match_start) = document.offset_to_position(abs_offset) else {
                return KeyResult::Handled;
            };
            let Some(match_end) = document.offset_to_position(abs_offset + search_text.len())
            else {
                return KeyResult::Handled;
            };

            let new_selection = Selection::new(match_start, match_end);
            new_cursor.add_cursor(new_selection);

            KeyResult::Command(Command::SetSelection {
                old_state: cursor.clone(),
                new_state: new_cursor,
            })
        } else {
            // Wrap around to start of document
            if let Some(match_offset) = doc_text.find(&search_text) {
                let Some(match_start) = document.offset_to_position(match_offset) else {
                    return KeyResult::Handled;
                };
                let Some(match_end) = document.offset_to_position(match_offset + search_text.len())
                else {
                    return KeyResult::Handled;
                };

                // Don't add if it's the same as an existing selection
                let new_range = Range::new(match_start, match_end);
                let already_selected = new_cursor.all_selections().any(|s| s.range() == new_range);

                if !already_selected {
                    let new_selection = Selection::new(match_start, match_end);
                    new_cursor.add_cursor(new_selection);

                    return KeyResult::Command(Command::SetSelection {
                        old_state: cursor.clone(),
                        new_state: new_cursor,
                    });
                }
            }
            KeyResult::Handled
        }
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

    // ========== Utility methods ==========

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
