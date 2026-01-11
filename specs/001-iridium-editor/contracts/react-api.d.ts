/**
 * Iridium Editor React Component API
 *
 * This file defines the React component interface for Iridium.
 * The component is a thin wrapper around the core TypeScript API.
 *
 * @packageDocumentation
 */

import { FC, RefObject } from 'react';
import {
  IridiumEditor,
  Theme,
  EditorConfig,
  Selection,
  Position,
  EditorEvent,
  SearchOptions,
} from './typescript-api';

// =============================================================================
// Component Props
// =============================================================================

/**
 * Props for the Iridium React component.
 */
export interface IridiumProps {
  // ---------------------------------------------------------------------------
  // Required Props
  // ---------------------------------------------------------------------------

  // None - component can be used with all defaults

  // ---------------------------------------------------------------------------
  // Content (Controlled)
  // ---------------------------------------------------------------------------

  /**
   * Document content (controlled mode).
   * When provided, the component operates in controlled mode.
   */
  value?: string;

  /**
   * Callback when content changes.
   * Required when using controlled mode (value prop).
   */
  onChange?: (content: string) => void;

  /**
   * Initial content (uncontrolled mode).
   * Ignored if `value` is provided.
   */
  defaultValue?: string;

  // ---------------------------------------------------------------------------
  // Language and Highlighting
  // ---------------------------------------------------------------------------

  /**
   * Language identifier for syntax highlighting.
   * Supported: 'cypher', 'sql', 'rust', 'python', 'typescript'
   */
  language?: string;

  // ---------------------------------------------------------------------------
  // Appearance
  // ---------------------------------------------------------------------------

  /**
   * Editor theme configuration.
   */
  theme?: Theme;

  /**
   * CSS class name for the container div.
   */
  className?: string;

  /**
   * Inline styles for the container div.
   */
  style?: React.CSSProperties;

  // ---------------------------------------------------------------------------
  // Configuration
  // ---------------------------------------------------------------------------

  /**
   * Editor configuration options.
   */
  config?: EditorConfig;

  /**
   * Whether the editor is read-only.
   */
  readOnly?: boolean;

  /**
   * Whether the editor should auto-focus on mount.
   */
  autoFocus?: boolean;

  // ---------------------------------------------------------------------------
  // Event Callbacks
  // ---------------------------------------------------------------------------

  /**
   * Callback when cursor/selection changes.
   */
  onSelectionChange?: (selections: Selection[]) => void;

  /**
   * Callback when scroll position changes.
   */
  onScroll?: (firstLine: number) => void;

  /**
   * Callback when search results update.
   */
  onSearchUpdate?: (matchCount: number, currentIndex: number | null) => void;

  /**
   * Callback when an error occurs.
   */
  onError?: (message: string, code: string) => void;

  /**
   * Callback when the editor gains focus.
   */
  onFocus?: () => void;

  /**
   * Callback when the editor loses focus.
   */
  onBlur?: () => void;

  // ---------------------------------------------------------------------------
  // Ref
  // ---------------------------------------------------------------------------

  /**
   * Ref to access the underlying IridiumEditor instance.
   */
  editorRef?: RefObject<IridiumEditor | null>;
}

// =============================================================================
// Component
// =============================================================================

/**
 * Iridium editor React component.
 *
 * @example
 * ```tsx
 * // Uncontrolled mode (simplest)
 * <Iridium
 *   defaultValue="MATCH (n) RETURN n"
 *   language="cypher"
 * />
 *
 * // Controlled mode
 * const [code, setCode] = useState('SELECT * FROM users');
 * <Iridium
 *   value={code}
 *   onChange={setCode}
 *   language="sql"
 * />
 *
 * // With ref for programmatic access
 * const editorRef = useRef<IridiumEditor>(null);
 * <Iridium
 *   editorRef={editorRef}
 *   defaultValue=""
 *   language="rust"
 * />
 *
 * // Later...
 * editorRef.current?.undo();
 * ```
 */
export const Iridium: FC<IridiumProps>;

// =============================================================================
// Hooks
// =============================================================================

/**
 * Hook to access the Iridium editor instance from context.
 * Must be used within an IridiumProvider.
 *
 * @returns The editor instance or null if not available
 */
export function useIridiumEditor(): IridiumEditor | null;

/**
 * Hook to get editor content.
 * Subscribes to content changes and returns current content.
 *
 * @param editor - Editor instance
 * @returns Current content string
 */
export function useEditorContent(editor: IridiumEditor | null): string;

/**
 * Hook to get cursor/selection state.
 * Subscribes to selection changes and returns current selections.
 *
 * @param editor - Editor instance
 * @returns Current selections array
 */
export function useEditorSelections(editor: IridiumEditor | null): Selection[];

/**
 * Hook for search functionality.
 *
 * @param editor - Editor instance
 * @returns Search state and functions
 */
export function useEditorSearch(editor: IridiumEditor | null): {
  /** Current search query */
  query: string;
  /** Set search query */
  setQuery: (query: string) => void;
  /** Search options */
  options: SearchOptions;
  /** Set search options */
  setOptions: (options: SearchOptions) => void;
  /** Number of matches */
  matchCount: number;
  /** Current match index */
  currentIndex: number | null;
  /** Go to next match */
  findNext: () => void;
  /** Go to previous match */
  findPrevious: () => void;
  /** Replace current match */
  replace: (replacement: string) => void;
  /** Replace all matches */
  replaceAll: (replacement: string) => void;
  /** Clear search */
  clear: () => void;
};

/**
 * Hook for undo/redo state.
 *
 * @param editor - Editor instance
 * @returns Undo/redo state and functions
 */
export function useEditorHistory(editor: IridiumEditor | null): {
  /** Can undo */
  canUndo: boolean;
  /** Can redo */
  canRedo: boolean;
  /** Number of redo branches */
  branchCount: number;
  /** Perform undo */
  undo: () => void;
  /** Perform redo */
  redo: () => void;
  /** Redo specific branch */
  redoBranch: (index: number) => void;
};

// =============================================================================
// Utilities
// =============================================================================

/**
 * Pre-check WebGPU support before rendering.
 *
 * @example
 * ```tsx
 * const [supported, setSupported] = useState<boolean | null>(null);
 *
 * useEffect(() => {
 *   checkWebGPUSupport().then(setSupported);
 * }, []);
 *
 * if (supported === null) return <div>Checking WebGPU...</div>;
 * if (!supported) return <div>WebGPU not supported</div>;
 * return <Iridium defaultValue="" />;
 * ```
 */
export function checkWebGPUSupport(): Promise<boolean>;
