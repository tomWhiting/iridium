//! Core editor state and main editor instance.

use serde::{Deserialize, Serialize};

use super::config::EditorConfig;
use crate::document::{CursorState, Document, Position, Selection};
use crate::history::{Command, UndoTree};
use crate::input::{
    ClipboardOperation, ImeEvent, ImeHandler, ImeResult, ImeState, KeyEvent, KeyResult,
    KeyboardHandler, MouseEvent, MouseHandler, MouseResult, SearchAction,
};
use crate::render::Viewport;
use crate::search::{SearchOptions, SearchState, replace_all, replace_current};
use crate::theme::Theme;

/// Events emitted by the editor to the host application.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EditorEvent {
    /// Document content changed.
    ContentChanged {
        /// New full content
        content: String,
    },

    /// Cursor or selection changed.
    SelectionChanged {
        /// All current selections
        selections: Vec<Selection>,
    },

    /// Scroll position changed.
    ScrollChanged {
        /// First visible line
        first_line: usize,
    },

    /// Search results updated.
    SearchUpdated {
        /// Number of matches
        match_count: usize,
        /// Current match index (if any)
        current_index: Option<usize>,
    },

    /// Theme changed (T147).
    ///
    /// Emitted when the theme is updated at runtime.
    ThemeChanged {
        /// Name of the new theme
        theme_name: String,
        /// Whether the new theme is dark
        is_dark: bool,
    },

    /// An error occurred.
    Error {
        /// Error message
        message: String,
        /// Error code for programmatic handling
        code: String,
    },
}

/// Result of handling a keyboard event in the editor.
///
/// Used to communicate what type of action was requested by the keyboard input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorKeyResult {
    /// No special action requested.
    None,
    /// A clipboard operation was requested.
    Clipboard(ClipboardOperation),
    /// A search action was requested.
    Search(SearchAction),
}

/// Complete state of an editor instance.
///
/// This struct holds all the state needed to render and interact with
/// the editor. It is the source of truth for the editor's current state.
#[derive(Debug)]
pub struct EditorState {
    /// The document being edited
    pub document: Document,

    /// Cursor and selection state
    pub cursor: CursorState,

    /// Undo/redo history
    pub history: UndoTree,

    /// Current theme
    pub theme: Theme,

    /// Editor configuration
    pub config: EditorConfig,

    /// Viewport for visible region tracking
    pub viewport: Viewport,

    /// Current scroll position (first visible line)
    pub scroll_line: usize,

    /// Horizontal scroll offset in pixels
    pub scroll_x: f32,

    /// Whether the editor has focus
    pub has_focus: bool,

    /// Whether the editor is read-only
    pub read_only: bool,

    /// Search state (T126)
    pub search: SearchState,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            document: Document::default(),
            cursor: CursorState::at(Position::zero()),
            history: UndoTree::new(),
            theme: Theme::default(),
            config: EditorConfig::default(),
            viewport: Viewport::default(),
            scroll_line: 0,
            scroll_x: 0.0,
            has_focus: false,
            read_only: false,
            search: SearchState::new(),
        }
    }
}

impl EditorState {
    /// Creates a new editor state with the given content.
    #[must_use]
    pub fn new(content: &str) -> Self {
        Self {
            document: Document::new(content),
            ..Self::default()
        }
    }

    /// Creates an empty editor state.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Returns the current content as a string.
    #[must_use]
    pub fn content(&self) -> String {
        self.document.text()
    }

    /// Sets the content, resetting cursor and history.
    pub fn set_content(&mut self, content: &str) {
        self.document = Document::new(content);
        self.cursor = CursorState::at(Position::zero());
        self.history = UndoTree::new();
        self.scroll_line = 0;
        self.scroll_x = 0.0;
    }
}

/// The main Iridium editor instance.
///
/// This is the primary interface for embedding the editor. It wraps
/// [`EditorState`] and provides methods for interaction.
///
/// # Example
///
/// ```ignore
/// use iridium_editor::{Editor, EditorConfig};
///
/// let editor = Editor::new(EditorConfig::default())?;
/// editor.set_content("Hello, world!");
/// ```
#[allow(clippy::type_complexity)]
pub struct Editor {
    /// The editor state
    state: EditorState,

    /// Event listeners
    listeners: Vec<Box<dyn Fn(&EditorEvent) + Send + Sync>>,

    /// Keyboard input handler
    keyboard_handler: KeyboardHandler,

    /// Mouse input handler
    mouse_handler: MouseHandler,

    /// IME composition handler
    ime_handler: ImeHandler,
}

impl Editor {
    /// Creates a new editor with the given configuration.
    #[must_use]
    pub fn new(config: EditorConfig) -> Self {
        Self {
            state: EditorState {
                config,
                ..EditorState::default()
            },
            listeners: Vec::new(),
            keyboard_handler: KeyboardHandler::new(),
            mouse_handler: MouseHandler::new(),
            ime_handler: ImeHandler::new(),
        }
    }

    /// Creates an editor with default configuration.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(EditorConfig::default())
    }

    /// Returns a reference to the editor state.
    #[must_use]
    pub const fn state(&self) -> &EditorState {
        &self.state
    }

    /// Returns a mutable reference to the editor state.
    pub const fn state_mut(&mut self) -> &mut EditorState {
        &mut self.state
    }

    /// Sets the content, replacing everything.
    pub fn set_content(&mut self, content: &str) {
        self.state.set_content(content);
        self.emit_content_changed();
    }

    /// Returns the current content.
    #[must_use]
    pub fn content(&self) -> String {
        self.state.content()
    }

    /// Returns the current cursor position.
    #[must_use]
    pub const fn cursor(&self) -> Position {
        self.state.cursor.primary.head
    }

    /// Sets the cursor position.
    pub fn set_cursor(&mut self, position: Position) {
        let clamped = self.state.document.clamp_position(position);
        self.state.cursor = CursorState::at(clamped);
        self.emit_selection_changed();
    }

    /// Handles a keyboard event.
    ///
    /// Returns an `EditorKeyResult` indicating if a clipboard or search action
    /// was requested. The caller is responsible for handling these operations.
    pub fn handle_key(&mut self, event: &KeyEvent) -> EditorKeyResult {
        if self.state.read_only {
            // In read-only mode, only allow navigation (no edits)
            let result = self.keyboard_handler.handle_key(
                event,
                &self.state.document,
                &self.state.cursor,
                &self.state.history,
            );

            match result {
                KeyResult::Command(cmd) => {
                    // Only apply selection changes in read-only mode
                    if matches!(cmd, Command::SetSelection { .. }) {
                        self.apply_command(cmd);
                    }
                },
                KeyResult::Handled | KeyResult::Ignored => {},
                KeyResult::Clipboard(clip) => {
                    // Only allow copy in read-only mode
                    if matches!(clip, ClipboardOperation::Copy(_)) {
                        return EditorKeyResult::Clipboard(clip);
                    }
                },
                // Search actions are allowed in read-only mode
                KeyResult::Search(action) => {
                    return self.handle_search_action(action);
                },
            }
            return EditorKeyResult::None;
        }

        let result = self.keyboard_handler.handle_key(
            event,
            &self.state.document,
            &self.state.cursor,
            &self.state.history,
        );

        match result {
            KeyResult::Command(cmd) => {
                self.apply_command(cmd);
                EditorKeyResult::None
            },
            KeyResult::Clipboard(clip) => EditorKeyResult::Clipboard(clip),
            KeyResult::Search(action) => self.handle_search_action(action),
            KeyResult::Handled | KeyResult::Ignored => EditorKeyResult::None,
        }
    }

    /// Handles a search action from keyboard input.
    fn handle_search_action(&mut self, action: SearchAction) -> EditorKeyResult {
        match action {
            SearchAction::OpenSearch => EditorKeyResult::Search(action),
            SearchAction::CloseSearch => {
                self.close_search();
                EditorKeyResult::Search(action)
            },
            SearchAction::NextMatch => {
                self.goto_next_match();
                EditorKeyResult::None
            },
            SearchAction::PreviousMatch => {
                self.goto_previous_match();
                EditorKeyResult::None
            },
        }
    }

    /// Handles a mouse event.
    pub fn handle_mouse(&mut self, event: &MouseEvent) {
        let result = self.mouse_handler.handle_mouse(
            event,
            &self.state.document,
            &self.state.cursor,
            &self.state.viewport,
        );

        match result {
            MouseResult::Command(cmd) => {
                self.apply_command(cmd);
            },
            MouseResult::Scroll { delta_x, delta_y } => {
                // Handle scrolling
                self.scroll_by(delta_x, delta_y);
            },
            MouseResult::Handled | MouseResult::Ignored => {},
        }
    }

    /// Handles an IME event for input method composition.
    ///
    /// Returns the current IME state if it changed, allowing the host to
    /// render composition text inline.
    pub fn handle_ime(&mut self, event: &ImeEvent) -> Option<ImeState> {
        if self.state.read_only {
            return None;
        }

        let result = self
            .ime_handler
            .handle_ime(event, &self.state.document, &self.state.cursor);

        match result {
            ImeResult::Command(cmd) => {
                self.apply_command(cmd);
                None
            },
            ImeResult::StateChanged(state) => Some(state),
            ImeResult::Handled | ImeResult::Ignored => None,
        }
    }

    /// Returns the current IME composition state.
    #[must_use]
    pub const fn ime_state(&self) -> &ImeState {
        self.ime_handler.state()
    }

    /// Returns true if IME composition is in progress.
    #[must_use]
    pub fn is_ime_composing(&self) -> bool {
        self.ime_handler.is_composing()
    }

    /// Returns the current IME composition text, if any.
    #[must_use]
    pub fn ime_composition_text(&self) -> Option<&str> {
        self.ime_handler.composition_text()
    }

    /// Scrolls the viewport by the given amount.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn scroll_by(&mut self, delta_x: f32, delta_y: f32) {
        self.state.scroll_x = (self.state.scroll_x + delta_x).max(0.0);

        // Convert pixel delta to lines (assuming ~20px line height)
        let line_delta = (delta_y / 20.0) as i32;
        let new_line = if line_delta < 0 {
            self.state
                .scroll_line
                .saturating_sub(line_delta.unsigned_abs() as usize)
        } else {
            self.state.scroll_line.saturating_add(line_delta as usize)
        };

        // Clamp to document bounds
        let max_line = self.state.document.line_count().saturating_sub(1);
        self.state.scroll_line = new_line.min(max_line);

        self.emit(&EditorEvent::ScrollChanged {
            first_line: self.state.scroll_line,
        });
    }

    /// Applies a command to the document.
    ///
    /// This method handles both document modifications and selection changes,
    /// emitting appropriate events. Errors during command application are
    /// emitted as error events.
    pub fn apply_command(&mut self, command: Command) {
        let content_changed = command.modifies_content();
        let selection_changed = command.modifies_selection();

        // Apply the command
        if let Err(e) = command.apply(&mut self.state.document, &mut self.state.cursor) {
            self.emit(&EditorEvent::Error {
                message: e.to_string(),
                code: "COMMAND_FAILED".to_string(),
            });
            return;
        }

        // Push to undo history if it modifies content
        if content_changed {
            self.state.history.push(command);
            self.emit_content_changed();
        }

        // Emit selection changed if needed
        if selection_changed || content_changed {
            self.emit_selection_changed();
        }
    }

    /// Performs undo.
    ///
    /// Returns true if an action was undone.
    pub fn undo(&mut self) -> bool {
        if let Some(cmd) = self.state.history.undo() {
            if let Err(e) = cmd
                .inverse()
                .apply(&mut self.state.document, &mut self.state.cursor)
            {
                self.emit(&EditorEvent::Error {
                    message: e.to_string(),
                    code: "UNDO_FAILED".to_string(),
                });
                return false;
            }
            self.emit_content_changed();
            self.emit_selection_changed();
            true
        } else {
            false
        }
    }

    /// Performs redo.
    ///
    /// Returns true if an action was redone.
    pub fn redo(&mut self) -> bool {
        if let Some(cmd) = self.state.history.redo() {
            if let Err(e) = cmd.apply(&mut self.state.document, &mut self.state.cursor) {
                self.emit(&EditorEvent::Error {
                    message: e.to_string(),
                    code: "REDO_FAILED".to_string(),
                });
                return false;
            }
            self.emit_content_changed();
            self.emit_selection_changed();
            true
        } else {
            false
        }
    }

    /// Handles a paste operation with the given text.
    pub fn paste(&mut self, text: &str) {
        if self.state.read_only || text.is_empty() {
            return;
        }

        // Delete selection if any, then insert
        if !self.state.cursor.primary.is_collapsed() {
            let range = self.state.cursor.primary.range();
            let deleted = self.state.document.slice(range);
            let cmd = Command::Delete {
                range,
                deleted_text: deleted,
            };
            self.apply_command(cmd);
        }

        let pos = self.state.cursor.primary.head;
        let cmd = Command::Insert {
            position: pos,
            text: text.to_string(),
        };
        self.apply_command(cmd);

        // Move cursor after inserted text
        let new_pos = compute_position_after_insert(pos, text);
        self.state.cursor = CursorState::at(new_pos);
        self.emit_selection_changed();
    }

    /// Adds an event listener.
    pub fn add_listener<F>(&mut self, listener: F)
    where
        F: Fn(&EditorEvent) + Send + Sync + 'static,
    {
        self.listeners.push(Box::new(listener));
    }

    /// Emits an event to all listeners.
    fn emit(&self, event: &EditorEvent) {
        for listener in &self.listeners {
            listener(event);
        }
    }

    /// Emits a content changed event (T060).
    fn emit_content_changed(&self) {
        self.emit(&EditorEvent::ContentChanged {
            content: self.state.content(),
        });
    }

    /// Emits a selection changed event (T061).
    fn emit_selection_changed(&self) {
        let selections: Vec<Selection> = self.state.cursor.all_selections().copied().collect();
        self.emit(&EditorEvent::SelectionChanged { selections });
    }

    /// Returns the current theme (T152).
    #[must_use]
    pub const fn get_theme(&self) -> &Theme {
        &self.state.theme
    }

    /// Sets the theme at runtime (T147, T152).
    ///
    /// This updates the editor's theme and emits a `ThemeChanged` event.
    /// The change takes effect immediately without requiring a restart.
    ///
    /// # Arguments
    ///
    /// * `theme` - The new theme to apply
    ///
    /// # Example
    ///
    /// ```
    /// use iridium_editor::{Editor, EditorConfig};
    /// use iridium_editor::theme::Theme;
    ///
    /// let mut editor = Editor::with_defaults();
    ///
    /// // Switch to light theme
    /// editor.set_theme(Theme::light());
    ///
    /// // Or load a custom theme from JSON
    /// // let custom = Theme::from_json(json_str).unwrap();
    /// // editor.set_theme(custom);
    /// ```
    pub fn set_theme(&mut self, theme: Theme) {
        let theme_name = theme.name.clone();
        let is_dark = theme.is_dark;

        self.state.theme = theme;

        self.emit(&EditorEvent::ThemeChanged {
            theme_name,
            is_dark,
        });
    }

    /// Returns true if the current theme is dark.
    #[must_use]
    pub const fn is_dark_theme(&self) -> bool {
        self.state.theme.is_dark
    }

    /// Switches to the default dark theme.
    pub fn use_dark_theme(&mut self) {
        self.set_theme(Theme::dark());
    }

    /// Switches to the default light theme.
    pub fn use_light_theme(&mut self) {
        self.set_theme(Theme::light());
    }

    /// Emits a theme changed event (T147).
    #[allow(dead_code)]
    fn emit_theme_changed(&self) {
        self.emit(&EditorEvent::ThemeChanged {
            theme_name: self.state.theme.name.clone(),
            is_dark: self.state.theme.is_dark,
        });
    }

    // ==================== Search Methods (T121-T126) ====================

    /// Starts a search with the given query and options.
    ///
    /// This finds all matches in the document and updates the search state.
    /// The current match is set to the first match found (if any).
    ///
    /// # Arguments
    ///
    /// * `query` - The search query (plain text or regex pattern)
    /// * `options` - Search options controlling matching behavior
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or `Err(String)` if the regex is invalid.
    pub fn find(&mut self, query: &str, options: &SearchOptions) -> Result<(), String> {
        self.state
            .search
            .find_all(query, options, &self.state.document)?;

        // Move to nearest match from current cursor position
        if self.state.search.has_matches() {
            self.state
                .search
                .goto_nearest_match(self.state.cursor.primary.head);

            // Move cursor to current match
            if let Some(range) = self.state.search.current_range() {
                self.state.cursor = CursorState::at(range.start);
                self.emit_selection_changed();
            }
        }

        self.emit_search_updated();
        Ok(())
    }

    /// Updates the search query incrementally.
    ///
    /// This is useful for live search where the query is updated as the user types.
    ///
    /// # Arguments
    ///
    /// * `query` - The new search query
    ///
    /// # Returns
    ///
    /// Returns `Ok(true)` if the matches changed, `Ok(false)` otherwise,
    /// or `Err(String)` if the regex is invalid.
    pub fn update_search(&mut self, query: &str) -> Result<bool, String> {
        let changed = self
            .state
            .search
            .update_query(query, &self.state.document)?;
        if changed {
            self.emit_search_updated();
        }
        Ok(changed)
    }

    /// Sets search options and re-runs the search.
    pub fn set_search_options(&mut self, options: SearchOptions) -> Result<(), String> {
        self.state
            .search
            .set_options(options, &self.state.document)?;
        self.emit_search_updated();
        Ok(())
    }

    /// Goes to the next search match.
    ///
    /// Wraps around to the first match when at the end.
    pub fn goto_next_match(&mut self) {
        self.state.search.next_match();

        if let Some(range) = self.state.search.current_range() {
            self.state.cursor = CursorState::at(range.start);
            self.emit_selection_changed();
        }

        self.emit_search_updated();
    }

    /// Goes to the previous search match.
    ///
    /// Wraps around to the last match when at the beginning.
    pub fn goto_previous_match(&mut self) {
        self.state.search.previous_match();

        if let Some(range) = self.state.search.current_range() {
            self.state.cursor = CursorState::at(range.start);
            self.emit_selection_changed();
        }

        self.emit_search_updated();
    }

    /// Replaces the current match with the replacement text.
    ///
    /// After replacement, moves to the next match.
    ///
    /// # Arguments
    ///
    /// * `replacement` - The text to replace the current match with
    ///
    /// # Returns
    ///
    /// Returns `true` if a replacement was made, `false` otherwise.
    pub fn replace_current_match(&mut self, replacement: &str) -> bool {
        if self.state.read_only {
            return false;
        }

        let cmd = replace_current(
            &self.state.search,
            replacement,
            &self.state.document,
            &self.state.cursor,
        );

        if let Some(cmd) = cmd {
            self.apply_command(cmd);

            // Re-run search to update match positions
            let query = self.state.search.query.clone();
            let options = self.state.search.options.clone();
            let _ = self
                .state
                .search
                .find_all(&query, &options, &self.state.document);

            // Move to next match
            self.goto_next_match();
            true
        } else {
            false
        }
    }

    /// Replaces all matches with the replacement text.
    ///
    /// This is a single undoable operation per FR-030.
    ///
    /// # Arguments
    ///
    /// * `replacement` - The text to replace each match with
    ///
    /// # Returns
    ///
    /// Returns the number of replacements made.
    pub fn replace_all_matches(&mut self, replacement: &str) -> usize {
        if self.state.read_only {
            return 0;
        }

        let result = replace_all(
            &self.state.search,
            replacement,
            &self.state.document,
            &self.state.cursor,
        );

        if let Some((cmd, replace_result)) = result {
            self.apply_command(cmd);

            // Clear search since all matches are replaced
            self.close_search();

            replace_result.count
        } else {
            0
        }
    }

    /// Closes/clears the current search.
    pub fn close_search(&mut self) {
        self.state.search.clear();
        self.emit_search_updated();
    }

    /// Returns true if a search is currently active.
    #[must_use]
    pub const fn is_searching(&self) -> bool {
        self.state.search.is_active
    }

    /// Returns the current search state.
    #[must_use]
    pub const fn search_state(&self) -> &SearchState {
        &self.state.search
    }

    /// Returns the number of search matches.
    #[must_use]
    pub fn search_match_count(&self) -> usize {
        self.state.search.match_count()
    }

    /// Returns the current match index (0-based), if any.
    #[must_use]
    pub const fn current_match_index(&self) -> Option<usize> {
        self.state.search.current_match
    }

    /// Emits a search updated event (T125).
    fn emit_search_updated(&self) {
        self.emit(&EditorEvent::SearchUpdated {
            match_count: self.state.search.match_count(),
            current_index: self.state.search.current_match,
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_state_new() {
        let state = EditorState::new("Hello, world!");
        assert_eq!(state.content(), "Hello, world!");
        assert_eq!(state.cursor.cursor_count(), 1);
    }

    #[test]
    fn editor_set_content() {
        let mut editor = Editor::with_defaults();
        editor.set_content("Hello");
        assert_eq!(editor.content(), "Hello");
    }

    #[test]
    fn editor_cursor() {
        let mut editor = Editor::with_defaults();
        editor.set_content("Hello\nWorld");
        editor.set_cursor(Position::new(1, 3));
        assert_eq!(editor.cursor(), Position::new(1, 3));
    }

    #[test]
    fn editor_get_theme() {
        let editor = Editor::with_defaults();
        let theme = editor.get_theme();
        assert_eq!(theme.name, "Iridium Dark");
    }

    #[test]
    fn editor_set_theme() {
        let mut editor = Editor::with_defaults();
        assert!(editor.is_dark_theme());

        editor.set_theme(Theme::light());
        assert!(!editor.is_dark_theme());
        assert_eq!(editor.get_theme().name, "Iridium Light");
    }

    #[test]
    fn editor_use_dark_light_theme() {
        let mut editor = Editor::with_defaults();

        editor.use_light_theme();
        assert!(!editor.is_dark_theme());

        editor.use_dark_theme();
        assert!(editor.is_dark_theme());
    }

    #[test]
    fn editor_theme_changed_event() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let mut editor = Editor::with_defaults();
        let event_received = Arc::new(AtomicBool::new(false));
        let event_received_clone = Arc::clone(&event_received);

        editor.add_listener(move |event| {
            if matches!(event, EditorEvent::ThemeChanged { .. }) {
                event_received_clone.store(true, Ordering::SeqCst);
            }
        });

        editor.set_theme(Theme::light());
        assert!(event_received.load(Ordering::SeqCst));
    }
}
