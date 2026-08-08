# Changelog

## 0.2.0

### Added

- **`pkg/PROVENANCE.txt`.** The `pkg/` directory is gitignored, so nothing
  connected the wasm bytes on npm to a revision of the source. Every build now
  goes through `scripts/build-wasm.sh`, which stamps the commit, whether that
  tree was clean, and the `rustc` and `wasm-pack` versions, and which refuses to
  build a dirty tree unless told to. (Issue #97.)
- **`view.toggleTheme`** in the host command ids, alongside the light/dark theme
  switch that landed in the kernel.
- **A README and a LICENSE.** Neither existed; the package declared MIT with no
  licence text in the tarball.

### Fixed

- The bytes published as 0.1.1 predated a run of kernel work, including the font
  loading fix that made the web target usable at all. This release is built from
  the current tree, and `PROVENANCE.txt` now says which one.

### Known

`package.json` still describes two artefacts: a `napi` block and an
`@napi-rs/cli` build script for a Node native addon, alongside the `main`,
`types`, `exports` and `files` that publish the wasm bundle. Only the wasm half
ships. Splitting them is issue #96, deliberately left out of this release —
folding a package rename into the release that makes consumption work is how
consumers end up straddling two names.

## 0.1.1

Earlier releases were published without a changelog. See the repository history.
