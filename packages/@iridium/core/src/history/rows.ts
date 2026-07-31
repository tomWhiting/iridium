/**
 * Turning an undo-tree snapshot into rows a view can draw.
 *
 * The kernel reports the tree as an unordered set of linked nodes, which is the
 * right shape to send across a boundary and the wrong shape to draw. This turns
 * it into a vertical list: one row per state, depth-first from the root, with
 * the depth and the age each row needs already worked out.
 *
 * Pure functions over plain data — no editor, no DOM — so the layout of the
 * panel is testable without either.
 *
 * @module
 */

import type { UndoTreeNode, UndoTreeSnapshot } from "../controller/index.ts";

/** One state of the document, positioned for drawing. */
export interface UndoTreeRow {
  /** The node id, and what {@link UndoTreePanel.jump} takes. */
  readonly id: string;
  /** How far to indent: the root is `0`, its children `1`, and so on. */
  readonly depth: number;
  /**
   * Whether this row is on the path the document would retrace.
   *
   * That is the chain from the root down through each node's preferred child.
   * Drawing it differently is what makes a fork legible: the branch the user is
   * on stands out from the ones they left behind.
   */
  readonly onActivePath: boolean;
  /** Whether the document is sitting on this state right now. */
  readonly isCurrent: boolean;
  /**
   * Which of its parent's branches this is, and how many there are.
   *
   * `undefined` on the root, which has no parent to be a branch of. A view uses
   * it to label a fork ("2 of 3") without walking the tree again.
   */
  readonly branch?: { readonly index: number; readonly count: number };
  /** How long before the newest edit in the tree this one was made. */
  readonly ageMs: number;
  /** The label the kernel recorded, when it recorded one. */
  readonly description?: string;
}

/**
 * Lays a snapshot out depth-first from the root, children in creation order.
 *
 * Depth-first, not the snapshot's own chronological order: chronological order
 * interleaves branches, so a fork would be drawn as two lists shuffled
 * together. Walking the links keeps each branch contiguous, which is the only
 * arrangement in which indentation means anything.
 *
 * A node the walk cannot reach is dropped rather than appended: an unreachable
 * node has no depth, so there is no honest place to draw it. That cannot happen
 * with a tree the kernel built — every node but the root has a parent — and the
 * guard exists so a malformed snapshot degrades to a smaller drawing instead of
 * an infinite one.
 */
export function buildRows(snapshot: UndoTreeSnapshot): UndoTreeRow[] {
  const byId = new Map<string, UndoTreeNode>();
  for (const node of snapshot.nodes) {
    byId.set(node.id, node);
  }

  // The newest edit anchors every age, so the bottom of the tree reads "now"
  // and everything else is an interval before it.
  let newest = 0;
  for (const node of snapshot.nodes) {
    newest = Math.max(newest, node.elapsedMs);
  }

  const active = activePath(byId, snapshot.info.rootId);
  const rows: UndoTreeRow[] = [];
  const visited = new Set<string>();

  const walk = (id: string, depth: number, branch?: UndoTreeRow["branch"]): void => {
    const node = byId.get(id);
    // `visited` guards against a cycle, which a tree cannot contain but a
    // corrupted snapshot could — and a cycle here would hang the renderer.
    if (!node || visited.has(id)) {
      return;
    }
    visited.add(id);
    rows.push({
      id: node.id,
      depth,
      onActivePath: active.has(node.id),
      isCurrent: node.isCurrent,
      ...(branch ? { branch } : {}),
      ageMs: Math.max(0, newest - node.elapsedMs),
      ...(node.description === undefined ? {} : { description: node.description }),
    });
    const count = node.childIds.length;
    node.childIds.forEach((childId, index) => {
      walk(childId, depth + 1, count > 1 ? { index, count } : undefined);
    });
  };

  walk(snapshot.info.rootId, 0);
  return rows;
}

/**
 * The ids on the chain from the root down through each preferred child.
 *
 * Followed from the root rather than back from the current node, because the
 * path continues *past* the current node: everything below it is what a redo
 * would retrace, and a panel that stopped at the current node would draw the
 * user's own future as an abandoned branch.
 */
function activePath(byId: Map<string, UndoTreeNode>, rootId: string): Set<string> {
  const path = new Set<string>();
  let id: string | undefined = rootId;
  while (id !== undefined && !path.has(id)) {
    path.add(id);
    id = byId.get(id)?.preferredChildId;
  }
  return path;
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;

/**
 * Renders an age as the shortest phrase that is still true.
 *
 * Here rather than in each view for the same reason the ranking is in Rust: two
 * faces disagreeing about whether an edit was "2m" or "3m" ago is a difference
 * nobody can explain and everybody notices. Rounds **down** throughout, so a
 * row never claims to be older than it is.
 */
export function formatAge(ageMs: number): string {
  if (!Number.isFinite(ageMs) || ageMs < 1000) {
    return "now";
  }
  if (ageMs < MINUTE) {
    return `${Math.floor(ageMs / 1000)}s`;
  }
  if (ageMs < HOUR) {
    return `${Math.floor(ageMs / MINUTE)}m`;
  }
  return `${Math.floor(ageMs / HOUR)}h`;
}
