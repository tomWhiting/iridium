/**
 * Tests for undo-tree panel key handling. Run with `bun test`.
 *
 * The arrows read as a *tree* here, not as a list, and that is the whole design
 * decision: up and down walk the path the document travels, left and right move
 * between the branches of a fork. Both web faces would otherwise own a copy of
 * this switch, and a copy is how they drift.
 */

import { describe, expect, test } from "bun:test";
import { UndoTreePanel, type UndoTreeHost } from "./index.ts";
import { applyUndoTreeKey, undoTreeKeyAction, type UndoTreeKeyEvent } from "./keys.ts";
import type { UndoTreeSnapshot } from "../controller/index.ts";

function press(key: string, modifiers: Partial<UndoTreeKeyEvent> = {}): UndoTreeKeyEvent {
  return { key, ctrlKey: false, metaKey: false, ...modifiers };
}

/** Root → "1", which forks three ways; the document sits on the middle one. */
function snapshot(): UndoTreeSnapshot {
  const nodes = [
    { id: "0", childIds: ["1"], preferredChildId: "1", elapsedMs: 0, isCurrent: false },
    {
      id: "1",
      parentId: "0",
      childIds: ["2", "3", "4"],
      preferredChildId: "3",
      elapsedMs: 0,
      isCurrent: false,
    },
    { id: "2", parentId: "1", childIds: [], elapsedMs: 0, isCurrent: false },
    { id: "3", parentId: "1", childIds: [], elapsedMs: 0, isCurrent: true },
    { id: "4", parentId: "1", childIds: [], elapsedMs: 0, isCurrent: false },
  ];
  return {
    nodes,
    info: {
      currentId: "3",
      rootId: "0",
      nodeCount: nodes.length,
      canUndo: true,
      canRedo: false,
      branchCount: 0,
    },
  };
}

function panel(): { panel: UndoTreePanel; jumped: string[] } {
  const jumped: string[] = [];
  const host: UndoTreeHost = {
    historySnapshot: snapshot,
    jumpToHistoryNode(nodeId) {
      jumped.push(nodeId);
      return true;
    },
    blurEditor() {},
    focus() {},
  };
  const instance = new UndoTreePanel(host);
  instance.open();
  return { panel: instance, jumped };
}

describe("classifying", () => {
  test("the unmodified keys", () => {
    expect(undoTreeKeyAction(press("Escape"))).toBe("close");
    expect(undoTreeKeyAction(press("Enter"))).toBe("jump");
    expect(undoTreeKeyAction(press("ArrowUp"))).toBe("parent");
    expect(undoTreeKeyAction(press("ArrowDown"))).toBe("child");
    expect(undoTreeKeyAction(press("ArrowRight"))).toBe("nextSibling");
    expect(undoTreeKeyAction(press("ArrowLeft"))).toBe("previousSibling");
  });

  test("Ctrl+H closes, so the key that opened the panel can shut it", () => {
    expect(undoTreeKeyAction(press("h", { ctrlKey: true }))).toBe("close");
    expect(undoTreeKeyAction(press("H", { metaKey: true }))).toBe("close");
  });

  test("Ctrl+N and Ctrl+P alias the vertical arrows, as in the palette", () => {
    expect(undoTreeKeyAction(press("n", { ctrlKey: true }))).toBe("child");
    expect(undoTreeKeyAction(press("p", { metaKey: true }))).toBe("parent");
  });

  test("the same letters unmodified are not ours", () => {
    for (const key of ["h", "n", "p", "H", "N", "P"]) {
      expect(undoTreeKeyAction(press(key))).toBe("none");
    }
  });

  test("anything else is not ours", () => {
    for (const key of ["a", "Tab", " ", "PageDown", "Home", "F1"]) {
      expect(undoTreeKeyAction(press(key))).toBe("none");
    }
  });
});

describe("applying", () => {
  test("the arrows walk the tree", () => {
    const { panel: instance } = panel();
    expect(applyUndoTreeKey(instance, press("ArrowUp"))).toBe(true);
    expect(instance.state.selectedId).toBe("1");
    expect(applyUndoTreeKey(instance, press("ArrowDown"))).toBe(true);
    expect(instance.state.selectedId).toBe("3");
  });

  test("left and right are opposites, which two branches could not prove", () => {
    const { panel: instance } = panel();
    applyUndoTreeKey(instance, press("ArrowRight"));
    expect(instance.state.selectedId).toBe("4");
    applyUndoTreeKey(instance, press("ArrowLeft"));
    expect(instance.state.selectedId).toBe("3");
    applyUndoTreeKey(instance, press("ArrowLeft"));
    expect(instance.state.selectedId).toBe("2");
  });

  test("Enter jumps to the selection and closes", () => {
    const { panel: instance, jumped } = panel();
    applyUndoTreeKey(instance, press("ArrowLeft"));
    expect(applyUndoTreeKey(instance, press("Enter"))).toBe(true);
    expect(jumped).toEqual(["2"]);
    expect(instance.state.open).toBe(false);
  });

  test("Escape closes without touching the document", () => {
    const { panel: instance, jumped } = panel();
    expect(applyUndoTreeKey(instance, press("Escape"))).toBe(true);
    expect(instance.state.open).toBe(false);
    expect(jumped).toEqual([]);
  });

  test("a key that is not ours is left alone", () => {
    const { panel: instance } = panel();
    expect(applyUndoTreeKey(instance, press("a"))).toBe(false);
    expect(instance.state.selectedId).toBe("3");
    expect(instance.state.open).toBe(true);
  });
});
