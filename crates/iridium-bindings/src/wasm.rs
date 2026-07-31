//! WebAssembly bindings for browser usage.
//!
//! This module provides the entry points for using Iridium in a web browser
//! via WebAssembly. It wraps the editor and rendering functionality in
//! wasm-bindgen exports.

use std::collections::HashMap;

use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::edit_tracking::{
    EditSpan, EditSpanError, PendingEdit, byte_point, compose_pending, compute_edit_span,
};
use crate::key_map::key_code_from_dom_key;
use crate::palette;
use crate::text_range::text_range;
use crate::web_span_index::{WebSpan, WebSpanIndex};
use iridium_editor::{
    CommandArgs, CommandId, EditorConfig, Keymap, ModifierPattern, Position, Range, StrokePattern,
    commands::{builtin, palette::CommandMru},
    editor::{Editor, FoldState},
    history::{Command, UndoNodeId},
    input::{
        ClipboardOperation, CommandRunError, HistoryRequest, KeyCode, KeyEvent, KeyResult,
        KeyboardHandler, Modifiers, SearchAction,
    },
    render::{
        CursorRenderer, GutterRenderer, Quad, QuadRenderer, SimpleHighlighter, TextRenderer,
        Viewport, ViewportConfig, WebSurface,
    },
    syntax_stubs::{Language, SyntaxTree},
    theme::Theme,
};

/// Initialize panic hook for better error messages in browser console.
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// A highlight span from tree-sitter (passed from JavaScript).
#[derive(Debug, Clone)]
pub struct JsHighlightSpan {
    /// Start byte offset
    pub start: usize,
    /// End byte offset
    pub end: usize,
    /// Highlight type as string (e.g., "keyword", "string", "comment")
    pub highlight_type: String,
}

/// Byte-accurate description of a document edit, for incremental
/// tree-sitter parsing on the JavaScript side.
///
/// All byte offsets and points are computed from the rope (see
/// [`WebEditor::take_last_edit`]), never from JavaScript strings. Rows are
/// 0-indexed document lines; columns are byte offsets within the row
/// (tree-sitter's `Point` convention).
#[wasm_bindgen]
#[derive(Debug, Clone, Copy)]
pub struct JsEditInfo {
    start_byte: usize,
    old_end_byte: usize,
    new_end_byte: usize,
    start_row: usize,
    start_column: usize,
    old_end_row: usize,
    old_end_column: usize,
    new_end_row: usize,
    new_end_column: usize,
}

// wasm-bindgen cannot export `const fn`, so the trivial getters below
// stay non-const by necessity.
#[allow(clippy::missing_const_for_fn)]
#[wasm_bindgen]
impl JsEditInfo {
    /// Byte offset where the edit begins (pre- and post-edit documents agree).
    #[wasm_bindgen(getter, js_name = startByte)]
    pub fn start_byte(&self) -> usize {
        self.start_byte
    }

    /// Byte offset where the replaced text ended in the pre-edit document.
    #[wasm_bindgen(getter, js_name = oldEndByte)]
    pub fn old_end_byte(&self) -> usize {
        self.old_end_byte
    }

    /// Byte offset where the new text ends in the post-edit document.
    #[wasm_bindgen(getter, js_name = newEndByte)]
    pub fn new_end_byte(&self) -> usize {
        self.new_end_byte
    }

    /// Row of the edit start.
    #[wasm_bindgen(getter, js_name = startRow)]
    pub fn start_row(&self) -> usize {
        self.start_row
    }

    /// Byte column of the edit start within its row.
    #[wasm_bindgen(getter, js_name = startColumn)]
    pub fn start_column(&self) -> usize {
        self.start_column
    }

    /// Row of the old end position (pre-edit document).
    #[wasm_bindgen(getter, js_name = oldEndRow)]
    pub fn old_end_row(&self) -> usize {
        self.old_end_row
    }

    /// Byte column of the old end position within its row (pre-edit document).
    #[wasm_bindgen(getter, js_name = oldEndColumn)]
    pub fn old_end_column(&self) -> usize {
        self.old_end_column
    }

    /// Row of the new end position (post-edit document).
    #[wasm_bindgen(getter, js_name = newEndRow)]
    pub fn new_end_row(&self) -> usize {
        self.new_end_row
    }

    /// Byte column of the new end position within its row (post-edit document).
    #[wasm_bindgen(getter, js_name = newEndColumn)]
    pub fn new_end_column(&self) -> usize {
        self.new_end_column
    }
}

/// Maximum expected lines in viewport (for pre-allocation sizing).
/// One host command a binding resolved to, on its way to JavaScript.
///
/// Serialized rather than returned as separate accessors so the id and the
/// arguments a key sequence captured cannot be read out of step with each other.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct HostCommandRequest {
    /// The registered command id the binding named.
    command: String,
    /// The numeric prefix the user typed, if any.
    count: Option<u32>,
    /// Characters captured by wildcard strokes, in sequence order.
    captures: Vec<char>,
}

/// Typical: 50 lines visible + 20 overscan = 70 lines.
const MAX_VIEWPORT_LINES: usize = 128;

/// Maximum expected selection quads per frame.
/// Typical: 1 quad per selected line, rarely more than viewport.
const MAX_SELECTION_QUADS: usize = 128;

/// WebEditor provides a complete browser-based editor experience.
///
/// This wraps the Iridium editor with WebGPU rendering capabilities
/// for use in web browsers.
#[wasm_bindgen]
pub struct WebEditor {
    editor: Editor,
    surface: WebSurface,
    text_renderer: TextRenderer,
    /// Quad renderer for background elements (gutter, selection highlights)
    background_quad_renderer: QuadRenderer,
    /// Quad renderer for foreground elements (cursor) - separate buffer to avoid GPU conflicts
    cursor_quad_renderer: QuadRenderer,
    cursor_renderer: CursorRenderer,
    gutter_renderer: GutterRenderer,
    highlighter: SimpleHighlighter,
    theme: Theme,
    needs_redraw: bool,
    /// Device pixel ratio for HiDPI scaling
    pixel_ratio: f32,
    /// Whether syntax highlighting is enabled
    syntax_enabled: bool,
    /// Whether gutter (line numbers) is enabled
    gutter_enabled: bool,
    /// Code folding state
    fold_state: FoldState,
    /// The parse tree the fold regions are read from.
    ///
    /// Without the `syntax` feature — which is every wasm build — this is the
    /// stub tree and costs nothing; the fold detector on this path matches
    /// braces and ignores it. The field exists so the call shape here is the
    /// same one the native kernel uses.
    fold_tree: SyntaxTree,
    /// Vertical scroll offset in pixels
    scroll_y: f32,
    /// Cached character width (measured from actual font metrics)
    cached_char_width: f32,
    /// Tree-sitter highlight spans from JavaScript (when available)
    ts_highlights: Vec<JsHighlightSpan>,
    /// Whether to use tree-sitter highlights from JS
    use_ts_highlights: bool,
    /// Span index for efficient viewport-based queries (O(log n + k))
    span_index: WebSpanIndex,
    /// Viewport configuration for overscan buffer
    viewport_config: ViewportConfig,
    /// Cached viewport dimensions to avoid redundant GPU updates
    cached_viewport_width: u32,
    cached_viewport_height: u32,

    // =========================================================================
    // Pre-allocated buffers for render_frame() - eliminates per-frame allocations
    // =========================================================================
    /// Pre-allocated buffer for visible content string.
    /// Capacity: ~10KB (typical viewport worth of text).
    cpu_visible_content: String,
    /// Pre-allocated buffer for document-to-visual line mapping.
    /// Capacity: document line count (grows as needed).
    cpu_doc_to_visual: Vec<Option<usize>>,
    /// Pre-allocated buffer for visible document line indices.
    /// Capacity: MAX_VIEWPORT_LINES.
    cpu_visible_doc_lines: Vec<usize>,
    /// Pre-allocated buffer for line numbers string.
    /// Capacity: ~2KB (typical viewport worth of line numbers).
    cpu_line_numbers: String,
    /// Pre-allocated buffer for gutter background quads.
    /// Capacity: 1-2 quads typically.
    cpu_gutter_quads: Vec<Quad>,
    /// Pre-allocated buffer for selection highlight quads.
    /// Capacity: MAX_SELECTION_QUADS.
    cpu_selection_quads: Vec<Quad>,
    /// Pre-allocated buffer for cursor quads.
    /// Capacity: 1-2 quads typically.
    cpu_cursor_quads: Vec<Quad>,
    /// Pre-allocated buffer for combined background quads.
    /// Capacity: gutter + selection quads.
    cpu_background_quads: Vec<Quad>,

    // =========================================================================
    // Cached layout info for pixel-to-position mapping with word wrap
    // =========================================================================
    /// Cached visual line mapping built during render_frame.
    /// Each entry maps a visual line to (buffer_line_index, start_column_in_line).
    cached_visual_line_map: Vec<(usize, usize)>,
    /// The viewport_start value when cached_visual_line_map was built.
    cached_map_viewport_start: usize,
    /// Content offset X when the map was built.
    cached_content_offset_x: f32,

    // =========================================================================
    // Cached scroll data for word-wrap-aware scrolling
    // =========================================================================
    /// Total visual lines including wrapped sub-lines, updated each render_frame.
    cached_total_visual_lines: usize,
    /// Absolute cursor Y position (document space) from last render_frame.
    cached_cursor_abs_y: f32,
    /// The cursor doc line when cached_cursor_abs_y was computed.
    cached_cursor_doc_line: usize,

    // =========================================================================
    // Externalized syntax theme — colors provided by host application
    // =========================================================================
    /// Map of capture name → color, populated via setSyntaxTheme from JS.
    /// When non-empty, used by color_for_highlight_type with hierarchical fallback.
    syntax_theme: HashMap<String, iridium_editor::theme::Color>,

    // =========================================================================
    // Git integration: read-only, line backgrounds, gutter changes, blame
    // =========================================================================
    /// When true, all content-mutating operations (insert, delete, etc.) are no-ops.
    read_only: bool,
    /// Per-line background colors (doc_line → color). Used for diff highlighting.
    line_backgrounds: HashMap<usize, iridium_editor::theme::Color>,
    /// Pre-allocated buffer for line background quads.
    cpu_line_bg_quads: Vec<Quad>,
    /// Per-line gutter change bar colors (doc_line → color). Used for change indicators.
    gutter_changes: HashMap<usize, iridium_editor::theme::Color>,
    /// Pre-allocated buffer for gutter change bar quads.
    cpu_gutter_change_quads: Vec<Quad>,
    /// Custom gutter text lines (replaces auto line numbers when Some).
    custom_gutter_lines: Option<Vec<String>>,
    /// Per-line blame text (doc_line → formatted blame string). Shown at end of cursor line.
    blame_data: HashMap<usize, String>,
    /// Pre-allocated buffer for blame text rendering.
    cpu_blame_content: String,

    // =========================================================================
    // Raw key event handling (the Rust core owns all editing behavior)
    // =========================================================================
    /// Keyboard handler driving the editor core from raw DOM key events.
    ///
    /// `WebEditor` drives the handler directly (instead of calling
    /// `Editor::handle_key`) because the binding needs the produced
    /// [`Command`] to record byte-accurate edit spans before application,
    /// and needs to distinguish consumed keys from ignored ones —
    /// `Editor::handle_key` exposes neither. The dispatch in
    /// [`WebEditor::handle_key_event`] mirrors `Editor::handle_key` exactly
    /// (read-only gating included) and applies commands through the same
    /// `Editor::apply_command` path.
    ///
    /// The handler carries per-cursor sticky columns for vertical movement;
    /// every `WebEditor` path that moves the cursor or edits content outside
    /// [`WebEditor::handle_key_event`] must call
    /// [`KeyboardHandler::reset_vertical_state`] on it.
    keyboard_handler: KeyboardHandler,
    /// Clipboard text stashed by the last `copy`/`cut` key result, consumed
    /// by `getPendingClipboardText` (the text never rides in the
    /// `handleKeyEvent` return value).
    pending_clipboard_text: Option<String>,

    /// Commands recently run by id, biasing the command palette's ranking.
    ///
    /// Lives beside [`Self::keyboard_handler`] rather than inside `self.editor`
    /// because this face routes every invocation through *this* handler; a second
    /// history on the editor's own handler would record nothing and silently
    /// disagree with the one the palette reads.
    palette_mru: CommandMru,

    /// The host command a binding last resolved to, awaiting
    /// `takePendingHostCommand`.
    ///
    /// Held rather than returned directly because `handleKeyEvent` answers with a
    /// status string; the id and its arguments would not fit that shape.
    pending_host_command: Option<HostCommandRequest>,
    /// Edit accumulated since the last `takeLastEdit` call.
    pending_edit: PendingEdit,
}

/// Log a message to the browser console.
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

/// Creates a new WebEditor attached to a canvas element.
///
/// Use this instead of `new WebEditor()` since async constructors are deprecated.
///
/// # Arguments
///
/// * `canvas` - The HTML canvas element to render to
/// * `pixel_ratio` - The device pixel ratio (window.devicePixelRatio) for HiDPI scaling
#[wasm_bindgen(js_name = createWebEditor)]
pub async fn create_web_editor(
    canvas: HtmlCanvasElement,
    pixel_ratio: f32,
) -> Result<WebEditor, JsValue> {
    log("[Iridium] Creating WebEditor...");

    // Get canvas dimensions
    let width = canvas.width();
    let height = canvas.height();
    log(&format!(
        "[Iridium] Canvas size: {}x{}, pixel ratio: {}",
        width, height, pixel_ratio
    ));

    // Create the web surface
    log("[Iridium] Creating WebSurface...");
    let surface = WebSurface::from_canvas(canvas, width, height)
        .await
        .map_err(|e| {
            let msg = format!("[Iridium] WebSurface error: {}", e);
            log(&msg);
            JsValue::from_str(&msg)
        })?;
    log("[Iridium] WebSurface created");

    // Create the text renderer with scaled font size for HiDPI
    log("[Iridium] Creating TextRenderer...");
    let mut text_renderer = TextRenderer::new(surface.device(), surface.queue(), surface.format())
        .map_err(|e| {
            let msg = format!("[Iridium] TextRenderer error: {}", e);
            log(&msg);
            JsValue::from_str(&msg)
        })?;

    // Scale font size for HiDPI displays
    let base_font_size = 14.0;
    let scaled_font_size = base_font_size * pixel_ratio;
    text_renderer.set_font_size(scaled_font_size);
    log(&format!(
        "[Iridium] Font size: {} (base {} * ratio {})",
        scaled_font_size, base_font_size, pixel_ratio
    ));
    log("[Iridium] TextRenderer created");

    // Create quad renderers - separate buffers to avoid GPU write conflicts
    // (multiple queue.write_buffer calls to same buffer within a frame cause corruption)
    log("[Iridium] Creating QuadRenderers...");
    let mut background_quad_renderer = QuadRenderer::new(surface.device(), surface.format());
    background_quad_renderer.update_viewport(surface.queue(), width, height);
    let mut cursor_quad_renderer = QuadRenderer::new(surface.device(), surface.format());
    cursor_quad_renderer.update_viewport(surface.queue(), width, height);
    log("[Iridium] QuadRenderers created");

    // Create editor with default config
    let editor = Editor::new(EditorConfig::default());
    let theme = Theme::dark();
    let cursor_renderer = CursorRenderer::default();
    let gutter_renderer = GutterRenderer::new();
    let highlighter = SimpleHighlighter::new();
    // The web surface has no way to declare a language yet, so folding starts
    // brace-based. `Language::C` is the stand-in for "fold on braces": without
    // the `syntax` feature every language routes to the same brace scanner, and
    // with it C's fold queries are themselves brace blocks — so the behaviour is
    // the same in both configurations. This replaced a `_Placeholder` variant
    // that existed only to be passed here; the stub now mirrors the real
    // language set, so a build without `syntax` behaves like one with it.
    let fold_state = FoldState::for_language(Language::C);

    log("[Iridium] WebEditor ready");
    Ok(WebEditor {
        editor,
        surface,
        text_renderer,
        background_quad_renderer,
        cursor_quad_renderer,
        cursor_renderer,
        gutter_renderer,
        highlighter,
        theme,
        needs_redraw: true,
        pixel_ratio,
        syntax_enabled: true,
        gutter_enabled: true,
        fold_state,
        fold_tree: SyntaxTree::new(Language::Rust).unwrap_or_else(|_| SyntaxTree::default()),
        scroll_y: 0.0,
        cached_char_width: 14.0 * 0.6, // Default until font is loaded
        ts_highlights: Vec::new(),
        use_ts_highlights: false,
        span_index: WebSpanIndex::empty(),
        viewport_config: ViewportConfig::default(),
        cached_viewport_width: width,
        cached_viewport_height: height,
        // Pre-allocated buffers to avoid per-frame allocations (120fps target)
        cpu_visible_content: String::with_capacity(10 * 1024), // 10KB typical viewport
        cpu_doc_to_visual: Vec::with_capacity(1024),           // 1K lines initial
        cpu_visible_doc_lines: Vec::with_capacity(MAX_VIEWPORT_LINES),
        cpu_line_numbers: String::with_capacity(2 * 1024), // 2KB line numbers
        cpu_gutter_quads: Vec::with_capacity(2),
        cpu_selection_quads: Vec::with_capacity(MAX_SELECTION_QUADS),
        cpu_cursor_quads: Vec::with_capacity(2),
        cpu_background_quads: Vec::with_capacity(MAX_SELECTION_QUADS + 2),
        // Cached layout info for pixel-to-position mapping with word wrap
        cached_visual_line_map: Vec::with_capacity(MAX_VIEWPORT_LINES),
        cached_map_viewport_start: 0,
        cached_content_offset_x: 0.0,
        // Cached scroll data for word-wrap-aware scrolling
        cached_total_visual_lines: 0,
        cached_cursor_abs_y: 0.0,
        cached_cursor_doc_line: 0,
        // Externalized syntax theme
        syntax_theme: HashMap::new(),
        // Git integration fields
        read_only: false,
        line_backgrounds: HashMap::new(),
        cpu_line_bg_quads: Vec::with_capacity(MAX_VIEWPORT_LINES),
        gutter_changes: HashMap::new(),
        cpu_gutter_change_quads: Vec::with_capacity(MAX_VIEWPORT_LINES),
        custom_gutter_lines: None,
        blame_data: HashMap::new(),
        cpu_blame_content: String::with_capacity(256),
        // Raw key event handling
        keyboard_handler: KeyboardHandler::new(),
        palette_mru: CommandMru::new(),
        pending_clipboard_text: None,
        pending_host_command: None,
        pending_edit: PendingEdit::None,
    })
}

#[wasm_bindgen]
impl WebEditor {
    /// Loads a font from raw TTF/OTF data.
    ///
    /// This MUST be called before any rendering on WASM, since there are no
    /// system fonts available. Pass the raw bytes of a .ttf or .otf font file.
    ///
    /// # Arguments
    ///
    /// * `data` - Raw font file bytes
    #[wasm_bindgen(js_name = loadFont)]
    pub fn load_font(&mut self, data: &[u8]) {
        log("[Iridium] Loading font...");
        self.text_renderer.load_font(data.to_vec());
        // Measure actual character width from the loaded font
        self.cached_char_width = self.text_renderer.char_width();
        log(&format!(
            "[Iridium] Font loaded, char_width: {:.2}px",
            self.cached_char_width
        ));
        self.needs_redraw = true;
    }

    /// Sets the editor content.
    #[wasm_bindgen(js_name = setContent)]
    pub fn set_content(&mut self, content: &str) {
        self.editor.set_content(content);
        // The cursor was reset outside handle_key; drop sticky columns.
        self.keyboard_handler.reset_vertical_state();
        // A full content replacement invalidates any pending incremental
        // edit; consumers do a full parse of the new content.
        self.pending_edit = PendingEdit::None;
        // Update fold regions for new content
        self.refresh_fold_regions(content);
        self.needs_redraw = true;
    }

    /// Gets the editor content.
    #[wasm_bindgen(js_name = getContent)]
    pub fn get_content(&self) -> String {
        self.editor.content()
    }

    // =========================================================================
    // Raw key event handling
    // =========================================================================

    /// Handles a raw DOM keyboard event through the Rust editing core.
    ///
    /// The host forwards `KeyboardEvent.key` plus the (already platform-mapped)
    /// modifier state; the Rust core owns all editing behavior — multi-cursor
    /// edits, Tab/indent/outdent, Enter auto-indent with bracket-block and
    /// code-fence expansion, auto-pairs, per-cursor sticky columns.
    ///
    /// Returns an action tag the host switches on:
    ///
    /// - `"handled"` — the key was consumed; no content change.
    /// - `"handled:edit"` — the key was consumed and the document changed
    ///   (fetch the edit span via `takeLastEdit`).
    /// - `"copy"` — copy requested; fetch the text via
    ///   `getPendingClipboardText` and write it to the system clipboard.
    /// - `"cut"` — cut performed (document changed); fetch the removed text
    ///   via `getPendingClipboardText`.
    /// - `"search:open"` / `"search:next"` / `"search:prev"` /
    ///   `"search:close"` — search UI actions (next/prev/close have already
    ///   been applied to the editor's search state).
    /// - `"ignored"` — the editor does not handle this key; leave the event
    ///   to the browser (do not call `preventDefault`).
    ///
    /// Undo (Ctrl+Z) and redo (Ctrl+Y, Ctrl+Shift+Z) are routed here as
    /// well: the core keyboard handler acknowledges them but documents that
    /// execution happens at the editor level, so this method executes them
    /// through the same paths as the legacy `undo()`/`redo()` methods.
    ///
    /// Read-only mode mirrors `Editor::handle_key`: only selection changes,
    /// copy, and search actions are honored; every other consumed key is a
    /// no-op that still reports `"handled"`.
    // The five bools mirror the DOM KeyboardEvent modifier flags 1:1
    // (`alt_graph` is `getModifierState("AltGraph")`); a struct would not
    // survive the wasm-bindgen boundary as ergonomically.
    #[allow(clippy::fn_params_excessive_bools)]
    #[wasm_bindgen(js_name = handleKeyEvent)]
    pub fn handle_key_event(
        &mut self,
        key: &str,
        ctrl: bool,
        shift: bool,
        alt: bool,
        meta: bool,
        alt_graph: bool,
        is_repeat: bool,
    ) -> String {
        let Some(key_code) = key_code_from_dom_key(key) else {
            // Unknown key: never reaches the Rust core.
            return "ignored".to_string();
        };

        let event = KeyEvent {
            key: key_code,
            modifiers: Modifiers {
                shift,
                ctrl,
                alt,
                meta,
                alt_graph,
            },
            is_repeat,
        };

        // `keyboard_handler` and `editor` are disjoint fields, so the
        // mutable handler borrow coexists with the immutable state borrows.
        let state = self.editor.state();
        let result = self.keyboard_handler.handle_key(
            &event,
            &state.document,
            &state.cursor,
            &state.history,
            &state.config,
        );

        self.consume_key_result(result)
    }

    /// Applies one [`KeyResult`] and returns the status string the host reads.
    ///
    /// Extracted so a keypress and a palette invocation cannot diverge. They
    /// produce the *same* `KeyResult` from the same handler, and every difference
    /// in what happens next — a clipboard stash, a search request, a host command
    /// held for collection, a redraw — would otherwise be duplicated in two places
    /// and drift the first time one of them is edited. `Editor::consume_key_result`
    /// exists in the kernel for exactly this reason.
    fn consume_key_result(&mut self, result: KeyResult) -> String {
        match result {
            KeyResult::Ignored => "ignored".to_string(),
            KeyResult::Handled => {
                // Consumed without producing a command (e.g. Escape with a
                // single collapsed cursor).
                self.cursor_renderer.reset_blink();
                self.needs_redraw = true;
                "handled".to_string()
            },
            KeyResult::Command(cmd) => self.apply_key_command(cmd),
            KeyResult::Clipboard(operation) => self.apply_clipboard_result(operation),
            KeyResult::Search(action) => self.apply_search_result(&action),
            KeyResult::History(request) => self.apply_history_result(request),
            // A binding named a command the kernel does not implement — a host
            // command. The key was consumed and the id is reported so the host can
            // run it; the caller reads `takePendingHostCommand` for the id.
            KeyResult::HostCommand { command, args } => {
                self.pending_host_command = Some(HostCommandRequest {
                    command: command.as_str().to_owned(),
                    count: args.count(),
                    captures: args.captures().to_vec(),
                });
                self.cursor_renderer.reset_blink();
                self.needs_redraw = true;
                "handled:command".to_string()
            },
        }
    }

    /// Performs one [`HistoryRequest`] and returns the status string.
    ///
    /// This face used to intercept `Ctrl+Z` and `Ctrl+Y` in `handleKeyEvent`
    /// *before* the keymap saw them, and drive `undo`/`redo` directly. Two
    /// things were broken by that, and both were reachable: running **Undo**
    /// from the command palette did nothing, because the palette runs commands
    /// and the command was a bare acknowledgement; and no keymap layer could
    /// rebind undo, because the key never reached one.
    ///
    /// `undo`/`redo` are the face's own methods, not the editor's, because
    /// this face additionally records a whole-document edit span for
    /// incremental re-highlighting — which the kernel neither knows nor should.
    fn apply_history_result(&mut self, request: HistoryRequest) -> String {
        let changed = match request {
            HistoryRequest::Undo => self.undo(),
            HistoryRequest::Redo => self.redo(),
            HistoryRequest::RedoBranch(index) => self.redo_branch_internal(index),
            // Branch selection moves nothing, so there is no document edit to
            // record — only a repaint, since a panel may be showing the choice.
            HistoryRequest::NextBranch | HistoryRequest::PreviousBranch => {
                let forward = matches!(request, HistoryRequest::NextBranch);
                self.editor.cycle_history_branch(forward);
                // Never `handled:edit`: nothing was applied. A repaint is still
                // wanted, because a panel may be showing which fork is active.
                self.needs_redraw = true;
                return "handled".to_string();
            },
        };
        if changed {
            "handled:edit".to_string()
        } else {
            "handled".to_string()
        }
    }

    // ========== Command palette ==========

    /// Every registered command as JSON, in browse order.
    ///
    /// What a palette shows before anything is typed. Key labels come from the
    /// *live* keymap, so a user layer pushed with `pushKeymap` is reflected here
    /// without the host recomputing anything.
    ///
    /// `PaletteCommand` is strings, booleans and integers throughout, so
    /// serialization cannot fail; `palette::tests` pins that by serializing the
    /// whole command set, because the empty-array fallback below would otherwise
    /// read to a host as "this editor has no commands".
    #[wasm_bindgen(js_name = listCommands)]
    #[must_use]
    pub fn list_commands(&self) -> String {
        let commands = palette::list(
            self.editor.commands(),
            self.keyboard_handler.key_hints(),
            self.read_only,
        );
        serde_json::to_string(&commands).unwrap_or_else(|_| "[]".to_string())
    }

    /// Commands matching `query` as JSON, best first; `limit` of `0` means all.
    ///
    /// Called on every keystroke in the palette input. The ranking is the
    /// kernel's, identical in every face, and the returned match offsets are
    /// UTF-16 so they can slice the strings alongside them directly.
    #[wasm_bindgen(js_name = searchCommands)]
    #[must_use]
    pub fn search_commands(&self, query: &str, limit: usize) -> String {
        let commands = palette::search(
            self.editor.commands(),
            self.keyboard_handler.key_hints(),
            &self.palette_mru,
            query,
            limit,
            self.read_only,
        );
        serde_json::to_string(&commands).unwrap_or_else(|_| "[]".to_string())
    }

    /// Runs a command by id, exactly as a key bound to it would.
    ///
    /// Returns the same status strings as `handleKeyEvent`, because it takes the
    /// same path: the same handler produces a `KeyResult`, and
    /// [`Self::consume_key_result`] applies it. That is what makes a palette
    /// invocation and a keypress indistinguishable downstream — including the
    /// sticky column, the multi-cursor addition order, and the undo grouping.
    ///
    /// A command the kernel does not implement is not a failure: the id is stashed
    /// for `takePendingHostCommand` and reported as `"handled:command"`, which is
    /// precisely what a key bound to that command does. `"ignored"` therefore means
    /// one thing only — no such command is registered.
    ///
    /// Every invocation is recorded in the palette's recency list, which is why
    /// running a command from the palette moves it toward the top next time.
    #[wasm_bindgen(js_name = runCommand)]
    pub fn run_command(&mut self, id: &str) -> String {
        if !self.editor.commands().contains(id) {
            return "ignored".to_string();
        }

        // `keyboard_handler`, `palette_mru` and `editor` are disjoint fields, so
        // the mutable handler borrow coexists with the immutable state borrows.
        let state = self.editor.state();
        let outcome = self.keyboard_handler.run_command(
            id,
            CommandArgs::NONE,
            &state.document,
            &state.cursor,
            &state.history,
            &state.config,
        );

        self.palette_mru.record(&CommandId::new(id.to_owned()));

        match outcome {
            Ok(result) => self.consume_key_result(result),
            Err(CommandRunError::Unimplemented { id }) => {
                self.pending_host_command = Some(HostCommandRequest {
                    command: id,
                    count: None,
                    captures: Vec::new(),
                });
                self.cursor_renderer.reset_blink();
                self.needs_redraw = true;
                "handled:command".to_string()
            },
        }
    }

    /// The key sequence bound to `id`, or an empty string when none is.
    ///
    /// `mac_glyphs` selects `⌘K` over `Ctrl+K`. The web face forwards macOS `Cmd`
    /// as the kernel's `ctrl`, so the two are the same binding rendered for two
    /// audiences, and the choice belongs to the host that knows the platform.
    #[wasm_bindgen(js_name = keyHintFor)]
    #[must_use]
    pub fn key_hint_for(&self, id: &str, mac_glyphs: bool) -> String {
        palette::key_hint(self.keyboard_handler.key_hints(), id, mac_glyphs).unwrap_or_default()
    }

    /// The strokes typed so far in an incomplete key sequence, rendered as text.
    ///
    /// Empty when nothing is pending. A host shows this so a chord leader
    /// (`Ctrl+K`, which consumes the keypress) does not look like an unresponsive
    /// editor; the count typed so far is included, so `2d` reads as `2d`.
    #[wasm_bindgen(js_name = pendingKeySequence)]
    #[must_use]
    pub fn pending_key_sequence(&self) -> String {
        let mut out = String::new();
        if let Some(count) = self.keyboard_handler.pending_count() {
            out.push_str(&count.to_string());
        }
        for press in self.keyboard_handler.pending_sequence() {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(
                &StrokePattern::new(press.key, ModifierPattern::exact(press.modifiers)).to_string(),
            );
        }
        out
    }

    /// Cancels any half-typed key sequence, returning `true` when one was
    /// cancelled.
    ///
    /// Call this on **blur**. Every cursor-moving host path already does it, which
    /// covers a click elsewhere in the page; a focus loss with no click does not,
    /// and a chord left pending across it would consume the first keystroke after
    /// the user came back.
    #[wasm_bindgen(js_name = abortPendingKeySequence)]
    pub fn abort_pending_key_sequence(&mut self) -> bool {
        self.keyboard_handler.abort_pending_sequence()
    }

    /// Takes the host command a binding last resolved to, if any.
    ///
    /// Take semantics: the request is cleared by this call. Returned as JSON so the
    /// command id, its count and its captured characters cross the boundary
    /// together.
    #[wasm_bindgen(js_name = takePendingHostCommand)]
    pub fn take_pending_host_command(&mut self) -> Option<String> {
        let request = self.pending_host_command.take()?;
        serde_json::to_string(&request).ok()
    }

    /// Replaces the user keymap layer from a JSON keymap, validated against the
    /// command registry.
    ///
    /// This is how bindings become swappable in the web face: the JSON is the
    /// serde shape of [`Keymap`], and the layer sits *on top* of the default so a
    /// user rebinds one key without forking the defaults. Any previously pushed
    /// user layer is replaced.
    ///
    /// Returns `None` on success and the diagnostic text on failure — an unknown
    /// command id, a binding whose bare prefix would strand a default chord, or
    /// malformed JSON — so a typo in a configuration file is a startup error rather
    /// than a key that silently does nothing.
    #[wasm_bindgen(js_name = setUserKeymap)]
    pub fn set_user_keymap(&mut self, json: &str) -> Option<String> {
        let keymap = match serde_json::from_str::<Keymap>(json) {
            Ok(keymap) => keymap,
            Err(error) => return Some(error.to_string()),
        };
        // Replace, not stack: a second call must not leave the first layer buried
        // where the user can no longer reach or remove it.
        let previous = if self.keyboard_handler.keymap().len() > 1 {
            self.keyboard_handler.pop_keymap()
        } else {
            None
        };
        match self
            .keyboard_handler
            .push_validated_keymap(keymap, self.editor.commands())
        {
            Ok(()) => None,
            Err(error) => {
                // The rejected layer changed nothing; put the working one back.
                if let Some(previous) = previous {
                    self.keyboard_handler.push_keymap(previous);
                }
                Some(error.to_string())
            },
        }
    }

    /// Drops the user keymap layer, restoring the built-in defaults.
    #[wasm_bindgen(js_name = clearUserKeymap)]
    pub fn clear_user_keymap(&mut self) {
        if self.keyboard_handler.keymap().len() > 1 {
            self.keyboard_handler.pop_keymap();
        }
    }

    /// Consumes the edit recorded by the most recent content mutation.
    ///
    /// The span is computed from the rope (byte offsets and byte-column
    /// points), never from JavaScript strings. Sequential edits between two
    /// calls are composed exactly where possible (e.g. consecutive typing);
    /// when exact composition is impossible the method returns `None` and
    /// the consumer must fall back to a full reparse. Take semantics: the
    /// pending edit is cleared by this call.
    #[wasm_bindgen(js_name = takeLastEdit)]
    pub fn take_last_edit(&mut self) -> Option<JsEditInfo> {
        match std::mem::take(&mut self.pending_edit) {
            PendingEdit::None | PendingEdit::Degraded => None,
            PendingEdit::Exact(pending) => {
                let span = pending.span;
                let document = &self.editor.state().document;
                // Bytes before the start are untouched, so the start point is
                // identical in the pre- and post-edit documents; the new end
                // point was captured against the current document when the
                // span was recorded (no edit has happened since, or it would
                // have been recomposed).
                let (start_row, start_column) = byte_point(document, span.start_byte)?;
                Some(JsEditInfo {
                    start_byte: span.start_byte,
                    old_end_byte: span.old_end_byte,
                    new_end_byte: span.new_end_byte,
                    start_row,
                    start_column,
                    old_end_row: span.old_end_row,
                    old_end_column: span.old_end_column,
                    new_end_row: pending.new_end_row,
                    new_end_column: pending.new_end_column,
                })
            },
        }
    }

    /// Applies a positional edit sequenced by the document authority (a
    /// remote edit), bypassing the undo tree.
    ///
    /// This is the application path for `edit-applied` deltas: the range
    /// `[startByte, oldEndByte)` is replaced with `text` exactly as the
    /// authority sequenced it. It deliberately works in read-only mode —
    /// followers are read-only and still apply remote deltas; the
    /// read-only gate protects against *local* mutation sources only.
    ///
    /// Behavior:
    ///
    /// - The edit never enters the undo tree, so it can never be locally
    ///   undone. The existing history is reset as well: its recorded
    ///   commands carry coordinates valid only in the pre-edit document
    ///   lineage, and replaying one after the rebase would apply an
    ///   inverse at shifted positions (the edit tracker's exact-or-refuse
    ///   discipline, applied to history).
    /// - Every local cursor and selection (multi-cursor aware) is
    ///   transformed across the change: positions before the edit stay,
    ///   positions at or after the old end shift by the byte delta (an
    ///   insertion at a cursor's exact position keeps the cursor glued to
    ///   the text that followed it), positions inside the replaced range
    ///   collapse to the edit start, and cursors landing on shared ground
    ///   merge.
    /// - The edit composes into the pending tracking for `takeLastEdit`
    ///   through the same exact-composition math as local edits; a
    ///   Degraded pending state stays Degraded (full reparse), never a
    ///   guessed span.
    ///
    /// Errors when the range is inverted, out of bounds, or not on UTF-8
    /// character boundaries; the document, history, and cursors are
    /// untouched on error.
    #[wasm_bindgen(js_name = applyRemoteEdit)]
    pub fn apply_remote_edit(
        &mut self,
        start_byte: usize,
        old_end_byte: usize,
        text: &str,
    ) -> Result<(), JsValue> {
        let span =
            crate::remote_edit::apply_remote_edit(&mut self.editor, start_byte, old_end_byte, text)
                .map_err(|error| JsValue::from_str(&error.to_string()))?;
        let Some(span) = span else {
            // Validated no-op (empty range, empty text): nothing changed.
            return Ok(());
        };
        // The cursor may have moved outside handle_key; drop the keyboard
        // handler's transient state like every other host-driven jump.
        self.keyboard_handler.reset_vertical_state();
        self.keyboard_handler.invalidate_cursor_order();
        self.record_edit(span);
        let content = self.editor.content();
        self.refresh_fold_regions(&content);
        self.needs_redraw = true;
        Ok(())
    }

    /// Returns the document text between two byte offsets.
    ///
    /// This is the ranged companion to `takeLastEdit`: the edit info
    /// carries byte offsets only, and a consumer that needs the bytes of
    /// the new span (e.g. an outbound edit-proposal builder) calls
    /// `getTextRange(info.startByte, info.newEndByte)` synchronously after
    /// the take, before any further edit can move the offsets. Only the
    /// requested range is materialized across the wasm boundary — never
    /// the whole document — so the per-keystroke cost is proportional to
    /// the edit, not the file.
    ///
    /// Errors when the range is inverted (`startByte > endByte`), out of
    /// bounds, or either offset does not fall on a UTF-8 character
    /// boundary.
    #[wasm_bindgen(js_name = getTextRange)]
    pub fn get_text_range(&self, start_byte: usize, end_byte: usize) -> Result<String, JsValue> {
        text_range(&self.editor.state().document, start_byte, end_byte)
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }

    /// Consumes the clipboard text stashed by the last `"copy"`/`"cut"` key
    /// result. Take semantics: returns `None` until the next clipboard key.
    // wasm-bindgen cannot export `const fn`.
    #[allow(clippy::missing_const_for_fn)]
    #[wasm_bindgen(js_name = getPendingClipboardText)]
    pub fn get_pending_clipboard_text(&mut self) -> Option<String> {
        self.pending_clipboard_text.take()
    }

    /// Returns the clipboard text for a host-driven copy (the native `copy`
    /// event), without mutating the document.
    ///
    /// Drives the exact `KeyboardHandler` copy path that `handleKeyEvent`
    /// uses (the canonical copy chord is synthesized so both entry points
    /// share one implementation): selections from every cursor are joined
    /// with the document line ending, and with only collapsed cursors each
    /// cursor's whole line is copied. Copy is honored in read-only mode,
    /// matching the keyboard path. Returns `None` only if the core no
    /// longer maps the chord to a copy operation.
    #[wasm_bindgen(js_name = copyText)]
    pub fn copy_text(&mut self) -> Option<String> {
        let event = KeyEvent {
            key: KeyCode::Char('c'),
            modifiers: Modifiers::ctrl(),
            is_repeat: false,
        };
        let state = self.editor.state();
        let result = self.keyboard_handler.handle_key(
            &event,
            &state.document,
            &state.cursor,
            &state.history,
            &state.config,
        );
        match result {
            KeyResult::Clipboard(ClipboardOperation::Copy(text)) => Some(text),
            _ => None,
        }
    }

    /// Performs a host-driven cut (the native `cut` event): returns the
    /// clipboard text and applies the multi-cursor cut command through the
    /// same path as `handleKeyEvent` (edit tracking for `takeLastEdit`,
    /// history, fold refresh).
    ///
    /// Drives the exact `KeyboardHandler` cut path (the canonical cut chord
    /// is synthesized so both entry points share one implementation).
    /// Returns `None` in read-only mode: the cut is swallowed entirely —
    /// not even the copy half happens — matching the keyboard path's
    /// `apply_clipboard_result`. When nothing can be removed (e.g. an empty
    /// document) the clipboard text is still returned with no document
    /// change, exactly like the keyboard cut path.
    #[wasm_bindgen(js_name = cutText)]
    pub fn cut_text(&mut self) -> Option<String> {
        if self.read_only {
            return None;
        }
        let event = KeyEvent {
            key: KeyCode::Char('x'),
            modifiers: Modifiers::ctrl(),
            is_repeat: false,
        };
        let state = self.editor.state();
        let result = self.keyboard_handler.handle_key(
            &event,
            &state.document,
            &state.cursor,
            &state.history,
            &state.config,
        );
        match result {
            KeyResult::Clipboard(ClipboardOperation::Cut { text, command }) => {
                if command.modifies_content() {
                    self.track_and_apply(command);
                    self.ensure_cursor_visible();
                }
                Some(text)
            },
            _ => None,
        }
    }

    /// Applies a command produced by the keyboard handler, mirroring
    /// `Editor::handle_key` (including its read-only gating).
    fn apply_key_command(&mut self, command: Command) -> String {
        if self.read_only {
            // Read-only: only selection changes apply (mirrors
            // `Editor::handle_key`); the key is still consumed.
            if matches!(command, Command::SetSelection { .. }) {
                self.editor.apply_command(command);
                self.cursor_renderer.reset_blink();
                self.needs_redraw = true;
                self.ensure_cursor_visible();
            }
            return "handled".to_string();
        }

        let edited = self.track_and_apply(command);
        self.ensure_cursor_visible();
        if edited { "handled:edit" } else { "handled" }.to_string()
    }

    /// Handles a clipboard result from the keyboard handler.
    ///
    /// `Editor::handle_key` returns cut results to the host *without*
    /// applying the deletion, so the binding applies the cut command through
    /// the normal `apply_command` path (with edit tracking and history).
    fn apply_clipboard_result(&mut self, operation: ClipboardOperation) -> String {
        match operation {
            ClipboardOperation::Copy(text) => {
                self.pending_clipboard_text = Some(text);
                "copy".to_string()
            },
            ClipboardOperation::Cut { text, command } => {
                if self.read_only {
                    // Mirrors `Editor::handle_key`: cut is swallowed entirely
                    // in read-only mode (not even the copy half happens).
                    return "handled".to_string();
                }
                self.pending_clipboard_text = Some(text);
                if command.modifies_content() {
                    self.track_and_apply(command);
                    self.ensure_cursor_visible();
                    "cut".to_string()
                } else {
                    // Nothing to remove (e.g. empty document): the clipboard
                    // is still updated, exactly like copy.
                    "copy".to_string()
                }
            },
            // A paste key reaching the core (the host normally lets the
            // browser fire the native paste event instead): report ignored
            // so the native `paste` event still fires and feeds `insert()`.
            ClipboardOperation::Paste => "ignored".to_string(),
        }
    }

    /// Handles a search action from the keyboard handler, mirroring
    /// `Editor::handle_search_action`.
    fn apply_search_result(&mut self, action: &SearchAction) -> String {
        match action {
            SearchAction::OpenSearch => "search:open".to_string(),
            SearchAction::NextMatch => {
                // goto_next_match moves the cursor outside handle_key.
                self.keyboard_handler.reset_vertical_state();
                self.editor.goto_next_match();
                self.cursor_renderer.reset_blink();
                self.needs_redraw = true;
                self.ensure_cursor_visible();
                "search:next".to_string()
            },
            SearchAction::PreviousMatch => {
                self.keyboard_handler.reset_vertical_state();
                self.editor.goto_previous_match();
                self.cursor_renderer.reset_blink();
                self.needs_redraw = true;
                self.ensure_cursor_visible();
                "search:prev".to_string()
            },
            SearchAction::CloseSearch => {
                self.editor.close_search();
                self.needs_redraw = true;
                "search:close".to_string()
            },
        }
    }

    /// Records the edit span of a command against the pre-edit document,
    /// applies it through `Editor::apply_command`, and refreshes fold
    /// regions. Returns whether the command modifies content.
    ///
    /// The span is computed before application (all command coordinates are
    /// valid in the pre-edit document) and recorded afterwards. Recording
    /// assumes application succeeds; a failing command is an editor-core
    /// invariant violation that `apply_command` already reports through an
    /// error event.
    fn track_and_apply(&mut self, command: Command) -> bool {
        let modifies_content = command.modifies_content();
        let span = if modifies_content {
            compute_edit_span(&self.editor.state().document, &command)
        } else {
            Ok(None)
        };

        self.editor.apply_command(command);

        if modifies_content {
            match span {
                Ok(Some(span)) => self.record_edit(span),
                // Never report a wrong span: unresolvable positions degrade
                // the pending edit to a full reparse.
                Ok(None) | Err(EditSpanError) => self.pending_edit = PendingEdit::Degraded,
            }
            let content = self.editor.content();
            self.refresh_fold_regions(&content);
        }

        self.cursor_renderer.reset_blink();
        self.needs_redraw = true;
        modifies_content
    }

    /// Merges a new edit span into the pending edit (see
    /// [`compose_pending`] for the composition rules). Must be called after
    /// the edit has been applied: composition captures the new-end point
    /// against the post-edit document.
    ///
    /// This is also the single choke point every content mutation funnels
    /// through — local keystrokes and paste (`track_and_apply`), cut,
    /// `applyRemoteEdit`, and undo/redo (`record_whole_document_edit`) all
    /// call it exactly once per mutation — so it doubles as the one place
    /// tracked highlight spans get carried forward across the edit
    /// ([`Self::shift_highlight_spans`]; see that method for the flicker it
    /// closes).
    fn record_edit(&mut self, span: EditSpan) {
        let pending = std::mem::take(&mut self.pending_edit);
        self.pending_edit = compose_pending(&self.editor.state().document, pending, span);
        self.shift_highlight_spans(span);
    }

    /// Repositions every stored tree-sitter highlight span across `span`,
    /// dropping any span the edit itself overlaps.
    ///
    /// Without this, a span applied by `setTreeSitterHighlights` stayed at
    /// its ABSOLUTE byte offsets forever, until the next
    /// `setTreeSitterHighlights`/`clearTreeSitterHighlights` call. Every
    /// edit between an edit's synchronous render and the highlight worker's
    /// asynchronous replacement spans landing (a few milliseconds later)
    /// rendered the CURRENT document sliced at STALE, pre-edit offsets —
    /// coloring the wrong characters for one or more frames (the reported
    /// flicker: text flashes to misaligned colors, then "recolors" once the
    /// worker's fresh spans arrive). [`crate::highlight_span_shift::shift_span`]
    /// carries the pure per-span math; this method applies it to every
    /// tracked span and rebuilds the query index in one pass, so the very
    /// next render — even before the worker responds — paints either the
    /// correct color at the correct position or nothing at all for the
    /// handful of characters the edit itself touched. Never a wrong one.
    fn shift_highlight_spans(&mut self, span: EditSpan) {
        if self.ts_highlights.is_empty() {
            return;
        }
        self.ts_highlights.retain_mut(|highlight| {
            match crate::highlight_span_shift::shift_span(
                highlight.start,
                highlight.end,
                span.start_byte,
                span.old_end_byte,
                span.new_end_byte,
            ) {
                Some((start, end)) => {
                    highlight.start = start;
                    highlight.end = end;
                    true
                },
                None => false,
            }
        });
        let web_spans = self
            .ts_highlights
            .iter()
            .map(|highlight| WebSpan {
                start: highlight.start,
                end: highlight.end,
                highlight_type: highlight.highlight_type.clone(),
            })
            .collect();
        self.span_index = WebSpanIndex::new(web_spans);
    }

    /// Records a conservative whole-document edit (used by undo/redo, where
    /// the replayed command's coordinates refer to a document state the
    /// binding never observed).
    ///
    /// `pre_len`, `pre_end_row`, and `pre_end_column` describe the end of
    /// the document *before* the operation; the new end is read from the
    /// current document. An unconsumed pending edit is composed exactly:
    /// the whole-document span's replaced range extends past the pending
    /// new text, which [`compose_pending`] back-maps to base coordinates.
    fn record_whole_document_edit(
        &mut self,
        pre_len: usize,
        pre_end_row: usize,
        pre_end_column: usize,
    ) {
        let document = &self.editor.state().document;
        let span = EditSpan {
            start_byte: 0,
            old_end_byte: pre_len,
            new_end_byte: document.byte_count(),
            old_end_row: pre_end_row,
            old_end_column: pre_end_column,
        };
        self.record_edit(span);
    }

    // =========================================================================
    // Read-only mode
    // =========================================================================

    /// Sets the editor to read-only mode.
    /// When read-only, all content-mutating operations are silently ignored.
    // wasm-bindgen cannot export `const fn`.
    #[allow(clippy::missing_const_for_fn)]
    #[wasm_bindgen(js_name = setReadOnly)]
    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
        // Keep the core editor's flag in sync so its own gates
        // (`Editor::handle_key`, `Editor::paste`, replace operations)
        // agree with the binding-level gates.
        self.editor.state_mut().read_only = read_only;
        self.needs_redraw = true;
    }

    /// Returns whether the editor is in read-only mode.
    #[wasm_bindgen(js_name = isReadOnly)]
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    // =========================================================================
    // Line backgrounds (diff highlighting)
    // =========================================================================

    /// Sets per-line background colors for diff highlighting.
    /// Accepts a JS array of `{line: number, color: string}` objects.
    /// Line numbers are 0-indexed document lines. Colors are hex strings.
    #[wasm_bindgen(js_name = setLineBackgrounds)]
    pub fn set_line_backgrounds(&mut self, backgrounds: &JsValue) -> Result<(), JsValue> {
        use iridium_editor::theme::Color;
        use js_sys::{Array, Reflect};

        self.line_backgrounds.clear();

        let arr = Array::from(backgrounds);
        for i in 0..arr.length() {
            let entry = arr.get(i);
            let line = Reflect::get(&entry, &JsValue::from_str("line"))?
                .as_f64()
                .ok_or_else(|| JsValue::from_str("line must be a number"))?
                as usize;
            let color_hex = Reflect::get(&entry, &JsValue::from_str("color"))?
                .as_string()
                .ok_or_else(|| JsValue::from_str("color must be a hex string"))?;
            let color = Color::from_hex(&color_hex)
                .ok_or_else(|| JsValue::from_str(&format!("invalid hex color: {}", color_hex)))?;
            self.line_backgrounds.insert(line, color);
        }

        self.needs_redraw = true;
        Ok(())
    }

    /// Clears all per-line background colors.
    #[wasm_bindgen(js_name = clearLineBackgrounds)]
    pub fn clear_line_backgrounds(&mut self) {
        self.line_backgrounds.clear();
        self.needs_redraw = true;
    }

    // =========================================================================
    // Gutter change indicators
    // =========================================================================

    /// Sets gutter change indicators (thin colored bars in the gutter).
    /// Accepts a JS array of `{line: number, kind: string}` objects.
    /// Kind must be "added", "modified", "deleted", "error", "warning", "info", or "hint".
    /// Line numbers are 0-indexed.
    #[wasm_bindgen(js_name = setGutterChanges)]
    pub fn set_gutter_changes(&mut self, changes: &JsValue) -> Result<(), JsValue> {
        use js_sys::{Array, Reflect};

        self.gutter_changes.clear();

        let arr = Array::from(changes);
        for i in 0..arr.length() {
            let entry = arr.get(i);
            let line = Reflect::get(&entry, &JsValue::from_str("line"))?
                .as_f64()
                .ok_or_else(|| JsValue::from_str("line must be a number"))?
                as usize;
            let kind = Reflect::get(&entry, &JsValue::from_str("kind"))?
                .as_string()
                .ok_or_else(|| JsValue::from_str("kind must be a string"))?;
            let color = match kind.as_str() {
                "added" => self.theme.editor.change_added,
                "modified" => self.theme.editor.change_modified,
                "deleted" => self.theme.editor.change_deleted,
                "error" => self.theme.editor.diagnostic_error,
                "warning" => self.theme.editor.diagnostic_warning,
                "info" => self.theme.editor.diagnostic_info,
                "hint" => self.theme.editor.diagnostic_hint,
                _ => {
                    return Err(JsValue::from_str(&format!(
                        "unknown change kind: {} (expected added/modified/deleted/error/warning/info/hint)",
                        kind
                    )));
                },
            };
            self.gutter_changes.insert(line, color);
        }

        self.needs_redraw = true;
        Ok(())
    }

    /// Clears all gutter change indicators.
    #[wasm_bindgen(js_name = clearGutterChanges)]
    pub fn clear_gutter_changes(&mut self) {
        self.gutter_changes.clear();
        self.needs_redraw = true;
    }

    // =========================================================================
    // Custom gutter text
    // =========================================================================

    /// Sets custom gutter text, replacing automatic line numbers.
    /// Accepts a JS array of strings (one per document line) or null to restore auto numbering.
    /// Use this for diff views to show dual line numbers + markers (e.g., "  5 | + ").
    #[wasm_bindgen(js_name = setCustomGutterText)]
    pub fn set_custom_gutter_text(&mut self, lines: &JsValue) -> Result<(), JsValue> {
        use js_sys::Array;

        if lines.is_null() || lines.is_undefined() {
            self.custom_gutter_lines = None;
        } else {
            let arr = Array::from(lines);
            let mut result = Vec::with_capacity(arr.length() as usize);
            for i in 0..arr.length() {
                let val = arr.get(i);
                let s = val
                    .as_string()
                    .ok_or_else(|| JsValue::from_str("gutter text entry must be a string"))?;
                result.push(s);
            }
            self.custom_gutter_lines = Some(result);
        }

        self.needs_redraw = true;
        Ok(())
    }

    // =========================================================================
    // Inline blame
    // =========================================================================

    /// Sets per-line blame data for inline blame ghost text.
    /// Accepts a JS array of `{line: number, text: string}` objects.
    /// Line numbers are 0-indexed. Text is pre-formatted (e.g., "Author · 3d ago · Summary").
    /// Only the blame for the current cursor line is rendered (as ghost text after the line content).
    #[wasm_bindgen(js_name = setBlameData)]
    pub fn set_blame_data(&mut self, data: &JsValue) -> Result<(), JsValue> {
        use js_sys::{Array, Reflect};

        self.blame_data.clear();

        let arr = Array::from(data);
        for i in 0..arr.length() {
            let entry = arr.get(i);
            let line = Reflect::get(&entry, &JsValue::from_str("line"))?
                .as_f64()
                .ok_or_else(|| JsValue::from_str("line must be a number"))?
                as usize;
            let text = Reflect::get(&entry, &JsValue::from_str("text"))?
                .as_string()
                .ok_or_else(|| JsValue::from_str("text must be a string"))?;
            self.blame_data.insert(line, text);
        }

        self.needs_redraw = true;
        Ok(())
    }

    /// Clears all blame data.
    #[wasm_bindgen(js_name = clearBlameData)]
    pub fn clear_blame_data(&mut self) {
        self.blame_data.clear();
        self.needs_redraw = true;
    }

    /// Inserts text at every cursor, replacing any selections.
    /// No-op in read-only mode.
    ///
    /// The insertion is one reversible command built by the core's paste
    /// path, so selection replacement, multi-cursor accounting, and undo all
    /// behave exactly like keyboard input, and the edit span is recorded for
    /// `takeLastEdit`.
    pub fn insert(&mut self, text: &str) {
        if self.read_only || text.is_empty() {
            return;
        }
        // Paste-style insertion moves the cursor outside handle_key.
        self.keyboard_handler.reset_vertical_state();
        let state = self.editor.state();
        let result = self
            .keyboard_handler
            .handle_paste(text, &state.document, &state.cursor);
        if let KeyResult::Command(cmd) = result {
            self.track_and_apply(cmd);
        }
    }

    /// Deletes the current selection and returns true, or returns false if no selection.
    fn delete_selection(&mut self) -> bool {
        let selection = self.editor.state().cursor.primary.clone();
        if selection.is_collapsed() {
            return false;
        }

        let start = selection.start();
        let end = selection.end();
        let range = Range::new(start, end);

        let cmd = Command::Delete {
            range,
            deleted_text: self.editor.state().document.slice(range),
        };
        // track_and_apply records the edit span for takeLastEdit and
        // refreshes fold regions.
        self.track_and_apply(cmd);

        // Collapse selection to start position
        self.set_selection_internal(start, start);
        true
    }

    /// Deletes the character before the cursor (backspace).
    /// If there's a selection, deletes the selected text instead.
    pub fn backspace(&mut self) {
        if self.read_only {
            return;
        }
        // If there's a selection, delete it
        if self.delete_selection() {
            return;
        }

        let cursor = self.editor.cursor();
        if cursor.line > 0 || cursor.column > 0 {
            // Calculate the position to delete from
            let delete_from = if cursor.column > 0 {
                Position::new(cursor.line, cursor.column - 1)
            } else {
                // At start of line, delete the newline from previous line
                let prev_line_len = self
                    .editor
                    .state()
                    .document
                    .line(cursor.line - 1)
                    .map(|l| l.chars().count())
                    .unwrap_or(0);
                Position::new(cursor.line - 1, prev_line_len)
            };

            let cmd = Command::Delete {
                range: Range::new(delete_from, cursor),
                deleted_text: self
                    .editor
                    .state()
                    .document
                    .slice(Range::new(delete_from, cursor)),
            };
            // track_and_apply records the edit span and refreshes folds.
            self.track_and_apply(cmd);
            // Move cursor to start of deleted range
            self.set_selection_internal(delete_from, delete_from);
        }
    }

    /// Deletes the character after the cursor (delete).
    /// If there's a selection, deletes the selected text instead.
    /// No-op in read-only mode.
    pub fn delete_forward(&mut self) {
        if self.read_only {
            return;
        }
        // If there's a selection, delete it
        if self.delete_selection() {
            return;
        }

        let cursor = self.editor.cursor();
        let line_count = self.editor.state().document.line_count();
        let current_line_len = self
            .editor
            .state()
            .document
            .line(cursor.line)
            .map(|l| l.chars().count())
            .unwrap_or(0);

        // Check if we can delete forward
        if cursor.column < current_line_len || cursor.line < line_count.saturating_sub(1) {
            let delete_to = if cursor.column < current_line_len {
                Position::new(cursor.line, cursor.column + 1)
            } else {
                // At end of line, delete the newline
                Position::new(cursor.line + 1, 0)
            };

            let cmd = Command::Delete {
                range: Range::new(cursor, delete_to),
                deleted_text: self
                    .editor
                    .state()
                    .document
                    .slice(Range::new(cursor, delete_to)),
            };
            // Cursor stays in place for forward delete; track_and_apply
            // records the edit span, refreshes folds, and resets blink.
            self.keyboard_handler.reset_vertical_state();
            self.track_and_apply(cmd);
        }
    }

    /// Resizes the editor viewport.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.surface.resize(width, height);
        self.text_renderer
            .update_viewport(self.surface.queue(), width, height);
        self.background_quad_renderer
            .update_viewport(self.surface.queue(), width, height);
        self.cursor_quad_renderer
            .update_viewport(self.surface.queue(), width, height);
        // Update cache to prevent redundant updates in render_frame
        self.cached_viewport_width = width;
        self.cached_viewport_height = height;
        self.needs_redraw = true;
    }

    /// Gets the current vertical scroll offset.
    #[wasm_bindgen(js_name = getScrollY)]
    pub fn get_scroll_y(&self) -> f32 {
        self.scroll_y
    }

    /// Sets the vertical scroll offset.
    #[wasm_bindgen(js_name = setScrollY)]
    pub fn set_scroll_y(&mut self, y: f32) {
        let max_scroll = self.max_scroll_y();
        self.scroll_y = y.clamp(0.0, max_scroll);
        self.needs_redraw = true;
    }

    /// Scrolls by a delta amount.
    #[wasm_bindgen(js_name = scrollBy)]
    pub fn scroll_by(&mut self, delta_y: f32) {
        self.set_scroll_y(self.scroll_y + delta_y);
    }

    /// Returns the maximum scroll offset.
    ///
    /// Uses `cached_total_visual_lines` from the last render_frame to account
    /// for word-wrapped lines that occupy multiple visual rows.
    #[wasm_bindgen(js_name = getMaxScrollY)]
    pub fn max_scroll_y(&self) -> f32 {
        let line_height = self.text_renderer.line_height();
        let total_visual = if self.cached_total_visual_lines > 0 {
            self.cached_total_visual_lines
        } else {
            // Before first render, fall back to fold-aware count (no wrapping info)
            self.fold_state
                .visible_line_count(self.editor.state().document.line_count())
        };
        let content_height = total_visual as f32 * line_height;
        let viewport_height = self.surface.height() as f32;
        (content_height - viewport_height + 20.0).max(0.0) // 20px padding
    }

    /// Ensures the cursor is visible by scrolling if needed.
    ///
    /// Uses the cached cursor absolute Y from the last render_frame when the
    /// cursor line hasn't changed (common case: typing on the same line).
    /// Falls back to fold-only estimation when the cursor has moved to a new line.
    #[wasm_bindgen(js_name = ensureCursorVisible)]
    pub fn ensure_cursor_visible(&mut self) {
        let line_height = self.text_renderer.line_height();
        let padding = 10.0;
        let viewport_height = self.surface.height() as f32;

        let cursor_line = self.editor.cursor().line;
        let cursor_y =
            if cursor_line == self.cached_cursor_doc_line && self.cached_cursor_abs_y > 0.0 {
                // Cursor on the same line as last render — use cached position
                // (accounts for wrapping within this line)
                self.cached_cursor_abs_y
            } else {
                // Cursor moved to a different line — approximate using fold mapping
                let visual_line = self
                    .fold_state
                    .document_to_visual_line(cursor_line)
                    .unwrap_or(0);
                padding + (visual_line as f32 * line_height)
            };

        // Scroll up if cursor is above viewport
        if cursor_y < self.scroll_y + padding {
            self.scroll_y = (cursor_y - padding).max(0.0);
            self.needs_redraw = true;
        }
        // Scroll down if cursor is below viewport
        else if cursor_y + line_height > self.scroll_y + viewport_height - padding {
            self.scroll_y = cursor_y + line_height - viewport_height + padding;
            self.needs_redraw = true;
        }
    }

    /// Converts highlight spans (from WebSpanIndex query) to colored text spans for rendering.
    ///
    /// This is the optimized version that works with WebSpan from the WebSpanIndex.
    /// The spans are collected, sorted, and processed only for the visible viewport.
    ///
    /// # Arguments
    ///
    /// * `content` - The visible content string
    /// * `spans` - Iterator of WebSpan from WebSpanIndex::query()
    /// * `content_start_byte` - The document byte offset where content begins
    fn build_rich_spans_viewport<'a>(
        &self,
        content: &'a str,
        spans: impl Iterator<Item = WebSpan>,
        content_start_byte: usize,
    ) -> Vec<(&'a str, iridium_editor::theme::Color)> {
        let foreground = self.theme.editor.foreground;
        let mut result = Vec::new();
        let mut last_end = 0;
        let content_end_byte = content_start_byte + content.len();

        // Collect and sort spans by start position
        let mut sorted_spans: Vec<WebSpan> = spans.collect();
        sorted_spans.sort_by_key(|s| s.start);

        for span in &sorted_spans {
            // Calculate span positions relative to content
            let span_start = span.start.saturating_sub(content_start_byte);
            let span_end = span.end.saturating_sub(content_start_byte);

            // Skip spans that are completely outside content
            if span.end <= content_start_byte || span.start >= content_end_byte {
                continue;
            }

            // Clamp to content bounds
            let span_start = span_start.min(content.len());
            let span_end = span_end.min(content.len());

            // Skip empty or invalid spans
            if span_start >= span_end {
                continue;
            }

            // Add gap before this span if needed
            if span_start > last_end {
                let gap_text = &content[last_end..span_start];
                if !gap_text.is_empty() {
                    result.push((gap_text, foreground));
                }
            }

            // Skip overlapping spans
            if span_start < last_end {
                continue;
            }

            // Get the color for this highlight type (using string-based lookup)
            let color = self.color_for_highlight_type(&span.highlight_type);

            // Add the highlighted span
            let text = &content[span_start..span_end];
            if !text.is_empty() {
                result.push((text, color));
            }

            last_end = span_end;
        }

        // Add remaining text after last span
        if last_end < content.len() {
            let remaining = &content[last_end..];
            if !remaining.is_empty() {
                result.push((remaining, foreground));
            }
        }

        result
    }

    /// Converts tree-sitter highlight spans to colored text spans for rendering.
    /// (Legacy version - kept for fallback when SpanIndex is empty)
    ///
    /// Maps tree-sitter capture names to theme colors and handles gaps between
    /// highlighted regions with default foreground color.
    fn build_rich_spans_from_ts<'a>(
        &self,
        content: &'a str,
        mut sorted_spans: Vec<JsHighlightSpan>,
    ) -> Vec<(&'a str, iridium_editor::theme::Color)> {
        let foreground = self.theme.editor.foreground;
        let mut result = Vec::new();
        let mut last_end = 0;

        // Sort spans by start position
        sorted_spans.sort_by_key(|s| s.start);

        for span in &sorted_spans {
            // Skip spans that are out of bounds
            if span.start >= content.len() || span.end > content.len() {
                continue;
            }

            // Add gap before this span if needed
            if span.start > last_end {
                let gap_text = &content[last_end..span.start];
                if !gap_text.is_empty() {
                    result.push((gap_text, foreground));
                }
            }

            // Skip overlapping spans
            if span.start < last_end {
                continue;
            }

            // Get the color for this highlight type
            let color = self.color_for_highlight_type(&span.highlight_type);

            // Add the highlighted span
            let text = &content[span.start..span.end];
            if !text.is_empty() {
                result.push((text, color));
            }

            last_end = span.end;
        }

        // Add remaining text after last span
        if last_end < content.len() {
            let remaining = &content[last_end..];
            if !remaining.is_empty() {
                result.push((remaining, foreground));
            }
        }

        result
    }

    /// Maps a tree-sitter highlight type to a theme color.
    ///
    /// Looks up the `syntax_theme` HashMap with hierarchical resolution:
    /// for a capture name like `punctuation.list_marker.markup`, tries the full name
    /// first, then walks up the dot-separated hierarchy (`punctuation.list_marker`,
    /// then `punctuation`) until a match is found. Falls back to editor foreground.
    ///
    /// Colors are provided by the host application via `setSyntaxTheme()`,
    /// making the editor fully theme-agnostic.
    fn color_for_highlight_type(&self, highlight_type: &str) -> iridium_editor::theme::Color {
        let mut name = highlight_type;
        loop {
            if let Some(color) = self.syntax_theme.get(name) {
                return *color;
            }
            match name.rsplit_once('.') {
                Some((parent, _)) => name = parent,
                None => return self.theme.editor.foreground,
            }
        }
    }

    /// Renders the editor to the canvas.
    ///
    /// Returns true if something was rendered, false if no redraw was needed.
    pub fn render(&mut self) -> Result<bool, JsValue> {
        if !self.needs_redraw {
            return Ok(false);
        }

        self.render_frame()?;
        self.needs_redraw = false;
        Ok(true)
    }

    /// Forces a render regardless of redraw state.
    #[wasm_bindgen(js_name = forceRender)]
    pub fn force_render(&mut self) -> Result<(), JsValue> {
        self.render_frame()
    }

    /// Internal render implementation.
    #[allow(clippy::cast_precision_loss)]
    fn render_frame(&mut self) -> Result<(), JsValue> {
        use glyphon::{TextArea, TextBounds};
        use web_time::Instant;
        use wgpu::{
            Color, LoadOp, Operations, RenderPassColorAttachment, RenderPassDescriptor, StoreOp,
        };

        // T042: Track frame time for performance monitoring
        let frame_start = Instant::now();

        // PERF: Only update viewport uniforms when dimensions actually change
        // Avoids 3 GPU buffer writes per frame when dimensions are unchanged
        let current_width = self.surface.width();
        let current_height = self.surface.height();
        if current_width != self.cached_viewport_width
            || current_height != self.cached_viewport_height
        {
            self.cached_viewport_width = current_width;
            self.cached_viewport_height = current_height;
            self.text_renderer
                .update_viewport(self.surface.queue(), current_width, current_height);
            self.background_quad_renderer.update_viewport(
                self.surface.queue(),
                current_width,
                current_height,
            );
            self.cursor_quad_renderer.update_viewport(
                self.surface.queue(),
                current_width,
                current_height,
            );
        }

        // Get the content to render, handling folded lines
        let doc = &self.editor.state().document;
        let line_count = doc.line_count();

        // Calculate visible line range for viewport virtualization
        let line_height = self.text_renderer.line_height();
        let surface_height = self.surface.height() as f32;
        let first_visible_line = (self.scroll_y / line_height).floor() as usize;
        let visible_line_count = (surface_height / line_height).ceil() as usize + 2;
        let overscan = 10; // Extra lines above/below for smooth scrolling
        let viewport_start = first_visible_line.saturating_sub(overscan);
        let viewport_end = (first_visible_line + visible_line_count + overscan).min(line_count);

        // Calculate the byte offset where visible_content starts in the full document
        // This is needed to correctly map span byte offsets to visible_content positions
        let viewport_start_byte = doc.line_to_byte_offset(viewport_start).unwrap_or(0);

        // Build visible content, only including lines in viewport
        // Line numbers are built AFTER shaping to account for line wrapping
        // PERF: Reuse pre-allocated buffers to avoid per-frame allocations
        self.cpu_visible_content.clear();
        self.cpu_visible_doc_lines.clear();

        // Ensure doc_to_visual has capacity for all lines (grows if needed, never shrinks)
        if self.cpu_doc_to_visual.capacity() < line_count {
            self.cpu_doc_to_visual
                .reserve(line_count - self.cpu_doc_to_visual.capacity());
        }
        self.cpu_doc_to_visual.clear();

        let mut visual_line = 0;

        // Pre-fill doc_to_visual for lines before viewport
        for doc_line in 0..viewport_start {
            if !self.fold_state.is_line_hidden(doc_line) {
                self.cpu_doc_to_visual.push(Some(visual_line));
                visual_line += 1;
            } else {
                self.cpu_doc_to_visual.push(None);
            }
        }

        // Only process lines in viewport range
        for doc_line in viewport_start..viewport_end {
            if self.fold_state.is_line_hidden(doc_line) {
                self.cpu_doc_to_visual.push(None);
                continue;
            }

            // Add newline separator (except for first visible line in our buffer)
            if !self.cpu_visible_doc_lines.is_empty() {
                self.cpu_visible_content.push('\n');
            }

            // Add line content
            if let Some(line_text) = doc.line(doc_line) {
                self.cpu_visible_content.push_str(&line_text);
            }

            // If this line is folded, also append the closing brace from the fold end
            if self.fold_state.is_folded(doc_line) {
                if let Some(region) = self.fold_state.region_at(doc_line) {
                    // Get the end line and find the closing brace
                    if let Some(end_line_text) = doc.line(region.end_line) {
                        let trimmed = end_line_text.trim();
                        // Append the closing portion (usually just "}")
                        if !trimmed.is_empty() {
                            self.cpu_visible_content.push_str(" ... ");
                            self.cpu_visible_content.push_str(trimmed);
                        }
                    }
                }
            }

            self.cpu_visible_doc_lines.push(doc_line);
            self.cpu_doc_to_visual.push(Some(visual_line));
            visual_line += 1;
        }

        // Fill remaining doc_to_visual for lines after viewport
        for doc_line in viewport_end..line_count {
            if !self.fold_state.is_line_hidden(doc_line) {
                self.cpu_doc_to_visual.push(Some(visual_line));
                visual_line += 1;
            } else {
                self.cpu_doc_to_visual.push(None);
            }
        }

        // Calculate layout dimensions
        let line_height = self.text_renderer.line_height();
        let char_width = self.cached_char_width;
        let padding = 10.0_f32;

        // Calculate gutter width (based on digit count or custom text width, before shaping)
        let gutter_width = if self.gutter_enabled {
            if let Some(ref custom_lines) = self.custom_gutter_lines {
                // Width from longest custom gutter line
                let max_chars = custom_lines.iter().map(|s| s.len()).max().unwrap_or(1);
                // Same padding formula as GutterRenderer::calculate_width
                (max_chars as f32 * char_width) + (char_width * 2.0)
            } else {
                self.gutter_renderer.calculate_width(line_count, char_width)
            }
        } else {
            0.0
        };
        let content_offset_x = gutter_width + padding;

        // Create and shape the main content buffer FIRST
        let content_width = self.surface.width() as f32 - content_offset_x;
        let mut buffer = self.text_renderer.create_buffer(Some(content_width));

        // Set the text with syntax highlighting if enabled
        let foreground = self.theme.editor.foreground;
        if self.syntax_enabled {
            if self.use_ts_highlights && !self.span_index.is_empty() {
                // Calculate viewport for efficient span query (T020)
                // Only query spans in the visible range instead of iterating all spans
                let surface_height = self.surface.height() as f32;
                let first_line = (self.scroll_y / line_height).floor() as usize;
                let visible_lines = (surface_height / line_height).ceil() as usize + 1;

                let viewport = Viewport {
                    first_line,
                    visible_lines,
                    line_height,
                    height: surface_height,
                    width: content_width,
                    ..Viewport::default()
                };

                // Query byte range with overscan buffer (T023)
                let doc = &self.editor.state().document;
                let (start_byte, end_byte) =
                    viewport.query_byte_range(doc.rope(), &self.fold_state, &self.viewport_config);

                // Query only viewport spans: O(log n + k) vs O(n) clone + sort (T021)
                // This eliminates per-frame clone (T018) and sort (T019)
                let rich_spans = self.build_rich_spans_viewport(
                    &self.cpu_visible_content,
                    self.span_index.query(start_byte, end_byte),
                    viewport_start_byte, // Byte offset where visible_content starts
                );
                self.text_renderer
                    .set_rich_text(&mut buffer, rich_spans.into_iter());
            } else if !self.ts_highlights.is_empty() {
                // Legacy fallback: use JsHighlightSpan when SpanIndex not available
                // PERF: This path clones ts_highlights to sort them. This is acceptable because:
                // 1. With WebSpanIndex, this path is rarely hit (only during transition)
                // 2. The clone happens only when span_index.is_empty() returns true
                // 3. Full optimization would require pre-sorted storage or sorted indices
                let spans = self.ts_highlights.clone();
                let rich_spans = self.build_rich_spans_from_ts(&self.cpu_visible_content, spans);
                self.text_renderer
                    .set_rich_text(&mut buffer, rich_spans.into_iter());
            } else {
                // Fall back to simple keyword-based highlighting
                // PERF: Pass iterator directly instead of collecting into Vec
                let spans = self.highlighter.highlight_flat(&self.cpu_visible_content);
                self.text_renderer.set_rich_text(
                    &mut buffer,
                    spans.iter().map(|span| (span.text.as_str(), span.color)),
                );
            }
        } else {
            // Plain text
            self.text_renderer
                .set_text(&mut buffer, &self.cpu_visible_content, foreground);
        }
        self.text_renderer.shape_buffer(&mut buffer);

        // Build cached visual line map from layout_runs() for pixel_to_position.
        // Each entry maps a visual line (within the buffer) to (buffer_line_index, run_start_column).
        // This allows pixel_to_position to correctly resolve clicks on wrapped lines.
        self.cached_visual_line_map.clear();
        self.cached_map_viewport_start = viewport_start;
        self.cached_content_offset_x = content_offset_x;
        for run in buffer.layout_runs() {
            let run_start_col = if run.glyphs.is_empty() {
                0
            } else {
                run.glyphs.first().map(|g| g.start).unwrap_or(0)
            };
            self.cached_visual_line_map
                .push((run.line_i, run_start_col));
        }

        // Cache total visual lines for max_scroll_y.
        // visual_line (from the doc_to_visual loop above) counts all visible doc lines as 1 each.
        // The actual buffer visual lines (from layout_runs) may be more due to wrapping.
        // Extra wrapped lines = buffer_visual_lines - buffer_logical_lines.
        let buffer_visual_lines = self.cached_visual_line_map.len();
        let buffer_logical_lines = self.cpu_visible_doc_lines.len();
        let extra_wrap_lines = buffer_visual_lines.saturating_sub(buffer_logical_lines);
        self.cached_total_visual_lines = visual_line + extra_wrap_lines;

        // NOW build line numbers with proper spacing for wrapped lines
        // PERF: Reuse pre-allocated string buffer, use write!() to avoid format!() allocation
        use std::fmt::Write;
        let visual_lines_per_line = self.text_renderer.visual_lines_per_logical_line(&buffer);
        self.cpu_line_numbers.clear();

        if let Some(ref custom_lines) = self.custom_gutter_lines {
            // Custom gutter text: use provided strings instead of auto line numbers.
            // This is used for diff views with dual line numbers + markers.
            for (i, &doc_line) in self.cpu_visible_doc_lines.iter().enumerate() {
                if i > 0 {
                    self.cpu_line_numbers.push('\n');
                }

                // Use custom text for this doc_line, or empty string if out of range
                let text = custom_lines.get(doc_line).map(|s| s.as_str()).unwrap_or("");
                self.cpu_line_numbers.push_str(text);

                // Add blank lines for wrapped visual lines
                let wrap_count = visual_lines_per_line.get(i).copied().unwrap_or(1);
                let text_len = text.len();
                for _ in 1..wrap_count {
                    self.cpu_line_numbers.push('\n');
                    for _ in 0..text_len {
                        self.cpu_line_numbers.push(' ');
                    }
                }
            }

            if self.cpu_visible_doc_lines.is_empty() {
                self.cpu_line_numbers.push(' ');
            }
        } else {
            // Standard auto line numbers
            let digit_width = GutterRenderer::digit_columns(line_count);

            for (i, &doc_line) in self.cpu_visible_doc_lines.iter().enumerate() {
                // Add newline separator (except for first visible line)
                if i > 0 {
                    self.cpu_line_numbers.push('\n');
                }

                // Add line number (1-indexed) directly into buffer without allocation
                // write!() into String never fails, so we can ignore the Result
                let _ = write!(
                    self.cpu_line_numbers,
                    "{:>width$}",
                    doc_line + 1,
                    width = digit_width,
                );

                // Add blank lines for wrapped visual lines (continuation lines)
                let wrap_count = visual_lines_per_line.get(i).copied().unwrap_or(1);
                for _ in 1..wrap_count {
                    self.cpu_line_numbers.push('\n');
                    // Add blank spacing to maintain alignment
                    for _ in 0..digit_width {
                        self.cpu_line_numbers.push(' ');
                    }
                }
            }

            // Handle empty document
            if line_count == 0 {
                self.cpu_line_numbers.push_str(" 1");
            }
        }

        // Create gutter buffer AFTER we know the wrapping
        let gutter_buffer = if self.gutter_enabled {
            let mut gutter_buf = self.text_renderer.create_buffer(Some(gutter_width));
            let line_number_color = self.theme.editor.line_number;
            self.text_renderer
                .set_text(&mut gutter_buf, &self.cpu_line_numbers, line_number_color);
            self.text_renderer.shape_buffer(&mut gutter_buf);
            Some(gutter_buf)
        } else {
            None
        };

        // Calculate cursor position (accounting for gutter offset, folding, scroll, and line wrapping)
        let cursor_pos = self.editor.cursor();
        let visual_cursor_line = self
            .cpu_doc_to_visual
            .get(cursor_pos.line)
            .and_then(|v| *v)
            .unwrap_or(0);

        // Calculate virtual scroll offset for viewport virtualization
        // This must match the offset used for text rendering (uses viewport_start directly)
        let virtual_scroll_offset = viewport_start as f32 * line_height;

        // Calculate cursor's line index within the viewport buffer
        // The buffer only contains lines from viewport_start, so we need relative positioning
        let viewport_start_visual = self
            .cpu_doc_to_visual
            .get(viewport_start)
            .and_then(|v| *v)
            .unwrap_or(0);
        let cursor_line_in_buffer = visual_cursor_line.saturating_sub(viewport_start_visual);

        // Use buffer layout to get accurate position with line wrapping.
        //
        // Only the vertical position is wanted here. Scrolling follows the
        // primary caret alone, and inline blame sits on its line; the caret
        // *quads* — the primary's included — are positioned in the loop further
        // down, which walks every selection.
        let (_, wrap_y) = self.text_renderer.cursor_position_in_buffer(
            &buffer,
            cursor_line_in_buffer,
            cursor_pos.column,
            char_width,
        );
        // Absolute cursor Y in document space (for ensure_cursor_visible)
        let cursor_abs_y = padding + wrap_y + virtual_scroll_offset;
        // Viewport-relative cursor Y for rendering
        let cursor_y = cursor_abs_y - self.scroll_y;

        // Cache cursor position for ensure_cursor_visible
        self.cached_cursor_abs_y = cursor_abs_y;
        self.cached_cursor_doc_line = cursor_pos.line;

        // Update cursor blink state
        self.cursor_renderer.update(Instant::now());

        // PERF: Reuse pre-allocated quad buffers
        // Create gutter background quad
        self.cpu_gutter_quads.clear();
        if self.gutter_enabled {
            let gutter_bg_color = self.theme.editor.gutter;
            self.cpu_gutter_quads.push(Quad::new(
                0.0,
                0.0,
                gutter_width,
                self.surface.height() as f32,
                gutter_bg_color,
            ));
        }

        // Create line background quads for diff highlighting
        // These render behind selection highlights so diffs are visible even when selected.
        self.cpu_line_bg_quads.clear();
        if !self.line_backgrounds.is_empty() {
            let surface_height = self.surface.height() as f32;
            let viewport_width = self.surface.width() as f32;
            for (vi, &doc_line) in self.cpu_visible_doc_lines.iter().enumerate() {
                if let Some(&bg_color) = self.line_backgrounds.get(&doc_line) {
                    // Walk the visual line map to find which visual rows correspond
                    // to this logical line (accounts for wrapping).
                    let buffer_line = self
                        .cpu_doc_to_visual
                        .get(doc_line)
                        .and_then(|v| *v)
                        .unwrap_or(0)
                        .saturating_sub(
                            self.cpu_doc_to_visual
                                .get(viewport_start)
                                .and_then(|v| *v)
                                .unwrap_or(0),
                        );

                    let mut emitted = false;
                    for (vline_idx, &(buf_idx, _)) in self.cached_visual_line_map.iter().enumerate()
                    {
                        if buf_idx != buffer_line {
                            continue;
                        }
                        let y = padding + (vline_idx as f32 * line_height) + virtual_scroll_offset
                            - self.scroll_y;
                        if y + line_height > 0.0 && y < surface_height {
                            self.cpu_line_bg_quads.push(Quad::new(
                                content_offset_x,
                                y,
                                viewport_width - content_offset_x,
                                line_height,
                                bg_color,
                            ));
                        }
                        emitted = true;
                    }

                    // Fallback: if visual line map wasn't built yet, use simple position
                    if !emitted {
                        let y = padding + (vi as f32 * line_height) + virtual_scroll_offset
                            - self.scroll_y;
                        if y + line_height > 0.0 && y < surface_height {
                            self.cpu_line_bg_quads.push(Quad::new(
                                content_offset_x,
                                y,
                                viewport_width - content_offset_x,
                                line_height,
                                bg_color,
                            ));
                        }
                    }
                }
            }
        }

        // Create gutter change indicator quads (thin colored bars at left edge)
        self.cpu_gutter_change_quads.clear();
        if !self.gutter_changes.is_empty() && self.gutter_enabled {
            let surface_height = self.surface.height() as f32;
            for (vi, &doc_line) in self.cpu_visible_doc_lines.iter().enumerate() {
                if let Some(&bar_color) = self.gutter_changes.get(&doc_line) {
                    let buffer_line = self
                        .cpu_doc_to_visual
                        .get(doc_line)
                        .and_then(|v| *v)
                        .unwrap_or(0)
                        .saturating_sub(
                            self.cpu_doc_to_visual
                                .get(viewport_start)
                                .and_then(|v| *v)
                                .unwrap_or(0),
                        );

                    let mut emitted = false;
                    for (vline_idx, &(buf_idx, _)) in self.cached_visual_line_map.iter().enumerate()
                    {
                        if buf_idx != buffer_line {
                            continue;
                        }
                        let y = padding + (vline_idx as f32 * line_height) + virtual_scroll_offset
                            - self.scroll_y;
                        if y + line_height > 0.0 && y < surface_height {
                            // 3px wide bar at left gutter edge
                            self.cpu_gutter_change_quads.push(Quad::new(
                                2.0,
                                y,
                                3.0,
                                line_height,
                                bar_color,
                            ));
                        }
                        emitted = true;
                    }

                    if !emitted {
                        let y = padding + (vi as f32 * line_height) + virtual_scroll_offset
                            - self.scroll_y;
                        if y + line_height > 0.0 && y < surface_height {
                            self.cpu_gutter_change_quads.push(Quad::new(
                                2.0,
                                y,
                                3.0,
                                line_height,
                                bar_color,
                            ));
                        }
                    }
                }
            }
        }

        // Create selection highlight quads (render before text)
        // Uses cursor_position_in_buffer for wrap-aware positioning so that
        // selection highlights align with text even when lines wrap.
        self.cpu_selection_quads.clear();
        let selection_color = self.theme.editor.selection;
        let surface_height = self.surface.height() as f32;
        // Every cursor's selection, not only the primary's.
        //
        // Drawing just the primary is what made multi-cursor invisible in this
        // face: the kernel held N selections and the screen showed one, so a
        // command that worked perfectly looked like a command that did nothing.
        for selection in self.editor.state().cursor.all_selections() {
            if selection.is_collapsed() {
                continue;
            }
            let sel_start = selection.start();
            let sel_end = selection.end();

            for doc_line in sel_start.line..=sel_end.line {
                // Skip hidden/folded lines
                let Some(vis_line) = self.cpu_doc_to_visual.get(doc_line).and_then(|v| *v) else {
                    continue;
                };

                let line_content = self.editor.state().document.line(doc_line);
                let line_len = line_content.map(|l| l.chars().count()).unwrap_or(0);

                // Determine selection columns for this line
                let sel_col_start = if doc_line == sel_start.line {
                    sel_start.column
                } else {
                    0
                };
                let sel_col_end = if doc_line == sel_end.line {
                    sel_end.column
                } else {
                    line_len
                };

                // Extra width for newline visualization on non-final lines
                let newline_extra = if doc_line != sel_end.line && sel_col_end == line_len {
                    char_width * 0.5
                } else {
                    0.0
                };

                if sel_col_start >= sel_col_end && newline_extra == 0.0 {
                    continue;
                }

                // Get buffer-relative line index for this doc line
                let buffer_line = vis_line.saturating_sub(viewport_start_visual);

                // Walk the visual line map to find which visual rows this buffer line
                // spans and emit a quad for each wrapped segment that overlaps the selection.
                let mut handled = false;
                for (vi, &(buf_idx, run_start_col)) in
                    self.cached_visual_line_map.iter().enumerate()
                {
                    if buf_idx != buffer_line {
                        continue;
                    }

                    // Determine the end column of this visual segment
                    let run_end_col = self
                        .cached_visual_line_map
                        .get(vi + 1)
                        .filter(|(next_buf, _)| *next_buf == buffer_line)
                        .map(|(_, next_start)| *next_start)
                        .unwrap_or(line_len);

                    // Intersect this segment with the selection range
                    let seg_start = sel_col_start.max(run_start_col);
                    let seg_end = sel_col_end.min(run_end_col);

                    // Extra width only applies on the last segment of the line
                    let seg_extra = if run_end_col >= line_len {
                        newline_extra
                    } else {
                        0.0
                    };

                    if seg_start < seg_end || (seg_start == seg_end && seg_extra > 0.0) {
                        let x =
                            content_offset_x + ((seg_start - run_start_col) as f32 * char_width);
                        let y = padding + (vi as f32 * line_height) + virtual_scroll_offset
                            - self.scroll_y;
                        let width = (seg_end - seg_start) as f32 * char_width + seg_extra;

                        if y + line_height > 0.0 && y < surface_height {
                            self.cpu_selection_quads.push(Quad::new(
                                x,
                                y,
                                width,
                                line_height,
                                selection_color,
                            ));
                        }
                        handled = true;
                    }
                }

                // Fallback for lines not in the visual map (shouldn't happen, but safe)
                if !handled {
                    let (sx, sy) = self.text_renderer.cursor_position_in_buffer(
                        &buffer,
                        buffer_line,
                        sel_col_start,
                        char_width,
                    );
                    let x = content_offset_x + sx;
                    let y = padding + sy + virtual_scroll_offset - self.scroll_y;
                    let width = (sel_col_end - sel_col_start) as f32 * char_width + newline_extra;

                    if y + line_height > 0.0 && y < surface_height {
                        self.cpu_selection_quads.push(Quad::new(
                            x,
                            y,
                            width,
                            line_height,
                            selection_color,
                        ));
                    }
                }
            }
        }

        // Create a cursor quad (2px wide line) for **every** caret.
        //
        // The primary's position is computed above and kept there, because
        // `ensure_cursor_visible` scrolls to it and must keep following the
        // primary alone. The others are derived here the same way.
        //
        // Blink is shared on purpose: carets blinking out of phase read as a
        // rendering fault rather than as one multi-cursor edit.
        let cursor_color = self.theme.editor.cursor;
        self.cpu_cursor_quads.clear();
        if self.cursor_renderer.is_visible() {
            for selection in self.editor.state().cursor.all_selections() {
                let head = selection.head;
                let head_visual_line = self
                    .cpu_doc_to_visual
                    .get(head.line)
                    .and_then(|v| *v)
                    .unwrap_or(0);
                let head_line_in_buffer = head_visual_line.saturating_sub(viewport_start_visual);
                let (head_x, head_y) = self.text_renderer.cursor_position_in_buffer(
                    &buffer,
                    head_line_in_buffer,
                    head.column,
                    char_width,
                );
                let x = content_offset_x + head_x;
                let y = padding + head_y + virtual_scroll_offset - self.scroll_y;
                // Off-screen carets are skipped, exactly as selection quads are.
                if y + line_height > 0.0 && y < surface_height {
                    self.cpu_cursor_quads
                        .push(Quad::new(x, y, 2.0, line_height, cursor_color));
                }
            }
        }

        // PERF: Use stack-allocated array instead of Vec for text areas
        // We know there are at most 2 text areas (main content + gutter)
        // Calculate virtual scroll offset for viewport virtualization
        let virtual_scroll_offset = viewport_start as f32 * line_height;
        let adjusted_scroll_y = self.scroll_y - virtual_scroll_offset;

        // Main content text area
        let main_text_area = TextArea {
            buffer: &buffer,
            left: content_offset_x,
            top: padding - adjusted_scroll_y,
            scale: 1.0,
            bounds: TextBounds {
                left: 0,
                top: 0,
                right: self.surface.width() as i32,
                bottom: self.surface.height() as i32,
            },
            default_color: glyphon::Color::rgba(
                (foreground.r * 255.0) as u8,
                (foreground.g * 255.0) as u8,
                (foreground.b * 255.0) as u8,
                (foreground.a * 255.0) as u8,
            ),
            custom_glyphs: &[],
        };

        // Build blame buffer if blame data exists for the cursor line.
        // This creates a third TextArea rendered as ghost text after the line content.
        let blame_fg = self.theme.editor.blame_foreground;
        let blame_buffer = if !self.blame_data.is_empty() {
            let cursor_doc_line = self.editor.cursor().line;
            if let Some(blame_text) = self.blame_data.get(&cursor_doc_line) {
                self.cpu_blame_content.clear();
                // Pad with spaces to separate from line content
                self.cpu_blame_content.push_str("  ");
                self.cpu_blame_content.push_str(blame_text);
                let mut blame_buf = self.text_renderer.create_buffer(None);
                self.text_renderer
                    .set_text(&mut blame_buf, &self.cpu_blame_content, blame_fg);
                self.text_renderer.shape_buffer(&mut blame_buf);
                Some(blame_buf)
            } else {
                None
            }
        } else {
            None
        };

        // Calculate blame text area position: after the end of the cursor line content
        let blame_left = if blame_buffer.is_some() {
            let cursor_doc_line = self.editor.cursor().line;
            let line_len = self
                .editor
                .state()
                .document
                .line(cursor_doc_line)
                .map(|l| l.chars().count())
                .unwrap_or(0);
            content_offset_x + (line_len as f32 * char_width)
        } else {
            0.0
        };

        // Prepare text for rendering - use Vec for dynamic text area count
        let line_number_color = self.theme.editor.line_number;

        // Build text areas list dynamically based on what's enabled
        let mut text_areas: Vec<TextArea> = Vec::with_capacity(3);
        text_areas.push(main_text_area);

        if let Some(ref gutter_buf) = gutter_buffer {
            text_areas.push(TextArea {
                buffer: gutter_buf,
                left: 8.0, // Small padding from left edge
                top: padding - adjusted_scroll_y,
                scale: 1.0,
                bounds: TextBounds {
                    left: 0,
                    top: 0,
                    right: gutter_width as i32,
                    bottom: self.surface.height() as i32,
                },
                default_color: glyphon::Color::rgba(
                    (line_number_color.r * 255.0) as u8,
                    (line_number_color.g * 255.0) as u8,
                    (line_number_color.b * 255.0) as u8,
                    (line_number_color.a * 255.0) as u8,
                ),
                custom_glyphs: &[],
            });
        }

        if let Some(ref blame_buf) = blame_buffer {
            text_areas.push(TextArea {
                buffer: blame_buf,
                left: blame_left,
                top: cursor_y,
                scale: 1.0,
                bounds: TextBounds {
                    left: blame_left as i32,
                    top: 0,
                    right: self.surface.width() as i32,
                    bottom: self.surface.height() as i32,
                },
                default_color: glyphon::Color::rgba(
                    (blame_fg.r * 255.0) as u8,
                    (blame_fg.g * 255.0) as u8,
                    (blame_fg.b * 255.0) as u8,
                    (blame_fg.a * 255.0) as u8,
                ),
                custom_glyphs: &[],
            });
        }

        self.text_renderer
            .prepare(self.surface.device(), self.surface.queue(), text_areas)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        // Get background color from theme
        let bg = self.theme.editor.background;
        let clear_color = Color {
            r: f64::from(bg.r),
            g: f64::from(bg.g),
            b: f64::from(bg.b),
            a: f64::from(bg.a),
        };

        // PERF: Batch all background quads using pre-allocated buffer.
        // Render order (back to front): gutter bg → line backgrounds → gutter change bars → selection
        self.cpu_background_quads.clear();
        self.cpu_background_quads
            .extend(self.cpu_gutter_quads.iter().copied());
        self.cpu_background_quads
            .extend(self.cpu_line_bg_quads.iter().copied());
        self.cpu_background_quads
            .extend(self.cpu_gutter_change_quads.iter().copied());
        self.cpu_background_quads
            .extend(self.cpu_selection_quads.iter().copied());

        // Render frame using SEPARATE quad renderers for background and cursor
        // This is critical: queue.write_buffer() to the same buffer multiple times within
        // a frame causes only the last write to be visible. Using separate QuadRenderers
        // with separate vertex buffers avoids this GPU synchronization issue.
        let text_renderer = &self.text_renderer;
        let background_quad_renderer = &mut self.background_quad_renderer;
        let cursor_quad_renderer = &mut self.cursor_quad_renderer;
        let background_quads = &self.cpu_background_quads;
        let cursor_quads = &self.cpu_cursor_quads;
        let queue = self.surface.queue_arc();
        self.surface
            .render_frame(|view, device, q| {
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Iridium Frame Encoder"),
                });

                {
                    let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                        label: Some("Iridium Render Pass"),
                        color_attachments: &[Some(RenderPassColorAttachment {
                            view,
                            resolve_target: None,
                            ops: Operations {
                                load: LoadOp::Clear(clear_color),
                                store: StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });

                    // Render gutter background and selection highlights (behind text)
                    // Uses background_quad_renderer with its own vertex buffer
                    background_quad_renderer.render(&mut pass, &queue, background_quads);

                    // Render text (main content and gutter line numbers)
                    text_renderer.render(&mut pass).map_err(|e| {
                        iridium_editor::editor::IridiumError::GpuInitFailed {
                            message: e.to_string(),
                        }
                    })?;

                    // Render cursor on top - uses cursor_quad_renderer with its own vertex buffer
                    // This avoids the GPU buffer overwrite issue that caused selection blinking
                    cursor_quad_renderer.render(&mut pass, &queue, cursor_quads);
                }

                q.submit(std::iter::once(encoder.finish()));
                Ok(())
            })
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        // Trim glyph cache periodically
        self.text_renderer.trim_cache();

        // T042: Log performance warning if frame exceeds 16ms budget
        let frame_time = frame_start.elapsed();
        if frame_time.as_millis() > 16 {
            log(&format!(
                "[Iridium] Slow frame: {}ms (target: 16ms)",
                frame_time.as_millis()
            ));
        }

        Ok(())
    }

    /// Sets whether to use dark theme.
    #[wasm_bindgen(js_name = setDarkTheme)]
    pub fn set_dark_theme(&mut self, dark: bool) {
        self.theme = if dark {
            self.highlighter.set_dark_theme();
            Theme::dark()
        } else {
            self.highlighter.set_light_theme();
            Theme::light()
        };
        self.needs_redraw = true;
    }

    /// Enables or disables syntax highlighting.
    #[wasm_bindgen(js_name = setSyntaxEnabled)]
    pub fn set_syntax_enabled(&mut self, enabled: bool) {
        self.syntax_enabled = enabled;
        self.needs_redraw = true;
    }

    /// Returns whether syntax highlighting is enabled.
    #[wasm_bindgen(js_name = isSyntaxEnabled)]
    pub fn is_syntax_enabled(&self) -> bool {
        self.syntax_enabled
    }

    /// Sets highlight spans from tree-sitter (called from JavaScript).
    ///
    /// The spans array should contain objects with: start (byte), end (byte), type (string).
    /// This enables proper tree-sitter syntax highlighting in the WASM build.
    ///
    /// Internally builds a WebSpanIndex for O(log n + k) viewport queries.
    #[wasm_bindgen(js_name = setTreeSitterHighlights)]
    pub fn set_tree_sitter_highlights(&mut self, spans_js: &JsValue) -> Result<(), JsValue> {
        use js_sys::{Array, Reflect};

        let array = Array::from(spans_js);
        let mut js_spans = Vec::with_capacity(array.length() as usize);
        let mut web_spans = Vec::with_capacity(array.length() as usize);

        for i in 0..array.length() {
            let obj = array.get(i);
            let start = Reflect::get(&obj, &JsValue::from_str("start"))
                .map_err(|_| JsValue::from_str("missing start"))?
                .as_f64()
                .ok_or_else(|| JsValue::from_str("start not a number"))?
                as usize;
            let end = Reflect::get(&obj, &JsValue::from_str("end"))
                .map_err(|_| JsValue::from_str("missing end"))?
                .as_f64()
                .ok_or_else(|| JsValue::from_str("end not a number"))?
                as usize;
            let type_str = Reflect::get(&obj, &JsValue::from_str("type"))
                .map_err(|_| JsValue::from_str("missing type"))?
                .as_string()
                .ok_or_else(|| JsValue::from_str("type not a string"))?;

            // Keep the string-based span for legacy color lookup
            js_spans.push(JsHighlightSpan {
                start,
                end,
                highlight_type: type_str.clone(),
            });

            // Build WebSpan for WebSpanIndex
            web_spans.push(WebSpan {
                start,
                end,
                highlight_type: type_str,
            });
        }

        // Build the WebSpanIndex for efficient viewport queries (O(n log n) once)
        self.span_index = WebSpanIndex::new(web_spans);

        // Keep legacy spans for fallback (will be removed in future)
        self.ts_highlights = js_spans;
        self.use_ts_highlights = true;
        self.needs_redraw = true;
        Ok(())
    }

    /// Sets the syntax color theme from a JS object.
    ///
    /// Accepts a `Record<string, string>` mapping capture names to hex colors.
    /// Example: `{ keyword: "#81A1C1", string: "#A3BE8C", comment: "#657B83" }`
    ///
    /// The editor uses hierarchical lookup: a capture like `keyword.control.return`
    /// will match `keyword.control.return`, then `keyword.control`, then `keyword`.
    /// This allows broad categories (e.g., `keyword`) to cover all sub-types while
    /// still allowing specific overrides (e.g., `keyword.control.return`).
    #[wasm_bindgen(js_name = setSyntaxTheme)]
    pub fn set_syntax_theme(&mut self, theme_js: &JsValue) -> Result<(), JsValue> {
        use iridium_editor::theme::Color;
        use js_sys::{Object, Reflect};

        self.syntax_theme.clear();

        let obj = Object::from(theme_js.clone());
        let keys = Object::keys(&obj);

        for i in 0..keys.length() {
            let key = keys.get(i);
            let key_str = key
                .as_string()
                .ok_or_else(|| JsValue::from_str("theme key not a string"))?;
            let value = Reflect::get(&obj, &key)?;
            let hex = value
                .as_string()
                .ok_or_else(|| JsValue::from_str("theme value not a hex color string"))?;
            let color = Color::from_hex(&hex).ok_or_else(|| {
                JsValue::from_str(&format!("invalid hex color for '{}': {}", key_str, hex))
            })?;
            self.syntax_theme.insert(key_str, color);
        }

        self.needs_redraw = true;
        Ok(())
    }

    /// Clears tree-sitter highlights and falls back to simple highlighting.
    #[wasm_bindgen(js_name = clearTreeSitterHighlights)]
    pub fn clear_tree_sitter_highlights(&mut self) {
        self.ts_highlights.clear();
        self.use_ts_highlights = false;
        self.needs_redraw = true;
    }

    /// Returns whether tree-sitter highlighting is active.
    #[wasm_bindgen(js_name = isTreeSitterActive)]
    pub fn is_tree_sitter_active(&self) -> bool {
        self.use_ts_highlights
    }

    /// Gets the current cursor line (0-indexed).
    #[wasm_bindgen(js_name = getCursorLine)]
    pub fn get_cursor_line(&self) -> u32 {
        self.editor.cursor().line as u32
    }

    /// Gets the current cursor column (0-indexed).
    #[wasm_bindgen(js_name = getCursorColumn)]
    pub fn get_cursor_column(&self) -> u32 {
        self.editor.cursor().column as u32
    }

    /// Gets the total line count.
    #[wasm_bindgen(js_name = getLineCount)]
    pub fn get_line_count(&self) -> u32 {
        self.editor.state().document.line_count() as u32
    }

    /// Returns the document's monotonic content-revision counter.
    ///
    /// The counter increases by at least one on every mutation that
    /// changes the text — keystroke, paste, cut, undo/redo, remote edit —
    /// never decreases, and is unaffected by pure cursor moves. The
    /// authoring adapter uses it as a cheap local cut to correlate spans
    /// taken via `takeLastEdit` with the proposals built from them. Two
    /// observations of the same value guarantee the text did not change
    /// between them.
    // wasm-bindgen cannot export `const fn`.
    #[allow(clippy::missing_const_for_fn)]
    pub fn revision(&self) -> u64 {
        self.editor.state().document.revision()
    }

    /// Checks if undo is available.
    #[wasm_bindgen(js_name = canUndo)]
    pub fn can_undo(&self) -> bool {
        self.editor.state().history.can_undo()
    }

    /// Checks if redo is available.
    #[wasm_bindgen(js_name = canRedo)]
    pub fn can_redo(&self) -> bool {
        self.editor.state().history.can_redo()
    }

    /// Performs undo. No-op in read-only mode.
    ///
    /// Records a conservative whole-document edit for `takeLastEdit` (the
    /// replayed command refers to a document state the binding never
    /// tracked), so incremental highlight consumers stay correct.
    pub fn undo(&mut self) -> bool {
        if self.read_only {
            return false;
        }
        self.with_whole_document_edit(Editor::undo)
    }

    /// Performs redo. No-op in read-only mode.
    ///
    /// Edit tracking behaves like [`Self::undo`]: a conservative
    /// whole-document edit is recorded for `takeLastEdit`.
    pub fn redo(&mut self) -> bool {
        if self.read_only {
            return false;
        }
        self.with_whole_document_edit(Editor::redo)
    }

    /// Redoes into a specific branch of the current node, by index into
    /// `historySnapshot()`'s `childIds` for the current node.
    ///
    /// This is how a panel enters a fork the plain `redo` would not take.
    /// No-op in read-only mode or when the index names no branch.
    #[wasm_bindgen(js_name = redoBranch)]
    pub fn redo_branch(&mut self, branch_index: u32) -> bool {
        self.redo_branch_internal(branch_index as usize)
    }

    /// The whole undo tree as JSON, for a panel that draws it.
    ///
    /// One call rather than a walk: the tree changes on every keystroke, and
    /// asking node by node would mean N boundary crossings per repaint. Node
    /// ids are decimal *strings* throughout, because they are `u64` and a
    /// JavaScript number is not. Keys are camelCase, matching the palette's
    /// wire shape rather than the Rust field names.
    ///
    /// Returns `"null"` only if serialization fails, which it cannot: the
    /// snapshot is strings, booleans and integers throughout.
    #[wasm_bindgen(js_name = historySnapshot)]
    pub fn history_snapshot(&self) -> String {
        serde_json::to_string(&self.editor.history_snapshot())
            .unwrap_or_else(|_| "null".to_string())
    }

    /// Moves to an arbitrary node of the undo tree, replaying the document to
    /// that state.
    ///
    /// `node_id` is the decimal string a snapshot reports. This reaches states
    /// no sequence of undo and redo could reach without first abandoning a
    /// branch — which is the entire point of having a tree rather than a stack.
    ///
    /// Returns `false`, changing nothing, in read-only mode, when `node_id` is
    /// not a number, or when it names no node in this tree.
    #[wasm_bindgen(js_name = jumpToHistoryNode)]
    pub fn jump_to_history_node(&mut self, node_id: &str) -> bool {
        if self.read_only {
            return false;
        }
        let Ok(raw) = node_id.parse::<u64>() else {
            return false;
        };
        self.with_whole_document_edit(|editor| {
            editor.jump_to_history_node(UndoNodeId::from_u64(raw))
        })
    }

    /// Runs one history traversal and records a conservative whole-document
    /// edit for `takeLastEdit` when it changed anything.
    ///
    /// The replayed command refers to a document state this binding never
    /// tracked, so a precise span is not available; a whole-document record is
    /// the conservative answer that keeps incremental highlight consumers
    /// correct. Every traversal shares it so none can forget.
    fn with_whole_document_edit<F>(&mut self, traverse: F) -> bool
    where
        F: FnOnce(&mut Editor) -> bool,
    {
        let pre_end = self.document_end_point();
        let result = traverse(&mut self.editor);
        if result {
            // The cursor moved outside handle_key; drop sticky columns.
            self.keyboard_handler.reset_vertical_state();
            match pre_end {
                Some((pre_len, pre_row, pre_column)) => {
                    self.record_whole_document_edit(pre_len, pre_row, pre_column);
                },
                None => self.pending_edit = PendingEdit::Degraded,
            }
            // Update fold regions after content change
            let content = self.editor.content();
            self.refresh_fold_regions(&content);
            self.needs_redraw = true;
        }
        result
    }

    /// `redoBranch` without the wasm-facing `u32`, so the key path can call it.
    fn redo_branch_internal(&mut self, branch_index: usize) -> bool {
        if self.read_only {
            return false;
        }
        self.with_whole_document_edit(|editor| editor.redo_branch(branch_index))
    }

    /// Returns `(byte_len, end_row, end_byte_column)` of the current
    /// document, used to capture the pre-operation document end for
    /// whole-document edit records.
    fn document_end_point(&self) -> Option<(usize, usize, usize)> {
        let document = &self.editor.state().document;
        let len = document.byte_count();
        let (row, column) = byte_point(document, len)?;
        Some((len, row, column))
    }

    /// Moves cursor left (collapses any selection).
    #[wasm_bindgen(js_name = moveCursorLeft)]
    pub fn move_cursor_left(&mut self) {
        let cursor = self.editor.cursor();
        let new_pos = if cursor.column > 0 {
            Position::new(cursor.line, cursor.column - 1)
        } else if cursor.line > 0 {
            // Move to end of previous line
            let prev_line_len = self
                .editor
                .state()
                .document
                .line(cursor.line - 1)
                .map(|l| l.chars().count())
                .unwrap_or(0);
            Position::new(cursor.line - 1, prev_line_len)
        } else {
            return;
        };
        self.set_selection_internal(new_pos, new_pos);
    }

    /// Moves cursor right (collapses any selection).
    #[wasm_bindgen(js_name = moveCursorRight)]
    pub fn move_cursor_right(&mut self) {
        let cursor = self.editor.cursor();
        let current_line_len = self
            .editor
            .state()
            .document
            .line(cursor.line)
            .map(|l| l.chars().count())
            .unwrap_or(0);

        let new_pos = if cursor.column < current_line_len {
            Position::new(cursor.line, cursor.column + 1)
        } else if cursor.line < self.editor.state().document.line_count().saturating_sub(1) {
            // Move to start of next line
            Position::new(cursor.line + 1, 0)
        } else {
            return;
        };
        self.set_selection_internal(new_pos, new_pos);
    }

    /// Moves cursor up (collapses any selection).
    #[wasm_bindgen(js_name = moveCursorUp)]
    pub fn move_cursor_up(&mut self) {
        let cursor = self.editor.cursor();
        if cursor.line > 0 {
            let target_line_len = self
                .editor
                .state()
                .document
                .line(cursor.line - 1)
                .map(|l| l.chars().count())
                .unwrap_or(0);
            let new_column = cursor.column.min(target_line_len);
            let new_pos = Position::new(cursor.line - 1, new_column);
            self.set_selection_internal(new_pos, new_pos);
        }
    }

    /// Moves cursor down (collapses any selection).
    #[wasm_bindgen(js_name = moveCursorDown)]
    pub fn move_cursor_down(&mut self) {
        let cursor = self.editor.cursor();
        let line_count = self.editor.state().document.line_count();
        if cursor.line < line_count.saturating_sub(1) {
            let target_line_len = self
                .editor
                .state()
                .document
                .line(cursor.line + 1)
                .map(|l| l.chars().count())
                .unwrap_or(0);
            let new_column = cursor.column.min(target_line_len);
            let new_pos = Position::new(cursor.line + 1, new_column);
            self.set_selection_internal(new_pos, new_pos);
        }
    }

    /// Moves cursor to the start of the current line (collapses any selection).
    #[wasm_bindgen(js_name = moveCursorLineStart)]
    pub fn move_cursor_line_start(&mut self) {
        let cursor = self.editor.cursor();
        let new_pos = Position::new(cursor.line, 0);
        self.set_selection_internal(new_pos, new_pos);
    }

    /// Moves cursor to the end of the current line (collapses any selection).
    #[wasm_bindgen(js_name = moveCursorLineEnd)]
    pub fn move_cursor_line_end(&mut self) {
        let cursor = self.editor.cursor();
        let line_len = self
            .editor
            .state()
            .document
            .line(cursor.line)
            .map(|l| l.chars().count())
            .unwrap_or(0);
        let new_pos = Position::new(cursor.line, line_len);
        self.set_selection_internal(new_pos, new_pos);
    }

    /// Moves cursor to the start of the document (collapses any selection).
    #[wasm_bindgen(js_name = moveCursorDocStart)]
    pub fn move_cursor_doc_start(&mut self) {
        let new_pos = Position::new(0, 0);
        self.set_selection_internal(new_pos, new_pos);
    }

    /// Moves cursor to the end of the document (collapses any selection).
    #[wasm_bindgen(js_name = moveCursorDocEnd)]
    pub fn move_cursor_doc_end(&mut self) {
        let line_count = self.editor.state().document.line_count();
        let new_pos = if line_count > 0 {
            let last_line = line_count - 1;
            let last_line_len = self
                .editor
                .state()
                .document
                .line(last_line)
                .map(|l| l.chars().count())
                .unwrap_or(0);
            Position::new(last_line, last_line_len)
        } else {
            Position::new(0, 0)
        };
        self.set_selection_internal(new_pos, new_pos);
    }

    /// Moves cursor to the previous word boundary (collapses any selection).
    #[wasm_bindgen(js_name = moveCursorWordLeft)]
    pub fn move_cursor_word_left(&mut self) {
        let cursor = self.editor.cursor();
        let new_pos = self.find_word_boundary_left(cursor);
        self.set_selection_internal(new_pos, new_pos);
    }

    /// Moves cursor to the next word boundary (collapses any selection).
    #[wasm_bindgen(js_name = moveCursorWordRight)]
    pub fn move_cursor_word_right(&mut self) {
        let cursor = self.editor.cursor();
        let new_pos = self.find_word_boundary_right(cursor);
        self.set_selection_internal(new_pos, new_pos);
    }

    /// Categorizes a character for word boundary detection.
    /// Returns: 0 = whitespace, 1 = word (alphanumeric/_), 2 = punctuation
    fn char_class(c: &char) -> u8 {
        if c.is_whitespace() {
            0
        } else if c.is_alphanumeric() || *c == '_' {
            1
        } else {
            2 // punctuation and other symbols
        }
    }

    /// Helper function to find the previous word boundary.
    fn find_word_boundary_left(&self, pos: Position) -> Position {
        let doc = &self.editor.state().document;

        // If at start of line, go to end of previous line
        if pos.column == 0 {
            if pos.line > 0 {
                let prev_line_len = doc
                    .line(pos.line - 1)
                    .map(|l| l.chars().count())
                    .unwrap_or(0);
                return Position::new(pos.line - 1, prev_line_len);
            }
            return pos;
        }

        // Get current line content
        let line_content = match doc.line(pos.line) {
            Some(l) => l,
            None => return pos,
        };
        let chars: Vec<char> = line_content.chars().collect();

        if pos.column > chars.len() {
            return Position::new(pos.line, chars.len());
        }

        let mut col = pos.column;

        // Skip whitespace going backwards
        while col > 0
            && chars
                .get(col - 1)
                .map(|c| c.is_whitespace())
                .unwrap_or(false)
        {
            col -= 1;
        }

        // If we hit the start, we're done
        if col == 0 {
            return Position::new(pos.line, col);
        }

        // Determine the class of character we're about to skip
        let target_class = chars.get(col - 1).map(Self::char_class).unwrap_or(0);

        // Skip characters of the same class going backwards
        while col > 0 && chars.get(col - 1).map(Self::char_class).unwrap_or(0) == target_class {
            col -= 1;
        }

        Position::new(pos.line, col)
    }

    /// Helper function to find the next word boundary.
    fn find_word_boundary_right(&self, pos: Position) -> Position {
        let doc = &self.editor.state().document;
        let line_count = doc.line_count();

        // Get current line content
        let line_content = match doc.line(pos.line) {
            Some(l) => l,
            None => return pos,
        };
        let chars: Vec<char> = line_content.chars().collect();

        // If at end of line, go to start of next line
        if pos.column >= chars.len() {
            if pos.line < line_count.saturating_sub(1) {
                return Position::new(pos.line + 1, 0);
            }
            return pos;
        }

        let mut col = pos.column;

        // Determine the class of the current character
        let current_class = chars.get(col).map(Self::char_class).unwrap_or(0);

        // Skip characters of the same class going forwards
        while col < chars.len()
            && chars.get(col).map(Self::char_class).unwrap_or(0) == current_class
        {
            col += 1;
        }

        // Skip whitespace going forwards
        while col < chars.len() && chars.get(col).map(|c| c.is_whitespace()).unwrap_or(false) {
            col += 1;
        }

        Position::new(pos.line, col)
    }

    /// Deletes the word before the cursor (Option+Backspace on Mac).
    /// No-op in read-only mode.
    #[wasm_bindgen(js_name = deleteWordBackward)]
    pub fn delete_word_backward(&mut self) {
        if self.read_only {
            return;
        }
        let cursor = self.editor.cursor();
        let word_start = self.find_word_boundary_left(cursor);

        if word_start != cursor {
            let cmd = Command::Delete {
                range: Range::new(word_start, cursor),
                deleted_text: self
                    .editor
                    .state()
                    .document
                    .slice(Range::new(word_start, cursor)),
            };
            // track_and_apply records the edit span, refreshes folds, and
            // resets blink; set_cursor resets the core's sticky columns and
            // ours must follow.
            self.track_and_apply(cmd);
            self.keyboard_handler.reset_vertical_state();
            self.editor.set_cursor(word_start);
        }
    }

    /// Deletes the word after the cursor (Option+Delete on Mac).
    /// No-op in read-only mode.
    #[wasm_bindgen(js_name = deleteWordForward)]
    pub fn delete_word_forward(&mut self) {
        if self.read_only {
            return;
        }
        let cursor = self.editor.cursor();
        let word_end = self.find_word_boundary_right(cursor);

        if word_end != cursor {
            let cmd = Command::Delete {
                range: Range::new(cursor, word_end),
                deleted_text: self
                    .editor
                    .state()
                    .document
                    .slice(Range::new(cursor, word_end)),
            };
            // track_and_apply records the edit span, refreshes folds, and
            // resets blink; the edit invalidates sticky columns.
            self.keyboard_handler.reset_vertical_state();
            self.track_and_apply(cmd);
        }
    }

    /// Deletes each caret's selection, or the text back to the start of its own
    /// line (Cmd+Backspace on Mac).
    ///
    /// Returns whether content changed: `false` in read-only mode or when no
    /// caret has anything to remove.
    #[wasm_bindgen(js_name = deleteToLineStart)]
    pub fn delete_to_line_start(&mut self) -> bool {
        self.run_editing_command(builtin::EDIT_DELETE_TO_LINE_START.as_str())
    }

    /// Deletes each caret's selection, or the text through to the end of its own
    /// line (Cmd+Delete on Mac).
    ///
    /// Returns whether content changed: `false` in read-only mode or when no
    /// caret has anything to remove.
    #[wasm_bindgen(js_name = deleteToLineEnd)]
    pub fn delete_to_line_end(&mut self) -> bool {
        self.run_editing_command(builtin::EDIT_DELETE_TO_LINE_END.as_str())
    }

    /// Runs a kernel editing command by id and reports whether it changed the
    /// document.
    ///
    /// These two verbs have no place in the platform-neutral default keymap —
    /// `Ctrl+Backspace` and `Ctrl+Delete` are already word-wise delete — so this
    /// face reaches them by id from its own macOS `Cmd` handling. Routing
    /// through `keyboard_handler` rather than reimplementing the edit is what
    /// makes them multi-cursor: they were hand-written here against the primary
    /// caret alone, which both spared the other carets' lines and collapsed the
    /// multi-cursor state, and no native build compiles this file to catch it.
    fn run_editing_command(&mut self, id: &str) -> bool {
        if self.read_only {
            return false;
        }
        // `keyboard_handler` and `editor` are disjoint fields, so the mutable
        // handler borrow coexists with the immutable state borrows.
        let state = self.editor.state();
        let outcome = self.keyboard_handler.run_command(
            id,
            CommandArgs::NONE,
            &state.document,
            &state.cursor,
            &state.history,
            &state.config,
        );
        let Ok(KeyResult::Command(command)) = outcome else {
            // `Handled` means every caret produced an empty edit; an error can
            // only mean the id left the registry, which the kernel's own tests
            // would have caught first. Neither changed the document.
            return false;
        };
        let changed = command.modifies_content();
        // The caret moves outside handle_key, so the sticky vertical column
        // this handler keeps must be dropped.
        self.keyboard_handler.reset_vertical_state();
        // track_and_apply records the edit span for takeLastEdit, refreshes
        // fold regions, and resets blink.
        self.track_and_apply(command);
        changed
    }

    // ==========================================================================
    // Selection Methods
    // ==========================================================================

    /// Returns true if *any* caret has a non-empty selection.
    ///
    /// Any, not the primary's: with three carets selecting words and the
    /// primary collapsed, there is plainly a selection, and a host that gated
    /// its copy button on the primary alone would grey it out over three
    /// selected words.
    #[wasm_bindgen(js_name = hasSelection)]
    pub fn has_selection(&self) -> bool {
        self.editor
            .state()
            .cursor
            .all_selections()
            .any(|sel| !sel.is_collapsed())
    }

    /// How many carets there are — `1` unless multi-cursor is in play.
    ///
    /// Exists so the number of cursors is *observable* from the host. This face
    /// drew only the primary caret for its whole life, and nothing outside the
    /// kernel could contradict it: every export here reports the primary and no
    /// count was published, so a document with four cursors looked exactly like
    /// a document with one. A status bar reading this makes that class of defect
    /// visible instead of silent.
    #[wasm_bindgen(js_name = cursorCount)]
    pub fn cursor_count(&self) -> u32 {
        u32::try_from(self.editor.state().cursor.all_selections().count()).unwrap_or(u32::MAX)
    }

    /// Gets the selected text across **every** caret, or an empty string when
    /// nothing is selected.
    ///
    /// Multiple selections are joined with the document's line ending, in
    /// document order — the same shape `copyText` produces, because "what is
    /// selected" and "what a copy would put on the clipboard" must not be two
    /// different answers.
    #[wasm_bindgen(js_name = getSelectedText)]
    pub fn get_selected_text(&self) -> String {
        let state = self.editor.state();
        if !self.has_selection() {
            return String::new();
        }
        let mut selections: Vec<_> = state.cursor.all_selections().copied().collect();
        selections.sort_by_key(iridium_editor::document::Selection::start);
        let line_ending = state.document.line_ending().as_str();
        selections
            .iter()
            .map(|sel| state.document.slice(sel.range()))
            .collect::<Vec<_>>()
            .join(line_ending)
    }

    /// Gets the **primary** selection's start line (for JS interop).
    ///
    /// These four accessors are singular by design: they answer "where is the
    /// selection?", and with several the only coherent singular answer is the
    /// primary's. A host that needs the others reads `cursorCount` first and
    /// then works with whole selections rather than four loose numbers.
    #[wasm_bindgen(js_name = getSelectionStartLine)]
    pub fn get_selection_start_line(&self) -> u32 {
        self.editor.state().cursor.primary.start().line as u32
    }

    /// Gets the primary selection's start column (for JS interop).
    #[wasm_bindgen(js_name = getSelectionStartColumn)]
    pub fn get_selection_start_column(&self) -> u32 {
        self.editor.state().cursor.primary.start().column as u32
    }

    /// Gets the primary selection's end line (for JS interop).
    #[wasm_bindgen(js_name = getSelectionEndLine)]
    pub fn get_selection_end_line(&self) -> u32 {
        self.editor.state().cursor.primary.end().line as u32
    }

    /// Gets the primary selection's end column (for JS interop).
    #[wasm_bindgen(js_name = getSelectionEndColumn)]
    pub fn get_selection_end_column(&self) -> u32 {
        self.editor.state().cursor.primary.end().column as u32
    }

    /// Drops every secondary caret and collapses the primary selection to its
    /// head — the same thing `Escape` does through the keyboard.
    ///
    /// Collapsing to *one* caret rather than clearing each caret's selection in
    /// place is deliberate and matches the kernel verb this now routes to: a
    /// host calling "clear the selection" wants the editor back to a plain
    /// caret, and leaving three carets behind would not be that.
    #[wasm_bindgen(js_name = clearSelection)]
    pub fn clear_selection(&mut self) {
        self.run_selection_command(builtin::SELECTION_COLLAPSE_TO_PRIMARY.as_str());
    }

    /// Sets the selection (for internal use).
    fn set_selection_internal(&mut self, anchor: Position, head: Position) {
        // Editor::set_selection clamps both endpoints and resets the core's
        // keyboard handler; the binding's handler (which drives
        // handleKeyEvent) bypasses handle_key here too, so its sticky
        // vertical columns must be dropped as well.
        self.keyboard_handler.reset_vertical_state();
        self.editor.set_selection(anchor, head);
        self.cursor_renderer.reset_blink();
        self.needs_redraw = true;
    }

    /// Extends every selection left by one character (Shift+Left).
    #[wasm_bindgen(js_name = extendSelectionLeft)]
    pub fn extend_selection_left(&mut self) {
        self.run_selection_command(builtin::CURSOR_CHAR_LEFT_SELECT.as_str());
    }

    /// Extends every selection right by one character (Shift+Right).
    #[wasm_bindgen(js_name = extendSelectionRight)]
    pub fn extend_selection_right(&mut self) {
        self.run_selection_command(builtin::CURSOR_CHAR_RIGHT_SELECT.as_str());
    }

    /// Extends every selection up by one line (Shift+Up).
    #[wasm_bindgen(js_name = extendSelectionUp)]
    pub fn extend_selection_up(&mut self) {
        self.run_selection_command(builtin::CURSOR_LINE_UP_SELECT.as_str());
    }

    /// Extends every selection down by one line (Shift+Down).
    #[wasm_bindgen(js_name = extendSelectionDown)]
    pub fn extend_selection_down(&mut self) {
        self.run_selection_command(builtin::CURSOR_LINE_DOWN_SELECT.as_str());
    }

    /// Extends every selection to the previous word boundary
    /// (Shift+Option+Left).
    #[wasm_bindgen(js_name = extendSelectionWordLeft)]
    pub fn extend_selection_word_left(&mut self) {
        self.run_selection_command(builtin::CURSOR_WORD_LEFT_SELECT.as_str());
    }

    /// Extends every selection to the next word boundary (Shift+Option+Right).
    #[wasm_bindgen(js_name = extendSelectionWordRight)]
    pub fn extend_selection_word_right(&mut self) {
        self.run_selection_command(builtin::CURSOR_WORD_RIGHT_SELECT.as_str());
    }

    /// Extends every selection to its line start (Shift+Cmd+Left or
    /// Shift+Home).
    ///
    /// This is *smart* home, matching what the same chord does through the
    /// keyboard: the first stop is the first non-whitespace character, and
    /// column zero only from there. The hand-rolled version this replaced went
    /// straight to column zero, so the mouse-driven and key-driven routes
    /// disagreed about where a line starts.
    #[wasm_bindgen(js_name = extendSelectionLineStart)]
    pub fn extend_selection_line_start(&mut self) {
        self.run_selection_command(builtin::CURSOR_LINE_START_SELECT.as_str());
    }

    /// Extends every selection to its line end (Shift+Cmd+Right or Shift+End).
    #[wasm_bindgen(js_name = extendSelectionLineEnd)]
    pub fn extend_selection_line_end(&mut self) {
        self.run_selection_command(builtin::CURSOR_LINE_END_SELECT.as_str());
    }

    /// Extends the selection to the document start (Shift+Cmd+Up), merging
    /// every caret into one.
    #[wasm_bindgen(js_name = extendSelectionDocStart)]
    pub fn extend_selection_doc_start(&mut self) {
        self.run_selection_command(builtin::CURSOR_DOCUMENT_START_SELECT.as_str());
    }

    /// Extends the selection to the document end (Shift+Cmd+Down), merging
    /// every caret into one.
    #[wasm_bindgen(js_name = extendSelectionDocEnd)]
    pub fn extend_selection_doc_end(&mut self) {
        self.run_selection_command(builtin::CURSOR_DOCUMENT_END_SELECT.as_str());
    }

    /// Selects all text (Cmd+A).
    #[wasm_bindgen(js_name = selectAll)]
    pub fn select_all(&mut self) {
        self.run_selection_command(builtin::SELECTION_SELECT_ALL.as_str());
    }

    /// Runs a kernel selection command by id.
    ///
    /// Every method above used to compute its own motion against
    /// `cursor.primary` and hand the result to `Editor::set_selection`, which
    /// *replaces all cursors with a single selection* — so a host-driven
    /// Shift+Left both moved one caret and destroyed the rest. The kernel
    /// already owns each of these motions, multi-cursor and sticky-column
    /// correct, and already runs them for the identical keystroke; this routes
    /// to that one implementation so the two entry points cannot disagree.
    ///
    /// Not gated on read-only: moving a selection changes no text, and the
    /// keyboard path does not gate it either.
    fn run_selection_command(&mut self, id: &str) {
        // `keyboard_handler` and `editor` are disjoint fields, so the mutable
        // handler borrow coexists with the immutable state borrows.
        let state = self.editor.state();
        let outcome = self.keyboard_handler.run_command(
            id,
            CommandArgs::NONE,
            &state.document,
            &state.cursor,
            &state.history,
            &state.config,
        );
        // A motion at the document edge legitimately produces `Handled` and no
        // command; an error can only mean the id left the registry, which the
        // kernel's own tests would have caught first.
        if let Ok(KeyResult::Command(command)) = outcome {
            self.track_and_apply(command);
        }
        self.cursor_renderer.reset_blink();
        self.needs_redraw = true;
    }

    /// Sets cursor position for click (collapses selection to clicked position).
    #[wasm_bindgen(js_name = setCursorFromClick)]
    pub fn set_cursor_from_click(&mut self, line: u32, column: u32) {
        let pos = Position::new(line as usize, column as usize);
        // set_selection_internal handles clamping
        self.set_selection_internal(pos, pos);
    }

    /// Extends selection from anchor to clicked position (for drag selection).
    #[wasm_bindgen(js_name = extendSelectionToPosition)]
    pub fn extend_selection_to_position(&mut self, line: u32, column: u32) {
        let anchor = self.editor.state().cursor.primary.anchor;
        let head = Position::new(line as usize, column as usize);
        self.set_selection_internal(anchor, head);
    }

    /// Starts a selection at the given position (for drag start).
    #[wasm_bindgen(js_name = startSelectionAt)]
    pub fn start_selection_at(&mut self, line: u32, column: u32) {
        let pos = Position::new(line as usize, column as usize);
        let clamped = self.editor.state().document.clamp_position(pos);
        self.set_selection_internal(clamped, clamped);
    }

    /// Gets the line height in pixels.
    #[wasm_bindgen(js_name = getLineHeight)]
    pub fn get_line_height(&self) -> f32 {
        self.text_renderer.line_height()
    }

    /// Gets the character width in pixels (for monospace).
    #[wasm_bindgen(js_name = getCharWidth)]
    pub fn get_char_width(&self) -> f32 {
        self.cached_char_width
    }

    /// Calculates the current gutter width.
    fn current_gutter_width(&self) -> f32 {
        if self.gutter_enabled {
            let char_width = self.cached_char_width;
            let line_count = self.editor.state().document.line_count();
            self.gutter_renderer.calculate_width(line_count, char_width)
        } else {
            0.0
        }
    }

    /// Gets the text padding/offset from left edge (including gutter).
    #[wasm_bindgen(js_name = getTextOffsetX)]
    pub fn get_text_offset_x(&self) -> f32 {
        self.current_gutter_width() + 10.0
    }

    /// Gets the text padding/offset from top edge.
    #[wasm_bindgen(js_name = getTextOffsetY)]
    pub fn get_text_offset_y(&self) -> f32 {
        10.0
    }

    /// Returns whether the gutter is enabled.
    #[wasm_bindgen(js_name = isGutterEnabled)]
    pub fn is_gutter_enabled(&self) -> bool {
        self.gutter_enabled
    }

    /// Enables or disables the gutter (line numbers).
    #[wasm_bindgen(js_name = setGutterEnabled)]
    pub fn set_gutter_enabled(&mut self, enabled: bool) {
        self.gutter_enabled = enabled;
        self.needs_redraw = true;
    }

    /// Converts pixel coordinates to line/column position.
    /// Returns [line, column]. Accounts for folded lines, scroll, and line wrapping.
    ///
    /// Uses the cached visual line map built during `render_frame` to correctly
    /// resolve clicks on wrapped lines. Each visual line in the buffer maps to
    /// a (buffer_line_index, run_start_column) pair, allowing accurate column
    /// calculation even when a single document line spans multiple visual rows.
    #[wasm_bindgen(js_name = pixelToPosition)]
    pub fn pixel_to_position(&self, x: f32, y: f32) -> Vec<u32> {
        let line_height = self.text_renderer.line_height();
        let char_width = self.cached_char_width;
        let padding = 10.0_f32;

        // Calculate the text area top offset (must match render_frame positioning)
        let virtual_scroll_offset = self.cached_map_viewport_start as f32 * line_height;
        let adjusted_scroll_y = self.scroll_y - virtual_scroll_offset;
        let text_area_top = padding - adjusted_scroll_y;

        // Calculate which visual line in the buffer was clicked
        let y_in_buffer = y - text_area_top;
        let visual_line_in_buffer = (y_in_buffer / line_height).floor().max(0.0) as usize;

        let doc = &self.editor.state().document;
        let line_count = doc.line_count();

        // Use the cached visual line map to resolve the click position
        if !self.cached_visual_line_map.is_empty() {
            // Clamp to last visual line in the map
            let clamped_visual =
                visual_line_in_buffer.min(self.cached_visual_line_map.len().saturating_sub(1));
            let (buffer_line_idx, run_start_col) = self.cached_visual_line_map[clamped_visual];

            // Map buffer line index to document line
            let doc_line = self
                .cpu_visible_doc_lines
                .get(buffer_line_idx)
                .copied()
                .unwrap_or(0)
                .min(line_count.saturating_sub(1));

            // Calculate column within this wrap segment
            let col_in_run =
                ((x - self.cached_content_offset_x) / char_width + 0.5).max(0.0) as usize;
            let column = run_start_col + col_in_run;

            // Clamp to actual line length
            let line_len = doc.line(doc_line).map(|l| l.chars().count()).unwrap_or(0);
            let clamped_column = column.min(line_len);

            vec![doc_line as u32, clamped_column as u32]
        } else {
            // Fallback: no cached map (before first render), use simple calculation
            let visual_line = ((y + self.scroll_y - padding) / line_height).max(0.0) as usize;
            let doc_line = self
                .fold_state
                .visual_to_document_line(visual_line)
                .min(line_count.saturating_sub(1));

            let offset_x = self.current_gutter_width() + padding;
            let line_len = doc.line(doc_line).map(|l| l.chars().count()).unwrap_or(0);
            let column = ((x - offset_x) / char_width + 0.5).max(0.0) as usize;
            let clamped_column = column.min(line_len);

            vec![doc_line as u32, clamped_column as u32]
        }
    }

    /// Converts a document line/column to pixel coordinates.
    /// Returns [x, y] in physical pixels. Returns [-1.0, -1.0] if the line is
    /// hidden (folded or off-screen).
    ///
    /// This is the inverse of `pixelToPosition`. It uses the same cached visual
    /// line map built during `render_frame` to correctly handle word wrapping
    /// and code folding.
    #[wasm_bindgen(js_name = positionToPixel)]
    pub fn position_to_pixel(&self, doc_line: u32, column: u32) -> Vec<f32> {
        let line_height = self.text_renderer.line_height();
        let char_width = self.cached_char_width;
        let padding = 10.0_f32;
        let doc_line = doc_line as usize;
        let column = column as usize;

        // Check if line is folded (hidden)
        if self.fold_state.is_line_hidden(doc_line) {
            return vec![-1.0, -1.0];
        }

        // Use the cached visual line map if available (after first render)
        if !self.cached_visual_line_map.is_empty() {
            // Find the buffer line index for this document line
            let buffer_line_idx = self
                .cpu_visible_doc_lines
                .iter()
                .position(|&dl| dl == doc_line);

            if let Some(buf_idx) = buffer_line_idx {
                // Find the visual line that contains this column
                // (handles word wrap — a single document line may span multiple visual lines)
                let mut target_visual = None;
                let mut col_in_segment = column;

                for (visual_idx, &(bi, run_start)) in self.cached_visual_line_map.iter().enumerate()
                {
                    if bi == buf_idx {
                        // Check if this is the last segment for this buffer line
                        let next_run_start = self
                            .cached_visual_line_map
                            .get(visual_idx + 1)
                            .filter(|&&(next_bi, _)| next_bi == buf_idx)
                            .map(|&(_, start)| start);

                        if let Some(next_start) = next_run_start {
                            if column >= run_start && column < next_start {
                                target_visual = Some(visual_idx);
                                col_in_segment = column - run_start;
                                break;
                            }
                        } else {
                            // Last (or only) segment — column must be here
                            if column >= run_start {
                                target_visual = Some(visual_idx);
                                col_in_segment = column - run_start;
                                break;
                            }
                        }
                    }
                }

                if let Some(visual_idx) = target_visual {
                    let virtual_scroll_offset = self.cached_map_viewport_start as f32 * line_height;
                    let x = self.cached_content_offset_x + (col_in_segment as f32 * char_width);
                    let y = padding + (visual_idx as f32 * line_height) + virtual_scroll_offset
                        - self.scroll_y;
                    return vec![x, y];
                }
            }

            // Document line not in current viewport
            return vec![-1.0, -1.0];
        }

        // Fallback: before first render, use simple calculation (no word wrap)
        let visual_line = self.fold_state.document_to_visual_line(doc_line);
        match visual_line {
            Some(vl) => {
                let offset_x = self.current_gutter_width() + padding;
                let x = offset_x + (column as f32 * char_width);
                let y = padding + (vl as f32 * line_height) - self.scroll_y;
                vec![x, y]
            },
            None => vec![-1.0, -1.0],
        }
    }

    // ==========================================================================
    // Code Folding Methods
    // ==========================================================================

    /// Updates fold regions based on current content.
    /// Call this after any text changes if you want fold regions to update.
    #[wasm_bindgen(js_name = updateFolds)]
    pub fn update_folds(&mut self) {
        let content = self.editor.content();
        self.refresh_fold_regions(&content);
        self.needs_redraw = true;
    }

    /// Returns all foldable line numbers (start lines of fold regions).
    #[wasm_bindgen(js_name = getFoldableLines)]
    pub fn get_foldable_lines(&self) -> Vec<u32> {
        self.fold_state
            .regions()
            .iter()
            .map(|r| r.start_line as u32)
            .collect()
    }

    /// Returns true if the given line is in a foldable region.
    #[wasm_bindgen(js_name = isFoldable)]
    pub fn is_foldable(&self, line: u32) -> bool {
        self.fold_state.is_in_foldable_region(line as usize)
    }

    /// Returns true if the given line is currently folded.
    #[wasm_bindgen(js_name = isFolded)]
    pub fn is_folded(&self, line: u32) -> bool {
        self.fold_state.is_folded(line as usize)
    }

    /// Returns true if the given line is visible (not hidden by a fold).
    #[wasm_bindgen(js_name = isLineVisible)]
    pub fn is_line_visible(&self, line: u32) -> bool {
        !self.fold_state.is_line_hidden(line as usize)
    }

    /// Toggles the fold state at or containing the given line.
    /// If the line is inside a foldable region, toggles that region.
    /// Returns true if the fold state was changed.
    #[wasm_bindgen(js_name = toggleFold)]
    pub fn toggle_fold(&mut self, line: u32) -> bool {
        // First try to toggle a fold starting at this line
        if self.fold_state.toggle_fold_at(line as usize) {
            self.needs_redraw = true;
            return true;
        }
        // Otherwise try to toggle the containing region
        if self
            .fold_state
            .toggle_fold_containing(line as usize)
            .is_some()
        {
            self.needs_redraw = true;
            return true;
        }
        false
    }

    /// Folds the region at the given line.
    /// Returns true if the region was folded.
    #[wasm_bindgen(js_name = foldAt)]
    pub fn fold_at(&mut self, line: u32) -> bool {
        let result = self.fold_state.fold_at(line as usize);
        if result {
            self.needs_redraw = true;
        }
        result
    }

    /// Unfolds the region at the given line.
    /// Returns true if the region was unfolded.
    #[wasm_bindgen(js_name = unfoldAt)]
    pub fn unfold_at(&mut self, line: u32) -> bool {
        let result = self.fold_state.unfold_at(line as usize);
        if result {
            self.needs_redraw = true;
        }
        result
    }

    /// Folds all foldable regions.
    #[wasm_bindgen(js_name = foldAll)]
    pub fn fold_all(&mut self) {
        self.fold_state.fold_all();
        self.needs_redraw = true;
    }

    /// Unfolds all folded regions.
    #[wasm_bindgen(js_name = unfoldAll)]
    pub fn unfold_all(&mut self) {
        self.fold_state.unfold_all();
        self.needs_redraw = true;
    }

    /// Returns all currently folded line numbers.
    #[wasm_bindgen(js_name = getFoldedLines)]
    pub fn get_folded_lines(&self) -> Vec<u32> {
        self.fold_state.folded_lines().map(|l| l as u32).collect()
    }

    /// Returns the number of hidden lines due to folding.
    #[wasm_bindgen(js_name = getHiddenLineCount)]
    pub fn get_hidden_line_count(&self) -> u32 {
        self.fold_state.hidden_line_count() as u32
    }

    /// Returns the number of visible lines (total minus hidden).
    #[wasm_bindgen(js_name = getVisibleLineCount)]
    pub fn get_visible_line_count(&self) -> u32 {
        let total = self.editor.state().document.line_count();
        self.fold_state.visible_line_count(total) as u32
    }

    /// Gets the end line of a fold region starting at the given line.
    /// Returns None (as -1) if no fold region exists at that line.
    #[wasm_bindgen(js_name = getFoldEndLine)]
    pub fn get_fold_end_line(&self, start_line: u32) -> i32 {
        self.fold_state
            .region_at(start_line as usize)
            .map(|r| r.end_line as i32)
            .unwrap_or(-1)
    }
}

/// Check if WebGPU is supported in this browser.
#[wasm_bindgen(js_name = isWebGPUSupported)]
pub async fn is_webgpu_supported() -> bool {
    use wgpu::{Backends, Instance, InstanceDescriptor, InstanceFlags};

    let instance = Instance::new(&InstanceDescriptor {
        backends: Backends::BROWSER_WEBGPU,
        flags: InstanceFlags::default(),
        ..Default::default()
    });

    instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .is_ok()
}

/// Fold bookkeeping that is not part of the JavaScript surface.
///
/// A private method cannot live in a `#[wasm_bindgen]` impl, so it lives here.
impl WebEditor {
    /// Reparses `content` and refreshes the fold regions from the result.
    ///
    /// The fold detector borrows a tree rather than owning one, so every path
    /// that changes the document comes through here — one place that knows how
    /// folds are recomputed, rather than five that each remember to.
    fn refresh_fold_regions(&mut self, content: &str) -> bool {
        let Some(tree) = self.fold_tree.parse(content) else {
            return false;
        };
        self.fold_state.update_regions(tree, content)
    }
}
