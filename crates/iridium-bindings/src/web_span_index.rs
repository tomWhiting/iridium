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
    /// Cached count of the spans actually stored — the ones that survived
    /// the degenerate filter, not the ones that were offered.
    count: usize,
}

impl WebSpanIndex {
    /// Creates a new span index from a list of spans.
    ///
    /// Spans with `start >= end` cannot overlap any query range, so they are
    /// dropped rather than stored. The count is taken *after* that filter:
    /// it describes the index, and [`is_empty`](Self::is_empty) is the
    /// question callers actually ask of it.
    #[must_use]
    pub fn new(spans: Vec<WebSpan>) -> Self {
        let intervals: Vec<Interval<usize, String>> = spans
            .into_iter()
            .filter(|s| s.start < s.end)
            .map(|s| Interval {
                start: s.start,
                stop: s.end,
                val: s.highlight_type,
            })
            .collect();

        let count = intervals.len();
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
    /// Returns an iterator over spans that overlap
    /// `[start_byte, end_byte)`.
    pub fn query(&self, start_byte: usize, end_byte: usize) -> impl Iterator<Item = WebSpan> + '_ {
        if start_byte >= end_byte {
            return WebQueryIterator::empty(&self.lapper);
        }

        WebQueryIterator::new(&self.lapper, start_byte, end_byte)
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

#[cfg(test)]
mod tests {
    use super::{WebSpan, WebSpanIndex};

    fn span(start: usize, end: usize) -> WebSpan {
        WebSpan {
            start,
            end,
            highlight_type: "keyword".to_owned(),
        }
    }

    /// `is_empty` answers for the index, not for the input that was offered
    /// to it.
    ///
    /// The two diverge exactly when every span is degenerate: the
    /// constructor drops `start >= end` on its way into the interval tree,
    /// so an all-degenerate input builds an index that can never return a
    /// span while reporting that it holds some. The caller in `wasm.rs`
    /// branches on `!is_empty()` to choose the indexed highlight path over
    /// the fallback, so the disagreement renders as syntax highlighting
    /// silently vanishing — no error, no diagnostic, just uncoloured text.
    #[test]
    fn an_index_that_dropped_every_span_reports_itself_empty() {
        // Zero-width at the same offset, and inverted. Both are shapes a
        // tree-sitter query can legitimately produce for a node whose byte
        // range collapses.
        let index = WebSpanIndex::new(vec![span(4, 4), span(9, 2)]);

        assert!(
            index.is_empty(),
            "every span was dropped as degenerate, so the index holds none"
        );
        assert_eq!(
            index.query(0, 100).count(),
            0,
            "the index cannot produce a span it did not store"
        );
    }

    /// A partially degenerate input keeps the survivors and counts only them.
    #[test]
    fn a_surviving_span_keeps_the_index_non_empty() {
        let index = WebSpanIndex::new(vec![span(4, 4), span(10, 20), span(9, 2)]);

        assert!(!index.is_empty(), "one span survived the filter");
        assert_eq!(
            index.query(0, 100).count(),
            1,
            "only the well-formed span is queryable"
        );
    }

    /// The ordinary case, pinned so the fix above cannot be mistaken for a
    /// change in what a well-formed index reports.
    #[test]
    fn a_well_formed_index_reports_every_span() {
        let index = WebSpanIndex::new(vec![span(0, 5), span(5, 10)]);

        assert!(!index.is_empty());
        assert_eq!(index.query(0, 100).count(), 2);
    }

    /// An empty input and an all-degenerate input are indistinguishable to a
    /// caller, which is the property the caller actually relies on.
    #[test]
    fn an_empty_input_and_an_all_degenerate_input_agree() {
        let from_nothing = WebSpanIndex::new(Vec::new());
        let from_degenerate = WebSpanIndex::new(vec![span(7, 7)]);

        assert_eq!(from_nothing.is_empty(), from_degenerate.is_empty());
        assert!(WebSpanIndex::empty().is_empty());
        assert!(WebSpanIndex::default().is_empty());
    }
}
