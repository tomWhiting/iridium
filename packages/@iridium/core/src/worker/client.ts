/**
 * Client API for syntax highlighting Web Worker.
 *
 * Use this class on the main thread to communicate with the
 * syntax highlighting worker without blocking the UI.
 */

import type { WorkerRequest, WorkerResponse, HighlightSpan, EditInfo } from "./protocol.ts";

export type { HighlightSpan, EditInfo } from "./protocol.ts";

export interface HighlightResult {
  spans: HighlightSpan[];
  parseTime: number;
}

type PendingRequest = {
  resolve: (result: HighlightResult) => void;
  reject: (error: Error) => void;
};

export class SyntaxHighlightClient {
  private worker: Worker | null = null;
  private pending: Map<number, PendingRequest> = new Map();
  private nextId = 1;
  private readyPromise: Promise<string[]> | null = null;
  private readyResolve: ((languages: string[]) => void) | null = null;
  private availableLanguages: string[] = [];

  /**
   * Create a new syntax highlight client.
   *
   * @param workerFactory - Function that creates the Worker instance.
   *   This allows the consumer to control how the worker is instantiated
   *   (e.g., using Vite's `new Worker(new URL(...), { type: 'module' })`)
   */
  constructor(workerFactory: () => Worker) {
    this.worker = workerFactory();
    this.worker.onmessage = this.handleMessage.bind(this);
    this.worker.onerror = this.handleError.bind(this);
  }

  private handleMessage(event: MessageEvent<WorkerResponse>): void {
    const response = event.data;

    switch (response.type) {
      case "ready":
        this.availableLanguages = response.languages;
        if (this.readyResolve) {
          this.readyResolve(response.languages);
          this.readyResolve = null;
        }
        break;

      case "languageSet":
        // Language set confirmation - could add callback if needed
        break;

      case "highlights": {
        const pending = this.pending.get(response.id);
        if (pending) {
          this.pending.delete(response.id);
          pending.resolve({
            spans: response.spans,
            parseTime: response.parseTime,
          });
        }
        break;
      }

      case "error": {
        const pendingErr = this.pending.get(response.id);
        if (pendingErr) {
          this.pending.delete(response.id);
          pendingErr.reject(new Error(response.message));
        }
        break;
      }
    }
  }

  private handleError(event: ErrorEvent): void {
    console.error("[SyntaxHighlightClient] Worker error:", event.message);
    // Reject all pending requests
    for (const [id, pending] of this.pending) {
      pending.reject(new Error(`Worker error: ${event.message}`));
      this.pending.delete(id);
    }
  }

  /**
   * Initialize the worker with a default language.
   * Returns when the worker is ready.
   *
   * @param language - Default language to load (e.g., "rust", "typescript")
   * @returns Promise resolving to list of available languages
   */
  async initialize(language: string = "rust"): Promise<string[]> {
    if (this.readyPromise) {
      return this.readyPromise;
    }

    this.readyPromise = new Promise((resolve) => {
      this.readyResolve = resolve;
    });

    const request: WorkerRequest = { type: "init", language };
    this.worker?.postMessage(request);

    return this.readyPromise;
  }

  /**
   * Get the list of available languages.
   */
  getAvailableLanguages(): string[] {
    return this.availableLanguages;
  }

  /**
   * Set the current language for highlighting.
   *
   * @param language - Language identifier
   */
  async setLanguage(language: string): Promise<void> {
    const request: WorkerRequest = { type: "setLanguage", language };
    this.worker?.postMessage(request);
  }

  /**
   * Highlight an entire document.
   *
   * @param content - Full document content
   * @returns Promise resolving to highlight spans and timing info
   */
  async highlight(content: string): Promise<HighlightResult> {
    const id = this.nextId++;
    const request: WorkerRequest = { type: "highlight", id, content };

    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.worker?.postMessage(request);
    });
  }

  /**
   * Highlight with incremental parsing after an edit.
   *
   * @param content - Full document content after edit
   * @param editInfo - Information about the edit for incremental parsing
   * @returns Promise resolving to highlight spans
   */
  async highlightIncremental(content: string, editInfo: EditInfo): Promise<HighlightResult> {
    const id = this.nextId++;
    const request: WorkerRequest = {
      type: "highlightIncremental",
      id,
      content,
      editInfo,
    };

    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.worker?.postMessage(request);
    });
  }

  /**
   * Highlight only a range of lines (viewport-aware).
   *
   * @param content - Full document content
   * @param startLine - First visible line (0-indexed)
   * @param endLine - Last visible line (exclusive)
   * @param editInfo - Optional edit info for incremental parsing
   * @returns Promise resolving to highlight spans for the range
   */
  async highlightRange(
    content: string,
    startLine: number,
    endLine: number,
    editInfo?: EditInfo | null
  ): Promise<HighlightResult> {
    const id = this.nextId++;
    const request: WorkerRequest = {
      type: "highlightRange",
      id,
      content,
      startLine,
      endLine,
      editInfo,
    };

    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.worker?.postMessage(request);
    });
  }

  /**
   * Cancel all pending highlight requests.
   * Useful when content changes rapidly.
   */
  cancelPending(): void {
    for (const pending of this.pending.values()) {
      pending.reject(new Error("Cancelled"));
    }
    this.pending.clear();
  }

  /**
   * Terminate the worker. Call when done with highlighting.
   */
  dispose(): void {
    this.cancelPending();
    this.worker?.terminate();
    this.worker = null;
  }
}
