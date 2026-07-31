/**
 * Tests for palette key handling. Run with `bun test`.
 *
 * These are the keystrokes a user presses hundreds of times, and the two web
 * faces would otherwise each own a copy of this switch. Pinning it here is what
 * stops them drifting.
 */

import { describe, expect, test } from "bun:test";
import { CommandPalette, type PaletteHost } from "./index.ts";
import { applyPaletteKey, paletteKeyAction, type PaletteKeyEvent } from "./keys.ts";
import type { PaletteCommand } from "../controller/index.ts";

function press(key: string, modifiers: Partial<PaletteKeyEvent> = {}): PaletteKeyEvent {
  return { key, ctrlKey: false, metaKey: false, ...modifiers };
}

describe("classifying", () => {
  test("the unmodified navigation keys", () => {
    expect(paletteKeyAction(press("Escape"))).toBe("close");
    expect(paletteKeyAction(press("Enter"))).toBe("run");
    expect(paletteKeyAction(press("ArrowDown"))).toBe("next");
    expect(paletteKeyAction(press("ArrowUp"))).toBe("previous");
  });

  test("Ctrl+N and Ctrl+P alias the arrows", () => {
    expect(paletteKeyAction(press("n", { ctrlKey: true }))).toBe("next");
    expect(paletteKeyAction(press("p", { ctrlKey: true }))).toBe("previous");
  });

  test("Ctrl+K closes rather than killing to end of line", () => {
    // macOS binds Ctrl+K in a text field to kill-to-end-of-line. Leaving it
    // unclaimed would make the palette's own open key silently delete the rest
    // of the query — the worst kind of defect, because it looks like nothing
    // happened.
    expect(paletteKeyAction(press("k", { ctrlKey: true }))).toBe("close");
  });

  test("Cmd counts alongside Ctrl, because both open the palette", () => {
    // The web face forwards macOS Cmd as the kernel's ctrl, so Cmd+K opens it
    // and Cmd+K must therefore close it too.
    expect(paletteKeyAction(press("k", { metaKey: true }))).toBe("close");
    expect(paletteKeyAction(press("n", { metaKey: true }))).toBe("next");
    expect(paletteKeyAction(press("p", { metaKey: true }))).toBe("previous");
  });

  test("the letters are claimed in either case", () => {
    // A held Shift, or caps lock, still reports an uppercase `key`.
    for (const [lower, upper] of [["k", "K"], ["n", "N"], ["p", "P"]]) {
      expect(paletteKeyAction(press(upper, { ctrlKey: true }))).toBe(
        paletteKeyAction(press(lower, { ctrlKey: true })),
      );
    }
  });

  test("those same letters unmodified are just typing", () => {
    // Claiming a bare `k` would make the palette unable to search for "kebab".
    for (const key of ["k", "K", "n", "N", "p", "P", "a", "z", " ", "1"]) {
      expect(paletteKeyAction(press(key))).toBe("none");
    }
  });

  test("emacs motions macOS puts in text fields are left alone", () => {
    // Ctrl+A and Ctrl+E are useful inside the query and belong to the input.
    for (const key of ["a", "e", "b", "f", "d", "h"]) {
      expect(paletteKeyAction(press(key, { ctrlKey: true }))).toBe("none");
    }
  });

  test("keys with no palette meaning are not claimed", () => {
    for (const key of ["Tab", "Home", "End", "PageDown", "Backspace", "F5"]) {
      expect(paletteKeyAction(press(key))).toBe("none");
    }
  });
});

const CATALOGUE: PaletteCommand[] = [
  { id: "a.one", title: "One", category: "T", mutatesDocument: false, available: true, implemented: true, score: 0, matchedText: "One" },
  { id: "a.two", title: "Two", category: "T", mutatesDocument: false, available: true, implemented: true, score: 0, matchedText: "Two" },
  { id: "a.three", title: "Three", category: "T", mutatesDocument: false, available: true, implemented: true, score: 0, matchedText: "Three" },
];

class FakeHost implements PaletteHost {
  readonly usesMacKeyLabels = false;
  readonly ran: string[] = [];
  focuses = 0;
  searchCommands(): PaletteCommand[] {
    return [...CATALOGUE];
  }
  runCommand(id: string): unknown {
    this.ran.push(id);
    return null;
  }
  blurEditor(): void {}
  focus(): void {
    this.focuses++;
  }
}

describe("applying", () => {
  function open(): { host: FakeHost; ui: CommandPalette } {
    const host = new FakeHost();
    const ui = new CommandPalette(host);
    ui.open();
    return { host, ui };
  }

  test("a claimed key acts and reports that the default must be suppressed", () => {
    const { ui } = open();
    expect(applyPaletteKey(ui, press("ArrowDown"))).toBe(true);
    expect(ui.state.selectedIndex).toBe(1);
    expect(applyPaletteKey(ui, press("p", { ctrlKey: true }))).toBe(true);
    expect(ui.state.selectedIndex).toBe(0);
  });

  test("Enter runs the highlighted row", () => {
    const { host, ui } = open();
    applyPaletteKey(ui, press("ArrowDown"));
    expect(applyPaletteKey(ui, press("Enter"))).toBe(true);
    expect(host.ran).toEqual(["a.two"]);
    expect(ui.state.open).toBe(false);
  });

  test("both Escape and Ctrl+K dismiss and restore focus", () => {
    for (const event of [press("Escape"), press("k", { ctrlKey: true })]) {
      const { host, ui } = open();
      expect(applyPaletteKey(ui, event)).toBe(true);
      expect(ui.state.open).toBe(false);
      expect(host.focuses).toBe(1);
    }
  });

  test("an unclaimed key changes nothing and lets the input have it", () => {
    const { ui } = open();
    expect(applyPaletteKey(ui, press("k"))).toBe(false);
    expect(ui.state.open).toBe(true);
    expect(ui.state.selectedIndex).toBe(0);
  });
});
