//! SQL language configuration.

/// SQL language configuration for tree-sitter.
pub struct SqlConfig {
    // Tree-sitter configuration will be added
    _placeholder: (),
}

impl SqlConfig {
    /// Creates a new SQL configuration.
    pub const fn new() -> Self {
        Self { _placeholder: () }
    }
}

impl Default for SqlConfig {
    fn default() -> Self {
        Self::new()
    }
}
