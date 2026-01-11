//! Editor bindings for TypeScript/WASM.

use napi::bindgen_prelude::*;
use napi_derive::napi;

use iridium_editor::Editor;

/// The Iridium editor instance exposed to TypeScript.
#[napi]
pub struct IridiumEditor {
    inner: Editor,
}

#[napi]
impl IridiumEditor {
    /// Creates a new editor instance.
    #[napi(constructor)]
    pub fn new() -> Result<Self> {
        Ok(Self { inner: Editor::with_defaults() })
    }

    /// Gets the current content.
    #[napi]
    pub fn get_content(&self) -> String {
        self.inner.content()
    }

    /// Sets the content.
    #[napi]
    pub fn set_content(&mut self, content: String) {
        self.inner.set_content(&content);
    }

    /// Gets the current line count.
    #[napi]
    pub fn get_line_count(&self) -> u32 {
        self.inner.state().document.line_count() as u32
    }

    /// Gets a specific line.
    #[napi]
    pub fn get_line(&self, line_number: u32) -> Option<String> {
        self.inner.state().document.line(line_number as usize)
    }

    /// Gets the cursor line.
    #[napi]
    pub fn get_cursor_line(&self) -> u32 {
        self.inner.cursor().line as u32
    }

    /// Gets the cursor column.
    #[napi]
    pub fn get_cursor_column(&self) -> u32 {
        self.inner.cursor().column as u32
    }

    /// Sets the cursor position.
    #[napi]
    pub fn set_cursor(&mut self, line: u32, column: u32) {
        self.inner.set_cursor(iridium_editor::Position::new(line as usize, column as usize));
    }

    /// Checks if undo is available.
    #[napi]
    pub fn can_undo(&self) -> bool {
        self.inner.state().history.can_undo()
    }

    /// Checks if redo is available.
    #[napi]
    pub fn can_redo(&self) -> bool {
        self.inner.state().history.can_redo()
    }
}

impl Default for IridiumEditor {
    fn default() -> Self {
        Self { inner: Editor::with_defaults() }
    }
}
