//! Replace functionality.

/// Result of a replace operation.
#[derive(Debug, Clone)]
pub struct ReplaceResult {
    /// Number of replacements made
    pub count: usize,
    /// Text that was replaced
    pub replaced_text: Vec<String>,
}

impl ReplaceResult {
    /// Creates a new replace result.
    pub fn new(count: usize, replaced_text: Vec<String>) -> Self {
        Self { count, replaced_text }
    }

    /// Creates an empty result (no replacements).
    pub fn empty() -> Self {
        Self { count: 0, replaced_text: Vec::new() }
    }
}
