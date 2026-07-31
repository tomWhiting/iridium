/**
 * Tests for undo-tree layout. Run with `bun test`.
 *
 * The kernel hands over an unordered set of linked nodes; everything a panel
 * draws — order, indentation, which branch is live, how old each edit is — is
 * worked out here. None of it is checked by any Rust test, because none of it
 * exists in Rust.
 */

import { describe, expect, test } from "bun:test";
import { buildRows, formatAge } from "./rows.ts";
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

function snapshot(nodes: UndoTreeNode[], currentId: string): UndoTreeSnapshot {
  return {
    nodes,
    info: {
      currentId,
      rootId: "0",
      nodeCount: nodes.length,
      canUndo: currentId !== "0",
      canRedo: false,
      branchCount: 0,
    },
  };
}

/**
 * A fork: the root has branches "1" (carrying "2") and "3", and the active
 * path runs root → 3.
 *
 * Deliberately asymmetric in depth, so a layout that assumed every branch was
 * one node long would produce visibly wrong indentation.
 */
function forked(): UndoTreeSnapshot {
  return snapshot(
    [
      node("0", undefined, ["1", "3"], { preferredChildId: "3", elapsedMs: 0 }),
      node("1", "0", ["2"], { preferredChildId: "2", elapsedMs: 1_000 }),
      node("2", "1", [], { elapsedMs: 2_000, description: "typed b" }),
      node("3", "0", [], { elapsedMs: 65_000, isCurrent: true }),
    ],
    "3",
  );
}

describe("laying out a tree", () => {
  test("depth-first from the root, so each branch stays contiguous", () => {
    // Chronological order would give 0, 1, 2, 3 — which interleaves nothing
    // here, but the *depths* are what prove the walk followed the links.
    const rows = buildRows(forked());
    expect(rows.map((row) => row.id)).toEqual(["0", "1", "2", "3"]);
    expect(rows.map((row) => row.depth)).toEqual([0, 1, 2, 1]);
  });

  test("the active path runs from the root past the current node", () => {
    const rows = buildRows(forked());
    const active = rows.filter((row) => row.onActivePath).map((row) => row.id);
    expect(active).toEqual(["0", "3"]);
  });

  test("the active path continues below the current node", () => {
    // The user has undone back to the root; "3" is still where redo would go,
    // so it is still on the path. A panel that walked *up* from the current
    // node would draw the user's own future as an abandoned branch.
    const tree = forked();
    const rows = buildRows(
      snapshot(
        tree.nodes.map((entry) =>
          entry.id === "3"
            ? { ...entry, isCurrent: false }
            : entry.id === "0"
            ? { ...entry, isCurrent: true }
            : entry
        ),
        "0",
      ),
    );
    expect(rows.filter((row) => row.onActivePath).map((row) => row.id)).toEqual(["0", "3"]);
  });

  test("a fork labels its branches, and a lone child does not", () => {
    const rows = buildRows(forked());
    const branches = rows.map((row) => row.branch);
    expect(branches[0]).toBeUndefined(); // the root is nobody's branch
    expect(branches[1]).toEqual({ index: 0, count: 2 });
    expect(branches[3]).toEqual({ index: 1, count: 2 });
    // "2" is an only child: calling it "1 of 1" would invent a fork.
    expect(branches[2]).toBeUndefined();
  });

  test("ages are measured back from the newest edit", () => {
    const rows = buildRows(forked());
    expect(rows.map((row) => row.ageMs)).toEqual([65_000, 64_000, 63_000, 0]);
  });

  test("the current node and the descriptions survive the walk", () => {
    const rows = buildRows(forked());
    expect(rows.filter((row) => row.isCurrent).map((row) => row.id)).toEqual(["3"]);
    expect(rows[2]?.description).toBe("typed b");
    expect(rows[3]?.description).toBeUndefined();
  });

  test("a linear history is a flat list", () => {
    const rows = buildRows(
      snapshot(
        [
          node("0", undefined, ["1"], { preferredChildId: "1" }),
          node("1", "0", ["2"], { preferredChildId: "2" }),
          node("2", "1", [], { isCurrent: true }),
        ],
        "2",
      ),
    );
    expect(rows.map((row) => row.depth)).toEqual([0, 1, 2]);
    expect(rows.every((row) => row.onActivePath)).toBe(true);
    expect(rows.every((row) => row.branch === undefined)).toBe(true);
  });

  test("a tree of only the root is one row", () => {
    const rows = buildRows(snapshot([node("0", undefined, [], { isCurrent: true })], "0"));
    expect(rows).toHaveLength(1);
    expect(rows[0]?.depth).toBe(0);
    expect(rows[0]?.onActivePath).toBe(true);
  });
});

describe("surviving a malformed snapshot", () => {
  // None of these can come from the kernel. They are here because the failure
  // mode of the walk is a hung renderer, not a wrong number, and "it cannot
  // happen" is not a reason to hang if it does.

  test("a cycle terminates instead of hanging", () => {
    const rows = buildRows(
      snapshot(
        [
          node("0", undefined, ["1"], { preferredChildId: "1" }),
          node("1", "0", ["0"], { preferredChildId: "0", isCurrent: true }),
        ],
        "1",
      ),
    );
    expect(rows.map((row) => row.id)).toEqual(["0", "1"]);
  });

  test("a child that names no node is skipped, not drawn empty", () => {
    const rows = buildRows(
      snapshot([node("0", undefined, ["1", "404"], { isCurrent: true }), node("1", "0", [])], "0"),
    );
    expect(rows.map((row) => row.id)).toEqual(["0", "1"]);
  });

  test("a node unreachable from the root is dropped rather than misplaced", () => {
    // It has no depth, so there is no honest indentation for it.
    const rows = buildRows(
      snapshot([node("0", undefined, [], { isCurrent: true }), node("9", "7", [])], "0"),
    );
    expect(rows.map((row) => row.id)).toEqual(["0"]);
  });

  test("a root the node list does not contain gives no rows at all", () => {
    expect(buildRows(snapshot([], "0"))).toEqual([]);
  });
});

describe("rendering an age", () => {
  test("anything under a second reads as now", () => {
    expect(formatAge(0)).toBe("now");
    expect(formatAge(999)).toBe("now");
  });

  test("it rounds down, so a row never claims to be older than it is", () => {
    expect(formatAge(1_999)).toBe("1s");
    expect(formatAge(59_999)).toBe("59s");
    expect(formatAge(119_999)).toBe("1m");
    expect(formatAge(3_599_999)).toBe("59m");
    expect(formatAge(7_199_999)).toBe("1h");
  });

  test("it steps up exactly at each boundary", () => {
    expect(formatAge(1_000)).toBe("1s");
    expect(formatAge(60_000)).toBe("1m");
    expect(formatAge(3_600_000)).toBe("1h");
  });

  test("a nonsense age reads as now rather than as NaN", () => {
    expect(formatAge(Number.NaN)).toBe("now");
    expect(formatAge(-1)).toBe("now");
  });
});
