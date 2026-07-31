/**
 * A framework-free command palette state machine.
 *
 * No DOM, no React, no rendering. It owns the four things every palette gets
 * wrong — what is open, what was typed, what matched, and which row is
 * selected — and leaves drawing entirely to the caller. React binds to it, the
 * web component binds to it, and a terminal face could bind to it, all with the
 * same behaviour.
 *
 * Ranking is **not** here: it lives in the Rust kernel, so that the same query
 * puts the same command first in every face. This file only decides *when* to
 * ask.
 *
 * @module
 */

import type { PaletteCommand } from "../controller/index.ts";

/**
 * The editor surface a palette needs.
 *
 * Structural, not a class: `IridiumEditor` satisfies it, and so does a fake in a
 * test. Keeping it this narrow is what lets the state machine be tested without
 * a canvas, a GPU or a wasm module.
 */
export interface PaletteHost {
  /** Commands matching `query`, best first; empty query returns everything. */
  searchCommands(query: string, limit?: number): PaletteCommand[];
  /** Runs a command by id. */
  runCommand(id: string): unknown;
  /** Releases the keyboard so the palette's own input can take it. */
  blurEditor(): void;
  /** Returns the keyboard to the editor. */
  focus(): void;
  /** Whether key labels should read `⌘K` rather than `Ctrl+K`. */
  readonly usesMacKeyLabels: boolean;
}

/** Everything a renderer needs, and nothing it does not. */
export interface PaletteState {
  /** Whether the palette is showing. */
  readonly open: boolean;
  /** What the user has typed. */
  readonly query: string;
  /** The matches, best first. Empty while closed. */
  readonly results: readonly PaletteCommand[];
  /**
   * The highlighted row, always a valid index into {@link results} unless
   * `results` is empty, in which case it is `0`.
   */
  readonly selectedIndex: number;
}

/** Notified after every state change, with the new state. */
export type PaletteListener = (state: PaletteState) => void;

/** Tuning knobs; every one has a sensible default. */
export interface PaletteOptions {
  /**
   * The most rows to return per search. `0` means every match.
   *
   * A cap is a rendering concern, not a correctness one — the ranking is total,
   * so the top of a capped list is the top of an uncapped one.
   */
  readonly limit?: number;
}

const CLOSED: PaletteState = Object.freeze({
  open: false,
  query: "",
  results: Object.freeze([]) as readonly PaletteCommand[],
  selectedIndex: 0,
});

/**
 * The palette's behaviour, independent of how it is drawn.
 *
 * Every mutator leaves the state consistent before notifying, so a listener
 * always reads a coherent snapshot — never a query that has been updated with
 * results that have not.
 */
export class CommandPalette {
  readonly #host: PaletteHost;
  readonly #limit: number;
  #listeners: PaletteListener[] = [];
  #state: PaletteState = CLOSED;

  constructor(host: PaletteHost, options: PaletteOptions = {}) {
    this.#host = host;
    this.#limit = options.limit ?? 0;
  }

  /** The current state. Immutable; replaced wholesale on every change. */
  get state(): PaletteState {
    return this.#state;
  }

  /** The highlighted command, or `undefined` when nothing matched. */
  get selected(): PaletteCommand | undefined {
    return this.#state.results[this.#state.selectedIndex];
  }

  /** Whether key labels should read `⌘K` rather than `Ctrl+K`. */
  get usesMacKeyLabels(): boolean {
    return this.#host.usesMacKeyLabels;
  }

  /**
   * Subscribes to state changes. Returns the unsubscribe function.
   *
   * Listeners are notified from a snapshot of the list, so subscribing or
   * unsubscribing from inside a listener affects the *next* notification rather
   * than corrupting the current one.
   */
  subscribe(listener: PaletteListener): () => void {
    this.#listeners = [...this.#listeners, listener];
    return () => {
      this.#listeners = this.#listeners.filter((entry) => entry !== listener);
    };
  }

  /**
   * Opens the palette, blank, with everything listed.
   *
   * Always a fresh start, even when already open: a palette that reopens holding
   * the last query makes the previous invocation's text look like something the
   * user just typed. The editor is blurred so its canvas stops consuming keys;
   * that also abandons any half-typed chord, which is what opening a palette
   * should do.
   */
  open(): void {
    this.#host.blurEditor();
    this.#commit({
      open: true,
      query: "",
      results: this.#search(""),
      selectedIndex: 0,
    });
  }

  /**
   * Closes the palette and returns the keyboard to the editor.
   *
   * Restoring focus is not optional politeness: an editor that has to be clicked
   * back into after every command is the single thing that makes a palette feel
   * broken. Closing an already-closed palette does nothing and notifies nobody.
   */
  close(): void {
    if (!this.#state.open) {
      return;
    }
    this.#commit(CLOSED);
    this.#host.focus();
  }

  /** Opens the palette, or closes it if it is already open. */
  toggle(): void {
    if (this.#state.open) {
      this.close();
    } else {
      this.open();
    }
  }

  /**
   * Sets the query and re-searches.
   *
   * Deliberately **not** debounced. The matcher is Rust and takes microseconds
   * over the command set; a debounce would only add lag the user can feel while
   * saving work that was never expensive.
   *
   * The selection resets to the top, because the row at index three of the old
   * results is a different command in the new ones — keeping the index would
   * silently move the highlight onto something the user never looked at.
   */
  setQuery(query: string): void {
    if (!this.#state.open) {
      return;
    }
    this.#commit({
      open: true,
      query,
      results: this.#search(query),
      selectedIndex: 0,
    });
  }

  /**
   * Moves the selection by `delta` rows, **clamped** at both ends.
   *
   * Clamping rather than wrapping: with key repeat, a wrapping list jumps from
   * the last row to the first while the user is still holding the key, and they
   * overshoot in a direction they were not travelling.
   */
  moveSelection(delta: number): void {
    this.selectIndex(this.#state.selectedIndex + delta);
  }

  /** Selects a row by index, clamped into range. A click handler calls this. */
  selectIndex(index: number): void {
    if (!this.#state.open || this.#state.results.length === 0) {
      return;
    }
    const clamped = Math.min(Math.max(index, 0), this.#state.results.length - 1);
    if (clamped === this.#state.selectedIndex) {
      return;
    }
    this.#commit({ ...this.#state, selectedIndex: clamped });
  }

  /**
   * Runs a command and closes the palette. Returns whether anything ran.
   *
   * With no `id`, runs the highlighted row. Returns `false` — leaving the palette
   * open — when there is nothing to run, or when the command is unavailable in
   * the current buffer, so pressing Enter on a greyed-out row does not silently
   * dismiss the palette as though it had worked.
   *
   * The palette closes *before* the command runs, so that a host command which
   * opens another overlay is not immediately closed by this one.
   */
  run(id?: string): boolean {
    const command = id === undefined
      ? this.selected
      : this.#state.results.find((entry) => entry.id === id);
    if (!command || !command.available) {
      return false;
    }
    this.close();
    this.#host.runCommand(command.id);
    return true;
  }

  #search(query: string): readonly PaletteCommand[] {
    return Object.freeze(this.#host.searchCommands(query, this.#limit));
  }

  /**
   * Replaces the state, then notifies.
   *
   * Every listener is called even if an earlier one throws — one broken renderer
   * must not stop the others from seeing the change — and the first error is
   * rethrown once they all have, so it still reaches the console rather than
   * disappearing.
   */
  #commit(state: PaletteState): void {
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
