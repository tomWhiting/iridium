//! Core editor state and main editor instance.

use serde::{Deserialize, Serialize};

use super::config::EditorConfig;
use super::fold_state::{FoldInfo, FoldState};
use crate::document::{CursorState, Document, Position, Selection};
use crate::history::{Command, UndoTree};
use crate::input::keyboard::editing;
use crate::input::{
    ClipboardOperation, ImeEvent, ImeHandler, ImeResult, ImeState, KeyEvent, KeyResult,
    KeyboardHandler, MouseEvent, MouseHandler, MouseResult, SearchAction,
};
use crate::render::Viewport;
use crate::search::{SearchOptions, SearchState, replace_all, replace_current};
use crate::theme::Theme;

#[cfg(not(feature = "syntax"))]
use crate::syntax_stubs::{FoldKind, Language};
#[cfg(feature = "syntax")]
use iridium_syntax::{FoldKind, Language};

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

    /// Fold state changed (T130).
    ///
    /// Emitted when a fold is toggled, or regions are updated.
    FoldChanged {
        /// Number of currently folded regions
        folded_count: usize,
        /// Total number of foldable regions
        total_regions: usize,
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

    /// Code folding state (T130)
    pub fold_state: FoldState,

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
            fold_state: FoldState::new(),
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
        // Update fold regions for the new content
        self.fold_state.update_regions(content);
    }

    /// Sets the language for syntax-aware folding.
    pub fn set_language(&mut self, language: Language) {
        self.fold_state.set_language(language);
        self.fold_state.update_regions(&self.document.text());
    }

    /// Returns the current language, if any.
    #[must_use]
    pub const fn language(&self) -> Option<Language> {
        self.fold_state.language()
    }

    /// Re-synchronizes the active search with the current document content.
    ///
    /// Recorded match ranges point into the document as it was when the
    /// search last ran; any content mutation can silently invalidate them
    /// (and literal-mode replace trusts them verbatim), so this must be
    /// called after every content-modifying command, including undo/redo.
    ///
    /// When a search with a non-empty query is active, the search is re-run
    /// against the mutated document with the current query and options, and
    /// the current match is preserved by nearest position (the match at or
    /// after the previous current match's start). With an empty query this
    /// is a no-op. Re-running the full search on every edit is the honest
    /// baseline; incremental match maintenance is a planned optimization
    /// (PLAN.md Phase 2 performance work).
    ///
    /// Returns `true` when an active search was re-run (matches and current
    /// index may have changed), `false` when there was nothing to refresh.
    pub fn refresh_search(&mut self) -> bool {
        if self.search.query.is_empty() {
            return false;
        }

        let previous_position = self.search.current_range().map(|range| range.start);
        let query = self.search.query.clone();
        let options = self.search.options.clone();

        // The query and options were validated when the search was created
        // and are unchanged here, so this can only fail if the public search
        // fields were mutated into an invalid state. `find_all` clears the
        // recorded matches before validating, which is exactly the safe
        // outcome for that case: no stale range survives.
        let _ = self.search.find_all(&query, &options, &self.document);

        if let Some(position) = previous_position {
            self.search.goto_nearest_match(position);
        }

        true
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
    ///
    /// The cursor is reset (so the keyboard handler's sticky vertical column
    /// is dropped) and any active search is re-run against the new content.
    pub fn set_content(&mut self, content: &str) {
        self.state.set_content(content);
        self.keyboard_handler.reset_vertical_state();
        if self.state.refresh_search() {
            self.emit_search_updated();
        }
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
    ///
    /// The position is clamped to the document. This bypasses the keyboard
    /// handler, so its sticky vertical column is reset.
    pub fn set_cursor(&mut self, position: Position) {
        let clamped = self.state.document.clamp_position(position);
        self.keyboard_handler.reset_vertical_state();
        self.state.cursor = CursorState::at(clamped);
        self.emit_selection_changed();
    }

    /// Sets the selection, replacing all cursors with a single selection.
    ///
    /// Both endpoints are clamped to the document. This bypasses the
    /// keyboard handler, so its sticky vertical column is reset.
    pub fn set_selection(&mut self, anchor: Position, head: Position) {
        let anchor = self.state.document.clamp_position(anchor);
        let head = self.state.document.clamp_position(head);
        self.keyboard_handler.reset_vertical_state();
        self.state.cursor = CursorState::new(Selection::new(anchor, head));
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
                // Mouse-driven cursor changes bypass the keyboard handler;
                // drop its sticky vertical column so the next Up/Down starts
                // from the clicked position.
                self.keyboard_handler.reset_vertical_state();
                self.apply_command(cmd);
            },
            MouseResult::Scroll { delta_x, delta_y } => {
                // Handle scrolling
                self.scroll_by(delta_x, delta_y);
            },
            MouseResult::ToggleFold { line } => {
                // T136: Handle fold indicator click
                self.toggle_fold_at(line);
            },
            MouseResult::ScrollToLine { target_line } => {
                // Handle minimap click-to-navigate (T142)
                self.scroll_to_line(target_line);
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
                // IME commits move the cursor without going through the
                // keyboard handler; drop its sticky vertical column.
                self.keyboard_handler.reset_vertical_state();
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
    pub const fn is_ime_composing(&self) -> bool {
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

        // Convert pixel delta to lines using configured line height
        let line_height_px = self.state.config.font_size * self.state.config.line_height;
        let line_delta = (delta_y / line_height_px) as i32;
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

    /// Scrolls to show a specific line (T142: minimap click-to-navigate).
    ///
    /// The viewport will be positioned to show the target line.
    /// Used by minimap click and drag navigation.
    pub fn scroll_to_line(&mut self, line: usize) {
        // Clamp to document bounds
        let max_line = self.state.document.line_count().saturating_sub(1);
        self.state.scroll_line = line.min(max_line);

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

            // Recorded search match ranges point into the pre-edit document;
            // re-synchronize the active search so replace operations never
            // act on stale ranges.
            if self.state.refresh_search() {
                self.emit_search_updated();
            }

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
    ///
    /// Commands recorded with a trailing `SetSelection` (all keyboard edits,
    /// paste, replace) restore the exact prior cursor state themselves. For
    /// history entries that carry no selection information (bare
    /// `Insert`/`Delete`/`Replace` from host-driven [`Self::apply_command`]
    /// calls), the cursor is explicitly placed at the edit site — see
    /// [`replayed_command_caret`] — instead of being left wherever it was.
    pub fn undo(&mut self) -> bool {
        if let Some(cmd) = self.state.history.undo() {
            // Note: history.undo() already returns the inverse command,
            // so we apply it directly without calling .inverse() again
            if let Err(e) = cmd.apply(&mut self.state.document, &mut self.state.cursor) {
                self.emit(&EditorEvent::Error {
                    message: e.to_string(),
                    code: "UNDO_FAILED".to_string(),
                });
                return false;
            }
            self.finish_history_replay(&cmd);
            true
        } else {
            false
        }
    }

    /// Performs redo.
    ///
    /// Returns true if an action was redone.
    ///
    /// Cursor placement follows the same rules as [`Self::undo`]: commands
    /// carrying a `SetSelection` restore the cursor state themselves, and
    /// bare content commands place the cursor at the edit site.
    pub fn redo(&mut self) -> bool {
        if let Some(cmd) = self.state.history.redo() {
            if let Err(e) = cmd.apply(&mut self.state.document, &mut self.state.cursor) {
                self.emit(&EditorEvent::Error {
                    message: e.to_string(),
                    code: "REDO_FAILED".to_string(),
                });
                return false;
            }
            self.finish_history_replay(&cmd);
            true
        } else {
            false
        }
    }

    /// Shared post-processing for a successfully applied undo/redo command.
    ///
    /// - Places the cursor at the edit site when the replayed command carries
    ///   no `SetSelection` of its own (documented behavior of [`Self::undo`]
    ///   and [`Self::redo`]).
    /// - Resets the keyboard handler's sticky vertical column (the cursor
    ///   moved without going through `handle_key`).
    /// - Re-synchronizes the active search with the mutated document.
    /// - Emits content, search, and selection events.
    fn finish_history_replay(&mut self, cmd: &Command) {
        if !command_restores_selection(cmd) {
            if let Some(position) = replayed_command_caret(cmd) {
                let clamped = self.state.document.clamp_position(position);
                self.state.cursor = CursorState::at(clamped);
            }
        }

        self.keyboard_handler.reset_vertical_state();

        if self.state.refresh_search() {
            self.emit_search_updated();
        }

        self.emit_content_changed();
        self.emit_selection_changed();
    }

    /// Handles a paste operation with the given text.
    ///
    /// The text is inserted at every cursor (each cursor's selection is
    /// replaced) through the same multi-cursor command builder as keyboard
    /// input, so the whole paste is one reversible command whose trailing
    /// `SetSelection` restores both text and the exact prior cursor state on
    /// undo.
    pub fn paste(&mut self, text: &str) {
        if self.state.read_only || text.is_empty() {
            return;
        }

        // Paste moves the cursor without going through handle_key.
        self.keyboard_handler.reset_vertical_state();

        let edits = editing::replace_all_edits(&self.state.cursor, text);
        if let Some(command) =
            editing::build_multi_cursor_command(&self.state.document, &self.state.cursor, edits)
        {
            self.apply_command(command);
        }
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

    // =========================================================================
    // Code Folding (T130-T133)
    // =========================================================================

    /// Sets the language for syntax-aware folding.
    ///
    /// This initializes the fold detector for the given language and
    /// detects fold regions in the current content.
    pub fn set_language(&mut self, language: Language) {
        self.state.set_language(language);
        self.emit_fold_changed();
    }

    /// Returns the current language, if any.
    #[must_use]
    pub const fn language(&self) -> Option<Language> {
        self.state.language()
    }

    /// Returns a reference to the fold state.
    #[must_use]
    pub const fn fold_state(&self) -> &FoldState {
        &self.state.fold_state
    }

    /// Returns a mutable reference to the fold state.
    pub const fn fold_state_mut(&mut self) -> &mut FoldState {
        &mut self.state.fold_state
    }

    /// Folds the region at the given line (T131).
    ///
    /// Returns true if the fold was successful, false if the line
    /// is not foldable or already folded.
    pub fn fold_at(&mut self, line: usize) -> bool {
        let result = self.state.fold_state.fold_at(line);
        if result {
            self.emit_fold_changed();
        }
        result
    }

    /// Unfolds the region at the given line (T132).
    ///
    /// Returns true if the unfold was successful, false if the line
    /// is not currently folded.
    pub fn unfold_at(&mut self, line: usize) -> bool {
        let result = self.state.fold_state.unfold_at(line);
        if result {
            self.emit_fold_changed();
        }
        result
    }

    /// Toggles the fold state of the region at the given line.
    ///
    /// Returns true if the toggle was successful, false if the line
    /// is not foldable.
    pub fn toggle_fold_at(&mut self, line: usize) -> bool {
        let result = self.state.fold_state.toggle_fold_at(line);
        if result {
            self.emit_fold_changed();
        }
        result
    }

    /// Folds all foldable regions (T133).
    pub fn fold_all(&mut self) {
        self.state.fold_state.fold_all();
        self.emit_fold_changed();
    }

    /// Unfolds all folded regions (T133).
    pub fn unfold_all(&mut self) {
        self.state.fold_state.unfold_all();
        self.emit_fold_changed();
    }

    /// Folds all regions of a specific kind.
    pub fn fold_all_of_kind(&mut self, kind: FoldKind) {
        self.state.fold_state.fold_all_of_kind(kind);
        self.emit_fold_changed();
    }

    /// Unfolds all regions of a specific kind.
    pub fn unfold_all_of_kind(&mut self, kind: FoldKind) {
        self.state.fold_state.unfold_all_of_kind(kind);
        self.emit_fold_changed();
    }

    /// Returns true if the given line is foldable.
    #[must_use]
    pub fn is_foldable(&self, line: usize) -> bool {
        self.state.fold_state.is_foldable(line)
    }

    /// Returns true if the given line is currently folded.
    #[must_use]
    pub fn is_folded(&self, line: usize) -> bool {
        self.state.fold_state.is_folded(line)
    }

    /// Returns true if the given line is hidden by a fold.
    #[must_use]
    pub fn is_line_hidden(&self, line: usize) -> bool {
        self.state.fold_state.is_line_hidden(line)
    }

    /// Returns the number of currently folded regions.
    #[must_use]
    pub fn folded_count(&self) -> usize {
        self.state.fold_state.folded_count()
    }

    /// Returns the total number of foldable regions.
    #[must_use]
    pub fn fold_region_count(&self) -> usize {
        self.state.fold_state.regions().len()
    }

    /// Maps a visual line number to a document line number.
    ///
    /// Visual lines skip over hidden (folded) lines.
    #[must_use]
    pub fn visual_to_document_line(&self, visual_line: usize) -> usize {
        self.state.fold_state.visual_to_document_line(visual_line)
    }

    /// Maps a document line number to a visual line number.
    ///
    /// Returns None if the document line is hidden by a fold.
    #[must_use]
    pub fn document_to_visual_line(&self, doc_line: usize) -> Option<usize> {
        self.state.fold_state.document_to_visual_line(doc_line)
    }

    /// Returns the visible line count (total lines minus hidden lines).
    #[must_use]
    pub fn visible_line_count(&self) -> usize {
        let total = self.state.document.line_count();
        self.state.fold_state.visible_line_count(total)
    }

    /// Exports fold information for persistence.
    #[must_use]
    pub fn export_fold_info(&self) -> FoldInfo {
        self.state.fold_state.export_fold_info()
    }

    /// Imports fold information (e.g., from a saved session).
    pub fn import_fold_info(&mut self, info: &FoldInfo) {
        self.state.fold_state.import_fold_info(info);
        self.emit_fold_changed();
    }

    /// Emits a fold changed event.
    fn emit_fold_changed(&self) {
        self.emit(&EditorEvent::FoldChanged {
            folded_count: self.state.fold_state.folded_count(),
            total_regions: self.state.fold_state.regions().len(),
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
                self.keyboard_handler.reset_vertical_state();
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
            self.keyboard_handler.reset_vertical_state();
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
            self.keyboard_handler.reset_vertical_state();
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
            // The replace command carries a SetSelection that moves the
            // cursor without going through handle_key.
            self.keyboard_handler.reset_vertical_state();
            self.apply_command(cmd);

            // Re-run search from the top so navigation restarts at the first
            // match (apply_command already refreshed the stale ranges; this
            // additionally resets the current-match index for the
            // goto_next_match below, preserving long-standing behavior).
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
            // The replace command carries a SetSelection that moves the
            // cursor without going through handle_key.
            self.keyboard_handler.reset_vertical_state();
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

/// Returns true if the command (or any nested command) sets the cursor state
/// itself via `SetSelection`.
///
/// Such commands need no fallback caret placement after undo/redo: the
/// recorded selection states restore the cursor exactly.
fn command_restores_selection(command: &Command) -> bool {
    match command {
        Command::SetSelection { .. } => true,
        Command::Compound { commands } => commands.iter().any(command_restores_selection),
        Command::Insert { .. } | Command::Delete { .. } | Command::Replace { .. } => false,
    }
}

/// Fallback caret position for a replayed (undo/redo) command that carries no
/// `SetSelection` of its own.
///
/// The caret is placed at the edit site of the applied command:
/// - `Insert` (the undo of a delete, or a redone insert): the end of the
///   inserted text.
/// - `Delete` (the undo of an insert, or a redone delete): the start of the
///   removed range.
/// - `Replace`: the end of the newly inserted text.
/// - `Compound`: the first content command's site.
///
/// Returns `None` when the command contains no content edit at all.
fn replayed_command_caret(command: &Command) -> Option<Position> {
    match command {
        Command::Insert { position, text } => Some(position.advanced_through(text)),
        Command::Delete { range, .. } => Some(range.start),
        Command::Replace {
            range, new_text, ..
        } => Some(range.start.advanced_through(new_text)),
        Command::SetSelection { .. } => None,
        Command::Compound { commands } => commands.iter().find_map(replayed_command_caret),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::document::Range;
    use crate::input::{KeyCode, Modifiers};

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

    // =========================================================================
    // Fold Tests (T130-T133)
    // =========================================================================

    #[test]
    fn editor_fold_at() {
        let mut editor = Editor::with_defaults();
        let code = "fn main() {\n    println!(\"hello\");\n}";
        editor.set_content(code);
        editor.set_language(Language::Rust);

        // Should have detected the function
        assert!(editor.fold_region_count() > 0);
        assert!(editor.is_foldable(0));
        assert!(!editor.is_folded(0));

        // Fold it
        assert!(editor.fold_at(0));
        assert!(editor.is_folded(0));
        assert!(editor.is_line_hidden(1));
    }

    #[test]
    fn editor_unfold_at() {
        let mut editor = Editor::with_defaults();
        let code = "fn main() {\n    println!(\"hello\");\n}";
        editor.set_content(code);
        editor.set_language(Language::Rust);

        editor.fold_at(0);
        assert!(editor.is_folded(0));

        // Unfold it
        assert!(editor.unfold_at(0));
        assert!(!editor.is_folded(0));
        assert!(!editor.is_line_hidden(1));
    }

    #[test]
    fn editor_toggle_fold_at() {
        let mut editor = Editor::with_defaults();
        let code = "fn main() {\n    println!(\"hello\");\n}";
        editor.set_content(code);
        editor.set_language(Language::Rust);

        assert!(!editor.is_folded(0));
        assert!(editor.toggle_fold_at(0));
        assert!(editor.is_folded(0));
        assert!(editor.toggle_fold_at(0));
        assert!(!editor.is_folded(0));
    }

    #[test]
    fn editor_fold_all_unfold_all() {
        let mut editor = Editor::with_defaults();
        let code = r#"fn foo() {
    println!("foo");
}

fn bar() {
    println!("bar");
}"#;
        editor.set_content(code);
        editor.set_language(Language::Rust);

        editor.fold_all();
        assert!(editor.is_folded(0));
        assert!(editor.is_folded(4));

        editor.unfold_all();
        assert!(!editor.is_folded(0));
        assert!(!editor.is_folded(4));
    }

    #[test]
    fn editor_visible_line_count() {
        let mut editor = Editor::with_defaults();
        let code = r"fn main() {
    line 1
    line 2
    line 3
}";
        editor.set_content(code);
        editor.set_language(Language::Rust);

        let total_lines = 5;
        assert_eq!(editor.visible_line_count(), total_lines);

        editor.fold_at(0);
        // After folding, 4 lines are hidden (lines 1-4)
        assert_eq!(editor.visible_line_count(), 1);
    }

    #[test]
    fn editor_visual_line_mapping() {
        let mut editor = Editor::with_defaults();
        let code = r"line 0
fn foo() {
    hidden 1
    hidden 2
}
line 5";
        editor.set_content(code);
        editor.set_language(Language::Rust);

        // Fold the function
        editor.fold_at(1);

        // Visual line mapping
        assert_eq!(editor.document_to_visual_line(0), Some(0));
        assert_eq!(editor.document_to_visual_line(1), Some(1));
        assert_eq!(editor.document_to_visual_line(2), None); // Hidden
        assert_eq!(editor.document_to_visual_line(3), None); // Hidden
        assert_eq!(editor.document_to_visual_line(4), None); // Hidden (closing brace)
        assert_eq!(editor.document_to_visual_line(5), Some(2));
    }

    #[test]
    fn editor_fold_changed_event() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let mut editor = Editor::with_defaults();
        let event_received = Arc::new(AtomicBool::new(false));
        let event_received_clone = Arc::clone(&event_received);

        editor.add_listener(move |event| {
            if matches!(event, EditorEvent::FoldChanged { .. }) {
                event_received_clone.store(true, Ordering::SeqCst);
            }
        });

        let code = "fn main() {\n    println!(\"hello\");\n}";
        editor.set_content(code);
        editor.set_language(Language::Rust);

        // Language setting should emit fold changed
        assert!(event_received.load(Ordering::SeqCst));
    }

    // =========================================================================
    // Paste (multi-cursor command path)
    // =========================================================================

    /// Returns the head positions of all cursors in `all_selections` order
    /// (primary first).
    fn cursor_heads(editor: &Editor) -> Vec<Position> {
        editor
            .state()
            .cursor
            .all_selections()
            .map(|sel| sel.head)
            .collect()
    }

    #[test]
    fn paste_multi_cursor_inserts_at_every_cursor() {
        let mut editor = Editor::with_defaults();
        editor.set_content("abcd");
        editor.set_cursor(Position::new(0, 0));
        editor
            .state_mut()
            .cursor
            .add_cursor(Selection::collapsed(Position::new(0, 2)));

        editor.paste("X");

        assert_eq!(editor.content(), "XabXcd");
        assert_eq!(
            cursor_heads(&editor),
            vec![Position::new(0, 1), Position::new(0, 4)]
        );
        assert!(
            editor
                .state()
                .cursor
                .all_selections()
                .all(Selection::is_collapsed)
        );
    }

    #[test]
    fn undo_of_paste_restores_text_and_all_cursors() {
        let mut editor = Editor::with_defaults();
        editor.set_content("abcd");
        editor.set_cursor(Position::new(0, 0));
        editor
            .state_mut()
            .cursor
            .add_cursor(Selection::collapsed(Position::new(0, 2)));

        editor.paste("X");
        assert_eq!(editor.content(), "XabXcd");

        // A single undo must restore both the text and the exact prior
        // multi-cursor state.
        assert!(editor.undo());
        assert_eq!(editor.content(), "abcd");
        assert_eq!(
            cursor_heads(&editor),
            vec![Position::new(0, 0), Position::new(0, 2)]
        );
    }

    #[test]
    fn paste_replaces_forward_selection_and_undoes() {
        let mut editor = Editor::with_defaults();
        editor.set_content("abcd");
        // Forward selection: anchor before head. The old single-cursor paste
        // inserted at the stale pre-delete head after deleting the selection.
        editor.set_selection(Position::new(0, 1), Position::new(0, 3));

        editor.paste("Z");
        assert_eq!(editor.content(), "aZd");
        assert_eq!(editor.cursor(), Position::new(0, 2));

        assert!(editor.undo());
        assert_eq!(editor.content(), "abcd");
        assert_eq!(editor.state().cursor.primary.anchor, Position::new(0, 1));
        assert_eq!(editor.state().cursor.primary.head, Position::new(0, 3));
    }

    #[test]
    fn paste_is_noop_when_read_only_or_empty() {
        let mut editor = Editor::with_defaults();
        editor.set_content("abcd");
        editor.set_cursor(Position::new(0, 2));

        editor.paste("");
        assert_eq!(editor.content(), "abcd");

        editor.state_mut().read_only = true;
        editor.paste("X");
        assert_eq!(editor.content(), "abcd");
        assert_eq!(editor.cursor(), Position::new(0, 2));
    }

    /// Regression test (Norn review, 2026-07-12): delete-forward leaves every
    /// caret in place, so the generated compound used to carry no
    /// `SetSelection` — undo's bare-command fallback then collapsed the
    /// multi-cursor state to a single caret. The builder now always records
    /// the cursor transition when content changes, so undo restores every
    /// cursor exactly.
    #[test]
    fn undo_of_multi_cursor_delete_forward_restores_all_cursors() {
        let mut editor = Editor::with_defaults();
        editor.set_content("abc\ndef");
        editor.state_mut().cursor = CursorState {
            primary: Selection::collapsed(Position::new(0, 1)),
            secondary: vec![Selection::collapsed(Position::new(1, 1))],
        };

        let event = KeyEvent::new(KeyCode::Delete, Modifiers::none());
        editor.handle_key(&event);
        assert_eq!(editor.content(), "ac\ndf");
        assert_eq!(editor.state().cursor.cursor_count(), 2);

        assert!(editor.undo());
        assert_eq!(editor.content(), "abc\ndef");
        assert_eq!(
            editor.state().cursor.cursor_count(),
            2,
            "undo must restore the multi-cursor state, not collapse it"
        );
        assert_eq!(editor.state().cursor.primary.head, Position::new(0, 1));
        assert_eq!(editor.state().cursor.secondary[0].head, Position::new(1, 1));

        assert!(editor.redo());
        assert_eq!(editor.content(), "ac\ndf");
        assert_eq!(editor.state().cursor.cursor_count(), 2);
    }

    // =========================================================================
    // Undo/redo cursor placement for bare (selection-less) commands
    // =========================================================================

    #[test]
    fn undo_of_bare_insert_places_cursor_at_insert_position() {
        let mut editor = Editor::with_defaults();
        editor.set_content("hello world");
        editor.apply_command(Command::Insert {
            position: Position::new(0, 5),
            text: "X".to_string(),
        });
        assert_eq!(editor.content(), "helloX world");

        // Move the caret away to prove undo repositions it explicitly.
        editor.set_cursor(Position::new(0, 0));

        assert!(editor.undo());
        assert_eq!(editor.content(), "hello world");
        // Undoing an insert removes the text; the caret lands at the start
        // of the removed range.
        assert_eq!(editor.cursor(), Position::new(0, 5));

        assert!(editor.redo());
        assert_eq!(editor.content(), "helloX world");
        // Redoing the insert places the caret at the end of the inserted text.
        assert_eq!(editor.cursor(), Position::new(0, 6));
    }

    #[test]
    fn undo_of_bare_delete_places_cursor_at_end_of_restored_text() {
        let mut editor = Editor::with_defaults();
        editor.set_content("hello world");
        editor.apply_command(Command::Delete {
            range: Range::new(Position::new(0, 0), Position::new(0, 6)),
            deleted_text: "hello ".to_string(),
        });
        assert_eq!(editor.content(), "world");
        editor.set_cursor(Position::new(0, 3));

        assert!(editor.undo());
        assert_eq!(editor.content(), "hello world");
        // Undoing a delete re-inserts the text; the caret lands at its end.
        assert_eq!(editor.cursor(), Position::new(0, 6));
    }

    // =========================================================================
    // Search invalidation on document mutation
    // =========================================================================

    #[test]
    fn typing_with_active_search_refreshes_match_ranges() {
        let mut editor = Editor::with_defaults();
        editor.set_content("foo bar foo");
        editor.find("foo", &SearchOptions::default()).unwrap();
        assert_eq!(editor.search_match_count(), 2);

        editor.set_cursor(Position::new(0, 0));
        editor.handle_key(&KeyEvent::simple(KeyCode::Char('x')));
        assert_eq!(editor.content(), "xfoo bar foo");

        // Matches must reflect the mutated document, not the stale offsets.
        let starts: Vec<Position> = editor
            .search_state()
            .all_matches()
            .iter()
            .map(|range| range.start)
            .collect();
        assert_eq!(starts, vec![Position::new(0, 1), Position::new(0, 9)]);
    }

    #[test]
    fn replace_all_literal_after_edit_ignores_stale_ranges() {
        let mut editor = Editor::with_defaults();
        editor.set_content("foo bar foo");
        editor.find("foo", &SearchOptions::default()).unwrap();

        // Mutate the document while the search is active: every recorded
        // range shifts right by one column.
        editor.set_cursor(Position::new(0, 0));
        editor.handle_key(&KeyEvent::simple(KeyCode::Char('x')));
        assert_eq!(editor.content(), "xfoo bar foo");

        // Literal-mode replace trusts the recorded ranges verbatim; without
        // the refresh it would have replaced "xfo" and " fo".
        let replaced = editor.replace_all_matches("Z");
        assert_eq!(replaced, 2);
        assert_eq!(editor.content(), "xZ bar Z");
    }

    #[test]
    fn replace_all_regex_after_edit_replaces_refreshed_matches() {
        let mut editor = Editor::with_defaults();
        editor.set_content("foo bar foo");
        editor.find("f(o+)", &SearchOptions::regex_mode()).unwrap();

        editor.set_cursor(Position::new(0, 0));
        editor.handle_key(&KeyEvent::simple(KeyCode::Char('x')));
        assert_eq!(editor.content(), "xfoo bar foo");

        // Regex replace verifies matches; without the refresh both recorded
        // ranges would fail verification and nothing would be replaced.
        let replaced = editor.replace_all_matches("[$1]");
        assert_eq!(replaced, 2);
        assert_eq!(editor.content(), "x[oo] bar [oo]");
    }

    #[test]
    fn undo_with_active_search_refreshes_match_ranges() {
        let mut editor = Editor::with_defaults();
        editor.set_content("foo bar foo");
        editor.set_cursor(Position::new(0, 0));
        editor.handle_key(&KeyEvent::simple(KeyCode::Char('x')));
        assert_eq!(editor.content(), "xfoo bar foo");

        editor.find("foo", &SearchOptions::default()).unwrap();
        let starts: Vec<Position> = editor
            .search_state()
            .all_matches()
            .iter()
            .map(|range| range.start)
            .collect();
        assert_eq!(starts, vec![Position::new(0, 1), Position::new(0, 9)]);

        // Undo mutates the document outside apply_command; matches must
        // follow the restored text.
        assert!(editor.undo());
        assert_eq!(editor.content(), "foo bar foo");
        let starts: Vec<Position> = editor
            .search_state()
            .all_matches()
            .iter()
            .map(|range| range.start)
            .collect();
        assert_eq!(starts, vec![Position::new(0, 0), Position::new(0, 8)]);
    }

    // =========================================================================
    // Sticky-column invalidation for cursor changes outside handle_key
    // =========================================================================

    #[test]
    fn set_cursor_resets_sticky_column_for_vertical_moves() {
        let mut editor = Editor::with_defaults();
        // Lines of length 10 / 2 / 10 (Norn review scenario).
        editor.set_content("aaaaaaaaaa\nbb\ncccccccccc");

        editor.set_cursor(Position::new(0, 8));
        editor.handle_key(&KeyEvent::simple(KeyCode::Down));
        assert_eq!(editor.cursor(), Position::new(1, 2));

        // The host moves the cursor without going through handle_key; the
        // sticky column from the previous Down (8) must not survive, or the
        // next Down would jump to (2, 8).
        editor.set_cursor(Position::new(1, 1));
        editor.handle_key(&KeyEvent::simple(KeyCode::Down));
        assert_eq!(editor.cursor(), Position::new(2, 1));
    }

    #[test]
    fn undo_resets_sticky_column_for_vertical_moves() {
        let mut editor = Editor::with_defaults();
        editor.set_content("aaaaaaaaaa\nbb\ncccccccccc");

        // Bare insert on line 1 so undo's fallback caret placement moves the
        // cursor without a SetSelection.
        editor.apply_command(Command::Insert {
            position: Position::new(1, 1),
            text: "Q".to_string(),
        });

        editor.set_cursor(Position::new(0, 8));
        editor.handle_key(&KeyEvent::simple(KeyCode::Down));
        assert_eq!(editor.cursor(), Position::new(1, 3));

        // Undo moves the caret to (1, 1); the sticky column (8) must reset.
        assert!(editor.undo());
        assert_eq!(editor.cursor(), Position::new(1, 1));
        editor.handle_key(&KeyEvent::simple(KeyCode::Down));
        assert_eq!(editor.cursor(), Position::new(2, 1));
    }
}
