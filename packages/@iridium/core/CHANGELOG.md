# Changelog

## 0.2.0

**The first version of this package that can actually be installed from npm and
used.** Every earlier release shipped raw TypeScript whose imports could not
resolve outside this repository. If you vendored the source to work around
that, this release is the reason to stop.

### Fixed — the package is consumable

- **The wasm module is loaded by package specifier.** It used to be resolved by
  walking `../../../../../crates/iridium-bindings/pkg/` up from
  `import.meta.url`. From an installed
  `node_modules/@iridium-editor/core/dist/controller/`, five levels up lands
  above the consumer's `node_modules`, looking for a `crates/` directory that
  exists only in this repository — so no published version could ever load its
  own core. It now does `import("iridium-bindings")`, the peer dependency it
  already declared.
- **The syntax worker points at built JavaScript.** The default worker URL named
  `syntax-worker/src/worker.ts` — TypeScript source, which no `Worker`
  constructor can load. It now names `syntax-worker/dist/worker.js`, resolved as
  a sibling package, so `enableSyntaxWorker: true` works outside a bundler
  configured for this repository.
- **The package is compiled.** `exports` now points at `dist/`, built by
  `tsc -p tsconfig.build.json` with declarations and source maps. Previously it
  pointed at `.ts` source containing 38 `.ts`-suffixed import specifiers, which
  fail under plain `tsc` and under any resolver not configured to tolerate them.
- **Test files are no longer published.** `src/**/*.test.ts` shipped inside the
  tarball and imported `bun:test`, a module consumers do not have.
- **`getState().foldedLines` is a real array.** It is declared `number[]` and was
  handing back the wasm heap's `Uint32Array` view under that name, so `.filter()`
  returned a typed array and `.push()` threw.
- **The wasm boundary is typed against the generated declarations.** Four methods
  were hand-declared with types the wasm build does not produce — three typed
  arrays described as `number[]`, and the action tag narrowed to a union where
  the boundary says `string`. Narrowing now happens once, at the call site,
  against a single list of tags; a tag the core starts emitting that this build
  does not know is reported to the console rather than silently mistaken for one
  it does.

### Added

- **`autoFocus`** (default `true`). `initialize()` focused the canvas
  unconditionally, so an editor mounting beside a live input stole the caret
  mid-typing and ate the keystrokes that followed. A host could not defend
  against it — the steal happens inside `create()`, before any handle exists to
  call `blur()` on. Pass `autoFocus: false` when the editor is not the only
  thing on the page. The default is unchanged behaviour.
- **`createSyntaxWorker`** — supply the `Worker` yourself when the sibling-package
  default does not fit your resolver (pnpm's store layout, bundlers that rewrite
  worker URLs, a worker you host).
- **`viewToggleTheme`** on `hostCommands`, matching the kernel's new
  `view.toggleTheme` command.
- **A README and a LICENSE.** Neither existed; the package declared MIT with no
  licence text in the tarball.

### Changed — behaviour

- **A failed font fetch now throws.** `initialize()` used to skip the font
  silently and then trap inside wasm with `unreachable`. A consumer relying on a
  broken font URL failing quietly will now see a rejected promise. `.woff2` is
  still not decoded — the build reads raw sfnt only, and now says so.

### Requires

`iridium-bindings >= 0.2.0`. `@iridium-editor/syntax-worker ^0.1.1` is an
optional peer, needed only when `enableSyntaxWorker` is on.

## 0.1.1

Earlier releases were published without a changelog. See the repository history.
