//! Interval tree implementation for highlight span queries.
//!
//! Uses [`rust_lapper::Lapper`] for efficient interval overlap queries.

use rust_lapper::{Interval, Lapper};

#[cfg(feature = "syntax")]
use iridium_syntax::{HighlightSpan, HighlightType};

/// An interval tree index for fast span queries by byte range.
///
/// This structure enables O(log n + k) queries for highlight spans that
/// overlap a given byte range, where n is the total number of spans and
/// k is the number of overlapping spans returned.
///
/// # Performance
///
/// | Operation | Complexity |
/// |-----------|------------|
/// | `new(spans)` | O(n log n) |
/// | `query(start, end)` | O(log n + k) |
/// | `len()` | O(1) |
/// | `is_empty()` | O(1) |
///
/// # Thread Safety
///
/// `SpanIndex` is immutable after creation. To update spans, create a new
/// index. This design avoids synchronization overhead during queries.
#[derive(Debug)]
#[cfg(feature = "syntax")]
pub struct SpanIndex {
    /// The underlying interval tree
    lapper: Lapper<usize, HighlightType>,
    /// Cached span count for O(1) len()
    count: usize,
}

#[cfg(feature = "syntax")]
impl SpanIndex {
    /// Creates a new span index from a list of highlight spans.
    ///
    /// The spans are consumed and indexed for efficient range queries.
    /// Building the index is O(n log n) where n is the number of spans.
    ///
    /// # Arguments
    ///
    /// * `spans` - Highlight spans to index (will be consumed)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let spans = vec![
    ///     HighlightSpan::new(0, 5, HighlightType::Keyword),
    ///     HighlightSpan::new(6, 12, HighlightType::String),
    /// ];
    /// let index = SpanIndex::new(spans);
    /// ```
    #[must_use]
    pub fn new(spans: Vec<HighlightSpan>) -> Self {
        let count = spans.len();

        // Convert HighlightSpan to Lapper Interval
        // Lapper uses half-open intervals [start, stop) which matches our span semantics
        let intervals: Vec<Interval<usize, HighlightType>> = spans
            .into_iter()
            .filter(|s| s.start < s.end) // Filter out empty/invalid spans
            .map(|s| Interval {
                start: s.start,
                stop: s.end,
                val: s.highlight,
            })
            .collect();

        let lapper = Lapper::new(intervals);

        Self { lapper, count }
    }

    /// Creates an empty span index.
    ///
    /// This is useful as a default value or placeholder before highlights
    /// are available.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let index = SpanIndex::empty();
    /// assert!(index.is_empty());
    /// ```
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
    /// This includes spans that:
    /// - Start before `start_byte` and end after `start_byte`
    /// - Start within the range
    /// - Completely contain the range
    ///
    /// # Arguments
    ///
    /// * `start_byte` - Start of query range (inclusive)
    /// * `end_byte` - End of query range (exclusive)
    ///
    /// # Returns
    ///
    /// Iterator over `HighlightSpan` values that overlap the query range.
    ///
    /// # Complexity
    ///
    /// O(log n + k) where n is total spans and k is overlapping spans.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Query for bytes 1000-5000 (typical viewport)
    /// let visible: Vec<_> = index.query(1000, 5000).collect();
    /// ```
    pub fn query(
        &self,
        start_byte: usize,
        end_byte: usize,
    ) -> impl Iterator<Item = HighlightSpan> + '_ {
        // Handle invalid range gracefully
        if start_byte >= end_byte {
            return QueryIterator::empty(&self.lapper);
        }

        QueryIterator::new(&self.lapper, start_byte, end_byte)
    }

    /// Returns the number of indexed spans.
    ///
    /// Note: This returns the count of spans passed to `new()`, not the
    /// count after filtering invalid spans.
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

#[cfg(feature = "syntax")]
impl Default for SpanIndex {
    fn default() -> Self {
        Self::empty()
    }
}

/// Iterator adapter for Lapper find results.
///
/// Converts Lapper's `Interval<usize, HighlightType>` back to `HighlightSpan`.
#[cfg(feature = "syntax")]
struct QueryIterator<'a> {
    inner: rust_lapper::IterFind<'a, usize, HighlightType>,
}

#[cfg(feature = "syntax")]
impl<'a> QueryIterator<'a> {
    fn new(lapper: &'a Lapper<usize, HighlightType>, start: usize, stop: usize) -> Self {
        Self {
            inner: lapper.find(start, stop),
        }
    }

    fn empty(lapper: &'a Lapper<usize, HighlightType>) -> Self {
        // Query an impossible range to get empty iterator
        Self {
            inner: lapper.find(0, 0),
        }
    }
}

#[cfg(feature = "syntax")]
impl Iterator for QueryIterator<'_> {
    type Item = HighlightSpan;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|interval| HighlightSpan {
            start: interval.start,
            end: interval.stop,
            highlight: interval.val,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

#[cfg(all(test, feature = "syntax"))]
mod tests {
    use super::*;

    fn make_span(start: usize, end: usize) -> HighlightSpan {
        HighlightSpan::new(start, end, HighlightType::Keyword)
    }

    #[test]
    fn empty_index() {
        let index = SpanIndex::empty();
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
        assert_eq!(index.query(0, 100).count(), 0);
    }

    #[test]
    fn single_span() {
        let index = SpanIndex::new(vec![make_span(10, 20)]);
        assert_eq!(index.len(), 1);

        // Query overlapping
        let results: Vec<_> = index.query(0, 100).collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].start, 10);
        assert_eq!(results[0].end, 20);

        // Query exactly matching
        let results: Vec<_> = index.query(10, 20).collect();
        assert_eq!(results.len(), 1);

        // Query before span
        let results: Vec<_> = index.query(0, 5).collect();
        assert_eq!(results.len(), 0);

        // Query after span
        let results: Vec<_> = index.query(25, 30).collect();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn overlapping_spans() {
        let index = SpanIndex::new(vec![make_span(0, 10), make_span(5, 15), make_span(10, 20)]);

        // Query that overlaps all three
        let results: Vec<_> = index.query(8, 12).collect();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn span_starting_before_query() {
        // This is the critical case for multi-line strings/comments
        let index = SpanIndex::new(vec![make_span(0, 1000)]);

        // Query a range in the middle of the span
        let results: Vec<_> = index.query(500, 600).collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].start, 0);
        assert_eq!(results[0].end, 1000);
    }

    #[test]
    fn span_ending_after_query() {
        let index = SpanIndex::new(vec![make_span(50, 150)]);

        // Query overlaps start of span
        let results: Vec<_> = index.query(0, 100).collect();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn invalid_range_returns_empty() {
        let index = SpanIndex::new(vec![make_span(0, 100)]);

        // start >= end should return empty
        let results: Vec<_> = index.query(50, 50).collect();
        assert_eq!(results.len(), 0);

        let results: Vec<_> = index.query(100, 50).collect();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn filters_invalid_spans() {
        let index = SpanIndex::new(vec![
            make_span(10, 20), // valid
            make_span(30, 30), // invalid: empty
            make_span(50, 40), // invalid: start > end (though constructor prevents this)
        ]);

        // Only the valid span should be queryable
        let results: Vec<_> = index.query(0, 100).collect();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn large_index_performance() {
        // Create 10,000 spans (simulating a large file)
        let spans: Vec<_> = (0..10_000).map(|i| make_span(i * 10, i * 10 + 8)).collect();

        let index = SpanIndex::new(spans);
        assert_eq!(index.len(), 10_000);

        // Query a small viewport (should be fast)
        let results: Vec<_> = index.query(50_000, 51_000).collect();

        // Should find approximately 100 spans in this range
        assert!(results.len() > 50 && results.len() < 150);
    }

    #[test]
    fn multiline_span_viewport_scroll() {
        // Simulate a 50-line multi-line string (bytes 0-5000)
        let long_string = make_span(0, 5000);

        // Simulate other spans after it
        let spans = vec![long_string, make_span(5000, 5010), make_span(5020, 5030)];

        let index = SpanIndex::new(spans);

        // Scroll to middle of the string (viewport at bytes 2000-3000)
        let results: Vec<_> = index.query(2000, 3000).collect();

        // Should still find the long string even though we're in the middle
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].start, 0);
        assert_eq!(results[0].end, 5000);
    }

    /// T036: Spans that start before first visible line should be returned
    /// when querying a viewport in the middle of the document.
    #[test]
    fn span_starts_before_viewport() {
        // Simulates a multi-line comment starting at line 0 (byte 0)
        // and the viewport showing lines 50-100 (bytes 5000-10000)
        let spans = vec![
            make_span(0, 8000),     // Long span starting before viewport
            make_span(5500, 5600),  // Span fully within viewport
            make_span(9500, 15000), // Span that extends beyond viewport
        ];

        let index = SpanIndex::new(spans);

        // Query viewport range (lines 50-100, bytes 5000-10000)
        let results: Vec<_> = index.query(5000, 10000).collect();

        // Should find all 3 spans that overlap with viewport
        assert_eq!(results.len(), 3);

        // The long span starting before viewport should be included with its original bounds
        let long_span = results
            .iter()
            .find(|s| s.start == 0)
            .expect("Should find span starting at 0");
        assert_eq!(long_span.end, 8000);

        // The span fully within viewport
        let inner_span = results
            .iter()
            .find(|s| s.start == 5500)
            .expect("Should find inner span");
        assert_eq!(inner_span.end, 5600);

        // The span extending beyond viewport
        let extending_span = results
            .iter()
            .find(|s| s.start == 9500)
            .expect("Should find extending span");
        assert_eq!(extending_span.end, 15000);
    }

    /// T040: SpanIndex should handle 100,000+ spans without stack overflow.
    /// This verifies that the interval tree scales to very large files.
    #[test]
    fn large_index_100k_spans() {
        // Create 100,000 spans (simulating a very large file like 100K lines)
        let spans: Vec<_> = (0..100_000)
            .map(|i| make_span(i * 100, i * 100 + 80))
            .collect();

        // Building the index should not cause stack overflow
        let index = SpanIndex::new(spans);
        assert_eq!(index.len(), 100_000);

        // Query should still be fast (O(log n + k))
        let start = std::time::Instant::now();
        let results: Vec<_> = index.query(5_000_000, 5_010_000).collect();
        let elapsed = start.elapsed();

        // Should find approximately 100 spans in this 10K byte range
        assert!(results.len() > 50 && results.len() < 150);

        // Query should complete in under 1ms for O(log n + k)
        assert!(
            elapsed.as_millis() < 10,
            "Query took {}ms, expected <10ms for 100K spans",
            elapsed.as_millis()
        );
    }

    /// T038: Rapid viewport jumps should correctly return spans.
    /// This tests that the interval tree handles non-sequential queries correctly.
    #[test]
    fn rapid_viewport_jumps() {
        // Create spans distributed across the document
        let spans = vec![
            make_span(0, 100),       // Beginning
            make_span(5000, 5100),   // Near line 500
            make_span(50000, 50100), // Near line 5000
            make_span(1000, 1100),   // Near line 100
        ];

        let index = SpanIndex::new(spans);

        // Jump to line 5000 (bytes ~50000)
        let results1: Vec<_> = index.query(50000, 51000).collect();
        assert_eq!(results1.len(), 1);
        assert_eq!(results1[0].start, 50000);

        // Immediately jump back to line 100 (bytes ~1000)
        let results2: Vec<_> = index.query(1000, 2000).collect();
        assert_eq!(results2.len(), 1);
        assert_eq!(results2[0].start, 1000);

        // Jump to beginning
        let results3: Vec<_> = index.query(0, 500).collect();
        assert_eq!(results3.len(), 1);
        assert_eq!(results3[0].start, 0);

        // Multiple rapid jumps should all work correctly
        assert_eq!(index.query(5000, 5500).count(), 1);
        assert_eq!(index.query(50000, 50500).count(), 1);
        assert_eq!(index.query(0, 200).count(), 1);
    }
}
