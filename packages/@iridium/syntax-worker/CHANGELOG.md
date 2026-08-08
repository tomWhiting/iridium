# Changelog

## 0.1.1

**The first version of this package that can be consumed from npm.** 0.1.0
shipped raw TypeScript that no consumer's `Worker` constructor could load.

### Fixed

- **The package is compiled.** `exports` now points at `dist/`, built by
  `tsc -p tsconfig.build.json` with declarations and source maps. 0.1.0 pointed
  `.`, `./worker` and `./protocol` at `.ts` source containing `.ts`-suffixed
  import specifiers — so `new Worker(new URL(".../worker", ...))` fetched
  TypeScript and the browser refused it.
- **`src/encoding.test.ts` is no longer published.** It shipped inside the
  tarball and imports `bun:test`, a module consumers do not have.
- **`deno.json` and `package.json` agree on the version.** They said 0.1.0 and
  0.1.1 respectively, with nothing checking that two manifests in one package
  describe the same thing.

### Added

- **A README and a LICENSE.** Neither existed; the package declared MIT with no
  licence text in the tarball.

### Requires

`@iridium-editor/core ^0.2.0`, whose `enableSyntaxWorker` option now resolves
this package's `dist/worker.js` rather than its source.

## 0.1.0

Earlier releases were published without a changelog. See the repository history.
