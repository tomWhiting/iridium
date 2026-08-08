# @iridium-editor/core

The TypeScript face of [Iridium](https://github.com/tomWhiting/iridium), a
GPU-accelerated text editor whose editing kernel is written in Rust and shipped
to the browser as WebAssembly.

The editor draws to a `<canvas>` through WebGPU. Text shaping, the rope, the
undo tree, multi-cursor editing, the keymap and the command palette's ranking
all live in the Rust core — this package is the browser's half: it owns the DOM
events, the clipboard, the render loop, and the API you call.

## Install

```bash
npm install @iridium-editor/core iridium-bindings
```

`iridium-bindings` carries the wasm module and is a **peer** dependency, so you
control which build you get. Syntax highlighting off the main thread is
optional:

```bash
npm install @iridium-editor/syntax-worker   # only if you want it
```

**Requires WebGPU.** The editor renders through it and has no 2D-canvas
fallback; check [caniuse.com/webgpu](https://caniuse.com/webgpu) for where that
leaves you today.

## Use

```ts
import { IridiumEditor } from "@iridium-editor/core";

const canvas = document.querySelector("canvas")!;

const editor = await IridiumEditor.create(canvas, {
  content: "fn main() {\n    println!(\"hello\");\n}\n",
  language: "rust",
  onChange: (text) => console.log(text.length, "chars"),
});
```

The canvas must be laid out and visible before `create` resolves — it reads its
size from `getBoundingClientRect()` and throws if that is still zero after
waiting.

### Options

| option | default | notes |
|---|---|---|
| `content` | `""` | initial text |
| `language` | `"rust"` | see `getAvailableLanguages()` |
| `darkTheme` | `true` | |
| `autoFocus` | `true` | **set `false` when the editor shares a page with another input** — see below |
| `fontUrl` | FiraCode from jsDelivr | raw sfnt (`.ttf`/`.otf`) only; **`.woff2` is not decoded** |
| `enableSyntaxWorker` | `false` | tree-sitter in a worker; costs 1–3 s on large files |
| `createSyntaxWorker` | — | supply the `Worker` yourself; see below |
| `onChange` | — | `(content: string) => void` |
| `onSelectionChange` | — | `{ line, column, hasSelection }` |
| `onMouseHover` | — | debounced 400 ms; `null` when the mouse leaves |
| `onScroll` | — | wheel or cursor movement |
| `onBeforeKeyDown` | — | return `true` to consume the event before the editor sees it |
| `onSearchAction` | — | `"open"`/`"next"`/`"prev"`/`"close"` |
| `onHostCommand` | — | a command the kernel resolved but does not implement — yours to run |
| `onPendingKeySequence` | — | the half-typed chord, e.g. `"Ctrl+K"`; render it or the editor looks stuck |

### `autoFocus`

`create()` focuses the canvas as soon as the editor is ready. If anything else
on the page can hold the caret, pass `autoFocus: false` — otherwise the editor
takes focus mid-typing and the keystrokes that follow go to the canvas. There
is no way for a host to defend against this from outside, because the focus
happens inside `create()`.

### The syntax worker

With `enableSyntaxWorker: true` the default resolves
`@iridium-editor/syntax-worker`'s built worker as a sibling of this package,
which is how npm, yarn and bun lay scoped packages out. If your resolver does
not (pnpm's store, some bundlers), hand the worker over:

```ts
await IridiumEditor.create(canvas, {
  enableSyntaxWorker: true,
  createSyntaxWorker: () =>
    new Worker(new URL("@iridium-editor/syntax-worker/worker", import.meta.url), {
      type: "module",
    }),
});
```

Highlighting can also come from outside entirely — `setHighlightSpans()` takes
`{ start, end, type }` byte spans from wherever you like, with no worker and no
grammar download.

## The surface

Content and state — `getContent`, `setContent`, `getState`, `insertText`,
`applyCompletion`, `setReadOnly`, `isReadOnly`, `setLanguage`,
`getAvailableLanguages`, `destroy`.

History — `undo`, `redo`, `redoBranch`, `historySnapshot`, `jumpToHistoryNode`.
The undo tree keeps branches rather than discarding them, so nothing typed is
ever unreachable.

Commands — `listCommands`, `searchCommands`, `runCommand`, `keyHintFor`,
`hostCommands`, `setUserKeymap`, `clearUserKeymap`, `pendingKeySequence`.
Ranking, key labels and match offsets are all computed in Rust so every face
that embeds the kernel orders and renders them identically.

Appearance — `setTheme`, `setSyntaxTheme`, `setGutterEnabled`, `setSyntaxEnabled`,
`setLineBackgrounds`, `setGutterChanges`, `setCustomGutterText`, `setBlameData`.

Folding — `foldAll`, `unfoldAll`, `toggleFold`, `toggleFoldAtCursor`.

Geometry and focus — `getLayoutMetrics`, `positionToPixel`, `focus`,
`blurEditor`, `usesMacKeyLabels`, `cursorCount`.

### Subpath exports

| specifier | what |
|---|---|
| `@iridium-editor/core` | the controller (default) |
| `@iridium-editor/core/element` | `<iridium-editor>` custom element |
| `@iridium-editor/core/palette` | framework-free command-palette state machine |
| `@iridium-editor/core/history` | framework-free undo-tree panel state |
| `@iridium-editor/core/syntax` | bundled grammars and highlight queries |
| `@iridium-editor/core/worker-client` | the syntax worker's client |
| `@iridium-editor/core/worker-protocol` | its message types |

## Licence

MIT. See `LICENSE`.
