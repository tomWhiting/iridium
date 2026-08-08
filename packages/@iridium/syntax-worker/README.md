# @iridium-editor/syntax-worker

The tree-sitter syntax worker for
[Iridium](https://github.com/tomWhiting/iridium). It runs parsing and highlight
queries on a Web Worker, so a large file being reparsed cannot drop a frame on
the main thread.

Optional. `@iridium-editor/core` renders perfectly well without it — either
unhighlighted, or with spans you supply yourself through `setHighlightSpans()`.

## Install

```bash
npm install @iridium-editor/syntax-worker @iridium-editor/core
```

`@iridium-editor/core` is a peer dependency: the grammars, the highlight
queries and the client class all live there, and this package is the worker
that drives them.

## Use

Usually you do not touch this package directly — you turn the option on and the
core resolves the worker itself:

```ts
await IridiumEditor.create(canvas, { enableSyntaxWorker: true });
```

That default finds this package as a sibling of `@iridium-editor/core`, which
holds under npm, yarn and bun. Where it does not — pnpm's store layout, or a
bundler that rewrites worker URLs — construct the worker yourself and hand it
over:

```ts
await IridiumEditor.create(canvas, {
  enableSyntaxWorker: true,
  createSyntaxWorker: () =>
    new Worker(new URL("@iridium-editor/syntax-worker/worker", import.meta.url), {
      type: "module",
    }),
});
```

To drive it without the editor at all:

```ts
import { SyntaxHighlightClient } from "@iridium-editor/syntax-worker";

const client = new SyntaxHighlightClient(
  () => new Worker(new URL("@iridium-editor/syntax-worker/worker", import.meta.url), {
    type: "module",
  }),
);
const languages = await client.initialize("rust");
const result = await client.highlight(sourceText);
```

`initialize` resolves with the languages the worker loaded. Beyond `highlight`
the client also offers `highlightIncremental(content, editInfo)` — which reuses
the retained parse tree and is what the editor actually calls while you type —
plus `highlightRange`, `setLanguage`, `cancelPending` and `dispose`.

## Exports

| specifier | what |
|---|---|
| `@iridium-editor/syntax-worker` | `SyntaxHighlightClient` and its types |
| `@iridium-editor/syntax-worker/worker` | the worker entry point — the URL you pass to `new Worker` |
| `@iridium-editor/syntax-worker/protocol` | the request/response message types |

## Cost

Downloading and compiling a grammar takes 1–3 seconds on a large file, which is
why the editor leaves this off by default. When highlighting can come from
somewhere cheaper — a server, a language server, an existing index — prefer
`setHighlightSpans()`.

Offsets cross the boundary as **UTF-8 byte offsets**, matching the Rust core's
rope. `encoding.ts` converts to and from the UTF-16 offsets JavaScript strings
use; getting that wrong is the classic way to make highlighting drift on any
line containing a non-ASCII character.

## Licence

MIT. See `LICENSE`.
