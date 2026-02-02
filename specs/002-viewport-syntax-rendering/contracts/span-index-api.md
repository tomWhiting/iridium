# Span Index API Contract

**Module**: `iridium-editor::span_index`
**Purpose**: Efficient O(log n) queries for highlight spans overlapping a byte range

## Types

### HighlightSpan

```rust
/// A syntax highlight span with byte range and type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightSpan {
    /// Start byte offset (inclusive)
    pub start: usize,
    /// End byte offset (exclusive)
    pub end: usize,
    /// Highlight category
    pub highlight_type: HighlightType,
}
```

### SpanIndex

```rust
/// Interval tree index for fast span queries.
pub struct SpanIndex {
    // Internal: Lapper<usize, HighlightType>
}
```

## Constructor

### `SpanIndex::new`

```rust
/// Create a new span index from a list of highlight spans.
///
/// # Arguments
/// * `spans` - Highlight spans to index (will be consumed)
///
/// # Returns
/// A new SpanIndex ready for queries
///
/// # Complexity
/// O(n log n) where n = number of spans
pub fn new(spans: Vec<HighlightSpan>) -> Self;
```

### `SpanIndex::empty`

```rust
/// Create an empty span index (no spans).
///
/// # Returns
/// An empty SpanIndex that returns no results for any query
pub fn empty() -> Self;
```

## Query Methods

### `SpanIndex::query`

```rust
/// Find all spans overlapping the given byte range.
///
/// # Arguments
/// * `start_byte` - Start of query range (inclusive)
/// * `end_byte` - End of query range (exclusive)
///
/// # Returns
/// Iterator over spans that overlap [start_byte, end_byte)
///
/// # Complexity
/// O(log n + k) where k = number of overlapping spans
pub fn query(&self, start_byte: usize, end_byte: usize) -> impl Iterator<Item = HighlightSpan> + '_;
```

### `SpanIndex::query_for_viewport`

```rust
/// Find spans for the visible viewport plus overscan buffer.
///
/// # Arguments
/// * `viewport` - Current viewport state
/// * `rope` - Document rope for line-to-byte conversion
/// * `fold_state` - Fold state for visual-to-document line mapping
///
/// # Returns
/// Vec of spans overlapping the viewport's visible byte range
///
/// # Complexity
/// O(log n + k) where k = number of visible spans
pub fn query_for_viewport(
    &self,
    viewport: &Viewport,
    rope: &Rope,
    fold_state: &FoldState,
) -> Vec<HighlightSpan>;
```

## Utility Methods

### `SpanIndex::len`

```rust
/// Return the number of indexed spans.
pub fn len(&self) -> usize;
```

### `SpanIndex::is_empty`

```rust
/// Check if the index has no spans.
pub fn is_empty(&self) -> bool;
```

## Usage Example

```rust
use iridium_editor::span_index::{SpanIndex, HighlightSpan, HighlightType};

// Build index from parser output
let spans = vec![
    HighlightSpan { start: 0, end: 3, highlight_type: HighlightType::Keyword },
    HighlightSpan { start: 4, end: 10, highlight_type: HighlightType::Variable },
    // ... more spans
];
let index = SpanIndex::new(spans);

// Query for viewport (e.g., bytes 1000-5000)
let visible_spans: Vec<_> = index.query(1000, 5000).collect();

// Or use the viewport-aware helper
let visible_spans = index.query_for_viewport(&viewport, &rope, &fold_state);
```

## Error Handling

- Invalid ranges (start >= end) return empty results, no error
- Out-of-bounds queries are clamped to valid range
- Empty index returns empty results for all queries
