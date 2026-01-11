//! Rust language configuration.

/// Rust language configuration for tree-sitter.
pub struct RustConfig {
    // Tree-sitter configuration will be added
    _placeholder: (),
}

impl RustConfig {
    /// Creates a new Rust configuration.
    pub fn new() -> Self {
        Self { _placeholder: () }
    }
}

impl Default for RustConfig {
    fn default() -> Self {
        Self::new()
    }
}
