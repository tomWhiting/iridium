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
 * Action tags returned by `WebEditor.handleKeyEvent` and `WebEditor.runCommand`.
 *
 * - `handled`      — key consumed, no content change
 * - `handled:edit` — key consumed, document changed (fetch `takeLastEdit`)
 * - `handled:command` — key consumed, resolved to a host command (fetch
 *   `takePendingHostCommand`)
 * - `copy` / `cut` — clipboard text pending in `getPendingClipboardText`
 * - `search:*`     — search UI actions requested by the core
 * - `ignored`      — leave the event to the browser
 *
 * ⚠️ The wasm boundary types both methods as `string`, because Rust returns
 * `String`; the closed set lives in `crates/iridium-bindings/src/wasm.rs`.
 * This tuple is the TypeScript half of that contract and the only place the
 * tags are written down here — {@link KeyEventAction} is derived from it and
 * {@link asKeyEventAction} checks against it, so a tag the core starts
 * emitting that this build does not know is reported rather than mistaken for
 * one that it does.
 */
const KEY_EVENT_ACTIONS = [
  "handled",
  "handled:edit",
  "handled:command",
  "copy",
  "cut",
  "search:open",
  "search:next",
  "search:prev",
  "search:close",
  "ignored",
] as const;

/** One of {@link KEY_EVENT_ACTIONS}. */
type KeyEventAction = (typeof KEY_EVENT_ACTIONS)[number];

/**
 * Narrows a tag the wasm boundary hands back as a bare `string`.
 *
 * An unrecognised tag means the core gained an action this build of the
 * TypeScript layer does not know about. It is reported rather than swallowed,
 * and treated as `"handled"`: the core returns `"ignored"` only for a key it
 * did not consume, so anything else was consumed, and letting the browser act
 * on it as well would apply the keypress twice.
 */
function asKeyEventAction(tag: string): KeyEventAction {
  if ((KEY_EVENT_ACTIONS as readonly string[]).includes(tag)) {
    return tag as KeyEventAction;
  }
  console.error(
    `[IridiumEditor] Unknown action tag "${tag}" from the editor core — add it ` +
      `to KEY_EVENT_ACTIONS in @iridium-editor/core. Treating it as "handled".`,
  );
  return "handled";
}

/** Search UI actions surfaced through {@link IridiumEditorOptions.onSearchAction}. */
export type SearchAction = "open" | "next" | "prev" | "close";

/** Which of a command's texts a palette query matched. */
export type PaletteMatchField = "title" | "alias" | "id" | "category" | "description";

/**
 * One command as a palette renders it.
 *
 * Every field is computed in Rust, including both key labels and the match
 * offsets. Nothing here should be re-derived in TypeScript: ranking, labelling
 * and highlighting must be identical in every face, and this shape is the whole
 * contract.
 */
export interface PaletteCommand {
  /** The stable id, and what {@link IridiumEditor.runCommand} takes. */
  readonly id: string;
  /** The human-facing title — what a row shows. */
  readonly title: string;
  /** The longer explanation, when the command has one. */
  readonly description?: string;
  /** The palette grouping label. */
  readonly category: string;
  /** The key sequence that runs it, portable spelling (`Ctrl+K`). */
  readonly keyHint?: string;
  /** The same sequence in macOS glyphs (`⌘K`). */
  readonly keyHintMac?: string;
  /** Advisory: running it can change document text. */
  readonly mutatesDocument: boolean;
  /**
   * Whether it can run right now. False for a mutating command in a read-only
   * buffer — grey the row out rather than hiding it, so the command stays
   * discoverable.
   */
  readonly available: boolean;
  /**
   * Whether the kernel implements it. When false, {@link IridiumEditor.runCommand}
   * reports it through `onHostCommand` instead of running anything, and the host
   * owns the behaviour.
   */
  readonly implemented: boolean;
  /** The rank score. Meaningless in absolute terms; only the order matters. */
  readonly score: number;
  /** Which text matched, absent when the query was empty. */
  readonly matchedField?: PaletteMatchField;
  /**
   * The exact text {@link matches} indexes into.
   *
   * Not always the title: a query can match an alias, the id or the description,
   * and highlighting those offsets inside the title would underline the wrong
   * characters. Render this string when showing *why* a row matched.
   */
  readonly matchedText: string;
  /**
   * Matched positions within {@link matchedText}, as **UTF-16 offsets** — the
   * units `String.prototype.slice` uses, so they can be applied directly.
   */
  readonly matches?: readonly number[];
}

/**
 * One state of the document, as the undo tree records it.
 *
 * Every id is a **decimal string**, not a number: node ids are 64-bit in the
 * kernel and a JavaScript number cannot hold one without silently rounding.
 * Compare them with `===`, never with arithmetic.
 */
export interface UndoTreeNode {
  /** This node's id, and what {@link IridiumEditor.jumpToHistoryNode} takes. */
  readonly id: string;
  /** The state this one was reached from — absent only on the root. */
  readonly parentId?: string;
  /**
   * The states reachable from here, in creation order.
   *
   * Index *i* is the branch {@link IridiumEditor.redoBranch} enters for
   * `branchIndex === i`. More than one means the history forks here.
   */
  readonly childIds: readonly string[];
  /**
   * The child a plain redo would take — the branch last travelled.
   *
   * Follow this from the root to draw the active path. Absent on a leaf, and
   * on a node no traversal has descended from.
   */
  readonly preferredChildId?: string;
  /**
   * Age of this edit in milliseconds, measured from when the tree was created.
   *
   * A monotonic offset, not a wall-clock time: it cannot run backwards when
   * the system clock is adjusted, and it has no epoch to render as a date.
   * Show it as "4m ago", relative to the largest value in the tree.
   */
  readonly elapsedMs: number;
  /** A human-readable label for this edit, when one was recorded. */
  readonly description?: string;
  /** Whether the document is sitting on this state right now. */
  readonly isCurrent: boolean;
}

/** The summary counterpart to {@link UndoTreeSnapshot.nodes}. */
export interface UndoTreeInfo {
  /** The node the document is on, matching the one node with `isCurrent`. */
  readonly currentId: string;
  /** The state the document was opened in. */
  readonly rootId: string;
  /** How many states the tree holds, including the root. */
  readonly nodeCount: number;
  /** Whether anything can be undone from here. */
  readonly canUndo: boolean;
  /** Whether anything can be redone from here. */
  readonly canRedo: boolean;
  /** How many branches leave the current node. */
  readonly branchCount: number;
}

/**
 * The whole undo tree at one instant, for a panel that draws it.
 *
 * Taken in a single call rather than walked node by node: the tree changes on
 * every keystroke, and asking per node would mean one boundary crossing per
 * node per repaint. Nothing here needs a follow-up query — depth comes from
 * following {@link UndoTreeNode.parentId}, and the active path from following
 * {@link UndoTreeNode.preferredChildId} down from the root.
 */
export interface UndoTreeSnapshot {
  /**
   * Every node, oldest first.
   *
   * That is creation order, not drawing order: a panel wanting the shape
   * derives it from the links, and a panel wanting "what did I do, in order"
   * reads this directly.
   */
  readonly nodes: readonly UndoTreeNode[];
  /** The same tree, summarized. */
  readonly info: UndoTreeInfo;
}

/**
 * The ids of the commands the kernel names but leaves to the host to run.
 *
 * Read from the kernel at creation rather than written out here, and that is
 * the whole point of the type. A host command arrives as
 * {@link HostCommandRequest.command}, a string, and a face has to decide what
 * it means — which used to be `request.command === "palette.open"`, a literal
 * typed on this side with nothing keeping it level with the kernel that sends
 * it. Every Rust face names the constant instead and stops compiling when it
 * moves; TypeScript would have gone on compiling, type-checking, passing its
 * suite, and silently never opening the palette again.
 *
 * So compare against these. A chord the kernel consumes and a face cannot
 * recognise is a dead key with nothing to say for itself.
 */
export interface HostCommandIds {
  /** Open the command palette. */
  readonly paletteOpen: string;
  /** Show or hide the undo-tree panel. */
  readonly historyTogglePanel: string;
  /**
   * Show or hide the file explorer.
   *
   * Named here even though a browser has no directory to show: the kernel
   * binds the chord regardless, so the key is consumed either way, and a face
   * that cannot name the command cannot explain why nothing happened.
   */
  readonly explorerTogglePanel: string;
  /**
   * Move the file explorer between a floating panel and a sidebar.
   *
   * Named here for the reason above, and it is the same case one step
   * further along: a page with one document and no window furniture has
   * nowhere to put a column, so this is one the web face can only ever
   * report. The kernel binds `Ctrl+Alt+B` regardless.
   */
  readonly explorerTogglePlacement: string;
  /**
   * Swap between the light and dark themes.
   *
   * Unlike the explorer, a browser can implement this one — the web face
   * holds a theme and the compositor repaints from it. It stays a host
   * command because *which* theme the toggle lands on is the face's call: a
   * page may follow `prefers-color-scheme` where a window follows the system
   * appearance, and the kernel should not have to know the difference.
   */
  readonly viewToggleTheme: string;
  /**
   * Re-read the user's configuration file.
   *
   * The starkest case of the rule above: a browser has no configuration file
   * at all, so this is one the web face can never implement. The kernel still
   * binds the chord, so the key is still consumed — and a face that cannot
   * name the command cannot say why nothing happened.
   */
  readonly configReload: string;
}

// Types for the low-level WASM editor
interface WebEditor {
  // Both of these are `string` and not {@link KeyEventAction} because that is
  // what the generated wasm declarations say: Rust returns `String`. Narrowing
  // happens once, at the call site, through {@link asKeyEventAction}.
  handleKeyEvent(key: string, ctrl: boolean, shift: boolean, alt: boolean, meta: boolean, altGraph: boolean, isRepeat: boolean): string;
  cursorCount(): number;
  listCommands(): string;
  searchCommands(query: string, limit: number): string;
  runCommand(id: string): string;
  keyHintFor(id: string, macGlyphs: boolean): string;
  pendingKeySequence(): string;
  abortPendingKeySequence(): boolean;
  takePendingHostCommand(): string | undefined;
  setUserKeymap(json: string): string | undefined;
  clearUserKeymap(): void;
  takeLastEdit(): WasmEditInfo | undefined;
  getPendingClipboardText(): string | undefined;
  copyText(): string | undefined;
  cutText(): string | undefined;
  /**
   * Loads a font from raw sfnt bytes (.ttf/.otf/.ttc).
   *
   * Throws when the bytes hold no readable face — `.woff2` is not decoded.
   * This used to trap instead of throwing, which on wasm meant the awaited
   * promise never settled at all.
   */
  loadFont(data: Uint8Array): void;
  setContent(content: string): void;
  getContent(): string;
  setDarkTheme(dark: boolean): void;
  forceRender(): void;
  render(): void;
  resize(width: number, height: number): void;
  setPixelRatio(ratio: number): boolean;
  insert(text: string): void;
  backspace(): void;
  delete_forward(): void;
  undo(): boolean;
  redo(): boolean;
  canUndo(): boolean;
  canRedo(): boolean;
  redoBranch(branchIndex: number): boolean;
  historySnapshot(): string;
  jumpToHistoryNode(nodeId: string): boolean;
  moveCursorLeft(): void;
  moveCursorRight(): void;
  moveCursorUp(): void;
  moveCursorDown(): void;
  // No moveCursorWordLeft/Right and no deleteWordBackward/Forward: word motion
  // and word deletion reach the kernel through handleKeyEvent, and the wasm
  // face no longer exports a second implementation of them.
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
  deleteToLineStart(): boolean;
  deleteToLineEnd(): boolean;
  setCursorFromClick(line: number, column: number): void;
  /** `[line, column]`. A `Uint32Array`, not an array — wasm-bindgen returns the
   *  typed view directly. Indexing is identical, so callers read `[0]`/`[1]`. */
  pixelToPosition(x: number, y: number): Uint32Array;
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
  getFoldableLines(): Uint32Array;
  getFoldedLines(): Uint32Array;
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
  /** `[x, y]`, or a negative `x` when the position is not on screen. */
  positionToPixel(line: number, column: number): Float32Array;
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
  /**
   * Focus the canvas once the editor is ready (default: true).
   *
   * ⚠️ Set this to `false` when the editor is not the only thing on the page.
   * `initialize()` used to focus unconditionally, which meant an editor
   * mounting beside a live input **stole the caret mid-typing** and ate the
   * keystrokes that followed — a host cannot defend against it, because the
   * steal happens inside `create()` before any handle exists to call `blur()`
   * on.
   *
   * The default stays `true`: an editor that mounts alone and cannot be typed
   * into until it is clicked is the more common complaint, and every existing
   * consumer was written against focus-on-ready.
   */
  autoFocus?: boolean;
  /** Font URL to load (default: FiraCode from CDN) */
  fontUrl?: string;
  /**
   * Enable syntax highlighting via web-tree-sitter worker (default: false).
   * WARNING: This can cause 1-3 second delays on large files.
   * Set to true only if you need client-side syntax highlighting.
   * For Meridian integration, leave disabled and use server-provided spans.
   */
  enableSyntaxWorker?: boolean;
  /**
   * Construct the syntax worker yourself. Only consulted when
   * {@link enableSyntaxWorker} is `true`.
   *
   * The default resolves `@iridium-editor/syntax-worker`'s built worker as a
   * sibling of this package — correct under npm, yarn and bun, where scoped
   * packages share one directory. Supply this when that does not hold (pnpm's
   * store layout, a bundler that rewrites worker URLs, or a worker you host
   * yourself):
   *
   * ```ts
   * createSyntaxWorker: () =>
   *   new Worker(new URL("@iridium-editor/syntax-worker/worker", import.meta.url), {
   *     type: "module",
   *   })
   * ```
   */
  createSyntaxWorker?: () => Worker;
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
  /**
   * Called when a key sequence resolves to a command the editing kernel does not
   * implement — i.e. one the host registered itself. The key was consumed; running
   * the command is the host's, and any document change must go back through the
   * editor's own methods so it stays undoable.
   */
  onHostCommand?: (request: HostCommandRequest) => void;
  /**
   * Called whenever the half-typed key sequence changes, with the text form of the
   * strokes so far (`""` when nothing is pending).
   *
   * A chord leader such as `Ctrl+K` — or, on macOS, `Cmd+K`, which this layer
   * forwards as `ctrl` — consumes the keypress. Without an indicator the editor
   * simply looks unresponsive, so a host that enables chords should render this.
   */
  onPendingKeySequence?: (sequence: string) => void;
}

/**
 * The option keys that stay optional after defaults are applied: every host
 * callback, plus the worker factory, which has no default value — the absence
 * of one is what selects the built-in sibling-package resolution. Named once
 * so adding a callback cannot silently make it required.
 */
type OptionalCallback =
  | "createSyntaxWorker"
  | "onChange"
  | "onSelectionChange"
  | "onMouseHover"
  | "onScroll"
  | "onBeforeKeyDown"
  | "onSearchAction"
  | "onHostCommand"
  | "onPendingKeySequence";

/**
 * The host callbacks as stored on the controller: every key **present**, its
 * value still allowed to be `undefined`.
 *
 * Deliberately not `Pick<IridiumEditorOptions, OptionalCallback>`, which keeps
 * the properties optional and so lets the constructor omit one entirely. That
 * is not hypothetical: `onHostCommand` and `onPendingKeySequence` were declared,
 * read, and never assigned, which silently discarded every host command the
 * kernel resolved. Requiring the key makes forgetting one a compile error.
 */
type HostCallbacks = {
  [K in OptionalCallback]: IridiumEditorOptions[K];
};

/** A command a key sequence resolved to that the host must run itself. */
export interface HostCommandRequest {
  /** The registered command id the binding named. */
  command: string;
  /** The numeric prefix the user typed, if any. */
  count?: number;
  /** Characters captured by wildcard strokes, in sequence order. */
  captures: string[];
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
  private options: Required<Omit<IridiumEditorOptions, OptionalCallback>> & HostCallbacks;
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
  /**
   * The kernel's display-scale sanitiser, held so every reader of
   * `window.devicePixelRatio` goes through the same one.
   *
   * `devicePixelRatio` is `0` in some headless environments and `undefined`
   * before first layout, and it is read in four places here: canvas sizing
   * at creation, canvas sizing on resize, mouse coordinates, and the
   * viewport height used to pick visible lines. Each used to guard itself,
   * or not — `|| 1` at three sites and nothing at the fourth. Two guards
   * that disagree are worse than one, because the canvas and the text then
   * disagree about how big a pixel is; so there is exactly one, it lives in
   * Rust, and this is the handle to it.
   */
  private readonly sanitizePixelRatio: (ratio: number) => number;

  /**
   * The host command ids as the kernel names them, read once at creation.
   *
   * Constant for the life of the process — the ids are compile-time constants
   * in Rust — so there is nothing to re-read, and holding them means the
   * comparison in a host-command handler costs no boundary crossing.
   */
  private readonly hostCommandIds: HostCommandIds;

  /**
   * The ids of the commands the kernel reports rather than runs.
   *
   * What a face compares {@link HostCommandRequest.command} against; see
   * {@link HostCommandIds} for why it is asked for rather than written down.
   */
  get hostCommands(): HostCommandIds {
    return this.hostCommandIds;
  }

  /**
   * The sanitised `window.devicePixelRatio`, read fresh.
   *
   * Read rather than cached because it changes when the window moves
   * between displays.
   */
  private get pixelRatio(): number {
    return this.sanitizePixelRatio(window.devicePixelRatio);
  }

  /**
   * The ratio the kernel was last told about, and the one the canvas backing
   * store was last sized with.
   *
   * ⭐ Held because the fresh getter above is not enough on its own, and #35
   * is what that looked like. Four things re-read `devicePixelRatio` every
   * time they need it — canvas sizing, mouse coordinates, viewport height —
   * and were correct. The font size was never a read: it was computed once
   * inside `createWebEditor` and baked into the compositor, so a window
   * dragged from a 2x display to a 1x one kept text sized for the display it
   * left. Applying a change requires knowing what was applied before, which
   * no fresh read can tell you.
   *
   * It also makes the canvas's current CSS size exactly recoverable — the
   * backing store divided by this — which is what
   * {@link applyDisplayScale} resizes from, with no layout read and no
   * content-box/border-box mismatch to get wrong.
   */
  private appliedPixelRatio: number;

  private constructor(
    canvas: HTMLCanvasElement,
    editor: WebEditor,
    options: IridiumEditorOptions,
    sanitizePixelRatio: (ratio: number) => number,
    initialPixelRatio: number,
    hostCommandIds: HostCommandIds
  ) {
    this.canvas = canvas;
    this.editor = editor;
    this.sanitizePixelRatio = sanitizePixelRatio;
    this.appliedPixelRatio = initialPixelRatio;
    this.hostCommandIds = hostCommandIds;
    this.options = {
      content: options.content ?? "",
      language: options.language ?? "rust",
      darkTheme: options.darkTheme ?? true,
      autoFocus: options.autoFocus ?? true,
      fontUrl: options.fontUrl ?? "https://cdn.jsdelivr.net/npm/firacode@6.2.0/distr/ttf/FiraCode-Regular.ttf",
      enableSyntaxWorker: options.enableSyntaxWorker ?? false,
      createSyntaxWorker: options.createSyntaxWorker,
      onChange: options.onChange,
      onSelectionChange: options.onSelectionChange,
      onMouseHover: options.onMouseHover,
      onScroll: options.onScroll,
      onBeforeKeyDown: options.onBeforeKeyDown,
      onSearchAction: options.onSearchAction,
      onHostCommand: options.onHostCommand,
      onPendingKeySequence: options.onPendingKeySequence,
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

    // The wasm module arrives through its package specifier, not a relative
    // path. `iridium-bindings` is this package's peerDependency, so every
    // resolver — a bundler, Node, or a browser import map — finds it the same
    // way.
    //
    // ⚠️ This used to walk `../../../../../crates/iridium-bindings/pkg/…` from
    // `import.meta.url`, which resolved only inside this repository. From an
    // installed `node_modules/@iridium-editor/core/dist/controller/`, five
    // levels up lands above the consumer's `node_modules`, where no `crates/`
    // directory exists — so no published version of this package could ever
    // load its own wasm. Fixed in 0.2.0.
    const wasm = await import("iridium-bindings");
    await wasm.default();

    // Set canvas size. The ratio is sanitised by the kernel rather than by
    // `|| 1` here, so that the canvas dimensions and the font scale below
    // are derived from the identical number — see `sanitizePixelRatio`.
    const sanitizePixelRatio = wasm.sanitizePixelRatio as (ratio: number) => number;
    const pixelRatio = sanitizePixelRatio(window.devicePixelRatio);
    canvas.width = rect.width * pixelRatio;
    canvas.height = rect.height * pixelRatio;

    // The ids of the commands the kernel names and hands back rather than
    // running. Read from the module, not written out here — see
    // `HostCommandIds`. Parsed once: they are compile-time constants in Rust.
    const hostCommandIds = JSON.parse(
      (wasm.hostCommandIds as () => string)()
    ) as HostCommandIds;

    // Create the low-level editor
    const editor = await wasm.createWebEditor(canvas, pixelRatio);

    // Create the controller
    // `pixelRatio` is passed on as the applied ratio, not re-read: the canvas
    // above and the compositor's font size were both derived from this exact
    // number, and that agreement is the invariant `appliedPixelRatio` records.
    const instance = new IridiumEditor(
      canvas,
      editor,
      options,
      sanitizePixelRatio,
      pixelRatio,
      hostCommandIds
    );

    // Initialize
    await instance.initialize();

    return instance;
  }

  private async initialize(): Promise<void> {
    // Load font.
    //
    // A wasm target has NO system fonts, so this is not an enhancement that
    // can be skipped — without it the renderer has nothing to shape and draws
    // no glyphs at all. Both failures below therefore throw rather than fall
    // through: `initialize` is awaited by `create`, so a throw here rejects
    // the caller's promise, which is the only way an embedder can tell "this
    // editor is dead" from "this editor is slow".
    //
    // The previous `if (ok)` swallowed a failed fetch entirely and left an
    // editor that looked constructed and could never render.
    const fontResponse = await fetch(this.options.fontUrl);
    if (!fontResponse.ok) {
      throw new Error(
        `Iridium could not fetch its font from ${this.options.fontUrl} ` +
          `(HTTP ${fontResponse.status}). A wasm build has no system font to ` +
          `fall back on, so the editor cannot render without it.`,
      );
    }
    const fontData = new Uint8Array(await fontResponse.arrayBuffer());
    // Throws when the bytes hold no readable face — most often because the
    // URL serves .woff2, which this build does not decode. Raw sfnt only.
    this.editor.loadFont(fontData);

    // Initialize syntax highlighting via Web Worker (only if enabled)
    // DISABLED BY DEFAULT: Web worker parsing causes 1-3s delays on large files.
    // For Meridian integration, syntax spans will come from the server.
    if (this.options.enableSyntaxWorker) {
      try {
        // A host that owns its bundler can hand the worker over directly; see
        // `createSyntaxWorker`. Otherwise fall back to the sibling package.
        this.syntaxWorker = new SyntaxHighlightClient(
          this.options.createSyntaxWorker ?? (() => {
            // From this module at `<scope>/core/dist/controller/index.js`,
            // three levels up is the scope directory the two packages share,
            // so this lands on `<scope>/syntax-worker/dist/worker.js` both in
            // an installed tree and in this repository, where `packages/
            // @iridium/` has the same shape.
            //
            // ⚠️ This used to name `syntax-worker/src/worker.ts` — TypeScript
            // source, which no consumer's Worker constructor can load, so
            // `enableSyntaxWorker: true` could never have worked outside a
            // bundler configured for this repository. Fixed in 0.2.0.
            const workerUrl = new URL(
              "../../../syntax-worker/dist/worker.js",
              import.meta.url
            );
            return new Worker(workerUrl, { type: "module" });
          })
        );
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

    // Focus the canvas, unless the host asked us not to. See `autoFocus`:
    // mounting beside a live input used to steal the caret mid-typing.
    if (this.options.autoFocus) {
      this.canvas.focus();
    }
  }

  private attachEventHandlers(): void {
    // Keyboard
    const handleKeyDown = this.handleKeyDown.bind(this);
    this.canvas.addEventListener("keydown", handleKeyDown);
    this.eventCleanup.push(() => this.canvas.removeEventListener("keydown", handleKeyDown));

    // Focus loss. A half-typed chord must not survive it: the core would hold the
    // leader pending indefinitely and consume the first keystroke after the user
    // came back. Every cursor-moving path already aborts the sequence, which covers
    // a click elsewhere on the page; a Cmd-Tab away with no click does not.
    const handleBlur = (): void => {
      if (this.editor.abortPendingKeySequence()) {
        this.notifyPendingKeySequence();
      }
    };
    this.canvas.addEventListener("blur", handleBlur);
    this.eventCleanup.push(() => this.canvas.removeEventListener("blur", handleBlur));

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
        const { pixelRatio } = this;
        // Before the canvas is sized, so the font and the backing store are
        // derived from the same ratio within one turn. A display change that
        // also changes the box size arrives here rather than at the watcher
        // below, and this is the only thing that makes the two paths agree.
        this.applyPixelRatioToKernel(pixelRatio);
        this.canvas.width = width * pixelRatio;
        this.canvas.height = height * pixelRatio;
        this.appliedPixelRatio = pixelRatio;
        this.editor.resize(this.canvas.width, this.canvas.height);
        this.editor.forceRender();
      }
    });
    resizeObserver.observe(resizeTarget);
    this.eventCleanup.push(() => resizeObserver.disconnect());

    this.watchDisplayScale();
  }

  /**
   * Re-applies the display scale factor whenever it changes.
   *
   * ⚠️ **The `ResizeObserver` above cannot carry this on its own.** Dragging a
   * window from a 2x display to a 1x one usually leaves the CSS box exactly
   * the same size, so the observer has nothing to report and never fires — yet
   * every physical dimension in the editor just changed. There is no resize
   * event to hang the fix on.
   *
   * The platform offers no `devicePixelRatiochange` event either. The
   * supported mechanism is a media query pinned to the *current* ratio, which
   * stops matching the moment the ratio moves. That makes it a one-shot: it
   * must be re-armed at the new ratio each time, which is why this calls
   * itself rather than looping.
   *
   * `dppx` and `devicePixelRatio` are the same quantity, so the query is exact
   * rather than a threshold. It is written with a small tolerance band anyway
   * because the ratio is a float and an exact-equality media query on
   * something like 1.7647058823529411 is asking the browser's parser to
   * round-trip a value it may not.
   */
  private watchDisplayScale(): void {
    if (typeof window.matchMedia !== "function") return;

    // Held so re-arming replaces the previous watch rather than accumulating
    // one dead listener per display change for the life of the session, and so
    // a single cleanup entry can cancel whichever one is currently live.
    let live: { query: MediaQueryList; onChange: () => void } | null = null;

    const disarm = () => {
      if (live === null) return;
      live.query.removeEventListener("change", live.onChange);
      live = null;
    };

    const arm = () => {
      disarm();
      const ratio = this.pixelRatio;
      // A band rather than `(resolution: Xdppx)`: a fractional ratio that does
      // not survive serialisation would produce a query that never matches,
      // which is a watcher that silently does nothing.
      const query = window.matchMedia(
        `(min-resolution: ${ratio * 0.99}dppx) and (max-resolution: ${ratio * 1.01}dppx)`
      );

      const onChange = () => {
        this.applyDisplayScale();
        // Re-armed at whatever the ratio is *now*, not at what it was when
        // this listener was created — which is the whole reason the watch is a
        // one-shot rather than a standing subscription.
        arm();
      };

      query.addEventListener("change", onChange);
      live = { query, onChange };
    };

    arm();
    this.eventCleanup.push(disarm);
  }

  /**
   * Brings the kernel and the canvas onto the current display's scale factor.
   *
   * Called only from the media-query watcher, where no resize event will
   * arrive — so it has to do the canvas half itself. It does not read layout
   * to find the CSS size: the backing store was set as CSS size times
   * {@link appliedPixelRatio}, so dividing recovers it, with none of the
   * content-box against border-box ambiguity a `getBoundingClientRect` would
   * introduce.
   *
   * Recovers it to within a rounding step, not exactly — `canvas.width` is an
   * integer, so the product that produced it was truncated. The drift is under
   * one CSS pixel and the next real resize replaces the value outright, which
   * is a better trade than a layout read that can disagree with the observer's
   * `contentRect` by a padding.
   */
  private applyDisplayScale(): void {
    const ratio = this.pixelRatio;
    if (ratio === this.appliedPixelRatio) return;

    const cssWidth = this.canvas.width / this.appliedPixelRatio;
    const cssHeight = this.canvas.height / this.appliedPixelRatio;

    this.applyPixelRatioToKernel(ratio);

    const width = Math.max(1, Math.round(cssWidth * ratio));
    const height = Math.max(1, Math.round(cssHeight * ratio));
    this.canvas.width = width;
    this.canvas.height = height;
    this.appliedPixelRatio = ratio;
    this.editor.resize(width, height);
    this.editor.forceRender();
  }

  /**
   * Tells the kernel the ratio, if it does not already have it.
   *
   * Separate from the canvas half because the two paths that need it differ in
   * what else they do: the observer sizes the canvas from a `contentRect` it
   * was handed, while {@link applyDisplayScale} has to work the size out. What
   * they must not differ on is *whether the kernel was told*, so that part is
   * one function with one guard.
   *
   * Guarded rather than unconditional because the observer fires on every
   * ordinary window resize, and re-applying an unchanged size would discard
   * the compositor's glyph atlas on each one.
   */
  private applyPixelRatioToKernel(ratio: number): void {
    if (ratio === this.appliedPixelRatio) return;
    this.editor.setPixelRatio(ratio);
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
    // Was the one site with no guard at all: a `devicePixelRatio` of `0`
    // put every click at the origin and `NaN` sent `NaN` into
    // `pixelToPosition`.
    const { pixelRatio } = this;
    const x = (e.clientX - rect.left) * pixelRatio;
    const y = (e.clientY - rect.top) * pixelRatio;
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
    const action = asKeyEventAction(this.editor.handleKeyEvent(
      t.key, t.ctrl, t.shift, t.alt, t.meta, t.altGraph, e.repeat,
    ));

    if (action === "ignored") {
      // The editor does not handle this key; leave it to the browser.
      return;
    }

    e.preventDefault();
    this.applyActionOutcome(action);
  }

  /**
   * Applies everything that follows a consumed action, whatever produced it.
   *
   * A keypress and a palette invocation return the same {@link KeyEventAction}
   * from the same Rust handler, and everything after that point — the pending
   * indicator, host-command collection, the clipboard, search requests, scrolling
   * the caret into view, rendering, re-highlighting and the change notifications —
   * is identical. Two copies would drift the first time either was edited, and the
   * divergence would show up as "the palette does not update the highlighting"
   * rather than as an obvious bug.
   *
   * Callers pass only actions they have decided to consume; `"ignored"` must be
   * handled before getting here.
   */
  private applyActionOutcome(action: KeyEventAction): void {
    // A consumed key may have left a chord leader pending (Ctrl+K, and on macOS
    // Cmd+K, which this layer forwards as ctrl). The core consumes it, so without
    // an indicator the editor looks unresponsive; surface it every time so the
    // indicator also clears when the sequence completes or is abandoned.
    this.notifyPendingKeySequence();

    if (action === "handled:command") {
      const request = this.editor.takePendingHostCommand();
      if (request) {
        this.options.onHostCommand?.(JSON.parse(request) as HostCommandRequest);
      }
    } else if (action === "copy" || action === "cut") {
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
  ): { key: string; ctrl: boolean; shift: boolean; alt: boolean; meta: boolean; altGraph: boolean } {
    // AltGr is reported by browsers as Ctrl+Alt held together on many non-US
    // layouts, which collides with the Ctrl+Alt add-cursor chord. The Rust
    // core disambiguates with this bit (Modifiers::alt_graph), so it is
    // forwarded on every branch. Synthetic KeyboardEvents may lack
    // getModifierState, hence the defensive call.
    const altGraph = e.getModifierState?.("AltGraph") ?? false;
    if (!this.isMacPlatform) {
      return {
        key: e.key,
        ctrl: e.ctrlKey,
        shift: e.shiftKey,
        alt: e.altKey,
        meta: e.metaKey,
        altGraph,
      };
    }

    // Option-only chords: word motions and word deletes.
    if (e.altKey && !e.metaKey && !e.ctrlKey) {
      switch (e.key) {
        case "ArrowLeft":
        case "ArrowRight":
        case "Backspace":
        case "Delete":
          return { key: e.key, ctrl: true, shift: e.shiftKey, alt: false, meta: false, altGraph };
        default:
          break;
      }
    }

    // Command-only chords: line/document motions.
    if (e.metaKey && !e.altKey && !e.ctrlKey) {
      switch (e.key) {
        case "ArrowLeft":
          return { key: "Home", ctrl: false, shift: e.shiftKey, alt: false, meta: false, altGraph };
        case "ArrowRight":
          return { key: "End", ctrl: false, shift: e.shiftKey, alt: false, meta: false, altGraph };
        case "ArrowUp":
          return { key: "Home", ctrl: true, shift: e.shiftKey, alt: false, meta: false, altGraph };
        case "ArrowDown":
          return { key: "End", ctrl: true, shift: e.shiftKey, alt: false, meta: false, altGraph };
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
          altGraph,
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
      altGraph,
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
    const viewportHeight = this.canvas.height / this.pixelRatio;

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
      // Copied out of the wasm heap's `Uint32Array` view rather than passed
      // through. {@link EditorState.foldedLines} is declared `number[]`, and
      // it used to hand back the typed array under that name — so a consumer
      // doing `.filter(...)` got a `Uint32Array` and `.push(...)` threw. The
      // list is a handful of entries; the copy is not worth measuring.
      foldedLines: Array.from(this.editor.getFoldedLines()),
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

  /**
   * Redo into a specific branch of the current state, rather than the one
   * {@link redo} would take.
   *
   * `branchIndex` indexes the current node's {@link UndoTreeNode.childIds}, in
   * creation order. This is how a panel descends a fork that was abandoned:
   * plain redo follows the branch last travelled, this chooses explicitly and
   * makes that choice the new active path.
   *
   * Returns false, changing nothing, when the index names no branch.
   */
  redoBranch(branchIndex: number): boolean {
    const result = this.editor.redoBranch(branchIndex);
    this.updateHighlights(result ? this.takeEditInfo() : null);
    this.editor.forceRender();
    return result;
  }

  /**
   * Move to any state in the undo tree, replaying the document to it.
   *
   * The tree walks up to the common ancestor and back down, so this reaches
   * states no sequence of {@link undo} and {@link redo} could reach without
   * first abandoning the current branch — which is the whole reason the
   * history is a tree and not a stack. However many edges it crosses, the host
   * sees one change.
   *
   * `nodeId` is the decimal string a snapshot reports. Returns false, changing
   * nothing, when it names no state in this tree.
   */
  jumpToHistoryNode(nodeId: string): boolean {
    const result = this.editor.jumpToHistoryNode(nodeId);
    this.updateHighlights(result ? this.takeEditInfo() : null);
    this.editor.forceRender();
    return result;
  }

  /**
   * The whole undo tree, for a panel that draws it.
   *
   * Cheap enough to call on every repaint — one boundary crossing and one
   * parse — and deliberately not cached here, because the tree changes on
   * every keystroke and a stale tree drawn over a live document is worse than
   * no panel at all.
   *
   * @throws if the kernel could not describe its own history, which would mean
   * the editor handle is no longer usable.
   */
  historySnapshot(): UndoTreeSnapshot {
    const parsed = JSON.parse(this.editor.historySnapshot()) as UndoTreeSnapshot | null;
    if (parsed === null) {
      throw new Error("The editor could not describe its undo history");
    }
    return parsed;
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

  /**
   * Replace the user keymap layer from a JSON keymap.
   *
   * The layer sits on top of the built-in defaults, so it overrides exactly the
   * sequences it names and nothing else; a second call replaces it rather than
   * stacking. Returns `undefined` on success, or the diagnostic text when the
   * keymap is rejected — an unknown command id, a binding whose bare prefix would
   * strand a default chord, or malformed JSON. Nothing changes on failure.
   */
  setUserKeymap(json: string): string | undefined {
    return this.editor.setUserKeymap(json);
  }

  /** Drop the user keymap layer, restoring the built-in defaults. */
  clearUserKeymap(): void {
    this.editor.clearUserKeymap();
    this.notifyPendingKeySequence();
  }

  /** The half-typed key sequence, as text; `""` when nothing is pending. */
  pendingKeySequence(): string {
    return this.editor.pendingKeySequence();
  }

  /** Reports the current half-typed key sequence to the host. */
  private notifyPendingKeySequence(): void {
    this.options.onPendingKeySequence?.(this.editor.pendingKeySequence());
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

  /**
   * Blur the editor canvas, so an overlay's own input can take the keyboard.
   *
   * The editor's `keydown` listener is on the **canvas**, not on `window`, so a
   * focused palette input receives keys the editor never sees — no capture-phase
   * interception is needed. Blurring is still required, because a canvas that
   * keeps focus keeps consuming keys.
   *
   * Blurring also runs the canvas blur handler, which aborts a half-typed chord.
   * That is wanted: opening a palette abandons whatever sequence was in flight.
   * Call {@link focus} when the overlay closes — an editor that has to be clicked
   * back into after every command is what makes a palette feel broken.
   */
  blurEditor(): void {
    this.canvas.blur();
  }

  /**
   * Whether key labels should be rendered in macOS glyphs (`⌘K` over `Ctrl+K`).
   *
   * The same binding either way: this layer forwards macOS `Cmd` as the core's
   * `ctrl`, so the difference is purely how it is spelled to a reader.
   */
  get usesMacKeyLabels(): boolean {
    return this.isMacPlatform;
  }

  /**
   * How many carets are active — `1` unless multi-cursor is in play.
   *
   * Worth putting in a status bar. This face rendered only the primary caret for
   * its whole life and published no count, so a document with four cursors was
   * indistinguishable from one with a single cursor, and a working command
   * looked broken. Showing the number makes that class of defect loud.
   */
  get cursorCount(): number {
    return this.editor.cursorCount();
  }

  /**
   * Every registered command, in browse order, for a palette opened with no
   * query.
   *
   * Grouped by category rather than ranked, because an empty palette is being
   * read rather than searched. Key labels come from the live keymap, so a user
   * layer pushed with {@link setUserKeymap} is reflected without any work here.
   */
  listCommands(): PaletteCommand[] {
    return JSON.parse(this.editor.listCommands()) as PaletteCommand[];
  }

  /**
   * Commands matching `query`, best first.
   *
   * Call this on every keystroke: the matcher is in Rust, takes microseconds over
   * the command set, and debouncing would only add perceived lag. An empty query
   * returns everything, ranked by recency, so a caller need not special-case it.
   *
   * `limit` caps the result *after* ranking, so the top of a limited list is the
   * top of an unlimited one. Omit it for everything.
   */
  searchCommands(query: string, limit?: number): PaletteCommand[] {
    return JSON.parse(this.editor.searchCommands(query, limit ?? 0)) as PaletteCommand[];
  }

  /**
   * Run a command by id, exactly as a key bound to it would.
   *
   * Takes the same path as a keypress — the same handler, the same
   * {@link KeyEventAction}, the same follow-up — so a palette invocation is
   * indistinguishable downstream, undo grouping and multi-cursor state included.
   *
   * A command the core does not implement is reported through
   * {@link IridiumEditorOptions.onHostCommand} rather than run, and still returns
   * `"handled:command"`. `"ignored"` therefore means one thing only: no command
   * with that id is registered.
   *
   * Running a command moves it up the palette's recency ranking for next time.
   */
  runCommand(id: string): KeyEventAction {
    const action = asKeyEventAction(this.editor.runCommand(id));
    if (action !== "ignored") {
      this.applyActionOutcome(action);
    }
    return action;
  }

  /**
   * The key sequence bound to `id`, or `undefined` when nothing is.
   *
   * Defaults to this platform's spelling; pass `macGlyphs` to override. Entries
   * from {@link listCommands} already carry both, so this is for the cases that
   * do not go through a palette — a menu item, a tooltip, an empty-state hint.
   */
  keyHintFor(id: string, macGlyphs: boolean = this.isMacPlatform): string | undefined {
    return this.editor.keyHintFor(id, macGlyphs) || undefined;
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
