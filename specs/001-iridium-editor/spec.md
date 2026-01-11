# Feature Specification: Iridium GPU-Accelerated Text Editor

**Feature Branch**: `001-iridium-editor`
**Created**: 2026-01-11
**Status**: Draft
**Input**: GPU-accelerated single-file text editor component built in Rust, targeting web (WASM/WebGPU) as primary platform with native desktop support. Renders at 120fps, provides professional editing features, syntax highlighting via tree-sitter, and pluggable LSP integration.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Basic Text Editing (Priority: P1)

A developer opens Iridium in their browser, sees their code rendered on screen, and can type, delete, select text, and navigate using keyboard and mouse. The editing feels instant and responsive—indistinguishable from a native application.

**Why this priority**: This is the foundational experience. Without fluid text entry and manipulation, nothing else matters. Users must feel zero latency between keystroke and visual feedback.

**Independent Test**: Can be fully tested by loading content into the editor, typing characters, using arrow keys, selecting text with shift+arrows and mouse, and verifying all operations feel instantaneous with no visible lag or dropped frames.

**Acceptance Scenarios**:

1. **Given** the editor is loaded with content, **When** user types a character, **Then** the character appears on screen within 8ms with no visible lag
2. **Given** the editor displays text, **When** user presses arrow keys, **Then** cursor moves smoothly to the expected position
3. **Given** text is displayed, **When** user clicks a position, **Then** cursor moves to that exact character position
4. **Given** cursor is positioned, **When** user holds Shift and presses arrow keys, **Then** text selection expands visually in real-time
5. **Given** text is selected, **When** user presses Backspace/Delete, **Then** selected text is removed and cursor repositioned
6. **Given** text is displayed, **When** user double-click a word, **Then** the entire word is selected
7. **Given** text is displayed, **When** user triple-clicks, **Then** the entire line is selected

---

### User Story 2 - Undo/Redo with Branching History (Priority: P2)

A developer makes changes, realizes they went down the wrong path, undoes several steps, then makes different changes. Later, they want to recover the original branch of changes they undid. The undo tree preserves all history branches, allowing navigation between them.

**Why this priority**: Undo/redo is fundamental to editing confidence. The tree structure prevents accidental loss of work and enables experimentation without fear.

**Independent Test**: Can be tested by making a series of changes, undoing to a mid-point, making different changes (creating a branch), then navigating back to the original branch and verifying that content is fully recoverable.

**Acceptance Scenarios**:

1. **Given** user has typed text, **When** user invokes undo, **Then** the last change is reversed
2. **Given** user has undone changes, **When** user invokes redo, **Then** the undone change is reapplied
3. **Given** user has undone 3 changes and then typed new text, **When** user navigates the undo tree, **Then** user can access both the new branch and the original (undone) branch
4. **Given** undo tree has multiple branches, **When** user selects a specific branch point, **Then** document state reflects that exact point in history
5. **Given** user is editing, **When** user types continuously without pause, **Then** those keystrokes are grouped as a single undo operation
6. **Given** user pauses typing for more than 500ms, **When** user resumes typing, **Then** new keystrokes form a new undo group

---

### User Story 3 - Smooth Scrolling and Viewport (Priority: P3)

A developer opens a large file and scrolls through it using mouse wheel, trackpad, keyboard (Page Up/Down), or scrollbar. Scrolling is perfectly smooth at 120fps with no stuttering, tearing, or frame drops regardless of file size.

**Why this priority**: Scrolling is constant during code review and navigation. Choppy scrolling breaks immersion and signals poor quality.

**Independent Test**: Can be tested by loading files of varying sizes (100 lines to 100,000 lines) and scrolling rapidly using all input methods while monitoring frame rate stays at 120fps.

**Acceptance Scenarios**:

1. **Given** a file is loaded, **When** user scrolls with mouse wheel, **Then** viewport moves smoothly at 120fps
2. **Given** a file is loaded, **When** user scrolls with trackpad gesture, **Then** viewport follows gesture with momentum scrolling
3. **Given** cursor is visible, **When** user presses Page Down, **Then** viewport moves one page and cursor moves to corresponding position
4. **Given** cursor is at line 50, **When** user invokes "Go to Line 500", **Then** viewport scrolls to show line 500 with cursor positioned there
5. **Given** a 100,000-line file is loaded, **When** user scrolls rapidly through entire file, **Then** frame rate remains at 120fps with no stuttering

---

### User Story 4 - Syntax Highlighting (Priority: P4)

A developer opens a Cypher query file and sees the code syntax-highlighted with appropriate colors for keywords, strings, numbers, comments, and identifiers. Highlighting updates incrementally as they type without visible delay.

**Why this priority**: Syntax highlighting is essential for code comprehension but depends on the text rendering foundation being solid first.

**Independent Test**: Can be tested by loading files in supported languages and verifying tokens are colored correctly, then editing text and confirming highlighting updates in real-time without lag.

**Acceptance Scenarios**:

1. **Given** a Cypher file is loaded, **When** editor renders, **Then** keywords (MATCH, WHERE, RETURN) appear in keyword color
2. **Given** a SQL file is loaded, **When** editor renders, **Then** strings, numbers, and comments are distinctly colored
3. **Given** user is typing code, **When** user completes a keyword, **Then** highlighting updates within the same frame
4. **Given** user deletes part of a string literal, **When** string becomes unclosed, **Then** highlighting updates to reflect the syntax error state
5. **Given** a large file is loaded, **When** user edits in the middle, **Then** only affected regions are re-parsed (incremental)

---

### User Story 5 - Multiple Cursors (Priority: P5)

A developer needs to rename a variable that appears in multiple places. They add cursors at each occurrence and type the new name once, with all cursors updating simultaneously.

**Why this priority**: Multiple cursors dramatically improve editing efficiency for repetitive changes, but require solid single-cursor editing first.

**Independent Test**: Can be tested by placing multiple cursors using Ctrl+D (select next occurrence) or Ctrl+Click, then typing and verifying all cursor positions update identically.

**Acceptance Scenarios**:

1. **Given** word "foo" is selected, **When** user presses Ctrl+D, **Then** next occurrence of "foo" is selected with a new cursor
2. **Given** cursor is positioned, **When** user Ctrl+Clicks another position, **Then** a second cursor appears at that position
3. **Given** multiple cursors exist, **When** user types, **Then** same text is inserted at all cursor positions simultaneously
4. **Given** multiple cursors exist, **When** user presses arrow key, **Then** all cursors move in the same direction
5. **Given** multiple cursors exist, **When** user presses Escape, **Then** all cursors collapse to primary cursor

---

### User Story 6 - Search and Replace (Priority: P6)

A developer needs to find all occurrences of a pattern in their file and optionally replace them. They open search, type a query, see matches highlighted, navigate between them, and perform replacements.

**Why this priority**: Search is essential for navigation and refactoring, but core editing must work first.

**Independent Test**: Can be tested by opening search, entering a query, verifying matches are highlighted and counted, navigating with next/previous, and performing single and bulk replacements.

**Acceptance Scenarios**:

1. **Given** editor is focused, **When** user presses Ctrl+F, **Then** search panel opens with focus in search field
2. **Given** search panel is open with query "foo", **When** user types, **Then** matches are highlighted in real-time as query changes
3. **Given** matches exist, **When** user presses Enter or F3, **Then** cursor moves to next match
4. **Given** matches exist and replace field has text, **When** user clicks Replace, **Then** current match is replaced and cursor moves to next
5. **Given** matches exist, **When** user clicks Replace All, **Then** all matches are replaced in a single undo-able operation
6. **Given** search panel is open, **When** user toggles regex mode and enters `\d+`, **Then** all number sequences are matched
7. **Given** search panel is open, **When** user toggles case-sensitive mode, **Then** matching respects case

---

### User Story 7 - Code Folding (Priority: P7)

A developer working with a large file wants to collapse sections they're not currently focused on. They fold function bodies, class definitions, or comment blocks to reduce visual noise and navigate more easily.

**Why this priority**: Code folding improves navigation in large files but requires syntax understanding (tree-sitter) to be in place.

**Independent Test**: Can be tested by loading a file with nested structures, clicking fold indicators to collapse regions, verifying content is hidden, and unfolding to restore.

**Acceptance Scenarios**:

1. **Given** file has foldable regions, **When** editor renders, **Then** fold indicators appear in gutter next to foldable lines
2. **Given** fold indicator is visible, **When** user clicks it, **Then** region collapses showing placeholder with line count
3. **Given** region is folded, **When** user clicks fold indicator again, **Then** region expands showing full content
4. **Given** multiple nested foldable regions, **When** user invokes "Fold All", **Then** all regions collapse to top level
5. **Given** regions are folded, **When** user invokes "Unfold All", **Then** all regions expand

---

### User Story 8 - Minimap (Priority: P8)

A developer wants to see an overview of their entire file and quickly navigate to any section. A minimap on the side shows a scaled-down view of the entire document with the current viewport highlighted.

**Why this priority**: Minimap is a navigation enhancement that requires solid rendering to display scaled content.

**Independent Test**: Can be tested by loading a file, verifying minimap shows document overview, clicking on minimap to navigate, and confirming viewport indicator matches visible content.

**Acceptance Scenarios**:

1. **Given** file is loaded, **When** minimap is enabled, **Then** scaled document preview appears on right side
2. **Given** minimap is visible, **When** viewport changes, **Then** viewport indicator on minimap updates to match
3. **Given** minimap is visible, **When** user clicks on minimap, **Then** viewport scrolls to that position
4. **Given** minimap is visible, **When** user drags on minimap, **Then** viewport follows drag position smoothly
5. **Given** syntax highlighting is active, **When** minimap renders, **Then** minimap shows approximate coloring matching editor

---

### User Story 9 - Theming (Priority: P9)

A developer wants to customize the editor's appearance to match their preferences or application theme. They can change colors, fonts, and spacing. Both light and dark modes are supported.

**Why this priority**: Theming is important for integration into host applications but doesn't affect core editing functionality.

**Independent Test**: Can be tested by loading different theme configurations and verifying colors, fonts, and spacing update correctly without requiring editor restart.

**Acceptance Scenarios**:

1. **Given** editor is loaded, **When** host provides dark theme configuration, **Then** editor renders with dark background and appropriate contrast
2. **Given** editor is loaded, **When** host provides light theme configuration, **Then** editor renders with light background and appropriate contrast
3. **Given** theme specifies custom font, **When** editor renders, **Then** text uses specified font family and size
4. **Given** theme specifies syntax colors, **When** syntax highlighting is active, **Then** token colors match theme specification
5. **Given** editor is running, **When** theme changes at runtime, **Then** editor updates appearance without reload

---

### User Story 10 - Host Application Integration (Priority: P10)

A developer embedding Iridium in their React application can pass content, receive change events, configure behavior, and control the editor programmatically. The integration is straightforward with minimal boilerplate.

**Why this priority**: Integration is essential for Iridium to be useful, but depends on all editor features being functional first.

**Independent Test**: Can be tested by creating a minimal React application that embeds Iridium, passes content, receives onChange events, and programmatically sets cursor position.

**Acceptance Scenarios**:

1. **Given** React component mounts, **When** content prop is provided, **Then** editor displays that content
2. **Given** editor is displaying content, **When** user edits, **Then** onChange callback fires with new content
3. **Given** editor is mounted, **When** host calls setCursor API, **Then** cursor moves to specified position
4. **Given** editor is mounted, **When** host calls setSelection API, **Then** specified range is selected
5. **Given** editor is mounted, **When** host calls focus API, **Then** editor receives keyboard focus
6. **Given** content prop changes externally, **When** React re-renders, **Then** editor updates to show new content

---

### Edge Cases

- What happens when user pastes 1MB of text? System accepts paste but may show brief processing indicator for very large pastes.
- What happens when file contains mixed line endings (CRLF/LF)? System normalizes to configured line ending, preserving original on load if no edits.
- What happens when file contains invalid UTF-8 sequences? System displays replacement character and prevents corruption on save.
- What happens when GPU is unavailable or WebGPU unsupported? System fails visibly with clear error message explaining requirements.
- What happens when user has multiple browser tabs with Iridium? Each instance operates independently with its own GPU context.
- What happens during rapid typing with IME (Input Method Editor)? System correctly handles composition events for CJK input.
- What happens when undo tree becomes very large (thousands of nodes)? System maintains performance, potentially with pruning of very old branches after warning.

## Requirements *(mandatory)*

### Functional Requirements

**Core Editing**
- **FR-001**: Editor MUST render text content at 120fps without frame drops during normal editing operations
- **FR-002**: Editor MUST respond to keystrokes with character appearing on screen within 8ms
- **FR-003**: Editor MUST support cursor positioning via keyboard (arrow keys, Home, End, Ctrl+arrows) and mouse click
- **FR-004**: Editor MUST support text selection via keyboard (Shift+arrows, Shift+Home/End, Ctrl+Shift+arrows) and mouse drag
- **FR-005**: Editor MUST support standard text operations: insert, delete (Backspace, Delete), cut, copy, paste
- **FR-006**: Editor MUST support word selection (double-click) and line selection (triple-click)
- **FR-007**: Editor MUST handle clipboard operations through host platform clipboard

**Undo/Redo**
- **FR-008**: Editor MUST maintain a tree-structured undo history preserving all branches
- **FR-009**: Editor MUST group continuous typing into single undo operations with pause-based boundaries (500ms default)
- **FR-010**: Editor MUST allow navigation between undo tree branches without losing any branch
- **FR-011**: Each undo node MUST store sufficient information to restore exact document state

**Scrolling and Viewport**
- **FR-012**: Editor MUST support smooth scrolling via mouse wheel, trackpad, keyboard (Page Up/Down), and scrollbar
- **FR-013**: Editor MUST maintain 120fps during rapid scrolling through files up to 100,000 lines
- **FR-014**: Editor MUST support "Go to Line" navigation
- **FR-015**: Editor MUST keep cursor visible, scrolling viewport when cursor would move off-screen

**Syntax Highlighting**
- **FR-016**: Editor MUST support syntax highlighting for Cypher query language
- **FR-017**: Editor MUST support syntax highlighting for SQL
- **FR-018**: Editor MUST support syntax highlighting for Rust, Python, and TypeScript
- **FR-019**: Editor MUST update highlighting incrementally when text changes (not full re-parse)
- **FR-020**: Syntax highlighting MUST NOT cause visible lag during typing

**Multiple Cursors**
- **FR-021**: Editor MUST support adding cursors via Ctrl+Click
- **FR-022**: Editor MUST support "Add Selection to Next Match" (Ctrl+D)
- **FR-023**: Editor MUST apply all editing operations to all active cursors simultaneously
- **FR-024**: Editor MUST allow collapsing to single cursor via Escape

**Search and Replace**
- **FR-025**: Editor MUST support text search with real-time match highlighting
- **FR-026**: Editor MUST support case-sensitive and case-insensitive search modes
- **FR-027**: Editor MUST support regular expression search
- **FR-028**: Editor MUST support whole-word matching mode
- **FR-029**: Editor MUST support single replacement and replace-all operations
- **FR-030**: Replace-all MUST be a single undo operation

**Code Folding**
- **FR-031**: Editor MUST detect foldable regions based on syntax structure
- **FR-032**: Editor MUST display fold indicators in the gutter
- **FR-033**: Editor MUST support fold, unfold, fold-all, and unfold-all operations
- **FR-034**: Folded regions MUST display line count indicator

**Minimap**
- **FR-035**: Editor MUST display optional scaled document overview (minimap)
- **FR-036**: Minimap MUST show current viewport position
- **FR-037**: Minimap MUST support click-to-navigate and drag-to-scroll

**Theming**
- **FR-038**: Editor MUST accept theme configuration for colors, fonts, and spacing
- **FR-039**: Editor MUST support runtime theme changes without restart
- **FR-040**: Editor MUST support both light and dark color schemes

**Integration**
- **FR-041**: Editor MUST be embeddable via WASM in web applications
- **FR-042**: Editor MUST expose API for setting/getting content
- **FR-043**: Editor MUST emit events for content changes, cursor changes, and selection changes
- **FR-044**: Editor MUST expose API for programmatic cursor and selection control
- **FR-045**: Editor MUST accept content as input and emit changes as output (controlled component pattern)

**Error Handling**
- **FR-046**: Editor MUST display clear error messages when GPU initialization fails
- **FR-047**: Editor MUST never silently fail—all errors must be visible to host application
- **FR-048**: Editor MUST handle invalid UTF-8 gracefully without data corruption

### Key Entities

- **Document**: The text content being edited, stored as a rope data structure for efficient manipulation of large files. Contains the full text, line index, and metadata.

- **Position**: A location in the document specified by line number and column offset. Used for cursor placement and range boundaries.

- **Selection**: A range in the document with anchor (start) and head (end) positions. May be forward or backward. Multiple selections can exist simultaneously.

- **Cursor**: The current editing position(s). Includes primary cursor and any additional cursors for multi-cursor editing. Each cursor has an associated selection (which may be collapsed to a point).

- **UndoTree**: Tree structure containing all document states. Each node stores the inverse command to reach parent, timestamp, and optional description. Branches form when edits are made after undo.

- **Command**: A reversible edit operation (insert, delete, replace). Commands are composable and invertible. The document model is command-sourced.

- **Viewport**: The visible region of the document. Defined by scroll position, visible line range, and dimensions. Used for rendering optimization.

- **Theme**: Configuration for visual appearance including syntax colors, editor colors, font family, font size, line height, and spacing.

- **SyntaxTree**: Parsed representation of document structure from tree-sitter. Used for highlighting, folding regions, and language-aware features.

## Success Criteria *(mandatory)*

### Measurable Outcomes

**Performance**
- **SC-001**: Editor maintains 120fps rendering during typing, scrolling, and all normal editing operations on reference hardware (M1 MacBook Air equivalent)
- **SC-002**: Time from keystroke to character visible on screen is under 8ms (sub-frame latency)
- **SC-003**: Editor opens, renders, and becomes interactive for a 10,000-line file in under 500ms
- **SC-004**: Editor scrolls through a 100,000-line file at 120fps with no frame drops

**Functionality Completeness**
- **SC-005**: All editing operations in the Professional Editing Features table (VISION.md) are fully implemented and working
- **SC-006**: Undo tree preserves all history branches with zero data loss
- **SC-007**: Syntax highlighting correctly identifies 95%+ of tokens for supported languages

**Cross-Platform**
- **SC-008**: Same build runs in Chrome, Firefox, Safari, and Edge with WebGPU support
- **SC-009**: Editor is embeddable in React applications with under 50 lines of integration code

**Reliability**
- **SC-010**: Zero silent failures—all error conditions produce visible, actionable error messages
- **SC-011**: No data loss scenarios—user content is never silently corrupted or discarded

**User Experience**
- **SC-012**: A developer familiar with VS Code or similar editors can use all basic features without documentation
- **SC-013**: Typing feels indistinguishable from native application (no perceptible input lag)

## Assumptions

- WebGPU is available in the target browser. Fallback to WebGL2 is not in scope for initial release.
- Host application handles file I/O. Iridium receives content as a string and emits changes; it does not read/write files directly.
- LSP integration is provided by a separate crate (iridium-lsp). This spec covers the editor component (iridium-editor) only.
- Tree-sitter grammars for Cypher, SQL, Rust, Python, and TypeScript exist and are usable. Custom grammar development is out of scope.
- The host application provides the canvas/surface for rendering. Iridium does not create windows or manage application lifecycle.
- Clipboard operations are mediated by the host platform. Iridium emits clipboard intents; the host executes them.
- IME (Input Method Editor) events are provided by the browser/host. Iridium handles composition events correctly but does not implement IME directly.
