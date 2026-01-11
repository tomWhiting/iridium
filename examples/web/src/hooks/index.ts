/**
 * React hooks for Iridium editor integration.
 *
 * @module examples/web/src/hooks
 */

export {
  useWebGPUSupport,
  useEditor,
  useEditorState,
  useSearch,
  useFolding,
  useUndoRedo,
} from "./useIridium";

export type {
  WebGPUStatus,
  EditorState,
  SearchState,
} from "./useIridium";
