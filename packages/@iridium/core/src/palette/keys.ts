/**
 * What a keystroke means to an open palette.
 *
 * Separated from both views for the same reason the ranking lives in Rust: it is
 * behaviour, and behaviour that differs between the React overlay and the web
 * component is a bug nobody notices until they switch faces. Keeping it a pure
 * function over a key description also makes it testable without a DOM.
 *
 * @module
 */

import type { CommandPalette } from "./index.ts";

/** What an open palette should do about a keystroke. */
export type PaletteKeyAction =
  /** Dismiss and hand the keyboard back. */
  | "close"
  /** Run the highlighted row. */
  | "run"
  /** Move one row down. */
  | "next"
  /** Move one row up. */
  | "previous"
  /** Not ours — let the input have it. */
  | "none";

/** The parts of a keyboard event this depends on. */
export interface PaletteKeyEvent {
  /** `KeyboardEvent.key`. */
  readonly key: string;
  /** Whether Control was held. */
  readonly ctrlKey: boolean;
  /** Whether Command (macOS) was held. */
  readonly metaKey: boolean;
}

/**
 * Classifies a keystroke against an open palette.
 *
 * Returns `"none"` for anything the input should handle itself — ordinary
 * typing, and the emacs-style motions macOS puts on `Ctrl+A`/`Ctrl+E`, which are
 * genuinely useful inside the query field.
 *
 * The `Ctrl+N`/`Ctrl+P` aliases exist because the arrow keys are a reach; note
 * they must be claimed rather than left alone, since macOS otherwise moves the
 * caret by line inside the input.
 *
 * **`Ctrl+K` closes.** It is the key that opened the palette, so pressing it
 * again should do something palette-shaped — and, more pressingly, macOS binds
 * `Ctrl+K` in a text field to kill-to-end-of-line, so leaving it unclaimed makes
 * the palette's own open key silently delete the rest of the user's query.
 * `Meta` counts alongside `Control` throughout, because the web face forwards
 * macOS `Cmd` as the kernel's `ctrl` and both spellings open the palette.
 */
export function paletteKeyAction(event: PaletteKeyEvent): PaletteKeyAction {
  const chord = event.ctrlKey || event.metaKey;
  switch (event.key) {
    case "Escape":
      return "close";
    case "Enter":
      return "run";
    case "ArrowDown":
      return "next";
    case "ArrowUp":
      return "previous";
    case "k":
    case "K":
      return chord ? "close" : "none";
    case "n":
    case "N":
      return chord ? "next" : "none";
    case "p":
    case "P":
      return chord ? "previous" : "none";
    default:
      return "none";
  }
}

/**
 * Applies a keystroke to `palette`, and reports whether it was ours.
 *
 * A `true` return means the caller should suppress the default — every action
 * here has a browser behaviour that would otherwise fire underneath it, from
 * caret motion to kill-to-end-of-line.
 */
export function applyPaletteKey(palette: CommandPalette, event: PaletteKeyEvent): boolean {
  switch (paletteKeyAction(event)) {
    case "close":
      palette.close();
      return true;
    case "run":
      palette.run();
      return true;
    case "next":
      palette.moveSelection(1);
      return true;
    case "previous":
      palette.moveSelection(-1);
      return true;
    case "none":
      return false;
  }
}
