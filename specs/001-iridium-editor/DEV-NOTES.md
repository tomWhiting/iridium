# Development Notes: Iridium Editor Scaffold

Last updated: 2026-01-11

## Scaffold Status

This document tracks implementation status beyond the task checkboxes.

### Fully Implemented (with tests)

These have real logic, not just type definitions:

| Tasks | Component | Notes |
|-------|-----------|-------|
| T014-T019 | Core types (Position, Range, Selection, CursorState, Document, LineEnding) | Full implementations with Ord, methods, tests |
| T020, T022 | Command enum + inverse() | All variants, inverse logic works |
| T023-T028 | Theme system (Color, EditorColors, SyntaxColors, Typography, Theme) | Hex parsing, dark/light presets |
| T029 | EditorConfig | All options with defaults |
| T030-T031 | Error types | IridiumError, ErrorCode with From impl |
| T037-T038 | EditorState, EditorEvent | Composes all state, event variants defined |
| T039-T042 | Document operations | insert/delete/replace/offset conversions work with ropey 2.0 |
| T062-T070, T072 | UndoTree core | Full tree implementation: push, undo, redo, branching, jump_to_node, grouping |

### Type Definitions Only (stubs behind them)

These tasks are "done" in that the types exist, but the systems using them are stubs:

| Tasks | Component | What's Missing |
|-------|-----------|----------------|
| T087-T088 | HighlightType, HighlightSpan | Types defined, but `Highlighter.highlight()` returns `Vec::new()` |
| T127-T128 | FoldKind, FoldRegion | Types defined, but `FoldDetector.detect()` returns `Vec::new()` |

### Known Stubs (placeholder implementations)

These modules have `_placeholder: ()` fields and empty method bodies:

- `input/keyboard.rs` - KeyboardHandler
- `input/mouse.rs` - MouseHandler
- `input/ime.rs` - ImeHandler
- `render/pipeline.rs` - RenderPipeline
- `render/text.rs` - TextRenderer
- `render/gutter.rs` - GutterRenderer
- `render/minimap.rs` - MinimapRenderer
- `syntax/highlight.rs` - Highlighter (type exists, methods stub)
- `syntax/folding.rs` - FoldDetector (type exists, methods stub)
- `syntax/languages/*.rs` - All language configs
- `bindings/events.rs` - EventEmitter

### Not Started

- T013: `tests/` directory structure
- T021: `Command::apply()` method
- T032-T036: GPU pipeline initialization (wgpu, glyphon)
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
