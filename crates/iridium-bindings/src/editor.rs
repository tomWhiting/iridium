//! Editor bindings for TypeScript/WASM.
//!
//! This module provides comprehensive napi-rs bindings for the Iridium editor,
//! exposing all editor operations to TypeScript consumers.

use std::sync::{Arc, Mutex, PoisonError};

use napi::bindgen_prelude::*;
use napi_derive::napi;

use iridium_editor::{Editor, EditorConfig, Position, Range, Selection, search::SearchOptions};
use iridium_syntax::Language;

use crate::events::{EventCallback, EventEmitter};
use crate::types::{
    JsEditorConfig, JsFoldInfo, JsPosition, JsRange, JsSearchOptions, JsSearchResult, JsSelection,
    JsUndoInfo, usize_to_u32,
};
use crate::with_napi_str;

/// Converts a JavaScript viewport dimension to the renderer's pixel type.
fn viewport_dimension(value: f64, name: &str) -> Result<f32> {
    if !value.is_finite() || value < 0.0 {
        return Err(Error::new(
            Status::InvalidArg,
            format!("viewport {name} must be a finite, non-negative number"),
        ));
    }
    if value > f64::from(f32::MAX) {
        return Err(Error::new(
            Status::InvalidArg,
            format!("viewport {name} exceeds the renderer's supported range"),
        ));
    }

    // napi-rs exposes JavaScript numbers as f64 but the renderer stores pixel
    // dimensions as f32. After validating the range, decimal parsing performs
    // defined nearest-f32 rounding; that rounding is appropriate for f32-based
    // sub-pixel rendering and avoids an unchecked narrowing cast.
    value.to_string().parse::<f32>().map_err(|error| {
        Error::new(
            Status::InvalidArg,
            format!("viewport {name} cannot be represented by the renderer: {error}"),
        )
    })
}

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
        self.with_editor(Editor::content)
    }

    /// Sets the content, replacing everything.
    ///
    /// This resets the cursor position and clears the undo history.
    #[napi]
    pub fn set_content(&self, content: String) {
        with_napi_str(content, |content| {
            self.with_editor_mut(|editor| editor.set_content(content));
            self.emit_event("contentChanged", content);
        });
    }

    /// Gets the current line count.
    #[napi]
    pub fn get_line_count(&self) -> Result<u32> {
        self.with_editor(|editor| {
            usize_to_u32(editor.state().document.line_count(), "document line count")
        })
    }

    /// Gets a specific line by number (0-indexed).
    #[napi]
    pub fn get_line(&self, line_number: u32) -> Option<String> {
        self.with_editor(|editor| editor.state().document.line(line_number as usize))
    }

    /// Gets the length of a specific line (in characters).
    #[napi]
    pub fn get_line_length(&self, line_number: u32) -> Result<Option<u32>> {
        self.with_editor(|editor| {
            editor
                .state()
                .document
                .line(line_number as usize)
                .map(|line| usize_to_u32(line.chars().count(), "line character count"))
                .transpose()
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
    pub fn get_cursor(&self) -> Result<JsPosition> {
        self.with_editor(|editor| editor.cursor().try_into())
    }

    /// Gets the cursor line (0-indexed).
    #[napi]
    pub fn get_cursor_line(&self) -> Result<u32> {
        self.with_editor(|editor| usize_to_u32(editor.cursor().line, "cursor line index"))
    }

    /// Gets the cursor column (0-indexed).
    #[napi]
    pub fn get_cursor_column(&self) -> Result<u32> {
        self.with_editor(|editor| usize_to_u32(editor.cursor().column, "cursor column index"))
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
    pub fn get_selection(&self) -> Result<JsSelection> {
        self.with_editor(|editor| editor.state().cursor.primary.try_into())
    }

    /// Gets all selections (for multi-cursor support).
    #[napi]
    pub fn get_all_selections(&self) -> Result<Vec<JsSelection>> {
        self.with_editor(|editor| {
            editor
                .state()
                .cursor
                .all_selections()
                .copied()
                .map(JsSelection::try_from)
                .collect()
        })
    }

    /// Sets the selection.
    #[napi]
    pub fn set_selection(&self, anchor: JsPosition, head: JsPosition) {
        self.with_editor_mut(|editor| {
            // Clamps both endpoints and resets keyboard vertical state.
            editor.set_selection(anchor.into(), head.into());
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
            let last_line = editor.state().document.line_count().saturating_sub(1);
            let last_col = editor
                .state()
                .document
                .line(last_line)
                .map_or(0, |line| line.chars().count());

            // Editor::set_selection clamps and resets keyboard vertical state.
            editor.set_selection(Position::zero(), Position::new(last_line, last_col));
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
    pub fn get_cursor_count(&self) -> Result<u32> {
        self.with_editor(|editor| {
            usize_to_u32(editor.state().cursor.cursor_count(), "cursor count")
        })
    }

    /// Collapses all cursors to the primary cursor.
    #[napi]
    pub fn collapse_to_primary_cursor(&self) {
        self.with_editor_mut(|editor| {
            // Reproduce the primary selection through the clamping API, which
            // drops secondary cursors and resets keyboard vertical state.
            let primary = editor.state().cursor.primary;
            editor.set_selection(primary.anchor, primary.head);
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
        let result = self.with_editor_mut(Editor::undo);
        if result {
            self.emit_content_and_selection_changed();
        }
        result
    }

    /// Performs redo. Returns true if an action was redone.
    #[napi]
    pub fn redo(&self) -> bool {
        let result = self.with_editor_mut(Editor::redo);
        if result {
            self.emit_content_and_selection_changed();
        }
        result
    }

    /// Gets undo tree information.
    #[napi]
    pub fn get_undo_info(&self) -> Result<JsUndoInfo> {
        self.with_editor(|editor| {
            let info = editor.state().history.get_tree_info();
            Ok(JsUndoInfo {
                node_count: usize_to_u32(info.node_count, "undo node count")?,
                branch_count: usize_to_u32(info.branch_count, "undo branch count")?,
                can_undo: info.can_undo,
                can_redo: info.can_redo,
                current_id: info.current_id,
                root_id: info.root_id,
            })
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

        with_napi_str(query, |query| {
            self.with_editor_mut(|editor| {
                editor.find(query, &opts).map_err(Error::from_reason)?;

                let matches = editor
                    .search_state()
                    .all_matches()
                    .iter()
                    .copied()
                    .map(JsRange::try_from)
                    .collect::<Result<Vec<_>>>()?;
                Ok(JsSearchResult {
                    match_count: usize_to_u32(editor.search_match_count(), "search match count")?,
                    current_index: editor
                        .current_match_index()
                        .map(|index| usize_to_u32(index, "current search match index"))
                        .transpose()?,
                    matches,
                })
            })
        })
    }

    /// Updates the search query (for incremental search).
    #[napi]
    pub fn update_search(&self, query: String) -> Result<bool> {
        with_napi_str(query, |query| {
            self.with_editor_mut(|editor| editor.update_search(query).map_err(Error::from_reason))
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
    pub fn get_match_count(&self) -> Result<u32> {
        self.with_editor(|editor| usize_to_u32(editor.search_match_count(), "search match count"))
    }

    /// Gets the current match index (0-based).
    #[napi]
    pub fn get_current_match_index(&self) -> Result<Option<u32>> {
        self.with_editor(|editor| {
            editor
                .current_match_index()
                .map(|index| usize_to_u32(index, "current search match index"))
                .transpose()
        })
    }

    /// Replaces the current match with the replacement text.
    ///
    /// Returns true if a replacement was made.
    #[napi]
    pub fn replace_current(&self, replacement: String) -> bool {
        let result = with_napi_str(replacement, |replacement| {
            self.with_editor_mut(|editor| editor.replace_current_match(replacement))
        });
        if result {
            self.emit_content_and_selection_changed();
        }
        result
    }

    /// Replaces all matches with the replacement text.
    ///
    /// This is a single undoable operation. Returns the number of replacements made.
    #[napi]
    pub fn replace_all(&self, replacement: String) -> Result<u32> {
        with_napi_str(replacement, |replacement| {
            let count = self.with_editor_mut(|editor| editor.replace_all_matches(replacement));
            if count > 0 {
                self.emit_content_and_selection_changed();
            }
            usize_to_u32(count, "replacement count")
        })
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
        self.with_editor(Editor::is_searching)
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
    pub fn get_folded_count(&self) -> Result<u32> {
        self.with_editor(|editor| usize_to_u32(editor.folded_count(), "folded region count"))
    }

    /// Gets the total number of foldable regions.
    #[napi]
    pub fn get_fold_region_count(&self) -> Result<u32> {
        self.with_editor(|editor| usize_to_u32(editor.fold_region_count(), "foldable region count"))
    }

    /// Gets the visible line count (total minus hidden lines).
    #[napi]
    pub fn get_visible_line_count(&self) -> Result<u32> {
        self.with_editor(|editor| usize_to_u32(editor.visible_line_count(), "visible line count"))
    }

    /// Maps a visual line to a document line.
    #[napi]
    pub fn visual_to_document_line(&self, visual_line: u32) -> Result<u32> {
        self.with_editor(|editor| {
            let document_line = editor.visual_to_document_line(visual_line as usize);
            usize_to_u32(document_line, "document line index")
        })
    }

    /// Maps a document line to a visual line (None if hidden).
    #[napi]
    pub fn document_to_visual_line(&self, doc_line: u32) -> Result<Option<u32>> {
        self.with_editor(|editor| {
            editor
                .document_to_visual_line(doc_line as usize)
                .map(|line| usize_to_u32(line, "visual line index"))
                .transpose()
        })
    }

    /// Exports fold information for persistence.
    #[napi]
    pub fn export_fold_info(&self) -> Result<JsFoldInfo> {
        self.with_editor(|editor| editor.export_fold_info().try_into())
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
        self.with_editor(Editor::is_dark_theme)
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
        with_napi_str(language, |language| {
            Language::from_extension(language)
                .or_else(|| Language::from_id(language))
                .is_some_and(|language| {
                    self.with_editor_mut(|editor| editor.set_language(language));
                    true
                })
        })
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
    pub fn resize(&self, width: f64, height: f64) -> Result<()> {
        let width = viewport_dimension(width, "width")?;
        let height = viewport_dimension(height, "height")?;
        self.with_editor_mut(|editor| editor.state_mut().viewport.resize(width, height));
        Ok(())
    }

    // =========================================================================
    // Scroll Operations
    // =========================================================================

    /// Gets the current scroll position (first visible line).
    #[napi]
    pub fn get_scroll_line(&self) -> Result<u32> {
        self.with_editor(|editor| usize_to_u32(editor.state().scroll_line, "scroll line index"))
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
        self.event_emitter.subscribe(event, callback)
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
        let guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        f(&guard)
    }

    /// Helper to safely access the editor for mutation.
    fn with_editor_mut<T, F>(&self, f: F) -> T
    where
        F: FnOnce(&mut Editor) -> T,
    {
        let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        f(&mut guard)
    }

    /// Emits a generic event.
    fn emit_event(&self, event: &str, data: &str) {
        self.event_emitter.emit(event, data);
    }

    /// Emits a selection changed event.
    fn emit_selection_changed(&self) {
        let serialized = self.get_selection().and_then(|selection| {
            serde_json::to_string(&selection).map_err(|error| Error::from_reason(error.to_string()))
        });
        match serialized {
            Ok(json) => self.event_emitter.emit("selectionChanged", json),
            Err(error) => {
                // A selection outside the JavaScript u32 coordinate space must not be
                // misreported as a different location. Surface the conversion failure.
                self.event_emitter.emit("error", error.to_string());
            },
        }
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
        let mut editor_config = Self::default();

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
        Self {
            case_sensitive: opts.case_sensitive.unwrap_or(false),
            whole_word: opts.whole_word.unwrap_or(false),
            regex: opts.regex.unwrap_or(false),
        }
    }
}

impl From<JsPosition> for Position {
    fn from(pos: JsPosition) -> Self {
        Self::new(pos.line as usize, pos.column as usize)
    }
}

impl TryFrom<Position> for JsPosition {
    type Error = Error;

    fn try_from(pos: Position) -> Result<Self> {
        Ok(Self {
            line: usize_to_u32(pos.line, "position line index")?,
            column: usize_to_u32(pos.column, "position column index")?,
        })
    }
}

impl From<JsRange> for Range {
    fn from(range: JsRange) -> Self {
        Self::new(range.start.into(), range.end.into())
    }
}

impl TryFrom<Range> for JsRange {
    type Error = Error;

    fn try_from(range: Range) -> Result<Self> {
        Ok(Self {
            start: range.start.try_into()?,
            end: range.end.try_into()?,
        })
    }
}

impl TryFrom<Selection> for JsSelection {
    type Error = Error;

    fn try_from(selection: Selection) -> Result<Self> {
        Ok(Self {
            anchor: selection.anchor.try_into()?,
            head: selection.head.try_into()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_dimensions_are_checked_before_narrowing() {
        let expected_width = 640.5_f32;
        let actual_width = viewport_dimension(640.5, "width").unwrap();
        // One relative f32 epsilon verifies renderer-level rounding without requiring
        // bit-identical floating-point values.
        assert!((actual_width - expected_width).abs() <= f32::EPSILON * expected_width);
        assert!(viewport_dimension(-1.0, "width").is_err());
        assert!(viewport_dimension(f64::NAN, "width").is_err());
        assert!(viewport_dimension(f64::INFINITY, "height").is_err());
        assert!(viewport_dimension(f64::from(f32::MAX) * 2.0, "height").is_err());
    }

    #[test]
    fn editor_creation() {
        let editor = IridiumEditor::new().unwrap();
        assert_eq!(editor.get_content(), "");
        assert_eq!(editor.get_line_count().unwrap(), 1);
    }

    #[test]
    fn editor_content_operations() {
        let editor = IridiumEditor::new().unwrap();
        editor.set_content("Hello\nWorld".to_string());

        assert_eq!(editor.get_content(), "Hello\nWorld");
        assert_eq!(editor.get_line_count().unwrap(), 2);
        // line() returns content without trailing newlines
        assert_eq!(editor.get_line(0), Some("Hello".to_string()));
        assert_eq!(editor.get_line(1), Some("World".to_string()));
    }

    #[test]
    fn editor_cursor_operations() {
        let editor = IridiumEditor::new().unwrap();
        editor.set_content("Hello\nWorld".to_string());

        editor.set_cursor(1, 3);
        assert_eq!(editor.get_cursor_line().unwrap(), 1);
        assert_eq!(editor.get_cursor_column().unwrap(), 3);
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
