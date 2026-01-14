/**
 * Tree-sitter syntax highlighting with fully bundled grammars.
 *
 * Everything is embedded as base64 at build time - no runtime HTTP requests.
 * Core WASM, language grammars, and highlight queries are all bundled.
 */

import { Parser, Language as TSLanguage, Query, Tree } from "web-tree-sitter";
import { decodeCoreWasm } from "./core.gen";
import { decodeGrammar, AVAILABLE_LANGUAGES } from "./grammars.gen";
import { HIGHLIGHT_QUERIES } from "./queries";

export { AVAILABLE_LANGUAGES };
export type Language = (typeof AVAILABLE_LANGUAGES)[number];

export interface HighlightSpan {
  start: number;
  end: number;
  type: string;
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
  private languages: Map<string, LanguageData> = new Map();
  private currentLanguage: string = "rust";
  private ready = false;

  /**
   * Initialize tree-sitter and load default language.
   */
  async initialize(defaultLanguage: string = "rust"): Promise<void> {
    // Decode the bundled core WASM
    const coreWasm = decodeCoreWasm();

    // Initialize tree-sitter with the bundled WASM - no CDN needed
    await Parser.init({
      instantiateWasm: async (
        imports: WebAssembly.Imports,
        callback: (instance: WebAssembly.Instance) => void
      ) => {
        const result = await WebAssembly.instantiate(coreWasm, imports);
        callback(result.instance);
      },
    });

    this.parser = new Parser();
    await this.loadLanguage(defaultLanguage);
    this.ready = true;
  }

  /**
   * Load a language grammar from bundled data.
   */
  private async loadLanguage(lang: string): Promise<boolean> {
    if (this.languages.has(lang)) {
      return true;
    }

    const wasmBytes = decodeGrammar(lang);
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
   * Highlight content and return spans.
   */
  highlight(content: string): HighlightSpan[] {
    if (!this.ready || !this.parser) return [];

    const data = this.languages.get(this.currentLanguage);
    if (!data) return [];

    try {
      this.tree = this.parser.parse(content);
      const captures = data.query.captures(this.tree.rootNode);

      const spans: HighlightSpan[] = captures.map((c) => ({
        start: c.node.startIndex,
        end: c.node.endIndex,
        type: c.name,
      }));

      spans.sort((a, b) => a.start - b.start || a.end - b.end);
      return spans;
    } catch (e) {
      console.error("[Syntax] Parse error:", e);
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
