/**
 * React wrapper component for the Iridium WebGPU editor.
 *
 * Uses the high-level IridiumEditor controller which handles all event binding,
 * syntax highlighting, and rendering internally.
 */

import React, {
  useEffect,
  useRef,
  useImperativeHandle,
  forwardRef,
  useState,
} from "react";

// Import from the new @iridium/core package
import { IridiumEditor, type EditorState } from "@iridium/core";

// ============================================================================
// Types
// ============================================================================

export interface IridiumProps {
  content?: string;
  language?: string;
  darkTheme?: boolean;
  onChange?: (content: string) => void;
  onSelectionChange?: (selection: { head: { line: number; column: number } }) => void;
  className?: string;
  style?: React.CSSProperties;
}

export interface IridiumHandle {
  getContent(): string;
  setContent(content: string): void;
  getState(): EditorState;
  undo(): boolean;
  redo(): boolean;
  foldAll(): void;
  unfoldAll(): void;
  toggleFold(line: number): boolean;
  setLanguage(language: string): Promise<boolean>;
  focus(): void;
}

// ============================================================================
// Component
// ============================================================================

export const Iridium = forwardRef<IridiumHandle, IridiumProps>(function Iridium(
  {
    content,
    language = "rust",
    darkTheme = true,
    onChange,
    onSelectionChange,
    className,
    style,
  },
  ref
) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<IridiumEditor | null>(null);
  const [isReady, setIsReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const lastContentRef = useRef<string>("");
  const lastLanguageRef = useRef<string>(language);

  // Initialize editor
  useEffect(() => {
    let mounted = true;

    async function init() {
      const canvas = canvasRef.current;
      if (!canvas) return;

      try {
        // Create editor using the high-level controller
        const editor = await IridiumEditor.create(canvas, {
          content: content ?? "",
          language,
          darkTheme,
          onChange: (newContent) => {
            lastContentRef.current = newContent;
            onChange?.(newContent);
          },
          onSelectionChange: (info) => {
            onSelectionChange?.({
              head: { line: info.line, column: info.column },
            });
          },
        });

        if (!mounted) {
          editor.destroy();
          return;
        }

        editorRef.current = editor;
        lastContentRef.current = content ?? "";
        lastLanguageRef.current = language;
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
      editorRef.current?.destroy();
    };
  }, []);

  // Update content when prop changes
  useEffect(() => {
    const editor = editorRef.current;
    if (editor && isReady && content !== undefined && content !== lastContentRef.current) {
      editor.setContent(content);
      lastContentRef.current = content;
    }
  }, [content, isReady]);

  // Update language when prop changes
  useEffect(() => {
    const editor = editorRef.current;
    if (editor && isReady && language !== lastLanguageRef.current) {
      editor.setLanguage(language).then(() => {
        lastLanguageRef.current = language;
      });
    }
  }, [language, isReady]);

  // Update theme when prop changes
  useEffect(() => {
    const editor = editorRef.current;
    if (editor && isReady) {
      editor.setTheme(darkTheme);
    }
  }, [darkTheme, isReady]);

  // Imperative handle
  useImperativeHandle(
    ref,
    () => ({
      getContent: () => editorRef.current?.getContent() ?? "",
      setContent: (newContent: string) => {
        if (editorRef.current) {
          editorRef.current.setContent(newContent);
          lastContentRef.current = newContent;
        }
      },
      getState: () => editorRef.current?.getState() ?? {
        line: 0,
        column: 0,
        lineCount: 0,
        hasSelection: false,
        canUndo: false,
        canRedo: false,
        language: "",
        foldedLines: [],
        hiddenLineCount: 0,
      },
      undo: () => editorRef.current?.undo() ?? false,
      redo: () => editorRef.current?.redo() ?? false,
      foldAll: () => editorRef.current?.foldAll(),
      unfoldAll: () => editorRef.current?.unfoldAll(),
      toggleFold: (line: number) => editorRef.current?.toggleFold(line) ?? false,
      setLanguage: async (lang: string) => editorRef.current?.setLanguage(lang) ?? false,
      focus: () => editorRef.current?.focus(),
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

  return (
    <div
      ref={containerRef}
      className={className}
      style={{
        width: "100%",
        height: "100%",
        minHeight: "200px",
        position: "relative",
        ...style,
      }}
    >
      <canvas
        ref={canvasRef}
        tabIndex={0}
        style={{
          width: "100%",
          height: "100%",
          display: "block",
          outline: "none",
          cursor: "text",
        }}
      />
      {!isReady && (
        <div
          style={{
            position: "absolute",
            inset: 0,
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
      )}
    </div>
  );
});

export default Iridium;
