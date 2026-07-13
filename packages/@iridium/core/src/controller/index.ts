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

// Import syntax worker client
import { SyntaxHighlightClient, type EditInfo } from "../worker/client.ts";

/**
 * Rust-computed edit span for incremental tree-sitter parsing.
 *
 * Mirrors the `JsEditInfo` wasm-bindgen class: byte offsets and byte-column
 * points computed from the rope, never from JavaScript strings.
 */
interface WasmEditInfo {
  readonly startByte: number;
  readonly oldEndByte: number;
  readonly newEndByte: number;
  readonly startRow: number;
  readonly startColumn: number;
  readonly oldEndRow: number;
  readonly oldEndColumn: number;
  readonly newEndRow: number;
  readonly newEndColumn: number;
  free(): void;
}

/**
 * Action tags returned by `WebEditor.handleKeyEvent`.
 *
 * - `handled`      — key consumed, no content change
 * - `handled:edit` — key consumed, document changed (fetch `takeLastEdit`)
 * - `copy` / `cut` — clipboard text pending in `getPendingClipboardText`
 * - `search:*`     — search UI actions requested by the core
 * - `ignored`      — leave the event to the browser
 */
type KeyEventAction =
  | "handled"
  | "handled:edit"
  | "copy"
  | "cut"
  | "search:open"
  | "search:next"
  | "search:prev"
  | "search:close"
  | "ignored";

/** Search UI actions surfaced through {@link IridiumEditorOptions.onSearchAction}. */
export type SearchAction = "open" | "next" | "prev" | "close";

// Types for the low-level WASM editor
interface WebEditor {
  handleKeyEvent(key: string, ctrl: boolean, shift: boolean, alt: boolean, meta: boolean): KeyEventAction;
  takeLastEdit(): WasmEditInfo | undefined;
  getPendingClipboardText(): string | undefined;
  copyText(): string | undefined;
  cutText(): string | undefined;
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
  deleteToLineStart(): boolean;
  deleteToLineEnd(): boolean;
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
  getCharWidth(): number;
  getTextOffsetX(): number;
  getTextOffsetY(): number;
  positionToPixel(line: number, column: number): number[];
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
  /** Callback when mouse hovers over a position (debounced 400ms). Null = mouse left. */
  onMouseHover?: (info: { line: number; column: number; clientX: number; clientY: number } | null) => void;
  /** Callback when editor scrolls (wheel or cursor movement). */
  onScroll?: () => void;
  /** Intercept keydown before editor processes it. Return true to consume the event. */
  onBeforeKeyDown?: (e: KeyboardEvent) => boolean;
  /**
   * Callback for search UI actions requested by the editor core
   * (Ctrl/Cmd+F opens, F3/Shift+F3 navigate matches — navigation is already
   * applied to the editor's search state when this fires).
   */
  onSearchAction?: (action: SearchAction) => void;
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
  private options: Required<Omit<IridiumEditorOptions, "onChange" | "onSelectionChange" | "onMouseHover" | "onScroll" | "onBeforeKeyDown" | "onSearchAction">> & Pick<IridiumEditorOptions, "onChange" | "onSelectionChange" | "onMouseHover" | "onScroll" | "onBeforeKeyDown" | "onSearchAction">;
  private currentLanguage: string;
  private isDragging = false;
  private animationFrameId = 0;
  private lastBlinkTime = 0;
  private eventCleanup: (() => void)[] = [];
  private destroyed = false;
  private hoverTimer: ReturnType<typeof setTimeout> | null = null;
  private lastHoverLine = -1;
  private lastHoverColumn = -1;
  /**
   * macOS/iOS detection for the keyboard translation layer (see
   * {@link translateKeyEvent}). User-agent sniffing is the established
   * mechanism here; `navigator.userAgentData` is not yet universal.
   */
  private readonly isMacPlatform: boolean =
    typeof navigator !== "undefined" && /Mac|iPhone|iPad|iPod/i.test(navigator.userAgent);

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
      onMouseHover: options.onMouseHover,
      onScroll: options.onScroll,
      onBeforeKeyDown: options.onBeforeKeyDown,
      onSearchAction: options.onSearchAction,
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

    // Mouse leave — cancel hover
    const handleMouseLeave = (): void => {
      this.resetHoverTimer();
      this.options.onMouseHover?.(null);
    };
    this.canvas.addEventListener("mouseleave", handleMouseLeave);
    this.eventCleanup.push(() => this.canvas.removeEventListener("mouseleave", handleMouseLeave));

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

  private resetHoverTimer(): void {
    if (this.hoverTimer) {
      clearTimeout(this.hoverTimer);
      this.hoverTimer = null;
    }
    this.lastHoverLine = -1;
    this.lastHoverColumn = -1;
  }

  private handleMouseDown(e: MouseEvent): void {
    this.resetHoverTimer();
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
    if (this.isDragging) {
      const [line, column] = this.getMousePosition(e);
      this.editor.extendSelectionToPosition(line, column);
      this.editor.forceRender();
      this.notifySelectionChange();
      return;
    }
    // Hover detection (only when not dragging)
    if (this.options.onMouseHover) {
      this.resetHoverTimer();
      const clientX = e.clientX;
      const clientY = e.clientY;
      this.hoverTimer = setTimeout(() => {
        const [line, column] = this.getMousePosition(e);
        if (line !== this.lastHoverLine || column !== this.lastHoverColumn) {
          this.lastHoverLine = line;
          this.lastHoverColumn = column;
          this.options.onMouseHover?.({ line, column, clientX, clientY });
        }
      }, 400);
    }
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
    this.resetHoverTimer();
    this.options.onScroll?.();
  }

  /**
   * Keyboard entry point: forwards raw key events to the Rust core, which
   * owns all editing behavior (multi-cursor edits, Tab/indent/outdent,
   * Enter auto-indent with bracket-block and code-fence expansion,
   * auto-pairs, sticky columns, undo/redo, search keys).
   *
   * The controller keeps four responsibilities here:
   * 1. the `onBeforeKeyDown` interception hook,
   * 2. native clipboard passthrough (Cmd/Ctrl+C/X/V must not be
   *    preventDefaulted so the browser fires copy/cut/paste events —
   *    required for Safari; the copy/cut *semantics* still come from the
   *    Rust core inside those event handlers),
   * 3. the IME composition guard, and
   * 4. platform key translation (see {@link translateKeyEvent}).
   */
  private handleKeyDown(e: KeyboardEvent): void {
    // (a) Host interception hook.
    if (this.options.onBeforeKeyDown?.(e)) {
      e.preventDefault();
      return;
    }
    // Cancel hover on any keypress.
    this.resetHoverTimer();

    // (b) Native clipboard passthrough: return WITHOUT preventDefault so the
    // browser fires the native copy/cut/paste events, which the handlers
    // below (handleCopy/handleCut/handlePaste) service. Safari only exposes
    // clipboard data through those events. The handlers drive the Rust
    // multi-cursor clipboard paths (copyText/cutText/insert), so behavior
    // is identical to the keyboard path — only the trigger is native.
    const primaryModifier = e.metaKey || e.ctrlKey;
    if (primaryModifier && !e.altKey) {
      const k = e.key.toLowerCase();
      if (k === "c" || k === "x" || k === "v") {
        return;
      }
    }

    // (c) IME composition guard: while composing, the IME owns the key
    // stream. NOTE: full web IME does not work yet — the canvas is not an
    // editable element, so browsers never run an IME against it and no
    // composition/beforeinput/input events fire. Supporting IME requires
    // the hidden-editable-element pattern, which is tracked in
    // docs/PLAN.md. This guard is kept so that, if a host embeds the
    // editor behind such an element, in-flight compositions are never
    // double-handled; committed text can be fed through
    // {@link insertText}.
    if (e.isComposing) {
      return;
    }

    // (d) macOS Cmd+Backspace/Delete map to delete-to-line-start/end,
    // which the Rust keyboard handler has no key for; they go through the
    // discrete WASM methods (track_and_apply inside: edit tracking,
    // history, read-only gating all hold). See the mapping table in
    // translateKeyEvent.
    if (this.isMacPlatform && e.metaKey && !e.ctrlKey && !e.altKey &&
        (e.key === "Backspace" || e.key === "Delete")) {
      e.preventDefault();
      const changed = e.key === "Backspace"
        ? this.editor.deleteToLineStart()
        : this.editor.deleteToLineEnd();
      this.editor.ensureCursorVisible();
      this.editor.forceRender();
      if (changed) {
        this.updateHighlights(this.takeEditInfo());
        this.notifyContentChange();
      }
      this.notifySelectionChange();
      this.options.onScroll?.();
      return;
    }

    // (e) Translate the DOM event into the Rust core's key vocabulary and
    // forward it (see the mapping table in translateKeyEvent).
    const t = this.translateKeyEvent(e);
    const action = this.editor.handleKeyEvent(t.key, t.ctrl, t.shift, t.alt, t.meta);

    if (action === "ignored") {
      // The editor does not handle this key; leave it to the browser.
      return;
    }

    e.preventDefault();

    if (action === "copy" || action === "cut") {
      // Defensive path for synthesized key events: real Cmd/Ctrl+C/X went
      // through the native clipboard passthrough above.
      const text = this.editor.getPendingClipboardText();
      if (text) {
        void this.writeClipboardText(text);
      }
    } else if (action.startsWith("search:")) {
      this.options.onSearchAction?.(action.slice("search:".length) as SearchAction);
    }

    const contentChanged = action === "handled:edit" || action === "cut";
    this.editor.ensureCursorVisible();
    // Render immediately for responsive typing feedback
    this.editor.forceRender();
    // Fire async worker request - will re-render with updated highlights
    // when ready. The edit span comes from Rust (takeLastEdit), not from
    // JS byte math.
    this.updateHighlights(contentChanged ? this.takeEditInfo() : null);
    if (contentChanged) {
      this.notifyContentChange();
    }
    this.notifySelectionChange();
    this.options.onScroll?.();
  }

  /**
   * Translates a DOM keyboard event into the Rust core's key vocabulary.
   *
   * The Rust handler is platform-agnostic: its `ctrl` modifier means
   * "word motion / editor shortcut" (word-left on Ctrl+Arrow, word delete
   * on Ctrl+Backspace, shortcuts on Ctrl+letter, document start/end on
   * Ctrl+Home/End). macOS distributes those semantics across Option and
   * Command, so on macOS this layer translates *semantically* before
   * forwarding. The proper home for platform keymaps is the future keymap
   * layer (decision D1 in docs/PLAN.md §7); this is the binding-level
   * translation until that lands.
   *
   * macOS mapping table (shift is always preserved):
   *
   * | DOM chord                  | Forwarded to Rust        | Semantics            |
   * |----------------------------|--------------------------|----------------------|
   * | Option+ArrowLeft/Right     | ctrl+ArrowLeft/Right     | word left/right      |
   * | Option+Backspace/Delete    | ctrl+Backspace/Delete    | word delete          |
   * | Option+ArrowUp/Down        | alt+ArrowUp/Down (as-is) | move line up/down    |
   * | Cmd+ArrowLeft/Right        | Home/End                 | line start/end       |
   * | Cmd+ArrowUp/Down           | ctrl+Home/End            | document start/end   |
   * | Cmd+Backspace/Delete       | (handled in handleKeyDown via the discrete |
   * |                            | deleteToLineStart/End WASM methods)         |
   * | Cmd+A/Z/Shift+Z/C/X/V/D/F/L| ctrl+letter              | editor shortcuts     |
   * | plain Ctrl+key             | ctrl+key (as-is)         | see note below       |
   * | anything else              | Cmd→ctrl, meta dropped   | legacy default       |
   *
   * Notes:
   * - Plain Control-key combos on macOS (terminal-style Ctrl+A etc.) are
   *   deliberately NOT translated to macOS emacs-style bindings; they
   *   forward with `ctrl` set and hit the Rust editor shortcuts. Remapping
   *   them is keymap-layer work (D1).
   * - Compound chords (Cmd+Option+…, Cmd+Ctrl+…) are not translated; they
   *   forward through the default mapping, where the Rust core ignores
   *   Ctrl+Alt/Ctrl+Meta character chords (AltGr safety).
   * - `meta` is never forwarded on macOS, so unbound Cmd combos fall
   *   through to the browser instead of self-inserting.
   *
   * On all other platforms the four DOM modifier flags map 1:1.
   */
  private translateKeyEvent(
    e: KeyboardEvent
  ): { key: string; ctrl: boolean; shift: boolean; alt: boolean; meta: boolean } {
    if (!this.isMacPlatform) {
      return { key: e.key, ctrl: e.ctrlKey, shift: e.shiftKey, alt: e.altKey, meta: e.metaKey };
    }

    // Option-only chords: word motions and word deletes.
    if (e.altKey && !e.metaKey && !e.ctrlKey) {
      switch (e.key) {
        case "ArrowLeft":
        case "ArrowRight":
        case "Backspace":
        case "Delete":
          return { key: e.key, ctrl: true, shift: e.shiftKey, alt: false, meta: false };
        default:
          break;
      }
    }

    // Command-only chords: line/document motions.
    if (e.metaKey && !e.altKey && !e.ctrlKey) {
      switch (e.key) {
        case "ArrowLeft":
          return { key: "Home", ctrl: false, shift: e.shiftKey, alt: false, meta: false };
        case "ArrowRight":
          return { key: "End", ctrl: false, shift: e.shiftKey, alt: false, meta: false };
        case "ArrowUp":
          return { key: "Home", ctrl: true, shift: e.shiftKey, alt: false, meta: false };
        case "ArrowDown":
          return { key: "End", ctrl: true, shift: e.shiftKey, alt: false, meta: false };
        default:
          break;
      }
    }

    // Alt-chorded letters: macOS resolves Option+letter to a special
    // character (Shift+Option+A -> "Å"), so the logical e.key would never
    // match a core binding like Shift+Alt+A (toggle block comment).
    // Normalize to the physical base letter via e.code — the Rust core's
    // contract is base logical letters plus modifiers. This does not affect
    // typing special characters: composed input (Option+e etc.) belongs to
    // the composition/IME path, not keydown dispatch.
    if (e.altKey) {
      const physical = /^Key([A-Z])$/.exec(e.code);
      if (physical) {
        const base = e.shiftKey ? physical[1] : physical[1].toLowerCase();
        return {
          key: base,
          ctrl: e.metaKey || e.ctrlKey,
          shift: e.shiftKey,
          alt: true,
          meta: false,
        };
      }
    }

    // Default macOS mapping: Cmd and Ctrl both forward as the Rust `ctrl`
    // (editor shortcuts), `meta` is never forwarded.
    return {
      key: e.key,
      ctrl: e.metaKey || e.ctrlKey,
      shift: e.shiftKey,
      alt: e.altKey,
      meta: false,
    };
  }

  /**
   * Write text to the system clipboard (used only for the defensive
   * `copy`/`cut` key-result path; real clipboard shortcuts go through the
   * native events).
   */
  private async writeClipboardText(text: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
    } catch (err) {
      console.warn("[IridiumEditor] Clipboard write failed:", err);
    }
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

    // insert() replaces any selection and records the edit span in Rust.
    this.editor.insert(text);
    this.editor.ensureCursorVisible();
    this.updateHighlights(this.takeEditInfo());
    this.editor.forceRender();
    this.notifyContentChange();
    this.notifySelectionChange();
  }

  /**
   * Handle native copy event (the trigger stays native for Safari, which
   * only exposes clipboard writes through this event).
   *
   * The text comes from the Rust core's multi-cursor copy path
   * (`copyText`), so native copy is identical to the keyboard path: all
   * cursors' selections joined with the document line ending, and with
   * only collapsed cursors each cursor's whole line. Copy never mutates.
   */
  private handleCopy(e: ClipboardEvent): void {
    if (!e.clipboardData) {
      // No synchronous clipboard access — nothing this handler can do.
      return;
    }
    const text = this.editor.copyText();
    if (text === undefined) {
      return;
    }
    e.preventDefault();
    e.clipboardData.setData("text/plain", text);
  }

  /**
   * Handle native cut event (the trigger stays native for Safari; see
   * {@link handleCopy}).
   *
   * `cutText()` drives the Rust core's multi-cursor cut: it returns the
   * clipboard text and applies the deletion through the tracked command
   * path (edit span for incremental parsing, history, fold refresh). In
   * read-only mode it returns undefined and the cut is swallowed entirely
   * — matching the keyboard path, where read-only cut does not even copy.
   */
  private handleCut(e: ClipboardEvent): void {
    // The semantics come from Rust in every case (including the read-only
    // swallow), so the browser default never runs.
    e.preventDefault();
    if (!e.clipboardData) {
      // No synchronous clipboard access: do NOT mutate the document, or
      // the cut text would be lost.
      return;
    }
    const text = this.editor.cutText();
    if (text === undefined) {
      // Read-only: swallowed.
      return;
    }
    e.clipboardData.setData("text/plain", text);
    this.editor.forceRender();
    this.updateHighlights(this.takeEditInfo());
    this.notifyContentChange();
    this.notifySelectionChange();
  }

  /**
   * Update syntax highlights via Web Worker (T031).
   *
   * Sends content to worker for parsing off the main thread.
   * Uses incremental parsing when Rust-provided edit info is available
   * (see {@link takeEditInfo}).
   */
  private updateHighlights(editInfo: EditInfo | null = null): void {
    if (!this.syntaxWorker) return;

    // Cancel any in-flight requests to prevent stale results
    this.syntaxWorker.cancelPending();

    const t0 = performance.now();
    const content = this.editor.getContent();
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
   * Consume the Rust-recorded edit span for incremental parsing.
   *
   * The span is computed from the rope inside the WASM editor (byte
   * offsets and byte-column points) — the controller no longer derives
   * byte offsets from UTF-16 JavaScript strings. Returns null when no
   * content changed since the last call, or when the accumulated edits
   * cannot be described exactly (the worker then does a full parse).
   */
  private takeEditInfo(): EditInfo | null {
    const info = this.editor.takeLastEdit();
    if (!info) return null;
    const edit: EditInfo = {
      startIndex: info.startByte,
      oldEndIndex: info.oldEndByte,
      newEndIndex: info.newEndByte,
      startPosition: { row: info.startRow, column: info.startColumn },
      oldEndPosition: { row: info.oldEndRow, column: info.oldEndColumn },
      newEndPosition: { row: info.newEndRow, column: info.newEndColumn },
    };
    // JsEditInfo is a wasm-bindgen object; free it deterministically.
    info.free();
    return edit;
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
    this.updateHighlights(result ? this.takeEditInfo() : null);
    this.editor.forceRender();
    return result;
  }

  /** Redo the last undone change. */
  redo(): boolean {
    const result = this.editor.redo();
    this.updateHighlights(result ? this.takeEditInfo() : null);
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

  // ============================================================================
  // Layout metrics and position conversion
  // ============================================================================

  /** Get layout metrics for position calculations. */
  getLayoutMetrics(): { lineHeight: number; charWidth: number; scrollY: number; textOffsetX: number; textOffsetY: number } {
    return {
      lineHeight: this.editor.getLineHeight(),
      charWidth: this.editor.getCharWidth(),
      scrollY: this.editor.getScrollY(),
      textOffsetX: this.editor.getTextOffsetX(),
      textOffsetY: this.editor.getTextOffsetY(),
    };
  }

  /**
   * Convert a document position to pixel coordinates.
   * Returns null if the position is off-screen or on a folded line.
   * Coordinates are in physical pixels (caller divides by devicePixelRatio).
   */
  positionToPixel(line: number, column: number): { x: number; y: number } | null {
    const result = this.editor.positionToPixel(line, column);
    if (result[0] < 0) return null;
    return { x: result[0], y: result[1] };
  }

  /**
   * Insert text at every cursor, replacing any selections.
   *
   * Public seam for hosts and the future IME layer (composition commits;
   * see docs/PLAN.md): the insertion goes through the Rust core's
   * multi-cursor paste path as one reversible command, with the edit span
   * recorded for incremental parsing.
   *
   * No-op (no callbacks fired) for empty text or in read-only mode.
   */
  insertText(text: string): void {
    if (text.length === 0 || this.editor.isReadOnly()) return;

    this.editor.insert(text);
    this.editor.ensureCursorVisible();
    this.editor.forceRender();
    this.updateHighlights(this.takeEditInfo());
    this.notifyContentChange();
    this.notifySelectionChange();
  }

  /**
   * Apply a completion: delete N characters before the (collapsed) cursor,
   * then insert text.
   *
   * The N characters are selected and replaced: with non-empty text via a
   * single `insert()` (one reversible command), with empty text via the
   * selection-delete path — both record the edit span in Rust (consumed
   * via `takeLastEdit` for incremental parsing).
   *
   * Callbacks fire only when content actually changes: an entirely empty
   * completion, a read-only editor, or an empty-text completion with
   * nothing deletable (cursor at document start) return without firing.
   */
  applyCompletion(deleteCount: number, text: string): void {
    if (this.editor.isReadOnly()) return;
    if (deleteCount <= 0 && text.length === 0) return;

    // Select the characters to replace.
    for (let i = 0; i < deleteCount; i++) {
      this.editor.extendSelectionLeft();
    }

    if (text.length > 0) {
      // insert() replaces the selection (and is a content change even when
      // nothing was selected).
      this.editor.insert(text);
    } else if (this.editor.hasSelection()) {
      // Empty text: delete the selection through the tracked delete path.
      this.editor.backspace();
    } else {
      // Empty text and nothing became selected (cursor at document start):
      // no content change, no callbacks. The no-op selection extension
      // left no state behind.
      return;
    }

    this.editor.ensureCursorVisible();
    this.updateHighlights(this.takeEditInfo());
    this.editor.forceRender();
    this.notifyContentChange();
    this.notifySelectionChange();
  }

  /** Destroy the editor and clean up resources. */
  destroy(): void {
    if (this.destroyed) return;
    this.destroyed = true;
    this.resetHoverTimer();

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
