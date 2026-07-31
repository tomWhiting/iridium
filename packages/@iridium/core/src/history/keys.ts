/**
 * What a keystroke means to an open undo-tree panel.
 *
 * Separated from every view for the same reason the palette's keys are: two
 * faces disagreeing about what the left arrow does is a bug nobody notices
 * until they switch faces. A pure function over a key description, so it is
 * testable without a DOM.
 *
 * @module
 */

import type { UndoTreePanel } from "./index.ts";

/** What an open undo-tree panel should do about a keystroke. */
export type UndoTreeKeyAction =
  /** Dismiss and hand the keyboard back. */
  | "close"
  /** Move the document to the selected state. */
  | "jump"
  /** Select the state this one was reached from. */
  | "parent"
  /** Select the state a redo would go to. */
  | "child"
  /** Select the next branch of the same fork. */
  | "nextSibling"
  /** Select the previous branch of the same fork. */
  | "previousSibling"
  /** Not ours. */
  | "none";

/** The parts of a keyboard event this depends on. */
export interface UndoTreeKeyEvent {
  /** `KeyboardEvent.key`. */
  readonly key: string;
  /** Whether Control was held. */
  readonly ctrlKey: boolean;
  /** Whether Command (macOS) was held. */
  readonly metaKey: boolean;
}

/**
 * Classifies a keystroke against an open panel.
 *
 * The arrows read as the tree, not as a list: **up and down walk the active
 * path** — the way the document actually travels — and **left and right move
 * between the branches of a fork**. A plain list cursor would be easier to
 * write and would misrepresent the one thing this panel exists to show.
 *
 * `Ctrl+H` closes, because `Ctrl+Alt+H` is the key that opened it and the
 * browser never delivers the `Alt` variant intact on every platform; claiming
 * the unmodified chord as well means the open key always has a way back out.
 * `Meta` counts alongside `Control` throughout, because the web face forwards
 * macOS `Cmd` as the kernel's `ctrl`.
 *
 * The `Ctrl+N`/`Ctrl+P` aliases match the palette's, so the two overlays are
 * driven the same way by anyone who never reaches for the arrows.
 */
export function undoTreeKeyAction(event: UndoTreeKeyEvent): UndoTreeKeyAction {
  const chord = event.ctrlKey || event.metaKey;
  switch (event.key) {
    case "Escape":
      return "close";
    case "Enter":
      return "jump";
    case "ArrowUp":
      return "parent";
    case "ArrowDown":
      return "child";
    case "ArrowRight":
      return "nextSibling";
    case "ArrowLeft":
      return "previousSibling";
    case "h":
    case "H":
      return chord ? "close" : "none";
    case "n":
    case "N":
      return chord ? "child" : "none";
    case "p":
    case "P":
      return chord ? "parent" : "none";
    default:
      return "none";
  }
}

/**
 * Applies a keystroke to `panel`, and reports whether it was ours.
 *
 * A `true` return means the caller should suppress the default — every action
 * here has a browser behaviour that would otherwise fire underneath it, from
 * scrolling the page to moving a caret.
 */
export function applyUndoTreeKey(panel: UndoTreePanel, event: UndoTreeKeyEvent): boolean {
  switch (undoTreeKeyAction(event)) {
    case "close":
      panel.close();
      return true;
    case "jump":
      panel.jump();
      return true;
    case "parent":
      panel.selectParent();
      return true;
    case "child":
      panel.selectChild();
      return true;
    case "nextSibling":
      panel.selectSibling(1);
      return true;
    case "previousSibling":
      panel.selectSibling(-1);
      return true;
    case "none":
      return false;
  }
}
