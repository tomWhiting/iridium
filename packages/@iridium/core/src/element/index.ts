/**
 * Iridium Editor Web Component
 *
 * A custom element that provides a fully-featured GPU-accelerated code editor.
 * Works in any framework or vanilla HTML.
 *
 * Usage:
 *   <iridium-editor language="rust" theme="dark"></iridium-editor>
 *
 * Attributes:
 *   - language: Programming language for syntax highlighting (default: "rust")
 *   - theme: "dark" or "light" (default: "dark")
 *   - content: Initial editor content
 *   - readonly: If present, editor is read-only
 *
 * Events:
 *   - iridium-change: Fired when content changes, detail contains { content: string }
 *   - iridium-selection: Fired when selection changes, detail contains { line, column }
 *   - iridium-ready: Fired when editor is initialized
 *   - iridium-host-command: Fired for a command the kernel resolved but does not
 *     implement and this element does not handle itself, detail is the request
 */

import {
  type HostCommandRequest,
  IridiumEditor,
  type EditorState,
} from "../controller/index.ts";
import { CommandPalette, type PaletteHost } from "../palette/index.ts";
import { PaletteView } from "./palette.ts";
import { UndoTreePanel, type UndoTreeHost } from "../history/index.ts";
import { UndoTreeView } from "./tree.ts";

/**
 * Custom element for the Iridium editor.
 */
export class IridiumEditorElement extends HTMLElement {
  private editor: IridiumEditor | null = null;
  private canvas: HTMLCanvasElement | null = null;
  private shadow: ShadowRoot;
  private initialized = false;
  private palette: CommandPalette | null = null;
  private paletteView: PaletteView | null = null;
  private undoTree: UndoTreePanel | null = null;
  private undoTreeView: UndoTreeView | null = null;

  // Observed attributes
  static get observedAttributes(): string[] {
    return ["language", "theme", "content", "readonly"];
  }

  constructor() {
    super();
    this.shadow = this.attachShadow({ mode: "open" });
  }

  connectedCallback(): void {
    this.render();
    this.initEditor();
  }

  disconnectedCallback(): void {
    this.paletteView?.destroy();
    this.paletteView = null;
    this.palette = null;
    this.undoTreeView?.destroy();
    this.undoTreeView = null;
    this.undoTree = null;
    this.editor?.destroy();
    this.editor = null;
    this.initialized = false;
  }

  attributeChangedCallback(name: string, oldValue: string | null, newValue: string | null): void {
    if (!this.editor || oldValue === newValue) return;

    switch (name) {
      case "language":
        if (newValue) {
          this.editor.setLanguage(newValue);
        }
        break;
      case "theme":
        this.editor.setTheme(newValue === "dark");
        break;
      case "content":
        if (newValue !== null) {
          this.editor.setContent(newValue);
        }
        break;
      // A boolean attribute: present in any form means read-only, absent
      // means writable — so the test is presence, not value, and
      // `readonly=""` (what the parser produces for a bare `readonly`) reads
      // as true rather than as an empty string meaning false.
      case "readonly":
        this.editor.setReadOnly(newValue !== null);
        break;
    }
  }

  private render(): void {
    this.shadow.innerHTML = `
      <style>
        :host {
          display: block;
          width: 100%;
          height: 100%;
          min-height: 200px;
          position: relative;
        }
        .container {
          width: 100%;
          height: 100%;
          position: relative;
        }
        canvas {
          width: 100%;
          height: 100%;
          display: block;
          outline: none;
          cursor: text;
        }
        .loading {
          position: absolute;
          inset: 0;
          display: flex;
          align-items: center;
          justify-content: center;
          background: #1a1a1a;
          color: #888;
          font-family: monospace;
        }
        .error {
          position: absolute;
          inset: 0;
          display: flex;
          align-items: center;
          justify-content: center;
          background: #1a1a1a;
          color: #ff6b6b;
          font-family: monospace;
          padding: 20px;
          text-align: center;
        }
      </style>
      <div class="container">
        <canvas tabindex="0"></canvas>
        <div class="loading">Loading Iridium...</div>
      </div>
    `;

    this.canvas = this.shadow.querySelector("canvas");

    // Mounted *inside* the shadow root, and after the markup above, because
    // setting `innerHTML` replaces everything the root holds. A palette portalled
    // to `document.body` — what the React example does — would render here with
    // no styles at all, since a shadow root's CSS does not reach outside it.
    this.palette = new CommandPalette(this.paletteHost());
    this.paletteView = new PaletteView(this.shadow, this.palette);
    this.undoTree = new UndoTreePanel(this.undoTreeHost());
    this.undoTreeView = new UndoTreeView(this.shadow, this.undoTree);
  }

  /**
   * The editor surface the palette drives.
   *
   * Delegates through `this.editor` rather than capturing it, so the palette can
   * be built before the wasm module has loaded; until then it simply lists
   * nothing, which is honest — no command is reachable yet either.
   */
  private paletteHost(): PaletteHost {
    const element = this;
    return {
      searchCommands: (query, limit) => element.editor?.searchCommands(query, limit) ?? [],
      runCommand: (id) => element.editor?.runCommand(id),
      blurEditor: () => element.editor?.blurEditor(),
      focus: () => element.editor?.focus(),
      get usesMacKeyLabels(): boolean {
        return element.editor?.usesMacKeyLabels ?? false;
      },
    };
  }

  /**
   * The editor surface the undo-tree panel drives.
   *
   * Delegates through `this.editor` for the same reason the palette's host
   * does, and reports an empty tree until the wasm module has loaded — which is
   * honest, since there is no history to show before there is a document.
   */
  private undoTreeHost(): UndoTreeHost {
    const element = this;
    return {
      historySnapshot: () =>
        element.editor?.historySnapshot() ?? {
          nodes: [],
          info: {
            currentId: "",
            rootId: "",
            nodeCount: 0,
            canUndo: false,
            canRedo: false,
            branchCount: 0,
          },
        },
      jumpToHistoryNode: (nodeId) => element.editor?.jumpToHistoryNode(nodeId) ?? false,
      blurEditor: () => element.editor?.blurEditor(),
      focus: () => element.editor?.focus(),
    };
  }

  /**
   * Runs a command the kernel named but left to the host.
   *
   * `palette.open` and `history.togglePanel` are handled here because this
   * element owns both overlays; anything else goes out as an event, so a host
   * that adds its own host commands is never silently ignored.
   */
  private handleHostCommand(request: HostCommandRequest): void {
    if (request.command === "palette.open") {
      this.palette?.open();
      return;
    }
    if (request.command === "history.togglePanel") {
      this.undoTree?.toggle();
      return;
    }
    this.dispatchEvent(
      new CustomEvent("iridium-host-command", {
        detail: request,
        bubbles: true,
        composed: true,
      })
    );
  }

  private async initEditor(): Promise<void> {
    if (this.initialized || !this.canvas) return;
    this.initialized = true;

    const loadingEl = this.shadow.querySelector(".loading") as HTMLElement;

    try {
      const language = this.getAttribute("language") || "rust";
      const theme = this.getAttribute("theme") !== "light";
      const content = this.getAttribute("content") || "";

      this.editor = await IridiumEditor.create(this.canvas, {
        language,
        darkTheme: theme,
        content,
        onChange: (newContent) => {
          // The tree grows on every edit, and a panel showing the tree as it
          // was would offer a jump to a node the editor has moved past.
          this.undoTree?.refresh();
          this.dispatchEvent(
            new CustomEvent("iridium-change", {
              detail: { content: newContent },
              bubbles: true,
              composed: true,
            })
          );
        },
        onSelectionChange: (info) => {
          this.dispatchEvent(
            new CustomEvent("iridium-selection", {
              detail: info,
              bubbles: true,
              composed: true,
            })
          );
        },
        onHostCommand: (request) => this.handleHostCommand(request),
      });

      // Applied after creation rather than passed as an option, because
      // `IridiumEditorOptions` has no field for it and the kernel's default is
      // writable. Read at mount as well as on change: an element that arrives
      // with `readonly` already set never fires
      // `attributeChangedCallback` for it.
      if (this.hasAttribute("readonly")) {
        this.editor.setReadOnly(true);
      }

      // Hide loading indicator
      if (loadingEl) {
        loadingEl.style.display = "none";
      }

      // Dispatch ready event
      this.dispatchEvent(
        new CustomEvent("iridium-ready", {
          bubbles: true,
          composed: true,
        })
      );
    } catch (e) {
      console.error("[IridiumEditorElement] Failed to initialize:", e);

      // Show error
      if (loadingEl) {
        loadingEl.className = "error";
        loadingEl.textContent = `Failed to initialize: ${e instanceof Error ? e.message : String(e)}`;
      }
    }
  }

  // Public API

  /** Get the current editor content. */
  getContent(): string {
    return this.editor?.getContent() ?? "";
  }

  /** Set the editor content. */
  setContent(content: string): void {
    this.editor?.setContent(content);
  }

  /** Get the current editor state. */
  getState(): EditorState | null {
    return this.editor?.getState() ?? null;
  }

  /** Undo the last change. */
  undo(): boolean {
    return this.editor?.undo() ?? false;
  }

  /** Redo the last undone change. */
  redo(): boolean {
    return this.editor?.redo() ?? false;
  }

  /** Fold all foldable regions. */
  foldAll(): void {
    this.editor?.foldAll();
  }

  /** Unfold all folded regions. */
  unfoldAll(): void {
    this.editor?.unfoldAll();
  }

  /** Toggle fold at a specific line. */
  toggleFold(line: number): boolean {
    return this.editor?.toggleFold(line) ?? false;
  }

  /** Set the syntax highlighting language. */
  async setLanguage(language: string): Promise<boolean> {
    return this.editor?.setLanguage(language) ?? false;
  }

  /** Set the theme (true = dark, false = light). */
  setTheme(dark: boolean): void {
    this.editor?.setTheme(dark);
  }

  /** Focus the editor. */
  override focus(): void {
    this.editor?.focus();
  }

  /** Opens the command palette, as `Ctrl+K` does. */
  openCommandPalette(): void {
    this.palette?.open();
  }

  /** Closes the command palette and returns the keyboard to the editor. */
  closeCommandPalette(): void {
    this.palette?.close();
  }

  /** Shows or hides the undo-tree panel, as `Ctrl+Alt+H` does. */
  toggleUndoTree(): void {
    this.undoTree?.toggle();
  }

  /** Closes the undo-tree panel and returns the keyboard to the editor. */
  closeUndoTree(): void {
    this.undoTree?.close();
  }
}

// Register the custom element
if (typeof customElements !== "undefined" && !customElements.get("iridium-editor")) {
  customElements.define("iridium-editor", IridiumEditorElement);
}

export default IridiumEditorElement;
