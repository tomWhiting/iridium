//! Web-specific span index for efficient viewport-based syntax highlighting.
//!
//! This module provides an interval tree that works with string-based highlight
//! types (from web-tree-sitter) rather than the native `HighlightType` enum.

use rust_lapper::{Interval, Lapper};

/// A highlight span with string-based type (for web-tree-sitter integration).
#[derive(Debug, Clone)]
pub struct WebSpan {
    /// Start byte offset
    pub start: usize,
    /// End byte offset
    pub end: usize,
    /// Highlight type as string (e.g., "keyword", "string", "comment")
    pub highlight_type: String,
}

/// An interval tree index for fast span queries by byte range.
///
/// This web-specific version uses string highlight types instead of the
/// native `HighlightType` enum, making it compatible with web-tree-sitter.
#[derive(Debug)]
pub struct WebSpanIndex {
    /// The underlying interval tree
    lapper: Lapper<usize, String>,
    /// Cached span count
    count: usize,
}

impl WebSpanIndex {
    /// Creates a new span index from a list of spans.
    #[must_use]
    pub fn new(spans: Vec<WebSpan>) -> Self {
        let count = spans.len();

        let intervals: Vec<Interval<usize, String>> = spans
            .into_iter()
            .filter(|s| s.start < s.end)
            .map(|s| Interval {
                start: s.start,
                stop: s.end,
                val: s.highlight_type,
            })
            .collect();

        let lapper = Lapper::new(intervals);

        Self { lapper, count }
    }

    /// Creates an empty span index.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            lapper: Lapper::new(Vec::new()),
            count: 0,
        }
    }

    /// Finds all spans overlapping the given byte range.
    ///
    /// Returns an iterator over spans that overlap [start_byte, end_byte).
    pub fn query(&self, start_byte: usize, end_byte: usize) -> impl Iterator<Item = WebSpan> + '_ {
        if start_byte >= end_byte {
            return WebQueryIterator::empty(&self.lapper);
        }

        WebQueryIterator::new(&self.lapper, start_byte, end_byte)
    }

    /// Returns the number of indexed spans.
    #[must_use]
    #[inline]
    pub const fn len(&self) -> usize {
        self.count
    }

    /// Returns true if the index has no spans.
    #[must_use]
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }
}

impl Default for WebSpanIndex {
    fn default() -> Self {
        Self::empty()
    }
}

/// Iterator adapter for Lapper find results.
struct WebQueryIterator<'a> {
    inner: rust_lapper::IterFind<'a, usize, String>,
}

impl<'a> WebQueryIterator<'a> {
    fn new(lapper: &'a Lapper<usize, String>, start: usize, stop: usize) -> Self {
        Self {
            inner: lapper.find(start, stop),
        }
    }

    fn empty(lapper: &'a Lapper<usize, String>) -> Self {
        Self {
            inner: lapper.find(0, 0),
        }
    }
}

impl Iterator for WebQueryIterator<'_> {
    type Item = WebSpan;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|interval| WebSpan {
            start: interval.start,
            end: interval.stop,
            highlight_type: interval.val.clone(),
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}
