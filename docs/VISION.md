# Iridium: Vision & Goals

## Overview

Iridium is a GPU-accelerated text editor component built in Rust, targeting both native desktop and web (via WebAssembly). It renders at 120fps using WebGPU/wgpu, delivering the fluid, responsive editing experience found in modern native editors like Zed—but portable to the browser.

This document defines the vision, goals, and scope for Iridium. It serves as the foundation for detailed specification work.

---

## Philosophy: Complete Features, No Half Measures

**Every feature is its own unit of completeness.** There is no "MVP version" of a feature that ships incomplete with the intention of finishing it later. When we implement a feature, we implement it fully:

- If we add undo/redo, it handles all edge cases, grouping, and persistence
- If we add selection, it supports all selection modes, keyboard and mouse, with proper rendering
- If we add syntax highlighting, it's fast, accurate, and handles incremental updates

This means we build incrementally—feature by feature—but each feature reaches production quality before moving to the next. We do not accumulate technical debt under the guise of "good enough for now."

**The bar is not "does it work?" but "would this be acceptable in a professional tool?"**

---

## Scope

### In Scope: Single-File Text Editor

Iridium is a **single-file text editor component**. It handles everything that happens inside the editing surface:

- Text display and rendering (GPU-accelerated)
- Text manipulation (insert, delete, selection, clipboard)
- Cursor and caret management (single and multiple cursors)
- Syntax highlighting (via tree-sitter)
- Language intelligence (via LSP)
- Code navigation (go to definition, find references)
- Diagnostics display (errors, warnings, hints)
- Code actions and completions
- Search and replace within the file
- Code folding
- Minimap
- Theming and customization

### Out of Scope: IDE Shell Features

The following are explicitly out of scope for this phase. They will be built as separate components that integrate with Iridium:

- File browser / project tree
- Tab management / multi-file editing
- Terminal integration
- Version control UI
- Build system integration
- Extension/plugin system
- Settings UI
- Command palette (will be a separate overlay component)

Iridium is the **editing core**. It receives a file's content, allows the user to edit it, and emits changes. Everything outside the editing surface is a different component.

---

## Goals

### 1. Performance That Feels Native

- 120fps rendering with no frame drops during normal editing
- Sub-frame response to keystrokes (input → screen < 8ms)
- Smooth scrolling through files of any size (100k+ lines)
- No perceptible lag when typing, even with syntax highlighting and LSP active

### 2. Cross-Platform From One Codebase

- Same Rust codebase compiles to native (macOS, Linux, Windows) and WASM (browser)
- No platform-specific rendering code—wgpu abstracts the GPU backend
- Platform-specific code limited to: window creation, clipboard, IME input

### 3. Full Language Intelligence

- Tree-sitter integration for syntax highlighting and structural navigation
- LSP client for completions, diagnostics, hover, go-to-definition, references
- Designed for multiple language support, not hardcoded to one grammar

### 4. Professional Editing Features

The editor must support everything a developer expects:

| Category | Features |
|----------|----------|
| **Text Editing** | Insert, delete, overwrite, auto-indent, smart brackets |
| **Selection** | Click, double-click (word), triple-click (line), shift+arrows, ctrl+D (select word), rectangular/column selection |
| **Multiple Cursors** | Add cursor above/below, add cursor at next match, edit at all cursors simultaneously |
| **Navigation** | Arrow keys, home/end, ctrl+arrows (word), page up/down, go to line, go to definition, go to references |
| **Search** | Find, find next/prev, find all, replace, replace all, regex support, case sensitivity, whole word |
| **Clipboard** | Cut, copy, paste, paste from history (if platform supports) |
| **Undo/Redo** | Full history, operation grouping (typing groups until pause), persistent across sessions (optional) |
| **Code Folding** | Fold/unfold regions, fold all, unfold all, fold level N |
| **Minimap** | Scaled overview of document, click to navigate, highlights current viewport |
| **Diagnostics** | Underline errors/warnings, gutter icons, hover for details, quick fix actions |
| **Completions** | Autocomplete dropdown, documentation preview, signature help |
| **Formatting** | Format document, format selection (via LSP) |
| **Comments** | Toggle line comment, toggle block comment |
| **Indentation** | Indent, outdent, auto-detect tabs vs spaces |

### 5. Theming and Appearance

- Full theme support (colors, fonts, spacing)
- Syntax highlighting colors per token type
- Support for popular theme formats (VS Code themes as reference)
- Light and dark mode
- Customizable font, font size, line height, letter spacing

### 6. Accessibility

- Screen reader support where platform allows
- High contrast mode
- Keyboard-only navigation (no mouse required)
- Configurable cursor blink rate (including no blink)

---

## Technical Foundation

These decisions have been made based on prior research and discussion:

### Core Stack

| Component | Choice | Rationale |
|-----------|--------|-----------|
| **Language** | Rust | Performance, safety, cross-platform, strong ecosystem |
| **GPU Abstraction** | wgpu | Cross-platform (Vulkan/Metal/DX12/WebGPU), same code native and web |
| **Text Rendering** | glyphon + cosmic-text | Fast GPU text rendering, proper Unicode/shaping support |
| **Text Buffer** | ropey | Efficient rope data structure for large files |
| **Parsing** | tree-sitter | Incremental parsing, excellent language support |
| **LSP Client** | tower-lsp or custom | Language Server Protocol for intelligence features |
| **WASM Bindings** | wasm-bindgen + wasm-pack | TypeScript types, npm packaging |
| **Native Bindings** | napi-rs v3 | Node.js/Bun/Deno support, same codebase as WASM |

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Host Application                         │
│            (React app, Electron, native window)              │
└─────────────────────────────────────────────────────────────┘
                              │
                    Props in, Events out
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                      Iridium Editor                            │
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
                              │
                              ▼
                    ┌─────────────────┐
                    │  Language       │
                    │  Servers        │
                    │  (external)     │
                    └─────────────────┘
```

### Integration Model

Iridium is a **headless-ish component**. The host provides:
- A canvas/surface to render into
- Content to edit (string or file path)
- Configuration (theme, language, LSP endpoints)

Iridium provides:
- Rendering at 120fps
- Edit operations
- Events (content changed, cursor moved, diagnostics updated)

For React integration:
```tsx
<Iridium
  content={code}
  language="cypher"
  theme={myTheme}
  onChange={(newContent) => setCode(newContent)}
  onCursorChange={(position) => ...}
  lspEndpoint="ws://localhost:3000/lsp"
/>
```

The React wrapper is minimal—it just manages the canvas lifecycle and bridges props/events.

---

## Success Criteria

Iridium is complete when:

1. **Performance**: Renders at 120fps on a mid-range laptop (M1 MacBook Air, equivalent Windows/Linux). No frame drops during typing, scrolling, or completions.

2. **Feature Completeness**: Every feature in the "Professional Editing Features" table is implemented and works correctly. No feature is partial or degraded.

3. **Cross-Platform**: The same build runs natively on macOS, Linux, Windows, and in modern browsers (Chrome, Firefox, Safari, Edge) via WASM/WebGPU.

4. **Language Intelligence**: LSP integration works for at least three languages (TypeScript, Rust, Python). Completions, diagnostics, and navigation function correctly.

5. **Large File Handling**: Opens, edits, and scrolls through a 100,000-line file without lag.

6. **Test Coverage**: Core editing operations have unit tests. Integration tests verify LSP communication. Visual regression tests catch rendering issues.

7. **Documentation**: API documentation for embedders. Architecture documentation for contributors.

---

## Future Integration

Iridium is designed to be the editing core of a larger system. Future components that will integrate with Iridium include:

- **File Browser**: Selects files, passes content to Iridium
- **Tab Bar**: Manages multiple Iridium instances, one per open file
- **Command Palette**: Overlay that sends commands to Iridium
- **Terminal**: Separate component, may share theming
- **Debugger**: Integrates with Iridium for breakpoints, stepping
- **AI Assistant**: Reads Iridium content, applies edits via Iridium API

These are not in scope now, but Iridium's API should not preclude them.

---

## Non-Goals

To be explicit about what Iridium is not:

- **Not a framework**: It's a component, not a toolkit for building editors
- **Not extensible via plugins**: Language support comes from tree-sitter/LSP, not a plugin API
- **Not a document editor**: It's for code, not rich text or markdown preview
- **Not collaborative**: No real-time multi-user editing (could be added later, but not in scope)

---

## Naming

"Iridium" is the element used to tip fine fountain pen nibs—chosen for its hardness, precision, and extraterrestrial origin (nearly all Earth's iridium arrived via asteroid impacts). The name reflects both the tool's purpose (a precision writing instrument) and its quality bar. The element's name derives from Greek "iris" (rainbow), connecting to syntax highlighting's colorful display.

Iridium sits alongside sibling projects: ManifoldDB (database), Hematite (polyglot notebook), Tessera (embedding library), and Meridian (server).

---

## Next Steps

This document provides the foundation. The next phase is detailed specification:

1. **Feature Specifications**: For each feature category, define exact behaviors, keyboard shortcuts, edge cases
2. **API Design**: Define the public API for embedders (React, native hosts)
3. **Architecture Specification**: Define internal modules, data flow, state management
4. **Test Plan**: Define testing strategy for each component

These specifications will guide implementation.
