# `iridium-bindings` web build spike

Verified 2026-07-17 from `spike/web-build` at `8a2ecb93654263dae08832ee1998207aa0283fae` on macOS/aarch64. No toolchain component was installed or updated.

## Verdict

**The `web` feature builds for `wasm32-unknown-unknown` and wasm-pack produces a browser-target ES module.** The binding is low-level and embeddable, but it is not a self-contained browser editor component: the host must own the canvas, font, DOM events, frame scheduling, sizing, clipboard, IME strategy, and (for tree-sitter quality highlighting) parsing.

The checked-in smoke harness loads the generated ES module in a page, checks WebGPU, hands over a canvas, loads the repository's JetBrains Mono font, creates a `WebSurface`, sets content, and forces a frame. It deliberately throws on every failure rather than converting failures into apparent lack of support.

## Build gate (PASS)

Required command, run exactly with the shared target directory:

```sh
CARGO_TARGET_DIR=/Users/tom/Developer/ablative/iridium/target \
  cargo build -p iridium-bindings --no-default-features --features web \
  --target wasm32-unknown-unknown
```

Genuine exit code: **0**. Build completed in the dev profile. There was no error set. Warnings were:

- `iridium-editor/src/syntax_stubs.rs`: two `in_line_comment` assignments are never read (lines 118 and 159).
- `iridium-bindings/src/wasm.rs`: `WebEditor.pixel_ratio` is never read.
- `iridium-bindings/src/web_span_index.rs`: `WebSpanIndex::len` is never used.

Tool versions observed: `rustc 1.92.0`, `cargo 1.92.0`, `wasm-pack 0.13.1`.

## wasm-pack browser package (PASS)

`Cargo.toml` declares `cdylib` and `[package.metadata.wasm-pack.profile.release] wasm-opt = false`. The corresponding browser/release packaging command was:

```sh
CARGO_TARGET_DIR=/Users/tom/Developer/ablative/iridium/target \
  wasm-pack build --target web --release \
  --out-dir /tmp/iridium-web-pkg-release crates/iridium-bindings \
  --no-default-features --features web
```

Genuine exit code: **0**. Generated artifact sizes (raw bytes, not compressed):

| Artifact | Bytes |
|---|---:|
| `iridium_bindings_bg.wasm` | 3,289,799 |
| `iridium_bindings.js` | 107,072 |

The package also contains generated TypeScript declarations. wasm-pack warned that the crate directory has no license file despite Cargo license metadata and that wasm-pack 0.15.0 exists; neither affects the build, and no update was attempted.

One invocation lesson: wasm-pack options must precede the crate path. Putting `--out-dir` after `crates/iridium-bindings` makes wasm-pack 0.13.1 forward it to Cargo, where Cargo 1.92 interprets it as unstable `--artifact-dir` and exits 101. This was an invocation error, not a source or feature failure.

## Browser smoke harness

Run:

```sh
docs/web-build-harness/run.sh
# open http://127.0.0.1:8000/docs/web-build-harness/
```

A Chrome headless probe loaded the page, JS glue, wasm binary, and font over HTTP (all 200). Chrome's `--dump-dom` remained at `starting` because the asynchronous WebGPU adapter callback did not settle under that headless/virtual-time probe, so this run is **not claimed as a completed render pass**. The harness is the honest interactive render check: it reports `PASS` only after `createWebEditor`, `loadFont`, and `forceRender` all return; it displays and rethrows the original error otherwise. Chrome and Safari were present locally. No browser automation dependency was installed.

## Feature gating and exact web surface

`lib.rs` compiles `wasm` and `web_span_index`, and re-exports `wasm::*`, only under `all(feature = "web", target_arch = "wasm32")`. NAPI modules, NAPI factories, GPU-info APIs, and NAPI types are gated by `feature = "napi"` and are absent from this build. The generated browser package has three top-level callable exports:

- `init()` (also registered as wasm-bindgen start): installs the panic hook.
- `isWebGPUSupported(): Promise<boolean>`: asks wgpu for an adapter.
- `createWebEditor(canvas, pixelRatio): Promise<WebEditor>`: consumes an `HTMLCanvasElement`, creates `WebSurface::from_canvas`, GPU text and quad renderers, and the editor. `WebEditor` has no public constructor.

It also exports `JsEditInfo`, whose read-only fields are `startByte`, `oldEndByte`, `newEndByte`, `startRow`, `startColumn`, `oldEndRow`, `oldEndColumn`, `newEndRow`, and `newEndColumn`.

The generated `WebEditor` methods, grouped without omitting any public operation, are:

- **Canvas/render/layout:** `loadFont`, `render`, `forceRender`, `resize`, `getLineHeight`, `getCharWidth`, `getTextOffsetX`, `getTextOffsetY`, `pixelToPosition`, `positionToPixel`.
- **Document/edit/history:** `setContent`, `getContent`, `insert`, `backspace`, `delete_forward` (not camel-cased in the generated API), `deleteWordBackward`, `deleteWordForward`, `deleteToLineStart`, `deleteToLineEnd`, `takeLastEdit`, `undo`, `redo`, `canUndo`, `canRedo`, `setReadOnly`, `isReadOnly`.
- **Raw key and clipboard handoff:** `handleKeyEvent`, `getPendingClipboardText`, `copyText`, `cutText`.
- **Cursor and selection:** `getCursorLine`, `getCursorColumn`, `moveCursorLeft`, `moveCursorRight`, `moveCursorUp`, `moveCursorDown`, `moveCursorLineStart`, `moveCursorLineEnd`, `moveCursorDocStart`, `moveCursorDocEnd`, `moveCursorWordLeft`, `moveCursorWordRight`, `hasSelection`, `getSelectedText`, `getSelectionStartLine`, `getSelectionStartColumn`, `getSelectionEndLine`, `getSelectionEndColumn`, `clearSelection`, `extendSelectionLeft`, `extendSelectionRight`, `extendSelectionUp`, `extendSelectionDown`, `extendSelectionWordLeft`, `extendSelectionWordRight`, `extendSelectionLineStart`, `extendSelectionLineEnd`, `extendSelectionDocStart`, `extendSelectionDocEnd`, `selectAll`, `setCursorFromClick`, `startSelectionAt`, `extendSelectionToPosition`.
- **Scroll:** `getScrollY`, `setScrollY`, `scrollBy`, `getMaxScrollY`, `ensureCursorVisible`.
- **Highlight/theme/gutter/diff decoration:** `setDarkTheme`, `setSyntaxEnabled`, `isSyntaxEnabled`, `setTreeSitterHighlights`, `clearTreeSitterHighlights`, `isTreeSitterActive`, `setSyntaxTheme`, `setGutterEnabled`, `isGutterEnabled`, `setLineBackgrounds`, `clearLineBackgrounds`, `setGutterChanges`, `clearGutterChanges`, `setCustomGutterText`, `setBlameData`, `clearBlameData`.
- **Lines/folding:** `getLineCount`, `updateFolds`, `getFoldableLines`, `isFoldable`, `isFolded`, `isLineVisible`, `toggleFold`, `foldAt`, `unfoldAt`, `foldAll`, `unfoldAll`, `getFoldedLines`, `getHiddenLineCount`, `getVisibleLineCount`, `getFoldEndLine`.
- **Lifetime generated by wasm-bindgen:** `free` and `[Symbol.dispose]`.

### Input contract

`handleKeyEvent(key, ctrl, shift, alt, meta, altGraph)` accepts the DOM `KeyboardEvent.key` and modifier booleans. It returns `handled`, `handled:edit`, `copy`, `cut`, `search:open`, `search:next`, `search:prev`, `search:close`, or `ignored`. The host must prevent default only for consumed actions, pull edit metadata with `takeLastEdit`, and pull copy/cut text with `getPendingClipboardText`. Mouse/pointer selection is expressed as host-computed calls to `pixelToPosition` plus click/selection methods; wheel input is expressed as `scrollBy`.

### Syntax availability

`napi = [napi, napi-derive, syntax, tokio]`; `syntax = [iridium-syntax, iridium-editor/syntax]`; `web` does **not** enable `syntax`. Therefore this exact no-default-features web build does not contain the native `iridium-syntax` tree-sitter pipeline. It uses `syntax_stubs::Language` and `SimpleHighlighter`. A JavaScript parser may supply byte spans through `setTreeSitterHighlights` and colors through `setSyntaxTheme`; `takeLastEdit` supplies byte-accurate incremental edit data. `setSyntaxEnabled(true)` does not make the omitted native syntax feature available.

## What a frame class-(b) component adapter must call

A canvas-owning component adapter would:

1. Import and await default wasm initialization; await `isWebGPUSupported` and surface a real failure.
2. Create/focus a canvas, set its **bitmap** width/height in physical pixels, then await `createWebEditor(canvas, devicePixelRatio)`.
3. Fetch a font, verify the response, and call `loadFont`; WASM has no system fonts. Set content/theme/gutter and optional host-produced syntax spans.
4. Attach keyboard, copy, cut, paste, pointer/drag, wheel, focus, and composition policy at the DOM layer. Forward supported keys to `handleKeyEvent`; use `insert` for host text/paste; route clipboard tags/text explicitly.
5. Use `ResizeObserver` (and DPR awareness) to update canvas bitmap dimensions and call `resize(width, height)`.
6. Own `requestAnimationFrame`: call `render()` when scheduled (or `forceRender` for a guaranteed frame), and schedule another frame after every mutation, cursor/selection change, scroll, resize, theme/highlight update, or blink policy.
7. Read state/content and `takeLastEdit` to notify the framework and update an external parser. On unmount, detach listeners/cancel RAF and call `free()`/`Symbol.dispose`.

## Browser embedding gaps (findings, not fixes)

- **Surface/canvas creation:** GPU surface creation is implemented inside `createWebEditor`; what is missing is a DOM-level owner. The binding does not create the canvas, validate secure-context/WebGPU browser policy, synchronize CSS size with bitmap size, or recover from device/surface loss.
- **Event-loop ownership:** there is no browser controller or RAF loop in this crate. `needs_redraw` only lets `render()` skip work; nothing schedules that call, cursor blinking, or event listeners.
- **Resize/DPR:** `resize(width, height)` exists, but no `ResizeObserver`, CSS-to-device-pixel conversion, DPR change handling, or automatic canvas attribute update exists. The stored `pixel_ratio` is currently warned as unread.
- **IME:** no `compositionstart/update/end`, `beforeinput`, marked-text range, candidate-window positioning, or composition replacement API exists. Treating composed text as ordinary key events is insufficient for CJK and other IMEs; a host can only commit final text via `insert` today.
- **Clipboard:** core copy/cut behavior and text handoff exist, but browser permission/API/event ownership does not. There is no paste/`ClipboardEvent` method; the host must read clipboard text and call `insert`. Async failures and permission denial must be surfaced by the host.
- **Pointer/touch/scroll/focus:** coordinate conversion and selection primitives exist, but listeners, capture, autoscroll while dragging, touch behavior, wheel normalization, focus management, and accessibility semantics do not.
- **Syntax:** native tree-sitter syntax is absent from `web`; serious highlighting requires a separate JS parser, capture mapping/theme, edit synchronization, and full-reparse fallback.
- **Packaging/size:** release wasm is about 3.14 MiB raw with wasm-opt intentionally disabled. No compression, cache strategy, CSP guidance, MIME/server configuration, or npm publication wiring is established by this spike.

No editor crate source was patched.
