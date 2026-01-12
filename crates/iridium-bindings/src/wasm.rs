//! WebAssembly bindings for browser usage.
//!
//! This module provides the entry points for using Iridium in a web browser
//! via WebAssembly. It wraps the editor and rendering functionality in
//! wasm-bindgen exports.

use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use iridium_editor::{
    document::{CursorState, Selection},
    editor::{Editor, FoldState},
    history::Command,
    render::{CursorRenderer, GutterRenderer, Quad, QuadRenderer, SimpleHighlighter, TextRenderer, WebSurface},
    syntax_stubs::Language,
    theme::Theme,
    EditorConfig, Position, Range,
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
    /// Vertical scroll offset in pixels
    scroll_y: f32,
    /// Cached character width (measured from actual font metrics)
    cached_char_width: f32,
    /// Tree-sitter highlight spans from JavaScript (when available)
    ts_highlights: Vec<JsHighlightSpan>,
    /// Whether to use tree-sitter highlights from JS
    use_ts_highlights: bool,
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
    let mut text_renderer =
        TextRenderer::new(surface.device(), surface.queue(), surface.format()).map_err(|e| {
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
    // Initialize fold state with the placeholder language (enables brace-based folding)
    let fold_state = FoldState::for_language(Language::_Placeholder);

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
        scroll_y: 0.0,
        cached_char_width: 14.0 * 0.6, // Default until font is loaded
        ts_highlights: Vec::new(),
        use_ts_highlights: false,
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
        // Update fold regions for new content
        self.fold_state.update_regions(content);
        self.needs_redraw = true;
    }

    /// Gets the editor content.
    #[wasm_bindgen(js_name = getContent)]
    pub fn get_content(&self) -> String {
        self.editor.content()
    }

    /// Inserts text at the current cursor position.
    /// If there's a selection, replaces the selected text.
    pub fn insert(&mut self, text: &str) {
        // If there's a selection, delete it first
        let selection = &self.editor.state().cursor.primary;
        if !selection.is_collapsed() {
            self.delete_selection();
        }
        self.editor.paste(text);
        self.cursor_renderer.reset_blink();
        // Update fold regions after content change
        let content = self.editor.content();
        self.fold_state.update_regions(&content);
        self.needs_redraw = true;
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
        self.editor.apply_command(cmd);

        // Collapse selection to start position
        self.set_selection_internal(start, start);

        // Update fold regions after content change
        let content = self.editor.content();
        self.fold_state.update_regions(&content);
        true
    }

    /// Deletes the character before the cursor (backspace).
    /// If there's a selection, deletes the selected text instead.
    pub fn backspace(&mut self) {
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
                deleted_text: self.editor.state().document.slice(Range::new(delete_from, cursor)),
            };
            self.editor.apply_command(cmd);
            // Move cursor to start of deleted range
            self.set_selection_internal(delete_from, delete_from);

            // Update fold regions after content change
            let content = self.editor.content();
            self.fold_state.update_regions(&content);
        }
    }

    /// Deletes the character after the cursor (delete).
    /// If there's a selection, deletes the selected text instead.
    pub fn delete_forward(&mut self) {
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
                deleted_text: self.editor.state().document.slice(Range::new(cursor, delete_to)),
            };
            self.editor.apply_command(cmd);
            // Cursor stays in place for forward delete
            self.cursor_renderer.reset_blink();

            // Update fold regions after content change
            let content = self.editor.content();
            self.fold_state.update_regions(&content);
            self.needs_redraw = true;
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
    #[wasm_bindgen(js_name = getMaxScrollY)]
    pub fn max_scroll_y(&self) -> f32 {
        let line_height = self.text_renderer.line_height();
        let visible_lines = self.fold_state.visible_line_count(self.editor.state().document.line_count());
        let content_height = visible_lines as f32 * line_height;
        let viewport_height = self.surface.height() as f32;
        (content_height - viewport_height + 20.0).max(0.0) // 20px padding
    }

    /// Ensures the cursor is visible by scrolling if needed.
    #[wasm_bindgen(js_name = ensureCursorVisible)]
    pub fn ensure_cursor_visible(&mut self) {
        let line_height = self.text_renderer.line_height();
        let padding = 10.0;
        let viewport_height = self.surface.height() as f32;

        // Get cursor's visual line (accounting for folds)
        let cursor_line = self.editor.cursor().line;
        let visual_line = self.fold_state.document_to_visual_line(cursor_line).unwrap_or(0);
        let cursor_y = padding + (visual_line as f32 * line_height);

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

    /// Converts tree-sitter highlight spans to colored text spans for rendering.
    ///
    /// Maps tree-sitter capture names to theme colors and handles gaps between
    /// highlighted regions with default foreground color.
    ///
    /// Takes a pre-cloned list of highlight spans to avoid borrowing self.
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
    fn color_for_highlight_type(&self, highlight_type: &str) -> iridium_editor::theme::Color {
        use iridium_editor::theme::Color;

        // Map tree-sitter capture names to theme colors
        // These match the names from Zed's .scm query files
        match highlight_type {
            // Keywords
            "keyword" | "keyword.function" | "keyword.storage" | "keyword.modifier"
            | "keyword.control" | "keyword.return" | "keyword.control.return" => {
                // Blue for keywords
                Color::rgb(0.506, 0.631, 0.757) // #81A1C1
            }

            // Strings
            "string" | "string.literal" | "string.special" => {
                // Green for strings
                Color::rgb(0.639, 0.745, 0.549) // #A3BE8C
            }

            // Escape sequences
            "string.escape" | "escape_sequence" | "escape" => {
                // Orange for escapes
                Color::rgb(0.847, 0.502, 0.337) // #D88055
            }

            // Numbers
            "number" | "number.literal" | "integer" | "float" => {
                // Purple for numbers
                Color::rgb(0.702, 0.561, 0.678) // #B48EAD
            }

            // Booleans
            "boolean" | "constant.builtin.boolean" => {
                Color::rgb(0.702, 0.561, 0.678) // #B48EAD (same as numbers)
            }

            // Comments
            "comment" | "comment.line" | "comment.block" => {
                // Gray for comments
                Color::rgb(0.396, 0.482, 0.514) // #657B83
            }

            // Doc comments
            "comment.doc" | "comment.documentation" => {
                Color::rgb(0.435, 0.545, 0.569) // Slightly brighter gray
            }

            // Functions
            "function" | "function.call" | "function.builtin" => {
                // Cyan for functions
                Color::rgb(0.533, 0.753, 0.816) // #88C0D0
            }

            // Function definitions
            "function.definition" | "function.name" => {
                Color::rgb(0.533, 0.753, 0.816) // #88C0D0
            }

            // Methods
            "function.method" | "method" | "method.call" => {
                Color::rgb(0.533, 0.753, 0.816) // #88C0D0
            }

            // Macros
            "function.special" | "function.macro" | "macro" | "function.special.definition" => {
                // Teal for macros
                Color::rgb(0.306, 0.718, 0.675) // #4EB7AC
            }

            // Types
            "type" | "type.name" | "type.definition" | "constructor" => {
                // Yellow for types
                Color::rgb(0.922, 0.796, 0.545) // #EBCB8B
            }

            // Built-in types
            "type.builtin" | "type.primitive" => {
                Color::rgb(0.922, 0.796, 0.545) // #EBCB8B
            }

            // Variables
            "variable" | "identifier" => {
                // Default foreground for variables
                self.theme.editor.foreground
            }

            // Parameters
            "variable.parameter" | "parameter" => {
                // Slightly different color for parameters
                Color::rgb(0.847, 0.871, 0.914) // #D8DEE9
            }

            // Special variables (self, this)
            "variable.special" | "variable.builtin" => {
                // Orange-ish for special vars
                Color::rgb(0.867, 0.545, 0.376) // #DD8B60
            }

            // Operators
            "operator" | "keyword.operator" => {
                Color::rgb(0.506, 0.631, 0.757) // #81A1C1 (like keywords)
            }

            // Punctuation
            "punctuation.bracket" | "bracket" => {
                Color::rgb(0.608, 0.639, 0.690) // #9BA0AB
            }

            "punctuation.delimiter" | "delimiter" | "punctuation" => {
                Color::rgb(0.608, 0.639, 0.690) // #9BA0AB
            }

            // Properties/fields
            "property" | "field" | "property.name" | "label" | "variable.member"
            | "variable.field" => {
                Color::rgb(0.533, 0.753, 0.816) // #88C0D0
            }

            // Constants
            "constant" | "constant.builtin" => {
                Color::rgb(0.702, 0.561, 0.678) // #B48EAD
            }

            // Lifetimes (Rust)
            "lifetime" => {
                Color::rgb(0.867, 0.545, 0.376) // #DD8B60
            }

            // Attributes
            "attribute" | "decorator" | "annotation" => {
                Color::rgb(0.639, 0.745, 0.549) // #A3BE8C (like strings)
            }

            // Tags (HTML/XML)
            "tag" | "tag.name" => {
                Color::rgb(0.506, 0.631, 0.757) // #81A1C1
            }

            // Errors
            "error" => {
                Color::rgb(0.749, 0.298, 0.298) // #BF4C4C
            }

            // Default fallback
            _ => self.theme.editor.foreground,
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

        // Ensure viewport is updated with current surface dimensions
        // This is critical - without it, glyphon won't render text correctly
        self.text_renderer.update_viewport(
            self.surface.queue(),
            self.surface.width(),
            self.surface.height(),
        );
        self.background_quad_renderer.update_viewport(
            self.surface.queue(),
            self.surface.width(),
            self.surface.height(),
        );
        self.cursor_quad_renderer.update_viewport(
            self.surface.queue(),
            self.surface.width(),
            self.surface.height(),
        );

        // Get the content to render, handling folded lines
        let doc = &self.editor.state().document;
        let line_count = doc.line_count();

        // Build visible content, skipping hidden (folded) lines
        // Line numbers are built AFTER shaping to account for line wrapping
        let mut visible_content = String::new();
        let mut visual_line = 0;
        let mut doc_to_visual: Vec<Option<usize>> = Vec::with_capacity(line_count);
        let mut visible_doc_lines: Vec<usize> = Vec::new(); // Track which doc lines are visible

        for doc_line in 0..line_count {
            if self.fold_state.is_line_hidden(doc_line) {
                doc_to_visual.push(None);
                continue;
            }

            // Add newline separator (except for first visible line)
            if visual_line > 0 {
                visible_content.push('\n');
            }

            // Add line content
            if let Some(line_text) = doc.line(doc_line) {
                visible_content.push_str(&line_text);
            }

            // If this line is folded, also append the closing brace from the fold end
            if self.fold_state.is_folded(doc_line) {
                if let Some(region) = self.fold_state.region_at(doc_line) {
                    // Get the end line and find the closing brace
                    if let Some(end_line_text) = doc.line(region.end_line) {
                        let trimmed = end_line_text.trim();
                        // Append the closing portion (usually just "}")
                        if !trimmed.is_empty() {
                            visible_content.push_str(" ... ");
                            visible_content.push_str(trimmed);
                        }
                    }
                }
            }

            visible_doc_lines.push(doc_line);
            doc_to_visual.push(Some(visual_line));
            visual_line += 1;
        }

        // Calculate layout dimensions
        let line_height = self.text_renderer.line_height();
        let char_width = self.cached_char_width;
        let padding = 10.0_f32;

        // Calculate gutter width (based on digit count, before shaping)
        let gutter_width = if self.gutter_enabled {
            self.gutter_renderer.calculate_width(line_count, char_width)
        } else {
            0.0
        };
        let content_offset_x = gutter_width + padding;

        // Create and shape the main content buffer FIRST
        let content_width = self.surface.width() as f32 - content_offset_x;
        let mut buffer = self.text_renderer.create_buffer(content_width);

        // Set the text with syntax highlighting if enabled
        let foreground = self.theme.editor.foreground;
        if self.syntax_enabled {
            if self.use_ts_highlights && !self.ts_highlights.is_empty() {
                // Use tree-sitter highlights from JavaScript
                // Clone spans to avoid borrow checker issues with self
                let spans = self.ts_highlights.clone();
                let rich_spans = self.build_rich_spans_from_ts(&visible_content, spans);
                self.text_renderer
                    .set_rich_text(&mut buffer, rich_spans.into_iter());
            } else {
                // Fall back to simple keyword-based highlighting
                let spans = self.highlighter.highlight_flat(&visible_content);
                let rich_spans: Vec<(&str, iridium_editor::theme::Color)> = spans
                    .iter()
                    .map(|span| (span.text.as_str(), span.color))
                    .collect();
                self.text_renderer
                    .set_rich_text(&mut buffer, rich_spans.into_iter());
            }
        } else {
            // Plain text
            self.text_renderer
                .set_text(&mut buffer, &visible_content, foreground);
        }
        self.text_renderer.shape_buffer(&mut buffer);

        // NOW build line numbers with proper spacing for wrapped lines
        let visual_lines_per_line = self.text_renderer.visual_lines_per_logical_line(&buffer);
        let mut visible_line_numbers = String::new();
        let digit_width = GutterRenderer::digit_columns(line_count);

        for (i, &doc_line) in visible_doc_lines.iter().enumerate() {
            // Add newline separator (except for first visible line)
            if i > 0 {
                visible_line_numbers.push('\n');
            }

            // Add line number (1-indexed)
            let num_str = format!("{:>width$}", doc_line + 1, width = digit_width);
            visible_line_numbers.push_str(&num_str);

            // Add blank lines for wrapped visual lines (continuation lines)
            let wrap_count = visual_lines_per_line.get(i).copied().unwrap_or(1);
            for _ in 1..wrap_count {
                visible_line_numbers.push('\n');
                // Add blank spacing to maintain alignment
                for _ in 0..digit_width {
                    visible_line_numbers.push(' ');
                }
            }
        }

        // Handle empty document
        if line_count == 0 {
            visible_line_numbers.push_str(" 1");
        }

        // Create gutter buffer AFTER we know the wrapping
        let gutter_buffer = if self.gutter_enabled {
            let mut gutter_buf = self.text_renderer.create_buffer(gutter_width);
            let line_number_color = self.theme.editor.line_number;
            self.text_renderer.set_text(&mut gutter_buf, &visible_line_numbers, line_number_color);
            self.text_renderer.shape_buffer(&mut gutter_buf);
            Some(gutter_buf)
        } else {
            None
        };

        // Calculate cursor position (accounting for gutter offset, folding, scroll, and line wrapping)
        let cursor_pos = self.editor.cursor();
        let visual_cursor_line = doc_to_visual
            .get(cursor_pos.line)
            .and_then(|v| *v)
            .unwrap_or(0);

        // Use buffer layout to get accurate position with line wrapping
        let (wrap_x, wrap_y) = self.text_renderer.cursor_position_in_buffer(
            &buffer,
            visual_cursor_line,
            cursor_pos.column,
            char_width,
        );
        let cursor_x = content_offset_x + wrap_x;
        let cursor_y = padding + wrap_y - self.scroll_y;

        // Update cursor blink state
        self.cursor_renderer.update(Instant::now());

        // Create gutter background quad
        let mut gutter_quads: Vec<Quad> = Vec::new();
        if self.gutter_enabled {
            let gutter_bg_color = self.theme.editor.gutter;
            gutter_quads.push(Quad::new(
                0.0,
                0.0,
                gutter_width,
                self.surface.height() as f32,
                gutter_bg_color,
            ));
        }

        // Create selection highlight quads (render before text)
        let selection = &self.editor.state().cursor.primary;
        let mut selection_quads: Vec<Quad> = Vec::new();
        if !selection.is_collapsed() {
            let selection_color = self.theme.editor.selection;
            let start = selection.start();
            let end = selection.end();

            // For each line in the selection, create a highlight quad
            for doc_line in start.line..=end.line {
                // Skip hidden lines
                let Some(vis_line) = doc_to_visual.get(doc_line).and_then(|v| *v) else {
                    continue;
                };

                let line_content = self.editor.state().document.line(doc_line);
                let line_len = line_content.map(|l| l.chars().count()).unwrap_or(0);

                // Determine start and end columns for this line
                let start_col = if doc_line == start.line { start.column } else { 0 };
                let end_col = if doc_line == end.line { end.column } else { line_len };

                // For lines that continue to next line, extend selection slightly
                // to visualize the newline character selection
                let extra_width = if doc_line != end.line && end_col == line_len {
                    char_width * 0.5 // Add half a char width for newline
                } else {
                    0.0
                };

                if start_col < end_col || (start_col == end_col && extra_width > 0.0) {
                    let x = content_offset_x + (start_col as f32 * char_width);
                    let y = padding + (vis_line as f32 * line_height) - self.scroll_y;
                    let width = (end_col - start_col) as f32 * char_width + extra_width;

                    // Only add quad if it's visible
                    if y + line_height > 0.0 && y < self.surface.height() as f32 {
                        selection_quads.push(Quad::new(x, y, width, line_height, selection_color));
                    }
                }
            }
        }

        // Create cursor quad (2px wide line cursor)
        let cursor_color = self.theme.editor.cursor;
        let cursor_quads: Vec<Quad> = if self.cursor_renderer.is_visible() {
            vec![Quad::new(cursor_x, cursor_y, 2.0, line_height, cursor_color)]
        } else {
            vec![]
        };

        // Create text areas for rendering
        let mut text_areas = Vec::new();

        // Main content text area (with scroll offset)
        let text_area = TextArea {
            buffer: &buffer,
            left: content_offset_x,
            top: padding - self.scroll_y,
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
        text_areas.push(text_area);

        // Gutter text area (if enabled, with scroll offset)
        let line_number_color = self.theme.editor.line_number;
        if let Some(ref gutter_buf) = gutter_buffer {
            let gutter_text_area = TextArea {
                buffer: gutter_buf,
                left: 8.0, // Small padding from left edge
                top: padding - self.scroll_y,
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
            };
            text_areas.push(gutter_text_area);
        }

        // Prepare text for rendering
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

        // Batch all background quads (gutter + selection) together
        let mut background_quads: Vec<Quad> = Vec::with_capacity(
            gutter_quads.len() + selection_quads.len()
        );
        background_quads.extend(gutter_quads);
        background_quads.extend(selection_quads);

        // Render frame using SEPARATE quad renderers for background and cursor
        // This is critical: queue.write_buffer() to the same buffer multiple times within
        // a frame causes only the last write to be visible. Using separate QuadRenderers
        // with separate vertex buffers avoids this GPU synchronization issue.
        let text_renderer = &self.text_renderer;
        let background_quad_renderer = &self.background_quad_renderer;
        let cursor_quad_renderer = &self.cursor_quad_renderer;
        let queue = self.surface.queue_arc();
        self.surface
            .render_frame(|view, device, q| {
                let mut encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
                    background_quad_renderer.render(&mut pass, &queue, &background_quads);

                    // Render text (main content and gutter line numbers)
                    text_renderer
                        .render(&mut pass)
                        .map_err(|e| iridium_editor::editor::IridiumError::GpuInitFailed {
                            message: e.to_string(),
                        })?;

                    // Render cursor on top - uses cursor_quad_renderer with its own vertex buffer
                    // This avoids the GPU buffer overwrite issue that caused selection blinking
                    cursor_quad_renderer.render(&mut pass, &queue, &cursor_quads);
                }

                q.submit(std::iter::once(encoder.finish()));
                Ok(())
            })
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        // Trim glyph cache periodically
        self.text_renderer.trim_cache();

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
    #[wasm_bindgen(js_name = setTreeSitterHighlights)]
    pub fn set_tree_sitter_highlights(&mut self, spans_js: &JsValue) -> Result<(), JsValue> {
        use js_sys::{Array, Reflect};

        let array = Array::from(spans_js);
        let mut spans = Vec::with_capacity(array.length() as usize);

        for i in 0..array.length() {
            let obj = array.get(i);
            let start = Reflect::get(&obj, &JsValue::from_str("start"))
                .map_err(|_| JsValue::from_str("missing start"))?
                .as_f64()
                .ok_or_else(|| JsValue::from_str("start not a number"))? as usize;
            let end = Reflect::get(&obj, &JsValue::from_str("end"))
                .map_err(|_| JsValue::from_str("missing end"))?
                .as_f64()
                .ok_or_else(|| JsValue::from_str("end not a number"))? as usize;
            let highlight_type = Reflect::get(&obj, &JsValue::from_str("type"))
                .map_err(|_| JsValue::from_str("missing type"))?
                .as_string()
                .ok_or_else(|| JsValue::from_str("type not a string"))?;

            spans.push(JsHighlightSpan {
                start,
                end,
                highlight_type,
            });
        }

        self.ts_highlights = spans;
        self.use_ts_highlights = true;
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

    /// Performs undo.
    pub fn undo(&mut self) -> bool {
        let result = self.editor.undo();
        if result {
            // Update fold regions after content change
            let content = self.editor.content();
            self.fold_state.update_regions(&content);
            self.needs_redraw = true;
        }
        result
    }

    /// Performs redo.
    pub fn redo(&mut self) -> bool {
        let result = self.editor.redo();
        if result {
            // Update fold regions after content change
            let content = self.editor.content();
            self.fold_state.update_regions(&content);
            self.needs_redraw = true;
        }
        result
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
                let prev_line_len = doc.line(pos.line - 1).map(|l| l.chars().count()).unwrap_or(0);
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
        while col > 0 && chars.get(col - 1).map(|c| c.is_whitespace()).unwrap_or(false) {
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
        while col < chars.len() && chars.get(col).map(Self::char_class).unwrap_or(0) == current_class {
            col += 1;
        }

        // Skip whitespace going forwards
        while col < chars.len() && chars.get(col).map(|c| c.is_whitespace()).unwrap_or(false) {
            col += 1;
        }

        Position::new(pos.line, col)
    }

    /// Deletes the word before the cursor (Option+Backspace on Mac).
    #[wasm_bindgen(js_name = deleteWordBackward)]
    pub fn delete_word_backward(&mut self) {
        let cursor = self.editor.cursor();
        let word_start = self.find_word_boundary_left(cursor);

        if word_start != cursor {
            let cmd = Command::Delete {
                range: Range::new(word_start, cursor),
                deleted_text: self.editor.state().document.slice(Range::new(word_start, cursor)),
            };
            self.editor.apply_command(cmd);
            self.editor.set_cursor(word_start);
            self.cursor_renderer.reset_blink();
            let content = self.editor.content();
            self.fold_state.update_regions(&content);
            self.needs_redraw = true;
        }
    }

    /// Deletes the word after the cursor (Option+Delete on Mac).
    #[wasm_bindgen(js_name = deleteWordForward)]
    pub fn delete_word_forward(&mut self) {
        let cursor = self.editor.cursor();
        let word_end = self.find_word_boundary_right(cursor);

        if word_end != cursor {
            let cmd = Command::Delete {
                range: Range::new(cursor, word_end),
                deleted_text: self.editor.state().document.slice(Range::new(cursor, word_end)),
            };
            self.editor.apply_command(cmd);
            self.cursor_renderer.reset_blink();
            let content = self.editor.content();
            self.fold_state.update_regions(&content);
            self.needs_redraw = true;
        }
    }

    /// Deletes from cursor to start of line (Cmd+Backspace on Mac).
    #[wasm_bindgen(js_name = deleteToLineStart)]
    pub fn delete_to_line_start(&mut self) {
        let cursor = self.editor.cursor();
        if cursor.column > 0 {
            let line_start = Position::new(cursor.line, 0);
            let cmd = Command::Delete {
                range: Range::new(line_start, cursor),
                deleted_text: self.editor.state().document.slice(Range::new(line_start, cursor)),
            };
            self.editor.apply_command(cmd);
            self.editor.set_cursor(line_start);
            self.cursor_renderer.reset_blink();
            let content = self.editor.content();
            self.fold_state.update_regions(&content);
            self.needs_redraw = true;
        }
    }

    /// Deletes from cursor to end of line (Cmd+Delete / Ctrl+K on Mac).
    #[wasm_bindgen(js_name = deleteToLineEnd)]
    pub fn delete_to_line_end(&mut self) {
        let cursor = self.editor.cursor();
        let line_len = self
            .editor
            .state()
            .document
            .line(cursor.line)
            .map(|l| l.chars().count())
            .unwrap_or(0);

        if cursor.column < line_len {
            let line_end = Position::new(cursor.line, line_len);
            let cmd = Command::Delete {
                range: Range::new(cursor, line_end),
                deleted_text: self.editor.state().document.slice(Range::new(cursor, line_end)),
            };
            self.editor.apply_command(cmd);
            self.cursor_renderer.reset_blink();
            let content = self.editor.content();
            self.fold_state.update_regions(&content);
            self.needs_redraw = true;
        }
    }

    // ==========================================================================
    // Selection Methods
    // ==========================================================================

    /// Returns true if there is a non-empty selection.
    #[wasm_bindgen(js_name = hasSelection)]
    pub fn has_selection(&self) -> bool {
        !self.editor.state().cursor.primary.is_collapsed()
    }

    /// Gets the selected text, or empty string if no selection.
    #[wasm_bindgen(js_name = getSelectedText)]
    pub fn get_selected_text(&self) -> String {
        let selection = &self.editor.state().cursor.primary;
        if selection.is_collapsed() {
            String::new()
        } else {
            self.editor.state().document.slice(selection.range())
        }
    }

    /// Gets selection start line (for JS interop).
    #[wasm_bindgen(js_name = getSelectionStartLine)]
    pub fn get_selection_start_line(&self) -> u32 {
        self.editor.state().cursor.primary.start().line as u32
    }

    /// Gets selection start column (for JS interop).
    #[wasm_bindgen(js_name = getSelectionStartColumn)]
    pub fn get_selection_start_column(&self) -> u32 {
        self.editor.state().cursor.primary.start().column as u32
    }

    /// Gets selection end line (for JS interop).
    #[wasm_bindgen(js_name = getSelectionEndLine)]
    pub fn get_selection_end_line(&self) -> u32 {
        self.editor.state().cursor.primary.end().line as u32
    }

    /// Gets selection end column (for JS interop).
    #[wasm_bindgen(js_name = getSelectionEndColumn)]
    pub fn get_selection_end_column(&self) -> u32 {
        self.editor.state().cursor.primary.end().column as u32
    }

    /// Clears selection, leaving cursor at the head position.
    #[wasm_bindgen(js_name = clearSelection)]
    pub fn clear_selection(&mut self) {
        let head = self.editor.state().cursor.primary.head;
        self.set_selection_internal(head, head);
    }

    /// Sets the selection (for internal use).
    fn set_selection_internal(&mut self, anchor: Position, head: Position) {
        let doc = &self.editor.state().document;
        let anchor = doc.clamp_position(anchor);
        let head = doc.clamp_position(head);
        // We need to directly manipulate the cursor state
        // Since Editor doesn't expose set_selection, we'll use a workaround
        let selection = Selection::new(anchor, head);
        self.editor.state_mut().cursor = CursorState::new(selection);
        self.cursor_renderer.reset_blink();
        self.needs_redraw = true;
    }

    /// Extends selection left by one character (Shift+Left).
    #[wasm_bindgen(js_name = extendSelectionLeft)]
    pub fn extend_selection_left(&mut self) {
        let cursor_state = &self.editor.state().cursor;
        let anchor = cursor_state.primary.anchor;
        let head = cursor_state.primary.head;

        let new_head = if head.column > 0 {
            Position::new(head.line, head.column - 1)
        } else if head.line > 0 {
            let prev_line_len = self
                .editor
                .state()
                .document
                .line(head.line - 1)
                .map(|l| l.chars().count())
                .unwrap_or(0);
            Position::new(head.line - 1, prev_line_len)
        } else {
            head
        };

        if new_head != head {
            self.set_selection_internal(anchor, new_head);
        }
    }

    /// Extends selection right by one character (Shift+Right).
    #[wasm_bindgen(js_name = extendSelectionRight)]
    pub fn extend_selection_right(&mut self) {
        let cursor_state = &self.editor.state().cursor;
        let anchor = cursor_state.primary.anchor;
        let head = cursor_state.primary.head;

        let line_len = self
            .editor
            .state()
            .document
            .line(head.line)
            .map(|l| l.chars().count())
            .unwrap_or(0);
        let line_count = self.editor.state().document.line_count();

        let new_head = if head.column < line_len {
            Position::new(head.line, head.column + 1)
        } else if head.line < line_count.saturating_sub(1) {
            Position::new(head.line + 1, 0)
        } else {
            head
        };

        if new_head != head {
            self.set_selection_internal(anchor, new_head);
        }
    }

    /// Extends selection up by one line (Shift+Up).
    #[wasm_bindgen(js_name = extendSelectionUp)]
    pub fn extend_selection_up(&mut self) {
        let cursor_state = &self.editor.state().cursor;
        let anchor = cursor_state.primary.anchor;
        let head = cursor_state.primary.head;

        if head.line > 0 {
            let target_line_len = self
                .editor
                .state()
                .document
                .line(head.line - 1)
                .map(|l| l.chars().count())
                .unwrap_or(0);
            let new_column = head.column.min(target_line_len);
            let new_head = Position::new(head.line - 1, new_column);
            self.set_selection_internal(anchor, new_head);
        }
    }

    /// Extends selection down by one line (Shift+Down).
    #[wasm_bindgen(js_name = extendSelectionDown)]
    pub fn extend_selection_down(&mut self) {
        let cursor_state = &self.editor.state().cursor;
        let anchor = cursor_state.primary.anchor;
        let head = cursor_state.primary.head;
        let line_count = self.editor.state().document.line_count();

        if head.line < line_count.saturating_sub(1) {
            let target_line_len = self
                .editor
                .state()
                .document
                .line(head.line + 1)
                .map(|l| l.chars().count())
                .unwrap_or(0);
            let new_column = head.column.min(target_line_len);
            let new_head = Position::new(head.line + 1, new_column);
            self.set_selection_internal(anchor, new_head);
        }
    }

    /// Extends selection to previous word boundary (Shift+Option+Left).
    #[wasm_bindgen(js_name = extendSelectionWordLeft)]
    pub fn extend_selection_word_left(&mut self) {
        let cursor_state = &self.editor.state().cursor;
        let anchor = cursor_state.primary.anchor;
        let head = cursor_state.primary.head;
        let new_head = self.find_word_boundary_left(head);

        if new_head != head {
            self.set_selection_internal(anchor, new_head);
        }
    }

    /// Extends selection to next word boundary (Shift+Option+Right).
    #[wasm_bindgen(js_name = extendSelectionWordRight)]
    pub fn extend_selection_word_right(&mut self) {
        let cursor_state = &self.editor.state().cursor;
        let anchor = cursor_state.primary.anchor;
        let head = cursor_state.primary.head;
        let new_head = self.find_word_boundary_right(head);

        if new_head != head {
            self.set_selection_internal(anchor, new_head);
        }
    }

    /// Extends selection to line start (Shift+Cmd+Left or Shift+Home).
    #[wasm_bindgen(js_name = extendSelectionLineStart)]
    pub fn extend_selection_line_start(&mut self) {
        let cursor_state = &self.editor.state().cursor;
        let anchor = cursor_state.primary.anchor;
        let head = cursor_state.primary.head;
        let new_head = Position::new(head.line, 0);

        if new_head != head {
            self.set_selection_internal(anchor, new_head);
        }
    }

    /// Extends selection to line end (Shift+Cmd+Right or Shift+End).
    #[wasm_bindgen(js_name = extendSelectionLineEnd)]
    pub fn extend_selection_line_end(&mut self) {
        let cursor_state = &self.editor.state().cursor;
        let anchor = cursor_state.primary.anchor;
        let head = cursor_state.primary.head;
        let line_len = self
            .editor
            .state()
            .document
            .line(head.line)
            .map(|l| l.chars().count())
            .unwrap_or(0);
        let new_head = Position::new(head.line, line_len);

        if new_head != head {
            self.set_selection_internal(anchor, new_head);
        }
    }

    /// Extends selection to document start (Shift+Cmd+Up).
    #[wasm_bindgen(js_name = extendSelectionDocStart)]
    pub fn extend_selection_doc_start(&mut self) {
        let cursor_state = &self.editor.state().cursor;
        let anchor = cursor_state.primary.anchor;
        let new_head = Position::new(0, 0);
        self.set_selection_internal(anchor, new_head);
    }

    /// Extends selection to document end (Shift+Cmd+Down).
    #[wasm_bindgen(js_name = extendSelectionDocEnd)]
    pub fn extend_selection_doc_end(&mut self) {
        let cursor_state = &self.editor.state().cursor;
        let anchor = cursor_state.primary.anchor;
        let line_count = self.editor.state().document.line_count();
        let new_head = if line_count > 0 {
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
        self.set_selection_internal(anchor, new_head);
    }

    /// Selects all text (Cmd+A).
    #[wasm_bindgen(js_name = selectAll)]
    pub fn select_all(&mut self) {
        let doc = &self.editor.state().document;
        let line_count = doc.line_count();
        let start = Position::new(0, 0);
        let end = if line_count > 0 {
            let last_line = line_count - 1;
            let last_line_len = doc.line(last_line).map(|l| l.chars().count()).unwrap_or(0);
            Position::new(last_line, last_line_len)
        } else {
            Position::new(0, 0)
        };
        self.set_selection_internal(start, end);
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
    /// Returns [line, column]. Accounts for folded lines and scroll.
    #[wasm_bindgen(js_name = pixelToPosition)]
    pub fn pixel_to_position(&self, x: f32, y: f32) -> Vec<u32> {
        let line_height = self.text_renderer.line_height();
        let char_width = self.cached_char_width;
        let offset_x = self.current_gutter_width() + 10.0;
        let offset_y = 10.0;

        // Calculate visual line number (what's rendered on screen, accounting for scroll)
        let visual_line = ((y + self.scroll_y - offset_y) / line_height).max(0.0) as usize;

        // Convert visual line to document line (accounting for folds)
        let doc = &self.editor.state().document;
        let line_count = doc.line_count();
        let doc_line = self.fold_state.visual_to_document_line(visual_line);
        let clamped_line = doc_line.min(line_count.saturating_sub(1));

        // Calculate column (accounting for gutter)
        let line_content = doc.line(clamped_line);
        let line_len = line_content.map(|l| l.chars().count()).unwrap_or(0);
        let column = ((x - offset_x) / char_width + 0.5).max(0.0) as usize;
        let clamped_column = column.min(line_len);

        vec![clamped_line as u32, clamped_column as u32]
    }

    // ==========================================================================
    // Code Folding Methods
    // ==========================================================================

    /// Updates fold regions based on current content.
    /// Call this after any text changes if you want fold regions to update.
    #[wasm_bindgen(js_name = updateFolds)]
    pub fn update_folds(&mut self) {
        let content = self.editor.content();
        self.fold_state.update_regions(&content);
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
        if self.fold_state.toggle_fold_containing(line as usize).is_some() {
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
