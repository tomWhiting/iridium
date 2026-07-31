/**
 * The undo-tree panel, drawn with plain DOM inside a shadow root.
 *
 * The web component cannot portal to `document.body` the way the React example
 * does: a shadow root's styles do not reach outside it, so a portalled overlay
 * would render unstyled. Everything here therefore mounts *inside* the shadow
 * root and uses `position: fixed` to escape the host element's box.
 *
 * Behaviour is not duplicated. This is a view over the same framework-free
 * {@link UndoTreePanel} the React overlay binds to, laid out by the same
 * {@link buildRows} and aged by the same {@link formatAge}, so the two faces
 * cannot drift.
 *
 * @module
 */

import {
  applyUndoTreeKey,
  formatAge,
  type UndoTreePanel,
  type UndoTreeRow,
} from "../history/index.ts";

/** The width of one level of indentation, in rem. */
const INDENT_REM = 1.125;

const STYLES = `
  .iu-backdrop {
    position: fixed;
    inset: 0;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 12vh;
    background: rgba(0, 0, 0, 0.45);
    z-index: 1000;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  }
  .iu-backdrop[hidden] { display: none; }
  .iu-panel {
    display: flex;
    flex-direction: column;
    width: min(520px, 90vw);
    max-height: 60vh;
    background: #252525;
    color: #e0e0e0;
    border: 1px solid #444;
    border-radius: 8px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.55);
    overflow: hidden;
  }
  .iu-header {
    display: flex;
    align-items: center;
    padding: 0.6rem 1rem;
    border-bottom: 1px solid #333;
    background: #2e2e2e;
    font-size: 0.8rem;
    letter-spacing: 0.02em;
  }
  .iu-count { margin-left: auto; color: #888; }
  .iu-rows { overflow-y: auto; padding: 0.25rem 0; outline: none; }
  .iu-row {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    padding: 0.3rem 1rem;
    cursor: pointer;
    font-size: 0.85rem;
  }
  .iu-row[aria-selected="true"] { background: #3a4a63; }
  .iu-marker { flex-shrink: 0; width: 0.9rem; color: #555; font-size: 0.7rem; }
  .iu-marker.iu-active { color: #8ab4f8; }
  .iu-marker.iu-current { color: #9ece6a; }
  .iu-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .iu-branch {
    flex-shrink: 0;
    padding: 0.05rem 0.35rem;
    border: 1px solid #444;
    border-radius: 4px;
    color: #9aa7b5;
    font-size: 0.65rem;
  }
  .iu-age {
    flex-shrink: 0;
    margin-left: auto;
    color: #777;
    font-family: monospace;
    font-size: 0.7rem;
  }
  .iu-empty { padding: 1rem; color: #888; font-size: 0.875rem; text-align: center; }
  .iu-footer {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.4rem 1rem;
    border-top: 1px solid #333;
    background: #2a2a2a;
    font-size: 0.7rem;
    color: #888;
  }
`;

/**
 * A DOM view over an {@link UndoTreePanel}, mounted into a shadow root.
 *
 * Owns nothing but its own nodes, so {@link UndoTreeView.destroy} is safe on
 * disconnect; it adds and removes exactly what it created.
 */
export class UndoTreeView {
  readonly #panel: UndoTreePanel;
  readonly #style: HTMLStyleElement;
  readonly #backdrop: HTMLDivElement;
  readonly #rows: HTMLDivElement;
  readonly #count: HTMLSpanElement;
  readonly #unsubscribe: () => void;
  #wasOpen = false;

  constructor(root: ShadowRoot, panel: UndoTreePanel) {
    this.#panel = panel;

    this.#style = document.createElement("style");
    this.#style.textContent = STYLES;

    this.#backdrop = document.createElement("div");
    this.#backdrop.className = "iu-backdrop";
    this.#backdrop.hidden = true;
    // Only a press that started on the backdrop itself dismisses, so a drag out
    // of the panel does not close it mid-gesture.
    this.#backdrop.addEventListener("mousedown", (event) => {
      if (event.target === this.#backdrop) {
        this.#panel.close();
      }
    });

    const panelBox = document.createElement("div");
    panelBox.className = "iu-panel";
    panelBox.setAttribute("role", "dialog");
    panelBox.setAttribute("aria-modal", "true");
    panelBox.setAttribute("aria-label", "Undo tree");

    const header = document.createElement("div");
    header.className = "iu-header";
    const title = document.createElement("span");
    title.textContent = "Undo tree";
    this.#count = document.createElement("span");
    this.#count.className = "iu-count";
    header.append(title, this.#count);

    this.#rows = document.createElement("div");
    this.#rows.className = "iu-rows";
    this.#rows.setAttribute("role", "listbox");
    this.#rows.setAttribute("aria-label", "Document states");
    // The list itself takes the keyboard: there is no text field here, and
    // without a focusable element the arrows would scroll the page instead.
    this.#rows.tabIndex = 0;
    this.#rows.addEventListener("keydown", (event) => this.#onKeyDown(event));

    const footer = document.createElement("div");
    footer.className = "iu-footer";
    for (const label of ["↑↓ walk the path", "←→ switch branch", "↵ go there", "esc dismiss"]) {
      const span = document.createElement("span");
      span.textContent = label;
      footer.appendChild(span);
    }

    panelBox.append(header, this.#rows, footer);
    this.#backdrop.appendChild(panelBox);
    root.append(this.#style, this.#backdrop);

    this.#unsubscribe = panel.subscribe(() => this.#render());
    this.#render();
  }

  /** Unsubscribes and removes everything this view added to the shadow root. */
  destroy(): void {
    this.#unsubscribe();
    this.#style.remove();
    this.#backdrop.remove();
  }

  #onKeyDown(event: KeyboardEvent): void {
    // Shared with the React overlay, so the two faces cannot drift. A claimed
    // key always suppresses the default: the arrows would otherwise scroll the
    // page underneath the panel.
    if (applyUndoTreeKey(this.#panel, event)) {
      event.preventDefault();
    }
  }

  #render(): void {
    const state = this.#panel.state;
    this.#backdrop.hidden = !state.open;

    if (!state.open) {
      this.#rows.replaceChildren();
      this.#wasOpen = false;
      return;
    }

    this.#rows.replaceChildren();
    let selectedRow: HTMLElement | null = null;
    if (state.rows.length === 0) {
      const empty = document.createElement("div");
      empty.className = "iu-empty";
      empty.textContent = "Nothing has been edited yet";
      this.#rows.appendChild(empty);
    } else {
      for (const row of state.rows) {
        const element = this.#renderRow(row, row.id === state.selectedId);
        if (row.id === state.selectedId) {
          selectedRow = element;
        }
        this.#rows.appendChild(element);
      }
    }

    const total = state.rows.length;
    this.#count.textContent = `${total} state${total === 1 ? "" : "s"}`;

    if (!this.#wasOpen) {
      // The controller has already blurred the canvas; without this nothing
      // holds the keyboard and the first keystroke is lost.
      this.#rows.focus();
      this.#wasOpen = true;
    }
    selectedRow?.scrollIntoView({ block: "nearest" });
  }

  #renderRow(row: UndoTreeRow, selected: boolean): HTMLElement {
    const element = document.createElement("div");
    element.className = "iu-row";
    element.setAttribute("role", "option");
    element.setAttribute("aria-selected", String(selected));
    if (row.isCurrent) {
      element.setAttribute("aria-current", "true");
    }
    element.style.paddingLeft = `${1 + row.depth * INDENT_REM}rem`;
    // Without this the list blurs before the click lands and the click appears
    // to do nothing at all.
    element.addEventListener("mousedown", (event) => event.preventDefault());
    // Select on click, jump on double-click: the panel is for looking before
    // committing, and a stray click that replayed the document would be the
    // opposite of that.
    element.addEventListener("click", () => this.#panel.select(row.id));
    element.addEventListener("dblclick", () => this.#panel.jump(row.id));

    const marker = document.createElement("span");
    marker.className = `iu-marker${row.onActivePath ? " iu-active" : ""}${
      row.isCurrent ? " iu-current" : ""
    }`;
    marker.setAttribute("aria-hidden", "true");
    marker.textContent = row.isCurrent ? "●" : row.onActivePath ? "○" : "·";
    element.appendChild(marker);

    const label = document.createElement("span");
    label.className = "iu-label";
    label.textContent = row.description ?? (row.depth === 0 ? "opened" : `edit ${row.id}`);
    element.appendChild(label);

    if (row.branch) {
      const badge = document.createElement("span");
      badge.className = "iu-branch";
      badge.textContent = `branch ${row.branch.index + 1}/${row.branch.count}`;
      element.appendChild(badge);
    }

    const age = document.createElement("span");
    age.className = "iu-age";
    age.textContent = formatAge(row.ageMs);
    element.appendChild(age);

    return element;
  }
}
