# Quickstart: Viewport-Aware Syntax Rendering

**Feature**: 002-viewport-syntax-rendering
**Estimated Implementation Time**: 2-3 days

## Prerequisites

- Rust 1.85+ with `cargo`
- Existing iridium codebase cloned
- Familiarity with the render pipeline (`wasm.rs:render_frame`)

## Setup

### 1. Add Dependencies

```bash
cd crates/iridium-editor
cargo add rust-lapper
```

### 2. Create Module Structure

```bash
mkdir -p src/span_index
touch src/span_index/mod.rs
touch src/span_index/interval_tree.rs
touch src/span_index/query.rs
```

## Implementation Order

### Phase 1: Span Index (Rust)

1. **`span_index/mod.rs`** - Module exports
2. **`span_index/interval_tree.rs`** - SpanIndex struct wrapping rust-lapper
3. **`span_index/query.rs`** - Viewport query helpers

**Test**: Unit tests for span queries

### Phase 2: Viewport Integration (Rust)

1. **`render/viewport.rs`** - Add `query_byte_range()` method
2. **`wasm.rs`** - Replace span processing in `render_frame()`

**Test**: Benchmark scrolling FPS on large file

### Phase 3: Incremental Parsing (TypeScript)

1. **`syntax/index.ts`** - Add `highlightIncremental()` method
2. **`controller/index.ts`** - Track edit positions, use incremental API

**Test**: Benchmark typing latency on large file

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/iridium-editor/src/lib.rs` | Add `pub mod span_index;` |
| `crates/iridium-editor/src/render/viewport.rs` | Add byte range query |
| `crates/iridium-bindings/src/wasm.rs` | Use SpanIndex in render_frame |
| `crates/iridium-bindings/ts/syntax/index.ts` | Add incremental parsing |
| `crates/iridium-bindings/ts/controller/index.ts` | Track edits, use incremental |

## Validation Checklist

- [ ] `cargo test` passes
- [ ] `cargo clippy` has no warnings
- [ ] 10,000-line file scrolls at 100+ FPS
- [ ] Typing latency < 16ms in large files
- [ ] No visual artifacts on multi-line spans
- [ ] Memory usage scales linearly with file size

## Quick Test

```bash
# Build WASM
cd crates/iridium-bindings
wasm-pack build --target web

# Start demo
cd examples/web-component
bun install && bun run dev

# Open http://localhost:12224
# Paste a large TypeScript file (5000+ lines)
# Open Chrome DevTools > Performance > Frame rate overlay
# Scroll and verify 100+ FPS
```

## Common Issues

### rust-lapper not found

```bash
# Make sure you're in the right crate
cd crates/iridium-editor
cargo add rust-lapper
```

### Spans not appearing

Check that `set_tree_sitter_highlights` is building the SpanIndex correctly and `render_frame` is querying with the right byte range.

### Visual glitches on scroll

Verify overscan buffer is large enough (default 2x). Check that viewport byte range calculation handles folds correctly.

## Next Steps

After basic implementation:

1. Add benchmarks to `tests/benchmarks/`
2. Profile for remaining bottlenecks
3. Consider caching shaped text buffers
4. Add telemetry for performance monitoring
