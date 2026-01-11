//! Cypher language configuration.

/// Cypher language configuration for tree-sitter.
pub struct CypherConfig {
    // Tree-sitter configuration will be added
    _placeholder: (),
}

impl CypherConfig {
    /// Creates a new Cypher configuration.
    pub fn new() -> Self {
        Self { _placeholder: () }
    }
}

impl Default for CypherConfig {
    fn default() -> Self {
        Self::new()
    }
}
