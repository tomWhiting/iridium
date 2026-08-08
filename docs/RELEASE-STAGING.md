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

## ⭐ The finding that decides the whole assignment

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

Head when staging began: `098abe73`. **Nothing has been committed yet**; all of
the below is in the working tree.

## Done

1. **`autoFocus` option** — `packages/@iridium/core/src/controller/index.ts`.
   Interface field, default `true` in the resolved options, and the
   `initialize()` call site now guarded. The other two `canvas.focus()` calls
   (`handleMouseDown`, public `focus()`) are correct and untouched.

2. **⭐ Core now builds to `dist/` and is actually consumable.** New
   `packages/@iridium/core/tsconfig.build.json` uses
   `rewriteRelativeImportExtensions` (TS 5.7+; 5.9.3 is the devDep) so sources
   keep writing `./thing.ts` while the emitted JS says `./thing.js`.
   **Verified**: `tsc -p tsconfig.build.json` exits 0, emits 16 `.js` + 16
   `.d.ts`, **zero** test files leak, and specifiers really are rewritten
   (`dist/element/index.js:25` reads `from "./palette.js"`). 2.5 MB.

3. **Versions bumped** (all three collided with npm before this):
   - `@iridium-editor/core` 0.1.1 -> **0.2.0**, `exports`/`main`/`types` now
     point at `dist/`, `files` is `["dist/", "src/", "!src/**/*.test.ts"]`,
     `build` + `prepublishOnly` scripts added, peer on `iridium-bindings >=0.2.0`
   - `@iridium-editor/syntax-worker` 0.1.0 -> **0.1.1**, peer on core `^0.2.0`
   - `iridium-bindings` 0.1.1 -> **0.2.0**
   - `packages/@iridium/core/deno.json` said **0.1.0** while package.json said
     0.1.1 — two manifests disagreeing, nothing checking. Both now say 0.2.0.

4. **`scripts/build-wasm.sh`** — builds `--target web --release
   --no-default-features --features web` and writes `pkg/PROVENANCE.txt`
   (commit SHA, clean/dirty, rustc, wasm-pack). Refuses a dirty tree unless
   `IRIDIUM_ALLOW_DIRTY=1`. That file ships in the tarball, which is the answer
   to #97: `pkg/` stays gitignored, but the artefact now says where it came
   from. **Run and verified**: `WASM_EXIT=0`, built from `098abe73`.
   ⚠️ It was built with `IRIDIUM_ALLOW_DIRTY=1` because the version bumps were
   already in the tree. **Rebuild it after committing** so PROVENANCE says
   `clean`.

## Still to do before Tom can publish

- [ ] `npm pack --dry-run` for all three, and **open the file listings** — the
      point is to confirm `dist/` is in core's tarball and tests are not.
- [ ] CHANGELOG.md for each of the three packages.
- [ ] Commit, then **re-run `./scripts/build-wasm.sh` with a clean tree**.
- [ ] `./scripts/ci.sh` — ten gates, redirected to a file, read `CI_EXIT`.
- [ ] Reply to Waffles (`dm:896955e1-86dd-4d6a-9c25-f3f978b189a9`) with the
      command sequence.

## The command sequence for Tom (draft — do not send until the above is done)

```bash
cd /Users/tom/Developer/ablative/libs/iridium
npm login                          # Tom runs this; OTP is his
./scripts/build-wasm.sh            # clean tree; stamps PROVENANCE.txt
(cd packages/@iridium/core && npm publish --access public)
(cd crates/iridium-bindings && npm publish --access public)
(cd packages/@iridium/syntax-worker && npm publish --access public)
```

⚠️ **Order matters.** `syntax-worker` peers on core `^0.2.0` and core peers on
`iridium-bindings >=0.2.0`; publishing syntax-worker first leaves a window
where its peer range resolves to nothing.

## crates.io — recommended NO this round

Reasons and the exact work it would take are in the section above. The short
version: `iridium-tui` has a path dep with no version, all nine crates share
one workspace version, and **nothing outside this repo consumes the Rust
crates** — Manifold consumes npm.
