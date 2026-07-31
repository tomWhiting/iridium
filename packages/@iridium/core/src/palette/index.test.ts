/**
 * Tests for the framework-free palette state machine. Run with `bun test`.
 *
 * The host is a fake, which is the point of the interface being structural: none
 * of this needs a canvas, a GPU or a wasm module, so the behaviour a user
 * actually feels — clamping, focus, what a stale selection would do — is cheap
 * to pin.
 */

import { describe, expect, test } from "bun:test";
import { CommandPalette, type PaletteHost, type PaletteState } from "./index.ts";
import type { PaletteCommand } from "../controller/index.ts";

function command(
  id: string,
  title: string,
  overrides: Partial<PaletteCommand> = {},
): PaletteCommand {
  return {
    id,
    title,
    category: "Test",
    mutatesDocument: false,
    available: true,
    implemented: true,
    score: 0,
    matchedText: title,
    ...overrides,
  };
}

const CATALOGUE: PaletteCommand[] = [
  command("lines.join", "Join Lines", { mutatesDocument: true }),
  command("clipboard.copy", "Copy"),
  command("history.undo", "Undo", { mutatesDocument: true }),
  command("palette.open", "Show All Commands", { implemented: false }),
];

/** A host that records what the palette asked it to do. */
class FakeHost implements PaletteHost {
  usesMacKeyLabels = false;
  catalogue: PaletteCommand[] = CATALOGUE;
  readonly queries: string[] = [];
  readonly ran: string[] = [];
  blurs = 0;
  focuses = 0;
  limitSeen: number | undefined;

  searchCommands(query: string, limit?: number): PaletteCommand[] {
    this.queries.push(query);
    this.limitSeen = limit;
    if (query === "") return [...this.catalogue];
    const needle = query.toLowerCase();
    return this.catalogue.filter((entry) => entry.title.toLowerCase().includes(needle));
  }

  runCommand(id: string): unknown {
    this.ran.push(id);
    return "handled";
  }

  blurEditor(): void {
    this.blurs++;
  }

  focus(): void {
    this.focuses++;
  }
}

function palette(options?: { limit?: number }): { host: FakeHost; ui: CommandPalette } {
  const host = new FakeHost();
  return { host, ui: new CommandPalette(host, options) };
}

describe("opening and closing", () => {
  test("starts closed and empty", () => {
    const { ui, host } = palette();
    expect(ui.state.open).toBe(false);
    expect(ui.state.query).toBe("");
    expect(ui.state.results).toEqual([]);
    expect(ui.selected).toBeUndefined();
    expect(host.queries).toEqual([]);
  });

  test("opening lists everything and takes the keyboard from the editor", () => {
    const { ui, host } = palette();
    ui.open();

    expect(ui.state.open).toBe(true);
    expect(ui.state.results).toHaveLength(CATALOGUE.length);
    expect(ui.state.selectedIndex).toBe(0);
    expect(host.queries).toEqual([""]);
    expect(host.blurs).toBe(1);
    expect(host.focuses).toBe(0);
  });

  test("closing returns the keyboard to the editor", () => {
    const { ui, host } = palette();
    ui.open();
    ui.close();

    expect(ui.state.open).toBe(false);
    expect(ui.state.results).toEqual([]);
    expect(host.focuses).toBe(1);
  });

  test("closing an already-closed palette does nothing", () => {
    // Guards against stealing focus back from whatever has it now.
    const { ui, host } = palette();
    ui.close();
    expect(host.focuses).toBe(0);

    const seen: PaletteState[] = [];
    ui.subscribe((state) => seen.push(state));
    ui.close();
    expect(seen).toEqual([]);
  });

  test("reopening starts blank rather than restoring the last query", () => {
    // A palette that reopens holding the previous query makes old text look like
    // something the user just typed.
    const { ui } = palette();
    ui.open();
    ui.setQuery("undo");
    ui.close();
    ui.open();

    expect(ui.state.query).toBe("");
    expect(ui.state.results).toHaveLength(CATALOGUE.length);
  });

  test("toggle opens then closes", () => {
    const { ui, host } = palette();
    ui.toggle();
    expect(ui.state.open).toBe(true);
    ui.toggle();
    expect(ui.state.open).toBe(false);
    expect(host.blurs).toBe(1);
    expect(host.focuses).toBe(1);
  });
});

describe("querying", () => {
  test("typing searches on every keystroke, undebounced", () => {
    const { ui, host } = palette();
    ui.open();
    for (const query of ["u", "un", "und"]) {
      ui.setQuery(query);
    }

    expect(host.queries).toEqual(["", "u", "un", "und"]);
    expect(ui.state.results.map((entry) => entry.id)).toEqual(["history.undo"]);
  });

  test("a new query resets the selection to the top", () => {
    // The row at index two of the old results is a different command in the new
    // ones; keeping the index would move the highlight onto something unlooked-at.
    const { ui } = palette();
    ui.open();
    ui.moveSelection(2);
    expect(ui.state.selectedIndex).toBe(2);

    ui.setQuery("o");
    expect(ui.state.selectedIndex).toBe(0);
  });

  test("a query matching nothing leaves an empty, selectable-by-nothing list", () => {
    const { ui } = palette();
    ui.open();
    ui.setQuery("zzzzz");

    expect(ui.state.results).toEqual([]);
    expect(ui.selected).toBeUndefined();
    expect(ui.state.selectedIndex).toBe(0);
    expect(ui.run()).toBe(false);
  });

  test("typing while closed does nothing", () => {
    const { ui, host } = palette();
    ui.setQuery("undo");
    expect(ui.state.open).toBe(false);
    expect(host.queries).toEqual([]);
  });

  test("the configured limit reaches the host", () => {
    const { ui, host } = palette({ limit: 12 });
    ui.open();
    expect(host.limitSeen).toBe(12);
  });
});

describe("selection", () => {
  test("clamps at both ends rather than wrapping", () => {
    // Wrapping overshoots under key repeat: the list jumps from the last row to
    // the first while the user is still holding the key.
    const { ui } = palette();
    ui.open();

    ui.moveSelection(-1);
    expect(ui.state.selectedIndex).toBe(0);

    ui.moveSelection(999);
    expect(ui.state.selectedIndex).toBe(CATALOGUE.length - 1);

    ui.moveSelection(1);
    expect(ui.state.selectedIndex).toBe(CATALOGUE.length - 1);
  });

  test("steps one row at a time and tracks the selected command", () => {
    const { ui } = palette();
    ui.open();

    ui.moveSelection(1);
    expect(ui.selected?.id).toBe(CATALOGUE[1].id);
    ui.moveSelection(-1);
    expect(ui.selected?.id).toBe(CATALOGUE[0].id);
  });

  test("selecting by index clamps too, and is a no-op with no results", () => {
    const { ui } = palette();
    ui.open();
    ui.selectIndex(99);
    expect(ui.state.selectedIndex).toBe(CATALOGUE.length - 1);

    ui.setQuery("zzzzz");
    ui.selectIndex(3);
    expect(ui.state.selectedIndex).toBe(0);
  });

  test("selecting the row already selected notifies nobody", () => {
    const { ui } = palette();
    ui.open();
    const seen: PaletteState[] = [];
    ui.subscribe((state) => seen.push(state));

    ui.selectIndex(0);
    expect(seen).toEqual([]);
  });
});

describe("running", () => {
  test("runs the highlighted command, closes, and restores focus", () => {
    const { ui, host } = palette();
    ui.open();
    ui.moveSelection(1);

    expect(ui.run()).toBe(true);
    expect(host.ran).toEqual(["clipboard.copy"]);
    expect(ui.state.open).toBe(false);
    expect(host.focuses).toBe(1);
  });

  test("closes before running, so a host command may open its own overlay", () => {
    // A `palette.open` bound to a key the host handles would otherwise be opened
    // by the host and closed again by this palette's own teardown.
    const host = new FakeHost();
    const openWhenRun: boolean[] = [];
    let ui!: CommandPalette;
    host.runCommand = (id: string): unknown => {
      openWhenRun.push(ui.state.open);
      host.ran.push(id);
      return "handled";
    };

    ui = new CommandPalette(host);
    ui.open();
    ui.run();

    expect(openWhenRun).toEqual([false]);
    expect(host.focuses).toBe(1);
  });

  test("runs a specific id regardless of the highlight", () => {
    const { ui, host } = palette();
    ui.open();

    expect(ui.run("history.undo")).toBe(true);
    expect(host.ran).toEqual(["history.undo"]);
  });

  test("an id that is not in the results runs nothing", () => {
    const { ui, host } = palette();
    ui.open();
    ui.setQuery("undo");

    expect(ui.run("clipboard.copy")).toBe(false);
    expect(host.ran).toEqual([]);
    expect(ui.state.open).toBe(true);
  });

  test("an unavailable command does not run and does not dismiss the palette", () => {
    // Pressing Enter on a greyed-out row must not look like it worked.
    const host = new FakeHost();
    host.catalogue = [command("lines.join", "Join Lines", {
      mutatesDocument: true,
      available: false,
    })];
    const ui = new CommandPalette(host);
    ui.open();

    expect(ui.run()).toBe(false);
    expect(host.ran).toEqual([]);
    expect(ui.state.open).toBe(true);
    expect(host.focuses).toBe(0);
  });

  test("a host command runs like any other — the host decides what it means", () => {
    const { ui, host } = palette();
    ui.open();

    expect(ui.run("palette.open")).toBe(true);
    expect(host.ran).toEqual(["palette.open"]);
  });
});

describe("subscription", () => {
  test("notifies on every change with the committed state", () => {
    const { ui } = palette();
    const seen: PaletteState[] = [];
    ui.subscribe((state) => seen.push(state));

    ui.open();
    ui.setQuery("o");
    ui.moveSelection(1);
    ui.close();

    expect(seen.map((state) => state.open)).toEqual([true, true, true, false]);
    // Every snapshot is coherent: the results always match the query that
    // produced them, never one keystroke behind.
    expect(seen[1].query).toBe("o");
    expect(seen[1].results.every((entry) => entry.title.toLowerCase().includes("o"))).toBe(true);
  });

  test("unsubscribing stops the notifications", () => {
    const { ui } = palette();
    let count = 0;
    const stop = ui.subscribe(() => count++);

    ui.open();
    stop();
    ui.setQuery("o");

    expect(count).toBe(1);
  });

  test("unsubscribing from inside a listener does not disturb the others", () => {
    const { ui } = palette();
    const order: string[] = [];
    let stopFirst = (): void => {};
    stopFirst = ui.subscribe(() => {
      order.push("first");
      stopFirst();
    });
    ui.subscribe(() => order.push("second"));

    ui.open();
    expect(order).toEqual(["first", "second"]);

    order.length = 0;
    ui.setQuery("o");
    expect(order).toEqual(["second"]);
  });

  test("one throwing listener does not rob the others of the change", () => {
    const { ui } = palette();
    const reached: string[] = [];
    ui.subscribe(() => {
      throw new Error("renderer exploded");
    });
    ui.subscribe(() => reached.push("second"));

    expect(() => ui.open()).toThrow("renderer exploded");
    expect(reached).toEqual(["second"]);
    expect(ui.state.open).toBe(true);
  });
});

describe("platform labels", () => {
  test("the mac spelling is passed through from the host", () => {
    const { ui, host } = palette();
    expect(ui.usesMacKeyLabels).toBe(false);
    host.usesMacKeyLabels = true;
    expect(ui.usesMacKeyLabels).toBe(true);
  });
});
