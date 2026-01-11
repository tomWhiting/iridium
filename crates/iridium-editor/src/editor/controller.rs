//! Editor controller for handling input actions.
//!
//! The EditorController integrates the Editor with input handling and
//! event emission to provide a complete editing experience.

use crate::buffer::Buffer;
use crate::events::{
    ContentChangedEvent, CursorMovedEvent, EventEmitter, SelectionChangeReason,
    SelectionChangedEvent,
};
use crate::input::{InputAction, InputResult, KeyEvent, KeyHandler, MouseEvent, MouseHandler};

use super::navigation::{self, select_line_at, select_word_at};
use super::{Editor, Position, Selection};

/// Configuration for the editor controller.
#[derive(Debug, Clone)]
pub struct ControllerConfig {
    /// Number of lines per page (for PageUp/PageDown).
    pub page_lines: usize,
    /// Whether to emit events.
    pub emit_events: bool,
}

impl Default for ControllerConfig {
    fn default() -> Self {
        Self {
            page_lines: 30,
            emit_events: true,
        }
    }
}

/// Clipboard interface for cut/copy/paste operations.
pub trait Clipboard: Send + Sync {
    /// Gets text from the clipboard.
    fn get(&self) -> Option<String>;
    /// Sets text to the clipboard.
    fn set(&self, text: &str);
}

/// A simple in-memory clipboard for testing.
#[derive(Debug, Default)]
pub struct MemoryClipboard {
    content: std::sync::Mutex<String>,
}

impl Clipboard for MemoryClipboard {
    fn get(&self) -> Option<String> {
        let content = self.content.lock().ok()?;
        if content.is_empty() {
            None
        } else {
            Some(content.clone())
        }
    }

    fn set(&self, text: &str) {
        if let Ok(mut content) = self.content.lock() {
            *content = text.to_string();
        }
    }
}

/// Controls the editor in response to input events.
///
/// The controller coordinates:
/// - Input handling (keyboard and mouse)
/// - Action execution (cursor movement, text editing)
/// - Event emission (content changes, selection changes)
pub struct EditorController {
    /// The core editor state.
    editor: Editor,
    /// Keyboard input handler.
    key_handler: KeyHandler,
    /// Mouse input handler.
    mouse_handler: MouseHandler,
    /// Event emitter for notifying listeners.
    event_emitter: EventEmitter,
    /// Configuration.
    config: ControllerConfig,
    /// Clipboard for cut/copy/paste.
    clipboard: Box<dyn Clipboard>,
}

impl EditorController {
    /// Creates a new editor controller.
    #[must_use]
    pub fn new() -> Self {
        Self {
            editor: Editor::new(),
            key_handler: KeyHandler::new(),
            mouse_handler: MouseHandler::new(),
            event_emitter: EventEmitter::new(),
            config: ControllerConfig::default(),
            clipboard: Box::new(MemoryClipboard::default()),
        }
    }

    /// Creates an editor controller with initial content.
    #[must_use]
    pub fn with_content(content: &str) -> Self {
        Self {
            editor: Editor::with_content(content),
            key_handler: KeyHandler::new(),
            mouse_handler: MouseHandler::new(),
            event_emitter: EventEmitter::new(),
            config: ControllerConfig::default(),
            clipboard: Box::new(MemoryClipboard::default()),
        }
    }

    /// Creates an editor controller with configuration.
    #[must_use]
    pub fn with_config(config: ControllerConfig) -> Self {
        Self {
            editor: Editor::new(),
            key_handler: KeyHandler::new(),
            mouse_handler: MouseHandler::new(),
            event_emitter: EventEmitter::new(),
            config,
            clipboard: Box::new(MemoryClipboard::default()),
        }
    }

    /// Sets a custom clipboard implementation.
    pub fn set_clipboard(&mut self, clipboard: Box<dyn Clipboard>) {
        self.clipboard = clipboard;
    }

    /// Returns a reference to the editor.
    #[must_use]
    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    /// Returns a mutable reference to the editor.
    pub fn editor_mut(&mut self) -> &mut Editor {
        &mut self.editor
    }

    /// Returns a reference to the event emitter.
    #[must_use]
    pub fn event_emitter(&self) -> &EventEmitter {
        &self.event_emitter
    }

    /// Returns a mutable reference to the event emitter.
    pub fn event_emitter_mut(&mut self) -> &mut EventEmitter {
        &mut self.event_emitter
    }

    /// Returns a reference to the mouse handler.
    #[must_use]
    pub fn mouse_handler(&self) -> &MouseHandler {
        &self.mouse_handler
    }

    /// Returns a mutable reference to the mouse handler.
    pub fn mouse_handler_mut(&mut self) -> &mut MouseHandler {
        &mut self.mouse_handler
    }

    /// Returns a reference to the key handler.
    #[must_use]
    pub fn key_handler(&self) -> &KeyHandler {
        &self.key_handler
    }

    /// Returns a mutable reference to the key handler.
    pub fn key_handler_mut(&mut self) -> &mut KeyHandler {
        &mut self.key_handler
    }

    /// Sets the page size for PageUp/PageDown.
    pub fn set_page_lines(&mut self, lines: usize) {
        self.config.page_lines = lines;
    }

    /// Handles a keyboard event.
    pub fn handle_key_event(&mut self, event: &KeyEvent) -> InputResult {
        let result = self.key_handler.handle_key(event);
        for action in &result.actions {
            self.execute_action(action.clone());
        }
        result
    }

    /// Handles a mouse event.
    pub fn handle_mouse_event(&mut self, event: &MouseEvent) -> InputResult {
        let result = self.mouse_handler.handle_mouse(event);
        for action in &result.actions {
            self.execute_action(action.clone());
        }
        result
    }

    /// Handles direct text input (bypassing key events).
    pub fn handle_text_input(&mut self, text: &str) -> InputResult {
        let result = self.key_handler.handle_text_input(text);
        for action in &result.actions {
            self.execute_action(action.clone());
        }
        result
    }

    /// Executes an input action.
    pub fn execute_action(&mut self, action: InputAction) {
        let previous_position = self.editor.cursor_position();
        let previous_selection = self.editor.selection();

        match action {
            // Cursor movement (no selection)
            InputAction::MoveLeft => {
                self.move_cursor(|buffer, pos, _| navigation::move_left(buffer, pos))
            }
            InputAction::MoveRight => {
                self.move_cursor(|buffer, pos, _| navigation::move_right(buffer, pos))
            }
            InputAction::MoveUp => {
                let preferred = self.get_preferred_column();
                self.move_cursor_with_preferred(|buffer, pos, _| {
                    navigation::move_up(buffer, pos, preferred)
                });
            }
            InputAction::MoveDown => {
                let preferred = self.get_preferred_column();
                self.move_cursor_with_preferred(|buffer, pos, _| {
                    navigation::move_down(buffer, pos, preferred)
                });
            }
            InputAction::MoveToLineStart => {
                self.move_cursor(|_, pos, _| navigation::move_to_line_start(pos))
            }
            InputAction::MoveToLineEnd => {
                self.move_cursor(|buffer, pos, _| navigation::move_to_line_end(buffer, pos))
            }
            InputAction::MoveWordLeft => {
                self.move_cursor(|buffer, pos, _| navigation::move_word_left(buffer, pos))
            }
            InputAction::MoveWordRight => {
                self.move_cursor(|buffer, pos, _| navigation::move_word_right(buffer, pos))
            }
            InputAction::PageUp => {
                let page_lines = self.config.page_lines;
                let preferred = self.get_preferred_column();
                self.move_cursor_with_preferred(|buffer, pos, _| {
                    navigation::page_up(buffer, pos, page_lines, preferred)
                });
            }
            InputAction::PageDown => {
                let page_lines = self.config.page_lines;
                let preferred = self.get_preferred_column();
                self.move_cursor_with_preferred(|buffer, pos, _| {
                    navigation::page_down(buffer, pos, page_lines, preferred)
                });
            }
            InputAction::MoveToDocumentStart => {
                self.move_cursor(|_, _, _| navigation::move_to_document_start())
            }
            InputAction::MoveToDocumentEnd => {
                self.move_cursor(|buffer, _, _| navigation::move_to_document_end(buffer))
            }

            // Selection (extend selection)
            InputAction::SelectLeft => {
                self.extend_selection(|buffer, pos, _| navigation::move_left(buffer, pos))
            }
            InputAction::SelectRight => {
                self.extend_selection(|buffer, pos, _| navigation::move_right(buffer, pos))
            }
            InputAction::SelectUp => {
                let preferred = self.get_preferred_column();
                self.extend_selection(|buffer, pos, _| navigation::move_up(buffer, pos, preferred));
            }
            InputAction::SelectDown => {
                let preferred = self.get_preferred_column();
                self.extend_selection(|buffer, pos, _| {
                    navigation::move_down(buffer, pos, preferred)
                });
            }
            InputAction::SelectToLineStart => {
                self.extend_selection(|_, pos, _| navigation::move_to_line_start(pos))
            }
            InputAction::SelectToLineEnd => {
                self.extend_selection(|buffer, pos, _| navigation::move_to_line_end(buffer, pos))
            }
            InputAction::SelectWordLeft => {
                self.extend_selection(|buffer, pos, _| navigation::move_word_left(buffer, pos))
            }
            InputAction::SelectWordRight => {
                self.extend_selection(|buffer, pos, _| navigation::move_word_right(buffer, pos))
            }
            InputAction::SelectAll => {
                self.editor.select_all();
                self.emit_selection_changed(previous_selection, SelectionChangeReason::SelectAll);
            }
            InputAction::SelectPageUp => {
                let page_lines = self.config.page_lines;
                let preferred = self.get_preferred_column();
                self.extend_selection(|buffer, pos, _| {
                    navigation::page_up(buffer, pos, page_lines, preferred)
                });
            }
            InputAction::SelectPageDown => {
                let page_lines = self.config.page_lines;
                let preferred = self.get_preferred_column();
                self.extend_selection(|buffer, pos, _| {
                    navigation::page_down(buffer, pos, page_lines, preferred)
                });
            }
            InputAction::SelectToDocumentStart => {
                self.extend_selection(|_, _, _| navigation::move_to_document_start())
            }
            InputAction::SelectToDocumentEnd => {
                self.extend_selection(|buffer, _, _| navigation::move_to_document_end(buffer))
            }

            // Text editing
            InputAction::InsertChar(c) => {
                self.insert_text(&c.to_string());
            }
            InputAction::InsertText(text) => {
                self.insert_text(&text);
            }
            InputAction::InsertNewline => {
                self.insert_text("\n");
            }
            InputAction::Backspace => {
                self.backspace();
            }
            InputAction::Delete => {
                self.delete();
            }
            InputAction::DeleteWordBefore => {
                self.delete_word_before();
            }
            InputAction::DeleteWordAfter => {
                self.delete_word_after();
            }
            InputAction::DeleteLine => {
                self.delete_line();
            }

            // Clipboard
            InputAction::Cut => {
                self.cut();
            }
            InputAction::Copy => {
                self.copy();
            }
            InputAction::Paste => {
                self.paste();
            }

            // History
            InputAction::Undo => {
                self.undo();
            }
            InputAction::Redo => {
                self.redo();
            }

            // Mouse actions
            InputAction::Click(position) => {
                self.click(position);
            }
            InputAction::ShiftClick(position) => {
                self.shift_click(position);
            }
            InputAction::DragStart(position) => {
                self.drag_start(position);
            }
            InputAction::DragMove(position) => {
                self.drag_move(position);
            }
            InputAction::DragEnd => {
                // No action needed, state is already updated
            }
            InputAction::DoubleClick(position) => {
                self.double_click(position);
            }
            InputAction::TripleClick(position) => {
                self.triple_click(position);
            }

            // IME
            InputAction::ImeCompositionStart => {
                self.key_handler.set_ime_active(true);
            }
            InputAction::ImeCompositionUpdate(_text) => {
                // IME preview is handled by the UI layer
            }
            InputAction::ImeCompositionEnd(text) => {
                self.key_handler.set_ime_active(false);
                self.insert_text(&text);
            }
            InputAction::ImeCompositionCancel => {
                self.key_handler.set_ime_active(false);
            }

            InputAction::None => {}
        }

        // Emit cursor moved event if position changed
        let new_position = self.editor.cursor_position();
        if new_position != previous_position {
            self.emit_cursor_moved(previous_position, new_position);
        }
    }

    /// Returns the preferred column for vertical movement.
    fn get_preferred_column(&self) -> usize {
        self.editor.cursor_position().column
    }

    /// Moves the cursor using the provided movement function.
    fn move_cursor<F>(&mut self, f: F)
    where
        F: FnOnce(&Buffer, Position, Option<Selection>) -> Position,
    {
        let previous_selection = self.editor.selection();
        let position = self.editor.cursor_position();
        let new_position = f(self.editor.buffer(), position, previous_selection);
        self.editor.move_cursor_to(new_position);
        self.emit_selection_changed(previous_selection, SelectionChangeReason::CursorMove);
    }

    /// Moves the cursor while preserving preferred column state.
    fn move_cursor_with_preferred<F>(&mut self, f: F)
    where
        F: FnOnce(&Buffer, Position, Option<Selection>) -> Position,
    {
        let previous_selection = self.editor.selection();
        let position = self.editor.cursor_position();
        let new_position = f(self.editor.buffer(), position, previous_selection);
        self.editor.move_cursor_to(new_position);
        self.emit_selection_changed(previous_selection, SelectionChangeReason::CursorMove);
    }

    /// Extends the selection using the provided movement function.
    fn extend_selection<F>(&mut self, f: F)
    where
        F: FnOnce(&Buffer, Position, Option<Selection>) -> Position,
    {
        let previous_selection = self.editor.selection();
        let position = self.editor.cursor_position();
        let new_position = f(self.editor.buffer(), position, previous_selection);
        self.editor.extend_selection_to(new_position);
        self.emit_selection_changed(previous_selection, SelectionChangeReason::KeyboardSelect);
    }

    /// Inserts text at the cursor position.
    fn insert_text(&mut self, text: &str) {
        let start = self.editor.cursor_position();
        let removed_text = if let Some(sel) = self.editor.selection() {
            let start_idx = navigation::position_to_char(self.editor.buffer(), sel.start());
            let end_idx = navigation::position_to_char(self.editor.buffer(), sel.end());
            self.editor.buffer().slice(start_idx, end_idx).to_string()
        } else {
            String::new()
        };

        self.editor.insert(text);

        let end = self.editor.cursor_position();
        self.emit_content_changed(start, start, removed_text, text.to_string(), end);
    }

    /// Handles backspace.
    fn backspace(&mut self) {
        let start = self.editor.cursor_position();
        let selection = self.editor.selection();

        let (removed_start, removed_text) = if let Some(sel) = selection {
            let s = sel.start();
            let e = sel.end();
            let start_idx = navigation::position_to_char(self.editor.buffer(), s);
            let end_idx = navigation::position_to_char(self.editor.buffer(), e);
            (
                s,
                self.editor.buffer().slice(start_idx, end_idx).to_string(),
            )
        } else if !start.is_origin() {
            let new_pos = navigation::move_left(self.editor.buffer(), start);
            let start_idx = navigation::position_to_char(self.editor.buffer(), new_pos);
            let end_idx = navigation::position_to_char(self.editor.buffer(), start);
            (
                new_pos,
                self.editor.buffer().slice(start_idx, end_idx).to_string(),
            )
        } else {
            return; // Nothing to delete
        };

        self.editor.backspace();

        let end = self.editor.cursor_position();
        self.emit_content_changed(removed_start, start, removed_text, String::new(), end);
    }

    /// Handles delete.
    fn delete(&mut self) {
        let start = self.editor.cursor_position();
        let selection = self.editor.selection();

        let (removed_start, removed_end, removed_text) = if let Some(sel) = selection {
            let s = sel.start();
            let e = sel.end();
            let start_idx = navigation::position_to_char(self.editor.buffer(), s);
            let end_idx = navigation::position_to_char(self.editor.buffer(), e);
            (
                s,
                e,
                self.editor.buffer().slice(start_idx, end_idx).to_string(),
            )
        } else {
            let char_idx = navigation::position_to_char(self.editor.buffer(), start);
            if char_idx >= self.editor.buffer().len_chars() {
                return; // Nothing to delete
            }
            let removed_char = self.editor.buffer().char_at(char_idx).to_string();
            let end_pos = navigation::move_right(self.editor.buffer(), start);
            (start, end_pos, removed_char)
        };

        self.editor.delete();

        let cursor_end = self.editor.cursor_position();
        self.emit_content_changed(
            removed_start,
            removed_end,
            removed_text,
            String::new(),
            cursor_end,
        );
    }

    /// Deletes the word before the cursor.
    fn delete_word_before(&mut self) {
        let start = self.editor.cursor_position();
        if start.is_origin() {
            return;
        }

        let word_start = navigation::move_word_left(self.editor.buffer(), start);
        let start_idx = navigation::position_to_char(self.editor.buffer(), word_start);
        let end_idx = navigation::position_to_char(self.editor.buffer(), start);
        let removed_text = self.editor.buffer().slice(start_idx, end_idx).to_string();

        self.editor.move_cursor_to(word_start);
        self.editor.extend_selection_to(start);
        self.editor.delete_selection();

        self.emit_content_changed(word_start, start, removed_text, String::new(), word_start);
    }

    /// Deletes the word after the cursor.
    fn delete_word_after(&mut self) {
        let start = self.editor.cursor_position();
        let char_idx = navigation::position_to_char(self.editor.buffer(), start);
        if char_idx >= self.editor.buffer().len_chars() {
            return;
        }

        let word_end = navigation::move_word_right(self.editor.buffer(), start);
        let end_idx = navigation::position_to_char(self.editor.buffer(), word_end);
        let removed_text = self.editor.buffer().slice(char_idx, end_idx).to_string();

        self.editor.extend_selection_to(word_end);
        self.editor.delete_selection();

        self.emit_content_changed(start, word_end, removed_text, String::new(), start);
    }

    /// Deletes the current line.
    fn delete_line(&mut self) {
        let position = self.editor.cursor_position();
        let (line_start, line_end) = select_line_at(self.editor.buffer(), position);

        let start_idx = navigation::position_to_char(self.editor.buffer(), line_start);
        let end_idx = navigation::position_to_char(self.editor.buffer(), line_end);
        let removed_text = self.editor.buffer().slice(start_idx, end_idx).to_string();

        self.editor.move_cursor_to(line_start);
        self.editor.extend_selection_to(line_end);
        self.editor.delete_selection();

        self.emit_content_changed(
            line_start,
            line_end,
            removed_text,
            String::new(),
            line_start,
        );
    }

    /// Cuts the selection to clipboard.
    fn cut(&mut self) {
        if let Some(sel) = self.editor.selection() {
            let start = sel.start();
            let end = sel.end();
            let start_idx = navigation::position_to_char(self.editor.buffer(), start);
            let end_idx = navigation::position_to_char(self.editor.buffer(), end);
            let text = self.editor.buffer().slice(start_idx, end_idx).to_string();

            self.clipboard.set(&text);
            self.editor.delete_selection();

            self.emit_content_changed(start, end, text, String::new(), start);
        }
    }

    /// Copies the selection to clipboard.
    fn copy(&mut self) {
        if let Some(sel) = self.editor.selection() {
            let start_idx = navigation::position_to_char(self.editor.buffer(), sel.start());
            let end_idx = navigation::position_to_char(self.editor.buffer(), sel.end());
            let text = self.editor.buffer().slice(start_idx, end_idx).to_string();
            self.clipboard.set(&text);
        }
    }

    /// Pastes from clipboard.
    fn paste(&mut self) {
        if let Some(text) = self.clipboard.get() {
            self.insert_text(&text);
        }
    }

    /// Undoes the last action.
    fn undo(&mut self) {
        let previous_selection = self.editor.selection();
        if self.editor.undo() {
            self.emit_selection_changed(previous_selection, SelectionChangeReason::ContentChange);
        }
    }

    /// Redoes the last undone action.
    fn redo(&mut self) {
        let previous_selection = self.editor.selection();
        if self.editor.redo() {
            self.emit_selection_changed(previous_selection, SelectionChangeReason::ContentChange);
        }
    }

    /// Handles a mouse click.
    fn click(&mut self, position: Position) {
        let previous_selection = self.editor.selection();
        self.editor.move_cursor_to(position);
        self.emit_selection_changed(previous_selection, SelectionChangeReason::MouseClick);
    }

    /// Handles a shift-click to extend selection.
    fn shift_click(&mut self, position: Position) {
        let previous_selection = self.editor.selection();
        self.editor.extend_selection_to(position);
        self.emit_selection_changed(previous_selection, SelectionChangeReason::MouseClick);
    }

    /// Handles drag start.
    fn drag_start(&mut self, position: Position) {
        let previous_selection = self.editor.selection();
        self.editor.move_cursor_to(position);
        self.emit_selection_changed(previous_selection, SelectionChangeReason::MouseDrag);
    }

    /// Handles drag move.
    fn drag_move(&mut self, position: Position) {
        let previous_selection = self.editor.selection();
        self.editor.extend_selection_to(position);
        self.emit_selection_changed(previous_selection, SelectionChangeReason::MouseDrag);
    }

    /// Handles double-click to select word.
    fn double_click(&mut self, position: Position) {
        let previous_selection = self.editor.selection();
        let (start, end) = select_word_at(self.editor.buffer(), position);
        self.editor.move_cursor_to(start);
        self.editor.extend_selection_to(end);
        self.emit_selection_changed(previous_selection, SelectionChangeReason::WordSelect);
    }

    /// Handles triple-click to select line.
    fn triple_click(&mut self, position: Position) {
        let previous_selection = self.editor.selection();
        let (start, end) = select_line_at(self.editor.buffer(), position);
        self.editor.move_cursor_to(start);
        self.editor.extend_selection_to(end);
        self.emit_selection_changed(previous_selection, SelectionChangeReason::LineSelect);
    }

    /// Emits a content changed event.
    fn emit_content_changed(
        &mut self,
        start: Position,
        end: Position,
        removed_text: String,
        inserted_text: String,
        cursor_position: Position,
    ) {
        if !self.config.emit_events {
            return;
        }

        let event =
            ContentChangedEvent::new(start, end, removed_text, inserted_text, cursor_position);
        self.event_emitter.emit_content_changed(event);
    }

    /// Emits a selection changed event.
    fn emit_selection_changed(
        &mut self,
        previous: Option<Selection>,
        reason: SelectionChangeReason,
    ) {
        if !self.config.emit_events {
            return;
        }

        let current = self
            .editor
            .selection()
            .unwrap_or_else(|| Selection::cursor(self.editor.cursor_position()));

        let event = SelectionChangedEvent::new(previous, current, reason);
        self.event_emitter.emit_selection_changed(event);
    }

    /// Emits a cursor moved event.
    fn emit_cursor_moved(&mut self, previous: Position, current: Position) {
        if !self.config.emit_events {
            return;
        }

        let event = CursorMovedEvent::new(previous, current);
        self.event_emitter.emit_cursor_moved(event);
    }
}

impl Default for EditorController {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for EditorController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditorController")
            .field("editor", &self.editor)
            .field("key_handler", &self.key_handler)
            .field("mouse_handler", &self.mouse_handler)
            .field("event_emitter", &self.event_emitter)
            .field("config", &self.config)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{Key, Modifiers};

    #[test]
    fn test_controller_new() {
        let controller = EditorController::new();
        assert_eq!(controller.editor().content(), "");
        assert_eq!(controller.editor().cursor_position(), Position::origin());
    }

    #[test]
    fn test_controller_with_content() {
        let controller = EditorController::with_content("Hello, world!");
        assert_eq!(controller.editor().content(), "Hello, world!");
    }

    #[test]
    fn test_insert_char() {
        let mut controller = EditorController::new();
        controller.execute_action(InputAction::InsertChar('H'));
        controller.execute_action(InputAction::InsertChar('i'));
        assert_eq!(controller.editor().content(), "Hi");
        assert_eq!(controller.editor().cursor_position(), Position::new(0, 2));
    }

    #[test]
    fn test_insert_text() {
        let mut controller = EditorController::new();
        controller.execute_action(InputAction::InsertText("Hello".to_string()));
        assert_eq!(controller.editor().content(), "Hello");
    }

    #[test]
    fn test_backspace() {
        let mut controller = EditorController::with_content("Hello");
        controller.editor_mut().move_cursor_to(Position::new(0, 5));
        controller.execute_action(InputAction::Backspace);
        assert_eq!(controller.editor().content(), "Hell");
    }

    #[test]
    fn test_delete() {
        let mut controller = EditorController::with_content("Hello");
        controller.editor_mut().move_cursor_to(Position::new(0, 0));
        controller.execute_action(InputAction::Delete);
        assert_eq!(controller.editor().content(), "ello");
    }

    #[test]
    fn test_cursor_movement() {
        let mut controller = EditorController::with_content("Hello\nWorld");
        controller.editor_mut().move_cursor_to(Position::new(0, 0));

        // Move right
        controller.execute_action(InputAction::MoveRight);
        assert_eq!(controller.editor().cursor_position(), Position::new(0, 1));

        // Move to end of line
        controller.execute_action(InputAction::MoveToLineEnd);
        assert_eq!(controller.editor().cursor_position(), Position::new(0, 5));

        // Move down
        controller.execute_action(InputAction::MoveDown);
        assert_eq!(controller.editor().cursor_position(), Position::new(1, 5));

        // Move to line start
        controller.execute_action(InputAction::MoveToLineStart);
        assert_eq!(controller.editor().cursor_position(), Position::new(1, 0));
    }

    #[test]
    fn test_selection() {
        let mut controller = EditorController::with_content("Hello, world!");
        controller.editor_mut().move_cursor_to(Position::new(0, 0));

        // Select right
        controller.execute_action(InputAction::SelectRight);
        controller.execute_action(InputAction::SelectRight);
        controller.execute_action(InputAction::SelectRight);

        let selection = controller.editor().selection();
        assert!(selection.is_some());
        let sel = selection.unwrap();
        assert_eq!(sel.start(), Position::new(0, 0));
        assert_eq!(sel.end(), Position::new(0, 3));
    }

    #[test]
    fn test_select_all() {
        let mut controller = EditorController::with_content("Hello, world!");
        controller.execute_action(InputAction::SelectAll);

        let selection = controller.editor().selection();
        assert!(selection.is_some());
        let sel = selection.unwrap();
        assert_eq!(sel.start(), Position::new(0, 0));
        assert_eq!(sel.end(), Position::new(0, 13));
    }

    #[test]
    fn test_word_navigation() {
        let mut controller = EditorController::with_content("hello world foo");
        controller.editor_mut().move_cursor_to(Position::new(0, 0));

        // Move word right
        controller.execute_action(InputAction::MoveWordRight);
        assert_eq!(controller.editor().cursor_position(), Position::new(0, 6)); // start of "world"

        // Move word right again
        controller.execute_action(InputAction::MoveWordRight);
        assert_eq!(controller.editor().cursor_position(), Position::new(0, 12)); // start of "foo"

        // Move word left
        controller.execute_action(InputAction::MoveWordLeft);
        assert_eq!(controller.editor().cursor_position(), Position::new(0, 6)); // start of "world"
    }

    #[test]
    fn test_delete_word() {
        let mut controller = EditorController::with_content("hello world");
        controller.editor_mut().move_cursor_to(Position::new(0, 11));

        // Delete word before
        controller.execute_action(InputAction::DeleteWordBefore);
        assert_eq!(controller.editor().content(), "hello ");
    }

    #[test]
    fn test_cut_copy_paste() {
        let mut controller = EditorController::with_content("Hello, world!");
        controller.editor_mut().move_cursor_to(Position::new(0, 7));
        controller
            .editor_mut()
            .extend_selection_to(Position::new(0, 12));

        // Copy
        controller.execute_action(InputAction::Copy);
        assert_eq!(controller.editor().content(), "Hello, world!");

        // Paste at end
        controller.editor_mut().move_cursor_to(Position::new(0, 13));
        controller.execute_action(InputAction::Paste);
        assert_eq!(controller.editor().content(), "Hello, world!world");

        // Cut
        controller.editor_mut().move_cursor_to(Position::new(0, 7));
        controller
            .editor_mut()
            .extend_selection_to(Position::new(0, 12));
        controller.execute_action(InputAction::Cut);
        assert_eq!(controller.editor().content(), "Hello, !world");
    }

    #[test]
    fn test_undo_redo() {
        let mut controller = EditorController::new();
        controller.execute_action(InputAction::InsertText("Hello".to_string()));
        assert_eq!(controller.editor().content(), "Hello");

        controller.execute_action(InputAction::Undo);
        assert_eq!(controller.editor().content(), "");

        controller.execute_action(InputAction::Redo);
        assert_eq!(controller.editor().content(), "Hello");
    }

    #[test]
    fn test_click() {
        let mut controller = EditorController::with_content("Hello, world!");
        controller.execute_action(InputAction::Click(Position::new(0, 7)));
        assert_eq!(controller.editor().cursor_position(), Position::new(0, 7));
    }

    #[test]
    fn test_double_click_word_selection() {
        let mut controller = EditorController::with_content("hello world");
        controller.execute_action(InputAction::DoubleClick(Position::new(0, 2)));

        let selection = controller.editor().selection();
        assert!(selection.is_some());
        let sel = selection.unwrap();
        assert_eq!(sel.start(), Position::new(0, 0)); // "hello" starts at 0
        assert_eq!(sel.end(), Position::new(0, 5)); // "hello" ends at 5
    }

    #[test]
    fn test_triple_click_line_selection() {
        let mut controller = EditorController::with_content("hello\nworld");
        controller.execute_action(InputAction::TripleClick(Position::new(0, 2)));

        let selection = controller.editor().selection();
        assert!(selection.is_some());
        let sel = selection.unwrap();
        assert_eq!(sel.start(), Position::new(0, 0));
        assert_eq!(sel.end(), Position::new(1, 0)); // Selects entire line including newline
    }

    #[test]
    fn test_handle_key_event() {
        let mut controller = EditorController::new();

        // Type a character
        let event = KeyEvent::new(Key::Char('H'), Modifiers::new());
        controller.handle_key_event(&event);
        assert_eq!(controller.editor().content(), "H");

        // Press Enter
        let event = KeyEvent::new(Key::Enter, Modifiers::new());
        controller.handle_key_event(&event);
        assert_eq!(controller.editor().content(), "H\n");
    }

    #[test]
    fn test_handle_text_input() {
        let mut controller = EditorController::new();
        controller.handle_text_input("Hello, world!");
        assert_eq!(controller.editor().content(), "Hello, world!");
    }
}
