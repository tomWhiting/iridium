# Incremental Parse API Contract

**Module**: TypeScript `syntax/index.ts`
**Purpose**: Efficient incremental tree-sitter parsing on document edits

## Types

### EditInfo

```typescript
interface EditInfo {
    /** Byte where edit began */
    startIndex: number;
    /** Byte where old content ended */
    oldEndIndex: number;
    /** Byte where new content ends */
    newEndIndex: number;
    /** Position where edit began */
    startPosition: { row: number; column: number };
    /** Position where old content ended */
    oldEndPosition: { row: number; column: number };
    /** Position where new content ends */
    newEndPosition: { row: number; column: number };
}
```

### HighlightSpan

```typescript
interface HighlightSpan {
    /** Start byte offset (inclusive) */
    start: number;
    /** End byte offset (exclusive) */
    end: number;
    /** Highlight category name */
    type: string;
}
```

## SyntaxHighlighter Methods

### `highlightIncremental`

```typescript
/**
 * Update syntax tree with edit and re-highlight.
 *
 * @param content - Full document content after edit
 * @param edit - Information about the edit that occurred
 * @returns Array of highlight spans for the document
 *
 * This is more efficient than highlight() for single edits
 * as it reuses unchanged portions of the syntax tree.
 */
highlightIncremental(content: string, edit: EditInfo): HighlightSpan[];
```

### `highlight` (existing, full reparse)

```typescript
/**
 * Parse content from scratch and generate highlights.
 *
 * @param content - Full document content
 * @returns Array of highlight spans
 *
 * Use highlightIncremental() when edit info is available.
 * This method is for initial parse or when edit tracking is lost.
 */
highlight(content: string): HighlightSpan[];
```

### `getChangedRanges`

```typescript
/**
 * Get byte ranges that changed between old and new parse.
 *
 * @returns Array of {startIndex, endIndex} ranges
 *
 * Useful for targeted span updates instead of full regeneration.
 */
getChangedRanges(): Array<{ startIndex: number; endIndex: number }>;
```

## Controller Integration

### Edit Tracking in IridiumEditor

```typescript
class IridiumEditor {
    /**
     * Track edit for incremental parsing.
     * Called internally by insert/delete operations.
     */
    private trackEdit(
        startByte: number,
        oldEndByte: number,
        newEndByte: number,
        startLine: number,
        startColumn: number,
        oldEndLine: number,
        oldEndColumn: number,
        newEndLine: number,
        newEndColumn: number
    ): void;

    /**
     * Get pending edit info and clear it.
     * Returns null if no edits since last call.
     */
    private consumeEditInfo(): EditInfo | null;

    /**
     * Update highlights after edit.
     * Uses incremental parsing when edit info available.
     */
    private updateHighlights(): void {
        const edit = this.consumeEditInfo();
        const content = this.editor.getContent();

        const spans = edit
            ? this.syntax.highlightIncremental(content, edit)
            : this.syntax.highlight(content);

        this.editor.setTreeSitterHighlights(spans);
    }
}
```

## Usage Example

```typescript
// User types a character at position (line 5, column 10), byte 150
const edit: EditInfo = {
    startIndex: 150,
    oldEndIndex: 150,      // insertion: old and new start at same point
    newEndIndex: 151,      // one character inserted
    startPosition: { row: 5, column: 10 },
    oldEndPosition: { row: 5, column: 10 },
    newEndPosition: { row: 5, column: 11 }
};

// Incremental parse (fast)
const spans = syntax.highlightIncremental(newContent, edit);

// vs. full parse (slow for large files)
const spans = syntax.highlight(newContent);
```

## Error Handling

- Invalid edit info falls back to full reparse
- Parse errors return previously cached spans (or empty array on first parse)
- Out-of-range edits are clamped to document bounds

## Performance Notes

- Incremental parse: O(edit_size + affected_tree_nodes)
- Full parse: O(file_size)
- For small edits in large files: 10-100x faster with incremental
