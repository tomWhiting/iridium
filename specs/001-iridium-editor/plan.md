# Implementation Plan: Iridium GPU-Accelerated Text Editor

**Branch**: `001-iridium-editor` | **Date**: 2026-01-11 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/001-iridium-editor/spec.md`

## Summary

Build a GPU-accelerated single-file text editor component in Rust that delivers 120fps rendering performance across web (WASM/WebGPU) and native platforms. The editor provides professional editing features including multi-cursor editing, undo tree with branching history, syntax highlighting via tree-sitter, and a command-sourced architecture for rock-solid reliability.

## Technical Context

**Language/Version**: Rust 1.82+ (MSRV for wgpu)
**Primary Dependencies**:
- wgpu 28.x (GPU abstraction, WebGPU/Vulkan/Metal/DX12)
- glyphon (GPU text rendering)
- cosmic-text (text shaping, Unicode, font fallback)
- ropey 1.6+ (rope data structure for text buffer)
- tree-sitter (incremental parsing)
- napi-rs 3.x (TypeScript/WASM bindings)

**Storage**: N/A (in-memory only, host handles file I/O)
**Testing**: cargo test, wasm-pack test, visual regression tests
**Target Platform**: WASM/WebGPU (primary), native desktop (secondary)
**Project Type**: Rust workspace with multiple crates
**Performance Goals**: 120fps rendering, <8ms input latency, 100k+ line files
**Constraints**: WebGPU required (no WebGL fallback), <50 lines React integration
**Scale/Scope**: Single-file editor, embedded component

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Consistency Is On Us | PASS | Command-sourced architecture ensures deterministic behavior |
| II. Contract, Not Coercion | PASS | Editor renders user input faithfully, no silent modifications |
| III. Trust Users, Don't Give Them Guns | PASS | Undo tree prevents data loss, no irreversible operations |
| IV. Expose All Controls, Make Defaults Excellent | PASS | Full configuration API, sensible defaults |
| V. No Silent Failures | PASS | FR-046/47 require visible errors for all failure modes |
| VI. Automation Over Gatekeeping | PASS | Event-based architecture, no arbitrary limits |
| VII. Low-Level Primitives | PASS | Direct wgpu usage, no abstraction layers |
| VIII. No Lazy Code | PASS | Each user story is complete before moving to next |
| IX. Easy to Have Fun | PASS | SC-012/13 require intuitive UX |
| X. Build With Love | PASS | Professional quality bar throughout |

**Technical Standards Compliance**:
- Dependencies via `cargo add`: WILL COMPLY
- Modular structure with lean `mod.rs`: WILL COMPLY
- File size <1000 lines: WILL COMPLY
- `cargo fmt` + `cargo clippy`: WILL COMPLY

## Project Structure

### Documentation (this feature)

```text
specs/001-iridium-editor/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output (TypeScript API)
├── checklists/          # Quality checklists
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code (repository root)

```text
crates/
├── iridium-editor/          # Main editor crate
│   ├── src/
│   │   ├── lib.rs           # Public API exports
│   │   ├── editor/
│   │   │   ├── mod.rs       # Module declarations
│   │   │   ├── core.rs      # Editor state machine
│   │   │   ├── commands.rs  # Command definitions
│   │   │   └── config.rs    # Configuration types
│   │   ├── document/
│   │   │   ├── mod.rs
│   │   │   ├── buffer.rs    # Rope wrapper
│   │   │   ├── position.rs  # Position/Range types
│   │   │   └── cursor.rs    # Cursor/Selection state
│   │   ├── history/
│   │   │   ├── mod.rs
│   │   │   ├── undo_tree.rs # Tree-structured undo
│   │   │   ├── stack.rs     # Simple linear undo (alternative to tree)
│   │   │   └── commands.rs  # Reversible commands
│   │   ├── render/
│   │   │   ├── mod.rs
│   │   │   ├── pipeline.rs  # wgpu render pipeline
│   │   │   ├── text.rs      # glyphon integration
│   │   │   ├── viewport.rs  # Scroll/viewport management
│   │   │   ├── gutter.rs    # Line numbers, fold indicators
│   │   │   └── minimap.rs   # Minimap rendering
│   │   ├── view/
│   │   │   ├── mod.rs
│   │   │   ├── frame_timer.rs  # Frame timing for 120fps target
│   │   │   ├── line_cache.rs   # Windowed line caching for large files
│   │   │   └── editor_view.rs  # Main editor view integration
│   │   ├── input/
│   │   │   ├── mod.rs
│   │   │   ├── keyboard.rs  # Key event handling
│   │   │   ├── mouse.rs     # Mouse event handling
│   │   │   └── ime.rs       # IME composition
│   │   ├── search/
│   │   │   ├── mod.rs
│   │   │   ├── find.rs      # Search implementation
│   │   │   └── replace.rs   # Replace operations
│   │   └── theme/
│   │       ├── mod.rs
│   │       ├── colors.rs    # Color definitions
│   │       └── fonts.rs     # Font configuration
│   └── Cargo.toml
│
├── iridium-syntax/          # Syntax highlighting crate
│   ├── src/
│   │   ├── lib.rs
│   │   ├── highlight.rs     # Highlighting engine
│   │   ├── languages/
│   │   │   ├── mod.rs
│   │   │   ├── cypher.rs    # Cypher configuration
│   │   │   ├── sql.rs       # SQL configuration
│   │   │   ├── rust.rs      # Rust configuration
│   │   │   ├── python.rs    # Python configuration
│   │   │   └── typescript.rs
│   │   └── folding.rs       # Code folding regions
│   └── Cargo.toml
│
└── iridium-bindings/        # JS/WASM bindings crate
    ├── src/
    │   ├── lib.rs           # napi-rs exports
    │   ├── editor.rs        # Editor wrapper
    │   ├── events.rs        # Event types
    │   └── types.rs         # Shared types
    └── Cargo.toml

tests/
├── integration/             # Integration tests
├── visual/                  # Visual regression tests
└── benchmarks/              # Performance benchmarks

examples/
├── web/                     # Web example (React)
└── native/                  # Native example (winit)
```

**Structure Decision**: Rust workspace with three crates following separation of concerns:
1. `iridium-editor` - Core editor logic, rendering, input handling
2. `iridium-syntax` - Tree-sitter integration, language support (can be used standalone)
3. `iridium-bindings` - napi-rs bindings for TypeScript/WASM

This structure allows:
- `iridium-syntax` to be used by both `iridium-editor` and future `iridium-lsp` crate
- Native and WASM builds from same codebase via napi-rs v3
- Clear dependency boundaries (syntax doesn't depend on rendering)

## Complexity Tracking

> No constitution violations requiring justification. Architecture follows primitives principle.

| Decision | Rationale | Simpler Alternative Rejected |
|----------|-----------|------------------------------|
| Three crates | Clean separation, reusability | Single crate would couple syntax to rendering |
| Undo tree (not stack) | User requirement, prevents data loss | Linear undo loses history branches |
| Command-sourced | Enables undo tree, debugging, replay | Direct mutation harder to reason about |

## Phase Summary

| Phase | Output | Purpose |
|-------|--------|---------|
| Phase 0 | research.md | Resolve technical questions, validate approach |
| Phase 1 | data-model.md, contracts/, quickstart.md | Define entities, API, getting started |
| Phase 2 | tasks.md | Actionable implementation tasks (via /speckit.tasks) |
