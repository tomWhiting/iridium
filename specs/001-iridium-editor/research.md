# Research: Iridium GPU-Accelerated Text Editor

**Date**: 2026-01-11
**Branch**: `001-iridium-editor`

## Overview

This document captures technical research and decisions for the Iridium editor implementation.

---

## 1. GPU Rendering Stack

### Decision: wgpu + glyphon + cosmic-text

**Rationale**: This stack provides cross-platform GPU rendering with proper text handling.

- **wgpu 28.x**: Cross-platform GPU abstraction supporting WebGPU (browsers), Vulkan (Linux/Windows), Metal (macOS), DX12 (Windows). Same code runs on web and native. MSRV is 1.82.
- **glyphon**: Fast 2D text renderer built on wgpu. Uses middleware pattern to render into existing render passes (no extra passes). Handles glyph caching and GPU texture atlas.
- **cosmic-text**: Pure Rust text shaping from Pop!_OS team. Handles complex scripts, bidirectional text, font fallback, ligatures. Uses rustybuzz for shaping, swash for rasterization.

**Alternatives Considered**:
| Option | Rejected Because |
|--------|-----------------|
| wgpu_glyph | Less actively maintained than glyphon, glyphon is the successor |
| Direct glyph rendering | Reinventing the wheel, cosmic-text already solves Unicode correctly |
| Canvas2D fallback | Doesn't meet 120fps requirement, different rendering path to maintain |

**Browser Support** (as of late 2025):
- Chrome 113+: WebGPU enabled by default
- Firefox 141+: WebGPU enabled by default (July 2025)
- Safari 26+: WebGPU enabled by default (June 2025)
- Edge 113+: WebGPU enabled by default

---

## 2. Text Buffer

### Decision: ropey 1.6.x

**Rationale**: Ropey is the standard rope implementation for Rust text editors.

**Performance Characteristics**:
- O(log N) for most operations
- 1.8+ million incoherent insertions/second on mobile i7
- 3.3+ million coherent insertions/second
- 10% memory overhead (100MB file → 110MB in memory)
- Flat, predictable performance - no stutters
- SIMD-accelerated (enabled by default)

**Key Features**:
- Proper Unicode support (char indices, not byte indices)
- Clone-on-write for snapshots (8 bytes initial clone)
- Thread-safe for concurrent access
- Line indexing built-in

**Alternatives Considered**:
| Option | Rejected Because |
|--------|-----------------|
| Gap buffer | Not thread-safe, poor concurrent access, can't snapshot |
| crop (by Nomad) | Newer, less battle-tested than ropey |
| Custom rope | Unnecessary reinvention |
| ropey 2.0 beta | Still in beta, 1.6.x is battle-tested |

---

## 3. Syntax Highlighting

### Decision: tree-sitter with language-specific grammars

**Rationale**: Tree-sitter provides incremental parsing, essential for real-time highlighting during editing.

**Language Grammars**:

| Language | Grammar | Notes |
|----------|---------|-------|
| Cypher | [pupli/tree-sitter-cypher](https://github.com/pupli/tree-sitter-cypher) | Based on openCypher standard, designed for editor support |
| SQL | [m-novikov/tree-sitter-sql](https://github.com/m-novikov/tree-sitter-sql) | PostgreSQL-focused, lax parsing for editor use |
| Rust | tree-sitter-rust (official) | Bundled with tree-sitter |
| Python | tree-sitter-python (official) | Bundled with tree-sitter |
| TypeScript | tree-sitter-typescript (official) | Bundled with tree-sitter |

**Integration Approach**:
- Bundle grammar .so/.wasm files with the crate
- Lazy-load grammars on first use for that language
- Incremental re-parsing on edit (only affected nodes)
- Highlight queries stored as .scm files per language

**Alternatives Considered**:
| Option | Rejected Because |
|--------|-----------------|
| TextMate grammars | Regex-based, not incremental, poor performance on large files |
| Monaco tokenizer | Tied to Monaco, not portable |
| Custom lexer | Would need to maintain for each language |

---

## 4. JavaScript/TypeScript Bindings

### Decision: napi-rs v3

**Rationale**: napi-rs v3 allows the same Rust code to compile to both native Node addon and WASM with no code changes.

**Key Features**:
- Single codebase for native and WASM builds
- Full TypeScript type generation
- Supports `std::thread` and tokio in WASM via wasm32-wasip1-threads
- Cross-compilation support

**Build Targets**:
- Native: Node.js, Bun, Deno 2.6+
- WASM: All browsers with WebGPU support

**Alternatives Considered**:
| Option | Rejected Because |
|--------|-----------------|
| wasm-bindgen + napi-rs separate | Two different binding layers to maintain |
| wasm-bindgen only | No native support, WASM has overhead |
| FFI (C ABI) | More complex integration, no TypeScript types |

---

## 5. Undo/Redo Architecture

### Decision: Command-sourced with tree-structured history

**Rationale**: User requirement for branching undo. Command-sourcing enables this naturally.

**Data Structure**:
```
UndoTree {
    nodes: Vec<UndoNode>,
    current: NodeId,
}

UndoNode {
    id: NodeId,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    command: Command,        // The edit that was applied
    inverse: Command,        // How to undo it
    timestamp: Instant,
    document_hash: u64,      // For validation
}
```

**Behavior**:
- Undo: Apply inverse command, move to parent node
- Redo: Apply command of selected child, move to that child
- Branch: When editing after undo, current node gets new child (branch)
- Grouping: Continuous typing within 500ms grouped into single node

**Navigation**:
- Expose tree structure via API for visualization
- Previous sibling / next sibling for branch switching
- Jump to any node by ID

**Alternatives Considered**:
| Option | Rejected Because |
|--------|-----------------|
| Linear stack | Loses history on branch, violates user requirement |
| Operation Transform (OT) | Overkill for single-user editor |
| CRDT | Overkill, designed for collaborative editing |

---

## 6. Command Architecture

### Decision: Typed command enum with Apply/Invert

**Rationale**: Commands are the unit of change. Each command knows how to apply and invert itself.

**Command Types**:
```rust
enum Command {
    Insert { position: Position, text: String },
    Delete { range: Range, deleted_text: String },
    Replace { range: Range, old_text: String, new_text: String },
    SetSelection { old: Vec<Selection>, new: Vec<Selection> },
    Compound { commands: Vec<Command> },  // For grouping
}
```

**Properties**:
- **Invertible**: Every command has an inverse
- **Composable**: Multiple commands can be grouped
- **Serializable**: Can be persisted for session recovery
- **Deterministic**: Same command on same document state = same result

---

## 7. Rendering Pipeline

### Decision: Immediate mode, single render pass

**Rationale**: Immediate mode is simpler and 120fps makes it fast enough.

**Pipeline Structure**:
1. **Layout pass**: Compute visible lines, glyph positions (cosmic-text)
2. **Prepare pass**: Upload changed glyphs to atlas, prepare instance buffers
3. **Render pass**:
   - Background (editor background)
   - Selection highlights
   - Text glyphs (instanced quads sampling from atlas)
   - Cursor(s)
   - Minimap (if enabled)
   - UI overlays (search panel, etc.)

**Optimizations**:
- Only re-layout lines that changed
- Glyph atlas caching (glyphon handles this)
- Viewport culling (only render visible lines)
- Instance batching for text rendering

---

## 8. Input Handling

### Decision: Platform-agnostic event abstraction

**Rationale**: Browser and native have different event models. Abstract to common interface.

**Event Types**:
```rust
enum InputEvent {
    KeyDown { key: Key, modifiers: Modifiers },
    KeyUp { key: Key, modifiers: Modifiers },
    Character { char: char },  // For IME/text input
    MouseDown { position: (f32, f32), button: MouseButton },
    MouseUp { position: (f32, f32), button: MouseButton },
    MouseMove { position: (f32, f32) },
    Scroll { delta: (f32, f32) },
    CompositionStart,
    CompositionUpdate { text: String },
    CompositionEnd { text: String },
}
```

**Key Mapping**:
- Define default keybindings in code
- Allow full customization via config
- Platform-specific defaults (Cmd vs Ctrl)

---

## 9. Theme System

### Decision: Structured theme definition with hot reload

**Rationale**: Need to support VS Code-style themes and runtime updates.

**Theme Structure**:
```rust
struct Theme {
    // Editor chrome
    background: Color,
    foreground: Color,
    selection: Color,
    cursor: Color,
    line_number: Color,
    current_line: Color,

    // Syntax
    syntax: SyntaxColors,

    // Typography
    font_family: String,
    font_size: f32,
    line_height: f32,
}

struct SyntaxColors {
    keyword: Color,
    string: Color,
    number: Color,
    comment: Color,
    function: Color,
    variable: Color,
    type_name: Color,
    operator: Color,
    punctuation: Color,
    // ... etc
}
```

**Hot Reload**:
- Theme changes trigger immediate re-render
- No restart required
- Colors are uniforms in shader, cheap to update

---

## 10. Performance Targets

### Decision: 120fps with sub-8ms input latency

**Breakdown**:
- Frame budget at 120fps: 8.33ms
- Target frame time: <6ms (leaves headroom)
- Input processing: <1ms
- Layout (changed lines): <1ms typical
- Render: <4ms

**Validation**:
- Performance benchmarks in CI
- Frame time histogram in debug builds
- Automated regression detection

---

## References

- [wgpu GitHub](https://github.com/gfx-rs/wgpu)
- [glyphon GitHub](https://github.com/grovesNL/glyphon)
- [cosmic-text GitHub](https://github.com/pop-os/cosmic-text)
- [ropey GitHub](https://github.com/cessen/ropey)
- [tree-sitter GitHub](https://github.com/tree-sitter/tree-sitter)
- [napi-rs v3 Announcement](https://napi.rs/blog/announce-v3)
- [tree-sitter-cypher](https://github.com/pupli/tree-sitter-cypher)
- [tree-sitter-sql (PostgreSQL)](https://github.com/m-novikov/tree-sitter-sql)
- [WebGPU Browser Support](https://web.dev/blog/webgpu-supported-major-browsers)
- [Zed Blog: Rope & SumTree](https://zed.dev/blog/zed-decoded-rope-sumtree)
