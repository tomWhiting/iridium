//! Code folding region detection.

use serde::{Deserialize, Serialize};

/// Kind of foldable region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FoldKind {
    /// Block delimited by braces: { }
    Block,
    /// Region delimited by markers: #region / #endregion
    Region,
    /// Import statements
    Import,
    /// Multi-line comment
    Comment,
}

/// A region of code that can be folded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoldRegion {
    /// Start line (0-indexed)
    pub start_line: usize,
    /// End line (0-indexed, inclusive)
    pub end_line: usize,
    /// Kind of fold
    pub kind: FoldKind,
    /// Whether currently folded
    pub is_folded: bool,
}

impl FoldRegion {
    /// Creates a new fold region.
    #[must_use]
    pub const fn new(start_line: usize, end_line: usize, kind: FoldKind) -> Self {
        Self { start_line, end_line, kind, is_folded: false }
    }

    /// Returns the number of lines in this region.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.end_line.saturating_sub(self.start_line) + 1
    }

    /// Returns true if the given line is within this region.
    #[must_use]
    pub fn contains_line(&self, line: usize) -> bool {
        line >= self.start_line && line <= self.end_line
    }
}

/// Detects foldable regions in source code.
#[derive(Debug)]
pub struct FoldDetector {
    language: super::Language,
}

impl FoldDetector {
    /// Creates a new fold detector for the given language.
    #[must_use]
    pub const fn new(language: super::Language) -> Self {
        Self { language }
    }

    /// Detects all foldable regions in the source code.
    #[must_use]
    pub fn detect(&self, _source: &str) -> Vec<FoldRegion> {
        // Fold detection will be implemented using tree-sitter
        Vec::new()
    }
}
