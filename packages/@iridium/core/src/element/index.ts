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
 */

import { IridiumEditor, type EditorState } from "../controller/index";

/**
 * Custom element for the Iridium editor.
 */
export class IridiumEditorElement extends HTMLElement {
  private editor: IridiumEditor | null = null;
  private canvas: HTMLCanvasElement | null = null;
  private shadow: ShadowRoot;
  private initialized = false;

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
      });

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
  focus(): void {
    this.editor?.focus();
  }
}

// Register the custom element
if (typeof customElements !== "undefined" && !customElements.get("iridium-editor")) {
  customElements.define("iridium-editor", IridiumEditorElement);
}

export default IridiumEditorElement;
