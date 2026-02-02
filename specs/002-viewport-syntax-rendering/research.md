# Research: Viewport-Aware Syntax Rendering

**Feature**: 002-viewport-syntax-rendering
**Date**: 2026-01-14

## Research Areas

1. Interval tree crate selection for Rust
2. Tree-sitter incremental parsing API (web-tree-sitter)
3. Viewport-to-byte-offset mapping strategies
4. Overscan buffer sizing

---

## 1. Interval Tree Crate Selection

### Decision: `rust-lapper`

### Rationale

`rust-lapper` is optimized for exactly our use case: querying overlapping intervals in sorted data where intervals have similar lengths (syntax spans are typically short - keywords, identifiers, etc.).

### Alternatives Considered

| Crate | Pros | Cons | Why Not Chosen |
|-------|------|------|----------------|
| **rust-lapper** | 3-10x faster than alternatives, optimized for genomic (similar to syntax) data, simple API | Assumes similar-length intervals | ✅ **CHOSEN** - best fit for syntax spans |
| **intervaltree** | True interval tree, handles varying lengths well | Immutable after creation, less optimized | Our spans are rebuilt on each parse anyway |
| **iset** | Mutable, O(log n + k) queries | More complex API, less benchmarked | Over-engineered for our needs |

### Key API

```rust
use rust_lapper::{Lapper, Interval};

// Create intervals: [start, stop) with associated data
let intervals = vec![
    Interval { start: 0, stop: 10, val: HighlightType::Keyword },
    Interval { start: 15, stop: 25, val: HighlightType::String },
];

// Build the index (sorts internally)
let lapper = Lapper::new(intervals);

// Query overlapping spans for viewport [100, 500)
let visible_spans: Vec<_> = lapper.find(100, 500).collect();
```

### Performance Characteristics

- Build: O(n log n) - done once when highlights are set
- Query: O(log n + k) where k = number of overlapping spans
- Memory: O(n) - stores all spans once

**Source**: [rust-lapper documentation](https://docs.rs/rust-lapper/latest/rust_lapper/)

---

## 2. Tree-sitter Incremental Parsing

### Decision: Use `tree.edit()` + pass old tree to `parser.parse()`

### Rationale

Tree-sitter's incremental parsing allows reusing unchanged portions of the syntax tree, making re-parsing after edits significantly faster than full parsing.

### API (web-tree-sitter JavaScript)

```typescript
// After user edits: "let x = 1;" → "let x = 42;"
tree.edit({
    startIndex: 8,           // byte where edit started
    oldEndIndex: 9,          // byte where old content ended
    newEndIndex: 10,         // byte where new content ends
    startPosition: { row: 0, column: 8 },
    oldEndPosition: { row: 0, column: 9 },
    newEndPosition: { row: 0, column: 10 }
});

// Parse with old tree for incremental update
const newTree = parser.parse(newSourceCode, tree);

// Optional: get changed ranges for targeted highlight updates
const changes = tree.getChangedRanges(newTree);
```

### Implementation Notes

1. **Edit tracking**: Controller must track edit position (start byte, old length, new length) on each keystroke
2. **Row/column calculation**: Can be derived from byte position using rope's line index
3. **Highlight regeneration**: Only need to re-query captures for changed ranges, not entire file

### Performance Benefits

- Parsing: Shares unchanged tree structure, O(edit size) not O(file size)
- Memory: New tree reuses nodes from old tree
- Enables per-keystroke parsing without performance penalty

**Source**: [Tree-sitter Advanced Parsing](https://tree-sitter.github.io/tree-sitter/using-parsers/3-advanced-parsing.html)

---

## 3. Viewport-to-Byte-Offset Mapping

### Decision: Use ropey's existing line index + compute byte offsets

### Rationale

Ropey already maintains a line index for the document. We can use `line_to_byte_idx()` to convert viewport line numbers to byte positions for span queries.

### Implementation

```rust
// In Rust (ropey 2.0 API)
use ropey::LineType;

// Get byte offset for start of visible region
let first_visible_line = viewport.first_line;
let start_byte = rope.line_to_byte_idx(first_visible_line);

// Get byte offset for end of visible region (with overscan)
let last_visible_line = viewport.first_line + viewport.visible_lines + overscan_lines;
let end_byte = rope.line_to_byte_idx(last_visible_line.min(rope.len_lines(LineType::LF_CR)));

// Query span index for this byte range
let visible_spans = span_index.find(start_byte, end_byte);
```

### Considerations

- **Folds**: When lines are folded, visual line ≠ document line. Use existing `fold_state.visual_to_document_line()` first.
- **Horizontal scroll**: Not relevant for span queries (spans cover full lines)
- **Long lines**: Spans within long lines are still included correctly by byte range

---

## 4. Overscan Buffer Sizing

### Decision: Default 2x viewport (configurable)

### Rationale

- **1x** (one screen above + below): May show loading on fast scrolling
- **2x** (two screens total buffer): Sufficient for typical scroll speeds
- **3x+**: Diminishing returns, increased memory/computation

### Configuration

```rust
pub struct ViewportConfig {
    /// Overscan as multiple of viewport height (default: 2.0)
    /// 1.0 = buffer equals viewport, 2.0 = 1 screen above + 1 below
    pub overscan_factor: f32,
}

impl Default for ViewportConfig {
    fn default() -> Self {
        Self { overscan_factor: 2.0 }
    }
}
```

### Calculation

```rust
let visible_lines = viewport.visible_lines;
let overscan_lines = (visible_lines as f32 * config.overscan_factor / 2.0) as usize;

let query_start_line = viewport.first_line.saturating_sub(overscan_lines);
let query_end_line = (viewport.first_line + visible_lines + overscan_lines).min(total_lines);
```

---

## Summary of Decisions

| Research Area | Decision | Key Benefit |
|---------------|----------|-------------|
| Interval tree crate | `rust-lapper` | 3-10x faster, simple API, fits syntax span patterns |
| Incremental parsing | `tree.edit()` API | O(edit size) reparsing, not O(file size) |
| Line-to-byte mapping | ropey's `line_to_byte_idx()` | Already available, accurate with folds |
| Overscan buffer | 2x default, configurable | Balances smoothness vs. computation |

## Dependencies to Add

```bash
cargo add rust-lapper
```

No new TypeScript dependencies needed - web-tree-sitter already has the `edit()` API.
