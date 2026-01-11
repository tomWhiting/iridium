//! Python language configuration.

/// Python language configuration for tree-sitter.
pub struct PythonConfig {
    // Tree-sitter configuration will be added
    _placeholder: (),
}

impl PythonConfig {
    /// Creates a new Python configuration.
    pub const fn new() -> Self {
        Self { _placeholder: () }
    }
}

impl Default for PythonConfig {
    fn default() -> Self {
        Self::new()
    }
}
