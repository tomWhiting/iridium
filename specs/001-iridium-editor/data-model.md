# Data Model: Iridium Editor

**Date**: 2026-01-11
**Branch**: `001-iridium-editor`

## Overview

This document defines the core data structures for the Iridium editor. These types form the foundation of the editor's state model.

---

## Core Types

### Position

A location in the document.

```rust
/// A position in the document specified by line and column.
/// Line and column are both 0-indexed.
pub struct Position {
    /// Line number (0-indexed)
    pub line: usize,
    /// Column offset in UTF-8 code points (0-indexed)
    pub column: usize,
}
```

**Invariants**:
- `line` must be < document line count
- `column` must be <= line length

**Operations**:
- `offset_to_position(offset: usize) -> Position`
- `position_to_offset(position: Position) -> usize`
- `compare(a: Position, b: Position) -> Ordering`

---

### Range

A span of text defined by start and end positions.

```rust
/// A range in the document from start to end.
/// Start is inclusive, end is exclusive.
pub struct Range {
    /// Start position (inclusive)
    pub start: Position,
    /// End position (exclusive)
    pub end: Position,
}
```

**Invariants**:
- `start <= end` (canonical form)
- Both positions must be valid for the document

**Operations**:
- `is_empty() -> bool`: True if start == end
- `contains(position: Position) -> bool`
- `intersects(other: Range) -> bool`
- `union(other: Range) -> Range`

---

### Selection

A selection with anchor and head, supporting directional selection.

```rust
/// A selection in the document with anchor and head.
/// Anchor is where selection started, head is where cursor is.
pub struct Selection {
    /// Where the selection started
    pub anchor: Position,
    /// Where the cursor/head is (direction indicator)
    pub head: Position,
}
```

**Properties**:
- Selection is forward if `anchor <= head`
- Selection is backward if `anchor > head`
- Selection is collapsed (just a cursor) if `anchor == head`

**Operations**:
- `range() -> Range`: Normalized range (start <= end)
- `is_collapsed() -> bool`
- `is_forward() -> bool`
- `cursor_position() -> Position`: Returns head

---

### Cursor

The current cursor state, potentially with multiple cursors.

```rust
/// Cursor state supporting multiple cursors.
pub struct CursorState {
    /// Primary cursor (always exists)
    pub primary: Selection,
    /// Additional cursors (may be empty)
    pub secondary: Vec<Selection>,
}
```

**Invariants**:
- Primary cursor always exists
- Secondary cursors are sorted by position
- No two cursors overlap

**Operations**:
- `all_selections() -> impl Iterator<Item = &Selection>`
- `cursor_count() -> usize`
- `add_cursor(selection: Selection)`
- `remove_cursor(index: usize)`
- `collapse_to_primary()`: Remove all secondary cursors

---

## Document Model

### Document

The text content being edited.

```rust
/// The document being edited, backed by a rope.
pub struct Document {
    /// Text content stored in rope structure
    content: Rope,
    /// Line ending style
    line_ending: LineEnding,
    /// Language identifier for syntax highlighting
    language: Option<String>,
}

pub enum LineEnding {
    LF,    // Unix: \n
    CRLF,  // Windows: \r\n
    CR,    // Legacy Mac: \r
}
```

**Invariants**:
- Content is always valid UTF-8
- Line endings are normalized to configured style

**Operations**:
- `text() -> &str`: Full text (may be expensive for large docs)
- `line(n: usize) -> Option<&str>`: Get line by number
- `line_count() -> usize`
- `char_count() -> usize`
- `slice(range: Range) -> String`
- `insert(position: Position, text: &str)`
- `delete(range: Range)`
- `replace(range: Range, text: &str)`

---

## Command Model

### Command

A reversible edit operation.

```rust
/// A command that can be applied to the document.
/// All commands are reversible.
pub enum Command {
    /// Insert text at a position
    Insert {
        position: Position,
        text: String,
    },

    /// Delete text in a range
    Delete {
        range: Range,
        /// The deleted text (for undo)
        deleted_text: String,
    },

    /// Replace text in a range
    Replace {
        range: Range,
        old_text: String,
        new_text: String,
    },

    /// Change cursor/selection state
    SetSelection {
        old_state: CursorState,
        new_state: CursorState,
    },

    /// Group of commands executed atomically
    Compound {
        commands: Vec<Command>,
    },
}
```

**Operations**:
- `apply(doc: &mut Document) -> Result<()>`
- `inverse() -> Command`
- `is_empty() -> bool`

---

## Undo Tree Model

### UndoNode

A node in the undo tree.

```rust
/// A node in the undo tree representing a document state.
pub struct UndoNode {
    /// Unique identifier
    pub id: UndoNodeId,
    /// Parent node (None for root)
    pub parent: Option<UndoNodeId>,
    /// Child nodes (branches)
    pub children: Vec<UndoNodeId>,
    /// Command that was applied to reach this state from parent
    pub command: Command,
    /// When this edit was made
    pub timestamp: Instant,
    /// Optional description for this edit
    pub description: Option<String>,
}

/// Opaque identifier for undo nodes
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct UndoNodeId(u64);
```

### UndoTree

The tree structure tracking edit history.

```rust
/// Tree-structured undo history.
pub struct UndoTree {
    /// All nodes in the tree
    nodes: HashMap<UndoNodeId, UndoNode>,
    /// Root node (initial document state)
    root: UndoNodeId,
    /// Current position in the tree
    current: UndoNodeId,
    /// Counter for generating node IDs
    next_id: u64,
    /// Time of last edit (for grouping)
    last_edit_time: Option<Instant>,
    /// Grouping timeout in milliseconds
    group_timeout_ms: u64,
}
```

**Operations**:
- `push(command: Command)`: Add new edit
- `undo() -> Option<Command>`: Move to parent
- `redo() -> Option<Command>`: Move to first child (or specified)
- `redo_branch(index: usize) -> Option<Command>`: Move to specific child
- `current_node() -> &UndoNode`
- `can_undo() -> bool`
- `can_redo() -> bool`
- `branch_count() -> usize`: Children of current node

---

## Viewport Model

### Viewport

The visible region of the document.

```rust
/// The visible region of the document.
pub struct Viewport {
    /// First visible line (0-indexed)
    pub first_line: usize,
    /// Scroll offset within first line (pixels)
    pub scroll_offset_y: f32,
    /// Horizontal scroll offset (pixels)
    pub scroll_offset_x: f32,
    /// Viewport width in pixels
    pub width: f32,
    /// Viewport height in pixels
    pub height: f32,
    /// Number of visible lines (computed)
    pub visible_lines: usize,
}
```

**Operations**:
- `contains_line(line: usize) -> bool`
- `scroll_to_line(line: usize)`
- `scroll_to_position(position: Position)`
- `ensure_cursor_visible(cursor: Position)`

---

## Syntax Model

### SyntaxHighlight

Syntax highlighting information.

```rust
/// A highlighted span of text.
pub struct HighlightSpan {
    /// Range of text this highlight applies to
    pub range: Range,
    /// The highlight type/scope
    pub highlight: HighlightType,
}

/// Types of syntax highlights.
pub enum HighlightType {
    Keyword,
    String,
    Number,
    Comment,
    Function,
    Variable,
    Type,
    Operator,
    Punctuation,
    Property,
    Constant,
    Tag,
    Attribute,
    Error,
}
```

### FoldRegion

A foldable region of code.

```rust
/// A region that can be folded/collapsed.
pub struct FoldRegion {
    /// Range of the foldable region
    pub range: Range,
    /// Whether currently folded
    pub is_folded: bool,
    /// Kind of fold (for icon display)
    pub kind: FoldKind,
}

pub enum FoldKind {
    Block,      // { }
    Region,     // #region / #endregion
    Import,     // import statements
    Comment,    // Multi-line comment
}
```

---

## Theme Model

### Theme

Editor appearance configuration.

```rust
/// Complete theme configuration.
pub struct Theme {
    /// Name of the theme
    pub name: String,
    /// Whether this is a dark theme
    pub is_dark: bool,
    /// Editor chrome colors
    pub editor: EditorColors,
    /// Syntax highlighting colors
    pub syntax: SyntaxColors,
    /// Typography settings
    pub typography: Typography,
}

pub struct EditorColors {
    pub background: Color,
    pub foreground: Color,
    pub selection: Color,
    pub selection_inactive: Color,
    pub cursor: Color,
    pub line_number: Color,
    pub line_number_active: Color,
    pub current_line: Color,
    pub gutter: Color,
    pub minimap_background: Color,
    pub search_match: Color,
    pub search_match_current: Color,
}

pub struct SyntaxColors {
    pub keyword: Color,
    pub string: Color,
    pub number: Color,
    pub comment: Color,
    pub function: Color,
    pub variable: Color,
    pub type_name: Color,
    pub operator: Color,
    pub punctuation: Color,
    pub property: Color,
    pub constant: Color,
    pub tag: Color,
    pub attribute: Color,
    pub error: Color,
}

pub struct Typography {
    pub font_family: String,
    pub font_size: f32,
    pub line_height: f32,
    pub letter_spacing: f32,
}

/// RGBA color.
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}
```

---

## Search Model

### SearchState

State for find/replace operations.

```rust
/// Find/replace search state.
pub struct SearchState {
    /// Current search query
    pub query: String,
    /// Search options
    pub options: SearchOptions,
    /// All matches in document
    pub matches: Vec<Range>,
    /// Currently selected match index
    pub current_match: Option<usize>,
}

pub struct SearchOptions {
    /// Case-sensitive matching
    pub case_sensitive: bool,
    /// Match whole words only
    pub whole_word: bool,
    /// Interpret query as regex
    pub regex: bool,
}
```

**Operations**:
- `find_all(document: &Document) -> Vec<Range>`
- `next_match() -> Option<Range>`
- `previous_match() -> Option<Range>`
- `replace_current(replacement: &str) -> Command`
- `replace_all(replacement: &str) -> Command`

---

## Editor State

### EditorState

Complete editor state.

```rust
/// Complete state of an editor instance.
pub struct EditorState {
    /// The document being edited
    pub document: Document,
    /// Cursor and selection state
    pub cursor: CursorState,
    /// Undo/redo history
    pub history: UndoTree,
    /// Current viewport
    pub viewport: Viewport,
    /// Syntax highlights (cached)
    pub highlights: Vec<HighlightSpan>,
    /// Fold regions
    pub folds: Vec<FoldRegion>,
    /// Active search state
    pub search: Option<SearchState>,
    /// Current theme
    pub theme: Theme,
    /// Editor configuration
    pub config: EditorConfig,
}

pub struct EditorConfig {
    /// Tab width in spaces
    pub tab_width: usize,
    /// Insert spaces instead of tabs
    pub insert_spaces: bool,
    /// Auto-indent on newline
    pub auto_indent: bool,
    /// Show line numbers
    pub show_line_numbers: bool,
    /// Show minimap
    pub show_minimap: bool,
    /// Cursor blink rate (0 = no blink)
    pub cursor_blink_ms: u32,
    /// Undo grouping timeout
    pub undo_group_timeout_ms: u64,
}
```

---

## Event Model

### EditorEvent

Events emitted by the editor to the host application.

```rust
/// Events emitted by the editor.
pub enum EditorEvent {
    /// Document content changed
    ContentChanged {
        /// New full content (for simple hosts)
        content: String,
    },

    /// Cursor/selection changed
    SelectionChanged {
        /// All current selections
        selections: Vec<Selection>,
    },

    /// Scroll position changed
    ScrollChanged {
        /// New first visible line
        first_line: usize,
    },

    /// Search results updated
    SearchUpdated {
        /// Number of matches
        match_count: usize,
        /// Current match index
        current_index: Option<usize>,
    },

    /// Editor error occurred
    Error {
        /// Error message
        message: String,
        /// Error code for programmatic handling
        code: ErrorCode,
    },
}

pub enum ErrorCode {
    GpuInitFailed,
    ShaderCompileFailed,
    FontLoadFailed,
    ParseError,
    InvalidUtf8,
}
```
