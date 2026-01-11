//! TypeScript language configuration.

/// TypeScript language configuration for tree-sitter.
pub struct TypeScriptConfig {
    // Tree-sitter configuration will be added
    _placeholder: (),
}

impl TypeScriptConfig {
    /// Creates a new TypeScript configuration.
    pub fn new() -> Self {
        Self { _placeholder: () }
    }
}

impl Default for TypeScriptConfig {
    fn default() -> Self {
        Self::new()
    }
}
