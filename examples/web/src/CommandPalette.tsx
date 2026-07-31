/**
 * The command palette overlay, rendered with React.
 *
 * Pure presentation. Every decision about *what* the palette does — what is
 * open, what matched, which row is selected, what running one means — belongs to
 * the framework-free `CommandPalette` in `@iridium/core/palette`, and every
 * decision about *ranking* belongs to the Rust kernel. This file draws the
 * result and forwards keystrokes, so the web component and the terminal face can
 * behave identically without sharing a line of rendering code.
 */

import React, { useCallback, useEffect, useRef, useSyncExternalStore } from "react";
import { createPortal } from "react-dom";
import {
  applyPaletteKey,
  CommandPalette as PaletteController,
  highlightSegments,
} from "@iridium/core/palette";
import type { PaletteCommand } from "@iridium/core";

export interface CommandPaletteProps {
  /** The state machine this overlay draws. */
  palette: PaletteController;
}

/**
 * Renders the matched characters in bold.
 *
 * The offsets come from the kernel and the segmentation from `@iridium/core`,
 * because getting UTF-16 widths wrong is silent — a title containing an emoji
 * would highlight half a surrogate pair.
 */
function Highlighted({
  text,
  matches,
}: {
  text: string;
  matches?: readonly number[];
}): React.ReactElement {
  return (
    <>
      {highlightSegments(text, matches).map((segment, index) =>
        segment.matched ? (
          <b key={index} style={styles.match}>
            {segment.text}
          </b>
        ) : (
          <React.Fragment key={index}>{segment.text}</React.Fragment>
        )
      )}
    </>
  );
}

/**
 * One result row.
 *
 * A match against an alias, an id or the description cannot be highlighted
 * inside the title — the offsets index different text — so the row shows *why*
 * it matched on a second line instead of underlining the wrong characters.
 */
function Row({
  command,
  selected,
  macLabels,
  onRun,
  rowRef,
}: {
  command: PaletteCommand;
  selected: boolean;
  macLabels: boolean;
  onRun: () => void;
  rowRef: React.Ref<HTMLDivElement>;
}): React.ReactElement {
  const matchedTitle = command.matchedField === "title";
  const hint = macLabels ? command.keyHintMac : command.keyHint;
  const why = !matchedTitle && command.matchedField !== undefined;

  return (
    <div
      ref={rowRef}
      role="option"
      aria-selected={selected}
      aria-disabled={!command.available}
      style={{
        ...styles.row,
        ...(selected ? styles.rowSelected : null),
        ...(command.available ? null : styles.rowUnavailable),
      }}
      // Without this the input blurs before the click lands, the palette's own
      // focus effect has nothing to restore, and the click appears to do nothing.
      onMouseDown={(event) => event.preventDefault()}
      onClick={onRun}
    >
      <div style={styles.rowText}>
        <div style={styles.rowTitle}>
          {matchedTitle ? (
            <Highlighted text={command.matchedText} matches={command.matches} />
          ) : (
            command.title
          )}
          <span style={styles.category}>{command.category}</span>
        </div>
        {why ? (
          <div style={styles.rowWhy}>
            <span style={styles.field}>{command.matchedField}</span>
            <Highlighted text={command.matchedText} matches={command.matches} />
          </div>
        ) : command.description ? (
          <div style={styles.rowDescription}>{command.description}</div>
        ) : null}
      </div>
      {hint ? <kbd style={styles.hint}>{hint}</kbd> : null}
    </div>
  );
}

/** The palette overlay. Renders nothing at all while closed. */
export function CommandPalette({ palette }: CommandPaletteProps): React.ReactPortal | null {
  const state = useSyncExternalStore(
    useCallback(
      (onStoreChange: () => void) => palette.subscribe(() => onStoreChange()),
      [palette]
    ),
    useCallback(() => palette.state, [palette])
  );

  const inputRef = useRef<HTMLInputElement>(null);
  const selectedRef = useRef<HTMLDivElement>(null);

  // Take the keyboard as soon as the overlay exists. The controller has already
  // blurred the canvas, so without this nothing is focused and typing is lost.
  useEffect(() => {
    if (state.open) {
      inputRef.current?.focus();
    }
  }, [state.open]);

  // Arrowing past the visible rows must follow the highlight, or the selection
  // silently leaves the screen and the palette looks stuck.
  useEffect(() => {
    selectedRef.current?.scrollIntoView({ block: "nearest" });
  }, [state.selectedIndex, state.results]);

  const handleKeyDown = (event: React.KeyboardEvent<HTMLInputElement>): void => {
    // Which keys mean what lives in `@iridium/core`, so this overlay and the web
    // component cannot drift. A claimed key always suppresses the default: every
    // one of them has a browser behaviour underneath, from caret motion to the
    // kill-to-end-of-line macOS puts on Ctrl+K.
    if (applyPaletteKey(palette, event)) {
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
          palette.close();
        }
      }}
    >
      <div style={styles.panel} role="dialog" aria-modal="true" aria-label="Command palette">
        <input
          ref={inputRef}
          style={styles.input}
          value={state.query}
          placeholder="Type a command name…"
          spellCheck={false}
          autoComplete="off"
          aria-autocomplete="list"
          onChange={(event) => palette.setQuery(event.target.value)}
          onKeyDown={handleKeyDown}
        />
        <div style={styles.results} role="listbox" aria-label="Commands">
          {state.results.length === 0 ? (
            <div style={styles.empty}>No matching commands</div>
          ) : (
            state.results.map((command, index) => (
              <Row
                key={command.id}
                command={command}
                selected={index === state.selectedIndex}
                macLabels={palette.usesMacKeyLabels}
                onRun={() => palette.run(command.id)}
                rowRef={index === state.selectedIndex ? selectedRef : null}
              />
            ))
          )}
        </div>
        <div style={styles.footer}>
          <span>↑↓ navigate</span>
          <span>↵ run</span>
          <span>esc dismiss</span>
          <span style={styles.count}>
            {state.results.length} command{state.results.length === 1 ? "" : "s"}
          </span>
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
    fontFamily:
      '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
  },
  panel: {
    display: "flex",
    flexDirection: "column",
    width: "min(640px, 90vw)",
    maxHeight: "60vh",
    backgroundColor: "#252525",
    color: "#e0e0e0",
    border: "1px solid #444",
    borderRadius: "8px",
    boxShadow: "0 16px 48px rgba(0, 0, 0, 0.55)",
    overflow: "hidden",
  },
  input: {
    padding: "0.75rem 1rem",
    border: "none",
    borderBottom: "1px solid #333",
    backgroundColor: "#2e2e2e",
    color: "#e0e0e0",
    fontSize: "0.95rem",
    outline: "none",
  },
  results: {
    overflowY: "auto",
    padding: "0.25rem 0",
  },
  row: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: "0.75rem",
    padding: "0.4rem 1rem",
    cursor: "pointer",
  },
  rowSelected: {
    backgroundColor: "#3a4a63",
  },
  rowUnavailable: {
    opacity: 0.45,
    cursor: "not-allowed",
  },
  rowText: {
    minWidth: 0,
  },
  rowTitle: {
    display: "flex",
    alignItems: "baseline",
    gap: "0.5rem",
    fontSize: "0.875rem",
  },
  rowDescription: {
    fontSize: "0.75rem",
    color: "#888",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  rowWhy: {
    display: "flex",
    alignItems: "baseline",
    gap: "0.4rem",
    fontSize: "0.75rem",
    color: "#9aa7b5",
  },
  category: {
    fontSize: "0.7rem",
    color: "#777",
  },
  field: {
    fontSize: "0.65rem",
    textTransform: "uppercase",
    letterSpacing: "0.04em",
    color: "#6f7c8a",
  },
  match: {
    color: "#8ab4f8",
    fontWeight: 600,
  },
  hint: {
    flexShrink: 0,
    padding: "0.1rem 0.4rem",
    border: "1px solid #444",
    borderRadius: "4px",
    backgroundColor: "#333",
    color: "#bbb",
    fontFamily: "monospace",
    fontSize: "0.7rem",
    whiteSpace: "nowrap",
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
  },
};

export default CommandPalette;
