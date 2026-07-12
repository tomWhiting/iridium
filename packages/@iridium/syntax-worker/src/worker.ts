/**
 * Web Worker for tree-sitter syntax highlighting.
 *
 * Runs tree-sitter parsing completely off the main thread,
 * eliminating any frame drops during typing or scrolling.
 */

import { Parser, Language as TSLanguage, Query, Tree } from "web-tree-sitter";
import type { WorkerRequest, WorkerResponse, HighlightSpan, EditInfo } from "./protocol.ts";

// Import bundled grammars from @iridium/core
import { decodeCoreWasm, decodeGrammar, AVAILABLE_LANGUAGES, HIGHLIGHT_QUERIES } from "@iridium-editor/core/syntax";

interface LanguageData {
  grammar: TSLanguage;
  query: Query;
}

class SyntaxWorker {
  private parser: Parser | null = null;
  private tree: Tree | null = null;
  private languages: Map<string, LanguageData> = new Map();
  private currentLanguage: string = "rust";
  private lastContentHash: number = 0;

  async initialize(defaultLanguage: string = "rust"): Promise<void> {
    const coreWasm = await decodeCoreWasm();

    await Parser.init({
      wasmBinary: coreWasm.buffer,
    });

    this.parser = new Parser();
    await this.loadLanguage(defaultLanguage);

    const data = this.languages.get(defaultLanguage);
    if (data) {
      this.parser.setLanguage(data.grammar);
      this.currentLanguage = defaultLanguage;
    }
  }

  private async loadLanguage(lang: string): Promise<boolean> {
    if (this.languages.has(lang)) {
      return true;
    }

    const wasmBytes = await decodeGrammar(lang);
    if (!wasmBytes) {
      console.error(`[SyntaxWorker] No bundled grammar for: ${lang}`);
      return false;
    }

    const querySource = HIGHLIGHT_QUERIES[lang]?.trim();
    if (!querySource) {
      console.error(`[SyntaxWorker] No highlight query for: ${lang}`);
      return false;
    }

    try {
      const grammar = await TSLanguage.load(wasmBytes);
      const query = new Query(grammar, querySource);
      this.languages.set(lang, { grammar, query });
      return true;
    } catch (e) {
      console.error(`[SyntaxWorker] Failed to load ${lang}:`, e);
      return false;
    }
  }

  async setLanguage(lang: string): Promise<boolean> {
    if (!this.parser) return false;

    if (!(await this.loadLanguage(lang))) {
      return false;
    }

    const data = this.languages.get(lang);
    if (!data) return false;

    this.currentLanguage = lang;
    this.parser.setLanguage(data.grammar);
    this.tree = null;
    this.lastContentHash = 0;
    return true;
  }

  highlight(content: string): HighlightSpan[] {
    if (!this.parser) return [];

    const data = this.languages.get(this.currentLanguage);
    if (!data) return [];

    try {
      this.tree = this.parser.parse(content);
      if (!this.tree) return [];

      const captures = data.query.captures(this.tree.rootNode);

      const spans: HighlightSpan[] = captures.map(
        (c: { node: { startIndex: number; endIndex: number }; name: string }) => ({
          start: c.node.startIndex,
          end: c.node.endIndex,
          type: c.name,
        })
      );

      spans.sort((a, b) => a.start - b.start || a.end - b.end);
      return spans;
    } catch (e) {
      console.error("[SyntaxWorker] Parse error:", e);
      return [];
    }
  }

  highlightIncremental(content: string, edit: EditInfo): HighlightSpan[] {
    if (!this.parser) return [];

    const data = this.languages.get(this.currentLanguage);
    if (!data) return [];

    if (!this.tree) {
      return this.highlight(content);
    }

    try {
      this.tree.edit({
        startIndex: edit.startIndex,
        oldEndIndex: edit.oldEndIndex,
        newEndIndex: edit.newEndIndex,
        startPosition: edit.startPosition,
        oldEndPosition: edit.oldEndPosition,
        newEndPosition: edit.newEndPosition,
      });

      const oldTree = this.tree;
      this.tree = this.parser.parse(content, oldTree);
      if (!this.tree) return this.highlight(content);

      const captures = data.query.captures(this.tree.rootNode);

      const spans: HighlightSpan[] = captures.map(
        (c: { node: { startIndex: number; endIndex: number }; name: string }) => ({
          start: c.node.startIndex,
          end: c.node.endIndex,
          type: c.name,
        })
      );

      spans.sort((a, b) => a.start - b.start || a.end - b.end);
      return spans;
    } catch (e) {
      console.error("[SyntaxWorker] Incremental parse error:", e);
      return this.highlight(content);
    }
  }

  highlightRange(
    content: string,
    startLine: number,
    endLine: number,
    editInfo?: EditInfo | null
  ): HighlightSpan[] {
    if (!this.parser) return [];

    const data = this.languages.get(this.currentLanguage);
    if (!data) return [];

    try {
      const contentHash = this.quickHash(content);
      const contentChanged = contentHash !== this.lastContentHash || !this.tree;

      if (contentChanged) {
        this.lastContentHash = contentHash;

        if (editInfo && this.tree) {
          this.tree.edit({
            startIndex: editInfo.startIndex,
            oldEndIndex: editInfo.oldEndIndex,
            newEndIndex: editInfo.newEndIndex,
            startPosition: editInfo.startPosition,
            oldEndPosition: editInfo.oldEndPosition,
            newEndPosition: editInfo.newEndPosition,
          });
          this.tree = this.parser.parse(content, this.tree);
        } else {
          this.tree = this.parser.parse(content);
        }
      }

      if (!this.tree) return [];

      const startPoint = { row: Math.max(0, startLine), column: 0 };
      const endPoint = { row: endLine, column: 0 };

      const lines = content.split("\n");
      let startByte = 0;
      for (let i = 0; i < startLine && i < lines.length; i++) {
        startByte += lines[i].length + 1;
      }
      let endByte = startByte;
      for (let i = startLine; i < endLine && i < lines.length; i++) {
        endByte += lines[i].length + 1;
      }

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const captures = (data.query as any).captures(this.tree.rootNode, startPoint, endPoint);

      const spans: HighlightSpan[] = [];
      for (const c of captures) {
        const node = c.node;
        if (node.endIndex <= startByte || node.startIndex >= endByte) {
          continue;
        }
        spans.push({
          start: node.startIndex,
          end: node.endIndex,
          type: c.name,
        });
      }

      spans.sort((a, b) => a.start - b.start || a.end - b.end);
      return spans;
    } catch (e) {
      console.error("[SyntaxWorker] Range highlight error:", e);
      return [];
    }
  }

  private quickHash(str: string): number {
    const len = str.length;
    if (len === 0) return 0;
    const sample =
      str.charCodeAt(0) +
      str.charCodeAt(Math.floor(len / 4)) * 31 +
      str.charCodeAt(Math.floor(len / 2)) * 997 +
      str.charCodeAt(Math.floor((3 * len) / 4)) * 7919 +
      str.charCodeAt(len - 1) * 65537 +
      len * 16777619;
    return sample >>> 0;
  }
}

// Worker instance
const worker = new SyntaxWorker();

// Message handler
self.onmessage = async (event: MessageEvent<WorkerRequest>) => {
  const request = event.data;

  try {
    switch (request.type) {
      case "init": {
        await worker.initialize(request.language);
        const response: WorkerResponse = {
          type: "ready",
          languages: [...AVAILABLE_LANGUAGES],
        };
        self.postMessage(response);
        break;
      }

      case "setLanguage": {
        const success = await worker.setLanguage(request.language);
        const response: WorkerResponse = { type: "languageSet", success };
        self.postMessage(response);
        break;
      }

      case "highlight": {
        const t0 = performance.now();
        const spans = worker.highlight(request.content);
        const parseTime = performance.now() - t0;
        const response: WorkerResponse = {
          type: "highlights",
          id: request.id,
          spans,
          parseTime,
        };
        self.postMessage(response);
        break;
      }

      case "highlightIncremental": {
        const t0 = performance.now();
        const spans = worker.highlightIncremental(request.content, request.editInfo);
        const parseTime = performance.now() - t0;
        const response: WorkerResponse = {
          type: "highlights",
          id: request.id,
          spans,
          parseTime,
        };
        self.postMessage(response);
        break;
      }

      case "highlightRange": {
        const t0 = performance.now();
        const spans = worker.highlightRange(
          request.content,
          request.startLine,
          request.endLine,
          request.editInfo
        );
        const parseTime = performance.now() - t0;
        const response: WorkerResponse = {
          type: "highlights",
          id: request.id,
          spans,
          parseTime,
        };
        self.postMessage(response);
        break;
      }
    }
  } catch (e) {
    const response: WorkerResponse = {
      type: "error",
      id: (request as { id?: number }).id ?? 0,
      message: e instanceof Error ? e.message : String(e),
    };
    self.postMessage(response);
  }
};
