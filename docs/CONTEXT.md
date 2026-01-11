# Iridium: Project Context

This document captures the full context of decisions, research, and rationale behind Iridium. It serves as onboarding material for agents and contributors.

---

## Origin

Iridium emerged from work on ManifoldDB, a unified graph-vector database. After completing the GraphQL server with real-time subscriptions, the next step was building a user interface—something like Memgraph Lab for querying and visualizing graph data.

During pre-flight checks for the UI, we confirmed the backend was ready:
- GraphQL queries for nodes, edges, Cypher, SQL
- Mutations for CRUD operations (including batch mutations and edge updates)
- WebSocket subscriptions for real-time graph changes
- Vector search capabilities

The question became: what should the query editor component be?

---

## Research: Existing Solutions

We surveyed existing graph database UIs and code editor options.

### Graph Database UIs Reviewed

| Tool | Notes |
|------|-------|
| Memgraph Lab | Multi-pane layout, Graph Style Script, natural language queries. Closed source. |
| Neo4j Browser | Open source. React-based, has reusable `neo4j-arc` component library. |
| Neo4j Bloom | Codeless exploration, perspectives. Closed source (enterprise). |
| NebulaGraph Studio | Open source. Schema designer, import wizard, query editor. |
| Qdrant Web UI | Open source. Clean vector collection browser, REST console. |
| G.V() | Multi-database IDE. Closed source (commercial). |

### Code Editor Components Reviewed

| Library | Notes |
|---------|-------|
| Monaco Editor | VS Code's editor. Feature-rich but ~5MB bundle, complex setup. |
| CodeMirror 6 | Modular, modern, Lezer parser for custom languages. Solid choice. |
| Ace Editor | Battle-tested, 10+ years, easier setup than Monaco. |
| react-simple-code-editor | Lightweight, pairs with Prism/Shiki for highlighting. |

The user has a custom WebGPU graph rendering library already in development. The question was whether to use an existing editor component or build something better.

---

## The Decision: Build a GPU-Accelerated Editor

The goal shifted from "use an existing editor" to "build the best possible single-file text editor"—one that renders at 120fps with the same fluid feel as Zed.

### Why Build Custom

1. **Performance**: Existing web editors don't hit 120fps. GPU-accelerated rendering with WebGPU can.
2. **Cross-platform from one codebase**: Rust + wgpu compiles to native (Vulkan/Metal/DX12) and web (WebGPU/WASM) from identical code.
3. **Reusability**: The editor will be used in multiple projects beyond ManifoldDB's UI.
4. **Control**: No dependency on large, complex projects like Monaco with their bundle size and configuration overhead.

### Why Rust

- Performance and safety
- Strong ecosystem for the required components (wgpu, glyphon, tree-sitter, ropey)
- Same codebase compiles to native binaries and WASM
- User preference: "I just generally prefer to work in Rust where I can"

---

## Technical Architecture

### Core Stack

| Component | Choice | Purpose |
|-----------|--------|---------|
| Language | Rust | Performance, safety, cross-platform |
| GPU Abstraction | wgpu | Vulkan/Metal/DX12/WebGPU from one API |
| Text Rendering | glyphon + cosmic-text | GPU text rendering with proper Unicode/shaping |
| Text Buffer | ropey | Rope data structure for efficient large-file editing |
| Parsing | tree-sitter | Incremental parsing, syntax highlighting |
| LSP Client | tower-lsp or custom | Language intelligence features |
| JS Bindings | napi-rs v3 | TypeScript bindings for Node/Bun/Deno and WASM |

### Why wgpu

wgpu is a Rust implementation of the WebGPU standard. It abstracts over:
- Vulkan (Linux, Windows, Android)
- Metal (macOS, iOS)
- DX12 (Windows)
- WebGPU (browsers via WASM)

The same rendering code runs everywhere. No platform-specific rendering logic needed.

### Why glyphon

glyphon is a fast GPU text renderer built on wgpu. It:
- Uses cosmic-text for text shaping and layout (handles Unicode, RTL, ligatures)
- Rasterizes glyphs into a texture atlas
- Renders text as textured quads on the GPU

This enables sub-millisecond frame times and smooth 120fps rendering.

### JavaScript/TypeScript Integration

We evaluated options for Rust → JavaScript bindings:

| Option | Target | Notes |
|--------|--------|-------|
| wasm-bindgen + wasm-pack | Browser + Node | Standard WASM approach, ~67-93% native speed |
| napi-rs | Node.js native | Full native speed, like PyO3 for Python |
| napi-rs v3 | Both | Same code compiles to native OR WASM |

**Decision**: napi-rs v3, which allows the same Rust codebase to compile as either a native Node module (for Node/Bun/Deno) or as WASM (for browsers). This avoids maintaining separate bindings.

### Runtime Compatibility

| Runtime | napi-rs Support |
|---------|-----------------|
| Node.js | Full |
| Bun | Full (Node-API compatible) |
| Deno 2.6+ | Works (Node-API supported, needs `--allow-ffi`) |
| Browser | Via WASM build |

---

## Architecture: Headless Core + Thin Host Integration

Iridium is a **headless editor core**. It manages text state, parsing, and rendering—but knows nothing about React, DOM structure, or application framework.

```
┌─────────────────────────────────────────────────────────────┐
│                     Host Application                         │
│              (React, native window, Electron)                │
└─────────────────────────────────────────────────────────────┘
                              │
                    Props in, Events out
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                     Iridium Editor                           │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   Input     │  │   Buffer    │  │     Rendering       │  │
│  │  Handling   │──│   (ropey)   │──│   (wgpu/glyphon)    │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
│         │                │                    │              │
│         ▼                ▼                    ▼              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   Cursor    │  │ Tree-sitter │  │      Viewport       │  │
│  │   State     │  │   Parser    │  │    Management       │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
│                          │                                   │
│                          ▼                                   │
│                   ┌─────────────┐                           │
│                   │ LSP Client  │                           │
│                   └─────────────┘                           │
└─────────────────────────────────────────────────────────────┘
```

### React Integration

The React wrapper is minimal—it just mounts a canvas and bridges props/events:

```tsx
function CypherEditor({ value, onChange, theme }) {
    const canvasRef = useRef<HTMLCanvasElement>(null)
    const editorRef = useRef<IridiumEditor | null>(null)

    useEffect(() => {
        initEditor(canvasRef.current!).then(editor => {
            editorRef.current = editor
            editor.setText(value)
            editor.setTheme(theme)
            editor.onContentChange((newValue) => onChange(newValue))
        })
        return () => editorRef.current?.destroy()
    }, [])

    return <canvas ref={canvasRef} />
}
```

React doesn't re-render the editor content—the Rust render loop does that at 120fps, independent of React's lifecycle.

---

## Scope: Single-File Editor

Iridium is explicitly a **single-file text editor component**. It handles everything inside the editing surface.

### In Scope

- Text display and GPU rendering
- Text manipulation (insert, delete, selection, clipboard)
- Cursor management (single and multiple cursors)
- Syntax highlighting (tree-sitter)
- Language intelligence (LSP integration)
- Code navigation, diagnostics, completions
- Search and replace
- Code folding
- Minimap
- Theming

### Out of Scope

These will be separate components that integrate with Iridium:

- File browser / project tree
- Tab management / multi-file editing
- Terminal integration
- Version control UI
- Command palette
- Extension/plugin system

Iridium is the editing core. The IDE shell is built around it, not inside it.

---

## Philosophy: Complete Features, No Half Measures

A critical principle: **there is no MVP version of a feature**.

When we implement a feature, we implement it fully:
- If we add undo/redo, it handles all edge cases, grouping, and persistence
- If we add selection, it supports all modes, keyboard and mouse, with proper rendering
- If we add syntax highlighting, it's fast, accurate, and handles incremental updates

We build incrementally—feature by feature—but each feature reaches production quality before moving to the next. No technical debt under the guise of "good enough for now."

The bar is not "does it work?" but "would this be acceptable in a professional tool?"

---

## Effort Estimate

For a production-quality single-file editor with all features:

| Phase | Effort | Deliverable |
|-------|--------|-------------|
| Basic editing | 3-4 weeks | Text rendering, typing, cursor, selection, scrolling |
| Full features | 6-8 weeks | All editing features complete (search, folding, minimap, etc.) |
| Language intelligence | +2-3 weeks | Tree-sitter + LSP integration |
| Polish | +2-3 weeks | Performance optimization, testing, documentation |

This assumes experience with Rust and GPU programming.

---

## Naming

"Iridium" was chosen for multiple resonances:

1. **Fountain pen nibs**: Iridium tips fine fountain pens for its hardness and precision—fitting for a writing instrument.
2. **Extraterrestrial origin**: Nearly all Earth's iridium arrived via asteroid impacts. It's literally space metal.
3. **Etymology**: Named from Greek "iris" (rainbow) because its salts are intensely colored—connecting to syntax highlighting.
4. **Rarity and quality**: One of the rarest elements, associated with precision and durability.

Iridium joins sibling projects:
- **ManifoldDB** - Graph-vector database (spatial manifold concept)
- **Hematite** - Polyglot notebook (raw material/mineral)
- **Tessera** - Embedding library (Latin for mosaic tile)
- **Meridian** - Server (line that joins things)

---

## Related Documents

- `VISION.md` - Goals, success criteria, technical foundation
- `CODING_STANDARDS.md` - Code organization, file size limits, dependency management

---

## Next Steps

1. **Detailed Specification**: Feature-by-feature specs with exact behaviors, keyboard shortcuts, edge cases
2. **API Design**: Public API for embedders (React, native hosts)
3. **Architecture Specification**: Internal modules, data flow, state management
4. **Implementation**: Following the spec, feature by feature, each complete before moving on
