//! WebAssembly bindings for browser usage.
//!
//! This module provides the entry points for using Iridium in a web browser
//! via WebAssembly. It wraps the editor and rendering functionality in
//! wasm-bindgen exports.

use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use iridium_editor::{
    editor::Editor,
    history::Command,
    render::{TextRenderer, WebSurface},
    theme::Theme,
    EditorConfig, Position, Range,
};

/// Initialize panic hook for better error messages in browser console.
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
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
    theme: Theme,
    needs_redraw: bool,
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
#[wasm_bindgen(js_name = createWebEditor)]
pub async fn create_web_editor(canvas: HtmlCanvasElement) -> Result<WebEditor, JsValue> {
    log("[Iridium] Creating WebEditor...");

    // Get canvas dimensions
    let width = canvas.width();
    let height = canvas.height();
    log(&format!("[Iridium] Canvas size: {}x{}", width, height));

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

    // Create the text renderer
    log("[Iridium] Creating TextRenderer...");
    let text_renderer =
        TextRenderer::new(surface.device(), surface.queue(), surface.format())
            .map_err(|e| {
                let msg = format!("[Iridium] TextRenderer error: {}", e);
                log(&msg);
                JsValue::from_str(&msg)
            })?;
    log("[Iridium] TextRenderer created");

    // Create editor with default config
    let editor = Editor::new(EditorConfig::default());
    let theme = Theme::dark();

    log("[Iridium] WebEditor ready");
    Ok(WebEditor {
        editor,
        surface,
        text_renderer,
        theme,
        needs_redraw: true,
    })
}

#[wasm_bindgen]
impl WebEditor {

    /// Sets the editor content.
    #[wasm_bindgen(js_name = setContent)]
    pub fn set_content(&mut self, content: &str) {
        self.editor.set_content(content);
        self.needs_redraw = true;
    }

    /// Gets the editor content.
    #[wasm_bindgen(js_name = getContent)]
    pub fn get_content(&self) -> String {
        self.editor.content()
    }

    /// Inserts text at the current cursor position.
    pub fn insert(&mut self, text: &str) {
        self.editor.paste(text);
        self.needs_redraw = true;
    }

    /// Deletes the character before the cursor (backspace).
    pub fn backspace(&mut self) {
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
            self.needs_redraw = true;
        }
    }

    /// Deletes the character after the cursor (delete).
    pub fn delete_forward(&mut self) {
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
            self.needs_redraw = true;
        }
    }

    /// Resizes the editor viewport.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.surface.resize(width, height);
        self.text_renderer
            .update_viewport(self.surface.queue(), width, height);
        self.needs_redraw = true;
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
    fn render_frame(&mut self) -> Result<(), JsValue> {
        use glyphon::{TextArea, TextBounds};
        use wgpu::{
            Color, LoadOp, Operations, RenderPassColorAttachment, RenderPassDescriptor, StoreOp,
        };

        // Get the content to render
        let content = self.editor.content();

        // Create a text buffer
        let mut buffer = self
            .text_renderer
            .create_buffer(self.surface.width() as f32);

        // Set the text with default color
        let foreground = self.theme.editor.foreground;
        self.text_renderer
            .set_text(&mut buffer, &content, foreground);
        self.text_renderer.shape_buffer(&mut buffer);

        // Create text area
        let text_area = TextArea {
            buffer: &buffer,
            left: 10.0,
            top: 10.0,
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

        // Prepare text for rendering
        self.text_renderer
            .prepare(self.surface.device(), self.surface.queue(), [text_area])
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        // Get background color from theme
        let bg = self.theme.editor.background;
        let clear_color = Color {
            r: f64::from(bg.r),
            g: f64::from(bg.g),
            b: f64::from(bg.b),
            a: f64::from(bg.a),
        };

        // Render frame
        let text_renderer = &self.text_renderer;
        self.surface
            .render_frame(|view, device, queue| {
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

                    // Render text
                    text_renderer
                        .render(&mut pass)
                        .map_err(|e| iridium_editor::editor::IridiumError::GpuInitFailed {
                            message: e.to_string(),
                        })?;
                }

                queue.submit(std::iter::once(encoder.finish()));
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
            Theme::dark()
        } else {
            Theme::light()
        };
        self.needs_redraw = true;
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
            self.needs_redraw = true;
        }
        result
    }

    /// Performs redo.
    pub fn redo(&mut self) -> bool {
        let result = self.editor.redo();
        if result {
            self.needs_redraw = true;
        }
        result
    }

    /// Moves cursor left.
    #[wasm_bindgen(js_name = moveCursorLeft)]
    pub fn move_cursor_left(&mut self) {
        let cursor = self.editor.cursor();
        if cursor.column > 0 {
            self.editor
                .set_cursor(Position::new(cursor.line, cursor.column - 1));
            self.needs_redraw = true;
        } else if cursor.line > 0 {
            // Move to end of previous line
            let prev_line_len = self
                .editor
                .state()
                .document
                .line(cursor.line - 1)
                .map(|l| l.chars().count())
                .unwrap_or(0);
            self.editor
                .set_cursor(Position::new(cursor.line - 1, prev_line_len));
            self.needs_redraw = true;
        }
    }

    /// Moves cursor right.
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

        if cursor.column < current_line_len {
            self.editor
                .set_cursor(Position::new(cursor.line, cursor.column + 1));
            self.needs_redraw = true;
        } else if cursor.line < self.editor.state().document.line_count().saturating_sub(1) {
            // Move to start of next line
            self.editor.set_cursor(Position::new(cursor.line + 1, 0));
            self.needs_redraw = true;
        }
    }

    /// Moves cursor up.
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
            self.editor
                .set_cursor(Position::new(cursor.line - 1, new_column));
            self.needs_redraw = true;
        }
    }

    /// Moves cursor down.
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
            self.editor
                .set_cursor(Position::new(cursor.line + 1, new_column));
            self.needs_redraw = true;
        }
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
