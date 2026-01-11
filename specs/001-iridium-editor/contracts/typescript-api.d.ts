/**
 * Iridium Editor TypeScript API
 *
 * This file defines the public TypeScript API for embedding Iridium
 * in web applications. Generated from Rust types via napi-rs.
 *
 * @packageDocumentation
 */

// =============================================================================
// Core Types
// =============================================================================

/**
 * A position in the document specified by line and column.
 * Both are 0-indexed.
 */
export interface Position {
  /** Line number (0-indexed) */
  line: number;
  /** Column offset in UTF-8 code points (0-indexed) */
  column: number;
}

/**
 * A range in the document from start to end.
 * Start is inclusive, end is exclusive.
 */
export interface Range {
  /** Start position (inclusive) */
  start: Position;
  /** End position (exclusive) */
  end: Position;
}

/**
 * A selection with anchor and head.
 * Anchor is where selection started, head is where cursor is.
 */
export interface Selection {
  /** Where the selection started */
  anchor: Position;
  /** Where the cursor/head is */
  head: Position;
}

// =============================================================================
// Theme Types
// =============================================================================

/**
 * RGBA color with components in 0-1 range.
 */
export interface Color {
  r: number;
  g: number;
  b: number;
  a: number;
}

/**
 * Editor chrome colors.
 */
export interface EditorColors {
  background: Color;
  foreground: Color;
  selection: Color;
  selectionInactive: Color;
  cursor: Color;
  lineNumber: Color;
  lineNumberActive: Color;
  currentLine: Color;
  gutter: Color;
  minimapBackground: Color;
  searchMatch: Color;
  searchMatchCurrent: Color;
}

/**
 * Syntax highlighting colors.
 */
export interface SyntaxColors {
  keyword: Color;
  string: Color;
  number: Color;
  comment: Color;
  function: Color;
  variable: Color;
  typeName: Color;
  operator: Color;
  punctuation: Color;
  property: Color;
  constant: Color;
  tag: Color;
  attribute: Color;
  error: Color;
}

/**
 * Typography settings.
 */
export interface Typography {
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  letterSpacing: number;
}

/**
 * Complete theme configuration.
 */
export interface Theme {
  /** Theme name */
  name: string;
  /** Whether this is a dark theme */
  isDark: boolean;
  /** Editor chrome colors */
  editor: EditorColors;
  /** Syntax highlighting colors */
  syntax: SyntaxColors;
  /** Typography settings */
  typography: Typography;
}

// =============================================================================
// Configuration Types
// =============================================================================

/**
 * Editor configuration options.
 */
export interface EditorConfig {
  /** Tab width in spaces (default: 4) */
  tabWidth?: number;
  /** Insert spaces instead of tabs (default: true) */
  insertSpaces?: boolean;
  /** Auto-indent on newline (default: true) */
  autoIndent?: boolean;
  /** Show line numbers (default: true) */
  showLineNumbers?: boolean;
  /** Show minimap (default: true) */
  showMinimap?: boolean;
  /** Cursor blink rate in ms, 0 = no blink (default: 500) */
  cursorBlinkMs?: number;
  /** Undo grouping timeout in ms (default: 500) */
  undoGroupTimeoutMs?: number;
}

/**
 * Search options for find/replace.
 */
export interface SearchOptions {
  /** Case-sensitive matching (default: false) */
  caseSensitive?: boolean;
  /** Match whole words only (default: false) */
  wholeWord?: boolean;
  /** Interpret query as regex (default: false) */
  regex?: boolean;
}

// =============================================================================
// Event Types
// =============================================================================

/**
 * Event emitted when document content changes.
 */
export interface ContentChangedEvent {
  type: 'contentChanged';
  /** New document content */
  content: string;
}

/**
 * Event emitted when selection/cursor changes.
 */
export interface SelectionChangedEvent {
  type: 'selectionChanged';
  /** All current selections */
  selections: Selection[];
}

/**
 * Event emitted when scroll position changes.
 */
export interface ScrollChangedEvent {
  type: 'scrollChanged';
  /** First visible line */
  firstLine: number;
}

/**
 * Event emitted when search results update.
 */
export interface SearchUpdatedEvent {
  type: 'searchUpdated';
  /** Total number of matches */
  matchCount: number;
  /** Current match index (null if no current match) */
  currentIndex: number | null;
}

/**
 * Event emitted on editor error.
 */
export interface ErrorEvent {
  type: 'error';
  /** Error message */
  message: string;
  /** Error code */
  code: ErrorCode;
}

/**
 * Error codes for programmatic error handling.
 */
export type ErrorCode =
  | 'gpuInitFailed'
  | 'shaderCompileFailed'
  | 'fontLoadFailed'
  | 'parseError'
  | 'invalidUtf8';

/**
 * Union of all editor events.
 */
export type EditorEvent =
  | ContentChangedEvent
  | SelectionChangedEvent
  | ScrollChangedEvent
  | SearchUpdatedEvent
  | ErrorEvent;

// =============================================================================
// Undo Tree Types
// =============================================================================

/**
 * Information about an undo tree node.
 */
export interface UndoNodeInfo {
  /** Unique node identifier */
  id: string;
  /** Parent node ID (null for root) */
  parentId: string | null;
  /** Child node IDs */
  childIds: string[];
  /** When this edit was made */
  timestamp: number;
  /** Optional description */
  description: string | null;
}

/**
 * Information about the undo tree structure.
 */
export interface UndoTreeInfo {
  /** Current node ID */
  currentId: string;
  /** Root node ID */
  rootId: string;
  /** Total number of nodes */
  nodeCount: number;
  /** Can undo from current position */
  canUndo: boolean;
  /** Can redo from current position */
  canRedo: boolean;
  /** Number of branches at current node */
  branchCount: number;
}

// =============================================================================
// Editor Instance
// =============================================================================

/**
 * Event listener callback type.
 */
export type EventListener = (event: EditorEvent) => void;

/**
 * The main Iridium editor instance.
 */
export interface IridiumEditor {
  // ---------------------------------------------------------------------------
  // Content Management
  // ---------------------------------------------------------------------------

  /**
   * Get the current document content.
   */
  getContent(): string;

  /**
   * Set the document content, replacing everything.
   * This clears undo history.
   */
  setContent(content: string): void;

  /**
   * Get the content of a specific line.
   */
  getLine(lineNumber: number): string | null;

  /**
   * Get the total number of lines.
   */
  getLineCount(): number;

  // ---------------------------------------------------------------------------
  // Cursor and Selection
  // ---------------------------------------------------------------------------

  /**
   * Get all current selections.
   */
  getSelections(): Selection[];

  /**
   * Set the primary cursor position (clears other cursors).
   */
  setCursor(position: Position): void;

  /**
   * Set the primary selection (clears other cursors).
   */
  setSelection(selection: Selection): void;

  /**
   * Set multiple selections.
   */
  setSelections(selections: Selection[]): void;

  /**
   * Add a cursor at the specified position.
   */
  addCursor(position: Position): void;

  /**
   * Get the primary cursor position.
   */
  getCursor(): Position;

  // ---------------------------------------------------------------------------
  // Editing Operations
  // ---------------------------------------------------------------------------

  /**
   * Insert text at current cursor position(s).
   */
  insert(text: string): void;

  /**
   * Delete text in the specified range.
   */
  delete(range: Range): void;

  /**
   * Replace text in the specified range.
   */
  replace(range: Range, text: string): void;

  // ---------------------------------------------------------------------------
  // Undo/Redo
  // ---------------------------------------------------------------------------

  /**
   * Undo the last operation.
   * @returns true if undo was performed
   */
  undo(): boolean;

  /**
   * Redo the last undone operation.
   * @returns true if redo was performed
   */
  redo(): boolean;

  /**
   * Redo a specific branch (when multiple branches exist).
   * @param branchIndex The branch to redo (0-indexed)
   * @returns true if redo was performed
   */
  redoBranch(branchIndex: number): boolean;

  /**
   * Check if undo is available.
   */
  canUndo(): boolean;

  /**
   * Check if redo is available.
   */
  canRedo(): boolean;

  /**
   * Get information about the undo tree.
   */
  getUndoTreeInfo(): UndoTreeInfo;

  /**
   * Get information about a specific undo node.
   */
  getUndoNode(nodeId: string): UndoNodeInfo | null;

  /**
   * Jump to a specific node in the undo tree.
   */
  jumpToUndoNode(nodeId: string): boolean;

  // ---------------------------------------------------------------------------
  // Viewport and Scrolling
  // ---------------------------------------------------------------------------

  /**
   * Scroll to make a specific line visible.
   */
  scrollToLine(lineNumber: number): void;

  /**
   * Scroll to make a position visible.
   */
  scrollToPosition(position: Position): void;

  /**
   * Get the first visible line.
   */
  getFirstVisibleLine(): number;

  /**
   * Get the number of visible lines.
   */
  getVisibleLineCount(): number;

  // ---------------------------------------------------------------------------
  // Search
  // ---------------------------------------------------------------------------

  /**
   * Start a search with the given query.
   */
  find(query: string, options?: SearchOptions): void;

  /**
   * Move to the next search match.
   */
  findNext(): void;

  /**
   * Move to the previous search match.
   */
  findPrevious(): void;

  /**
   * Replace the current match.
   */
  replaceMatch(replacement: string): void;

  /**
   * Replace all matches.
   */
  replaceAll(replacement: string): void;

  /**
   * Clear the current search.
   */
  clearSearch(): void;

  /**
   * Get the current search match count.
   */
  getMatchCount(): number;

  // ---------------------------------------------------------------------------
  // Code Folding
  // ---------------------------------------------------------------------------

  /**
   * Fold the region containing the given line.
   */
  foldAt(lineNumber: number): void;

  /**
   * Unfold the region containing the given line.
   */
  unfoldAt(lineNumber: number): void;

  /**
   * Fold all foldable regions.
   */
  foldAll(): void;

  /**
   * Unfold all folded regions.
   */
  unfoldAll(): void;

  /**
   * Toggle fold state at the given line.
   */
  toggleFoldAt(lineNumber: number): void;

  // ---------------------------------------------------------------------------
  // Configuration
  // ---------------------------------------------------------------------------

  /**
   * Set the editor theme.
   */
  setTheme(theme: Theme): void;

  /**
   * Get the current theme.
   */
  getTheme(): Theme;

  /**
   * Update editor configuration.
   */
  setConfig(config: Partial<EditorConfig>): void;

  /**
   * Get current editor configuration.
   */
  getConfig(): EditorConfig;

  /**
   * Set the language for syntax highlighting.
   */
  setLanguage(language: string): void;

  /**
   * Get the current language.
   */
  getLanguage(): string | null;

  // ---------------------------------------------------------------------------
  // Events
  // ---------------------------------------------------------------------------

  /**
   * Add an event listener.
   */
  addEventListener(listener: EventListener): void;

  /**
   * Remove an event listener.
   */
  removeEventListener(listener: EventListener): void;

  // ---------------------------------------------------------------------------
  // Lifecycle
  // ---------------------------------------------------------------------------

  /**
   * Give keyboard focus to the editor.
   */
  focus(): void;

  /**
   * Remove keyboard focus from the editor.
   */
  blur(): void;

  /**
   * Check if the editor has focus.
   */
  hasFocus(): boolean;

  /**
   * Resize the editor to fit its container.
   * Call this when the container size changes.
   */
  resize(): void;

  /**
   * Destroy the editor and release resources.
   */
  destroy(): void;
}

// =============================================================================
// Factory Function
// =============================================================================

/**
 * Options for creating an editor instance.
 */
export interface CreateEditorOptions {
  /** The canvas element to render into */
  canvas: HTMLCanvasElement;
  /** Initial content */
  content?: string;
  /** Initial language for syntax highlighting */
  language?: string;
  /** Initial theme */
  theme?: Theme;
  /** Editor configuration */
  config?: EditorConfig;
}

/**
 * Create a new Iridium editor instance.
 *
 * @param options - Configuration options
 * @returns Promise that resolves to the editor instance
 * @throws Error if WebGPU is not available
 *
 * @example
 * ```typescript
 * const canvas = document.getElementById('editor') as HTMLCanvasElement;
 * const editor = await createEditor({
 *   canvas,
 *   content: 'MATCH (n) RETURN n',
 *   language: 'cypher',
 * });
 *
 * editor.addEventListener((event) => {
 *   if (event.type === 'contentChanged') {
 *     console.log('Content:', event.content);
 *   }
 * });
 * ```
 */
export function createEditor(options: CreateEditorOptions): Promise<IridiumEditor>;

// =============================================================================
// Utility Functions
// =============================================================================

/**
 * Check if WebGPU is supported in the current environment.
 */
export function isWebGPUSupported(): Promise<boolean>;

/**
 * Get the list of supported languages for syntax highlighting.
 */
export function getSupportedLanguages(): string[];

/**
 * Built-in themes.
 */
export const themes: {
  dark: Theme;
  light: Theme;
};
