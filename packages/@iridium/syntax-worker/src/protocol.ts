/**
 * Protocol definitions for main thread <-> worker communication.
 */

export interface HighlightSpan {
  start: number;
  end: number;
  type: string;
}

export interface EditInfo {
  startIndex: number;
  oldEndIndex: number;
  newEndIndex: number;
  startPosition: { row: number; column: number };
  oldEndPosition: { row: number; column: number };
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
