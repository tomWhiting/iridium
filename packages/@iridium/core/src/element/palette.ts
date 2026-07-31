/**
 * The command palette, drawn with plain DOM inside a shadow root.
 *
 * The web component cannot portal to `document.body` the way the React example
 * does: a shadow root's styles do not reach outside it, so a portalled overlay
 * would render unstyled. Everything here therefore mounts *inside* the shadow
 * root and uses `position: fixed` to escape the host element's box.
 *
 * Behaviour is not duplicated. This is a view over the same framework-free
 * {@link CommandPalette} the React overlay binds to, and the same
 * {@link highlightSegments} both faces share, so the two cannot drift.
 *
 * @module
 */

import type { PaletteCommand } from "../controller/index.ts";
import { type CommandPalette, highlightSegments } from "../palette/index.ts";

const STYLES = `
  .ip-backdrop {
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
  .ip-backdrop[hidden] { display: none; }
  .ip-panel {
    display: flex;
    flex-direction: column;
    width: min(640px, 90vw);
    max-height: 60vh;
    background: #252525;
    color: #e0e0e0;
    border: 1px solid #444;
    border-radius: 8px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.55);
    overflow: hidden;
  }
  .ip-input {
    padding: 0.75rem 1rem;
    border: none;
    border-bottom: 1px solid #333;
    background: #2e2e2e;
    color: #e0e0e0;
    font-size: 0.95rem;
    font-family: inherit;
    outline: none;
  }
  .ip-results { overflow-y: auto; padding: 0.25rem 0; }
  .ip-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.4rem 1rem;
    cursor: pointer;
  }
  .ip-row[aria-selected="true"] { background: #3a4a63; }
  .ip-row[aria-disabled="true"] { opacity: 0.45; cursor: not-allowed; }
  .ip-text { min-width: 0; }
  .ip-title {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    font-size: 0.875rem;
  }
  .ip-description {
    font-size: 0.75rem;
    color: #888;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ip-why {
    display: flex;
    align-items: baseline;
    gap: 0.4rem;
    font-size: 0.75rem;
    color: #9aa7b5;
  }
  .ip-category { font-size: 0.7rem; color: #777; }
  .ip-field {
    font-size: 0.65rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: #6f7c8a;
  }
  .ip-match { color: #8ab4f8; font-weight: 600; }
  .ip-hint {
    flex-shrink: 0;
    padding: 0.1rem 0.4rem;
    border: 1px solid #444;
    border-radius: 4px;
    background: #333;
    color: #bbb;
    font-family: monospace;
    font-size: 0.7rem;
    white-space: nowrap;
  }
  .ip-empty {
    padding: 1rem;
    color: #888;
    font-size: 0.875rem;
    text-align: center;
  }
  .ip-footer {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.4rem 1rem;
    border-top: 1px solid #333;
    background: #2a2a2a;
    font-size: 0.7rem;
    color: #888;
  }
  .ip-count { margin-left: auto; }
`;

/** Builds the matched-run markup for `text`, bolding what the query hit. */
function renderHighlighted(
  target: HTMLElement,
  text: string,
  matches?: readonly number[],
): void {
  for (const segment of highlightSegments(text, matches)) {
    if (segment.matched) {
      const mark = document.createElement("b");
      mark.className = "ip-match";
      mark.textContent = segment.text;
      target.appendChild(mark);
    } else {
      target.appendChild(document.createTextNode(segment.text));
    }
  }
}

/**
 * A palette overlay bound to one shadow root.
 *
 * Construct it once when the element initialises and call {@link destroy} on
 * disconnect; it adds and removes exactly what it created.
 */
export class PaletteView {
  readonly #palette: CommandPalette;
  readonly #style: HTMLStyleElement;
  readonly #backdrop: HTMLDivElement;
  readonly #input: HTMLInputElement;
  readonly #results: HTMLDivElement;
  readonly #count: HTMLSpanElement;
  readonly #unsubscribe: () => void;
  #wasOpen = false;

  constructor(root: ShadowRoot, palette: CommandPalette) {
    this.#palette = palette;

    this.#style = document.createElement("style");
    this.#style.textContent = STYLES;

    this.#backdrop = document.createElement("div");
    this.#backdrop.className = "ip-backdrop";
    this.#backdrop.hidden = true;
    // Only a press that started on the backdrop itself dismisses, so a drag out
    // of the panel does not close it mid-gesture.
    this.#backdrop.addEventListener("mousedown", (event) => {
      if (event.target === this.#backdrop) {
        this.#palette.close();
      }
    });

    const panel = document.createElement("div");
    panel.className = "ip-panel";
    panel.setAttribute("role", "dialog");
    panel.setAttribute("aria-modal", "true");
    panel.setAttribute("aria-label", "Command palette");

    this.#input = document.createElement("input");
    this.#input.className = "ip-input";
    this.#input.placeholder = "Type a command name…";
    this.#input.spellcheck = false;
    this.#input.autocomplete = "off";
    this.#input.setAttribute("aria-autocomplete", "list");
    this.#input.addEventListener("input", () => {
      this.#palette.setQuery(this.#input.value);
    });
    this.#input.addEventListener("keydown", (event) => this.#onKeyDown(event));

    this.#results = document.createElement("div");
    this.#results.className = "ip-results";
    this.#results.setAttribute("role", "listbox");
    this.#results.setAttribute("aria-label", "Commands");

    const footer = document.createElement("div");
    footer.className = "ip-footer";
    for (const label of ["↑↓ navigate", "↵ run", "esc dismiss"]) {
      const span = document.createElement("span");
      span.textContent = label;
      footer.appendChild(span);
    }
    this.#count = document.createElement("span");
    this.#count.className = "ip-count";
    footer.appendChild(this.#count);

    panel.append(this.#input, this.#results, footer);
    this.#backdrop.appendChild(panel);
    root.append(this.#style, this.#backdrop);

    this.#unsubscribe = palette.subscribe(() => this.#render());
    this.#render();
  }

  /** Unsubscribes and removes everything this view added to the shadow root. */
  destroy(): void {
    this.#unsubscribe();
    this.#style.remove();
    this.#backdrop.remove();
  }

  #onKeyDown(event: KeyboardEvent): void {
    switch (event.key) {
      case "Escape":
        event.preventDefault();
        this.#palette.close();
        break;
      case "Enter":
        event.preventDefault();
        this.#palette.run();
        break;
      case "ArrowDown":
        event.preventDefault();
        this.#palette.moveSelection(1);
        break;
      case "ArrowUp":
        event.preventDefault();
        this.#palette.moveSelection(-1);
        break;
      case "n":
      case "N":
        if (event.ctrlKey) {
          event.preventDefault();
          this.#palette.moveSelection(1);
        }
        break;
      case "p":
      case "P":
        if (event.ctrlKey) {
          event.preventDefault();
          this.#palette.moveSelection(-1);
        }
        break;
      default:
        break;
    }
  }

  #render(): void {
    const state = this.#palette.state;
    this.#backdrop.hidden = !state.open;

    if (!state.open) {
      this.#results.replaceChildren();
      this.#wasOpen = false;
      return;
    }

    // Assigning an identical value would still reset the caret in some engines,
    // so only write when the controller and the field actually disagree.
    if (this.#input.value !== state.query) {
      this.#input.value = state.query;
    }

    this.#results.replaceChildren();
    let selectedRow: HTMLElement | null = null;
    if (state.results.length === 0) {
      const empty = document.createElement("div");
      empty.className = "ip-empty";
      empty.textContent = "No matching commands";
      this.#results.appendChild(empty);
    } else {
      for (let index = 0; index < state.results.length; index++) {
        const row = this.#renderRow(state.results[index], index === state.selectedIndex);
        if (index === state.selectedIndex) {
          selectedRow = row;
        }
        this.#results.appendChild(row);
      }
    }

    const total = state.results.length;
    this.#count.textContent = `${total} command${total === 1 ? "" : "s"}`;

    if (!this.#wasOpen) {
      // The controller has already blurred the canvas; without this nothing holds
      // the keyboard and the first keystroke is lost.
      this.#input.focus();
      this.#wasOpen = true;
    }
    selectedRow?.scrollIntoView({ block: "nearest" });
  }

  #renderRow(command: PaletteCommand, selected: boolean): HTMLElement {
    const row = document.createElement("div");
    row.className = "ip-row";
    row.setAttribute("role", "option");
    row.setAttribute("aria-selected", String(selected));
    row.setAttribute("aria-disabled", String(!command.available));
    // Without this the input blurs before the click lands and the click appears
    // to do nothing at all.
    row.addEventListener("mousedown", (event) => event.preventDefault());
    row.addEventListener("click", () => this.#palette.run(command.id));

    const text = document.createElement("div");
    text.className = "ip-text";

    const title = document.createElement("div");
    title.className = "ip-title";
    const matchedTitle = command.matchedField === "title";
    if (matchedTitle) {
      renderHighlighted(title, command.matchedText, command.matches);
    } else {
      title.appendChild(document.createTextNode(command.title));
    }
    const category = document.createElement("span");
    category.className = "ip-category";
    category.textContent = command.category;
    title.appendChild(category);
    text.appendChild(title);

    if (!matchedTitle && command.matchedField !== undefined) {
      // The offsets index the alias, id or description — not the title — so the
      // row shows that text rather than underlining the wrong characters.
      const why = document.createElement("div");
      why.className = "ip-why";
      const field = document.createElement("span");
      field.className = "ip-field";
      field.textContent = command.matchedField;
      why.appendChild(field);
      renderHighlighted(why, command.matchedText, command.matches);
      text.appendChild(why);
    } else if (command.description) {
      const description = document.createElement("div");
      description.className = "ip-description";
      description.textContent = command.description;
      text.appendChild(description);
    }

    row.appendChild(text);

    const hint = this.#palette.usesMacKeyLabels ? command.keyHintMac : command.keyHint;
    if (hint) {
      const chip = document.createElement("kbd");
      chip.className = "ip-hint";
      chip.textContent = hint;
      row.appendChild(chip);
    }
    return row;
  }
}
