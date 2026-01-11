//! Keyboard input handling.
//!
//! This module provides keyboard event handling for the editor, including:
//! - Arrow key navigation (with Ctrl for word-level, Shift for selection)
//! - Home/End keys (with Shift for selection)
//! - Character insertion
//! - Backspace/Delete
//! - Clipboard operations (Ctrl+C/X/V)
//! - Undo/Redo (Ctrl+Z, Ctrl+Y/Ctrl+Shift+Z)

use serde::{Deserialize, Serialize};

use crate::document::{CursorState, Document, Position, Range, Selection};
use crate::history::{Command, UndoTree};

/// Key codes for keyboard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyCode {
    /// Alphanumeric and symbol keys
    Char(char),

    /// Left arrow key
    Left,
    /// Right arrow key
    Right,
    /// Up arrow key
    Up,
    /// Down arrow key
    Down,

    /// Home key (go to start of line)
    Home,
    /// End key (go to end of line)
    End,
    /// Page Up key
    PageUp,
    /// Page Down key
    PageDown,

    /// Backspace key (delete character before cursor)
    Backspace,
    /// Delete key (delete character after cursor)
    Delete,
    /// Enter/Return key
    Enter,
    /// Tab key
    Tab,

    /// Shift modifier key
    Shift,
    /// Control modifier key
    Control,
    /// Alt/Option modifier key
    Alt,
    /// Meta/Windows/Command modifier key
    Meta,

    /// Escape key
    Escape,

    /// Function key F1
    F1,
    /// Function key F2
    F2,
    /// Function key F3
    F3,
    /// Function key F4
    F4,
    /// Function key F5
    F5,
    /// Function key F6
    F6,
    /// Function key F7
    F7,
    /// Function key F8
    F8,
    /// Function key F9
    F9,
    /// Function key F10
    F10,
    /// Function key F11
    F11,
    /// Function key F12
    F12,
}

/// Keyboard modifier state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Modifiers {
    /// Shift key is held
    pub shift: bool,
    /// Control key is held (Cmd on Mac)
    pub ctrl: bool,
    /// Alt key is held (Option on Mac)
    pub alt: bool,
    /// Meta key is held (Win key, or Cmd on Mac)
    pub meta: bool,
}

impl Modifiers {
    /// No modifiers pressed.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
        }
    }

    /// Shift only.
    #[must_use]
    pub const fn shift() -> Self {
        Self {
            shift: true,
            ctrl: false,
            alt: false,
            meta: false,
        }
    }

    /// Ctrl only.
    #[must_use]
    pub const fn ctrl() -> Self {
        Self {
            shift: false,
            ctrl: true,
            alt: false,
            meta: false,
        }
    }

    /// Ctrl+Shift.
    #[must_use]
    pub const fn ctrl_shift() -> Self {
        Self {
            shift: true,
            ctrl: true,
            alt: false,
            meta: false,
        }
    }
}

/// A keyboard event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyEvent {
    /// The key that was pressed
    pub key: KeyCode,
    /// Active modifiers
    pub modifiers: Modifiers,
    /// True if this is a key repeat event
    pub is_repeat: bool,
}

impl KeyEvent {
    /// Creates a new key event.
    #[must_use]
    pub const fn new(key: KeyCode, modifiers: Modifiers) -> Self {
        Self {
            key,
            modifiers,
            is_repeat: false,
        }
    }

    /// Creates a new key event with no modifiers.
    #[must_use]
    pub const fn simple(key: KeyCode) -> Self {
        Self::new(key, Modifiers::none())
    }
}

/// Search-related actions triggered by keyboard input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchAction {
    /// Open the search panel (Ctrl+F)
    OpenSearch,
    /// Go to next match (F3 or Enter in search)
    NextMatch,
    /// Go to previous match (Shift+F3)
    PreviousMatch,
    /// Close search panel (Escape when search is open)
    CloseSearch,
}

/// Result of handling a keyboard event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyResult {
    /// The event was handled, no further action needed
    Handled,

    /// The event was handled and a command was produced
    Command(Command),

    /// The event triggered a clipboard operation
    Clipboard(ClipboardOperation),

    /// The event triggered a search action (T121, T122)
    Search(SearchAction),

    /// The event was not handled (pass to next handler)
    Ignored,
}

/// Clipboard operation requested by keyboard handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardOperation {
    /// Copy selected text
    Copy(String),
    /// Cut selected text (returns text to copy and command to delete)
    Cut {
        /// The text to copy to clipboard
        text: String,
        /// The command to apply to delete the selection
        command: Command,
    },
    /// Request paste from clipboard
    Paste,
}

/// Keyboard event handler for the editor.
///
/// This handler processes keyboard input and produces commands that can be
/// applied to the document. It handles:
/// - Navigation (arrow keys, Home/End, Page Up/Down)
/// - Selection (Shift+navigation)
/// - Text editing (character input, Backspace, Delete)
/// - Clipboard operations (Ctrl+C/X/V)
/// - Undo/Redo (Ctrl+Z, Ctrl+Y)
#[derive(Debug, Default)]
pub struct KeyboardHandler {
    /// Preferred column when moving vertically (for sticky column behavior)
    preferred_column: Option<usize>,
}

impl KeyboardHandler {
    /// Creates a new keyboard handler.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            preferred_column: None,
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

            // Page navigation
            KeyCode::PageUp | KeyCode::PageDown => KeyResult::Ignored, // Handled by viewport

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

            // Other function keys (not handled)
            KeyCode::F1
            | KeyCode::F2
            | KeyCode::F3 // Fallback (shouldn't reach here due to guards above)
            | KeyCode::F4
            | KeyCode::F5
            | KeyCode::F6
            | KeyCode::F7
            | KeyCode::F8
            | KeyCode::F9
            | KeyCode::F10
            | KeyCode::F11
            | KeyCode::F12 => KeyResult::Ignored,

            // Modifier-only keys
            KeyCode::Shift | KeyCode::Control | KeyCode::Alt | KeyCode::Meta => KeyResult::Ignored,

            // Unhandled character with alt/meta modifiers
            KeyCode::Char(_) => KeyResult::Ignored,
        }
    }

    /// Handles left arrow key.
    fn handle_left(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        self.preferred_column = None;

        let new_cursor = if event.modifiers.ctrl {
            // Word-level movement
            self.move_word_left(document, cursor, event.modifiers.shift)
        } else {
            // Character-level movement
            self.move_char_left(document, cursor, event.modifiers.shift)
        };

        self.create_selection_command(cursor, &new_cursor)
    }

    /// Handles right arrow key.
    fn handle_right(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        self.preferred_column = None;

        let new_cursor = if event.modifiers.ctrl {
            // Word-level movement
            self.move_word_right(document, cursor, event.modifiers.shift)
        } else {
            // Character-level movement
            self.move_char_right(document, cursor, event.modifiers.shift)
        };

        self.create_selection_command(cursor, &new_cursor)
    }

    /// Handles up arrow key.
    fn handle_up(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        let new_cursor = self.move_line_up(document, cursor, event.modifiers.shift);
        self.create_selection_command(cursor, &new_cursor)
    }

    /// Handles down arrow key.
    fn handle_down(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        let new_cursor = self.move_line_down(document, cursor, event.modifiers.shift);
        self.create_selection_command(cursor, &new_cursor)
    }

    /// Handles Home key.
    fn handle_home(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        self.preferred_column = None;

        let new_cursor = if event.modifiers.ctrl {
            // Move to start of document
            self.move_to_document_start(cursor, event.modifiers.shift)
        } else {
            // Move to start of line (smart home: first non-whitespace or column 0)
            self.move_to_line_start(document, cursor, event.modifiers.shift)
        };

        self.create_selection_command(cursor, &new_cursor)
    }

    /// Handles End key.
    fn handle_end(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        self.preferred_column = None;

        let new_cursor = if event.modifiers.ctrl {
            // Move to end of document
            self.move_to_document_end(document, cursor, event.modifiers.shift)
        } else {
            // Move to end of line
            self.move_to_line_end(document, cursor, event.modifiers.shift)
        };

        self.create_selection_command(cursor, &new_cursor)
    }

    /// Handles Ctrl+key combinations.
    fn handle_ctrl_char(
        &mut self,
        c: char,
        document: &Document,
        cursor: &CursorState,
        history: &UndoTree,
    ) -> KeyResult {
        match c.to_ascii_lowercase() {
            'c' => self.handle_copy(document, cursor),
            'x' => self.handle_cut(document, cursor),
            'v' => KeyResult::Clipboard(ClipboardOperation::Paste),
            'z' => self.handle_undo(history),
            'y' => self.handle_redo(history),
            'a' => self.handle_select_all(document, cursor),
            'd' => self.handle_add_selection_next_match(document, cursor), // T107
            'f' => KeyResult::Search(SearchAction::OpenSearch), // T121
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
        self.preferred_column = None;
        self.insert_text(&c.to_string(), document, cursor)
    }

    /// Handles Enter key.
    fn handle_enter(&mut self, document: &Document, cursor: &CursorState) -> KeyResult {
        self.preferred_column = None;
        let line_ending = document.line_ending().as_str();
        self.insert_text(line_ending, document, cursor)
    }

    /// Handles Tab key.
    fn handle_tab(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        self.preferred_column = None;

        if event.modifiers.shift {
            // Outdent - not implemented in basic editing
            KeyResult::Ignored
        } else {
            // Insert tab character (or spaces if configured)
            // For now, insert a tab character
            self.insert_text("\t", document, cursor)
        }
    }

    /// Handles Backspace key.
    fn handle_backspace(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        self.preferred_column = None;

        // If there's a selection, delete it
        if !cursor.primary.is_collapsed() {
            return self.delete_selection(document, cursor);
        }

        // Otherwise, delete character before cursor
        if event.modifiers.ctrl {
            self.delete_word_before(document, cursor)
        } else {
            self.delete_char_before(document, cursor)
        }
    }

    /// Handles Delete key.
    fn handle_delete(
        &mut self,
        event: &KeyEvent,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        self.preferred_column = None;

        // If there's a selection, delete it
        if !cursor.primary.is_collapsed() {
            return self.delete_selection(document, cursor);
        }

        // Otherwise, delete character after cursor
        if event.modifiers.ctrl {
            self.delete_word_after(document, cursor)
        } else {
            self.delete_char_after(document, cursor)
        }
    }

    /// Handles Escape key - collapse to primary cursor.
    fn handle_escape(&mut self, cursor: &CursorState) -> KeyResult {
        if cursor.secondary.is_empty() && cursor.primary.is_collapsed() {
            return KeyResult::Handled;
        }

        let mut new_cursor = cursor.clone();
        new_cursor.collapse_to_primary();
        new_cursor.primary = Selection::collapsed(cursor.primary.head);

        self.create_selection_command(cursor, &new_cursor)
    }

    // ========== Navigation helpers ==========

    /// Moves cursor one character to the left.
    fn move_char_left(
        &self,
        document: &Document,
        cursor: &CursorState,
        extend_selection: bool,
    ) -> CursorState {
        let head = cursor.primary.head;
        let new_head = if head.column > 0 {
            Position::new(head.line, head.column - 1)
        } else if head.line > 0 {
            // Move to end of previous line
            let prev_line = head.line - 1;
            let line_len = document.line_len(prev_line).unwrap_or(0);
            Position::new(prev_line, line_len)
        } else {
            head // Already at start of document
        };

        self.make_cursor_state(cursor, new_head, extend_selection)
    }

    /// Moves cursor one character to the right.
    fn move_char_right(
        &self,
        document: &Document,
        cursor: &CursorState,
        extend_selection: bool,
    ) -> CursorState {
        let head = cursor.primary.head;
        let line_len = document.line_len(head.line).unwrap_or(0);

        let new_head = if head.column < line_len {
            Position::new(head.line, head.column + 1)
        } else if head.line < document.line_count().saturating_sub(1) {
            // Move to start of next line
            Position::new(head.line + 1, 0)
        } else {
            head // Already at end of document
        };

        self.make_cursor_state(cursor, new_head, extend_selection)
    }

    /// Moves cursor one word to the left.
    fn move_word_left(
        &self,
        document: &Document,
        cursor: &CursorState,
        extend_selection: bool,
    ) -> CursorState {
        let head = cursor.primary.head;
        let new_head = self.find_word_boundary_left(document, head);
        self.make_cursor_state(cursor, new_head, extend_selection)
    }

    /// Moves cursor one word to the right.
    fn move_word_right(
        &self,
        document: &Document,
        cursor: &CursorState,
        extend_selection: bool,
    ) -> CursorState {
        let head = cursor.primary.head;
        let new_head = self.find_word_boundary_right(document, head);
        self.make_cursor_state(cursor, new_head, extend_selection)
    }

    /// Moves cursor one line up.
    fn move_line_up(
        &mut self,
        document: &Document,
        cursor: &CursorState,
        extend_selection: bool,
    ) -> CursorState {
        let head = cursor.primary.head;

        if head.line == 0 {
            // Already at first line
            let new_head = Position::new(0, 0);
            self.preferred_column = None;
            return self.make_cursor_state(cursor, new_head, extend_selection);
        }

        // Use preferred column for sticky behavior
        let target_column = self.preferred_column.unwrap_or(head.column);
        if self.preferred_column.is_none() {
            self.preferred_column = Some(head.column);
        }

        let prev_line = head.line - 1;
        let prev_line_len = document.line_len(prev_line).unwrap_or(0);
        let new_column = target_column.min(prev_line_len);
        let new_head = Position::new(prev_line, new_column);

        self.make_cursor_state(cursor, new_head, extend_selection)
    }

    /// Moves cursor one line down.
    fn move_line_down(
        &mut self,
        document: &Document,
        cursor: &CursorState,
        extend_selection: bool,
    ) -> CursorState {
        let head = cursor.primary.head;
        let line_count = document.line_count();

        if head.line >= line_count.saturating_sub(1) {
            // Already at last line - move to end
            let last_line = line_count.saturating_sub(1);
            let line_len = document.line_len(last_line).unwrap_or(0);
            let new_head = Position::new(last_line, line_len);
            self.preferred_column = None;
            return self.make_cursor_state(cursor, new_head, extend_selection);
        }

        // Use preferred column for sticky behavior
        let target_column = self.preferred_column.unwrap_or(head.column);
        if self.preferred_column.is_none() {
            self.preferred_column = Some(head.column);
        }

        let next_line = head.line + 1;
        let next_line_len = document.line_len(next_line).unwrap_or(0);
        let new_column = target_column.min(next_line_len);
        let new_head = Position::new(next_line, new_column);

        self.make_cursor_state(cursor, new_head, extend_selection)
    }

    /// Moves cursor to start of line (smart home).
    fn move_to_line_start(
        &self,
        document: &Document,
        cursor: &CursorState,
        extend_selection: bool,
    ) -> CursorState {
        let head = cursor.primary.head;
        let line_text = document.line(head.line).unwrap_or_default();

        // Find first non-whitespace character
        let first_non_ws = line_text
            .chars()
            .position(|c| !c.is_whitespace())
            .unwrap_or(0);

        // Smart home: toggle between first non-ws and column 0
        let new_column = if head.column == first_non_ws || head.column == 0 {
            if head.column == 0 { first_non_ws } else { 0 }
        } else {
            first_non_ws
        };

        let new_head = Position::new(head.line, new_column);
        self.make_cursor_state(cursor, new_head, extend_selection)
    }

    /// Moves cursor to end of line.
    fn move_to_line_end(
        &self,
        document: &Document,
        cursor: &CursorState,
        extend_selection: bool,
    ) -> CursorState {
        let head = cursor.primary.head;
        let line_len = document.line_len(head.line).unwrap_or(0);
        let new_head = Position::new(head.line, line_len);
        self.make_cursor_state(cursor, new_head, extend_selection)
    }

    /// Moves cursor to start of document.
    fn move_to_document_start(&self, cursor: &CursorState, extend_selection: bool) -> CursorState {
        let new_head = Position::zero();
        self.make_cursor_state(cursor, new_head, extend_selection)
    }

    /// Moves cursor to end of document.
    fn move_to_document_end(
        &self,
        document: &Document,
        cursor: &CursorState,
        extend_selection: bool,
    ) -> CursorState {
        let last_line = document.line_count().saturating_sub(1);
        let last_col = document.line_len(last_line).unwrap_or(0);
        let new_head = Position::new(last_line, last_col);
        self.make_cursor_state(cursor, new_head, extend_selection)
    }

    // ========== Word boundary helpers ==========

    /// Finds the word boundary to the left of position.
    fn find_word_boundary_left(&self, document: &Document, pos: Position) -> Position {
        // If at start of line, go to end of previous line
        if pos.column == 0 {
            if pos.line == 0 {
                return pos;
            }
            let prev_line = pos.line - 1;
            let line_len = document.line_len(prev_line).unwrap_or(0);
            return Position::new(prev_line, line_len);
        }

        let line_text = document.line(pos.line).unwrap_or_default();
        let chars: Vec<char> = line_text.chars().collect();
        let mut col = pos.column.min(chars.len());

        // Skip whitespace
        while col > 0
            && chars
                .get(col.saturating_sub(1))
                .map_or(false, |c| c.is_whitespace())
        {
            col -= 1;
        }

        // Skip word characters
        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';
        if col > 0
            && chars
                .get(col.saturating_sub(1))
                .map_or(false, |c| is_word_char(*c))
        {
            while col > 0
                && chars
                    .get(col.saturating_sub(1))
                    .map_or(false, |c| is_word_char(*c))
            {
                col -= 1;
            }
        } else {
            // Skip non-word, non-whitespace characters (punctuation)
            while col > 0
                && chars
                    .get(col.saturating_sub(1))
                    .map_or(false, |c| !is_word_char(*c) && !c.is_whitespace())
            {
                col -= 1;
            }
        }

        Position::new(pos.line, col)
    }

    /// Finds the word boundary to the right of position.
    fn find_word_boundary_right(&self, document: &Document, pos: Position) -> Position {
        let line_text = document.line(pos.line).unwrap_or_default();
        let chars: Vec<char> = line_text.chars().collect();
        let line_len = chars.len();

        // If at end of line, go to start of next line
        if pos.column >= line_len {
            if pos.line >= document.line_count().saturating_sub(1) {
                return pos;
            }
            return Position::new(pos.line + 1, 0);
        }

        let mut col = pos.column;
        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';

        // Skip current word or punctuation
        if chars.get(col).map_or(false, |c| is_word_char(*c)) {
            while col < line_len && chars.get(col).map_or(false, |c| is_word_char(*c)) {
                col += 1;
            }
        } else if chars.get(col).map_or(false, |c| !c.is_whitespace()) {
            while col < line_len
                && chars
                    .get(col)
                    .map_or(false, |c| !is_word_char(*c) && !c.is_whitespace())
            {
                col += 1;
            }
        }

        // Skip whitespace
        while col < line_len && chars.get(col).map_or(false, |c| c.is_whitespace()) {
            col += 1;
        }

        Position::new(pos.line, col)
    }

    // ========== Editing helpers ==========

    /// Inserts text at all cursor positions (T109).
    ///
    /// Handles multiple cursors by inserting at each position, properly adjusting
    /// positions for earlier insertions/deletions.
    fn insert_text(&self, text: &str, document: &Document, cursor: &CursorState) -> KeyResult {
        let mut commands = Vec::new();

        // Collect all selections, sorted by position (end to start for proper offset adjustment)
        let mut selections: Vec<Selection> = cursor.all_selections().copied().collect();
        selections.sort_by(|a, b| b.start().cmp(&a.start())); // Sort descending

        // Track offset adjustments from earlier operations
        let mut new_selections = Vec::new();

        // Process from end to start to maintain valid positions
        for selection in &selections {
            let insert_pos = if selection.is_collapsed() {
                selection.head
            } else {
                // Delete selection first
                let range = selection.range();
                let deleted = document.slice(range);
                commands.push(Command::Delete {
                    range,
                    deleted_text: deleted,
                });
                range.start
            };

            commands.push(Command::Insert {
                position: insert_pos,
                text: text.to_string(),
            });

            // Compute new cursor position after insert
            let new_pos = Self::compute_position_after_insert(insert_pos, text);
            new_selections.push(Selection::collapsed(new_pos));
        }

        // Reverse to get correct order (start to end)
        new_selections.reverse();

        // Create new cursor state with all cursors
        let new_cursor = if new_selections.is_empty() {
            cursor.clone()
        } else {
            let primary = new_selections.remove(0);
            CursorState {
                primary,
                secondary: new_selections,
            }
        };

        commands.push(Command::SetSelection {
            old_state: cursor.clone(),
            new_state: new_cursor,
        });

        if commands.len() == 1 {
            KeyResult::Command(commands.remove(0))
        } else {
            KeyResult::Command(Command::Compound { commands })
        }
    }

    /// Computes cursor position after inserting text.
    fn compute_position_after_insert(start: Position, text: &str) -> Position {
        let mut line = start.line;
        let mut column = start.column;

        for ch in text.chars() {
            if ch == '\n' {
                line += 1;
                column = 0;
            } else if ch == '\r' {
                // Skip CR in CRLF
            } else {
                column += 1;
            }
        }

        Position::new(line, column)
    }

    /// Deletes the current selection.
    fn delete_selection(&self, document: &Document, cursor: &CursorState) -> KeyResult {
        let range = cursor.primary.range();
        let deleted = document.slice(range);

        let new_cursor = CursorState::at(range.start);

        let commands = vec![
            Command::Delete {
                range,
                deleted_text: deleted,
            },
            Command::SetSelection {
                old_state: cursor.clone(),
                new_state: new_cursor,
            },
        ];

        KeyResult::Command(Command::Compound { commands })
    }

    /// Deletes one character before cursor.
    fn delete_char_before(&self, document: &Document, cursor: &CursorState) -> KeyResult {
        let head = cursor.primary.head;

        if head.column == 0 && head.line == 0 {
            return KeyResult::Handled; // Nothing to delete
        }

        let (start_pos, deleted_text) = if head.column > 0 {
            let start = Position::new(head.line, head.column - 1);
            let line_text = document.line(head.line).unwrap_or_default();
            let deleted_char = line_text
                .chars()
                .nth(head.column - 1)
                .map_or(String::new(), |c| c.to_string());
            (start, deleted_char)
        } else {
            // Delete newline at end of previous line
            let prev_line = head.line - 1;
            let prev_line_len = document.line_len(prev_line).unwrap_or(0);
            let start = Position::new(prev_line, prev_line_len);
            (start, document.line_ending().as_str().to_string())
        };

        let range = Range::new(start_pos, head);
        let new_cursor = CursorState::at(start_pos);

        let commands = vec![
            Command::Delete {
                range,
                deleted_text,
            },
            Command::SetSelection {
                old_state: cursor.clone(),
                new_state: new_cursor,
            },
        ];

        KeyResult::Command(Command::Compound { commands })
    }

    /// Deletes one character after cursor.
    fn delete_char_after(&self, document: &Document, cursor: &CursorState) -> KeyResult {
        let head = cursor.primary.head;
        let line_len = document.line_len(head.line).unwrap_or(0);
        let line_count = document.line_count();

        if head.line >= line_count.saturating_sub(1) && head.column >= line_len {
            return KeyResult::Handled; // Nothing to delete
        }

        let (end_pos, deleted_text) = if head.column < line_len {
            let end = Position::new(head.line, head.column + 1);
            let line_text = document.line(head.line).unwrap_or_default();
            let deleted_char = line_text
                .chars()
                .nth(head.column)
                .map_or(String::new(), |c| c.to_string());
            (end, deleted_char)
        } else {
            // Delete newline at end of current line
            let end = Position::new(head.line + 1, 0);
            (end, document.line_ending().as_str().to_string())
        };

        let range = Range::new(head, end_pos);

        KeyResult::Command(Command::Delete {
            range,
            deleted_text,
        })
    }

    /// Deletes word before cursor.
    fn delete_word_before(&self, document: &Document, cursor: &CursorState) -> KeyResult {
        let head = cursor.primary.head;
        let start_pos = self.find_word_boundary_left(document, head);

        if start_pos == head {
            return KeyResult::Handled; // Nothing to delete
        }

        let range = Range::new(start_pos, head);
        let deleted_text = document.slice(range);
        let new_cursor = CursorState::at(start_pos);

        let commands = vec![
            Command::Delete {
                range,
                deleted_text,
            },
            Command::SetSelection {
                old_state: cursor.clone(),
                new_state: new_cursor,
            },
        ];

        KeyResult::Command(Command::Compound { commands })
    }

    /// Deletes word after cursor.
    fn delete_word_after(&self, document: &Document, cursor: &CursorState) -> KeyResult {
        let head = cursor.primary.head;
        let end_pos = self.find_word_boundary_right(document, head);

        if end_pos == head {
            return KeyResult::Handled; // Nothing to delete
        }

        let range = Range::new(head, end_pos);
        let deleted_text = document.slice(range);

        KeyResult::Command(Command::Delete {
            range,
            deleted_text,
        })
    }

    // ========== Clipboard operations ==========

    /// Handles copy operation.
    fn handle_copy(&self, document: &Document, cursor: &CursorState) -> KeyResult {
        if cursor.primary.is_collapsed() {
            // Copy entire line if no selection
            let line = cursor.primary.head.line;
            let line_text = document.line(line).unwrap_or_default();
            let text = format!("{}{}", line_text, document.line_ending().as_str());
            KeyResult::Clipboard(ClipboardOperation::Copy(text))
        } else {
            let range = cursor.primary.range();
            let text = document.slice(range);
            KeyResult::Clipboard(ClipboardOperation::Copy(text))
        }
    }

    /// Handles cut operation.
    fn handle_cut(&self, document: &Document, cursor: &CursorState) -> KeyResult {
        if cursor.primary.is_collapsed() {
            // Cut entire line if no selection
            let line = cursor.primary.head.line;
            let line_text = document.line(line).unwrap_or_default();
            let text = format!("{}{}", line_text, document.line_ending().as_str());

            let start = Position::new(line, 0);
            let end = if line < document.line_count().saturating_sub(1) {
                Position::new(line + 1, 0)
            } else {
                Position::new(line, line_text.len())
            };

            let range = Range::new(start, end);
            let new_cursor = CursorState::at(start);

            let command = Command::Compound {
                commands: vec![
                    Command::Delete {
                        range,
                        deleted_text: text.clone(),
                    },
                    Command::SetSelection {
                        old_state: cursor.clone(),
                        new_state: new_cursor,
                    },
                ],
            };

            KeyResult::Clipboard(ClipboardOperation::Cut { text, command })
        } else {
            let range = cursor.primary.range();
            let text = document.slice(range);
            let new_cursor = CursorState::at(range.start);

            let command = Command::Compound {
                commands: vec![
                    Command::Delete {
                        range,
                        deleted_text: text.clone(),
                    },
                    Command::SetSelection {
                        old_state: cursor.clone(),
                        new_state: new_cursor,
                    },
                ],
            };

            KeyResult::Clipboard(ClipboardOperation::Cut { text, command })
        }
    }

    /// Handles undo operation.
    fn handle_undo(&self, history: &UndoTree) -> KeyResult {
        if history.can_undo() {
            // The undo operation is handled at a higher level
            KeyResult::Handled
        } else {
            KeyResult::Handled
        }
    }

    /// Handles redo operation.
    fn handle_redo(&self, history: &UndoTree) -> KeyResult {
        if history.can_redo() {
            // The redo operation is handled at a higher level
            KeyResult::Handled
        } else {
            KeyResult::Handled
        }
    }

    /// Handles select all operation.
    fn handle_select_all(&self, document: &Document, cursor: &CursorState) -> KeyResult {
        let start = Position::zero();
        let last_line = document.line_count().saturating_sub(1);
        let last_col = document.line_len(last_line).unwrap_or(0);
        let end = Position::new(last_line, last_col);

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
    fn handle_add_selection_next_match(
        &mut self,
        document: &Document,
        cursor: &CursorState,
    ) -> KeyResult {
        // Get the search text - either from selection or select word at cursor
        let (search_text, initial_selection) = if cursor.primary.is_collapsed() {
            // No selection - select word at cursor first
            let word_sel = self.select_word_at(cursor.primary.head, document);
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
        let search_start = self.get_last_selection_end(&new_cursor);
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
    fn get_last_selection_end(&self, cursor: &CursorState) -> Position {
        let mut last_end = cursor.primary.end();
        for sel in &cursor.secondary {
            if sel.end() > last_end {
                last_end = sel.end();
            }
        }
        last_end
    }

    /// Selects the word at the given position.
    fn select_word_at(&self, position: Position, document: &Document) -> Selection {
        let line_text = document.line(position.line).unwrap_or_default();
        let chars: Vec<char> = line_text.chars().collect();

        if chars.is_empty() {
            return Selection::collapsed(position);
        }

        let col = position.column.min(chars.len().saturating_sub(1));
        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';

        // Check if we're on a word character
        if !chars.get(col).map_or(false, |c| is_word_char(*c)) {
            return Selection::collapsed(position);
        }

        // Find word boundaries
        let mut start = col;
        let mut end = col;

        while start > 0 && chars.get(start - 1).map_or(false, |c| is_word_char(*c)) {
            start -= 1;
        }
        while end < chars.len() && chars.get(end).map_or(false, |c| is_word_char(*c)) {
            end += 1;
        }

        Selection::new(
            Position::new(position.line, start),
            Position::new(position.line, end),
        )
    }

    // ========== Utility methods ==========

    /// Creates a new cursor state with the given head position.
    fn make_cursor_state(
        &self,
        old: &CursorState,
        new_head: Position,
        extend_selection: bool,
    ) -> CursorState {
        let new_selection = if extend_selection {
            Selection::new(old.primary.anchor, new_head)
        } else {
            Selection::collapsed(new_head)
        };

        CursorState::new(new_selection)
    }

    /// Creates a selection command if the cursor changed.
    fn create_selection_command(&self, old: &CursorState, new: &CursorState) -> KeyResult {
        if old == new {
            KeyResult::Handled
        } else {
            KeyResult::Command(Command::SetSelection {
                old_state: old.clone(),
                new_state: new.clone(),
            })
        }
    }

    /// Handles pasting text from clipboard.
    ///
    /// This is called by the editor when clipboard content is available.
    pub fn handle_paste(&self, text: &str, document: &Document, cursor: &CursorState) -> KeyResult {
        self.insert_text(text, document, cursor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_document() -> Document {
        Document::new("Hello World\nSecond Line\nThird Line")
    }

    #[test]
    fn move_char_left() {
        let doc = create_test_document();
        let cursor = CursorState::at(Position::new(0, 5));
        let mut handler = KeyboardHandler::new();

        let result = handler.handle_key(
            &KeyEvent::simple(KeyCode::Left),
            &doc,
            &cursor,
            &UndoTree::new(),
        );

        if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
            assert_eq!(new_state.primary.head, Position::new(0, 4));
        } else {
            panic!("Expected SetSelection command");
        }
    }

    #[test]
    fn move_char_right() {
        let doc = create_test_document();
        let cursor = CursorState::at(Position::new(0, 5));
        let mut handler = KeyboardHandler::new();

        let result = handler.handle_key(
            &KeyEvent::simple(KeyCode::Right),
            &doc,
            &cursor,
            &UndoTree::new(),
        );

        if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
            assert_eq!(new_state.primary.head, Position::new(0, 6));
        } else {
            panic!("Expected SetSelection command");
        }
    }

    #[test]
    fn move_to_next_line() {
        let doc = create_test_document();
        // At end of first line
        let cursor = CursorState::at(Position::new(0, 11));
        let mut handler = KeyboardHandler::new();

        let result = handler.handle_key(
            &KeyEvent::simple(KeyCode::Right),
            &doc,
            &cursor,
            &UndoTree::new(),
        );

        if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
            assert_eq!(new_state.primary.head, Position::new(1, 0));
        } else {
            panic!("Expected SetSelection command");
        }
    }

    #[test]
    fn shift_extends_selection() {
        let doc = create_test_document();
        let cursor = CursorState::at(Position::new(0, 5));
        let mut handler = KeyboardHandler::new();

        let event = KeyEvent::new(KeyCode::Right, Modifiers::shift());
        let result = handler.handle_key(&event, &doc, &cursor, &UndoTree::new());

        if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
            assert_eq!(new_state.primary.anchor, Position::new(0, 5));
            assert_eq!(new_state.primary.head, Position::new(0, 6));
            assert!(!new_state.primary.is_collapsed());
        } else {
            panic!("Expected SetSelection command");
        }
    }

    #[test]
    fn ctrl_moves_by_word() {
        let doc = create_test_document();
        let cursor = CursorState::at(Position::new(0, 6));
        let mut handler = KeyboardHandler::new();

        let event = KeyEvent::new(KeyCode::Left, Modifiers::ctrl());
        let result = handler.handle_key(&event, &doc, &cursor, &UndoTree::new());

        if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
            // Should move to start of "World"
            assert_eq!(new_state.primary.head, Position::new(0, 0));
        } else {
            panic!("Expected SetSelection command");
        }
    }

    #[test]
    fn char_insertion() {
        let doc = Document::new("Hello");
        let cursor = CursorState::at(Position::new(0, 5));
        let mut handler = KeyboardHandler::new();

        let event = KeyEvent::simple(KeyCode::Char('!'));
        let result = handler.handle_key(&event, &doc, &cursor, &UndoTree::new());

        if let KeyResult::Command(Command::Compound { commands }) = result {
            assert!(commands.len() >= 1);
        } else {
            panic!("Expected Compound command");
        }
    }

    #[test]
    fn backspace_deletes_char() {
        let doc = Document::new("Hello");
        let cursor = CursorState::at(Position::new(0, 5));
        let mut handler = KeyboardHandler::new();

        let result = handler.handle_key(
            &KeyEvent::simple(KeyCode::Backspace),
            &doc,
            &cursor,
            &UndoTree::new(),
        );

        if let KeyResult::Command(Command::Compound { commands }) = result {
            assert!(commands.iter().any(|c| matches!(c, Command::Delete { .. })));
        } else {
            panic!("Expected Compound command");
        }
    }

    #[test]
    fn home_moves_to_line_start() {
        let doc = create_test_document();
        let cursor = CursorState::at(Position::new(0, 5));
        let mut handler = KeyboardHandler::new();

        let result = handler.handle_key(
            &KeyEvent::simple(KeyCode::Home),
            &doc,
            &cursor,
            &UndoTree::new(),
        );

        if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
            assert_eq!(new_state.primary.head.column, 0);
        } else {
            panic!("Expected SetSelection command");
        }
    }

    #[test]
    fn end_moves_to_line_end() {
        let doc = create_test_document();
        let cursor = CursorState::at(Position::new(0, 0));
        let mut handler = KeyboardHandler::new();

        let result = handler.handle_key(
            &KeyEvent::simple(KeyCode::End),
            &doc,
            &cursor,
            &UndoTree::new(),
        );

        if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
            assert_eq!(new_state.primary.head, Position::new(0, 11));
        } else {
            panic!("Expected SetSelection command");
        }
    }

    #[test]
    fn escape_collapses_selection() {
        let selection = Selection::new(Position::new(0, 0), Position::new(0, 5));
        let cursor = CursorState::new(selection);
        let doc = Document::new("Hello World");
        let mut handler = KeyboardHandler::new();

        let result = handler.handle_key(
            &KeyEvent::simple(KeyCode::Escape),
            &doc,
            &cursor,
            &UndoTree::new(),
        );

        if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
            assert!(new_state.primary.is_collapsed());
            assert_eq!(new_state.primary.head, Position::new(0, 5));
        } else {
            panic!("Expected SetSelection command");
        }
    }

    #[test]
    fn up_arrow_with_sticky_column() {
        let doc = Document::new("Short\nMedium Line\nShort");
        let cursor = CursorState::at(Position::new(1, 11)); // End of "Medium Line"
        let mut handler = KeyboardHandler::new();

        // Move up - should try to stay at column 11 but will be clamped
        let result = handler.handle_key(
            &KeyEvent::simple(KeyCode::Up),
            &doc,
            &cursor,
            &UndoTree::new(),
        );

        if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
            assert_eq!(new_state.primary.head.line, 0);
            assert_eq!(new_state.primary.head.column, 5); // Clamped to "Short" length
        } else {
            panic!("Expected SetSelection command");
        }
    }

    #[test]
    fn ctrl_d_selects_word_first() {
        let doc = Document::new("foo bar foo baz foo");
        let cursor = CursorState::at(Position::new(0, 1)); // Inside "foo"
        let mut handler = KeyboardHandler::new();

        // First Ctrl+D should select the word
        let event = KeyEvent::new(KeyCode::Char('d'), Modifiers::ctrl());
        let result = handler.handle_key(&event, &doc, &cursor, &UndoTree::new());

        if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
            assert!(!new_state.primary.is_collapsed());
            assert_eq!(new_state.primary.start(), Position::new(0, 0));
            assert_eq!(new_state.primary.end(), Position::new(0, 3)); // "foo"
        } else {
            panic!("Expected SetSelection command");
        }
    }

    #[test]
    fn ctrl_d_adds_next_match() {
        let doc = Document::new("foo bar foo baz foo");
        // Start with "foo" already selected
        let cursor = CursorState::new(Selection::new(Position::new(0, 0), Position::new(0, 3)));
        let mut handler = KeyboardHandler::new();

        // Ctrl+D should find next "foo"
        let event = KeyEvent::new(KeyCode::Char('d'), Modifiers::ctrl());
        let result = handler.handle_key(&event, &doc, &cursor, &UndoTree::new());

        if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
            // Should have 2 cursors now
            assert_eq!(new_state.cursor_count(), 2);
            // Second cursor at "foo" position 8-11
            assert!(
                new_state
                    .secondary
                    .iter()
                    .any(|s| s.start() == Position::new(0, 8))
            );
        } else {
            panic!("Expected SetSelection command");
        }
    }

    #[test]
    fn escape_collapses_multi_cursor() {
        let mut cursor = CursorState::at(Position::new(0, 0));
        cursor.add_cursor(Selection::collapsed(Position::new(1, 0)));
        cursor.add_cursor(Selection::collapsed(Position::new(2, 0)));

        let doc = Document::new("Line 1\nLine 2\nLine 3");
        let mut handler = KeyboardHandler::new();

        let result = handler.handle_key(
            &KeyEvent::simple(KeyCode::Escape),
            &doc,
            &cursor,
            &UndoTree::new(),
        );

        if let KeyResult::Command(Command::SetSelection { new_state, .. }) = result {
            // Should have only 1 cursor
            assert_eq!(new_state.cursor_count(), 1);
            assert!(new_state.secondary.is_empty());
        } else {
            panic!("Expected SetSelection command");
        }
    }
}
