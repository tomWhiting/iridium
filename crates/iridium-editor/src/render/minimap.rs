//! Minimap rendering for document overview.

/// Minimap renderer for document overview.
#[derive(Debug)]
pub struct MinimapRenderer {
    // Minimap resources will be added during implementation
    _placeholder: (),
}

impl MinimapRenderer {
    /// Creates a new minimap renderer.
    pub fn new() -> Self {
        Self { _placeholder: () }
    }
}

impl Default for MinimapRenderer {
    fn default() -> Self {
        Self::new()
    }
}
