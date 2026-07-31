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
import {
  IridiumEditor,
  type EditorState,
  type HostCommandRequest,
  type PaletteCommand,
  type UndoTreeSnapshot,
} from "@iridium/core";

// ============================================================================
// Types
// ============================================================================

/**
 * What the handle reports before the editor exists.
 *
 * A tree of no nodes rather than a thrown error: an overlay opened during
 * startup should draw nothing and recover on the next refresh.
 */
const EMPTY_HISTORY: UndoTreeSnapshot = {
  nodes: [],
  info: {
    currentId: "",
    rootId: "",
    nodeCount: 0,
    canUndo: false,
    canRedo: false,
    branchCount: 0,
  },
};

export interface IridiumProps {
  content?: string;
  language?: string;
  darkTheme?: boolean;
  onChange?: (content: string) => void;
  onSelectionChange?: (selection: { head: { line: number; column: number } }) => void;
  /**
   * A command the kernel resolved but does not implement — `palette.open` is the
   * first. The kernel names and binds these so every face agrees on the id and
   * the key; what they *mean* is the host's to decide.
   */
  onHostCommand?: (request: HostCommandRequest) => void;
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

  // The palette surface. Together with `focus`, this is exactly the `PaletteHost`
  // the framework-free `CommandPalette` needs, so a caller can hand it the ref.

  /** Every registered command, grouped for browsing. */
  listCommands(): PaletteCommand[];
  /** Commands matching `query`, best first; an empty query returns everything. */
  searchCommands(query: string, limit?: number): PaletteCommand[];
  /** Runs a command by id. Unimplemented ids come back through `onHostCommand`. */
  runCommand(id: string): void;
  /** Releases the keyboard so an overlay's own input can take it. */
  blurEditor(): void;

  // The undo-tree surface. Together with `focus` and `blurEditor`, this is
  // exactly the `UndoTreeHost` the framework-free `UndoTreePanel` needs.

  /** The whole undo tree as it stands, for a panel that draws it. */
  historySnapshot(): UndoTreeSnapshot;
  /** Moves the document to a state, by node id. */
  jumpToHistoryNode(nodeId: string): boolean;
  /** Whether key labels should read `⌘K` rather than `Ctrl+K`. */
  readonly usesMacKeyLabels: boolean;
  /** How many carets are active — `1` unless multi-cursor is in play. */
  readonly cursorCount: number;
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
    onHostCommand,
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

  // The editor captures its callbacks once, at construction, but props change on
  // every render — so the callbacks it holds must read through a ref rather than
  // close over the first render's values. Without this an `onHostCommand` that
  // depends on component state fires against a stale snapshot.
  const callbacksRef = useRef({ onChange, onSelectionChange, onHostCommand });
  callbacksRef.current = { onChange, onSelectionChange, onHostCommand };

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
            callbacksRef.current.onChange?.(newContent);
          },
          onSelectionChange: (info) => {
            callbacksRef.current.onSelectionChange?.({
              head: { line: info.line, column: info.column },
            });
          },
          onHostCommand: (request) => {
            callbacksRef.current.onHostCommand?.(request);
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
      listCommands: () => editorRef.current?.listCommands() ?? [],
      searchCommands: (query: string, limit?: number) =>
        editorRef.current?.searchCommands(query, limit) ?? [],
      runCommand: (id: string) => {
        editorRef.current?.runCommand(id);
      },
      blurEditor: () => editorRef.current?.blurEditor(),
      // An empty tree rather than a thrown error when the editor is not up yet:
      // a panel opened during startup should draw nothing, not break the page.
      historySnapshot: () => editorRef.current?.historySnapshot() ?? EMPTY_HISTORY,
      jumpToHistoryNode: (nodeId: string) =>
        editorRef.current?.jumpToHistoryNode(nodeId) ?? false,
      // A getter, not a captured value: the editor does not exist yet when this
      // handle is built, and the answer is a property of the platform anyway.
      get usesMacKeyLabels(): boolean {
        return editorRef.current?.usesMacKeyLabels ?? false;
      },
      get cursorCount(): number {
        return editorRef.current?.cursorCount ?? 1;
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
