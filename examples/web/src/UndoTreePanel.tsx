/**
 * The undo-tree panel overlay, rendered with React.
 *
 * Pure presentation. Every decision about *what* the panel does — where the
 * arrows go, what survives a refresh, what jumping means — belongs to the
 * framework-free `UndoTreePanel` in `@iridium/core/history`, and the tree itself
 * belongs to the Rust kernel. This file draws rows and forwards keystrokes, so
 * the web component and a terminal face can behave identically without sharing a
 * line of rendering code.
 */

import React, { useCallback, useEffect, useRef, useSyncExternalStore } from "react";
import { createPortal } from "react-dom";
import {
  applyUndoTreeKey,
  formatAge,
  UndoTreePanel as PanelController,
  type UndoTreeRow,
} from "@iridium/core/history";

export interface UndoTreePanelProps {
  /** The state machine this overlay draws. */
  panel: PanelController;
}

/** The width of one level of indentation, in pixels. */
const INDENT = 18;

/**
 * One state of the document.
 *
 * The three things a row has to say apart are: is this where I am, is this on
 * the path I would retrace, and is this a fork. They are deliberately different
 * signals — a background, a rail colour and a badge — because a user scanning
 * the panel is answering all three at once.
 */
function Row({
  row,
  selected,
  onJump,
  onSelect,
  rowRef,
}: {
  row: UndoTreeRow;
  selected: boolean;
  onJump: () => void;
  onSelect: () => void;
  rowRef: React.Ref<HTMLDivElement>;
}): React.ReactElement {
  return (
    <div
      ref={rowRef}
      role="option"
      aria-selected={selected}
      aria-current={row.isCurrent}
      style={{
        ...styles.row,
        ...(selected ? styles.rowSelected : null),
        paddingLeft: `${0.75 + (row.depth * INDENT) / 16}rem`,
      }}
      // Without this the panel blurs before the click lands, its focus effect
      // has nothing to restore, and the click appears to do nothing.
      onMouseDown={(event) => event.preventDefault()}
      onClick={onSelect}
      onDoubleClick={onJump}
    >
      <span
        style={{
          ...styles.marker,
          ...(row.onActivePath ? styles.markerActive : null),
          ...(row.isCurrent ? styles.markerCurrent : null),
        }}
        aria-hidden="true"
      >
        {row.isCurrent ? "●" : row.onActivePath ? "○" : "·"}
      </span>
      <span style={styles.label}>
        {row.description ?? (row.depth === 0 ? "opened" : `edit ${row.id}`)}
      </span>
      {row.branch ? (
        <span style={styles.branch}>
          branch {row.branch.index + 1}/{row.branch.count}
        </span>
      ) : null}
      <span style={styles.age}>{formatAge(row.ageMs)}</span>
    </div>
  );
}

/** The undo-tree overlay. Renders nothing at all while closed. */
export function UndoTreePanel({ panel }: UndoTreePanelProps): React.ReactPortal | null {
  const state = useSyncExternalStore(
    useCallback(
      (onStoreChange: () => void) => panel.subscribe(() => onStoreChange()),
      [panel]
    ),
    useCallback(() => panel.state, [panel])
  );

  const listRef = useRef<HTMLDivElement>(null);
  const selectedRef = useRef<HTMLDivElement>(null);

  // Take the keyboard as soon as the overlay exists. The controller has already
  // blurred the canvas, so without this nothing is focused and the arrows would
  // scroll the page instead of walking the tree.
  useEffect(() => {
    if (state.open) {
      listRef.current?.focus();
    }
  }, [state.open]);

  // Walking past the visible rows must follow the highlight, or the selection
  // silently leaves the screen and the panel looks stuck.
  useEffect(() => {
    selectedRef.current?.scrollIntoView({ block: "nearest" });
  }, [state.selectedId, state.rows]);

  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>): void => {
    // Which keys mean what lives in `@iridium/core`, so this overlay and the web
    // component cannot drift. A claimed key always suppresses the default: the
    // arrows would otherwise scroll the page underneath the panel.
    if (applyUndoTreeKey(panel, event)) {
      event.preventDefault();
    }
  };

  if (!state.open) {
    return null;
  }

  return createPortal(
    <div
      style={styles.backdrop}
      // Only a click on the backdrop itself dismisses; one that started inside
      // the panel and drifted out must not.
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          panel.close();
        }
      }}
    >
      <div style={styles.panel} role="dialog" aria-modal="true" aria-label="Undo tree">
        <div style={styles.header}>
          <span>Undo tree</span>
          <span style={styles.count}>
            {state.rows.length} state{state.rows.length === 1 ? "" : "s"}
          </span>
        </div>
        <div
          ref={listRef}
          style={styles.rows}
          role="listbox"
          aria-label="Document states"
          tabIndex={0}
          onKeyDown={handleKeyDown}
        >
          {state.rows.length === 0 ? (
            <div style={styles.empty}>Nothing has been edited yet</div>
          ) : (
            state.rows.map((row) => (
              <Row
                key={row.id}
                row={row}
                selected={row.id === state.selectedId}
                onSelect={() => panel.select(row.id)}
                onJump={() => panel.jump(row.id)}
                rowRef={row.id === state.selectedId ? selectedRef : null}
              />
            ))
          )}
        </div>
        <div style={styles.footer}>
          <span>↑↓ walk the path</span>
          <span>←→ switch branch</span>
          <span>↵ go there</span>
          <span>esc dismiss</span>
        </div>
      </div>
    </div>,
    document.body
  );
}

const styles: Record<string, React.CSSProperties> = {
  backdrop: {
    position: "fixed",
    inset: 0,
    display: "flex",
    justifyContent: "center",
    alignItems: "flex-start",
    paddingTop: "12vh",
    backgroundColor: "rgba(0, 0, 0, 0.45)",
    zIndex: 1000,
    fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
  },
  panel: {
    display: "flex",
    flexDirection: "column",
    width: "min(520px, 90vw)",
    maxHeight: "60vh",
    backgroundColor: "#252525",
    color: "#e0e0e0",
    border: "1px solid #444",
    borderRadius: "8px",
    boxShadow: "0 16px 48px rgba(0, 0, 0, 0.55)",
    overflow: "hidden",
  },
  header: {
    display: "flex",
    alignItems: "center",
    padding: "0.6rem 1rem",
    borderBottom: "1px solid #333",
    backgroundColor: "#2e2e2e",
    fontSize: "0.8rem",
    letterSpacing: "0.02em",
  },
  rows: {
    overflowY: "auto",
    padding: "0.25rem 0",
    outline: "none",
  },
  row: {
    display: "flex",
    alignItems: "baseline",
    gap: "0.5rem",
    padding: "0.3rem 1rem",
    cursor: "pointer",
    fontSize: "0.85rem",
  },
  rowSelected: {
    backgroundColor: "#3a4a63",
  },
  marker: {
    flexShrink: 0,
    width: "0.9rem",
    color: "#555",
    fontSize: "0.7rem",
  },
  markerActive: {
    color: "#8ab4f8",
  },
  markerCurrent: {
    color: "#9ece6a",
  },
  label: {
    minWidth: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  branch: {
    flexShrink: 0,
    padding: "0.05rem 0.35rem",
    border: "1px solid #444",
    borderRadius: "4px",
    color: "#9aa7b5",
    fontSize: "0.65rem",
  },
  age: {
    flexShrink: 0,
    marginLeft: "auto",
    color: "#777",
    fontFamily: "monospace",
    fontSize: "0.7rem",
  },
  empty: {
    padding: "1rem",
    color: "#888",
    fontSize: "0.875rem",
    textAlign: "center",
  },
  footer: {
    display: "flex",
    alignItems: "center",
    gap: "1rem",
    padding: "0.4rem 1rem",
    borderTop: "1px solid #333",
    backgroundColor: "#2a2a2a",
    fontSize: "0.7rem",
    color: "#888",
  },
  count: {
    marginLeft: "auto",
    color: "#888",
  },
};

export default UndoTreePanel;
