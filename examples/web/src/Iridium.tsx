/**
 * React wrapper component for the Iridium WebGPU editor.
 *
 * Uses the WASM/WebGPU browser API with canvas rendering.
 * Includes full keyboard handling, mouse selection, and syntax highlighting.
 */

import React, {
  useEffect,
  useRef,
  useImperativeHandle,
  forwardRef,
  useState,
} from "react";

import type { WebEditor } from "iridium-bindings";
import { getSyntax, type SyntaxHighlighter } from "iridium-bindings/syntax";

// Extended type for methods not yet in the d.ts file
type WebEditorExtended = WebEditor & {
  scrollBy(delta: number): void;
  foldAll(): void;
  unfoldAll(): void;
  toggleFold(line: number): boolean;
  delete_forward(): void;
  pixelToPosition(x: number, y: number): number[];
  setCursorFromClick(line: number, column: number): void;
  extendSelectionToPosition(line: number, column: number): void;
  extendSelectionLeft(): void;
  extendSelectionRight(): void;
  extendSelectionUp(): void;
  extendSelectionDown(): void;
  extendSelectionWordLeft(): void;
  extendSelectionWordRight(): void;
  extendSelectionLineStart(): void;
  extendSelectionLineEnd(): void;
  extendSelectionDocStart(): void;
  extendSelectionDocEnd(): void;
  moveCursorWordLeft(): void;
  moveCursorWordRight(): void;
  moveCursorLineStart(): void;
  moveCursorLineEnd(): void;
  moveCursorDocStart(): void;
  moveCursorDocEnd(): void;
  deleteWordBackward(): void;
  deleteWordForward(): void;
  deleteToLineStart(): void;
  deleteToLineEnd(): void;
  selectAll(): void;
  getSelectedText(): string;
  hasSelection(): boolean;
  ensureCursorVisible(): void;
  setTreeSitterHighlights(spans: { start: number; end: number; type: string }[]): void;
  isTreeSitterActive(): boolean;
  isSyntaxEnabled(): boolean;
  setSyntaxEnabled(enabled: boolean): void;
  isGutterEnabled(): boolean;
  setGutterEnabled(enabled: boolean): void;
  isFoldable(line: number): boolean;
  isFolded(line: number): boolean;
  getFoldableLines(): number[];
  getFoldedLines(): number[];
  getHiddenLineCount(): number;
};

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
  undo(): boolean;
  redo(): boolean;
  foldAll(): void;
  unfoldAll(): void;
  toggleFold(line: number): boolean;
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
  const editorRef = useRef<WebEditorExtended | null>(null);
  const syntaxRef = useRef<SyntaxHighlighter | null>(null);
  const [isReady, setIsReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const animationFrameRef = useRef<number>(0);
  const lastContentRef = useRef<string>("");
  const isDraggingRef = useRef(false);
  const currentLanguageRef = useRef(language);

  // Update highlights using tree-sitter
  const updateHighlights = () => {
    const editor = editorRef.current;
    const syntax = syntaxRef.current;
    if (!editor || !syntax) return;

    try {
      const editorContent = editor.getContent();
      const spans = syntax.highlight(editorContent);
      editor.setTreeSitterHighlights(spans);
    } catch (e) {
      console.error("Failed to update highlights:", e);
    }
  };

  // Initialize editor
  useEffect(() => {
    let mounted = true;

    async function init() {
      const canvas = canvasRef.current;
      if (!canvas) return;

      try {
        // Import and initialize WASM module via alias
        const bindings = await import("iridium-wasm");
        await bindings.default();

        if (!mounted) return;

        // Set canvas size
        const rect = canvas.getBoundingClientRect();
        const pixelRatio = window.devicePixelRatio || 1;
        canvas.width = rect.width * pixelRatio;
        canvas.height = rect.height * pixelRatio;

        // Create editor with canvas
        const editor = await bindings.createWebEditor(canvas, pixelRatio) as unknown as WebEditorExtended;
        editorRef.current = editor;

        // Load font from CDN (required for WASM - no system fonts)
        const fontUrl = "https://cdn.jsdelivr.net/npm/firacode@6.2.0/distr/ttf/FiraCode-Regular.ttf";
        const fontResponse = await fetch(fontUrl);
        if (fontResponse.ok) {
          const fontData = await fontResponse.arrayBuffer();
          editor.loadFont(new Uint8Array(fontData));
        }

        // Initialize tree-sitter syntax highlighting
        try {
          const syntax = await getSyntax();
          syntaxRef.current = syntax;
          await syntax.setLanguage(language);
          currentLanguageRef.current = language;
        } catch (e) {
          console.warn("Tree-sitter syntax highlighting not available:", e);
        }

        // Set initial content
        if (content) {
          editor.setContent(content);
          lastContentRef.current = content;
        }

        // Set theme
        editor.setDarkTheme(darkTheme);

        // Apply initial highlights
        updateHighlights();

        // Initial render
        editor.forceRender();

        if (mounted) {
          setIsReady(true);
        }

        // Animation loop for cursor blinking
        let lastRenderTime = 0;
        const BLINK_INTERVAL = 500;

        function animate(currentTime: number) {
          if (!mounted || !editorRef.current) return;

          if (currentTime - lastRenderTime >= BLINK_INTERVAL) {
            editorRef.current.forceRender();
            lastRenderTime = currentTime;
          }
          animationFrameRef.current = requestAnimationFrame(animate);
        }
        animationFrameRef.current = requestAnimationFrame(animate);

      } catch (e) {
        if (mounted) {
          setError(e instanceof Error ? e.message : String(e));
        }
      }
    }

    init();

    return () => {
      mounted = false;
      if (animationFrameRef.current) {
        cancelAnimationFrame(animationFrameRef.current);
      }
    };
  }, []);

  // Update content when prop changes
  useEffect(() => {
    const editor = editorRef.current;
    if (editor && isReady && content !== undefined && content !== lastContentRef.current) {
      editor.setContent(content);
      lastContentRef.current = content;
      updateHighlights();
      editor.forceRender();
    }
  }, [content, isReady]);

  // Update language when prop changes
  useEffect(() => {
    const syntax = syntaxRef.current;
    if (syntax && isReady && language !== currentLanguageRef.current) {
      syntax.setLanguage(language).then(() => {
        currentLanguageRef.current = language;
        updateHighlights();
        editorRef.current?.forceRender();
      });
    }
  }, [language, isReady]);

  // Update theme when prop changes
  useEffect(() => {
    const editor = editorRef.current;
    if (editor && isReady) {
      editor.setDarkTheme(darkTheme);
      editor.forceRender();
    }
  }, [darkTheme, isReady]);

  // Handle resize
  useEffect(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    const editor = editorRef.current;
    if (!canvas || !container || !editor || !isReady) return;

    const resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const { width, height } = entry.contentRect;
        const pixelRatio = window.devicePixelRatio || 1;
        canvas.width = width * pixelRatio;
        canvas.height = height * pixelRatio;
        editor.resize(canvas.width, canvas.height);
        editor.forceRender();
      }
    });

    resizeObserver.observe(container);

    return () => {
      resizeObserver.disconnect();
    };
  }, [isReady]);

  // Helper to get mouse position in editor coordinates
  const getMousePosition = (e: MouseEvent): [number, number] => {
    const canvas = canvasRef.current;
    const editor = editorRef.current;
    if (!canvas || !editor) return [0, 0];

    const rect = canvas.getBoundingClientRect();
    const x = (e.clientX - rect.left) * window.devicePixelRatio;
    const y = (e.clientY - rect.top) * window.devicePixelRatio;
    const pos = editor.pixelToPosition(x, y);
    return [pos[0], pos[1]];
  };

  // Notify parent of content changes
  const notifyContentChange = () => {
    const editor = editorRef.current;
    if (onChange && editor) {
      const newContent = editor.getContent();
      lastContentRef.current = newContent;
      onChange(newContent);
    }
  };

  // Notify parent of selection changes
  const notifySelectionChange = () => {
    const editor = editorRef.current;
    if (onSelectionChange && editor) {
      onSelectionChange({
        head: {
          line: editor.getCursorLine(),
          column: editor.getCursorColumn(),
        },
      });
    }
  };

  // Handle mouse events
  useEffect(() => {
    const canvas = canvasRef.current;
    const editor = editorRef.current;
    if (!canvas || !editor || !isReady) return;

    function handleMouseDown(e: MouseEvent) {
      if (!editor) return;
      canvas?.focus();
      const [line, column] = getMousePosition(e);

      if (e.shiftKey) {
        editor.extendSelectionToPosition(line, column);
      } else {
        editor.setCursorFromClick(line, column);
        isDraggingRef.current = true;
      }
      editor.forceRender();
      notifySelectionChange();
    }

    function handleMouseMove(e: MouseEvent) {
      if (!editor || !isDraggingRef.current) return;
      const [line, column] = getMousePosition(e);
      editor.extendSelectionToPosition(line, column);
      editor.forceRender();
      notifySelectionChange();
    }

    function handleMouseUp() {
      isDraggingRef.current = false;
    }

    function handleWheel(e: WheelEvent) {
      e.preventDefault();
      if (editor) {
        editor.scrollBy(e.deltaY);
        editor.forceRender();
      }
    }

    canvas.addEventListener("mousedown", handleMouseDown);
    canvas.addEventListener("mousemove", handleMouseMove);
    canvas.addEventListener("mouseup", handleMouseUp);
    document.addEventListener("mouseup", handleMouseUp);
    canvas.addEventListener("wheel", handleWheel, { passive: false });

    return () => {
      canvas.removeEventListener("mousedown", handleMouseDown);
      canvas.removeEventListener("mousemove", handleMouseMove);
      canvas.removeEventListener("mouseup", handleMouseUp);
      document.removeEventListener("mouseup", handleMouseUp);
      canvas.removeEventListener("wheel", handleWheel);
    };
  }, [isReady, onChange, onSelectionChange]);

  // Handle keyboard input
  useEffect(() => {
    const canvas = canvasRef.current;
    const editor = editorRef.current;
    if (!canvas || !editor || !isReady) return;

    function handleKeyDown(e: KeyboardEvent) {
      if (!editor) return;

      let handled = true;
      const isMac = navigator.platform.toUpperCase().indexOf("MAC") >= 0;
      const selecting = e.shiftKey;

      // Arrow keys with modifiers
      if (e.key === "ArrowLeft") {
        if (e.metaKey && isMac) {
          selecting ? editor.extendSelectionLineStart() : editor.moveCursorLineStart();
        } else if (e.altKey) {
          selecting ? editor.extendSelectionWordLeft() : editor.moveCursorWordLeft();
        } else if (e.ctrlKey && !isMac) {
          selecting ? editor.extendSelectionWordLeft() : editor.moveCursorWordLeft();
        } else {
          selecting ? editor.extendSelectionLeft() : editor.moveCursorLeft();
        }
      } else if (e.key === "ArrowRight") {
        if (e.metaKey && isMac) {
          selecting ? editor.extendSelectionLineEnd() : editor.moveCursorLineEnd();
        } else if (e.altKey) {
          selecting ? editor.extendSelectionWordRight() : editor.moveCursorWordRight();
        } else if (e.ctrlKey && !isMac) {
          selecting ? editor.extendSelectionWordRight() : editor.moveCursorWordRight();
        } else {
          selecting ? editor.extendSelectionRight() : editor.moveCursorRight();
        }
      } else if (e.key === "ArrowUp") {
        if (e.metaKey && isMac) {
          selecting ? editor.extendSelectionDocStart() : editor.moveCursorDocStart();
        } else {
          selecting ? editor.extendSelectionUp() : editor.moveCursorUp();
        }
      } else if (e.key === "ArrowDown") {
        if (e.metaKey && isMac) {
          selecting ? editor.extendSelectionDocEnd() : editor.moveCursorDocEnd();
        } else {
          selecting ? editor.extendSelectionDown() : editor.moveCursorDown();
        }
      } else if (e.key === "Home") {
        if (e.metaKey || e.ctrlKey) {
          selecting ? editor.extendSelectionDocStart() : editor.moveCursorDocStart();
        } else {
          selecting ? editor.extendSelectionLineStart() : editor.moveCursorLineStart();
        }
      } else if (e.key === "End") {
        if (e.metaKey || e.ctrlKey) {
          selecting ? editor.extendSelectionDocEnd() : editor.moveCursorDocEnd();
        } else {
          selecting ? editor.extendSelectionLineEnd() : editor.moveCursorLineEnd();
        }
      } else if (e.key === "a" && (e.metaKey || e.ctrlKey)) {
        editor.selectAll();
      } else if (e.key === "Backspace") {
        if (e.metaKey && isMac) {
          editor.deleteToLineStart();
        } else if (e.altKey) {
          editor.deleteWordBackward();
        } else if (e.ctrlKey && !isMac) {
          editor.deleteWordBackward();
        } else {
          // Check for auto-pair deletion
          const content = editor.getContent();
          const lines = content.split("\n");
          const line = editor.getCursorLine();
          const col = editor.getCursorColumn();
          const currentLine = lines[line] || "";
          const charBefore = col > 0 ? currentLine[col - 1] : "";
          const charAfter = currentLine[col] || "";
          const pairMap: Record<string, string> = { "(": ")", "[": "]", "{": "}", '"': '"', "'": "'" };

          if (pairMap[charBefore] && charAfter === pairMap[charBefore]) {
            editor.backspace();
            editor.delete_forward();
          } else {
            editor.backspace();
          }
        }
        notifyContentChange();
      } else if (e.key === "Delete") {
        if (e.altKey) {
          editor.deleteWordForward();
        } else if (e.ctrlKey && !isMac) {
          editor.deleteWordForward();
        } else {
          editor.delete_forward();
        }
        notifyContentChange();
      } else if (e.key === "k" && e.ctrlKey) {
        editor.deleteToLineEnd();
        notifyContentChange();
      } else if (e.key === "Enter") {
        const content = editor.getContent();
        const lines = content.split("\n");
        const line = editor.getCursorLine();
        const col = editor.getCursorColumn();
        const currentLine = lines[line] || "";
        const charBefore = col > 0 ? currentLine[col - 1] : "";
        const charAfter = currentLine[col] || "";
        const indent = currentLine.match(/^(\s*)/)?.[1] || "";
        const textBeforeCursor = currentLine.slice(0, col);

        // Check for bracket pairs
        if (
          (charBefore === "{" && charAfter === "}") ||
          (charBefore === "[" && charAfter === "]") ||
          (charBefore === "(" && charAfter === ")")
        ) {
          editor.insert("\n" + indent + "    \n" + indent);
          editor.moveCursorUp();
          editor.moveCursorLineEnd();
        } else {
          const endsWithOpener = /[{(\[]$/.test(textBeforeCursor.trim());
          if (endsWithOpener) {
            editor.insert("\n" + indent + "    ");
          } else {
            editor.insert("\n" + indent);
          }
        }
        notifyContentChange();
      } else if (e.key === "Tab") {
        editor.insert("    ");
        notifyContentChange();
      } else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey) {
        // Auto-pairing
        const pairs: Record<string, string> = { "(": ")", "[": "]", "{": "}", '"': '"', "'": "'" };
        const closers = [")", "]", "}", '"', "'"];

        const content = editor.getContent();
        const lines = content.split("\n");
        const line = editor.getCursorLine();
        const col = editor.getCursorColumn();
        const currentLine = lines[line] || "";
        const charAfter = currentLine[col] || "";

        if (closers.includes(e.key) && charAfter === e.key) {
          editor.moveCursorRight();
        } else if (pairs[e.key]) {
          editor.insert(e.key + pairs[e.key]);
          editor.moveCursorLeft();
        } else {
          editor.insert(e.key);
        }
        notifyContentChange();
      } else if (e.metaKey || e.ctrlKey) {
        if (e.key === "z") {
          if (e.shiftKey) {
            editor.redo();
          } else {
            editor.undo();
          }
          notifyContentChange();
        } else if (e.key === "y") {
          editor.redo();
          notifyContentChange();
        } else if (e.key === "c") {
          const text = editor.getSelectedText();
          if (text) {
            navigator.clipboard.writeText(text).catch(console.error);
          }
        } else if (e.key === "x") {
          const text = editor.getSelectedText();
          if (text) {
            navigator.clipboard.writeText(text).then(() => {
              editor.backspace();
              updateHighlights();
              editor.forceRender();
              notifyContentChange();
            }).catch(console.error);
          }
        } else if (e.key === "v") {
          navigator.clipboard.readText().then((text) => {
            if (text) {
              editor.insert(text);
              updateHighlights();
              editor.forceRender();
              notifyContentChange();
            }
          }).catch(console.error);
        } else {
          handled = false;
        }
      } else {
        handled = false;
      }

      if (handled) {
        e.preventDefault();
        editor.ensureCursorVisible();
        updateHighlights();
        editor.forceRender();
        notifySelectionChange();
      }
    }

    canvas.addEventListener("keydown", handleKeyDown);

    return () => {
      canvas.removeEventListener("keydown", handleKeyDown);
    };
  }, [isReady, onChange, onSelectionChange]);

  // Imperative handle
  useImperativeHandle(
    ref,
    () => ({
      getContent: () => editorRef.current?.getContent() ?? "",
      setContent: (newContent: string) => {
        if (editorRef.current) {
          editorRef.current.setContent(newContent);
          lastContentRef.current = newContent;
          updateHighlights();
          editorRef.current.forceRender();
        }
      },
      undo: () => {
        const result = editorRef.current?.undo() ?? false;
        updateHighlights();
        editorRef.current?.forceRender();
        return result;
      },
      redo: () => {
        const result = editorRef.current?.redo() ?? false;
        updateHighlights();
        editorRef.current?.forceRender();
        return result;
      },
      foldAll: () => {
        editorRef.current?.foldAll();
        editorRef.current?.forceRender();
      },
      unfoldAll: () => {
        editorRef.current?.unfoldAll();
        editorRef.current?.forceRender();
      },
      toggleFold: (line: number) => {
        const result = editorRef.current?.toggleFold(line) ?? false;
        editorRef.current?.forceRender();
        return result;
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
