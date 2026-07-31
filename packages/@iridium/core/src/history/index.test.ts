/**
 * Tests for the undo-tree panel state machine. Run with `bun test`.
 *
 * The panel is the only part of the undo tree a user touches directly, and none
 * of its behaviour exists in Rust: what "up" means, what survives a refresh,
 * and when the keyboard goes back to the editor are all decided here.
 */

import { describe, expect, mock, test } from "bun:test";
import { UndoTreePanel, type UndoTreeHost } from "./index.ts";
import type { UndoTreeNode, UndoTreeSnapshot } from "../controller/index.ts";

function node(
  id: string,
  parentId: string | undefined,
  childIds: string[],
  extra: Partial<UndoTreeNode> = {},
): UndoTreeNode {
  return {
    id,
    ...(parentId === undefined ? {} : { parentId }),
    childIds,
    elapsedMs: 0,
    isCurrent: false,
    ...extra,
  };
}

/**
 * A three-way fork below "1", with the document on the middle branch.
 *
 * ```text
 * 0 root
 * └ 1
 *   ├ 2   (branch 0)
 *   ├ 3   (branch 1, current, active path)
 *   │ └ 5
 *   └ 4   (branch 2)
 * ```
 *
 * Three branches rather than two throughout, for the same reason the Rust
 * tests fork three ways: with two, stepping forward and stepping backward land
 * on the same row, and a test cannot tell the two directions apart.
 */
function forkedSnapshot(currentId = "3"): UndoTreeSnapshot {
  const nodes = [
    node("0", undefined, ["1"], { preferredChildId: "1" }),
    node("1", "0", ["2", "3", "4"], { preferredChildId: "3" }),
    node("2", "1", []),
    node("3", "1", ["5"], { preferredChildId: "5" }),
    node("4", "1", []),
    node("5", "3", []),
  ].map((entry) => (entry.id === currentId ? { ...entry, isCurrent: true } : entry));
  return {
    nodes,
    info: {
      currentId,
      rootId: "0",
      nodeCount: nodes.length,
      canUndo: currentId !== "0",
      canRedo: true,
      branchCount: 1,
    },
  };
}

class FakeHost implements UndoTreeHost {
  snapshot: UndoTreeSnapshot = forkedSnapshot();
  jumped: string[] = [];
  jumpResult = true;
  blurs = 0;
  focuses = 0;

  historySnapshot(): UndoTreeSnapshot {
    return this.snapshot;
  }

  jumpToHistoryNode(nodeId: string): boolean {
    this.jumped.push(nodeId);
    return this.jumpResult;
  }

  blurEditor(): void {
    this.blurs += 1;
  }

  focus(): void {
    this.focuses += 1;
  }
}

function opened(host = new FakeHost()): { panel: UndoTreePanel; host: FakeHost } {
  const panel = new UndoTreePanel(host);
  panel.open();
  return { panel, host };
}

describe("opening and closing", () => {
  test("opening selects the state the document is on", () => {
    const { panel, host } = opened();
    expect(panel.state.open).toBe(true);
    expect(panel.state.selectedId).toBe("3");
    expect(panel.selected?.isCurrent).toBe(true);
    // The canvas must stop consuming keys before the panel can have them.
    expect(host.blurs).toBe(1);
  });

  test("closing hands the keyboard back, once", () => {
    const { panel, host } = opened();
    panel.close();
    expect(panel.state.open).toBe(false);
    expect(panel.state.rows).toEqual([]);
    expect(host.focuses).toBe(1);

    // Closing an already-closed panel must not steal focus from whatever has
    // it now — the user may have clicked into something else entirely.
    panel.close();
    expect(host.focuses).toBe(1);
  });

  test("toggling alternates", () => {
    const host = new FakeHost();
    const panel = new UndoTreePanel(host);
    panel.toggle();
    expect(panel.state.open).toBe(true);
    panel.toggle();
    expect(panel.state.open).toBe(false);
    panel.toggle();
    expect(panel.state.open).toBe(true);
  });

  test("reopening re-reads the tree", () => {
    const { panel, host } = opened();
    panel.close();
    host.snapshot = forkedSnapshot("5");
    panel.open();
    expect(panel.state.selectedId).toBe("5");
  });
});

describe("moving the selection", () => {
  test("up is the parent, not the row above", () => {
    // From "3", the row above is "2" — a *sibling*. Stepping into a sibling
    // when the user asked to go back is the confusion a tree view must avoid.
    const { panel } = opened();
    panel.selectParent();
    expect(panel.state.selectedId).toBe("1");
    panel.selectParent();
    expect(panel.state.selectedId).toBe("0");
    // The root has nowhere further up.
    panel.selectParent();
    expect(panel.state.selectedId).toBe("0");
  });

  test("down follows the preferred child, not the first one", () => {
    // From "1" the first child is "2", but redo would take "3".
    const { panel } = opened();
    panel.selectParent();
    expect(panel.state.selectedId).toBe("1");
    panel.selectChild();
    expect(panel.state.selectedId).toBe("3");
    panel.selectChild();
    expect(panel.state.selectedId).toBe("5");
    // A leaf has nowhere further down.
    panel.selectChild();
    expect(panel.state.selectedId).toBe("5");
  });

  test("down falls back to the first child when no branch is preferred", () => {
    const host = new FakeHost();
    host.snapshot = {
      ...forkedSnapshot(),
      nodes: forkedSnapshot().nodes.map((entry) =>
        entry.id === "1" ? { ...entry, preferredChildId: undefined } : entry
      ),
    };
    const panel = new UndoTreePanel(host);
    panel.open();
    panel.selectParent();
    panel.selectChild();
    expect(panel.state.selectedId).toBe("2");
  });

  test("left and right move between the branches of a fork, and wrap", () => {
    const { panel } = opened();
    expect(panel.state.selectedId).toBe("3"); // branch 1 of 3
    panel.selectSibling(1);
    expect(panel.state.selectedId).toBe("4");
    panel.selectSibling(1);
    expect(panel.state.selectedId).toBe("2"); // wrapped
    panel.selectSibling(-1);
    expect(panel.state.selectedId).toBe("4"); // wrapped the other way
    panel.selectSibling(-1);
    expect(panel.state.selectedId).toBe("3");
  });

  test("a node with no siblings does not move", () => {
    const { panel } = opened();
    panel.selectParent(); // "1", an only child
    panel.selectSibling(1);
    expect(panel.state.selectedId).toBe("1");
    panel.selectParent(); // the root, which has no parent at all
    panel.selectSibling(-1);
    expect(panel.state.selectedId).toBe("0");
  });

  test("moving the selection never touches the document", () => {
    // The whole reason browsing is separate from jumping: looking down a
    // branch must be free, or looking is itself an edit.
    const { panel, host } = opened();
    panel.selectParent();
    panel.selectSibling(1);
    panel.selectChild();
    expect(host.jumped).toEqual([]);
  });

  test("selecting by id ignores a row that is not there", () => {
    const { panel } = opened();
    panel.select("404");
    expect(panel.state.selectedId).toBe("3");
    panel.select("2");
    expect(panel.state.selectedId).toBe("2");
  });

  test("a closed panel ignores every navigation", () => {
    const host = new FakeHost();
    const panel = new UndoTreePanel(host);
    panel.selectParent();
    panel.selectChild();
    panel.selectSibling(1);
    panel.select("2");
    expect(panel.state).toEqual({ open: false, rows: [], selectedId: "" });
  });
});

describe("refreshing", () => {
  test("the selection follows the node, not the row number", () => {
    const { panel, host } = opened();
    panel.select("2");
    // A new branch appears above the selection, shifting every row index.
    host.snapshot = {
      ...forkedSnapshot(),
      nodes: [
        node("0", undefined, ["6", "1"], { preferredChildId: "1" }),
        node("6", "0", []),
        ...forkedSnapshot().nodes.slice(1),
      ],
    };
    panel.refresh();
    expect(panel.state.selectedId).toBe("2");
  });

  test("a selection that no longer exists falls back to the current node", () => {
    const { panel, host } = opened();
    panel.select("4");
    host.snapshot = forkedSnapshot("5");
    host.snapshot = {
      ...host.snapshot,
      nodes: host.snapshot.nodes.filter((entry) => entry.id !== "4"),
    };
    panel.refresh();
    expect(panel.state.selectedId).toBe("5");
  });

  test("refreshing a closed panel does nothing, so a host can call it freely", () => {
    const host = new FakeHost();
    const panel = new UndoTreePanel(host);
    const listener = mock(() => {});
    panel.subscribe(listener);
    panel.refresh();
    expect(listener).not.toHaveBeenCalled();
  });
});

describe("jumping", () => {
  test("it moves the document and closes", () => {
    const { panel, host } = opened();
    panel.select("2");
    expect(panel.jump()).toBe(true);
    expect(host.jumped).toEqual(["2"]);
    expect(panel.state.open).toBe(false);
    expect(host.focuses).toBe(1);
  });

  test("an explicit id overrides the selection, for a click", () => {
    const { panel, host } = opened();
    expect(panel.jump("4")).toBe(true);
    expect(host.jumped).toEqual(["4"]);
  });

  test("it closes even when the editor refuses the jump", () => {
    // Leaving the panel open would suggest another key might work; nothing
    // will, because a refused jump means the node is gone.
    const { panel, host } = opened();
    host.jumpResult = false;
    expect(panel.jump()).toBe(false);
    expect(panel.state.open).toBe(false);
  });

  test("a closed panel jumps nowhere", () => {
    const host = new FakeHost();
    const panel = new UndoTreePanel(host);
    expect(panel.jump("2")).toBe(false);
    expect(host.jumped).toEqual([]);
  });

  test("an empty tree has nothing to jump to", () => {
    const host = new FakeHost();
    host.snapshot = { nodes: [], info: { ...forkedSnapshot().info, nodeCount: 0 } };
    const panel = new UndoTreePanel(host);
    panel.open();
    expect(panel.state.selectedId).toBe("");
    expect(panel.jump()).toBe(false);
    expect(host.jumped).toEqual([]);
  });
});

describe("subscribing", () => {
  test("listeners see every change, and unsubscribe cleanly", () => {
    const { panel } = opened();
    const seen: string[] = [];
    const unsubscribe = panel.subscribe((state) => seen.push(state.selectedId));
    panel.select("2");
    panel.select("4");
    unsubscribe();
    panel.select("3");
    expect(seen).toEqual(["2", "4"]);
  });

  test("one broken listener does not blind the others", () => {
    const { panel } = opened();
    const second = mock(() => {});
    panel.subscribe(() => {
      throw new Error("renderer exploded");
    });
    panel.subscribe(second);
    expect(() => panel.select("2")).toThrow("renderer exploded");
    expect(second).toHaveBeenCalledTimes(1);
  });

  test("state is frozen, so a renderer cannot mutate what it was handed", () => {
    const { panel } = opened();
    expect(Object.isFrozen(panel.state)).toBe(true);
    expect(Object.isFrozen(panel.state.rows)).toBe(true);
  });
});
