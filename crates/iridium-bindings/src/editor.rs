//! Editor bindings for TypeScript/WASM.
//!
//! This module provides comprehensive napi-rs bindings for the Iridium editor,
//! exposing all editor operations to TypeScript consumers.

use std::sync::{Arc, Mutex};

use napi::bindgen_prelude::*;
use napi_derive::napi;

use iridium_editor::{
    Editor, EditorConfig, Position, Range, Selection,
    search::SearchOptions,
};
use iridium_syntax::Language;

use crate::events::{EventCallback, EventEmitter};
use crate::types::{
    JsEditorConfig, JsFoldInfo, JsPosition, JsRange, JsSearchOptions,
    JsSearchResult, JsSelection, JsUndoInfo,
};

/// The Iridium editor instance exposed to TypeScript.
///
/// This is the main entry point for embedding the editor in a JavaScript/TypeScript
/// application. It wraps the core `Editor` struct and provides a complete API for:
///
/// - Content management (get/set content)
/// - Cursor and selection control
/// - Undo/redo operations
/// - Search and replace
/// - Code folding
/// - Theme management
/// - Event subscription
///
/// # Example (TypeScript)
///
/// ```typescript
/// import { IridiumEditor } from 'iridium-bindings';
///
/// const editor = new IridiumEditor();
/// editor.setContent('Hello, world!');
/// editor.setCursor(0, 5);
///
/// editor.on('contentChanged', (content) => {
///   console.log('Content changed:', content);
/// });
/// ```
#[napi]
pub struct IridiumEditor {
    /// The underlying editor instance, wrapped for thread-safety.
    inner: Arc<Mutex<Editor>>,
    /// Event emitter for broadcasting editor events.
    event_emitter: EventEmitter,
}

#[napi]
impl IridiumEditor {
    // =========================================================================
    // Constructor and Factory (T153, T154)
    // =========================================================================

    /// Creates a new editor instance with default configuration.
    #[napi(constructor)]
    pub fn new() -> Result<Self> {
        Self::with_config(None)
    }

    /// Creates an editor with the given configuration.
    #[napi(factory)]
    pub fn with_config(config: Option<JsEditorConfig>) -> Result<Self> {
        let editor_config = config.map(EditorConfig::from).unwrap_or_default();
        let editor = Editor::new(editor_config);

        Ok(Self {
            inner: Arc::new(Mutex::new(editor)),
            event_emitter: EventEmitter::new(),
        })
    }

    // =========================================================================
    // Content Operations (T156)
    // =========================================================================

    /// Gets the current content.
    #[napi]
    pub fn get_content(&self) -> String {
        self.with_editor(|editor| editor.content())
    }

    /// Sets the content, replacing everything.
    ///
    /// This resets the cursor position and clears the undo history.
    #[napi]
    pub fn set_content(&self, content: String) {
        self.with_editor_mut(|editor| {
            editor.set_content(&content);
        });
        self.emit_event("contentChanged", &content);
    }

    /// Gets the current line count.
    #[napi]
    pub fn get_line_count(&self) -> u32 {
        self.with_editor(|editor| editor.state().document.line_count() as u32)
    }

    /// Gets a specific line by number (0-indexed).
    #[napi]
    pub fn get_line(&self, line_number: u32) -> Option<String> {
        self.with_editor(|editor| editor.state().document.line(line_number as usize))
    }

    /// Gets the length of a specific line (in characters).
    #[napi]
    pub fn get_line_length(&self, line_number: u32) -> Option<u32> {
        self.with_editor(|editor| {
            editor
                .state()
                .document
                .line(line_number as usize)
                .map(|line| line.chars().count() as u32)
        })
    }

    /// Gets a range of text from the document.
    #[napi]
    pub fn get_text_in_range(&self, range: JsRange) -> String {
        self.with_editor(|editor| {
            let r: Range = range.into();
            editor.state().document.slice(r)
        })
    }

    // =========================================================================
    // Cursor and Selection Operations (T157)
    // =========================================================================

    /// Gets the current cursor position (primary cursor).
    #[napi]
    pub fn get_cursor(&self) -> JsPosition {
        self.with_editor(|editor| editor.cursor().into())
    }

    /// Gets the cursor line (0-indexed).
    #[napi]
    pub fn get_cursor_line(&self) -> u32 {
        self.with_editor(|editor| editor.cursor().line as u32)
    }

    /// Gets the cursor column (0-indexed).
    #[napi]
    pub fn get_cursor_column(&self) -> u32 {
        self.with_editor(|editor| editor.cursor().column as u32)
    }

    /// Sets the cursor position.
    #[napi]
    pub fn set_cursor(&self, line: u32, column: u32) {
        self.with_editor_mut(|editor| {
            editor.set_cursor(Position::new(line as usize, column as usize));
        });
        self.emit_selection_changed();
    }

    /// Sets the cursor using a position object.
    #[napi]
    pub fn set_cursor_position(&self, position: JsPosition) {
        self.set_cursor(position.line, position.column);
    }

    /// Gets the current selection (primary selection).
    #[napi]
    pub fn get_selection(&self) -> JsSelection {
        self.with_editor(|editor| editor.state().cursor.primary.into())
    }

    /// Gets all selections (for multi-cursor support).
    #[napi]
    pub fn get_all_selections(&self) -> Vec<JsSelection> {
        self.with_editor(|editor| {
            editor
                .state()
                .cursor
                .all_selections()
                .map(|s| (*s).into())
                .collect()
        })
    }

    /// Sets the selection.
    #[napi]
    pub fn set_selection(&self, anchor: JsPosition, head: JsPosition) {
        self.with_editor_mut(|editor| {
            let selection = Selection::new(anchor.into(), head.into());
            editor.state_mut().cursor = iridium_editor::CursorState::new(selection);
        });
        self.emit_selection_changed();
    }

    /// Sets the selection using a range.
    #[napi]
    pub fn set_selection_range(&self, range: JsRange) {
        self.set_selection(range.start, range.end);
    }

    /// Selects all content.
    #[napi]
    pub fn select_all(&self) {
        self.with_editor_mut(|editor| {
            let line_count = editor.state().document.line_count();
            if line_count == 0 {
                return;
            }

            let last_line = line_count.saturating_sub(1);
            let last_col = editor
                .state()
                .document
                .line(last_line)
                .map(|l| l.chars().count())
                .unwrap_or(0);

            let selection =
                Selection::new(Position::zero(), Position::new(last_line, last_col));
            editor.state_mut().cursor = iridium_editor::CursorState::new(selection);
        });
        self.emit_selection_changed();
    }

    /// Returns true if there is an active selection (non-collapsed).
    #[napi]
    pub fn has_selection(&self) -> bool {
        self.with_editor(|editor| !editor.state().cursor.primary.is_collapsed())
    }

    /// Gets the selected text (empty string if no selection).
    #[napi]
    pub fn get_selected_text(&self) -> String {
        self.with_editor(|editor| {
            let selection = &editor.state().cursor.primary;
            if selection.is_collapsed() {
                String::new()
            } else {
                editor.state().document.slice(selection.range())
            }
        })
    }

    /// Gets the number of cursors (1 for single cursor, >1 for multi-cursor).
    #[napi]
    pub fn get_cursor_count(&self) -> u32 {
        self.with_editor(|editor| editor.state().cursor.cursor_count() as u32)
    }

    /// Collapses all cursors to the primary cursor.
    #[napi]
    pub fn collapse_to_primary_cursor(&self) {
        self.with_editor_mut(|editor| {
            editor.state_mut().cursor.collapse_to_primary();
        });
        self.emit_selection_changed();
    }

    // =========================================================================
    // Undo/Redo Operations (T158)
    // =========================================================================

    /// Checks if undo is available.
    #[napi]
    pub fn can_undo(&self) -> bool {
        self.with_editor(|editor| editor.state().history.can_undo())
    }

    /// Checks if redo is available.
    #[napi]
    pub fn can_redo(&self) -> bool {
        self.with_editor(|editor| editor.state().history.can_redo())
    }

    /// Performs undo. Returns true if an action was undone.
    #[napi]
    pub fn undo(&self) -> bool {
        let result = self.with_editor_mut(|editor| editor.undo());
        if result {
            self.emit_content_and_selection_changed();
        }
        result
    }

    /// Performs redo. Returns true if an action was redone.
    #[napi]
    pub fn redo(&self) -> bool {
        let result = self.with_editor_mut(|editor| editor.redo());
        if result {
            self.emit_content_and_selection_changed();
        }
        result
    }

    /// Gets undo tree information.
    #[napi]
    pub fn get_undo_info(&self) -> JsUndoInfo {
        self.with_editor(|editor| {
            let info = editor.state().history.get_tree_info();
            JsUndoInfo {
                node_count: info.node_count as u32,
                branch_count: info.branch_count as u32,
                can_undo: info.can_undo,
                can_redo: info.can_redo,
                current_id: info.current_id.clone(),
                root_id: info.root_id.clone(),
            }
        })
    }

    // =========================================================================
    // Search and Replace Operations (T159)
    // =========================================================================

    /// Starts a search with the given query.
    ///
    /// Returns the search result with match count and current index.
    #[napi]
    pub fn find(&self, query: String, options: Option<JsSearchOptions>) -> Result<JsSearchResult> {
        let opts: SearchOptions = options.map(SearchOptions::from).unwrap_or_default();

        self.with_editor_mut(|editor| {
            editor
                .find(&query, &opts)
                .map_err(|e| Error::from_reason(e))?;

            Ok(JsSearchResult {
                match_count: editor.search_match_count() as u32,
                current_index: editor.current_match_index().map(|i| i as u32),
                matches: editor
                    .search_state()
                    .all_matches()
                    .iter()
                    .map(|r| (*r).into())
                    .collect(),
            })
        })
    }

    /// Updates the search query (for incremental search).
    #[napi]
    pub fn update_search(&self, query: String) -> Result<bool> {
        self.with_editor_mut(|editor| {
            editor
                .update_search(&query)
                .map_err(|e| Error::from_reason(e))
        })
    }

    /// Goes to the next search match.
    #[napi]
    pub fn next_match(&self) {
        self.with_editor_mut(|editor| {
            editor.goto_next_match();
        });
        self.emit_selection_changed();
    }

    /// Goes to the previous search match.
    #[napi]
    pub fn previous_match(&self) {
        self.with_editor_mut(|editor| {
            editor.goto_previous_match();
        });
        self.emit_selection_changed();
    }

    /// Gets the current search match count.
    #[napi]
    pub fn get_match_count(&self) -> u32 {
        self.with_editor(|editor| editor.search_match_count() as u32)
    }

    /// Gets the current match index (0-based).
    #[napi]
    pub fn get_current_match_index(&self) -> Option<u32> {
        self.with_editor(|editor| editor.current_match_index().map(|i| i as u32))
    }

    /// Replaces the current match with the replacement text.
    ///
    /// Returns true if a replacement was made.
    #[napi]
    pub fn replace_current(&self, replacement: String) -> bool {
        let result = self.with_editor_mut(|editor| editor.replace_current_match(&replacement));
        if result {
            self.emit_content_and_selection_changed();
        }
        result
    }

    /// Replaces all matches with the replacement text.
    ///
    /// This is a single undoable operation. Returns the number of replacements made.
    #[napi]
    pub fn replace_all(&self, replacement: String) -> u32 {
        let count = self.with_editor_mut(|editor| editor.replace_all_matches(&replacement));
        if count > 0 {
            self.emit_content_and_selection_changed();
        }
        count as u32
    }

    /// Closes/clears the current search.
    #[napi]
    pub fn close_search(&self) {
        self.with_editor_mut(|editor| {
            editor.close_search();
        });
    }

    /// Returns true if a search is currently active.
    #[napi]
    pub fn is_searching(&self) -> bool {
        self.with_editor(|editor| editor.is_searching())
    }

    // =========================================================================
    // Code Folding Operations (T160)
    // =========================================================================

    /// Folds the region at the given line.
    ///
    /// Returns true if the fold was successful.
    #[napi]
    pub fn fold_at(&self, line: u32) -> bool {
        self.with_editor_mut(|editor| editor.fold_at(line as usize))
    }

    /// Unfolds the region at the given line.
    ///
    /// Returns true if the unfold was successful.
    #[napi]
    pub fn unfold_at(&self, line: u32) -> bool {
        self.with_editor_mut(|editor| editor.unfold_at(line as usize))
    }

    /// Toggles the fold state at the given line.
    ///
    /// Returns true if the toggle was successful.
    #[napi]
    pub fn toggle_fold_at(&self, line: u32) -> bool {
        self.with_editor_mut(|editor| editor.toggle_fold_at(line as usize))
    }

    /// Folds all foldable regions.
    #[napi]
    pub fn fold_all(&self) {
        self.with_editor_mut(|editor| {
            editor.fold_all();
        });
    }

    /// Unfolds all folded regions.
    #[napi]
    pub fn unfold_all(&self) {
        self.with_editor_mut(|editor| {
            editor.unfold_all();
        });
    }

    /// Returns true if the line is foldable.
    #[napi]
    pub fn is_foldable(&self, line: u32) -> bool {
        self.with_editor(|editor| editor.is_foldable(line as usize))
    }

    /// Returns true if the line is currently folded.
    #[napi]
    pub fn is_folded(&self, line: u32) -> bool {
        self.with_editor(|editor| editor.is_folded(line as usize))
    }

    /// Returns true if the line is hidden by a fold.
    #[napi]
    pub fn is_line_hidden(&self, line: u32) -> bool {
        self.with_editor(|editor| editor.is_line_hidden(line as usize))
    }

    /// Gets the number of currently folded regions.
    #[napi]
    pub fn get_folded_count(&self) -> u32 {
        self.with_editor(|editor| editor.folded_count() as u32)
    }

    /// Gets the total number of foldable regions.
    #[napi]
    pub fn get_fold_region_count(&self) -> u32 {
        self.with_editor(|editor| editor.fold_region_count() as u32)
    }

    /// Gets the visible line count (total minus hidden lines).
    #[napi]
    pub fn get_visible_line_count(&self) -> u32 {
        self.with_editor(|editor| editor.visible_line_count() as u32)
    }

    /// Maps a visual line to a document line.
    #[napi]
    pub fn visual_to_document_line(&self, visual_line: u32) -> u32 {
        self.with_editor(|editor| editor.visual_to_document_line(visual_line as usize) as u32)
    }

    /// Maps a document line to a visual line (None if hidden).
    #[napi]
    pub fn document_to_visual_line(&self, doc_line: u32) -> Option<u32> {
        self.with_editor(|editor| {
            editor
                .document_to_visual_line(doc_line as usize)
                .map(|l| l as u32)
        })
    }

    /// Exports fold information for persistence.
    #[napi]
    pub fn export_fold_info(&self) -> JsFoldInfo {
        self.with_editor(|editor| editor.export_fold_info().into())
    }

    /// Imports fold information (e.g., from a saved session).
    #[napi]
    pub fn import_fold_info(&self, info: JsFoldInfo) {
        self.with_editor_mut(|editor| {
            editor.import_fold_info(&info.into());
        });
    }

    // =========================================================================
    // Theme Operations
    // =========================================================================

    /// Returns true if the current theme is dark.
    #[napi]
    pub fn is_dark_theme(&self) -> bool {
        self.with_editor(|editor| editor.is_dark_theme())
    }

    /// Switches to the default dark theme.
    #[napi]
    pub fn use_dark_theme(&self) {
        self.with_editor_mut(|editor| {
            editor.use_dark_theme();
        });
        self.event_emitter.emit("themeChanged", "dark");
    }

    /// Switches to the default light theme.
    #[napi]
    pub fn use_light_theme(&self) {
        self.with_editor_mut(|editor| {
            editor.use_light_theme();
        });
        self.event_emitter.emit("themeChanged", "light");
    }

    /// Gets the current theme name.
    #[napi]
    pub fn get_theme_name(&self) -> String {
        self.with_editor(|editor| editor.get_theme().name.clone())
    }

    // =========================================================================
    // Language Configuration
    // =========================================================================

    /// Sets the language for syntax highlighting and folding.
    #[napi]
    pub fn set_language(&self, language: String) -> bool {
        let lang = Language::from_extension(&language)
            .or_else(|| Language::from_id(&language));

        if let Some(lang) = lang {
            self.with_editor_mut(|editor| {
                editor.set_language(lang);
            });
            true
        } else {
            false
        }
    }

    /// Gets the current language ID (if set).
    #[napi]
    pub fn get_language(&self) -> Option<String> {
        self.with_editor(|editor| editor.language().map(|l| l.id().to_string()))
    }

    // =========================================================================
    // Focus and State Operations (T167)
    // =========================================================================

    /// Sets the focus state of the editor.
    #[napi]
    pub fn set_focus(&self, focused: bool) {
        self.with_editor_mut(|editor| {
            editor.state_mut().has_focus = focused;
        });
        let event_name = if focused { "focus" } else { "blur" };
        self.event_emitter.emit(event_name, "");
    }

    /// Returns true if the editor has focus.
    #[napi]
    pub fn has_focus(&self) -> bool {
        self.with_editor(|editor| editor.state().has_focus)
    }

    /// Gives focus to the editor.
    #[napi]
    pub fn focus(&self) {
        self.set_focus(true);
    }

    /// Removes focus from the editor.
    #[napi]
    pub fn blur(&self) {
        self.set_focus(false);
    }

    /// Sets the read-only state.
    #[napi]
    pub fn set_read_only(&self, read_only: bool) {
        self.with_editor_mut(|editor| {
            editor.state_mut().read_only = read_only;
        });
    }

    /// Returns true if the editor is read-only.
    #[napi]
    pub fn is_read_only(&self) -> bool {
        self.with_editor(|editor| editor.state().read_only)
    }

    /// Notifies the editor of a resize.
    ///
    /// This should be called when the editor's container is resized.
    #[napi]
    pub fn resize(&self, width: f64, height: f64) {
        self.with_editor_mut(|editor| {
            editor.state_mut().viewport.resize(width as f32, height as f32);
        });
    }

    // =========================================================================
    // Scroll Operations
    // =========================================================================

    /// Gets the current scroll position (first visible line).
    #[napi]
    pub fn get_scroll_line(&self) -> u32 {
        self.with_editor(|editor| editor.state().scroll_line as u32)
    }

    /// Scrolls to a specific line.
    #[napi]
    pub fn scroll_to_line(&self, line: u32) {
        self.with_editor_mut(|editor| {
            editor.scroll_to_line(line as usize);
        });
    }

    /// Scrolls to make a position visible.
    #[napi]
    pub fn reveal_position(&self, position: JsPosition) {
        // For now, just scroll to the line
        self.scroll_to_line(position.line);
    }

    // =========================================================================
    // Event Subscription (T161)
    // =========================================================================

    /// Subscribes to an editor event.
    ///
    /// Supported events:
    /// - `contentChanged`: Fired when content changes
    /// - `selectionChanged`: Fired when selection/cursor changes
    /// - `themeChanged`: Fired when theme changes
    /// - `focus`: Fired when editor gains focus
    /// - `blur`: Fired when editor loses focus
    /// - `searchUpdated`: Fired when search results change
    /// - `foldChanged`: Fired when fold state changes
    ///
    /// Returns a subscription ID that can be used to unsubscribe.
    #[napi]
    pub fn on(&self, event: String, callback: EventCallback) -> u32 {
        self.event_emitter.subscribe(&event, callback)
    }

    /// Unsubscribes from an editor event.
    #[napi]
    pub fn off(&self, subscription_id: u32) {
        self.event_emitter.unsubscribe(subscription_id);
    }

    /// Removes all event listeners for a specific event type.
    #[napi]
    pub fn remove_all_listeners(&self, event: Option<String>) {
        if let Some(event_name) = event {
            self.event_emitter.remove_listeners(&event_name);
        } else {
            self.event_emitter.clear_all();
        }
    }

    // =========================================================================
    // Cleanup (T168)
    // =========================================================================

    /// Destroys the editor and releases all resources.
    ///
    /// After calling this method, the editor should not be used.
    #[napi]
    pub fn destroy(&self) {
        self.event_emitter.clear_all();
        // The editor will be dropped when all references are released
    }

    // =========================================================================
    // Internal Helpers
    // =========================================================================

    /// Helper to safely access the editor for reading.
    fn with_editor<T, F>(&self, f: F) -> T
    where
        F: FnOnce(&Editor) -> T,
    {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        f(&guard)
    }

    /// Helper to safely access the editor for mutation.
    fn with_editor_mut<T, F>(&self, f: F) -> T
    where
        F: FnOnce(&mut Editor) -> T,
    {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard)
    }

    /// Emits a generic event.
    fn emit_event(&self, event: &str, data: &str) {
        self.event_emitter.emit(event, data);
    }

    /// Emits a selection changed event.
    fn emit_selection_changed(&self) {
        // Serialize selection info
        let selection = self.get_selection();
        let json = serde_json::to_string(&selection).unwrap_or_default();
        self.event_emitter.emit("selectionChanged", &json);
    }

    /// Emits both content and selection changed events.
    fn emit_content_and_selection_changed(&self) {
        let content = self.get_content();
        self.event_emitter.emit("contentChanged", &content);
        self.emit_selection_changed();
    }
}

impl Default for IridiumEditor {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Editor::with_defaults())),
            event_emitter: EventEmitter::new(),
        }
    }
}

// =========================================================================
// Type Conversions
// =========================================================================

impl From<JsEditorConfig> for EditorConfig {
    fn from(config: JsEditorConfig) -> Self {
        let mut editor_config = EditorConfig::default();

        if let Some(tw) = config.tab_width {
            editor_config.tab_width = tw as usize;
        }
        if let Some(is) = config.insert_spaces {
            editor_config.insert_spaces = is;
        }
        if let Some(ai) = config.auto_indent {
            editor_config.auto_indent = ai;
        }
        if let Some(sln) = config.show_line_numbers {
            editor_config.show_line_numbers = sln;
        }
        if let Some(sm) = config.show_minimap {
            editor_config.show_minimap = sm;
        }
        if let Some(cbm) = config.cursor_blink_ms {
            editor_config.cursor_blink_ms = cbm;
        }
        if let Some(ugt) = config.undo_group_timeout_ms {
            editor_config.undo_group_timeout_ms = u64::from(ugt);
        }

        editor_config
    }
}

impl From<JsSearchOptions> for SearchOptions {
    fn from(opts: JsSearchOptions) -> Self {
        SearchOptions {
            case_sensitive: opts.case_sensitive.unwrap_or(false),
            whole_word: opts.whole_word.unwrap_or(false),
            regex: opts.regex.unwrap_or(false),
        }
    }
}

impl From<JsPosition> for Position {
    fn from(pos: JsPosition) -> Self {
        Position::new(pos.line as usize, pos.column as usize)
    }
}

impl From<Position> for JsPosition {
    fn from(pos: Position) -> Self {
        JsPosition {
            line: pos.line as u32,
            column: pos.column as u32,
        }
    }
}

impl From<JsRange> for Range {
    fn from(range: JsRange) -> Self {
        Range::new(range.start.into(), range.end.into())
    }
}

impl From<Range> for JsRange {
    fn from(range: Range) -> Self {
        JsRange {
            start: range.start.into(),
            end: range.end.into(),
        }
    }
}

impl From<Selection> for JsSelection {
    fn from(sel: Selection) -> Self {
        JsSelection {
            anchor: sel.anchor.into(),
            head: sel.head.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_creation() {
        let editor = IridiumEditor::new().unwrap();
        assert_eq!(editor.get_content(), "");
        assert_eq!(editor.get_line_count(), 1);
    }

    #[test]
    fn editor_content_operations() {
        let editor = IridiumEditor::new().unwrap();
        editor.set_content("Hello\nWorld".to_string());

        assert_eq!(editor.get_content(), "Hello\nWorld");
        assert_eq!(editor.get_line_count(), 2);
        // line() returns content without trailing newlines
        assert_eq!(editor.get_line(0), Some("Hello".to_string()));
        assert_eq!(editor.get_line(1), Some("World".to_string()));
    }

    #[test]
    fn editor_cursor_operations() {
        let editor = IridiumEditor::new().unwrap();
        editor.set_content("Hello\nWorld".to_string());

        editor.set_cursor(1, 3);
        assert_eq!(editor.get_cursor_line(), 1);
        assert_eq!(editor.get_cursor_column(), 3);
    }

    #[test]
    fn editor_selection_operations() {
        let editor = IridiumEditor::new().unwrap();
        editor.set_content("Hello World".to_string());

        editor.set_selection(
            JsPosition { line: 0, column: 0 },
            JsPosition { line: 0, column: 5 },
        );

        assert!(editor.has_selection());
        assert_eq!(editor.get_selected_text(), "Hello");
    }

    #[test]
    fn editor_undo_redo() {
        let editor = IridiumEditor::new().unwrap();

        // Initially nothing to undo
        assert!(!editor.can_undo());
        assert!(!editor.can_redo());
    }

    #[test]
    fn editor_focus_state() {
        let editor = IridiumEditor::new().unwrap();

        assert!(!editor.has_focus());
        editor.focus();
        assert!(editor.has_focus());
        editor.blur();
        assert!(!editor.has_focus());
    }

    #[test]
    fn editor_read_only() {
        let editor = IridiumEditor::new().unwrap();

        assert!(!editor.is_read_only());
        editor.set_read_only(true);
        assert!(editor.is_read_only());
    }
}
