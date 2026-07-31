/**
 * A framework-free undo-tree panel state machine.
 *
 * No DOM, no React, no rendering — the same division as the command palette,
 * for the same reason: the behaviour of the panel is the part that must not
 * differ between faces, and the drawing is the part that must.
 *
 * The tree itself is never cached here. It changes on every keystroke, and a
 * panel drawing a tree the document has moved on from is worse than no panel:
 * it invites a jump to a node the user is looking at and the editor is not.
 * {@link UndoTreePanel.refresh} re-reads, and the host calls it whenever the
 * document or the history moves.
 *
 * @module
 */

import type { UndoTreeSnapshot } from "../controller/index.ts";
import { buildRows, type UndoTreeRow } from "./rows.ts";

export { buildRows, formatAge, type UndoTreeRow } from "./rows.ts";
export {
  applyUndoTreeKey,
  type UndoTreeKeyAction,
  type UndoTreeKeyEvent,
  undoTreeKeyAction,
} from "./keys.ts";

/**
 * The editor surface an undo-tree panel needs.
 *
 * Structural, not a class: `IridiumEditor` satisfies it, and so does a fake in
 * a test. Keeping it this narrow is what lets the panel be tested without a
 * canvas, a GPU or a wasm module.
 */
export interface UndoTreeHost {
  /** The whole tree as it stands right now. */
  historySnapshot(): UndoTreeSnapshot;
  /** Moves the document to a state, by node id. */
  jumpToHistoryNode(nodeId: string): boolean;
  /** Releases the keyboard so the panel can take it. */
  blurEditor(): void;
  /** Returns the keyboard to the editor. */
  focus(): void;
}

/** Everything a renderer needs, and nothing it does not. */
export interface UndoTreePanelState {
  /** Whether the panel is showing. */
  readonly open: boolean;
  /** Every state of the document, laid out for drawing. Empty while closed. */
  readonly rows: readonly UndoTreeRow[];
  /**
   * The highlighted row's node id, or `""` when there is nothing to highlight.
   *
   * An id rather than an index, because the tree is re-read whenever it
   * changes and every index shifts when a branch appears above the selection.
   * The id survives that; a row number does not.
   */
  readonly selectedId: string;
}

/** Notified after every state change, with the new state. */
export type UndoTreeListener = (state: UndoTreePanelState) => void;

const CLOSED: UndoTreePanelState = Object.freeze({
  open: false,
  rows: Object.freeze([]) as readonly UndoTreeRow[],
  selectedId: "",
});

/**
 * The undo-tree panel's behaviour, independent of how it is drawn.
 *
 * Navigation here moves only the *selection*: nothing reaches the document
 * until {@link UndoTreePanel.jump}. That is deliberate and is the difference
 * between this and the `history.nextBranch` key — browsing the tree must be
 * free, so that looking at where a branch goes is not itself an edit.
 */
export class UndoTreePanel {
  readonly #host: UndoTreeHost;
  #listeners: UndoTreeListener[] = [];
  #state: UndoTreePanelState = CLOSED;

  constructor(host: UndoTreeHost) {
    this.#host = host;
  }

  /** The current state. Immutable; replaced wholesale on every change. */
  get state(): UndoTreePanelState {
    return this.#state;
  }

  /** The highlighted row, or `undefined` when the panel is closed or empty. */
  get selected(): UndoTreeRow | undefined {
    return this.#state.rows.find((row) => row.id === this.#state.selectedId);
  }

  /**
   * Subscribes to state changes. Returns the unsubscribe function.
   *
   * Listeners are notified from a snapshot of the list, so subscribing or
   * unsubscribing from inside a listener affects the *next* notification rather
   * than corrupting the current one.
   */
  subscribe(listener: UndoTreeListener): () => void {
    this.#listeners = [...this.#listeners, listener];
    return () => {
      this.#listeners = this.#listeners.filter((entry) => entry !== listener);
    };
  }

  /**
   * Opens the panel, selecting the state the document is on.
   *
   * Starting on the current node rather than the root is what makes the arrow
   * keys immediately useful: up is "where I came from" and down is "where redo
   * would go", both measured from where the user actually is.
   */
  open(): void {
    this.#host.blurEditor();
    const rows = this.#read();
    this.#commit({ open: true, rows, selectedId: currentId(rows) });
  }

  /**
   * Closes the panel and returns the keyboard to the editor.
   *
   * Restoring focus is not optional politeness: an editor that has to be
   * clicked back into after every glance at the history is the single thing
   * that would make this feel broken. Closing an already-closed panel does
   * nothing and notifies nobody.
   */
  close(): void {
    if (!this.#state.open) {
      return;
    }
    this.#commit(CLOSED);
    this.#host.focus();
  }

  /** Opens the panel, or closes it if it is already open. */
  toggle(): void {
    if (this.#state.open) {
      this.close();
    } else {
      this.open();
    }
  }

  /**
   * Re-reads the tree, keeping the selection where it can.
   *
   * Called whenever the document or the history moves. The selection follows
   * the *node*, not the row number, and falls back to the current node when the
   * selected one is gone — which it can be after a jump replays the document
   * along a different path. Does nothing while closed, so a host may call it on
   * every change without checking.
   */
  refresh(): void {
    if (!this.#state.open) {
      return;
    }
    const rows = this.#read();
    const kept = rows.some((row) => row.id === this.#state.selectedId)
      ? this.#state.selectedId
      : currentId(rows);
    this.#commit({ open: true, rows, selectedId: kept });
  }

  /** Selects a row by node id. A click handler calls this. */
  select(nodeId: string): void {
    if (!this.#state.open || nodeId === this.#state.selectedId) {
      return;
    }
    if (!this.#state.rows.some((row) => row.id === nodeId)) {
      return;
    }
    this.#commit({ ...this.#state, selectedId: nodeId });
  }

  /**
   * Moves the selection one step up the tree, towards the root.
   *
   * "Up" is the parent, not the row above: the row above may belong to another
   * branch entirely, and stepping into a sibling's subtree when the user asked
   * to go back is exactly the confusion a tree view has to avoid.
   */
  selectParent(): void {
    const here = this.selected;
    if (!here) {
      return;
    }
    const index = this.#state.rows.indexOf(here);
    // The parent is the nearest earlier row that is shallower — which holds
    // because the layout is depth-first from the root.
    for (let i = index - 1; i >= 0; i -= 1) {
      const row = this.#state.rows[i];
      if (row !== undefined && row.depth < here.depth) {
        this.select(row.id);
        return;
      }
    }
  }

  /**
   * Moves the selection one step down the active path.
   *
   * Down the *preferred* child, not the first one: that is the branch a redo
   * would take, so holding the down arrow retraces exactly the future the
   * editor would replay.
   */
  selectChild(): void {
    const here = this.selected;
    if (!here) {
      return;
    }
    const children = this.#childrenOf(here);
    const next = children.find((row) => row.onActivePath) ?? children[0];
    if (next) {
      this.select(next.id);
    }
  }

  /**
   * Moves the selection to the next (or previous) sibling, wrapping.
   *
   * Only the selection moves — this does not point redo anywhere. Switching
   * which branch the editor would actually take is what {@link jump} does, and
   * keeping the two apart is what lets a user look down a branch before
   * committing to it.
   *
   * Wraps rather than clamps, because siblings are a ring of alternatives with
   * no natural first or last, and nothing is moving that could overshoot.
   */
  selectSibling(delta: number): void {
    const here = this.selected;
    if (!here || delta === 0) {
      return;
    }
    const siblings = this.#siblingsOf(here);
    if (siblings.length < 2) {
      return;
    }
    const index = siblings.indexOf(here);
    if (index < 0) {
      return;
    }
    const count = siblings.length;
    // `% count` twice, with a `+ count` between, so a negative delta lands in
    // range: JavaScript's remainder keeps the sign of the dividend.
    const next = siblings[(((index + delta) % count) + count) % count];
    if (next) {
      this.select(next.id);
    }
  }

  /**
   * Moves the document to the selected state, and closes.
   *
   * Returns whether anything moved. A jump to the state already occupied
   * succeeds and changes nothing, which is why the panel closes either way:
   * pressing Enter on the current row means "I am done looking", and leaving it
   * open would make that keystroke appear to have failed.
   */
  jump(nodeId?: string): boolean {
    const id = nodeId ?? this.#state.selectedId;
    if (!this.#state.open || id === "") {
      return false;
    }
    this.close();
    return this.#host.jumpToHistoryNode(id);
  }

  #read(): readonly UndoTreeRow[] {
    return Object.freeze(buildRows(this.#host.historySnapshot()));
  }

  /** The rows directly beneath `row`, in creation order. */
  #childrenOf(row: UndoTreeRow): UndoTreeRow[] {
    const start = this.#state.rows.indexOf(row) + 1;
    const children: UndoTreeRow[] = [];
    for (let i = start; i < this.#state.rows.length; i += 1) {
      const candidate = this.#state.rows[i];
      if (candidate === undefined || candidate.depth <= row.depth) {
        break;
      }
      if (candidate.depth === row.depth + 1) {
        children.push(candidate);
      }
    }
    return children;
  }

  /** Every row sharing `row`'s parent, in creation order, including `row`. */
  #siblingsOf(row: UndoTreeRow): UndoTreeRow[] {
    const index = this.#state.rows.indexOf(row);
    if (index < 0) {
      return [];
    }
    let parent: UndoTreeRow | undefined;
    for (let i = index - 1; i >= 0; i -= 1) {
      const candidate = this.#state.rows[i];
      if (candidate !== undefined && candidate.depth < row.depth) {
        parent = candidate;
        break;
      }
    }
    // The root has no parent, and so no siblings to move between.
    return parent === undefined ? [] : this.#childrenOf(parent);
  }

  /**
   * Replaces the state, then notifies.
   *
   * Every listener is called even if an earlier one throws — one broken
   * renderer must not stop the others from seeing the change — and the first
   * error is rethrown once they all have, so it still reaches the console
   * rather than disappearing.
   */
  #commit(state: UndoTreePanelState): void {
    this.#state = Object.freeze(state);
    let failure: unknown;
    let failed = false;
    for (const listener of this.#listeners) {
      try {
        listener(this.#state);
      } catch (error) {
        if (!failed) {
          failure = error;
          failed = true;
        }
      }
    }
    if (failed) {
      throw failure;
    }
  }
}

/** The id of the current row, or the first row, or `""` for an empty tree. */
function currentId(rows: readonly UndoTreeRow[]): string {
  return (rows.find((row) => row.isCurrent) ?? rows[0])?.id ?? "";
}
