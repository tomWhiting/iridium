/**
 * React wrapper component for the Iridium editor.
 *
 * This component provides a declarative React interface to the Iridium editor,
 * handling lifecycle management, event binding, and canvas integration.
 *
 * @module examples/web/src/Iridium
 */

import React, {
  useEffect,
  useRef,
  useImperativeHandle,
  forwardRef,
  useCallback,
  useState,
} from "react";

import type {
  IridiumEditor as IridiumEditorType,
  JsEditorConfig,
  JsPosition,
  JsSelection,
  JsFoldInfo,
} from "iridium-bindings";

// ============================================================================
// Types
// ============================================================================

/**
 * Props for the Iridium editor component.
 */
export interface IridiumProps {
  /** Initial content to load */
  content?: string;

  /** Language for syntax highlighting (e.g., 'rust', 'typescript', 'python') */
  language?: string;

  /** Editor configuration */
  config?: JsEditorConfig;

  /** Whether the editor is read-only */
  readOnly?: boolean;

  /** Whether to use dark theme */
  darkTheme?: boolean;

  /** Callback when content changes */
  onChange?: (content: string) => void;

  /** Callback when selection/cursor changes */
  onSelectionChange?: (selection: JsSelection) => void;

  /** Callback when editor gains focus */
  onFocus?: () => void;

  /** Callback when editor loses focus */
  onBlur?: () => void;

  /** Callback when search results update */
  onSearchUpdate?: (matchCount: number, currentIndex: number | null) => void;

  /** Callback when fold state changes */
  onFoldChange?: (info: JsFoldInfo) => void;

  /** CSS class name for the container */
  className?: string;

  /** Inline styles for the container */
  style?: React.CSSProperties;
}

/**
 * Imperative handle for direct editor control.
 */
export interface IridiumHandle {
  /** Get the underlying editor instance */
  getEditor(): IridiumEditorType | null;

  /** Get current content */
  getContent(): string;

  /** Set content */
  setContent(content: string): void;

  /** Get cursor position */
  getCursor(): JsPosition;

  /** Set cursor position */
  setCursor(line: number, column: number): void;

  /** Focus the editor */
  focus(): void;

  /** Blur the editor */
  blur(): void;

  /** Undo last change */
  undo(): boolean;

  /** Redo last undone change */
  redo(): boolean;

  /** Start a search */
  find(query: string): void;

  /** Go to next match */
  nextMatch(): void;

  /** Go to previous match */
  previousMatch(): void;

  /** Close search */
  closeSearch(): void;

  /** Fold at line */
  foldAt(line: number): boolean;

  /** Unfold at line */
  unfoldAt(line: number): boolean;

  /** Fold all regions */
  foldAll(): void;

  /** Unfold all regions */
  unfoldAll(): void;
}

// ============================================================================
// Component
// ============================================================================

/**
 * Iridium editor React component.
 *
 * A minimal React wrapper for the Iridium GPU-accelerated text editor.
 * The component handles lifecycle management and provides both declarative
 * props and an imperative handle for direct editor control.
 *
 * @example
 * ```tsx
 * import { Iridium, IridiumHandle } from './Iridium';
 *
 * function Editor() {
 *   const editorRef = useRef<IridiumHandle>(null);
 *   const [content, setContent] = useState('');
 *
 *   return (
 *     <Iridium
 *       ref={editorRef}
 *       content={content}
 *       language="typescript"
 *       onChange={setContent}
 *       config={{ tabWidth: 2 }}
 *     />
 *   );
 * }
 * ```
 */
export const Iridium = forwardRef<IridiumHandle, IridiumProps>(function Iridium(
  {
    content,
    language,
    config,
    readOnly = false,
    darkTheme = true,
    onChange,
    onSelectionChange,
    onFocus,
    onBlur,
    onSearchUpdate,
    onFoldChange,
    className,
    style,
  },
  ref
) {
  const containerRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<IridiumEditorType | null>(null);
  const subscriptionsRef = useRef<number[]>([]);
  const [isReady, setIsReady] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Initialize editor
  useEffect(() => {
    let mounted = true;

    async function init() {
      try {
        // Dynamic import to support both SSR and client-side rendering
        const bindings = await import("iridium-bindings");

        // Check WebGPU support
        const supported = await bindings.isWebGPUSupported();
        if (!supported) {
          throw new Error("WebGPU is not supported in this browser");
        }

        if (!mounted) return;

        // Create editor
        const editor = bindings.createEditor(config);
        editorRef.current = editor;

        // Set initial content
        if (content) {
          editor.setContent(content);
        }

        // Set language
        if (language) {
          editor.setLanguage(language);
        }

        // Set read-only state
        editor.setReadOnly(readOnly);

        // Set theme
        if (darkTheme) {
          editor.useDarkTheme();
        } else {
          editor.useLightTheme();
        }

        // Subscribe to events
        const subs: number[] = [];

        if (onChange) {
          subs.push(
            editor.on("contentChanged", (data: string) => {
              onChange(data);
            })
          );
        }

        if (onSelectionChange) {
          subs.push(
            editor.on("selectionChanged", (data: string) => {
              try {
                const selection = JSON.parse(data) as JsSelection;
                onSelectionChange(selection);
              } catch {
                // Ignore parse errors
              }
            })
          );
        }

        if (onFocus) {
          subs.push(editor.on("focus", onFocus));
        }

        if (onBlur) {
          subs.push(editor.on("blur", onBlur));
        }

        if (onSearchUpdate) {
          subs.push(
            editor.on("searchUpdated", () => {
              onSearchUpdate(
                editor.getMatchCount(),
                editor.getCurrentMatchIndex()
              );
            })
          );
        }

        if (onFoldChange) {
          subs.push(
            editor.on("foldChanged", () => {
              onFoldChange(editor.exportFoldInfo());
            })
          );
        }

        subscriptionsRef.current = subs;
        setIsReady(true);
      } catch (e) {
        if (mounted) {
          setError(e instanceof Error ? e.message : String(e));
        }
      }
    }

    init();

    return () => {
      mounted = false;

      // Cleanup subscriptions
      const editor = editorRef.current;
      if (editor) {
        for (const subId of subscriptionsRef.current) {
          editor.off(subId);
        }
        editor.destroy();
        editorRef.current = null;
      }
    };
  }, []); // Only run on mount

  // Update content when prop changes
  useEffect(() => {
    const editor = editorRef.current;
    if (editor && isReady && content !== undefined) {
      const currentContent = editor.getContent();
      if (currentContent !== content) {
        editor.setContent(content);
      }
    }
  }, [content, isReady]);

  // Update language when prop changes
  useEffect(() => {
    const editor = editorRef.current;
    if (editor && isReady && language) {
      editor.setLanguage(language);
    }
  }, [language, isReady]);

  // Update read-only when prop changes
  useEffect(() => {
    const editor = editorRef.current;
    if (editor && isReady) {
      editor.setReadOnly(readOnly);
    }
  }, [readOnly, isReady]);

  // Update theme when prop changes
  useEffect(() => {
    const editor = editorRef.current;
    if (editor && isReady) {
      if (darkTheme) {
        editor.useDarkTheme();
      } else {
        editor.useLightTheme();
      }
    }
  }, [darkTheme, isReady]);

  // Handle resize
  useEffect(() => {
    const container = containerRef.current;
    const editor = editorRef.current;
    if (!container || !editor || !isReady) return;

    const resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const { width, height } = entry.contentRect;
        editor.resize(width, height);
      }
    });

    resizeObserver.observe(container);

    return () => {
      resizeObserver.disconnect();
    };
  }, [isReady]);

  // Imperative handle
  useImperativeHandle(
    ref,
    () => ({
      getEditor: () => editorRef.current,

      getContent: () => editorRef.current?.getContent() ?? "",

      setContent: (newContent: string) => {
        editorRef.current?.setContent(newContent);
      },

      getCursor: () =>
        editorRef.current?.getCursor() ?? { line: 0, column: 0 },

      setCursor: (line: number, column: number) => {
        editorRef.current?.setCursor(line, column);
      },

      focus: () => {
        editorRef.current?.focus();
      },

      blur: () => {
        editorRef.current?.blur();
      },

      undo: () => editorRef.current?.undo() ?? false,

      redo: () => editorRef.current?.redo() ?? false,

      find: (query: string) => {
        editorRef.current?.find(query);
      },

      nextMatch: () => {
        editorRef.current?.nextMatch();
      },

      previousMatch: () => {
        editorRef.current?.previousMatch();
      },

      closeSearch: () => {
        editorRef.current?.closeSearch();
      },

      foldAt: (line: number) => editorRef.current?.foldAt(line) ?? false,

      unfoldAt: (line: number) => editorRef.current?.unfoldAt(line) ?? false,

      foldAll: () => {
        editorRef.current?.foldAll();
      },

      unfoldAll: () => {
        editorRef.current?.unfoldAll();
      },
    }),
    []
  );

  // Render error state
  if (error) {
    return (
      <div
        ref={containerRef}
        className={className}
        style={{
          ...style,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          backgroundColor: "#1a1a1a",
          color: "#ff6b6b",
          padding: "20px",
          fontFamily: "monospace",
        }}
      >
        <div>
          <strong>Iridium Error:</strong> {error}
        </div>
      </div>
    );
  }

  // Render loading state
  if (!isReady) {
    return (
      <div
        ref={containerRef}
        className={className}
        style={{
          ...style,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          backgroundColor: "#1a1a1a",
          color: "#888",
          fontFamily: "monospace",
        }}
      >
        Loading Iridium...
      </div>
    );
  }

  // Render editor container
  // Note: In a full implementation, this would contain a canvas element
  // that the Rust/WebGPU code renders to. For now, it's a placeholder.
  return (
    <div
      ref={containerRef}
      className={className}
      style={{
        width: "100%",
        height: "100%",
        minHeight: "200px",
        backgroundColor: darkTheme ? "#1a1a1a" : "#ffffff",
        ...style,
      }}
      tabIndex={0}
      onFocus={() => editorRef.current?.focus()}
      onBlur={() => editorRef.current?.blur()}
    >
      {/* Canvas will be inserted here by the WebGPU renderer */}
    </div>
  );
});

export default Iridium;
