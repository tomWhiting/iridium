# iridium-bindings

The WebAssembly build of [Iridium](https://github.com/tomWhiting/iridium)'s
editing kernel, plus the tree-sitter grammars it highlights with.

This is the engine, not the editor. Everything a browser needs on top of it —
DOM events, the clipboard, the render loop, the public API — lives in
[`@iridium-editor/core`](https://www.npmjs.com/package/@iridium-editor/core),
which declares this package as a peer dependency. Install both; call the core.

```bash
npm install @iridium-editor/core iridium-bindings
```

## What is inside

`pkg/` is `wasm-pack --target web` output: `iridium_bindings.js` (the glue),
`iridium_bindings_bg.wasm` (the kernel), and the matching `.d.ts` files. The
module is the default export and must be initialised before use:

```ts
import init, { createWebEditor, sanitizePixelRatio } from "iridium-bindings";

await init();
const editor = await createWebEditor(canvas, sanitizePixelRatio(devicePixelRatio));
```

`grammars/` holds the compiled tree-sitter grammars — C++, CSS, Go, HTML,
JavaScript, JSON, Lua, Python, Rust, TOML, TSX, TypeScript and YAML — for hosts
that want to load them directly rather than through
`@iridium-editor/core/syntax`.

The kernel inside is the same Rust that runs the native desktop editor: the
rope, the undo tree, multi-cursor editing, the keymap and chord resolver, the
command registry and the palette's ranking. One implementation, several faces,
so behaviour cannot drift between them.

## Provenance

Every published tarball carries `pkg/PROVENANCE.txt`, naming the commit the
wasm was built from, whether that tree was clean, and the `rustc` and
`wasm-pack` versions that built it. The `pkg/` directory is not committed to
git, so this file is the only record connecting the bytes on npm to a revision
of the source.

## A note on the manifest

`package.json` also carries a `napi` block and a `build` script for
`@napi-rs/cli`. That describes a **Node native addon** built from the same
crate, which is not what this package publishes — `main`, `types`, `exports`
and `files` all point at the wasm output. Splitting the two apart is tracked as
issue #96; until then, treat the napi half as documentation of a sibling build,
not of this artefact.

## Licence

MIT. See `LICENSE`.
