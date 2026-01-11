# Tasks: Iridium GPU-Accelerated Text Editor

**Input**: Design documents from `/specs/001-iridium-editor/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

```text
crates/
├── iridium-editor/src/     # Core editor crate
├── iridium-syntax/src/     # Syntax highlighting crate
└── iridium-bindings/src/   # JS/WASM bindings crate
tests/                      # Integration and benchmark tests
examples/                   # Web and native examples
```

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization, workspace setup, and core dependencies

- [x] T001 Create Rust workspace with Cargo.toml at repository root defining three crates
- [x] T002 Initialize iridium-editor crate with Cargo.toml in crates/iridium-editor/
- [x] T003 [P] Initialize iridium-syntax crate with Cargo.toml in crates/iridium-syntax/
- [x] T004 [P] Initialize iridium-bindings crate with Cargo.toml in crates/iridium-bindings/
- [x] T005 Add wgpu, glyphon, cosmic-text dependencies to iridium-editor via cargo add
- [x] T006 [P] Add ropey dependency to iridium-editor via cargo add
- [x] T007 [P] Add tree-sitter dependency to iridium-syntax via cargo add
- [x] T008 [P] Add napi-rs v3 dependencies to iridium-bindings via cargo add
- [x] T009 Configure rustfmt.toml with project formatting rules
- [x] T010 [P] Configure clippy.toml with lint rules
- [x] T011 Create module structure in crates/iridium-editor/src/ with mod.rs files for editor/, document/, history/, render/, input/, search/, theme/
- [x] T012 [P] Create module structure in crates/iridium-syntax/src/ with mod.rs files for languages/
- [x] T013 [P] Create tests/ directory structure with integration/, visual/, benchmarks/

**Checkpoint**: Workspace compiles with `cargo build`, all crates resolve dependencies

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core types and infrastructure that ALL user stories depend on

**CRITICAL**: No user story work can begin until this phase is complete

### Core Types

- [x] T014 Implement Position struct with line/column fields in crates/iridium-editor/src/document/position.rs
- [x] T015 [P] Implement Range struct with start/end positions in crates/iridium-editor/src/document/position.rs
- [x] T016 Implement Selection struct with anchor/head in crates/iridium-editor/src/document/cursor.rs
- [x] T017 Implement CursorState struct with primary and secondary cursors in crates/iridium-editor/src/document/cursor.rs
- [x] T018 Implement Document struct wrapping ropey::Rope in crates/iridium-editor/src/document/buffer.rs
- [x] T019 Implement LineEnding enum and line ending detection in crates/iridium-editor/src/document/buffer.rs

### Command System

- [x] T020 Define Command enum (Insert, Delete, Replace, SetSelection, Compound) in crates/iridium-editor/src/history/commands.rs
- [x] T021 Implement Command::apply() method for all variants in crates/iridium-editor/src/history/commands.rs
- [x] T022 Implement Command::inverse() method for all variants in crates/iridium-editor/src/history/commands.rs

### Theme System (needed for rendering)

- [x] T023 Implement Color struct (RGBA f32) in crates/iridium-editor/src/theme/colors.rs
- [x] T024 [P] Implement EditorColors struct in crates/iridium-editor/src/theme/colors.rs
- [x] T025 [P] Implement SyntaxColors struct in crates/iridium-editor/src/theme/colors.rs
- [x] T026 Implement Typography struct in crates/iridium-editor/src/theme/fonts.rs
- [x] T027 Implement Theme struct combining colors and typography in crates/iridium-editor/src/theme/mod.rs
- [x] T028 Create default dark and light themes in crates/iridium-editor/src/theme/mod.rs

### Configuration

- [x] T029 Implement EditorConfig struct with all configuration options in crates/iridium-editor/src/editor/config.rs

### Error Types

- [x] T030 Define IridiumError enum with all error variants in crates/iridium-editor/src/editor/mod.rs
- [x] T031 [P] Define ErrorCode enum for host communication in crates/iridium-editor/src/editor/mod.rs

### GPU Pipeline Foundation

- [x] T032 Create wgpu Device and Queue initialization in crates/iridium-editor/src/render/pipeline.rs
- [x] T033 Implement GPU error handling with detailed error messages in crates/iridium-editor/src/render/pipeline.rs
- [x] T034 Create basic render pass structure in crates/iridium-editor/src/render/pipeline.rs
- [x] T035 Initialize glyphon TextRenderer and FontSystem in crates/iridium-editor/src/render/text.rs
- [x] T036 Implement glyph atlas management in crates/iridium-editor/src/render/text.rs

### Editor State

- [x] T037 Implement EditorState struct composing Document, CursorState, Theme, Config in crates/iridium-editor/src/editor/core.rs
- [x] T038 Implement EditorEvent enum for host communication in crates/iridium-editor/src/editor/core.rs

**Checkpoint**: Foundation ready - `cargo test` passes, basic types functional

---

## Phase 3: User Story 1 - Basic Text Editing (Priority: P1) MVP

**Goal**: Users can type, delete, select, and navigate text with <8ms latency at 120fps

**Independent Test**: Load content, type characters, use arrow keys, select with shift+arrows, delete selection - all operations feel instant

### Implementation for User Story 1

- [x] T039 [US1] Implement Document::insert() for inserting text at position in crates/iridium-editor/src/document/buffer.rs
- [x] T040 [US1] Implement Document::delete() for deleting range in crates/iridium-editor/src/document/buffer.rs
- [x] T041 [US1] Implement Document::replace() for replacing range in crates/iridium-editor/src/document/buffer.rs
- [x] T042 [US1] Implement position_to_offset and offset_to_position conversions in crates/iridium-editor/src/document/buffer.rs
- [ ] T043 [US1] Implement keyboard event handling for arrow key navigation in crates/iridium-editor/src/input/keyboard.rs
- [ ] T044 [P] [US1] Implement keyboard event handling for Home/End keys in crates/iridium-editor/src/input/keyboard.rs
- [ ] T045 [P] [US1] Implement keyboard event handling for Ctrl+arrows word navigation in crates/iridium-editor/src/input/keyboard.rs
- [ ] T046 [US1] Implement keyboard event handling for character insertion in crates/iridium-editor/src/input/keyboard.rs
- [ ] T047 [US1] Implement keyboard event handling for Backspace/Delete in crates/iridium-editor/src/input/keyboard.rs
- [ ] T048 [US1] Implement keyboard event handling for Shift+arrows selection in crates/iridium-editor/src/input/keyboard.rs
- [ ] T049 [US1] Implement mouse click to position cursor in crates/iridium-editor/src/input/mouse.rs
- [ ] T050 [US1] Implement mouse drag for selection in crates/iridium-editor/src/input/mouse.rs
- [ ] T051 [US1] Implement double-click for word selection in crates/iridium-editor/src/input/mouse.rs
- [ ] T052 [US1] Implement triple-click for line selection in crates/iridium-editor/src/input/mouse.rs
- [ ] T053 [US1] Implement clipboard cut/copy/paste command handling in crates/iridium-editor/src/input/keyboard.rs
- [ ] T054 [US1] Implement text rendering with glyphon in crates/iridium-editor/src/render/text.rs
- [ ] T055 [US1] Implement cursor rendering (blinking caret) in crates/iridium-editor/src/render/text.rs
- [ ] T056 [US1] Implement selection highlight rendering in crates/iridium-editor/src/render/text.rs
- [ ] T057 [US1] Implement current line highlight rendering in crates/iridium-editor/src/render/text.rs
- [ ] T058 [US1] Implement IME composition handling in crates/iridium-editor/src/input/ime.rs
- [ ] T059 [US1] Wire up render loop at 120fps with frame timing in crates/iridium-editor/src/render/pipeline.rs
- [ ] T060 [US1] Implement ContentChanged event emission in crates/iridium-editor/src/editor/core.rs
- [ ] T061 [US1] Implement SelectionChanged event emission in crates/iridium-editor/src/editor/core.rs

**Checkpoint**: User Story 1 complete - basic editing at 120fps is functional

---

## Phase 4: User Story 2 - Undo/Redo with Branching History (Priority: P2)

**Goal**: Full undo tree with branch navigation - no work is ever lost

**Independent Test**: Make changes, undo to mid-point, make different changes, navigate back to original branch

### Implementation for User Story 2

- [x] T062 [US2] Implement UndoNodeId type in crates/iridium-editor/src/history/undo_tree.rs
- [x] T063 [US2] Implement UndoNode struct with parent/children/command/timestamp in crates/iridium-editor/src/history/undo_tree.rs
- [x] T064 [US2] Implement UndoTree struct with nodes HashMap and current pointer in crates/iridium-editor/src/history/undo_tree.rs
- [x] T065 [US2] Implement UndoTree::push() to add new command as child of current in crates/iridium-editor/src/history/undo_tree.rs
- [x] T066 [US2] Implement UndoTree::undo() to apply inverse and move to parent in crates/iridium-editor/src/history/undo_tree.rs
- [x] T067 [US2] Implement UndoTree::redo() to apply command and move to child in crates/iridium-editor/src/history/undo_tree.rs
- [x] T068 [US2] Implement UndoTree::redo_branch() for selecting specific child branch in crates/iridium-editor/src/history/undo_tree.rs
- [x] T069 [US2] Implement UndoTree::jump_to_node() for arbitrary tree navigation in crates/iridium-editor/src/history/undo_tree.rs
- [x] T070 [US2] Implement edit grouping based on 500ms pause timeout in crates/iridium-editor/src/history/undo_tree.rs
- [x] T071 [US2] Implement keyboard bindings for Ctrl+Z (undo) and Ctrl+Y/Ctrl+Shift+Z (redo) in crates/iridium-editor/src/input/keyboard.rs
- [x] T072 [US2] Implement UndoTree::get_tree_info() for exposing tree structure to host in crates/iridium-editor/src/history/undo_tree.rs
- [x] T073 [US2] Implement UndoTree::get_node_info() for individual node inspection in crates/iridium-editor/src/history/undo_tree.rs
- [x] T074 [US2] Wire undo tree into EditorState in crates/iridium-editor/src/editor/core.rs

**Bonus**: `history/stack.rs` - Simple linear undo stack implementation for testing and simpler use cases.

**Checkpoint**: User Story 2 complete - undo tree with branches is functional

---

## Phase 5: User Story 3 - Smooth Scrolling and Viewport (Priority: P3)

**Goal**: 120fps scrolling through 100k+ line files with all input methods

**Independent Test**: Load 100k line file, scroll rapidly with wheel/trackpad/keyboard, verify 120fps maintained

### Implementation for User Story 3

- [x] T075 [US3] Implement Viewport struct with scroll position and dimensions in crates/iridium-editor/src/render/viewport.rs
- [x] T076 [US3] Implement Viewport::scroll_to_line() in crates/iridium-editor/src/render/viewport.rs
- [x] T077 [US3] Implement Viewport::scroll_to_position() in crates/iridium-editor/src/render/viewport.rs
- [x] T078 [US3] Implement Viewport::ensure_cursor_visible() in crates/iridium-editor/src/render/viewport.rs
- [x] T079 [US3] Implement mouse wheel scroll handling in crates/iridium-editor/src/input/mouse.rs
- [x] T080 [US3] Implement trackpad momentum scrolling in crates/iridium-editor/src/input/mouse.rs
- [x] T081 [US3] Implement Page Up/Down keyboard handling in crates/iridium-editor/src/input/keyboard.rs
- [x] T082 [US3] Implement Go to Line command in crates/iridium-editor/src/editor/core.rs
- [x] T083 [US3] Implement viewport culling for rendering only visible lines in crates/iridium-editor/src/render/text.rs
- [x] T084 [US3] Implement line number gutter rendering in crates/iridium-editor/src/render/gutter.rs
- [x] T085 [US3] Implement ScrollChanged event emission in crates/iridium-editor/src/editor/core.rs
- [x] T086 [US3] Optimize rendering for large files with line caching in crates/iridium-editor/src/render/text.rs

**Bonus**: `view/frame_timer.rs` - Frame timing logic for 120fps target; `view/line_cache.rs` - Windowed line caching optimization; `view/editor_view.rs` - Main editor view integration point.

**Checkpoint**: User Story 3 complete - smooth scrolling at 120fps on large files

---

## Phase 6: User Story 4 - Syntax Highlighting (Priority: P4)

**Goal**: Incremental syntax highlighting using bundled TreeSitter query files (.scm) from Zed editor

**Approach**: Use proven highlights.scm, brackets.scm, indents.scm, and injections.scm query files bundled from Zed editor for consistent, comprehensive highlighting. Execute queries against tree-sitter parse trees for each supported language.

**Supported Languages** (with bundled query files):
- Primary: Rust, Python, TypeScript, JavaScript, TSX, Go, JSON, YAML, Markdown, CSS, Bash, C, C++
- Cypher: Uses tree-sitter-cypher git dependency (taekwombo/tree-sitter-cypher) - requires minimal highlights.scm
- SQL: Uses tree-sitter-sql crate - requires minimal highlights.scm

**Version Compatibility Note**: If query files require minor updates to work with newer tree-sitter grammar versions, those updates should be made. If extensive rewrites would be needed, use the latest compatible version of the tree-sitter grammar crate instead.

**Independent Test**: Load files in each language, verify highlighting matches expected token colors, edit and confirm incremental updates without lag

### Implementation for User Story 4

**Core Highlighting Engine:**
- [x] T087 [US4] Implement HighlightType enum in crates/iridium-syntax/src/lib.rs
- [x] T088 [US4] Implement HighlightSpan struct in crates/iridium-syntax/src/lib.rs
- [x] T089 [US4] Create tree-sitter Parser wrapper in crates/iridium-syntax/src/highlight.rs
- [x] T090 [US4] Implement incremental parsing with tree-sitter edit in crates/iridium-syntax/src/highlight.rs
- [x] T091 [US4] Implement highlight query execution using bundled .scm files in crates/iridium-syntax/src/highlight.rs
- [x] T091a [US4] Create query loader to read/embed .scm files from languages/queries/ directory

**Language Configuration (use bundled query files):**
- [ ] T092 [P] [US4] Configure Cypher language - add tree-sitter-cypher git dep, create minimal highlights.scm if needed (BLOCKED: no compatible grammar available)
- [ ] T093 [P] [US4] Configure SQL language - use tree-sitter-sql, create minimal highlights.scm if needed (BLOCKED: tree-sitter-sql uses incompatible tree-sitter 0.19.5)
- [x] T094 [P] [US4] Configure Rust language - wire up bundled queries/rust/*.scm files
- [x] T095 [P] [US4] Configure Python language - wire up bundled queries/python/*.scm files
- [x] T096 [P] [US4] Configure TypeScript/JavaScript/TSX - wire up bundled queries
- [x] T096a [P] [US4] Configure Go, JSON, YAML, Markdown, CSS, Bash, C, C++ - wire up bundled queries

**Integration:**
- [x] T097 [US4] Verify all bundled .scm query files load correctly, fix any version incompatibilities
- [x] T098 [US4] Integrate iridium-syntax into iridium-editor in crates/iridium-editor/Cargo.toml
- [ ] T099 [US4] Implement syntax-colored text rendering in crates/iridium-editor/src/render/text.rs (PARTIAL: set_rich_text exists but not wired to highlighter)
- [ ] T100 [US4] Wire syntax highlighting into EditorState in crates/iridium-editor/src/editor/core.rs (NOT DONE: DocumentHighlighter not in EditorState)
- [x] T101 [US4] Implement language detection from file extension or explicit setting

**Checkpoint**: User Story 4 PARTIAL - Core highlighting engine works for 13/15 languages. Integration with EditorState incomplete (T099, T100).

---

## Phase 7: User Story 5 - Multiple Cursors (Priority: P5)

**Goal**: Add/remove cursors, all cursors edit simultaneously

**Independent Test**: Add multiple cursors with Ctrl+D and Ctrl+Click, type, verify all update

### Implementation for User Story 5

- [ ] T102 [US5] Implement CursorState::add_cursor() in crates/iridium-editor/src/document/cursor.rs
- [ ] T103 [US5] Implement CursorState::remove_cursor() in crates/iridium-editor/src/document/cursor.rs
- [ ] T104 [US5] Implement CursorState::collapse_to_primary() in crates/iridium-editor/src/document/cursor.rs
- [ ] T105 [US5] Implement cursor sorting and overlap prevention in crates/iridium-editor/src/document/cursor.rs
- [ ] T106 [US5] Implement Ctrl+Click to add cursor in crates/iridium-editor/src/input/mouse.rs
- [ ] T107 [US5] Implement Ctrl+D (Add Selection to Next Match) in crates/iridium-editor/src/input/keyboard.rs
- [ ] T108 [US5] Implement Escape to collapse to primary cursor in crates/iridium-editor/src/input/keyboard.rs
- [ ] T109 [US5] Update all editing operations to work with multiple cursors in crates/iridium-editor/src/editor/core.rs
- [ ] T110 [US5] Implement multi-cursor rendering in crates/iridium-editor/src/render/text.rs
- [ ] T111 [US5] Implement multi-selection highlight rendering in crates/iridium-editor/src/render/text.rs

**Checkpoint**: User Story 5 complete - multi-cursor editing works

---

## Phase 8: User Story 6 - Search and Replace (Priority: P6)

**Goal**: Find/replace with regex, case-sensitivity, whole-word options

**Independent Test**: Open search, enter query, navigate matches, replace single and all

### Implementation for User Story 6

- [ ] T112 [US6] Implement SearchOptions struct in crates/iridium-editor/src/search/find.rs
- [ ] T113 [US6] Implement SearchState struct in crates/iridium-editor/src/search/find.rs
- [ ] T114 [US6] Implement find_all() for locating all matches in crates/iridium-editor/src/search/find.rs
- [ ] T115 [US6] Implement regex search support in crates/iridium-editor/src/search/find.rs
- [ ] T116 [US6] Implement case-sensitive/insensitive toggle in crates/iridium-editor/src/search/find.rs
- [ ] T117 [US6] Implement whole-word matching in crates/iridium-editor/src/search/find.rs
- [ ] T118 [US6] Implement next_match() and previous_match() navigation in crates/iridium-editor/src/search/find.rs
- [ ] T119 [US6] Implement replace_current() in crates/iridium-editor/src/search/replace.rs
- [ ] T120 [US6] Implement replace_all() as single compound command in crates/iridium-editor/src/search/replace.rs
- [ ] T121 [US6] Implement Ctrl+F keyboard binding for search in crates/iridium-editor/src/input/keyboard.rs
- [ ] T122 [US6] Implement F3/Shift+F3 for next/previous match in crates/iridium-editor/src/input/keyboard.rs
- [ ] T123 [US6] Implement search match highlighting in crates/iridium-editor/src/render/text.rs
- [ ] T124 [US6] Implement current match highlight (different color) in crates/iridium-editor/src/render/text.rs
- [ ] T125 [US6] Implement SearchUpdated event emission in crates/iridium-editor/src/editor/core.rs
- [ ] T126 [US6] Wire search state into EditorState in crates/iridium-editor/src/editor/core.rs

**Checkpoint**: User Story 6 complete - search and replace with all options

---

## Phase 9: User Story 7 - Code Folding (Priority: P7)

**Goal**: Fold/unfold regions based on syntax structure

**Independent Test**: Load file with blocks, click fold indicators, verify collapse/expand

### Implementation for User Story 7

- [x] T127 [US7] Implement FoldKind enum in crates/iridium-syntax/src/folding.rs
- [x] T128 [US7] Implement FoldRegion struct in crates/iridium-syntax/src/folding.rs
- [ ] T129 [US7] Implement fold region detection from tree-sitter nodes in crates/iridium-syntax/src/folding.rs
- [ ] T130 [US7] Implement fold/unfold state tracking in crates/iridium-editor/src/editor/core.rs
- [ ] T131 [US7] Implement fold_at() operation in crates/iridium-editor/src/editor/core.rs
- [ ] T132 [US7] Implement unfold_at() operation in crates/iridium-editor/src/editor/core.rs
- [ ] T133 [US7] Implement fold_all() and unfold_all() operations in crates/iridium-editor/src/editor/core.rs
- [ ] T134 [US7] Implement fold indicator rendering in gutter in crates/iridium-editor/src/render/gutter.rs
- [ ] T135 [US7] Implement folded region placeholder rendering in crates/iridium-editor/src/render/text.rs
- [ ] T136 [US7] Implement fold indicator click handling in crates/iridium-editor/src/input/mouse.rs
- [ ] T137 [US7] Update viewport to skip folded lines in crates/iridium-editor/src/render/viewport.rs

**Checkpoint**: User Story 7 complete - code folding works with syntax awareness

---

## Phase 10: User Story 8 - Minimap (Priority: P8)

**Goal**: Scaled document overview with viewport indicator and click-to-navigate

**Independent Test**: Load file, verify minimap shows overview, click to navigate, drag viewport

### Implementation for User Story 8

- [ ] T138 [US8] Implement minimap dimensions and scaling calculations in crates/iridium-editor/src/render/minimap.rs
- [ ] T139 [US8] Implement minimap text rendering (scaled down) in crates/iridium-editor/src/render/minimap.rs
- [ ] T140 [US8] Implement minimap syntax coloring in crates/iridium-editor/src/render/minimap.rs
- [ ] T141 [US8] Implement viewport indicator rendering on minimap in crates/iridium-editor/src/render/minimap.rs
- [ ] T142 [US8] Implement minimap click-to-navigate in crates/iridium-editor/src/input/mouse.rs
- [ ] T143 [US8] Implement minimap drag-to-scroll in crates/iridium-editor/src/input/mouse.rs
- [ ] T144 [US8] Wire minimap toggle into EditorConfig in crates/iridium-editor/src/editor/config.rs
- [ ] T145 [US8] Integrate minimap into main render pass in crates/iridium-editor/src/render/pipeline.rs

**Checkpoint**: User Story 8 complete - minimap with navigation works

---

## Phase 11: User Story 9 - Theming (Priority: P9)

**Goal**: Runtime theme changes, light/dark modes, custom fonts

**Independent Test**: Load different themes, verify colors/fonts update without restart

### Implementation for User Story 9

- [ ] T146 [US9] Implement Theme::from_json() for loading theme definitions in crates/iridium-editor/src/theme/mod.rs
- [ ] T147 [US9] Implement runtime theme switching in EditorState in crates/iridium-editor/src/editor/core.rs
- [ ] T148 [US9] Implement font loading and caching in crates/iridium-editor/src/render/text.rs
- [ ] T149 [US9] Implement runtime font change support in crates/iridium-editor/src/render/text.rs
- [ ] T150 [US9] Update shader uniforms for theme colors in crates/iridium-editor/src/render/pipeline.rs
- [ ] T151 [US9] Create VS Code theme compatibility layer in crates/iridium-editor/src/theme/mod.rs
- [ ] T152 [US9] Expose setTheme() and getTheme() in public API in crates/iridium-editor/src/lib.rs

**Checkpoint**: User Story 9 complete - theming with hot reload works

---

## Phase 12: User Story 10 - Host Application Integration (Priority: P10)

**Goal**: React component with <50 lines, full API for programmatic control

**Independent Test**: Create minimal React app, embed editor, verify props/events/API

### Implementation for User Story 10

- [ ] T153 [US10] Define napi-rs bindings for IridiumEditor in crates/iridium-bindings/src/editor.rs
- [ ] T154 [US10] Implement createEditor() factory function in crates/iridium-bindings/src/lib.rs
- [ ] T155 [US10] Implement isWebGPUSupported() check in crates/iridium-bindings/src/lib.rs
- [ ] T156 [US10] Implement getContent/setContent bindings in crates/iridium-bindings/src/editor.rs
- [ ] T157 [US10] Implement getCursor/setCursor/setSelection bindings in crates/iridium-bindings/src/editor.rs
- [ ] T158 [US10] Implement undo/redo/canUndo/canRedo bindings in crates/iridium-bindings/src/editor.rs
- [ ] T159 [US10] Implement find/replace API bindings in crates/iridium-bindings/src/editor.rs
- [ ] T160 [US10] Implement fold/unfold API bindings in crates/iridium-bindings/src/editor.rs
- [ ] T161 [US10] Implement event listener system in crates/iridium-bindings/src/events.rs
- [ ] T162 [US10] Implement TypeScript type generation via napi-rs in crates/iridium-bindings/
- [ ] T163 [US10] Build WASM target with wasm32-wasip1-threads in crates/iridium-bindings/
- [ ] T164 [US10] Create React wrapper component in examples/web/src/Iridium.tsx
- [ ] T165 [US10] Create React hooks (useIridiumEditor, useEditorContent, etc.) in examples/web/src/hooks/
- [ ] T166 [US10] Create minimal React example demonstrating integration in examples/web/
- [ ] T167 [US10] Implement focus/blur/resize methods in crates/iridium-bindings/src/editor.rs
- [ ] T168 [US10] Implement destroy() for cleanup in crates/iridium-bindings/src/editor.rs

**Checkpoint**: User Story 10 complete - React integration works with <50 lines

---

## Phase 13: Polish & Cross-Cutting Concerns

**Purpose**: Final quality improvements, testing, documentation

- [ ] T169 Implement performance benchmarks for rendering in tests/benchmarks/render_bench.rs
- [ ] T170 [P] Implement performance benchmarks for editing operations in tests/benchmarks/edit_bench.rs
- [ ] T171 [P] Implement performance benchmarks for large file handling in tests/benchmarks/large_file_bench.rs
- [ ] T172 Create visual regression test infrastructure in tests/visual/
- [ ] T173 Add visual tests for basic editing scenarios in tests/visual/
- [ ] T174 [P] Add visual tests for syntax highlighting in tests/visual/
- [ ] T175 Validate all error messages are visible and actionable in crates/iridium-editor/src/
- [ ] T176 Review all public API documentation in crates/iridium-editor/src/lib.rs
- [ ] T177 [P] Review all public API documentation in crates/iridium-syntax/src/lib.rs
- [ ] T178 [P] Review all public API documentation in crates/iridium-bindings/src/lib.rs
- [ ] T179 Run cargo fmt and cargo clippy across entire workspace
- [ ] T180 Validate quickstart.md instructions work end-to-end
- [ ] T181 Create native example with winit in examples/native/
- [ ] T182 Final performance validation against SC-001/002/003/004 targets

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies - can start immediately
- **Phase 2 (Foundational)**: Depends on Phase 1 - BLOCKS all user stories
- **Phases 3-12 (User Stories)**: All depend on Phase 2 completion
  - US1 (Basic Editing): No dependencies on other stories
  - US2 (Undo Tree): Depends on US1 (needs edit operations)
  - US3 (Scrolling): Can run parallel to US2
  - US4 (Syntax): Can run parallel to US2/US3
  - US5 (Multi-cursor): Depends on US1
  - US6 (Search): Can run parallel to US2-US5
  - US7 (Folding): Depends on US4 (needs tree-sitter)
  - US8 (Minimap): Depends on US3 (needs viewport)
  - US9 (Theming): Can run parallel to US2-US8
  - US10 (Integration): Depends on all previous stories
- **Phase 13 (Polish)**: Depends on all user stories being complete

### User Story Dependencies (Visual)

```
Phase 2 (Foundation)
       │
       ▼
    ┌──────┐
    │  US1 │ Basic Editing
    └──┬───┘
       │
   ┌───┴───┬───────┬───────┐
   ▼       ▼       ▼       ▼
┌──────┐┌──────┐┌──────┐┌──────┐
│  US2 ││  US3 ││  US4 ││  US9 │ (Parallel capable)
└──┬───┘└──┬───┘└──┬───┘└──────┘
   │       │       │
   ▼       │       ▼
┌──────┐   │    ┌──────┐
│  US5 │   │    │  US7 │ Code Folding (needs US4)
└──────┘   ▼    └──────┘
        ┌──────┐
        │  US8 │ Minimap (needs US3)
        └──────┘

┌──────┐
│  US6 │ Search (independent, can run anytime after Phase 2)
└──────┘

All stories ──► US10 (Integration) ──► Phase 13 (Polish)
```

### Parallel Opportunities

**Within Phase 1 (Setup)**:
- T003, T004 (crate init)
- T006, T007, T008 (dependencies)
- T010 (clippy config)
- T012, T013 (module structure)

**Within Phase 2 (Foundational)**:
- T015 (Range - after Position)
- T024, T025 (color structs)
- T031 (ErrorCode)

**Within User Stories (if team capacity)**:
- US3, US4, US6, US9 can all proceed in parallel after US1
- US5 can proceed after US1
- US7 needs US4, US8 needs US3
- US10 needs all others

**Within Each Story**:
- Tasks marked [P] can run in parallel
- See each phase for specific parallel tasks

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational
3. Complete Phase 3: User Story 1 (Basic Editing)
4. **STOP and VALIDATE**: Test basic editing at 120fps
5. Demo: Text editor that types, deletes, selects, navigates

### Incremental Delivery

1. MVP: Setup + Foundation + US1 → Basic text editing
2. Add US2 → Undo tree with branches
3. Add US3 → Smooth scrolling
4. Add US4 → Syntax highlighting
5. Add US5 → Multi-cursor
6. Add US6 → Search/replace
7. Add US7 → Code folding
8. Add US8 → Minimap
9. Add US9 → Theming
10. Add US10 → React integration
11. Polish → Final quality pass

### Suggested Team Strategy

**Solo developer**: Follow priority order (P1 → P2 → ... → P10)

**Two developers**:
- Dev A: US1 → US2 → US5 → US10 (core editing path)
- Dev B: US3 → US4 → US7 → US8 (rendering/syntax path)
- Both: US6, US9 (independent)

---

## Summary

| Phase | Story | Task Count | Parallel Tasks |
|-------|-------|------------|----------------|
| 1 | Setup | 13 | 7 |
| 2 | Foundation | 25 | 6 |
| 3 | US1 Basic Editing | 23 | 3 |
| 4 | US2 Undo Tree | 13 | 0 |
| 5 | US3 Scrolling | 12 | 0 |
| 6 | US4 Syntax | 15 | 5 |
| 7 | US5 Multi-cursor | 10 | 0 |
| 8 | US6 Search | 15 | 0 |
| 9 | US7 Folding | 11 | 0 |
| 10 | US8 Minimap | 8 | 0 |
| 11 | US9 Theming | 7 | 0 |
| 12 | US10 Integration | 16 | 0 |
| 13 | Polish | 14 | 5 |
| **Total** | | **182** | **26** |

**MVP Scope**: Phases 1-3 (61 tasks) delivers basic text editing at 120fps

**Notes**:
- All tasks include exact file paths
- [P] tasks can run in parallel within their phase
- [US#] labels map tasks to user stories
- Each checkpoint validates story completeness
- Constitution compliance verified throughout
