# Implementation Plan: Viewport-Aware Syntax Rendering

**Branch**: `002-viewport-syntax-rendering` | **Date**: 2026-01-14 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/002-viewport-syntax-rendering/spec.md`

## Summary

Implement viewport-aware virtualized syntax highlighting to achieve 120fps scrolling on large files (10,000+ lines). The current implementation processes ALL highlight spans on every render frame, causing 30-40 FPS on large files. This plan introduces an interval tree for O(log n) span queries, viewport-driven rendering with overscan buffer, incremental tree-sitter parsing, and line-to-byte offset mapping.

## Technical Context

**Language/Version**: Rust 1.85+ (edition 2024), TypeScript 5.7+
**Primary Dependencies**: wgpu 28.0, glyphon 0.10, ropey 2.0, tree-sitter 0.26, web-tree-sitter 0.25.6
**Storage**: N/A (in-memory data structures only)
**Testing**: cargo test, manual FPS benchmarking with Chrome DevTools
**Target Platform**: WASM (browser), with native Rust support
**Project Type**: Multi-crate Rust workspace with TypeScript bindings
**Performance Goals**: 120fps scrolling, <16ms keystroke latency, <50ms highlight update
**Constraints**: <8ms per frame budget, linear memory scaling with file size
**Scale/Scope**: Files up to 100,000 lines, typical usage 1,000-10,000 lines

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Consistency | ✅ PASS | Syntax highlighting will behave identically regardless of file size - only performance improves |
| II. Contract, Not Coercion | ✅ PASS | No user-facing API changes; internal optimization only |
| III. Trust Users, Don't Give Them Guns | ✅ PASS | Large files allowed (graceful degradation), no arbitrary limits |
| IV. Expose All Controls | ✅ PASS | Overscan buffer will be configurable |
| V. No Silent Failures | ✅ PASS | Performance issues will be measurable; no hidden degradation |
| VI. Automation Over Gatekeeping | ✅ PASS | System automatically uses virtualization - doesn't block large files |
| VII. Low-Level Primitives | ✅ PASS | Using interval tree directly, not through abstraction layers |
| VIII. No Lazy Code | ✅ PASS | Full implementation of all edge cases (multi-line spans, rapid viewport changes) |
| IX. Make It Easy to Have Fun | ✅ PASS | This IS the feature that makes large files fun to use |
| X. Build With Love | ✅ PASS | Proper solution, not quick hack |

**Technical Standards Compliance**:
- ✅ Will use `cargo add` for new interval tree dependency
- ✅ New code will be in logical modules (span_index.rs, viewport_query.rs)
- ✅ Files will stay under 800 lines
- ✅ All public API documented

## Project Structure

### Documentation (this feature)

```text
specs/002-viewport-syntax-rendering/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output (internal APIs)
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code (repository root)

```text
crates/
├── iridium-editor/
│   └── src/
│       ├── render/
│       │   ├── viewport.rs      # EXISTS - viewport tracking (to be extended)
│       │   ├── highlight.rs     # EXISTS - highlight rendering (to be modified)
│       │   └── text.rs          # EXISTS - text rendering (to be modified)
│       ├── span_index/          # NEW - interval tree module
│       │   ├── mod.rs           # Module declarations
│       │   ├── interval_tree.rs # Interval tree implementation/wrapper
│       │   └── query.rs         # Viewport-aware span queries
│       └── syntax.rs            # EXISTS - syntax highlighting (to be modified)
│
├── iridium-bindings/
│   └── src/
│       └── wasm.rs              # EXISTS - WASM bindings (render_frame to be optimized)
│   └── ts/
│       ├── syntax/
│       │   └── index.ts         # EXISTS - tree-sitter integration (incremental parsing)
│       └── controller/
│           └── index.ts         # EXISTS - controller (highlight update path)
│
└── iridium-syntax/              # EXISTS - may need line index utilities

tests/
└── benchmarks/                  # NEW - performance benchmarks
    ├── scroll_large_file.rs     # Scrolling FPS benchmark
    └── typing_latency.rs        # Keystroke latency benchmark
```

**Structure Decision**: This feature modifies existing crates rather than adding new ones. A new `span_index` module is added to `iridium-editor` for the interval tree and query logic. Benchmarks are added for performance validation.

## Complexity Tracking

> No constitution violations requiring justification. The solution uses standard data structures (interval tree) and follows existing patterns in the codebase.

| Decision | Rationale |
|----------|-----------|
| Interval tree over binary search | Correctness: handles spans that start before viewport but overlap it |
| Rust-side span index over JS-side | Performance: avoids JS↔WASM serialization overhead per frame |
| 2x overscan buffer default | Balance: enough for typical scroll speeds without excessive memory |
