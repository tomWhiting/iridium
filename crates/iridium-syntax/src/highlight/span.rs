//! A highlighted region of the document.

use serde::{Deserialize, Serialize};

use super::capture::HighlightType;

/// A highlighted span of text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightSpan {
    /// Start byte offset
    pub start: usize,
    /// End byte offset
    pub end: usize,
    /// The highlight type
    pub highlight: HighlightType,
}

impl HighlightSpan {
    /// Creates a new highlight span.
    #[must_use]
    pub const fn new(start: usize, end: usize, highlight: HighlightType) -> Self {
        Self {
            start,
            end,
            highlight,
        }
    }
}

impl PartialOrd for HighlightSpan {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HighlightSpan {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.start
            .cmp(&other.start)
            .then_with(|| self.end.cmp(&other.end))
    }
}
