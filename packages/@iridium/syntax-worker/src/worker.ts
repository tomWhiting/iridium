/**
 * Web Worker for tree-sitter syntax highlighting.
 *
 * Runs tree-sitter parsing completely off the main thread,
 * eliminating any frame drops during typing or scrolling.
 */

import { Parser, Language as TSLanguage, Query, Tree } from "web-tree-sitter";
import type { WorkerRequest, WorkerResponse, HighlightSpan, EditInfo } from "./protocol.ts";
import { convertSpansToUtf8, convertEditInfo } from "./encoding.ts";

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
  /**
   * The exact document string that produced {@link tree}, kept so incremental
   * edits can convert the Rust core's rope-byte edit info against the *pre-edit*
   * text (see {@link convertEditInfo}). Held in lock-step with `tree`: set to
   * the parsed content whenever `tree` is (re)assigned from a parse, and cleared
   * to `null` whenever `tree` is discarded. When it is `null` no valid pre-edit
   * text exists and callers must do a full (non-incremental) parse.
   */
  private lastContent: string | null = null;

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
    this.lastContent = null;
    return true;
  }

  highlight(content: string): HighlightSpan[] {
    if (!this.parser) return [];

    const data = this.languages.get(this.currentLanguage);
    if (!data) return [];

    try {
      this.tree = this.parser.parse(content);
      if (!this.tree) return [];
      // Keep the parsed text so the next incremental edit can convert
      // rope-byte edit info against this pre-edit content.
      this.lastContent = content;

      const captures = data.query.captures(this.tree.rootNode);

      const spans: HighlightSpan[] = captures.map(
        (c: { node: { startIndex: number; endIndex: number }; name: string }) => ({
          start: c.node.startIndex,
          end: c.node.endIndex,
          type: c.name,
        })
      );

      spans.sort((a, b) => a.start - b.start || a.end - b.end);
      // tree-sitter reports UTF-16 code-unit offsets; the rope indexes by
      // UTF-8 byte. Convert before handing spans back to the Rust core.
      return convertSpansToUtf8(content, spans);
    } catch (e) {
      // A failed parse must not leave a stale tree/lastContent pair behind:
      // the controller's next edit is relative to THIS request's content.
      this.tree = null;
      this.lastContent = null;
      console.error("[SyntaxWorker] Parse error:", e);
      return [];
    }
  }

  highlightIncremental(content: string, edit: EditInfo): HighlightSpan[] {
    if (!this.parser) return [];

    const data = this.languages.get(this.currentLanguage);
    if (!data) return [];

    // Without both the old tree and the pre-edit text, the byte->UTF-16 edit
    // conversion has no reference document; fall back to a full parse.
    if (!this.tree || this.lastContent === null) {
      return this.highlight(content);
    }

    try {
      // The edit arrives in rope bytes / byte columns; tree-sitter needs
      // UTF-16 code units, converted against the pre- and post-edit text.
      const tsEdit = convertEditInfo(this.lastContent, content, edit);
      this.tree.edit({
        startIndex: tsEdit.startIndex,
        oldEndIndex: tsEdit.oldEndIndex,
        newEndIndex: tsEdit.newEndIndex,
        startPosition: tsEdit.startPosition,
        oldEndPosition: tsEdit.oldEndPosition,
        newEndPosition: tsEdit.newEndPosition,
      });

      const oldTree = this.tree;
      this.tree = this.parser.parse(content, oldTree);
      if (!this.tree) return this.highlight(content);
      this.lastContent = content;

      const captures = data.query.captures(this.tree.rootNode);

      const spans: HighlightSpan[] = captures.map(
        (c: { node: { startIndex: number; endIndex: number }; name: string }) => ({
          start: c.node.startIndex,
          end: c.node.endIndex,
          type: c.name,
        })
      );

      spans.sort((a, b) => a.start - b.start || a.end - b.end);
      return convertSpansToUtf8(content, spans);
    } catch (e) {
      // tree.edit() may already have mutated the old tree; drop the pair and
      // recover with a clean full parse of the current content.
      this.tree = null;
      this.lastContent = null;
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
      // Content identity must be exact: a sampled hash once skipped
      // same-length edits at unsampled positions, leaving the tree and
      // lastContent describing stale text that later edit-info conversions
      // ran against. String equality early-exits on the first difference,
      // which is cheap next to a parse.
      const contentChanged = !this.tree || this.lastContent !== content;

      if (contentChanged) {
        // Incremental reuse needs the old tree AND the pre-edit text (to
        // convert the rope-byte edit into tree-sitter's UTF-16 space);
        // otherwise re-parse from scratch.
        if (editInfo && this.tree && this.lastContent !== null) {
          const tsEdit = convertEditInfo(this.lastContent, content, editInfo);
          this.tree.edit({
            startIndex: tsEdit.startIndex,
            oldEndIndex: tsEdit.oldEndIndex,
            newEndIndex: tsEdit.newEndIndex,
            startPosition: tsEdit.startPosition,
            oldEndPosition: tsEdit.oldEndPosition,
            newEndPosition: tsEdit.newEndPosition,
          });
          this.tree = this.parser.parse(content, this.tree);
        } else {
          this.tree = this.parser.parse(content);
        }
        this.lastContent = this.tree ? content : null;
      }

      if (!this.tree) return [];

      const startPoint = { row: Math.max(0, startLine), column: 0 };
      const endPoint = { row: endLine, column: 0 };

      // Range bounds are in tree-sitter's UTF-16 code-unit space (JS
      // `string.length` per line), matching node.startIndex/endIndex, so the
      // overlap test below is done entirely in UTF-16 before conversion.
      const lines = content.split("\n");
      let startUnit = 0;
      for (let i = 0; i < startLine && i < lines.length; i++) {
        startUnit += lines[i].length + 1;
      }
      let endUnit = startUnit;
      for (let i = startLine; i < endLine && i < lines.length; i++) {
        endUnit += lines[i].length + 1;
      }

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const captures = (data.query as any).captures(this.tree.rootNode, startPoint, endPoint);

      const spans: HighlightSpan[] = [];
      for (const c of captures) {
        const node = c.node;
        if (node.endIndex <= startUnit || node.startIndex >= endUnit) {
          continue;
        }
        spans.push({
          start: node.startIndex,
          end: node.endIndex,
          type: c.name,
        });
      }

      spans.sort((a, b) => a.start - b.start || a.end - b.end);
      // Convert to UTF-8 byte offsets for the rope only once, on the filtered
      // viewport spans.
      return convertSpansToUtf8(content, spans);
    } catch (e) {
      // Same discipline as the other paths: never keep a possibly
      // edit-adjusted tree paired with text it no longer describes.
      this.tree = null;
      this.lastContent = null;
      console.error("[SyntaxWorker] Range highlight error:", e);
      return [];
    }
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
