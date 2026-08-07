# #64 — the legacy `iridium-bindings/ts` tree

**Finding, verified by hand. Needs one ruling from Tom before anything is
deleted, because deleting it withdraws something two config files call
"supported".**

## What the task note said, and what is actually true

The note reads *"21.9 MB tracked grammar bundle that nothing imports, and the
builder writes to it"*. Both halves are true, and the situation is larger than
one file.

## The whole tree is dead, not just the bundle

`crates/iridium-bindings/ts/` — 21 MB tracked — holds `controller/index.ts`,
`element/index.ts` and `syntax/{index,core.gen,grammars.gen,queries}.ts`.

**Nothing anywhere imports any of it.** Every reference to the name
`iridium-bindings` in the repo, excluding `node_modules` and the package's own
`package.json`, is a *vite alias declaration*:

```
examples/web/vite.config.ts:25            "iridium-bindings"          → ts/controller/index.ts
examples/web/vite.config.ts:26            "iridium-bindings/wasm"     → pkg/iridium_bindings.js
examples/web-component/vite.config.ts:18  "iridium-bindings/element"  → ts/element/index.ts
examples/web-component/vite.config.ts:19  "iridium-bindings/wasm"     → pkg/iridium_bindings.js
examples/web-component/vite.config.ts:20  "iridium-bindings"          → ts/controller/index.ts
```

They are aliases for names no source file uses. Both examples import
`@iridium/core/*` exclusively. The `/wasm` aliases point at `pkg/`, which is
the wasm build and is *not* part of this — only the `ts/` tree is.

## It is a fork that fell a long way behind

`packages/@iridium/core/src/` is the live copy. Comparing file by file:

| file | legacy | live |
| --- | --- | --- |
| `syntax/queries.ts` | 34,031 | **byte-identical** |
| `syntax/index.ts` | 14,890 | 15,199 |
| `element/index.ts` | 6,365 | 10,869 |
| `controller/index.ts` | 34,707 | **65,800** |

The controller is **half the size of the live one**. It is a snapshot from
before roughly half the current feature set — no palette, no undo-tree
navigation. "Still supported", as the vite comments call it, is a claim rather
than a fact: anyone reaching for it would get an editor missing the two
features most recently built.

## The builder writes only to the dead copy

`packages/tree-sitter-builder/build.ts:88`

```ts
const OUTPUT_DIR = join(__dirname, "../../crates/iridium-bindings/ts/syntax");
```

and writes `core.gen.ts`, `grammars.gen.ts` and `queries.ts` there
(`:411-413`), printing all three paths as its output.

**This is the part that is a defect rather than clutter.** Rebuilding the
grammars reports success, writes 21 MB, and changes nothing that runs — the
live bundle at `packages/@iridium/core/src/syntax/grammars.gen.ts` is
untouched. There is no copy step; that file was last written on 30 July and
the legacy one on 14 January.

## The sizes do not match, and that matters

Live `grammars.gen.ts` is **2.0 MB**; the builder's output is **21 MB**. They
are not the same artefact built twice — so *repointing `OUTPUT_DIR` at the
live path is not a safe mechanical fix*. It would replace a 2 MB bundle with a
21 MB one and put ten times the weight into the browser payload. Whatever
produces the 2 MB file is not this builder, and that has to be found before
the builder is repointed.

## The ruling needed

**Does the legacy `crates/iridium-bindings/ts/` tree go?**

Recommend **yes, delete the tree and the five vite aliases**. It has no
importers, it is a stale fork of code that has since doubled, and keeping it
means every grammar rebuild silently writes 21 MB nobody reads.

If it goes, `OUTPUT_DIR` still needs an answer, and that is a second question:
what produces the live 2 MB bundle today? Until that is known, the honest
interim is to have the builder **fail loudly** rather than write to a path
nothing reads.

Not done unilaterally: withdrawing a surface two config files call supported
is a product decision, and 21 MB of tracked history is not a deletion to make
on my own reading of a grep.
