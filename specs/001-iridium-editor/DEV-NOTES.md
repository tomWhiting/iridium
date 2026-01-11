# Development Notes: Iridium Editor Scaffold

Last updated: 2026-01-11

## Scaffold Status

This document tracks implementation status beyond the task checkboxes.

### Fully Implemented (with tests)

These have real logic, not just type definitions:

| Tasks | Component | Notes |
|-------|-----------|-------|
| T014-T019 | Core types (Position, Range, Selection, CursorState, Document, LineEnding) | Full implementations with Ord, methods, tests |
| T020-T022 | Command enum + apply() + inverse() | All variants, apply/inverse logic work, comprehensive tests |
| T023-T028 | Theme system (Color, EditorColors, SyntaxColors, Typography, Theme) | Hex parsing, dark/light presets |
| T029 | EditorConfig | All options with defaults |
| T030-T031 | Error types | IridiumError, ErrorCode with From impl |
| T037-T038 | EditorState, EditorEvent | Composes all state, event variants defined |
| T039-T042 | Document operations | insert/delete/replace/offset conversions work with ropey 2.0 |
| T062-T070, T072 | UndoTree core | Full tree implementation: push, undo, redo, branching, jump_to_node, grouping |
| T032-T036 | GPU Pipeline + Text Rendering | wgpu 28.0 device/queue init, glyphon 0.10 TextRenderer, glyph atlas management |
| T087-T091a | Syntax Highlighting Core | Full Highlighter with tree-sitter, query execution, incremental parsing |
| T094-T096a | Language Support | 13 languages configured: Rust, Python, TypeScript, JavaScript, TSX, Go, JSON, YAML, Markdown, CSS, Bash, C, C++ |
| T097-T098, T101 | Syntax Integration | iridium-syntax integrated, DocumentHighlighter with caching, language detection |

### Type Definitions Only (stubs behind them)

These tasks are "done" in that the types exist, but the systems using them are stubs:

| Tasks | Component | What's Missing |
|-------|-----------|----------------|
| T127-T128 | FoldKind, FoldRegion | Types defined, but `FoldDetector.detect()` returns `Vec::new()` |
| T092-T093 | Cypher, SQL | Query files bundled but grammars incompatible with tree-sitter 0.26 |

### Integration Gaps (partial implementations)

| Tasks | Component | What's Missing |
|-------|-----------|----------------|
| T099 | Syntax Rendering | `TextRenderer.set_rich_text()` exists but not called with highlight data |
| T100 | EditorState Syntax | `DocumentHighlighter` not added to `EditorState` |

### Known Stubs (placeholder implementations)

These modules have `_placeholder: ()` fields and empty method bodies:

- `input/keyboard.rs` - KeyboardHandler
- `input/mouse.rs` - MouseHandler
- `input/ime.rs` - ImeHandler
- `render/gutter.rs` - GutterRenderer
- `render/minimap.rs` - MinimapRenderer
- `syntax/folding.rs` - FoldDetector (type exists, methods stub)
- `bindings/events.rs` - EventEmitter

### Not Started

- T073: `UndoTree::get_node_info()`
- T074: Wiring undo tree into editor operations

## API Notes

### ropey 2.0 Breaking Changes

The scaffold uses ropey 2.0.0-beta.1 which has breaking API changes:

```rust
// Old API (1.x)
rope.len_lines()
rope.line(idx)
rope.line_to_char(line)
rope.char_to_line(offset)

// New API (2.0) - requires LineType parameter
const LINE_TYPE: LineType = LineType::LF_CR;
rope.len_lines(LINE_TYPE)
rope.line(idx, LINE_TYPE)
rope.line_to_byte_idx(line, LINE_TYPE)
rope.byte_to_line_idx(offset, LINE_TYPE)
```

Required Cargo.toml features: `metric_chars`, `metric_lines_lf_cr`

### napi-rs 3.x

Using stable napi-rs 3.8.2 (not alpha). Bindings compile but are minimal stubs.

### wgpu 28.0 API Changes

The project uses wgpu 28.0 which has significant API changes from earlier versions:

```rust
// request_adapter now returns Result, not Option
let adapter = instance.request_adapter(&options).await?;

// enumerate_adapters is now async
let adapters = instance.enumerate_adapters(Backends::all()).await;

// DeviceDescriptor requires trace field
DeviceDescriptor {
    trace: Trace::Off,
    ..Default::default()
}

// RenderPassColorAttachment needs depth_slice
RenderPassColorAttachment {
    depth_slice: None,
    ..
}

// RenderPassDescriptor needs multiview_mask
RenderPassDescriptor {
    multiview_mask: None,
    ..
}
```

Uses `pollster` crate for blocking on async GPU initialization.

### glyphon 0.10 + cosmic-text 0.15/0.16

Text rendering uses glyphon 0.10 with cosmic-text:

```rust
// TextRenderer::new needs mutable atlas
GlyphonTextRenderer::new(&mut atlas, device, MultisampleState::default(), None)

// Buffer methods need 5 args (added alignment parameter)
buffer.set_text(&mut font_system, text, &attrs, Shaping::Advanced, None);
buffer.set_rich_text(&mut font_system, spans, &default_attrs, Shaping::Advanced, None);
```
