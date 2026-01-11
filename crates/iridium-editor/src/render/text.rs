//! Text rendering with glyphon.

/// Text renderer using glyphon for GPU text rendering.
#[derive(Debug)]
pub struct TextRenderer {
    // Glyphon resources will be added during implementation
    _placeholder: (),
}

impl TextRenderer {
    /// Creates a new text renderer.
    pub fn new() -> Self {
        Self { _placeholder: () }
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}
