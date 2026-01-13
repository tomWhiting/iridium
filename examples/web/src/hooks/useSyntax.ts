/**
 * React hook for syntax highlighting integration.
 */

import { useEffect, useRef, useCallback, useState } from "react";
import { getSyntax, SyntaxHighlighter, type HighlightSpan } from "iridium-bindings/syntax";
import type { WebEditor } from "iridium-bindings";

interface UseSyntaxOptions {
  editor: WebEditor | null;
  language: string;
  enabled?: boolean;
}

interface UseSyntaxResult {
  ready: boolean;
  currentLanguage: string;
  availableLanguages: string[];
  highlight: (content: string) => HighlightSpan[];
}

/**
 * Hook that provides syntax highlighting for an Iridium editor.
 *
 * @example
 * ```tsx
 * const { ready, highlight } = useSyntax({
 *   editor: editorRef.current,
 *   language: 'rust',
 * });
 * ```
 */
export function useSyntax({
  editor,
  language,
  enabled = true,
}: UseSyntaxOptions): UseSyntaxResult {
  const syntaxRef = useRef<SyntaxHighlighter | null>(null);
  const [ready, setReady] = useState(false);
  const [currentLanguage, setCurrentLanguage] = useState(language);
  const [availableLanguages, setAvailableLanguages] = useState<string[]>([]);

  // Initialize syntax highlighter
  useEffect(() => {
    if (!enabled) return;

    let mounted = true;

    async function init() {
      try {
        const syntax = await getSyntax();
        if (!mounted) return;

        syntaxRef.current = syntax;
        setAvailableLanguages(syntax.getAvailableLanguages());
        setReady(true);
      } catch (e) {
        console.error("[useSyntax] Failed to initialize:", e);
      }
    }

    init();

    return () => {
      mounted = false;
    };
  }, [enabled]);

  // Update language when prop changes
  useEffect(() => {
    if (!ready || !syntaxRef.current) return;

    async function updateLanguage() {
      const syntax = syntaxRef.current;
      if (!syntax) return;

      const success = await syntax.setLanguage(language);
      if (success) {
        setCurrentLanguage(language);
      }
    }

    updateLanguage();
  }, [language, ready]);

  // Apply highlights when content changes
  useEffect(() => {
    if (!ready || !editor || !syntaxRef.current || !enabled) return;

    const syntax = syntaxRef.current;

    // Get current content and highlight
    const content = editor.getContent();
    const spans = syntax.highlight(content);

    // Send highlights to editor (method may not exist yet in WASM bindings)
    const editorWithHighlights = editor as WebEditor & { setTreeSitterHighlights?: (spans: HighlightSpan[]) => void };
    if (editorWithHighlights.setTreeSitterHighlights) {
      editorWithHighlights.setTreeSitterHighlights(spans);
    }
  }, [ready, editor, enabled, currentLanguage]);

  const highlight = useCallback(
    (content: string): HighlightSpan[] => {
      if (!ready || !syntaxRef.current) return [];
      return syntaxRef.current.highlight(content);
    },
    [ready]
  );

  return {
    ready,
    currentLanguage,
    availableLanguages,
    highlight,
  };
}

export default useSyntax;
