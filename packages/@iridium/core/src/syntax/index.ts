/**
 * Tree-sitter syntax highlighting with fully bundled grammars.
 *
 * Everything is embedded as base64 at build time - no runtime HTTP requests.
 * Core WASM, language grammars, and highlight queries are all bundled.
 */

import { Parser, Language as TSLanguage, Query, Tree } from "web-tree-sitter";
import { decodeCoreWasm } from "./core.gen.ts";
import { decodeGrammar, AVAILABLE_LANGUAGES } from "./grammars.gen.ts";
import { HIGHLIGHT_QUERIES } from "./queries.ts";

export { AVAILABLE_LANGUAGES, decodeCoreWasm, decodeGrammar, HIGHLIGHT_QUERIES };
export type Language = (typeof AVAILABLE_LANGUAGES)[number];

export interface HighlightSpan {
  start: number;
  end: number;
  type: string;
}

export interface ViewportRange {
  startByte: number;
  endByte: number;
}

/**
 * Information about an edit for incremental tree-sitter parsing.
 *
 * Contains byte offsets and row/column positions for the edit region.
 * Used by highlightIncremental() to efficiently update the syntax tree.
 */
export interface EditInfo {
  /** Byte where edit began */
  startIndex: number;
  /** Byte where old content ended */
  oldEndIndex: number;
  /** Byte where new content ends */
  newEndIndex: number;
  /** Position where edit began */
  startPosition: { row: number; column: number };
  /** Position where old content ended */
  oldEndPosition: { row: number; column: number };
  /** Position where new content ends */
  newEndPosition: { row: number; column: number };
}

interface LanguageData {
  grammar: TSLanguage;
  query: Query;
}

/**
 * Syntax highlighter using tree-sitter.
 */
export class SyntaxHighlighter {
  private parser: Parser | null = null;
  private tree: Tree | null = null;
  /** Previous tree for incremental parsing and getChangedRanges */
  private previousTree: Tree | null = null;
  private languages: Map<string, LanguageData> = new Map();
  private currentLanguage: string = "rust";
  private ready = false;
  /** Cached spans from last full highlight - used for incremental updates */
  private cachedSpans: HighlightSpan[] = [];
  /** Last parsed content hash - used to skip reparse when only viewport changed */
  private lastContentHash: number = 0;

  /**
   * Initialize tree-sitter and load default language.
   */
  async initialize(defaultLanguage: string = "rust"): Promise<void> {
    // Decode the bundled core WASM
    const coreWasm = await decodeCoreWasm();

    // Initialize tree-sitter with the bundled WASM binary - no CDN needed
    // wasmBinary is an Emscripten option that provides pre-loaded WASM bytes
    await Parser.init({
      wasmBinary: coreWasm.buffer,
    });

    this.parser = new Parser();
    await this.loadLanguage(defaultLanguage);

    // CRITICAL: Set the parser's language after loading!
    // Without this, parser.parse() returns an empty tree.
    const data = this.languages.get(defaultLanguage);
    if (data) {
      this.parser.setLanguage(data.grammar);
      this.currentLanguage = defaultLanguage;
    }

    this.ready = true;
  }

  /**
   * Load a language grammar from bundled data.
   */
  private async loadLanguage(lang: string): Promise<boolean> {
    if (this.languages.has(lang)) {
      return true;
    }

    const wasmBytes = await decodeGrammar(lang);
    if (!wasmBytes) {
      console.error(`[Syntax] No bundled grammar for: ${lang}`);
      return false;
    }

    const querySource = HIGHLIGHT_QUERIES[lang]?.trim();
    if (!querySource) {
      console.error(`[Syntax] No highlight query for: ${lang}`);
      return false;
    }

    try {
      console.log(`[Syntax] Loading ${lang} grammar from bundle...`);
      const grammar = await TSLanguage.load(wasmBytes);
      const query = new Query(grammar, querySource);
      this.languages.set(lang, { grammar, query });
      console.log(`[Syntax] ${lang} loaded successfully`);
      return true;
    } catch (e) {
      console.error(`[Syntax] Failed to load ${lang}:`, e);
      return false;
    }
  }

  /**
   * Set the current language for highlighting.
   */
  async setLanguage(lang: string): Promise<boolean> {
    if (!this.ready || !this.parser) return false;

    if (!(await this.loadLanguage(lang))) {
      return false;
    }

    const data = this.languages.get(lang);
    if (!data) return false;

    this.currentLanguage = lang;
    this.parser.setLanguage(data.grammar);
    this.tree = null;
    this.previousTree = null;
    this.cachedSpans = [];
    return true;
  }

  /**
   * Get current language.
   */
  getCurrentLanguage(): string {
    return this.currentLanguage;
  }

  /**
   * Get available languages.
   */
  getAvailableLanguages(): string[] {
    return [...AVAILABLE_LANGUAGES];
  }

  /**
   * Check if ready.
   */
  isReady(): boolean {
    return this.ready;
  }

  /**
   * Highlight content and return spans (full reparse).
   *
   * Use highlightIncremental() when edit info is available for better performance.
   */
  highlight(content: string): HighlightSpan[] {
    if (!this.ready || !this.parser) return [];

    const data = this.languages.get(this.currentLanguage);
    if (!data) return [];

    try {
      this.previousTree = this.tree;
      this.tree = this.parser.parse(content);
      if (!this.tree) return [];
      const captures = data.query.captures(this.tree.rootNode);

      const spans: HighlightSpan[] = captures.map((c: { node: { startIndex: number; endIndex: number }; name: string }) => ({
        start: c.node.startIndex,
        end: c.node.endIndex,
        type: c.name,
      }));

      spans.sort((a, b) => a.start - b.start || a.end - b.end);

      // Cache spans for incremental updates
      this.cachedSpans = spans;

      return spans;
    } catch (e) {
      console.error("[Syntax] Parse error:", e);
      return [];
    }
  }

  /**
   * Update syntax tree with edit and re-highlight.
   *
   * This is more efficient than highlight() for single edits
   * as it reuses unchanged portions of the syntax tree.
   *
   * @param content - Full document content after edit
   * @param edit - Information about the edit that occurred
   * @returns Array of highlight spans for the document
   */
  highlightIncremental(content: string, edit: EditInfo): HighlightSpan[] {
    if (!this.ready || !this.parser) return [];

    const data = this.languages.get(this.currentLanguage);
    if (!data) return [];

    // If no previous tree or no cached spans, fall back to full parse
    if (!this.tree || this.cachedSpans.length === 0) {
      return this.highlight(content);
    }

    try {
      // Apply edit to existing tree (T026)
      this.tree.edit({
        startIndex: edit.startIndex,
        oldEndIndex: edit.oldEndIndex,
        newEndIndex: edit.newEndIndex,
        startPosition: edit.startPosition,
        oldEndPosition: edit.oldEndPosition,
        newEndPosition: edit.newEndPosition,
      });

      // Store previous tree for getChangedRanges
      this.previousTree = this.tree;

      // Incremental parse
      this.tree = this.parser.parse(content, this.previousTree);
      if (!this.tree || !this.previousTree) return this.highlight(content);

      // Get changed ranges
      const changedRanges = this.tree.getChangedRanges(this.previousTree);

      // If no changes detected or too many, do full reparse
      if (changedRanges.length === 0 || changedRanges.length > 20) {
        return this.highlight(content);
      }

      // Calculate the byte offset adjustment from the edit
      const byteDelta = edit.newEndIndex - edit.oldEndIndex;

      // Find the affected byte range (union of all changed ranges)
      let affectedStart = Infinity;
      let affectedEnd = 0;
      for (const range of changedRanges) {
        affectedStart = Math.min(affectedStart, range.startIndex);
        affectedEnd = Math.max(affectedEnd, range.endIndex);
      }

      // Expand affected range for context (multi-line constructs)
      affectedStart = Math.max(0, affectedStart - 500);
      affectedEnd = affectedEnd + 500;

      // Remove spans that overlap with the affected region
      // Also adjust positions of spans after the edit point
      const keptSpans: HighlightSpan[] = [];
      for (const span of this.cachedSpans) {
        // Span is entirely before the edit - keep as is
        if (span.end <= edit.startIndex) {
          keptSpans.push(span);
        }
        // Span is entirely after the affected region - adjust position
        else if (span.start >= affectedEnd - byteDelta) {
          keptSpans.push({
            start: span.start + byteDelta,
            end: span.end + byteDelta,
            type: span.type,
          });
        }
        // Span overlaps with affected region - will be replaced by new query
      }

      // Query new spans for the affected region
      // Note: tree-sitter types may not include all overloads, cast to any
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const newCaptures = (data.query as any).captures(
        this.tree!.rootNode,
        { row: 0, column: affectedStart },
        { row: 999999, column: affectedEnd }
      );

      const newSpans: HighlightSpan[] = newCaptures.map((c: { node: { startIndex: number; endIndex: number }; name: string }) => ({
        start: c.node.startIndex,
        end: c.node.endIndex,
        type: c.name,
      }));

      // Merge kept spans with new spans
      const allSpans = [...keptSpans, ...newSpans];

      // Deduplicate and sort
      const seen = new Set<string>();
      const uniqueSpans = allSpans.filter(s => {
        const key = `${s.start}:${s.end}:${s.type}`;
        if (seen.has(key)) return false;
        seen.add(key);
        return true;
      });

      uniqueSpans.sort((a, b) => a.start - b.start || a.end - b.end);

      // Update cache
      this.cachedSpans = uniqueSpans;

      return uniqueSpans;
    } catch (e) {
      console.error("[Syntax] Incremental parse error, falling back:", e);
      return this.highlight(content);
    }
  }

  /**
   * Highlight only a range of lines (for large file viewport optimization).
   *
   * This dramatically reduces work by only querying spans in the visible
   * region instead of the entire document.
   *
   * @param content - Full document content
   * @param startLine - First line to highlight (0-indexed)
   * @param endLine - Last line to highlight (exclusive)
   * @param editInfo - Optional edit info for incremental parsing
   * @returns Array of highlight spans in the specified range
   */
  highlightRange(content: string, startLine: number, endLine: number, editInfo?: EditInfo | null): HighlightSpan[] {
    if (!this.ready || !this.parser) return [];

    const data = this.languages.get(this.currentLanguage);
    if (!data) return [];

    try {
      // Simple hash to detect content changes
      const contentHash = this.quickHash(content);
      const contentChanged = contentHash !== this.lastContentHash || !this.tree;

      if (contentChanged) {
          this.lastContentHash = contentHash;

        // Use incremental parsing if we have edit info and existing tree
        if (editInfo && this.tree) {
          // Apply edit to existing tree for incremental parse
          this.tree.edit({
            startIndex: editInfo.startIndex,
            oldEndIndex: editInfo.oldEndIndex,
            newEndIndex: editInfo.newEndIndex,
            startPosition: editInfo.startPosition,
            oldEndPosition: editInfo.oldEndPosition,
            newEndPosition: editInfo.newEndPosition,
          });
          this.tree = this.parser.parse(content, this.tree);
          console.log(`[Syntax] highlightRange: INCREMENTAL lines ${startLine}-${endLine}`);
        } else {
          // Full reparse - no edit info or no existing tree
          this.previousTree = null;
          this.cachedSpans = [];
          this.tree = this.parser.parse(content);
          console.log(`[Syntax] highlightRange: REPARSE lines ${startLine}-${endLine}`);
        }
      } else {
        // Content same - reuse existing tree, just query new viewport
        console.log(`[Syntax] highlightRange: REUSE lines ${startLine}-${endLine}`);
      }

      // Query only the specified line range
      // Note: tree-sitter Point uses 0-indexed row/column
      const startPoint = { row: Math.max(0, startLine), column: 0 };
      const endPoint = { row: endLine, column: 0 };

      // Calculate byte offsets for the visible range
      const lines = content.split('\n');
      let startByte = 0;
      for (let i = 0; i < startLine && i < lines.length; i++) {
        startByte += lines[i].length + 1; // +1 for newline
      }
      let endByte = startByte;
      for (let i = startLine; i < endLine && i < lines.length; i++) {
        endByte += lines[i].length + 1;
      }

      // Note: tree-sitter types may not include all overloads
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const captures = (data.query as any).captures(this.tree!.rootNode, startPoint, endPoint);

      // Filter captures to only those that overlap with the visible byte range
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
      console.error("[Syntax] Range highlight error:", e);
      return [];
    }
  }

  /**
   * Quick hash for content change detection.
   * Not cryptographic, just for fast equality check.
   */
  private quickHash(str: string): number {
    const len = str.length;
    if (len === 0) return 0;
    // Sample a few characters + length for fast hash
    const sample = str.charCodeAt(0) +
      str.charCodeAt(Math.floor(len / 4)) * 31 +
      str.charCodeAt(Math.floor(len / 2)) * 997 +
      str.charCodeAt(Math.floor(3 * len / 4)) * 7919 +
      str.charCodeAt(len - 1) * 65537 +
      len * 16777619;
    return sample >>> 0; // Convert to unsigned 32-bit
  }

  /**
   * Get byte ranges that changed between old and new parse.
   *
   * Useful for targeted span updates instead of full regeneration.
   *
   * @returns Array of {startIndex, endIndex} ranges
   */
  getChangedRanges(): Array<{ startIndex: number; endIndex: number }> {
    if (!this.tree || !this.previousTree) {
      return [];
    }

    try {
      const ranges = this.tree.getChangedRanges(this.previousTree);
      return ranges.map((r: { startIndex: number; endIndex: number }) => ({
        startIndex: r.startIndex,
        endIndex: r.endIndex,
      }));
    } catch (e) {
      console.error("[Syntax] getChangedRanges error:", e);
      return [];
    }
  }
}

// Singleton instance
let instance: SyntaxHighlighter | null = null;

/**
 * Get or create the syntax highlighter instance.
 */
export async function getSyntax(): Promise<SyntaxHighlighter> {
  if (!instance) {
    instance = new SyntaxHighlighter();
    await instance.initialize();
  }
  return instance;
}
