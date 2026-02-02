# Viewport Query API Contract

**Module**: `iridium-bindings::wasm` (render_frame modifications)
**Purpose**: Query only visible spans during rendering for O(k) instead of O(n) per frame

## Types

### ViewportConfig

```rust
/// Configuration for viewport-aware rendering.
#[derive(Debug, Clone)]
pub struct ViewportConfig {
    /// Overscan buffer as multiple of viewport height.
    /// 2.0 = one screen above + one screen below viewport.
    /// Default: 2.0
    pub overscan_factor: f32,
}
```

## Viewport Methods

### `Viewport::query_byte_range`

```rust
impl Viewport {
    /// Calculate byte range to query for visible spans.
    ///
    /// # Arguments
    /// * `rope` - Document rope for line-to-byte conversion
    /// * `fold_state` - Fold state for visual line mapping
    /// * `config` - Viewport configuration (overscan settings)
    ///
    /// # Returns
    /// (start_byte, end_byte) tuple for span index query
    ///
    /// # Notes
    /// - Includes overscan buffer above and below viewport
    /// - Accounts for folded regions
    /// - Clamps to document bounds
    pub fn query_byte_range(
        &self,
        rope: &Rope,
        fold_state: &FoldState,
        config: &ViewportConfig,
    ) -> (usize, usize);
}
```

### `Viewport::visible_line_range`

```rust
impl Viewport {
    /// Get document line range for current viewport.
    ///
    /// # Arguments
    /// * `fold_state` - Fold state for visual line mapping
    /// * `config` - Viewport configuration
    ///
    /// # Returns
    /// (first_doc_line, last_doc_line) inclusive range
    pub fn visible_line_range(
        &self,
        fold_state: &FoldState,
        config: &ViewportConfig,
    ) -> (usize, usize);
}
```

## WebEditor Render Integration

### `WebEditor::render_frame` (modified flow)

```rust
impl WebEditor {
    pub fn render_frame(&mut self) {
        // 1. Calculate visible byte range (NEW)
        let (start_byte, end_byte) = self.viewport.query_byte_range(
            &self.rope,
            &self.fold_state,
            &self.viewport_config,
        );

        // 2. Query ONLY visible spans (NEW - was: process ALL spans)
        let visible_spans = self.span_index.query(start_byte, end_byte);

        // 3. Build rich spans from visible subset
        let rich_spans = self.build_rich_spans_from_ts(&visible_content, visible_spans);

        // 4. Render (unchanged)
        self.render_with_spans(rich_spans);
    }
}
```

### `WebEditor::set_tree_sitter_highlights` (modified)

```rust
impl WebEditor {
    /// Set highlight spans and build index.
    ///
    /// # Arguments
    /// * `spans` - Highlight spans from tree-sitter (full document)
    ///
    /// # Notes
    /// - Builds SpanIndex for efficient queries
    /// - Does NOT process spans immediately (done per-frame)
    pub fn set_tree_sitter_highlights(&mut self, spans: Vec<HighlightSpan>) {
        // Build index (O(n log n) but only when highlights change)
        self.span_index = SpanIndex::new(spans);

        // Remove: NO longer clones/sorts/processes all spans here
        // self.ts_highlights = spans; // OLD
    }
}
```

## Usage Flow

```
setTreeSitterHighlights(spans)
    │
    ▼
SpanIndex::new(spans)  ←── O(n log n) once
    │
    ▼
[User scrolls / render_frame called]
    │
    ▼
viewport.query_byte_range()  ←── O(1)
    │
    ▼
span_index.query(start, end)  ←── O(log n + k)
    │
    ▼
build_rich_spans(visible_spans)  ←── O(k) not O(n)
    │
    ▼
render()
```

## Performance Guarantees

| Operation | Old Complexity | New Complexity |
|-----------|----------------|----------------|
| Set highlights | O(n) clone | O(n log n) index build |
| Per-frame query | O(n) iterate all | O(log n + k) |
| Per-frame sort | O(n log n) | O(1) (pre-sorted) |
| Per-frame clone | O(n) | O(k) (visible only) |

Where:
- n = total spans in document (~10,000 for large file)
- k = visible spans (~500 for typical viewport)

**Net improvement**: ~20x reduction in per-frame work for large files
