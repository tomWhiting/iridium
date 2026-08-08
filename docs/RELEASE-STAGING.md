# Publishing the Iridium packages — staging assessment

**Assignment:** Tom, via Waffles the Terrible, 9 Aug 2026 — prepare everything
publishable in this repo for publication at head so that Manifold (and anyone
else) never has to vendor again. **Stage only; Tom runs the publish himself.**

Head at assessment: `098abe73`. Manifold currently vendors at `2e9a224c`.

---

## Two corrections to the brief, measured not assumed

Both change what has to happen, so they are first.

### 1. `iridium-bindings` is at **0.1.1** on npm, not 0.1.0

```
$ npm view iridium-bindings versions dist-tags.latest
versions = [ '0.1.0', '0.1.1' ]
dist-tags.latest = '0.1.1'
```

And the local `package.json` also says `0.1.1`. **Every local version already
equals its published version**, so nothing can publish without a bump — npm
refuses to overwrite an existing version.

| package | local | npm latest | collides? |
|---|---|---|---|
| `@iridium-editor/core` | 0.1.1 | 0.1.1 | **yes** |
| `@iridium-editor/syntax-worker` | 0.1.0 | 0.1.0 | **yes** |
| `iridium-bindings` | 0.1.1 | 0.1.1 | **yes** |

### 2. `syntax-worker` is already published

Waffles asked whether it is "meant to be consumable". It is already on npm at
0.1.0, so the question is settled by fact rather than by intent.

---

## ⭐⭐ The finding that decides the whole assignment

**No published version of `@iridium-editor/core` could ever load its own wasm.**

`controller/index.ts` resolved the core by walking five directories up from
`import.meta.url`:

```ts
new URL("../../../../../crates/iridium-bindings/pkg/iridium_bindings.js", import.meta.url)
```

That arithmetic is written for this repository's layout. Resolved from an
installed package it lands somewhere else entirely — measured, not argued:

```
$ node -e '…new URL("../../../../../crates/…", "file:///proj/node_modules/@iridium-editor/core/dist/controller/index.js")'
/proj/crates/iridium-bindings/pkg/iridium_bindings.js
```

It walks out of `node_modules` into the consumer's project root and looks for a
`crates/` directory that only exists here. **So the package has never been
installable-and-usable, in any version.** That — not the `.ts` specifiers
below — is the reason a consumer ends up vendoring: vendoring is what puts the
source back inside a tree where that path resolves.

The same defect, second instance: the syntax worker's default URL named
`../../../syntax-worker/src/worker.ts` — **TypeScript source**, which no
`Worker` constructor can load. So `enableSyntaxWorker: true` could not work
either.

**Both fixed.** The wasm now arrives as `import("iridium-bindings")` — the peer
dependency the manifest already declared, resolvable by any bundler, Node, or
an import map. The worker default now names `syntax-worker/dist/worker.js`,
which resolves correctly in both layouts (verified with `new URL`, both cases),
plus a `createSyntaxWorker` option for resolvers where the sibling assumption
does not hold.

## ⭐ The second finding

**`@iridium-editor/core` publishes raw TypeScript that does not typecheck.**

Its `exports` map points at `.ts` source and `files` ships `src/`. Run `tsc`
over it and you get 38 errors:

```
src/controller/index.ts(22,54): error TS5097: An import path can only end with
  a '.ts' extension when 'allowImportingTsExtensions' is enabled.
src/history/index.test.ts(9,46): error TS2307: Cannot find module 'bun:test'
```

Two distinct defects in the shipped tarball:

1. **38 `.ts` import specifiers.** A consumer only resolves these under Bun,
   Deno, or a bundler configured to tolerate them. Under plain `tsc` — which is
   what a library consumer's typecheck runs — it fails.
2. **Test files are shipped and compiled.** `src/**/*.test.ts` are inside
   `files: ["src/"]`, and they import `bun:test`, which a consumer does not
   have.

⚠️ **This is almost certainly why Manifold vendors instead of consuming.**
Vendoring is the workaround for a package you cannot import; deleting the
vendoring script means fixing this, not just bumping a version. Publishing
another raw-TS version would re-ship the reason the script exists.

**Recommendation: give core a real build step** — compile to `dist/` with
`.d.ts`, point `exports` at the built output, keep every export *specifier*
identical so no consumer import path changes, and drop tests from `files`.
Additive for consumers: the same `import { IridiumEditor } from
"@iridium-editor/core"` starts working in toolchains where it currently
cannot.

⭐ **This has since been staged and verified** — see STAGING PROGRESS below.
Tom's standing rule is to decide rather than ask, and the evidence pointed one
way: Manifold vendors *because* it cannot consume, so fixing this is the
assignment rather than a side quest. `tsc` exits 0, tests no longer ship, and
every export specifier is unchanged.

---

## Other defects found while staging

### `deno.json` and `package.json` disagree about the version

`packages/@iridium/core/deno.json` says `0.1.0`; `package.json` says `0.1.1`.
Two manifests in one package, and nothing checks they agree. Fixed as part of
staging, and worth a test.

### `iridium-bindings`' manifest describes two packages (#96)

`scripts.build` is `napi build --platform --release` and there is a
`napi.triples` block naming seven targets — a Node **native addon**. But
`main`, `types`, `exports` and `files` all point at `pkg/`, which is
**wasm-pack** output. The napi half is never referenced by anything published.

What actually ships today is the wasm bundle; the napi block is misleading
weight. **Not fixed in this release** — splitting it is #96 and doing it inside
a "get everything published" pass is how consumers end up straddling two
packages. Flagged in the changelog instead.

### `pkg/` is gitignored (#97)

`.gitignore:34` ignores `pkg/`, so the published wasm has **no committed
provenance** — nothing in git says which SHA produced the bytes on npm. The
current `pkg/` is dated **8 Aug 10:45**, which predates the font fix
(`2e9a224c`) and the whole #31 theme arc. It **must** be rebuilt before publish.

Fixed by `scripts/build-wasm.sh`, which stamps the SHA it built from, rather
than by committing 3.6 MB of wasm to git.

---

## Naming: `iridium-bindings` stays unscoped, for now

`@iridium-editor/core` and `@iridium-editor/syntax-worker` are scoped;
`iridium-bindings` is not. Renaming it to `@iridium-editor/bindings` would
strand `iridium-bindings@0.1.1` and break core's `peerDependencies`.

**Decision: keep the name this release.** A rename is a deliberate migration —
publish the new name, deprecate the old, widen core's peer range — and mixing
it into a release whose purpose is "make consumption work" is how you get
consumers on two names at once. Follow-up, not now.

---

## The `canvas.focus()` steal — fixed

Waffles hit this in a probe: `initialize()` ended with an unconditional
`this.canvas.focus()`, so an editor mounting beside a live input **stole the
caret mid-typing and ate the keystrokes that followed**. A host cannot defend
against it: the steal happens inside `create()`, before any handle exists to
call `blur()` on.

**Fixed as an `autoFocus` option defaulting to `true`**, not as a removal.
Removing it would break every consumer written against focus-on-ready — the
web demo included — and "the editor cannot be typed into until you click it"
is the more common complaint. The default preserves today's behaviour; hosts
with a peer input pass `autoFocus: false`.

The other two `canvas.focus()` calls are correct and untouched:
`handleMouseDown` (the user clicked it) and the public `focus()` method.

---

## Consumed surface — preserved

Manifold drives `IridiumEditor.create` with `fontUrl`, `enableSyntaxWorker`,
`onBeforeKeyDown`, plus `setContent` and `destroy`. All five survive. The only
option added is `autoFocus`, which is optional and defaults to today's
behaviour.

⚠️ One **behavioural** change since Manifold's vendored `2e9a224c`, and it is
deliberate: `initialize()` now **throws** when the font fetch fails, where it
used to skip silently and then trap inside wasm with `unreachable`. A consumer
that relied on a broken font URL failing quietly will now see a rejected
promise — which is the point. It is why this is a **minor** bump and not a
patch.

---

## crates.io — recommend **no**, for now

All nine crates carry `description`, `license` and `repository`, so metadata is
not the blocker. Two things are:

1. `iridium-tui` depends on `iridium-editor` by **path with no version**
   (`crates/iridium-tui/Cargo.toml:16`). crates.io rejects that; every
   inter-crate dep needs `version = "..."` alongside `path`.
2. The whole workspace shares `version = "0.1.0"` from `[workspace.package]`,
   so publishing means either lockstepping nine crates forever or splitting
   their versions — a decision with a long tail.

And the case for publishing is weak right now: **nothing outside this repo
consumes the Rust crates.** Manifold consumes the npm packages. Publishing to
crates.io would create nine public API surfaces to keep stable in exchange for
no consumer.

**Recommendation: publish the npm packages now, hold crates.io.** Revisit when
something outside this repo actually wants to `cargo add iridium-editor`. If
Tom wants it anyway, the work is: version the path deps, decide lockstep vs
independent, then publish leaves-first — `iridium-file`, `iridium-tree`,
`iridium-lang`, `iridium-config`, `iridium-syntax`, `iridium-editor`,
`iridium-explorer`, `iridium-tui`, `iridium-bindings`.

---


# STAGING PROGRESS — read this first after a compaction

Head when staging began: `098abe73`. Staged across `442c8a1e` and the commit
this section lands in.

## Done, and how each was verified

1. **`autoFocus` option** — `packages/@iridium/core/src/controller/index.ts`.
   Interface field, default `true` in the resolved options, and the
   `initialize()` call site guarded. The other two `canvas.focus()` calls
   (`handleMouseDown`, public `focus()`) are correct and untouched.

2. **⭐⭐ The wasm module is loaded by package specifier.** See the finding at
   the top of this file. `import("iridium-bindings")` replaces a five-level
   relative walk that resolved only inside this repository. **Verified**: the
   emitted `dist/controller/index.js:234` reads
   `const wasm = await import("iridium-bindings");`, and `new URL` was run over
   both the old and new paths in both layouts to prove which resolves where.

3. **⭐ The syntax worker default names built JavaScript.** `dist/worker.js`,
   not `src/worker.ts`. **Verified** by `new URL` against both an installed
   tree (`node_modules/@iridium-editor/syntax-worker/dist/worker.js`) and this
   repository (`packages/@iridium/syntax-worker/dist/worker.js`). New
   `createSyntaxWorker` option for resolvers where the sibling layout does not
   hold.

4. **Both example bundler configs re-alias bare `iridium-bindings`.** The
   comment saying that alias was "gone with #64" was true of the deleted `ts/`
   fork and is no longer true of the specifier, which now means the wasm build.
   Without this the examples would have broken on change 2.

5. **⭐ Core builds to `dist/`.** `tsconfig.build.json` uses
   `rewriteRelativeImportExtensions` (TS 5.7+; 5.9.3 is the devDep) so sources
   keep writing `./thing.ts` while the emitted JS says `./thing.js`.
   **Verified**: `tsc -p tsconfig.build.json` exits 0, zero test files leak,
   specifiers really are rewritten. Source maps and declaration maps are on, so
   the `src/` in the tarball is wired to the build rather than dead weight —
   measured cost, 2.5 MB → 2.6 MB.

6. **⭐ Syntax-worker builds to `dist/` too.** It had the identical defect and
   was going to be republished with it: `exports` pointing at `.ts`, four `.ts`
   specifiers, and `src/encoding.test.ts` inside the tarball importing
   `bun:test`. It now has the same two tsconfigs, a `build` +
   `prepublishOnly` script, and `@iridium-editor/core` linked as a devDep via
   `file:../core` so the build is reproducible rather than depending on a
   symlink someone made by hand. **Verified**: `tsc` exits 0, 4 `.js` + 4
   `.d.ts`, zero test files, `dist/worker.js:8` reads `from "./encoding.js"`.
   Its `deno.json` also said 0.1.0 against package.json's 0.1.1 — the same
   two-manifests defect core had. Both now say 0.1.1.

7. **Four boundary types were wrong, and the wasm declarations proved it.**
   Linking core's `node_modules/iridium-bindings` to the live crate made `tsc`
   check the boundary for the first time, and it failed three times running:
   - `pixelToPosition`, `getFoldableLines`, `getFoldedLines` return typed
     arrays; core declared `number[]`. `positionToPixel` returns `Float32Array`.
   - `EditorState.foldedLines` is **public** and declared `number[]`, and was
     handing back the wasm heap's `Uint32Array` under that name — so a consumer
     calling `.filter()` got a typed array back and `.push()` threw. Now copied
     with `Array.from`, which keeps the published type honest.
   - `handleKeyEvent`/`runCommand` return `String` from Rust; core declared the
     narrowed union. Narrowing now happens once at the call site through
     `asKeyEventAction`, checked against a single `KEY_EVENT_ACTIONS` tuple that
     the type is derived from — one transcription, not two. An unknown tag is
     reported to the console and treated as `"handled"`, because the core
     returns `"ignored"` only for keys it did not consume and letting the
     browser also act would apply the keypress twice.

8. **Versions bumped** (all three collided with npm before this):
   `@iridium-editor/core` 0.1.1 → **0.2.0**, `@iridium-editor/syntax-worker`
   0.1.0 → **0.1.1**, `iridium-bindings` 0.1.1 → **0.2.0**.

9. **`scripts/build-wasm.sh`** — builds `--target web --release
   --no-default-features --features web` and writes `pkg/PROVENANCE.txt`
   (commit SHA, clean/dirty, rustc, wasm-pack). Refuses a dirty tree unless
   `IRIDIUM_ALLOW_DIRTY=1`. `PROVENANCE.txt` is now named explicitly in
   `files`. This is the answer to #97: `pkg/` stays gitignored, but the
   artefact says where it came from.

10. **A LICENSE in all three packages, and `LICENSE-MIT` at the repo root.**
    Every package declared `"license": "MIT"` and **no licence text existed
    anywhere in the repository** — SPDX metadata with nothing behind it, which
    is exactly what a consumer's legal review stops on. One file, copied (not
    retyped) into three packages; all four hashes verified identical.
    ⚠️ **Tom should eyeball the copyright line** — `Copyright (c) 2026 Iridium
    Contributors`, taken from `[workspace.package] authors`. That attribution
    is his to set, not mine.
    ⚠️ The Rust workspace declares `MIT OR Apache-2.0`, so a `LICENSE-APACHE`
    is still missing. It is deliberately **not** hand-written here: an inexact
    copy of a legal text is worse than none. Fetch it verbatim from apache.org
    before any crates.io publish.

11. **A README and a CHANGELOG in all three packages.** None had either. An npm
    page reading "This package does not have a readme" is a direct disincentive
    to adopt, which is the thing this whole assignment is trying to fix.
    ⚠️ `crates/iridium-bindings/README.md` will also become the **crates.io**
    page if that crate is ever published, and it is written for the npm
    package. Worth splitting at that point.

12. **`npm pack --dry-run` run for all three and the listings read**, not just
    the exit codes:
    - core — 84 files, 3.4 MB; `dist/` present with maps, README + LICENSE +
      CHANGELOG present, **zero** test files.
    - syntax-worker — 24 files, 19.3 kB; `encoding.test.ts` gone.
    - iridium-bindings — 22 files, 2.6 MB; `pkg/PROVENANCE.txt` present.

13. **Tests green**: core `bun test` 104 pass / 0 fail, syntax-worker 17 pass /
    0 fail.

14. **The wasm was rebuilt on the clean committed tree.** `WASM_EXIT=0`,
    `PROVENANCE.txt` now reads commit `4704baae`, tree `clean`, `rustc 1.97.1`,
    `wasm-pack 0.15.0`. The compile itself was a cache hit — nothing under
    `crates/` changed in that commit, so the bytes are the same ones the
    earlier dirty build produced and the new stamp is a true claim about them,
    not a re-derivation.

15. **The ten gates are green.** `./scripts/ci.sh`, output redirected to a file
    and `CI_EXIT` read **from that file** rather than from a completion
    notification — twice this session a notification reported exit 0 over a red
    run. `CI_EXIT=0`, every gate named and reached: `test/workspace` 1282,
    `test/kernel` 1111, `test/syntax` 1243, `test/lang` 81, `check/wasm`,
    `clippy/all`, `clippy/kernel`, `clippy/syntax`, `clippy/wasm`, `fmt`. Zero
    failures anywhere.

## Still to do before Tom can publish

Nothing on this side. The staging is complete; the only remaining step needs
Tom's npm credentials.

- [ ] Reply to Waffles (`dm:896955e1-86dd-4d6a-9c25-f3f978b189a9`) with the
      command sequence below.

## The command sequence for Tom

```bash
cd /Users/tom/Developer/ablative/libs/iridium
npm login                          # Tom runs this; the OTP is his

./scripts/build-wasm.sh            # clean tree; stamps pkg/PROVENANCE.txt

(cd crates/iridium-bindings        && npm publish --access public)
(cd packages/@iridium/core         && npm publish --access public)
(cd packages/@iridium/syntax-worker && npm publish --access public)
```

⚠️ **Order matters, and it is the reverse of the dependency arrows.**
`syntax-worker` peers on core `^0.2.0`; core peers on `iridium-bindings
>=0.2.0`. Publishing in any other order leaves a window in which a package's
declared peer range resolves to nothing on npm.

ℹ️ Core's and syntax-worker's `prepublishOnly` scripts rebuild `dist/` at
publish time, so the tarball cannot go out stale. Syntax-worker's build needs
its `node_modules` in place (`bun install` in that directory) because it
resolves `@iridium-editor/core` through a `file:../core` devDependency; if that
link is missing the build fails loudly rather than publishing without `dist/`.

## crates.io — recommended NO this round

Reasons and the exact work it would take are in the section above. The short
version: `iridium-tui` has a path dep with no version, all nine crates share
one workspace version, and **nothing outside this repo consumes the Rust
crates** — Manifold consumes npm.

## Deliberately not in this release

- **#96**, splitting `iridium-bindings`' manifest into the napi addon and the
  wasm bundle. Folding a package split into the release whose purpose is "make
  consumption work" is how consumers end up straddling two names.
- **Renaming `iridium-bindings` to `@iridium-editor/bindings`.** Same argument;
  a rename is a deliberate migration with a deprecation on the old name.
- **The circular peer dependency.** Core optionally peers on `syntax-worker`,
  which peers on core. Both arms are truthful — the worker's client and
  grammars live in core, the worker file lives in syntax-worker — but the cycle
  is a symptom of the split being in the wrong place. It resolves cleanly under
  npm/bun today; worth revisiting with #96.
