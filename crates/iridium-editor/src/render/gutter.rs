//! Line number gutter and fold indicator rendering.

/// Gutter renderer for line numbers and fold indicators.
#[derive(Debug)]
pub struct GutterRenderer {
    // Gutter resources will be added during implementation
    _placeholder: (),
}

impl GutterRenderer {
    /// Creates a new gutter renderer.
    pub const fn new() -> Self {
        Self { _placeholder: () }
    }
}

impl Default for GutterRenderer {
    fn default() -> Self {
        Self::new()
    }
}
