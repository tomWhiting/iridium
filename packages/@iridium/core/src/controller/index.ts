/**
 * IridiumEditor - High-level wrapper for the Iridium WebGPU editor.
 *
 * This controller handles all the boilerplate:
 * - WASM initialization
 * - Event handling (keyboard, mouse, clipboard)
 * - Syntax highlighting integration (via Web Worker for 120fps)
 * - Render loop
 *
 * Usage:
 * ```typescript
 * import { IridiumEditor } from '@iridium/core';
 *
 * const editor = await IridiumEditor.create(canvas, {
 *   language: 'rust',
 *   darkTheme: true,
 * });
 * ```
 */

// Import syntax worker client - uses relative path for bundler compatibility
import { SyntaxHighlightClient, type EditInfo } from "../../../syntax-worker/src/client";

// Types for the low-level WASM editor
interface WebEditor {
  loadFont(data: Uint8Array): void;
  setContent(content: string): void;
  applyTextDelta(replacements: TextReplacement[]): void;
  getContent(): string;
  setDarkTheme(dark: boolean): void;
  forceRender(): void;
  render(): void;
  resize(width: number, height: number): void;
  insert(text: string): void;
  backspace(): void;
  delete_forward(): void;
  undo(): boolean;
  redo(): boolean;
  canUndo(): boolean;
  canRedo(): boolean;
  moveCursorLeft(): void;
  moveCursorRight(): void;
  moveCursorUp(): void;
  moveCursorDown(): void;
  moveCursorWordLeft(): void;
  moveCursorWordRight(): void;
  moveCursorLineStart(): void;
  moveCursorLineEnd(): void;
  moveCursorDocStart(): void;
  moveCursorDocEnd(): void;
  extendSelectionLeft(): void;
  extendSelectionRight(): void;
  extendSelectionUp(): void;
  extendSelectionDown(): void;
  extendSelectionWordLeft(): void;
  extendSelectionWordRight(): void;
  extendSelectionLineStart(): void;
  extendSelectionLineEnd(): void;
  extendSelectionDocStart(): void;
  extendSelectionDocEnd(): void;
  extendSelectionToPosition(line: number, column: number): void;
  selectAll(): void;
  getSelectedText(): string;
  hasSelection(): boolean;
  deleteWordBackward(): void;
  deleteWordForward(): void;
  deleteToLineStart(): void;
  deleteToLineEnd(): void;
  setCursorFromClick(line: number, column: number): void;
  pixelToPosition(x: number, y: number): number[];
  ensureCursorVisible(): void;
  scrollBy(delta: number): void;
  getCursorLine(): number;
  getCursorColumn(): number;
  getLineCount(): number;
  setTreeSitterHighlights(spans: { start: number; end: number; type: string }[]): void;
  foldAll(): void;
  unfoldAll(): void;
  toggleFold(line: number): boolean;
  isFoldable(line: number): boolean;
  isFolded(line: number): boolean;
  getFoldableLines(): number[];
  getFoldedLines(): number[];
  getHiddenLineCount(): number;
  isSyntaxEnabled(): boolean;
  setSyntaxEnabled(enabled: boolean): void;
  setSyntaxTheme(theme: Record<string, string>): void;
  isGutterEnabled(): boolean;
  setGutterEnabled(enabled: boolean): void;
  isTreeSitterActive(): boolean;
  getScrollY(): number;
  getLineHeight(): number;
  // Git integration: read-only, line backgrounds, gutter changes, blame
  setReadOnly(readOnly: boolean): void;
  isReadOnly(): boolean;
  setLineBackgrounds(backgrounds: Array<{ line: number; color: string }>): void;
  clearLineBackgrounds(): void;
  setGutterChanges(changes: Array<{ line: number; kind: string }>): void;
  clearGutterChanges(): void;
  setCustomGutterText(lines: string[] | null): void;
  setBlameData(data: Array<{ line: number; text: string }>): void;
  clearBlameData(): void;
  setUnderlineTheme(theme: Record<string, string>): void;
  setUnderlineDecorations(decorations: UnderlineDecoration[]): void;
}

export interface TextReplacement {
  /** Inclusive UTF-8 byte offset in the pre-edit document. */
  start: number;
  /** Exclusive UTF-8 byte offset in the pre-edit document. */
  end: number;
  text: string;
}

export interface UnderlineDecoration {
  /** Inclusive UTF-8 byte offset. */
  start: number;
  /** Exclusive UTF-8 byte offset. */
  end: number;
  /** Name previously configured with setUnderlineTheme. */
  colorClass: string;
}

export interface IridiumEditorOptions {
  /** Initial content */
  content?: string;
  /** Language for syntax highlighting (default: 'rust') */
  language?: string;
  /** Use dark theme (default: true) */
  darkTheme?: boolean;
  /** Font URL to load (default: FiraCode from CDN) */
  fontUrl?: string;
  /**
   * Enable syntax highlighting via web-tree-sitter worker (default: false).
   * WARNING: This can cause 1-3 second delays on large files.
   * Set to true only if you need client-side syntax highlighting.
   * For Meridian integration, leave disabled and use server-provided spans.
   */
  enableSyntaxWorker?: boolean;
  /** Callback when content changes */
  onChange?: (content: string) => void;
  /** Callback when cursor/selection changes */
  onSelectionChange?: (info: { line: number; column: number; hasSelection: boolean }) => void;
}

export interface EditorState {
  line: number;
  column: number;
  lineCount: number;
  hasSelection: boolean;
  canUndo: boolean;
  canRedo: boolean;
  language: string;
  foldedLines: number[];
  hiddenLineCount: number;
}

// Auto-pair mappings
const PAIRS: Record<string, string> = {
  "(": ")",
  "[": "]",
  "{": "}",
  '"': '"',
  "'": "'",
};
const CLOSERS = [")", "]", "}", '"', "'"];

// Track canvases that are being initialized to prevent double-init from React StrictMode
const pendingInitializations = new Map<HTMLCanvasElement, Promise<IridiumEditor>>();
const initializedCanvases = new WeakMap<HTMLCanvasElement, IridiumEditor>();

/**
 * High-level Iridium editor with batteries included.
 */
export class IridiumEditor {
  private canvas: HTMLCanvasElement;
  private editor: WebEditor;
  private syntaxWorker: SyntaxHighlightClient | null = null;
  private options: Required<Omit<IridiumEditorOptions, "onChange" | "onSelectionChange">> & Pick<IridiumEditorOptions, "onChange" | "onSelectionChange">;
  private currentLanguage: string;
  private isDragging = false;
  private animationFrameId = 0;
  private lastBlinkTime = 0;
  private eventCleanup: (() => void)[] = [];
  private destroyed = false;
  /** Pending edit info for incremental tree-sitter parsing (T027) */
  private lastEditInfo: EditInfo | null = null;

  private constructor(
    canvas: HTMLCanvasElement,
    editor: WebEditor,
    options: IridiumEditorOptions
  ) {
    this.canvas = canvas;
    this.editor = editor;
    this.options = {
      content: options.content ?? "",
      language: options.language ?? "rust",
      darkTheme: options.darkTheme ?? true,
      fontUrl: options.fontUrl ?? "https://cdn.jsdelivr.net/npm/firacode@6.2.0/distr/ttf/FiraCode-Regular.ttf",
      enableSyntaxWorker: options.enableSyntaxWorker ?? false,
      onChange: options.onChange,
      onSelectionChange: options.onSelectionChange,
    };
    this.currentLanguage = this.options.language;
  }

  /**
   * Create a new IridiumEditor instance.
   */
  static async create(
    canvas: HTMLCanvasElement,
    options: IridiumEditorOptions = {}
  ): Promise<IridiumEditor> {
    // Check if this canvas already has an editor (React StrictMode protection)
    const existingEditor = initializedCanvases.get(canvas);
    if (existingEditor && !existingEditor.destroyed) {
      console.log("[IridiumEditor] Canvas already has an editor attached, returning existing instance");
      return existingEditor;
    }

    // Check if initialization is already in progress (race condition protection)
    const pendingInit = pendingInitializations.get(canvas);
    if (pendingInit) {
      console.log("[IridiumEditor] Initialization already in progress, waiting...");
      return pendingInit;
    }

    // Create the initialization promise
    const initPromise = IridiumEditor.doCreate(canvas, options);
    pendingInitializations.set(canvas, initPromise);

    try {
      const instance = await initPromise;
      initializedCanvases.set(canvas, instance);
      return instance;
    } finally {
      pendingInitializations.delete(canvas);
    }
  }

  /**
   * Internal creation logic.
   */
  private static async doCreate(
    canvas: HTMLCanvasElement,
    options: IridiumEditorOptions
  ): Promise<IridiumEditor> {
    // Wait for canvas to have a valid size (React may not have laid it out yet)
    let rect = canvas.getBoundingClientRect();
    let attempts = 0;
    while ((rect.width === 0 || rect.height === 0) && attempts < 50) {
      await new Promise(resolve => setTimeout(resolve, 20));
      rect = canvas.getBoundingClientRect();
      attempts++;
    }

    if (rect.width === 0 || rect.height === 0) {
      throw new Error("Canvas has no size - ensure it's mounted and visible");
    }

    // Dynamically import the WASM module
    // Use import.meta.url to resolve relative to this file's location
    const wasmUrl = new URL(
      "../../../../../crates/iridium-bindings/pkg/iridium_bindings.js",
      import.meta.url
    ).href;
    const wasm = await import(/* @vite-ignore */ wasmUrl);
    await wasm.default();

    // Set canvas size
    const pixelRatio = window.devicePixelRatio || 1;
    canvas.width = rect.width * pixelRatio;
    canvas.height = rect.height * pixelRatio;

    // Create the low-level editor
    const editor = await wasm.createWebEditor(canvas, pixelRatio);

    // Create the controller
    const instance = new IridiumEditor(canvas, editor, options);

    // Initialize
    await instance.initialize();

    return instance;
  }

  private async initialize(): Promise<void> {
    // Load font
    const fontResponse = await fetch(this.options.fontUrl);
    if (fontResponse.ok) {
      const fontData = new Uint8Array(await fontResponse.arrayBuffer());
      this.editor.loadFont(fontData);
    }

    // Initialize syntax highlighting via Web Worker (only if enabled)
    // DISABLED BY DEFAULT: Web worker parsing causes 1-3s delays on large files.
    // For Meridian integration, syntax spans will come from the server.
    if (this.options.enableSyntaxWorker) {
      try {
        // Create worker - the consumer's bundler handles the worker URL
        this.syntaxWorker = new SyntaxHighlightClient(() => {
          // Use import.meta.url to resolve worker relative to this module
          // Path: controller/index.ts → ../../.. → @iridium/ → syntax-worker/src/worker.ts
          const workerUrl = new URL(
            "../../../syntax-worker/src/worker.ts",
            import.meta.url
          );
          return new Worker(workerUrl, { type: "module" });
        });
        await this.syntaxWorker.initialize(this.options.language);
        console.log("[IridiumEditor] Syntax worker initialized");
      } catch (e) {
        console.warn("[IridiumEditor] Syntax highlighting not available:", e);
        this.syntaxWorker = null;
      }
    }

    // Set initial content
    if (this.options.content) {
      this.editor.setContent(this.options.content);
    }

    // Set theme
    this.editor.setDarkTheme(this.options.darkTheme);

    // Apply syntax highlighting
    this.updateHighlights();

    // Set up event handlers
    this.attachEventHandlers();

    // Start render loop
    this.startRenderLoop();

    // Initial render
    this.editor.forceRender();

    // Focus the canvas
    this.canvas.focus();
  }

  private attachEventHandlers(): void {
    // Keyboard
    const handleKeyDown = this.handleKeyDown.bind(this);
    this.canvas.addEventListener("keydown", handleKeyDown);
    this.eventCleanup.push(() => this.canvas.removeEventListener("keydown", handleKeyDown));

    // Clipboard events - critical for Safari compatibility
    // Safari requires direct paste event handling rather than intercepting Ctrl/Cmd+V
    // The paste event provides secure, synchronous access to clipboard via clipboardData
    const handlePaste = this.handlePaste.bind(this);
    const handleCopy = this.handleCopy.bind(this);
    const handleCut = this.handleCut.bind(this);
    this.canvas.addEventListener("paste", handlePaste);
    this.canvas.addEventListener("copy", handleCopy);
    this.canvas.addEventListener("cut", handleCut);
    this.eventCleanup.push(() => {
      this.canvas.removeEventListener("paste", handlePaste);
      this.canvas.removeEventListener("copy", handleCopy);
      this.canvas.removeEventListener("cut", handleCut);
    });

    // Mouse
    const handleMouseDown = this.handleMouseDown.bind(this);
    const handleMouseMove = this.handleMouseMove.bind(this);
    const handleMouseUp = this.handleMouseUp.bind(this);
    const handleWheel = this.handleWheel.bind(this);

    this.canvas.addEventListener("mousedown", handleMouseDown);
    this.canvas.addEventListener("mousemove", handleMouseMove);
    this.canvas.addEventListener("mouseup", handleMouseUp);
    document.addEventListener("mouseup", handleMouseUp);
    this.canvas.addEventListener("wheel", handleWheel, { passive: false });

    this.eventCleanup.push(() => {
      this.canvas.removeEventListener("mousedown", handleMouseDown);
      this.canvas.removeEventListener("mousemove", handleMouseMove);
      this.canvas.removeEventListener("mouseup", handleMouseUp);
      document.removeEventListener("mouseup", handleMouseUp);
      this.canvas.removeEventListener("wheel", handleWheel);
    });

    // Resize - observe parent container, not canvas (CSS % sizing doesn't trigger resize on canvas itself)
    const resizeTarget = this.canvas.parentElement || this.canvas;
    const resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const { width, height } = entry.contentRect;
        if (width === 0 || height === 0) continue;
        const pixelRatio = window.devicePixelRatio || 1;
        this.canvas.width = width * pixelRatio;
        this.canvas.height = height * pixelRatio;
        this.editor.resize(this.canvas.width, this.canvas.height);
        this.editor.forceRender();
      }
    });
    resizeObserver.observe(resizeTarget);
    this.eventCleanup.push(() => resizeObserver.disconnect());
  }

  private startRenderLoop(): void {
    const BLINK_INTERVAL = 500;

    const animate = (currentTime: number) => {
      if (currentTime - this.lastBlinkTime >= BLINK_INTERVAL) {
        this.editor.forceRender();
        this.lastBlinkTime = currentTime;
      }
      this.animationFrameId = requestAnimationFrame(animate);
    };

    this.animationFrameId = requestAnimationFrame(animate);
  }

  private getMousePosition(e: MouseEvent): [number, number] {
    const rect = this.canvas.getBoundingClientRect();
    const x = (e.clientX - rect.left) * window.devicePixelRatio;
    const y = (e.clientY - rect.top) * window.devicePixelRatio;
    const pos = this.editor.pixelToPosition(x, y);
    return [pos[0], pos[1]];
  }

  private handleMouseDown(e: MouseEvent): void {
    this.canvas.focus();
    const [line, column] = this.getMousePosition(e);

    if (e.shiftKey) {
      this.editor.extendSelectionToPosition(line, column);
    } else {
      this.editor.setCursorFromClick(line, column);
      this.isDragging = true;
    }
    this.editor.forceRender();
    this.notifySelectionChange();
  }

  private handleMouseMove(e: MouseEvent): void {
    if (!this.isDragging) return;
    const [line, column] = this.getMousePosition(e);
    this.editor.extendSelectionToPosition(line, column);
    this.editor.forceRender();
    this.notifySelectionChange();
  }

  private handleMouseUp(): void {
    this.isDragging = false;
  }

  private handleWheel(e: WheelEvent): void {
    e.preventDefault();
    this.editor.scrollBy(e.deltaY);
    // Update highlights immediately - tree reuse makes this fast
    this.updateHighlights();
    this.editor.forceRender();
  }

  private handleKeyDown(e: KeyboardEvent): void {
    let handled = true;
    const isMac = /Mac|iPhone|iPad|iPod/i.test(navigator.userAgent);
    const selecting = e.shiftKey;
    const isReadOnly = this.editor.isReadOnly();

    // Arrow keys with modifiers
    if (e.key === "ArrowLeft") {
      if (e.metaKey && isMac) {
        selecting ? this.editor.extendSelectionLineStart() : this.editor.moveCursorLineStart();
      } else if (e.altKey || (e.ctrlKey && !isMac)) {
        selecting ? this.editor.extendSelectionWordLeft() : this.editor.moveCursorWordLeft();
      } else {
        selecting ? this.editor.extendSelectionLeft() : this.editor.moveCursorLeft();
      }
    } else if (e.key === "ArrowRight") {
      if (e.metaKey && isMac) {
        selecting ? this.editor.extendSelectionLineEnd() : this.editor.moveCursorLineEnd();
      } else if (e.altKey || (e.ctrlKey && !isMac)) {
        selecting ? this.editor.extendSelectionWordRight() : this.editor.moveCursorWordRight();
      } else {
        selecting ? this.editor.extendSelectionRight() : this.editor.moveCursorRight();
      }
    } else if (e.key === "ArrowUp") {
      if (e.metaKey && isMac) {
        selecting ? this.editor.extendSelectionDocStart() : this.editor.moveCursorDocStart();
      } else {
        selecting ? this.editor.extendSelectionUp() : this.editor.moveCursorUp();
      }
    } else if (e.key === "ArrowDown") {
      if (e.metaKey && isMac) {
        selecting ? this.editor.extendSelectionDocEnd() : this.editor.moveCursorDocEnd();
      } else {
        selecting ? this.editor.extendSelectionDown() : this.editor.moveCursorDown();
      }
    } else if (e.key === "Home") {
      if (e.metaKey || e.ctrlKey) {
        selecting ? this.editor.extendSelectionDocStart() : this.editor.moveCursorDocStart();
      } else {
        selecting ? this.editor.extendSelectionLineStart() : this.editor.moveCursorLineStart();
      }
    } else if (e.key === "End") {
      if (e.metaKey || e.ctrlKey) {
        selecting ? this.editor.extendSelectionDocEnd() : this.editor.moveCursorDocEnd();
      } else {
        selecting ? this.editor.extendSelectionLineEnd() : this.editor.moveCursorLineEnd();
      }
    } else if (e.key === "a" && (e.metaKey || e.ctrlKey)) {
      this.editor.selectAll();
    } else if (e.key === "Backspace") {
      if (!isReadOnly) this.handleBackspace(e, isMac); else handled = false;
    } else if (e.key === "Delete") {
      if (!isReadOnly) this.handleDelete(e, isMac); else handled = false;
    } else if (e.key === "k" && e.ctrlKey) {
      if (!isReadOnly) {
        // Delete to line end - track deletion (T029)
        const startByte = this.getCursorByteOffset();
        const content = this.editor.getContent();
        const lines = content.split("\n");
        const line = this.editor.getCursorLine();
        const lineEnd = lines[line]?.length || 0;
        const lineEndByte = this.positionToByteOffset(content, line, lineEnd);
        this.editor.deleteToLineEnd();
        this.trackEdit(startByte, lineEndByte, startByte);
        this.notifyContentChange();
      } else { handled = false; }
    } else if (e.key === "Enter") {
      if (!isReadOnly) this.handleEnter(); else handled = false;
    } else if (e.key === "Tab") {
      if (!isReadOnly) {
        // Tab insertion - 4 spaces (T029)
        const startByte = this.getCursorByteOffset();
        this.editor.insert("    ");
        this.trackEdit(startByte, startByte, startByte + 4);
        this.notifyContentChange();
      } else { handled = false; }
    } else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey) {
      if (!isReadOnly) this.handleCharacterInput(e.key); else handled = false;
    } else if (e.metaKey || e.ctrlKey) {
      handled = this.handleShortcut(e);
    } else {
      handled = false;
    }

    if (handled) {
      e.preventDefault();
      this.editor.ensureCursorVisible();
      // Render immediately for responsive typing feedback
      this.editor.forceRender();
      // Fire async worker request - will re-render with updated highlights when ready
      this.updateHighlights();
      this.notifySelectionChange();
    }
  }


  private handleBackspace(e: KeyboardEvent, isMac: boolean): void {
    const startByte = this.getCursorByteOffset();

    if (e.metaKey && isMac) {
      // Delete to line start - complex deletion (T029)
      const content = this.editor.getContent();
      const line = this.editor.getCursorLine();
      const lineStart = this.positionToByteOffset(content, line, 0);
      this.editor.deleteToLineStart();
      this.trackEdit(lineStart, startByte, lineStart);
    } else if (e.altKey || (e.ctrlKey && !isMac)) {
      // Word deletion - track by measuring content change (T029)
      const contentBefore = this.editor.getContent();
      this.editor.deleteWordBackward();
      const contentAfter = this.editor.getContent();
      const deletedLen = contentBefore.length - contentAfter.length;
      const newStartByte = startByte - deletedLen;
      this.trackEdit(newStartByte, startByte, newStartByte);
    } else {
      // Check for auto-pair deletion
      const content = this.editor.getContent();
      const lines = content.split("\n");
      const line = this.editor.getCursorLine();
      const col = this.editor.getCursorColumn();
      const currentLine = lines[line] || "";
      const charBefore = col > 0 ? currentLine[col - 1] : "";
      const charAfter = currentLine[col] || "";

      if (PAIRS[charBefore] && charAfter === PAIRS[charBefore]) {
        // Delete both characters of pair (T029)
        this.editor.backspace();
        this.editor.delete_forward();
        this.trackEdit(startByte - 1, startByte + 1, startByte - 1);
      } else {
        // Single character backspace (T029)
        this.editor.backspace();
        this.trackEdit(startByte - 1, startByte, startByte - 1);
      }
    }
    this.notifyContentChange();
  }

  private handleDelete(e: KeyboardEvent, isMac: boolean): void {
    const startByte = this.getCursorByteOffset();

    if (e.altKey || (e.ctrlKey && !isMac)) {
      // Word deletion forward - track by measuring content change (T029)
      const contentBefore = this.editor.getContent();
      this.editor.deleteWordForward();
      const contentAfter = this.editor.getContent();
      const deletedLen = contentBefore.length - contentAfter.length;
      this.trackEdit(startByte, startByte + deletedLen, startByte);
    } else {
      // Single character delete forward (T029)
      this.editor.delete_forward();
      this.trackEdit(startByte, startByte + 1, startByte);
    }
    this.notifyContentChange();
  }

  private handleEnter(): void {
    const content = this.editor.getContent();
    const lines = content.split("\n");
    const line = this.editor.getCursorLine();
    const col = this.editor.getCursorColumn();
    const currentLine = lines[line] || "";
    const charBefore = col > 0 ? currentLine[col - 1] : "";
    const charAfter = currentLine[col] || "";
    const indent = currentLine.match(/^(\s*)/)?.[1] || "";
    const textBeforeCursor = currentLine.slice(0, col);
    const startByte = this.getCursorByteOffset();

    // Check for code block (triple backticks)
    const codeBlockMatch = textBeforeCursor.match(/^(\s*)```(\w*)$/);
    if (codeBlockMatch) {
      const blockIndent = codeBlockMatch[1];
      const insertedText = "\n" + blockIndent + "\n" + blockIndent + "```";
      this.editor.insert(insertedText);
      this.editor.moveCursorUp();
      this.editor.moveCursorLineEnd();
      // Track insertion (T029)
      this.trackEdit(startByte, startByte, startByte + insertedText.length);
    }
    // Check for bracket pairs
    else if (
      (charBefore === "{" && charAfter === "}") ||
      (charBefore === "[" && charAfter === "]") ||
      (charBefore === "(" && charAfter === ")")
    ) {
      const insertedText = "\n" + indent + "    \n" + indent;
      this.editor.insert(insertedText);
      this.editor.moveCursorUp();
      this.editor.moveCursorLineEnd();
      // Track insertion (T029)
      this.trackEdit(startByte, startByte, startByte + insertedText.length);
    } else {
      // Regular enter with indent preservation
      const endsWithOpener = /[{(\[]$/.test(textBeforeCursor.trim());
      const insertedText = endsWithOpener ? "\n" + indent + "    " : "\n" + indent;
      this.editor.insert(insertedText);
      // Track insertion (T029)
      this.trackEdit(startByte, startByte, startByte + insertedText.length);
    }
    this.notifyContentChange();
  }

  private handleCharacterInput(char: string): void {
    const content = this.editor.getContent();
    const lines = content.split("\n");
    const line = this.editor.getCursorLine();
    const col = this.editor.getCursorColumn();
    const currentLine = lines[line] || "";
    const charAfter = currentLine[col] || "";

    if (CLOSERS.includes(char) && charAfter === char) {
      // Skip over auto-inserted closing char
      this.editor.moveCursorRight();
    } else if (PAIRS[char]) {
      // Insert pair and move cursor back
      const startByte = this.getCursorByteOffset();
      this.editor.insert(char + PAIRS[char]);
      this.editor.moveCursorLeft();
      // Track as 2-character insertion (T029)
      this.trackEdit(startByte, startByte, startByte + 2);
    } else {
      const startByte = this.getCursorByteOffset();
      this.editor.insert(char);
      // Track single character insertion (T029)
      this.trackEdit(startByte, startByte, startByte + char.length);
    }
    this.notifyContentChange();
  }

  private handleShortcut(e: KeyboardEvent): boolean {
    if (e.key === "z") {
      if (e.shiftKey) {
        this.editor.redo();
      } else {
        this.editor.undo();
      }
      this.notifyContentChange();
      return true;
    } else if (e.key === "y") {
      this.editor.redo();
      this.notifyContentChange();
      return true;
    } else if (e.key === "c" || e.key === "x" || e.key === "v") {
      // Clipboard operations are handled by native paste/copy/cut events
      // This is critical for Safari compatibility - Safari requires direct
      // paste event handling rather than intercepting keyboard shortcuts.
      // Return false to let the native event fire.
      return false;
    }
    return false;
  }

  /**
   * Handle native paste event - critical for Safari compatibility.
   *
   * Safari requires direct paste event handling rather than intercepting Ctrl/Cmd+V.
   * The paste event provides secure, synchronous access to clipboard via clipboardData.
   * This works across all browsers and handles both Ctrl+V and Cmd+V.
   */
  private handlePaste(e: ClipboardEvent): void {
    e.preventDefault();
    if (this.editor.isReadOnly()) return;

    const text = e.clipboardData?.getData("text/plain");
    if (!text) return;

    const startByte = this.getCursorByteOffset();
    const hasSelection = this.editor.hasSelection();
    const selectedText = hasSelection ? this.editor.getSelectedText() : "";

    this.editor.insert(text);
    this.trackEdit(startByte, startByte + selectedText.length, startByte + text.length);
    this.editor.ensureCursorVisible();
    this.updateHighlights();
    this.editor.forceRender();
    this.notifyContentChange();
    this.notifySelectionChange();
  }

  /**
   * Handle native copy event - provides consistent behavior across browsers.
   */
  private handleCopy(e: ClipboardEvent): void {
    const text = this.editor.getSelectedText();
    if (text && e.clipboardData) {
      e.preventDefault();
      e.clipboardData.setData("text/plain", text);
    }
  }

  /**
   * Handle native cut event - provides consistent behavior across browsers.
   */
  private handleCut(e: ClipboardEvent): void {
    if (this.editor.isReadOnly()) {
      // In read-only mode, cut behaves like copy
      const text = this.editor.getSelectedText();
      if (text && e.clipboardData) {
        e.preventDefault();
        e.clipboardData.setData("text/plain", text);
      }
      return;
    }

    const text = this.editor.getSelectedText();
    if (text && e.clipboardData) {
      e.preventDefault();
      e.clipboardData.setData("text/plain", text);

      const startByte = this.getCursorByteOffset();
      this.editor.backspace();
      this.trackEdit(startByte, startByte + text.length, startByte);
      this.updateHighlights();
      this.editor.forceRender();
      this.notifyContentChange();
      this.notifySelectionChange();
    }
  }

  /**
   * Update syntax highlights via Web Worker (T031).
   *
   * Sends content to worker for parsing off the main thread.
   * Uses incremental parsing when edit info is available.
   */
  private updateHighlights(): void {
    if (!this.syntaxWorker) return;

    // Cancel any in-flight requests to prevent stale results
    this.syntaxWorker.cancelPending();

    const t0 = performance.now();
    const content = this.editor.getContent();
    const editInfo = this.consumeEditInfo();
    const lineCount = this.editor.getLineCount();

    // Fire off worker request (non-blocking)
    const highlightPromise = lineCount > 200
      ? this.requestRangeHighlight(content, lineCount, editInfo)
      : this.requestFullHighlight(content, editInfo);

    // Handle result when ready (doesn't block main thread)
    highlightPromise.then(({ spans, parseTime }) => {
      const t1 = performance.now();
      this.editor.setTreeSitterHighlights(spans);
      const t2 = performance.now();

      console.log(
        `[Perf] highlights: worker=${parseTime.toFixed(1)}ms (${spans.length} spans), ` +
        `wasm=${(t2-t1).toFixed(1)}ms, total=${(t2-t0).toFixed(1)}ms`
      );
      // Render after highlights are applied
      this.editor.forceRender();
    }).catch((e) => {
      // Ignore cancelled requests (normal during rapid typing)
      if (e.message !== "Cancelled") {
        console.error("[IridiumEditor] Highlight error:", e);
      }
    });
  }

  /**
   * Request viewport-range highlights from worker.
   */
  private async requestRangeHighlight(
    content: string,
    lineCount: number,
    editInfo: EditInfo | null
  ) {
    // Calculate actual visible range from scroll position
    const scrollY = this.editor.getScrollY();
    const lineHeight = this.editor.getLineHeight();
    const viewportHeight = this.canvas.height / (window.devicePixelRatio || 1);

    // Calculate visible line range
    const firstVisibleLine = Math.floor(scrollY / lineHeight);
    const visibleLineCount = Math.ceil(viewportHeight / lineHeight);
    const buffer = 50; // Extra lines for smooth scrolling

    const startLine = Math.max(0, firstVisibleLine - buffer);
    const endLine = Math.min(lineCount, firstVisibleLine + visibleLineCount + buffer);

    return this.syntaxWorker!.highlightRange(content, startLine, endLine, editInfo);
  }

  /**
   * Request full-document highlights from worker.
   */
  private async requestFullHighlight(content: string, editInfo: EditInfo | null) {
    return editInfo
      ? this.syntaxWorker!.highlightIncremental(content, editInfo)
      : this.syntaxWorker!.highlight(content);
  }

  /**
   * Track edit for incremental parsing (T028).
   *
   * Captures the byte offsets and row/column positions for an edit.
   * Called by mutation operations to enable efficient incremental parsing.
   *
   * @param startByte - Byte where edit began
   * @param oldEndByte - Byte where old content ended
   * @param newEndByte - Byte where new content ends
   */
  private trackEdit(
    startByte: number,
    oldEndByte: number,
    newEndByte: number
  ): void {
    const content = this.editor.getContent();

    // Calculate row/column positions from byte offsets (T032)
    const startPos = this.byteToPosition(content, startByte);
    const oldEndPos = this.byteToPosition(content, oldEndByte);
    const newEndPos = this.byteToPosition(content, newEndByte);

    this.lastEditInfo = {
      startIndex: startByte,
      oldEndIndex: oldEndByte,
      newEndIndex: newEndByte,
      startPosition: startPos,
      oldEndPosition: oldEndPos,
      newEndPosition: newEndPos,
    };
  }

  /**
   * Get pending edit info and clear it (T030).
   *
   * Returns the accumulated edit information since the last call,
   * or null if no edits have occurred.
   */
  private consumeEditInfo(): EditInfo | null {
    const edit = this.lastEditInfo;
    this.lastEditInfo = null;
    return edit;
  }

  /**
   * Convert byte offset to row/column position (T032).
   *
   * Tree-sitter requires both byte offsets and row/column positions
   * for incremental parsing. This calculates position from content.
   */
  private byteToPosition(
    content: string,
    byteOffset: number
  ): { row: number; column: number } {
    // Handle out of bounds
    if (byteOffset <= 0) {
      return { row: 0, column: 0 };
    }
    if (byteOffset >= content.length) {
      const lines = content.split("\n");
      const lastLine = lines[lines.length - 1] || "";
      return { row: lines.length - 1, column: lastLine.length };
    }

    // Count newlines up to byteOffset to get row
    const prefix = content.slice(0, byteOffset);
    const lines = prefix.split("\n");
    const row = lines.length - 1;
    const column = lines[row].length;

    return { row, column };
  }

  /**
   * Convert line/column position to byte offset.
   *
   * Used to capture cursor position before edits for incremental parsing.
   */
  private positionToByteOffset(content: string, line: number, column: number): number {
    const lines = content.split("\n");
    let offset = 0;

    for (let i = 0; i < line && i < lines.length; i++) {
      offset += lines[i].length + 1; // +1 for newline
    }

    if (line < lines.length) {
      offset += Math.min(column, lines[line].length);
    }

    return Math.min(offset, content.length);
  }

  /**
   * Get current cursor byte offset in the content.
   */
  private getCursorByteOffset(): number {
    const content = this.editor.getContent();
    const line = this.editor.getCursorLine();
    const column = this.editor.getCursorColumn();
    return this.positionToByteOffset(content, line, column);
  }

  private notifyContentChange(): void {
    if (this.options.onChange) {
      this.options.onChange(this.editor.getContent());
    }
  }

  private notifySelectionChange(): void {
    if (this.options.onSelectionChange) {
      this.options.onSelectionChange({
        line: this.editor.getCursorLine(),
        column: this.editor.getCursorColumn(),
        hasSelection: this.editor.hasSelection(),
      });
    }
  }

  // ============================================================================
  // Public API
  // ============================================================================

  /** Get the current content. */
  getContent(): string {
    return this.editor.getContent();
  }

  /** Set the content. Logs a warning for files >50,000 lines (T039). */
  setContent(content: string): void {
    // T039: Warn about very large files
    const lineCount = (content.match(/\n/g) || []).length + 1;
    if (lineCount > 50_000) {
      console.warn(
        `[IridiumEditor] Large file detected (${lineCount.toLocaleString()} lines). ` +
        `Performance may be degraded. Consider enabling viewport virtualization.`
      );
    }

    this.editor.setContent(content);
    this.updateHighlights();
    this.editor.forceRender();
  }

  /**
   * Apply non-overlapping byte-range replacements as one undo unit.
   * Ranges refer to the content before the edit. Selection is mapped through
   * the replacements and the pixel scroll offset is preserved.
   */
  applyTextDelta(replacements: TextReplacement[]): void {
    this.editor.applyTextDelta(replacements);
    this.updateHighlights();
    this.editor.forceRender();
    this.notifyContentChange();
    this.notifySelectionChange();
  }

  /** Get the current editor state. */
  getState(): EditorState {
    return {
      line: this.editor.getCursorLine(),
      column: this.editor.getCursorColumn(),
      lineCount: this.editor.getLineCount(),
      hasSelection: this.editor.hasSelection(),
      canUndo: this.editor.canUndo(),
      canRedo: this.editor.canRedo(),
      language: this.currentLanguage,
      foldedLines: this.editor.getFoldedLines(),
      hiddenLineCount: this.editor.getHiddenLineCount(),
    };
  }

  /** Set the language for syntax highlighting. */
  async setLanguage(language: string): Promise<boolean> {
    if (!this.syntaxWorker) return false;
    await this.syntaxWorker.setLanguage(language);
    this.currentLanguage = language;
    this.updateHighlights();
    this.editor.forceRender();
    return true;
  }

  /** Get available languages for syntax highlighting. */
  getAvailableLanguages(): string[] {
    return this.syntaxWorker?.getAvailableLanguages() ?? [];
  }

  /** Set the theme. */
  setTheme(dark: boolean): void {
    this.editor.setDarkTheme(dark);
    this.editor.forceRender();
  }

  /** Undo the last change. */
  undo(): boolean {
    const result = this.editor.undo();
    this.updateHighlights();
    this.editor.forceRender();
    return result;
  }

  /** Redo the last undone change. */
  redo(): boolean {
    const result = this.editor.redo();
    this.updateHighlights();
    this.editor.forceRender();
    return result;
  }

  /** Fold all foldable regions. */
  foldAll(): void {
    this.editor.foldAll();
    this.editor.forceRender();
  }

  /** Unfold all folded regions. */
  unfoldAll(): void {
    this.editor.unfoldAll();
    this.editor.forceRender();
  }

  /** Toggle fold at a specific line. */
  toggleFold(line: number): boolean {
    const result = this.editor.toggleFold(line);
    this.editor.forceRender();
    return result;
  }

  /** Toggle fold at the current cursor line. */
  toggleFoldAtCursor(): boolean {
    return this.toggleFold(this.editor.getCursorLine());
  }

  /** Enable or disable the gutter (line numbers). */
  setGutterEnabled(enabled: boolean): void {
    this.editor.setGutterEnabled(enabled);
    this.editor.forceRender();
  }

  /** Enable or disable syntax highlighting. */
  setSyntaxEnabled(enabled: boolean): void {
    this.editor.setSyntaxEnabled(enabled);
    this.editor.forceRender();
  }

  /**
   * Set the syntax color theme.
   *
   * Accepts a mapping of capture names to hex colors. The editor uses
   * hierarchical lookup, so `keyword` covers `keyword.control`, etc.
   *
   * @param theme Record mapping capture names to hex colors (e.g., `{ keyword: "#81A1C1" }`)
   */
  setSyntaxTheme(theme: Record<string, string>): void {
    this.editor.setSyntaxTheme(theme);
    this.editor.forceRender();
  }

  /**
   * Set syntax highlight spans directly (for Meridian server integration).
   *
   * Use this API to provide syntax spans from an external source (e.g., Meridian server)
   * instead of using the built-in web-tree-sitter worker.
   *
   * @param spans Array of {start, end, type} where start/end are byte offsets
   */
  setHighlightSpans(spans: { start: number; end: number; type: string }[]): void {
    this.editor.setTreeSitterHighlights(spans);
    this.editor.forceRender();
  }

  /** Configure host-defined color classes used by underline decorations. */
  setUnderlineTheme(theme: Record<string, string>): void {
    this.editor.setUnderlineTheme(theme);
    this.editor.forceRender();
  }

  /**
   * Replace all underline decorations. Byte ranges refer to current content.
   * Every colorClass must exist in the underline theme.
   */
  setUnderlineDecorations(decorations: UnderlineDecoration[]): void {
    this.editor.setUnderlineDecorations(decorations);
    this.editor.forceRender();
  }

  /** Clear all underline decorations. */
  clearUnderlineDecorations(): void {
    this.editor.setUnderlineDecorations([]);
    this.editor.forceRender();
  }

  /** Focus the editor canvas. */
  focus(): void {
    this.canvas.focus();
  }

  // ============================================================================
  // Git integration: read-only, line backgrounds, gutter changes, blame
  // ============================================================================

  /** Set read-only mode. When read-only, all mutations are silently ignored. */
  setReadOnly(readOnly: boolean): void {
    this.editor.setReadOnly(readOnly);
    this.editor.forceRender();
  }

  /** Returns whether the editor is in read-only mode. */
  isReadOnly(): boolean {
    return this.editor.isReadOnly();
  }

  /**
   * Set per-line background colors (for diff highlighting).
   * Each entry: `{line: 0-indexed doc line, color: hex string}`.
   */
  setLineBackgrounds(backgrounds: Array<{ line: number; color: string }>): void {
    this.editor.setLineBackgrounds(backgrounds);
    this.editor.forceRender();
  }

  /** Clear all per-line background colors. */
  clearLineBackgrounds(): void {
    this.editor.clearLineBackgrounds();
    this.editor.forceRender();
  }

  /**
   * Set gutter change indicators (thin colored bars).
   * Each entry: `{line: 0-indexed doc line, kind: "added"|"modified"|"deleted"}`.
   */
  setGutterChanges(changes: Array<{ line: number; kind: string }>): void {
    this.editor.setGutterChanges(changes);
    this.editor.forceRender();
  }

  /** Clear all gutter change indicators. */
  clearGutterChanges(): void {
    this.editor.clearGutterChanges();
    this.editor.forceRender();
  }

  /**
   * Set custom gutter text, replacing automatic line numbers.
   * Pass an array of strings (one per doc line) or null to restore auto numbering.
   */
  setCustomGutterText(lines: string[] | null): void {
    this.editor.setCustomGutterText(lines);
    this.editor.forceRender();
  }

  /**
   * Set per-line blame data for inline ghost text.
   * Only the cursor line's blame text is rendered (after line content).
   * Each entry: `{line: 0-indexed doc line, text: "Author · 3d ago · Summary"}`.
   */
  setBlameData(data: Array<{ line: number; text: string }>): void {
    this.editor.setBlameData(data);
    this.editor.forceRender();
  }

  /** Clear all blame data. */
  clearBlameData(): void {
    this.editor.clearBlameData();
    this.editor.forceRender();
  }

  /** Destroy the editor and clean up resources. */
  destroy(): void {
    if (this.destroyed) return;
    this.destroyed = true;

    // Remove from tracking map
    initializedCanvases.delete(this.canvas);

    // Stop render loop
    if (this.animationFrameId) {
      cancelAnimationFrame(this.animationFrameId);
    }

    // Terminate syntax worker
    if (this.syntaxWorker) {
      this.syntaxWorker.dispose();
      this.syntaxWorker = null;
    }

    // Remove event listeners
    for (const cleanup of this.eventCleanup) {
      cleanup();
    }
    this.eventCleanup = [];
  }
}

export default IridiumEditor;
