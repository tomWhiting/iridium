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
use crate::web_folds::WebFoldSyntax;
use crate::web_span_index::{WebSpan, WebSpanIndex};
use iridium_editor::{
    CommandArgs, CommandId, Document, EditorConfig, Keymap, ModifierPattern, Position, Range,
    StrokePattern,
    commands::{builtin, palette::CommandMru},
    editor::{Editor, FoldState},
    history::{Command, UndoNodeId},
    input::{
        ClipboardOperation, CommandRunError, HistoryRequest, KeyCode, KeyEvent, KeyResult,
        KeyboardHandler, Modifiers, SearchAction,
    },
    render::{
        FrameCompositor, FrameTarget, HighlightContext, HighlightSource, Viewport, WebSurface,
        units::{pixel_to_index, u32_to_f32},
    },
    syntax_stubs::Language,
    theme::Color,
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

/// WebEditor provides a complete browser-based editor experience.
///
/// This wraps the Iridium editor with WebGPU rendering capabilities
/// for use in web browsers.
#[wasm_bindgen]
pub struct WebEditor {
    editor: Editor,
    surface: WebSurface,
    /// The kernel's per-frame composition: owns the renderers, the layout
    /// caches hit-testing reads between frames, and the presentation inputs
    /// (theme, line backgrounds, gutter changes, blame). This face keeps only
    /// what is genuinely web-shaped — the canvas surface, the scroll offset,
    /// the tree-sitter span store — and drives `compose` once per frame.
    compositor: FrameCompositor,
    needs_redraw: bool,
    /// Code folding state
    fold_state: FoldState,
    /// The parse state the fold regions are read from.
    ///
    /// See [`WebFoldSyntax`]: it is what makes a fold refresh cost the edit
    /// rather than the document.
    fold_syntax: WebFoldSyntax,
    /// Vertical scroll offset in pixels
    scroll_y: f32,
    /// Tree-sitter highlight spans from JavaScript (when available)
    ts_highlights: Vec<JsHighlightSpan>,
    /// Whether to use tree-sitter highlights from JS
    use_ts_highlights: bool,
    /// Span index for efficient viewport-based queries (O(log n + k))
    span_index: WebSpanIndex,
    /// The highlight generation reported to the compositor's
    /// retained-shaping key: bumped by every mutation of the span state
    /// above (worker spans arriving, spans cleared, spans shifted across an
    /// edit), per the [`HighlightSource::generation`] contract — a span
    /// change without a bump would leave stale colours on a retained frame.
    highlight_generation: u64,

    // =========================================================================
    // Git integration: read-only gating (presentation maps live on the
    // compositor)
    // =========================================================================
    /// When true, all content-mutating operations (insert, delete, etc.) are no-ops.
    read_only: bool,

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

    // Create the compositor (text renderer, quad renderers, theme, caches)
    // with scaled font size for HiDPI
    log("[Iridium] Creating FrameCompositor...");
    let mut compositor = FrameCompositor::new(
        surface.device(),
        surface.queue(),
        surface.format(),
        width,
        height,
    )
    .map_err(|e| {
        let msg = format!("[Iridium] FrameCompositor error: {}", e);
        log(&msg);
        JsValue::from_str(&msg)
    })?;

    // Scale font size for HiDPI displays
    let base_font_size = 14.0;
    let scaled_font_size = base_font_size * pixel_ratio;
    compositor.set_font_size(scaled_font_size);
    log(&format!(
        "[Iridium] Font size: {} (base {} * ratio {})",
        scaled_font_size, base_font_size, pixel_ratio
    ));
    log("[Iridium] FrameCompositor created");

    // Create editor with default config
    let editor = Editor::new(EditorConfig::default());
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
        compositor,
        needs_redraw: true,
        fold_state,
        fold_syntax: WebFoldSyntax::new(Language::C),
        scroll_y: 0.0,
        ts_highlights: Vec::new(),
        use_ts_highlights: false,
        span_index: WebSpanIndex::empty(),
        highlight_generation: 0,
        // Git integration fields
        read_only: false,
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
        // The compositor measures the actual character width from the loaded font.
        self.compositor.load_font(data.to_vec());
        log(&format!(
            "[Iridium] Font loaded, char_width: {:.2}px",
            self.compositor.char_width()
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
        // The document was replaced, not edited: a revision comparison cannot
        // see that on its own, because a fresh document starts counting again.
        self.fold_syntax.invalidate();
        self.refresh_fold_regions();
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
                self.compositor.reset_blink();
                self.needs_redraw = true;
                "handled".to_string()
            },
            KeyResult::Command(cmd) => self.apply_key_command(cmd),
            KeyResult::Clipboard(operation) => self.apply_clipboard_result(operation),
            KeyResult::Search(action) => self.apply_search_result(&action),
            KeyResult::History(request) => self.apply_history_result(request),
            // Routed to the kernel rather than answered here, even though this
            // build has no tree for it to read: `syntax` is off for wasm32, so
            // `perform_ast_request` returns `false` and the key does nothing.
            // Calling it anyway is what makes turning the feature on the only
            // change needed, instead of a second implementation living here.
            KeyResult::Ast(request) => {
                if self.editor.perform_ast_request(request) {
                    self.compositor.reset_blink();
                    self.needs_redraw = true;
                }
                "handled".to_string()
            },
            // A binding named a command the kernel does not implement — a host
            // command. The key was consumed and the id is reported so the host can
            // run it; the caller reads `takePendingHostCommand` for the id.
            KeyResult::HostCommand { command, args } => {
                self.pending_host_command = Some(HostCommandRequest {
                    command: command.as_str().to_owned(),
                    count: args.count(),
                    captures: args.captures().to_vec(),
                });
                self.compositor.reset_blink();
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
                self.compositor.reset_blink();
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
        self.fold_syntax
            .note_edit(&self.editor.state().document, &span);
        self.refresh_fold_regions();
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
                self.compositor.reset_blink();
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
                self.compositor.reset_blink();
                self.needs_redraw = true;
                self.ensure_cursor_visible();
                "search:next".to_string()
            },
            SearchAction::PreviousMatch => {
                self.keyboard_handler.reset_vertical_state();
                self.editor.goto_previous_match();
                self.compositor.reset_blink();
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
                Ok(Some(span)) => {
                    self.record_edit(span);
                    self.fold_syntax
                        .note_edit(&self.editor.state().document, &span);
                },
                // Never report a wrong span: unresolvable positions degrade
                // the pending edit to a full reparse, and leave the fold state
                // to notice the revision it was never told about.
                Ok(None) | Err(EditSpanError) => self.pending_edit = PendingEdit::Degraded,
            }
            self.refresh_fold_regions();
        }

        self.compositor.reset_blink();
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
        // The spans moved (and some may have been dropped): a subsequent
        // resolution answers differently for identical content, so the
        // generation moves with them. The edit that triggered this shift
        // already misses the retained-shape key on the document revision,
        // but the generation contract is about the resolver's answer, not
        // about who else happens to invalidate the frame.
        self.highlight_generation = self.highlight_generation.wrapping_add(1);
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
        use js_sys::{Array, Reflect};

        self.compositor.line_backgrounds_mut().clear();

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
            self.compositor.line_backgrounds_mut().insert(line, color);
        }

        self.needs_redraw = true;
        Ok(())
    }

    /// Clears all per-line background colors.
    #[wasm_bindgen(js_name = clearLineBackgrounds)]
    pub fn clear_line_backgrounds(&mut self) {
        self.compositor.line_backgrounds_mut().clear();
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

        self.compositor.gutter_changes_mut().clear();

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
                "added" => self.compositor.theme().editor.change_added,
                "modified" => self.compositor.theme().editor.change_modified,
                "deleted" => self.compositor.theme().editor.change_deleted,
                "error" => self.compositor.theme().editor.diagnostic_error,
                "warning" => self.compositor.theme().editor.diagnostic_warning,
                "info" => self.compositor.theme().editor.diagnostic_info,
                "hint" => self.compositor.theme().editor.diagnostic_hint,
                _ => {
                    return Err(JsValue::from_str(&format!(
                        "unknown change kind: {} (expected added/modified/deleted/error/warning/info/hint)",
                        kind
                    )));
                },
            };
            self.compositor.gutter_changes_mut().insert(line, color);
        }

        self.needs_redraw = true;
        Ok(())
    }

    /// Clears all gutter change indicators.
    #[wasm_bindgen(js_name = clearGutterChanges)]
    pub fn clear_gutter_changes(&mut self) {
        self.compositor.gutter_changes_mut().clear();
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
            self.compositor.set_custom_gutter_lines(None);
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
            self.compositor.set_custom_gutter_lines(Some(result));
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

        self.compositor.blame_data_mut().clear();

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
            self.compositor.blame_data_mut().insert(line, text);
        }

        self.needs_redraw = true;
        Ok(())
    }

    /// Clears all blame data.
    #[wasm_bindgen(js_name = clearBlameData)]
    pub fn clear_blame_data(&mut self) {
        self.compositor.blame_data_mut().clear();
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
        self.compositor.resize(self.surface.queue(), width, height);
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
    /// Uses the compositor's wrap-aware visual line total from the last
    /// composed frame to account for word-wrapped lines that occupy multiple
    /// visual rows.
    #[wasm_bindgen(js_name = getMaxScrollY)]
    pub fn max_scroll_y(&self) -> f32 {
        self.compositor.max_scroll_y(
            &self.editor,
            &self.fold_state,
            u32_to_f32(self.surface.height()),
        )
    }

    /// Ensures the cursor is visible by scrolling if needed.
    ///
    /// The cursor's document-space Y comes from the compositor (cached and
    /// wrap-aware when the cursor line hasn't changed, fold-only estimation
    /// when it has); the scroll decision itself stays on this face, which
    /// owns `scroll_y`.
    #[wasm_bindgen(js_name = ensureCursorVisible)]
    pub fn ensure_cursor_visible(&mut self) {
        let line_height = self.compositor.line_height();
        let padding = 10.0;
        let viewport_height = u32_to_f32(self.surface.height());

        let cursor_y = self
            .compositor
            .cursor_anchor_y(&self.editor, &self.fold_state);

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
    ///
    /// The per-frame composition itself lives in the kernel's
    /// [`FrameCompositor`]; this face contributes exactly what is web-shaped —
    /// the canvas surface's frame acquisition, the scroll offset it owns, the
    /// tree-sitter span resolver, and the frame timer whose slow-frame warning
    /// goes to the browser console.
    fn render_frame(&mut self) -> Result<(), JsValue> {
        use web_time::Instant;

        // T042: Track frame time for performance monitoring
        let frame_start = Instant::now();

        let width = self.surface.width();
        let height = self.surface.height();

        // The highlight seam: tree-sitter spans arrive from JavaScript and
        // stay on this face; the compositor asks this resolver for them once
        // per frame.
        let mut highlights = WebHighlightSource {
            span_index: &self.span_index,
            ts_highlights: &self.ts_highlights,
            use_ts_highlights: self.use_ts_highlights,
            document: &self.editor.state().document,
            fold_state: &self.fold_state,
            scroll_y: self.scroll_y,
            surface_height: u32_to_f32(height),
            generation: self.highlight_generation,
        };

        let compositor = &mut self.compositor;
        let editor = &self.editor;
        let fold_state = &self.fold_state;
        let scroll_y = self.scroll_y;
        self.surface
            .render_frame(|view, device, queue| {
                compositor.compose(
                    editor,
                    fold_state,
                    scroll_y,
                    &mut highlights,
                    FrameTarget {
                        view,
                        device,
                        queue,
                        width,
                        height,
                    },
                )
            })
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

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
        self.compositor.set_dark_theme(dark);
        self.needs_redraw = true;
    }

    /// Enables or disables syntax highlighting.
    ///
    /// This is also this face's no-language channel: the grammar lives in
    /// the host's worker, so a host opening a document with no language
    /// disables syntax here and the frame renders uniform foreground text.
    /// While enabled, frames without worker spans are bridged by the
    /// compositor's built-in keyword highlighter.
    #[wasm_bindgen(js_name = setSyntaxEnabled)]
    pub fn set_syntax_enabled(&mut self, enabled: bool) {
        self.compositor.set_syntax_enabled(enabled);
        self.needs_redraw = true;
    }

    /// Returns whether syntax highlighting is enabled.
    #[wasm_bindgen(js_name = isSyntaxEnabled)]
    pub fn is_syntax_enabled(&self) -> bool {
        self.compositor.syntax_enabled()
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
        // New spans mean a new resolution answer: move the generation so the
        // compositor's retained shapes recolour.
        self.highlight_generation = self.highlight_generation.wrapping_add(1);
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
        use js_sys::{Object, Reflect};

        self.compositor.syntax_theme_mut().clear();

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
            self.compositor.syntax_theme_mut().insert(key_str, color);
        }

        self.needs_redraw = true;
        Ok(())
    }

    /// Clears tree-sitter highlights and falls back to simple highlighting.
    #[wasm_bindgen(js_name = clearTreeSitterHighlights)]
    pub fn clear_tree_sitter_highlights(&mut self) {
        self.ts_highlights.clear();
        self.use_ts_highlights = false;
        // Clearing changes the resolution answer as surely as new spans do —
        // the span-clearing half of the generation contract.
        self.highlight_generation = self.highlight_generation.wrapping_add(1);
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
            // A traversal moves the document by an amount this surface never
            // measured, so there is no honest span to report; guessing one
            // would cost correctness rather than speed.
            self.fold_syntax.invalidate();
            self.refresh_fold_regions();
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
        self.compositor.reset_blink();
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
        self.compositor.reset_blink();
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
        self.compositor.line_height()
    }

    /// Gets the character width in pixels (for monospace).
    #[wasm_bindgen(js_name = getCharWidth)]
    pub fn get_char_width(&self) -> f32 {
        self.compositor.char_width()
    }

    /// Calculates the current gutter width.
    fn current_gutter_width(&self) -> f32 {
        self.compositor
            .gutter_width(self.editor.state().document.line_count())
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
        self.compositor.gutter_enabled()
    }

    /// Enables or disables the gutter (line numbers).
    #[wasm_bindgen(js_name = setGutterEnabled)]
    pub fn set_gutter_enabled(&mut self, enabled: bool) {
        self.compositor.set_gutter_enabled(enabled);
        self.needs_redraw = true;
    }

    /// Converts pixel coordinates to line/column position.
    /// Returns [line, column]. Accounts for folded lines, scroll, and line wrapping.
    ///
    /// The resolution itself lives on the compositor, against the visual line
    /// map cached by the last composed frame; this export contributes the
    /// scroll offset this face owns and the JS-shaped return value.
    #[wasm_bindgen(js_name = pixelToPosition)]
    pub fn pixel_to_position(&self, x: f32, y: f32) -> Vec<u32> {
        let (line, column) =
            self.compositor
                .pixel_to_position(&self.editor, &self.fold_state, self.scroll_y, x, y);
        vec![line as u32, column as u32]
    }

    /// Converts a document line/column to pixel coordinates.
    /// Returns [x, y] in physical pixels. Returns [-1.0, -1.0] if the line is
    /// hidden (folded or off-screen).
    ///
    /// This is the inverse of `pixelToPosition`, resolved on the compositor
    /// against the same cached visual line map; the [-1.0, -1.0] sentinel is
    /// this export's JS-shaped rendering of the compositor's `None`.
    #[wasm_bindgen(js_name = positionToPixel)]
    pub fn position_to_pixel(&self, doc_line: u32, column: u32) -> Vec<f32> {
        self.compositor
            .position_to_pixel(
                &self.editor,
                &self.fold_state,
                self.scroll_y,
                doc_line as usize,
                column as usize,
            )
            .map_or_else(|| vec![-1.0, -1.0], |(x, y)| vec![x, y])
    }

    // ==========================================================================
    // Code Folding Methods
    // ==========================================================================

    /// Updates fold regions based on current content.
    /// Call this after any text changes if you want fold regions to update.
    #[wasm_bindgen(js_name = updateFolds)]
    pub fn update_folds(&mut self) {
        self.refresh_fold_regions();
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
    /// Refreshes the fold regions from the current document.
    ///
    /// Every path that changes the document comes through here, and what it
    /// costs depends on what that path said first: one that reported its edit
    /// span through [`WebFoldSyntax::note_edit`] gets a refresh proportional to
    /// that edit; one that reported nothing, or declared the document replaced,
    /// gets a rescan. Both are correct, so a path with nothing accurate to say
    /// can safely stay silent.
    fn refresh_fold_regions(&mut self) -> bool {
        self.fold_syntax
            .refresh(&self.editor.state().document, &mut self.fold_state)
    }
}

// ============================================================================
// The highlight seam: this face's side of the compositor's HighlightSource
// ============================================================================

/// The web face's per-frame [`HighlightSource`]: resolves the tree-sitter
/// spans JavaScript delivered against the frame's visible content.
///
/// Built fresh each frame from borrows of the [`WebEditor`] fields that stay
/// on this side of the compositor seam. The two resolution paths here are the
/// two that read bindings-crate types ([`WebSpanIndex`], [`JsHighlightSpan`]);
/// the keyword fallback and the plain-text path live with the compositor,
/// which owns that highlighter.
struct WebHighlightSource<'a> {
    /// Span index for efficient viewport-based queries (O(log n + k)).
    span_index: &'a WebSpanIndex,
    /// Legacy flat span list, the fallback when the index is empty.
    ts_highlights: &'a [JsHighlightSpan],
    /// Whether tree-sitter highlights from JS are in use at all.
    use_ts_highlights: bool,
    /// The document, for byte-range queries against its rope.
    document: &'a Document,
    /// Fold state, which the viewport byte-range query is aware of.
    fold_state: &'a FoldState,
    /// The face's scroll offset when the frame began.
    scroll_y: f32,
    /// Surface height in physical pixels.
    surface_height: f32,
    /// The owning [`WebEditor`]'s highlight generation when the frame
    /// began. The resolver's other frame inputs (scroll, surface height)
    /// are functions of the viewport range the compositor already keys.
    generation: u64,
}

impl HighlightSource for WebHighlightSource<'_> {
    fn resolve<'a>(&mut self, context: &HighlightContext<'a>) -> Option<Vec<(&'a str, Color)>> {
        if self.use_ts_highlights && !self.span_index.is_empty() {
            // Calculate viewport for efficient span query (T020)
            // Only query spans in the visible range instead of iterating all spans
            //
            // `pixel_to_index` rather than `as usize`. This changes NO
            // behaviour: the kernel's conversion is bit-for-bit the cast's,
            // and asserted so by `pixel_to_index_matches_cast`. What it buys
            // is a single definition — this face previously reached for the
            // bare cast because `render::units` was `pub(crate)`, which is
            // how the desktop face ended up with a copy that had silently
            // drifted. It also subsumes the `.floor()` on the first line.
            //
            // NOT a guard, and must not be read as one: a degenerate
            // `line_height` (zero, or a `NaN` from an unvalidated
            // device-pixel-ratio) still yields `0` or a `usize::MAX`
            // visible-line count fed straight to `query_byte_range`, exactly
            // as the cast did. Validating `line_height` at this boundary is
            // task #37 and is deliberately not done here.
            let first_line = pixel_to_index(self.scroll_y / context.line_height);
            let visible_lines = pixel_to_index((self.surface_height / context.line_height).ceil())
                .saturating_add(1);

            let viewport = Viewport {
                first_line,
                visible_lines,
                line_height: context.line_height,
                height: self.surface_height,
                width: context.content_width,
                ..Viewport::default()
            };

            // Query byte range with overscan buffer (T023)
            let (start_byte, end_byte) = viewport.query_byte_range(
                self.document.rope(),
                self.fold_state,
                context.viewport_config,
            );

            // Query only viewport spans: O(log n + k) vs O(n) clone + sort (T021)
            // This eliminates per-frame clone (T018) and sort (T019)
            Some(build_rich_spans_viewport(
                context.content,
                self.span_index.query(start_byte, end_byte),
                context.content_start_byte,
                context.foreground,
                context.syntax_theme,
            ))
        } else if !self.ts_highlights.is_empty() {
            // Legacy fallback: use JsHighlightSpan when SpanIndex not available
            // PERF: This path clones ts_highlights to sort them. This is acceptable because:
            // 1. With WebSpanIndex, this path is rarely hit (only during transition)
            // 2. The clone happens only when span_index.is_empty() returns true
            // 3. Full optimization would require pre-sorted storage or sorted indices
            let spans = self.ts_highlights.to_vec();
            Some(build_rich_spans_from_ts(
                context.content,
                spans,
                context.foreground,
                context.syntax_theme,
            ))
        } else {
            // Nothing from tree-sitter yet: the compositor bridges with its
            // built-in keyword highlighter (see `language_active`).
            None
        }
    }

    fn language_active(&self) -> bool {
        // This face's grammar lives in the host's worker; the kernel never
        // learns it, so a span-less frame here is always the bridge — the
        // keyword fallback colours until the worker's spans arrive. The
        // host's channel for "this document has no language" is
        // `setSyntaxEnabled(false)`, which renders the plain foreground.
        true
    }

    fn generation(&self) -> u64 {
        self.generation
    }
}

/// Converts highlight spans (from `WebSpanIndex` query) to colored text spans
/// for rendering.
///
/// This is the optimized version that works with [`WebSpan`] from the
/// `WebSpanIndex`. The spans are collected, sorted, and processed only for
/// the visible viewport.
///
/// # Arguments
///
/// * `content` - The visible content string
/// * `spans` - Iterator of `WebSpan` from `WebSpanIndex::query()`
/// * `content_start_byte` - The document byte offset where content begins
/// * `foreground` - Default color for gaps between highlighted spans
/// * `syntax_theme` - Capture-name → color map for the hierarchical lookup
fn build_rich_spans_viewport<'a>(
    content: &'a str,
    spans: impl Iterator<Item = WebSpan>,
    content_start_byte: usize,
    foreground: Color,
    syntax_theme: &HashMap<String, Color>,
) -> Vec<(&'a str, Color)> {
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
        let color = color_for_highlight_type(syntax_theme, foreground, &span.highlight_type);

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
/// (Legacy version - kept for fallback when `SpanIndex` is empty)
///
/// Maps tree-sitter capture names to theme colors and handles gaps between
/// highlighted regions with default foreground color.
fn build_rich_spans_from_ts<'a>(
    content: &'a str,
    mut sorted_spans: Vec<JsHighlightSpan>,
    foreground: Color,
    syntax_theme: &HashMap<String, Color>,
) -> Vec<(&'a str, Color)> {
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
        let color = color_for_highlight_type(syntax_theme, foreground, &span.highlight_type);

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
/// Looks up the `syntax_theme` map with hierarchical resolution: for a
/// capture name like `punctuation.list_marker.markup`, tries the full name
/// first, then walks up the dot-separated hierarchy
/// (`punctuation.list_marker`, then `punctuation`) until a match is found.
/// Falls back to the given foreground.
///
/// Colors are provided by the host application via `setSyntaxTheme()`,
/// making the editor fully theme-agnostic.
fn color_for_highlight_type(
    syntax_theme: &HashMap<String, Color>,
    foreground: Color,
    highlight_type: &str,
) -> Color {
    let mut name = highlight_type;
    loop {
        if let Some(color) = syntax_theme.get(name) {
            return *color;
        }
        match name.rsplit_once('.') {
            Some((parent, _)) => name = parent,
            None => return foreground,
        }
    }
}
