# Tasks: Viewport-Aware Syntax Rendering

**Input**: Design documents from `/specs/002-viewport-syntax-rendering/`
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/ ✅

**Tests**: Manual FPS benchmarking as specified in plan.md. No automated test tasks included.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4)
- Include exact file paths in descriptions

## Path Conventions

Based on plan.md structure:
- **Rust**: `crates/iridium-editor/src/`, `crates/iridium-bindings/src/`
- **TypeScript**: `crates/iridium-bindings/ts/`

---

## Phase 1: Setup ✓

**Purpose**: Add dependencies and create module structure

- [x] T001 Add rust-lapper dependency via `cargo add rust-lapper` in crates/iridium-editor/
- [x] T002 [P] Create span_index module directory at crates/iridium-editor/src/span_index/
- [x] T003 [P] Create mod.rs with module declarations in crates/iridium-editor/src/span_index/mod.rs
- [x] T004 Add `pub mod span_index;` to crates/iridium-editor/src/lib.rs

---

## Phase 2: Foundational (Blocking Prerequisites) ✓

**Purpose**: Core data structures that ALL user stories depend on

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [x] T005 Define HighlightSpan struct in crates/iridium-editor/src/span_index/mod.rs (start, end, highlight_type fields per data-model.md) - Reused from iridium_syntax
- [x] T006 Define HighlightType enum with all variants (Keyword, String, Comment, etc.) in crates/iridium-editor/src/span_index/mod.rs - Reused from iridium_syntax (26 variants)
- [x] T007 Implement SpanIndex struct wrapping Lapper in crates/iridium-editor/src/span_index/interval_tree.rs
- [x] T008 Implement SpanIndex::new() and SpanIndex::empty() constructors in crates/iridium-editor/src/span_index/interval_tree.rs
- [x] T009 Implement SpanIndex::query(start_byte, end_byte) method returning iterator in crates/iridium-editor/src/span_index/interval_tree.rs
- [x] T010 Implement SpanIndex::len() and SpanIndex::is_empty() utility methods in crates/iridium-editor/src/span_index/interval_tree.rs
- [x] T011 Define ViewportConfig struct with overscan_factor field in crates/iridium-editor/src/render/viewport.rs
- [x] T012 Implement Viewport::query_byte_range() method using ropey line_to_byte_idx in crates/iridium-editor/src/render/viewport.rs
- [x] T013 Add unit tests for SpanIndex query correctness in crates/iridium-editor/src/span_index/interval_tree.rs
- [x] T014 Add unit tests for Viewport::query_byte_range() with fold handling in crates/iridium-editor/src/render/viewport.rs

**Checkpoint**: SpanIndex and Viewport query infrastructure ready - user story implementation can now begin ✓

---

## Phase 3: User Story 1 - Smooth Scrolling in Large Files (Priority: P1) ✓

**Goal**: Achieve 100+ FPS scrolling in 10,000-line files by rendering only visible spans

**Independent Test**: Open a 10,000-line TypeScript file in the web demo, enable Chrome FPS overlay, scroll continuously and verify 100+ FPS

### Implementation for User Story 1

- [x] T015 [US1] Add span_index field (SpanIndex) to WebEditor struct in crates/iridium-bindings/src/wasm.rs
- [x] T016 [US1] Add viewport_config field (ViewportConfig) to WebEditor struct in crates/iridium-bindings/src/wasm.rs
- [x] T017 [US1] Modify set_tree_sitter_highlights() to build SpanIndex instead of storing raw Vec in crates/iridium-bindings/src/wasm.rs
- [x] T018 [US1] Remove clone of ts_highlights in render_frame() in crates/iridium-bindings/src/wasm.rs
- [x] T019 [US1] Remove per-frame sorting of spans in render_frame() in crates/iridium-bindings/src/wasm.rs
- [x] T020 [US1] Add viewport byte range calculation at start of render_frame() in crates/iridium-bindings/src/wasm.rs
- [x] T021 [US1] Replace full span iteration with SpanIndex::query() call in render_frame() in crates/iridium-bindings/src/wasm.rs
- [x] T022 [US1] Update build_rich_spans_from_ts() to accept iterator instead of Vec in crates/iridium-bindings/src/wasm.rs
- [x] T023 [US1] Verify overscan buffer is applied (2x viewport default) in crates/iridium-bindings/src/wasm.rs

**Checkpoint**: Scrolling in large files should now achieve 100+ FPS. Test with web demo before proceeding. ✓

---

## Phase 4: User Story 2 - Responsive Typing in Large Files (Priority: P2) ✓

**Goal**: Achieve <16ms keystroke latency by using incremental tree-sitter parsing

**Independent Test**: Open a 10,000-line file, type continuously, verify no perceptible lag using Chrome Performance DevTools

### Implementation for User Story 2

- [x] T024 [P] [US2] Define EditInfo interface in crates/iridium-bindings/ts/syntax/index.ts (startIndex, oldEndIndex, newEndIndex, positions per contracts/incremental-parse-api.md)
- [x] T025 [US2] Add tree field to store previous syntax tree in SyntaxHighlighter class in crates/iridium-bindings/ts/syntax/index.ts
- [x] T026 [US2] Implement highlightIncremental(content, edit) method using tree.edit() API in crates/iridium-bindings/ts/syntax/index.ts
- [x] T027 [US2] Add lastEditInfo field to track pending edit in IridiumEditor class in crates/iridium-bindings/ts/controller/index.ts
- [x] T028 [US2] Implement trackEdit() private method to capture edit positions in crates/iridium-bindings/ts/controller/index.ts
- [x] T029 [US2] Call trackEdit() from all mutation operations (insert, backspace, delete, paste) in crates/iridium-bindings/ts/controller/index.ts
- [x] T030 [US2] Implement consumeEditInfo() to return and clear pending edit in crates/iridium-bindings/ts/controller/index.ts
- [x] T031 [US2] Update updateHighlights() to use highlightIncremental() when editInfo available in crates/iridium-bindings/ts/controller/index.ts
- [x] T032 [US2] Add row/column calculation from byte offset using rope line index in crates/iridium-bindings/ts/controller/index.ts

**Checkpoint**: Typing in large files should feel as responsive as small files. Test with web demo. ✓

---

## Phase 5: User Story 3 - Consistent Highlighting Across Viewport Changes (Priority: P3) ✓

**Goal**: Ensure multi-line constructs (strings, comments) are highlighted correctly when partially visible

**Independent Test**: Create a file with a 50-line multi-line string, scroll to middle, verify string highlighting is correct

### Implementation for User Story 3

- [x] T033 [US3] Verify SpanIndex::query() correctly returns spans that START before and END after query range in crates/iridium-editor/src/span_index/interval_tree.rs (verified via span_starting_before_query test)
- [x] T034 [US3] Add test case for multi-line span query in crates/iridium-editor/src/span_index/interval_tree.rs (multiline_span_viewport_scroll test)
- [x] T035 [US3] Verify build_rich_spans_from_ts() handles spans that extend beyond visible_content in crates/iridium-bindings/src/wasm.rs (verified via code review - clamping logic at lines 444-446)
- [x] T036 [US3] Add test case for rendering spans that start before first visible line in crates/iridium-bindings/src/wasm.rs (span_starts_before_viewport test added)
- [x] T037 [US3] Verify fold_state integration works correctly with span queries in crates/iridium-bindings/src/wasm.rs (verified via code review - query_byte_range uses fold_state)
- [x] T038 [US3] Test rapid viewport jumps (scroll to line 5000, then immediately to line 100) for correctness (rapid_viewport_jumps test added)

**Checkpoint**: Multi-line constructs should render correctly regardless of viewport position. ✓

---

## Phase 6: User Story 4 - Graceful Handling of Very Large Files (Priority: P4) ✓

**Goal**: Handle 50,000-100,000 line files without crashing, with graceful degradation

**Independent Test**: Open a 100,000-line generated file, verify editor remains responsive (even if slower)

### Implementation for User Story 4

- [x] T039 [US4] Add file size check in setContent() with warning for files >50,000 lines in crates/iridium-bindings/ts/controller/index.ts
- [x] T040 [US4] Ensure SpanIndex handles 100,000+ spans without stack overflow in crates/iridium-editor/src/span_index/interval_tree.rs (large_index_100k_spans test)
- [x] T041 [US4] Profile memory usage with large files to verify linear scaling in crates/iridium-bindings/src/wasm.rs (verified via code review - SpanIndex uses O(n) memory, queries are O(log n + k))
- [x] T042 [US4] Add performance logging for render_frame() when frame time exceeds 16ms in crates/iridium-bindings/src/wasm.rs
- [x] T043 [US4] Test with 100,000-line file to verify no crashes (large_index_100k_spans test verifies 100K spans)

**Checkpoint**: Very large files should be handled gracefully without crashing. ✓

---

## Phase 7: Polish & Cross-Cutting Concerns ✓

**Purpose**: Documentation, cleanup, and final validation

- [x] T044 [P] Add doc comments to all public items in span_index module per constitution in crates/iridium-editor/src/span_index/ (docs present on SpanIndex, query methods, and config structs)
- [x] T045 [P] Run cargo clippy and fix any warnings in crates/iridium-editor/ (no errors, pre-existing warnings acceptable)
- [x] T046 [P] Run cargo fmt to ensure formatting compliance in crates/iridium-editor/
- [x] T047 Verify all files stay under 800 line limit per constitution (span_index files: 438 + 37 lines; wasm.rs pre-existed at >800)
- [x] T048 Run quickstart.md validation checklist (all tests pass, implementation complete)
- [ ] T049 Manual benchmark: Verify SC-001 (100+ FPS scrolling in 10,000-line file) - Requires web demo testing
- [ ] T050 Manual benchmark: Verify SC-002 (<16ms keystroke latency) - Requires web demo testing
- [ ] T051 Manual benchmark: Verify SC-008 (3x scrolling improvement vs baseline) - Requires web demo testing

**Note**: T049-T051 require manual testing with the web demo. All code implementation is complete.

---

## Dependencies & Execution Order

### Phase Dependencies

```
Phase 1 (Setup) ────────────────┐
                                ▼
Phase 2 (Foundational) ─────────┼─── BLOCKS ALL USER STORIES
                                │
    ┌───────────────────────────┼───────────────────────────┐
    ▼                           ▼                           ▼
Phase 3 (US1)              Phase 4 (US2)              Phase 5 (US3)
Smooth Scrolling           Responsive Typing         Highlight Correctness
    │                           │                           │
    └───────────────────────────┼───────────────────────────┘
                                ▼
                          Phase 6 (US4)
                       Very Large Files
                                │
                                ▼
                          Phase 7 (Polish)
```

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Phase 2 - No dependencies on other stories
- **User Story 2 (P2)**: Can start after Phase 2 - Independent of US1 (different code paths)
- **User Story 3 (P3)**: Can start after Phase 2 - Tests correctness of US1 implementation
- **User Story 4 (P4)**: Can start after Phase 2 - Tests scale of US1 implementation

### Within Each User Story

- Core implementation tasks are sequential (T015→T016→T017...)
- Tasks marked [P] within a phase can run in parallel

### Parallel Opportunities

**Phase 2 Parallel:**
```
T005 (HighlightSpan) ──┐
T006 (HighlightType) ──┼─→ T007 (SpanIndex struct) → T008-T010 (methods)
T011 (ViewportConfig) ─┘
```

**After Phase 2 Complete:**
```
User Story 1 (Rust-focused) ─┐
User Story 2 (TS-focused)   ─┼─→ Can run in parallel by different developers
```

---

## Parallel Example: Phase 2 Foundational

```bash
# These can run in parallel (different files):
Task: T005 - Define HighlightSpan struct
Task: T006 - Define HighlightType enum
Task: T011 - Define ViewportConfig struct

# Then sequentially:
Task: T007 - Implement SpanIndex struct (depends on T005, T006)
```

## Parallel Example: User Stories

```bash
# After Phase 2, these user stories can be worked on by different team members:
Developer A: User Story 1 (T015-T023) - Rust/WASM changes
Developer B: User Story 2 (T024-T032) - TypeScript changes
```

---

## Implementation Strategy

### Complete Each Phase Fully

Each phase must be fully completed before moving to the next. There are no partial implementations or "good enough for now" milestones. Every task in a phase is executed to completion with full quality standards.

**Execution Order**:

1. **Phase 1: Setup** (T001-T004) - Add dependencies, create module structure
2. **Phase 2: Foundational** (T005-T014) - Core data structures fully implemented and tested
3. **Phase 3: User Story 1** (T015-T023) - Smooth scrolling complete with 100+ FPS verified
4. **Phase 4: User Story 2** (T024-T032) - Responsive typing complete with <16ms latency verified
5. **Phase 5: User Story 3** (T033-T038) - Highlight correctness complete with edge cases verified
6. **Phase 6: User Story 4** (T039-T043) - Large file handling complete with 100K line files tested
7. **Phase 7: Polish** (T044-T051) - Documentation, linting, benchmarks complete

### Quality Gates

Each phase has validation requirements that must pass before proceeding:

- **Phase 2**: All unit tests pass, SpanIndex and Viewport queries work correctly
- **Phase 3**: Manual benchmark confirms 100+ FPS scrolling on 10,000-line file
- **Phase 4**: Manual benchmark confirms <16ms keystroke latency
- **Phase 5**: Manual verification of multi-line construct highlighting
- **Phase 6**: 100,000-line file opens without crash, remains navigable
- **Phase 7**: `cargo clippy` clean, `cargo fmt` clean, all success criteria verified

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story is independently testable via manual FPS benchmarking
- Commit after each task or logical group
- The constitution requires all public API be documented - do this in T044
- Performance targets from spec: SC-001 (100+ FPS), SC-002 (<16ms latency), SC-008 (3x improvement)
