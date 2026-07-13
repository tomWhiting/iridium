/**
 * Protocol definitions for main thread <-> worker communication.
 */

export interface HighlightSpan {
  start: number;
  end: number;
  type: string;
}

/**
 * Describes a single document edit for incremental tree-sitter parsing.
 *
 * IMPORTANT — encoding: across the wire this carries the Rust core's rope
 * coordinates, i.e. **UTF-8 byte offsets** in the `*Index` fields and **byte
 * columns** in the `*Position.column` fields (exactly the shape
 * `WebEditor::takeLastEdit` / `JsEditInfo` produces). web-tree-sitter's
 * `Tree.edit()` expects UTF-16 code units and UTF-16 columns instead, so the
 * worker converts this structure at the boundary (see the syntax worker's
 * `convertEditInfo`) before applying it. On ASCII documents the two encodings
 * coincide and the conversion is a no-op.
 */
export interface EditInfo {
  /** Byte offset where the edit begins (same in the pre- and post-edit text). */
  startIndex: number;
  /** Byte offset where the replaced text ended in the pre-edit text. */
  oldEndIndex: number;
  /** Byte offset where the new text ends in the post-edit text. */
  newEndIndex: number;
  /** Edit start, as row + byte column. */
  startPosition: { row: number; column: number };
  /** Pre-edit end, as row + byte column. */
  oldEndPosition: { row: number; column: number };
  /** Post-edit end, as row + byte column. */
  newEndPosition: { row: number; column: number };
}

// Messages from main thread to worker
export type WorkerRequest =
  | { type: "init"; language: string }
  | { type: "setLanguage"; language: string }
  | { type: "highlight"; id: number; content: string }
  | {
      type: "highlightRange";
      id: number;
      content: string;
      startLine: number;
      endLine: number;
      editInfo?: EditInfo | null;
    }
  | { type: "highlightIncremental"; id: number; content: string; editInfo: EditInfo };

// Messages from worker to main thread
export type WorkerResponse =
  | { type: "ready"; languages: string[] }
  | { type: "languageSet"; success: boolean }
  | { type: "highlights"; id: number; spans: HighlightSpan[]; parseTime: number }
  | { type: "error"; id: number; message: string };
