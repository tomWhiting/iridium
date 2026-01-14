/**
 * IridiumEditor - High-level wrapper for the Iridium WebGPU editor.
 *
 * This controller handles all the boilerplate:
 * - WASM initialization
 * - Event handling (keyboard, mouse, clipboard)
 * - Syntax highlighting integration
 * - Render loop
 *
 * Usage:
 * ```typescript
 * import { IridiumEditor } from 'iridium-bindings';
 *
 * const editor = await IridiumEditor.create(canvas, {
 *   language: 'rust',
 *   theme: 'dark',
 * });
 * ```
 */

import { getSyntax, type SyntaxHighlighter } from "../syntax/index.ts";

// Types for the low-level WASM editor
interface WebEditor {
  loadFont(data: Uint8Array): void;
  setContent(content: string): void;
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
  isGutterEnabled(): boolean;
  setGutterEnabled(enabled: boolean): void;
  isTreeSitterActive(): boolean;
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
  private syntax: SyntaxHighlighter | null = null;
  private options: Required<Omit<IridiumEditorOptions, "onChange" | "onSelectionChange">> & Pick<IridiumEditorOptions, "onChange" | "onSelectionChange">;
  private currentLanguage: string;
  private isDragging = false;
  private animationFrameId = 0;
  private lastBlinkTime = 0;
  private eventCleanup: (() => void)[] = [];
  private destroyed = false;
  private highlightTimeout: ReturnType<typeof setTimeout> | null = null;
  private static readonly HIGHLIGHT_DEBOUNCE_MS = 50;

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
    const wasmUrl = new URL("../../pkg/iridium_bindings.js", import.meta.url).href;
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

    // Initialize syntax highlighting
    try {
      this.syntax = await getSyntax();
      await this.syntax.setLanguage(this.options.language);
    } catch (e) {
      console.warn("[IridiumEditor] Syntax highlighting not available:", e);
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
    this.editor.forceRender();
  }

  private handleKeyDown(e: KeyboardEvent): void {
    let handled = true;
    const isMac = /Mac|iPhone|iPad|iPod/i.test(navigator.userAgent);
    const selecting = e.shiftKey;

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
      this.handleBackspace(e, isMac);
    } else if (e.key === "Delete") {
      this.handleDelete(e, isMac);
    } else if (e.key === "k" && e.ctrlKey) {
      this.editor.deleteToLineEnd();
      this.notifyContentChange();
    } else if (e.key === "Enter") {
      this.handleEnter();
    } else if (e.key === "Tab") {
      this.editor.insert("    ");
      this.notifyContentChange();
    } else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey) {
      this.handleCharacterInput(e.key);
    } else if (e.metaKey || e.ctrlKey) {
      handled = this.handleShortcut(e);
    } else {
      handled = false;
    }

    if (handled) {
      e.preventDefault();
      this.editor.ensureCursorVisible();
      this.updateHighlights();
      this.editor.forceRender();
      this.notifySelectionChange();
    }
  }

  private handleBackspace(e: KeyboardEvent, isMac: boolean): void {
    if (e.metaKey && isMac) {
      this.editor.deleteToLineStart();
    } else if (e.altKey || (e.ctrlKey && !isMac)) {
      this.editor.deleteWordBackward();
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
        this.editor.backspace();
        this.editor.delete_forward();
      } else {
        this.editor.backspace();
      }
    }
    this.notifyContentChange();
  }

  private handleDelete(e: KeyboardEvent, isMac: boolean): void {
    if (e.altKey || (e.ctrlKey && !isMac)) {
      this.editor.deleteWordForward();
    } else {
      this.editor.delete_forward();
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

    // Check for code block (triple backticks)
    const codeBlockMatch = textBeforeCursor.match(/^(\s*)```(\w*)$/);
    if (codeBlockMatch) {
      const blockIndent = codeBlockMatch[1];
      this.editor.insert("\n" + blockIndent + "\n" + blockIndent + "```");
      this.editor.moveCursorUp();
      this.editor.moveCursorLineEnd();
    }
    // Check for bracket pairs
    else if (
      (charBefore === "{" && charAfter === "}") ||
      (charBefore === "[" && charAfter === "]") ||
      (charBefore === "(" && charAfter === ")")
    ) {
      this.editor.insert("\n" + indent + "    \n" + indent);
      this.editor.moveCursorUp();
      this.editor.moveCursorLineEnd();
    } else {
      // Regular enter with indent preservation
      const endsWithOpener = /[{(\[]$/.test(textBeforeCursor.trim());
      if (endsWithOpener) {
        this.editor.insert("\n" + indent + "    ");
      } else {
        this.editor.insert("\n" + indent);
      }
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
      this.editor.insert(char + PAIRS[char]);
      this.editor.moveCursorLeft();
    } else {
      this.editor.insert(char);
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
    } else if (e.key === "c") {
      const text = this.editor.getSelectedText();
      if (text) {
        navigator.clipboard.writeText(text).catch(console.error);
      }
      return true;
    } else if (e.key === "x") {
      const text = this.editor.getSelectedText();
      if (text) {
        navigator.clipboard.writeText(text).then(() => {
          this.editor.backspace();
          this.updateHighlights();
          this.editor.forceRender();
          this.notifyContentChange();
        }).catch(console.error);
      }
      return true;
    } else if (e.key === "v") {
      navigator.clipboard.readText().then((text) => {
        if (text) {
          this.editor.insert(text);
          this.updateHighlights();
          this.editor.forceRender();
          this.notifyContentChange();
        }
      }).catch(console.error);
      return true;
    }
    return false;
  }

  private updateHighlights(): void {
    if (!this.syntax) return;
    try {
      const content = this.editor.getContent();
      const spans = this.syntax.highlight(content);
      this.editor.setTreeSitterHighlights(spans);
    } catch (e) {
      console.error("[IridiumEditor] Failed to update highlights:", e);
    }
  }

  /**
   * Schedule a debounced highlight update.
   * Renders immediately, then updates syntax highlighting after a short delay.
   * This keeps typing responsive while syntax catches up.
   */
  private scheduleHighlightUpdate(): void {
    if (this.highlightTimeout) {
      clearTimeout(this.highlightTimeout);
    }
    this.highlightTimeout = setTimeout(() => {
      this.updateHighlights();
      this.editor.forceRender();
      this.highlightTimeout = null;
    }, IridiumEditor.HIGHLIGHT_DEBOUNCE_MS);
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

  /** Set the content. */
  setContent(content: string): void {
    this.editor.setContent(content);
    this.updateHighlights();
    this.editor.forceRender();
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
    if (!this.syntax) return false;
    const success = await this.syntax.setLanguage(language);
    if (success) {
      this.currentLanguage = language;
      this.updateHighlights();
      this.editor.forceRender();
    }
    return success;
  }

  /** Get available languages for syntax highlighting. */
  getAvailableLanguages(): string[] {
    return this.syntax?.getAvailableLanguages() ?? [];
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

  /** Focus the editor canvas. */
  focus(): void {
    this.canvas.focus();
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

    // Cancel pending highlight update
    if (this.highlightTimeout) {
      clearTimeout(this.highlightTimeout);
    }

    // Remove event listeners
    for (const cleanup of this.eventCleanup) {
      cleanup();
    }
    this.eventCleanup = [];
  }
}

export default IridiumEditor;
