/**
 * React hooks for Iridium editor integration.
 *
 * These hooks provide a convenient way to work with the Iridium editor
 * in React applications, handling state management and lifecycle.
 *
 * @module examples/web/src/hooks/useIridium
 */

import { useState, useEffect, useCallback, useRef } from "react";

import type {
  IridiumEditor,
  JsEditorConfig,
  JsPosition,
  JsSelection,
  JsSearchResult,
  JsFoldInfo,
  JsUndoInfo,
} from "iridium-bindings";

// ============================================================================
// Types
// ============================================================================

/**
 * WebGPU support status.
 */
export interface WebGPUStatus {
  /** Whether WebGPU is supported */
  supported: boolean;
  /** Whether the check has completed */
  checked: boolean;
  /** GPU information if available */
  gpuInfo?: {
    name: string;
    backend: string;
    deviceType: string;
  };
}

/**
 * Editor state returned by useEditor hook.
 */
export interface EditorState {
  /** Current content */
  content: string;
  /** Current cursor position */
  cursor: JsPosition;
  /** Current selection */
  selection: JsSelection;
  /** Line count */
  lineCount: number;
  /** Whether editor has focus */
  hasFocus: boolean;
  /** Whether content has been modified */
  isDirty: boolean;
  /** Undo availability */
  canUndo: boolean;
  /** Redo availability */
  canRedo: boolean;
}

/**
 * Search state returned by useSearch hook.
 */
export interface SearchState {
  /** Whether search is active */
  isActive: boolean;
  /** Current search query */
  query: string;
  /** Number of matches */
  matchCount: number;
  /** Current match index (0-based) */
  currentIndex: number | null;
  /** All match ranges */
  matches: JsSearchResult["matches"];
}

// ============================================================================
// Hooks
// ============================================================================

/**
 * Hook to check WebGPU support.
 *
 * @example
 * ```tsx
 * function App() {
 *   const { supported, checked, gpuInfo } = useWebGPUSupport();
 *
 *   if (!checked) return <div>Checking WebGPU support...</div>;
 *   if (!supported) return <div>WebGPU is not supported</div>;
 *
 *   return <div>Using GPU: {gpuInfo?.name}</div>;
 * }
 * ```
 */
export function useWebGPUSupport(): WebGPUStatus {
  const [status, setStatus] = useState<WebGPUStatus>({
    supported: false,
    checked: false,
  });

  useEffect(() => {
    let mounted = true;

    async function check() {
      try {
        const bindings = await import("iridium-bindings");
        const supported = await bindings.isWebGPUSupported();

        if (!mounted) return;

        if (supported) {
          const gpuInfo = await bindings.getGPUInfo();
          setStatus({
            supported: true,
            checked: true,
            gpuInfo: gpuInfo
              ? {
                  name: gpuInfo.name,
                  backend: gpuInfo.backend,
                  deviceType: gpuInfo.deviceType,
                }
              : undefined,
          });
        } else {
          setStatus({ supported: false, checked: true });
        }
      } catch {
        if (mounted) {
          setStatus({ supported: false, checked: true });
        }
      }
    }

    check();

    return () => {
      mounted = false;
    };
  }, []);

  return status;
}

/**
 * Hook to create and manage an Iridium editor instance.
 *
 * @param config - Editor configuration
 * @returns Editor instance and loading state
 *
 * @example
 * ```tsx
 * function Editor() {
 *   const { editor, isLoading, error } = useEditor({ tabWidth: 2 });
 *
 *   useEffect(() => {
 *     if (editor) {
 *       editor.setContent('Hello, Iridium!');
 *     }
 *   }, [editor]);
 *
 *   if (isLoading) return <div>Loading...</div>;
 *   if (error) return <div>Error: {error}</div>;
 *
 *   return <div>Editor ready</div>;
 * }
 * ```
 */
export function useEditor(config?: JsEditorConfig): {
  editor: IridiumEditor | null;
  isLoading: boolean;
  error: string | null;
} {
  const [editor, setEditor] = useState<IridiumEditor | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const configRef = useRef(config);

  useEffect(() => {
    let mounted = true;

    async function init() {
      try {
        const bindings = await import("iridium-bindings");

        const supported = await bindings.isWebGPUSupported();
        if (!supported) {
          throw new Error("WebGPU is not supported");
        }

        if (!mounted) return;

        const newEditor = bindings.createEditor(configRef.current);
        setEditor(newEditor);
        setIsLoading(false);
      } catch (e) {
        if (mounted) {
          setError(e instanceof Error ? e.message : String(e));
          setIsLoading(false);
        }
      }
    }

    init();

    return () => {
      mounted = false;
      if (editor) {
        editor.destroy();
      }
    };
  }, []);

  return { editor, isLoading, error };
}

/**
 * Hook to track editor state.
 *
 * @param editor - The Iridium editor instance
 * @returns Current editor state
 *
 * @example
 * ```tsx
 * function EditorStatus({ editor }: { editor: IridiumEditor }) {
 *   const state = useEditorState(editor);
 *
 *   return (
 *     <div>
 *       Line {state.cursor.line + 1}, Column {state.cursor.column + 1}
 *       {state.isDirty && ' (modified)'}
 *     </div>
 *   );
 * }
 * ```
 */
export function useEditorState(editor: IridiumEditor | null): EditorState {
  const [state, setState] = useState<EditorState>({
    content: "",
    cursor: { line: 0, column: 0 },
    selection: {
      anchor: { line: 0, column: 0 },
      head: { line: 0, column: 0 },
    },
    lineCount: 1,
    hasFocus: false,
    isDirty: false,
    canUndo: false,
    canRedo: false,
  });

  const originalContentRef = useRef<string>("");

  useEffect(() => {
    if (!editor) return;

    // Get initial state
    const content = editor.getContent();
    originalContentRef.current = content;

    setState({
      content,
      cursor: editor.getCursor(),
      selection: editor.getSelection(),
      lineCount: editor.getLineCount(),
      hasFocus: editor.hasFocus(),
      isDirty: false,
      canUndo: editor.canUndo(),
      canRedo: editor.canRedo(),
    });

    // Subscribe to changes
    const subscriptions: number[] = [];

    subscriptions.push(
      editor.on("contentChanged", (newContent: string) => {
        setState((prev) => ({
          ...prev,
          content: newContent,
          lineCount: editor.getLineCount(),
          isDirty: newContent !== originalContentRef.current,
          canUndo: editor.canUndo(),
          canRedo: editor.canRedo(),
        }));
      })
    );

    subscriptions.push(
      editor.on("selectionChanged", (data: string) => {
        try {
          const selection = JSON.parse(data) as JsSelection;
          setState((prev) => ({
            ...prev,
            cursor: selection.head,
            selection,
          }));
        } catch {
          // Ignore parse errors
        }
      })
    );

    subscriptions.push(
      editor.on("focus", () => {
        setState((prev) => ({ ...prev, hasFocus: true }));
      })
    );

    subscriptions.push(
      editor.on("blur", () => {
        setState((prev) => ({ ...prev, hasFocus: false }));
      })
    );

    return () => {
      for (const subId of subscriptions) {
        editor.off(subId);
      }
    };
  }, [editor]);

  return state;
}

/**
 * Hook to manage search state.
 *
 * @param editor - The Iridium editor instance
 * @returns Search state and controls
 *
 * @example
 * ```tsx
 * function SearchBar({ editor }: { editor: IridiumEditor }) {
 *   const { state, find, next, previous, close } = useSearch(editor);
 *
 *   return (
 *     <div>
 *       <input
 *         value={state.query}
 *         onChange={(e) => find(e.target.value)}
 *         placeholder="Search..."
 *       />
 *       {state.isActive && (
 *         <span>
 *           {state.currentIndex !== null ? state.currentIndex + 1 : 0} of{' '}
 *           {state.matchCount}
 *         </span>
 *       )}
 *       <button onClick={previous}>Prev</button>
 *       <button onClick={next}>Next</button>
 *       <button onClick={close}>Close</button>
 *     </div>
 *   );
 * }
 * ```
 */
export function useSearch(editor: IridiumEditor | null): {
  state: SearchState;
  find: (query: string) => void;
  next: () => void;
  previous: () => void;
  close: () => void;
  replace: (replacement: string) => boolean;
  replaceAll: (replacement: string) => number;
} {
  const [state, setState] = useState<SearchState>({
    isActive: false,
    query: "",
    matchCount: 0,
    currentIndex: null,
    matches: [],
  });

  const find = useCallback(
    (query: string) => {
      if (!editor) return;

      if (!query) {
        editor.closeSearch();
        setState({
          isActive: false,
          query: "",
          matchCount: 0,
          currentIndex: null,
          matches: [],
        });
        return;
      }

      const result = editor.find(query);
      setState({
        isActive: true,
        query,
        matchCount: result.matchCount,
        currentIndex: result.currentIndex ?? null,
        matches: result.matches,
      });
    },
    [editor]
  );

  const next = useCallback(() => {
    if (!editor) return;
    editor.nextMatch();
    setState((prev) => ({
      ...prev,
      currentIndex: editor.getCurrentMatchIndex() ?? null,
    }));
  }, [editor]);

  const previous = useCallback(() => {
    if (!editor) return;
    editor.previousMatch();
    setState((prev) => ({
      ...prev,
      currentIndex: editor.getCurrentMatchIndex() ?? null,
    }));
  }, [editor]);

  const close = useCallback(() => {
    if (!editor) return;
    editor.closeSearch();
    setState({
      isActive: false,
      query: "",
      matchCount: 0,
      currentIndex: null,
      matches: [],
    });
  }, [editor]);

  const replace = useCallback(
    (replacement: string) => {
      if (!editor) return false;
      const result = editor.replaceCurrent(replacement);
      if (result) {
        setState((prev) => ({
          ...prev,
          matchCount: editor.getMatchCount(),
          currentIndex: editor.getCurrentMatchIndex() ?? null,
        }));
      }
      return result;
    },
    [editor]
  );

  const replaceAll = useCallback(
    (replacement: string) => {
      if (!editor) return 0;
      const count = editor.replaceAll(replacement);
      if (count > 0) {
        setState((prev) => ({
          ...prev,
          matchCount: 0,
          currentIndex: null,
          matches: [],
        }));
      }
      return count;
    },
    [editor]
  );

  // Subscribe to search updates
  useEffect(() => {
    if (!editor) return;

    const subId = editor.on("searchUpdated", () => {
      setState((prev) => ({
        ...prev,
        matchCount: editor.getMatchCount(),
        currentIndex: editor.getCurrentMatchIndex() ?? null,
      }));
    });

    return () => {
      editor.off(subId);
    };
  }, [editor]);

  return { state, find, next, previous, close, replace, replaceAll };
}

/**
 * Hook to manage fold state.
 *
 * @param editor - The Iridium editor instance
 * @returns Fold state and controls
 */
export function useFolding(editor: IridiumEditor | null): {
  info: JsFoldInfo;
  foldAt: (line: number) => boolean;
  unfoldAt: (line: number) => boolean;
  toggleAt: (line: number) => boolean;
  foldAll: () => void;
  unfoldAll: () => void;
  isFoldable: (line: number) => boolean;
  isFolded: (line: number) => boolean;
} {
  const [info, setInfo] = useState<JsFoldInfo>({ regions: [], folded: [] });

  useEffect(() => {
    if (!editor) return;

    // Get initial state
    setInfo(editor.exportFoldInfo());

    // Subscribe to fold changes
    const subId = editor.on("foldChanged", () => {
      setInfo(editor.exportFoldInfo());
    });

    return () => {
      editor.off(subId);
    };
  }, [editor]);

  const foldAt = useCallback(
    (line: number) => {
      if (!editor) return false;
      const result = editor.foldAt(line);
      if (result) setInfo(editor.exportFoldInfo());
      return result;
    },
    [editor]
  );

  const unfoldAt = useCallback(
    (line: number) => {
      if (!editor) return false;
      const result = editor.unfoldAt(line);
      if (result) setInfo(editor.exportFoldInfo());
      return result;
    },
    [editor]
  );

  const toggleAt = useCallback(
    (line: number) => {
      if (!editor) return false;
      const result = editor.toggleFoldAt(line);
      if (result) setInfo(editor.exportFoldInfo());
      return result;
    },
    [editor]
  );

  const foldAll = useCallback(() => {
    if (!editor) return;
    editor.foldAll();
    setInfo(editor.exportFoldInfo());
  }, [editor]);

  const unfoldAll = useCallback(() => {
    if (!editor) return;
    editor.unfoldAll();
    setInfo(editor.exportFoldInfo());
  }, [editor]);

  const isFoldable = useCallback(
    (line: number) => {
      return editor?.isFoldable(line) ?? false;
    },
    [editor]
  );

  const isFolded = useCallback(
    (line: number) => {
      return editor?.isFolded(line) ?? false;
    },
    [editor]
  );

  return {
    info,
    foldAt,
    unfoldAt,
    toggleAt,
    foldAll,
    unfoldAll,
    isFoldable,
    isFolded,
  };
}

/**
 * Hook to manage undo/redo state.
 *
 * @param editor - The Iridium editor instance
 * @returns Undo state and controls
 */
export function useUndoRedo(editor: IridiumEditor | null): {
  info: JsUndoInfo;
  undo: () => boolean;
  redo: () => boolean;
  canUndo: boolean;
  canRedo: boolean;
} {
  const [info, setInfo] = useState<JsUndoInfo>({
    nodeCount: 0,
    branchCount: 0,
    canUndo: false,
    canRedo: false,
    currentId: "",
    rootId: "",
  });

  const updateInfo = useCallback(() => {
    if (editor) {
      setInfo(editor.getUndoInfo());
    }
  }, [editor]);

  useEffect(() => {
    if (!editor) return;

    // Get initial state
    updateInfo();

    // Subscribe to content changes (undo state may change)
    const subId = editor.on("contentChanged", updateInfo);

    return () => {
      editor.off(subId);
    };
  }, [editor, updateInfo]);

  const undo = useCallback(() => {
    if (!editor) return false;
    const result = editor.undo();
    updateInfo();
    return result;
  }, [editor, updateInfo]);

  const redo = useCallback(() => {
    if (!editor) return false;
    const result = editor.redo();
    updateInfo();
    return result;
  }, [editor, updateInfo]);

  return {
    info,
    undo,
    redo,
    canUndo: info.canUndo,
    canRedo: info.canRedo,
  };
}
