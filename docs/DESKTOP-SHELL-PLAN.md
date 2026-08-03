# The desktop shell — iridium as its own .app

**Status: GREEN-LIT 3 Aug 2026 — all four decisions ruled by Tom (his DM,
~06:27Z). Build order is as planned: compositor extraction first.**

Rulings:
- **D-A — winit: ACCEPTED.** "If that really is the best solution then yeah
  we'll go with that."
- **D-B — mouse: IN v1.** Mouse matters for version one. The desktop face is
  the first with honest pixel metrics, so the kernel's hit-testing gets used
  as designed.
- **D-C — overlays: THE FULL SET in v1.** "I don't want version one to be a
  short sell at all — I want it to include the full everything." Prompts,
  search, palette AND the undo-tree panel all ship in v1.
- **D-D — identity: name "iridium", icon "77"** (the atomic number).

Tom's bar, in his words: the snappiness is what makes him love it. That rules
out any webview (compositor adds a frame or two of input latency; rAF may cap
at 60Hz) and defines the target: a native window where the kernel's 120fps /
sub-8ms budgets are honest — the same directness the terminal face has, with
pixels instead of cells.

## Verified ground (read from the tree, 3 Aug 2026)

- The kernel's `render` feature (wgpu 28, glyphon, cosmic-text) carries a full
  pipeline: `render/{pipeline,text,gutter,cursor,quad,viewport,minimap}`.
  It is face-neutral and proven — the browser demo renders through it today.
- **The only surface in the tree is `render/web.rs::WebSurface`** —
  `SurfaceTarget::Canvas`, `Backends::BROWSER_WEBGPU`. Its shape (surface +
  config + device/queue arcs + `resize` + `render_frame(closure)`) is exactly
  what a native twin needs; ~200 lines of it are canvas-specific.
- **The per-frame composition lives in the wrong crate for a second GPU
  face.** `iridium-bindings/src/wasm.rs::render_frame` (plus its cached
  visual-line map, scroll math, and buffer reuse) is where text, gutter,
  cursor and minimap are actually assembled into a frame. It is web-face
  code. A native shell that re-wrote it would fork frame composition — the
  exact failure mode one-kernel-N-faces exists to prevent.
- **winit appears nowhere in the estate** (no manifest, no lockfile). This
  shell introduces it. wgpu 28 accepts a winit window as a surface target
  directly (via raw-window-handle); no bridging code needed.
- The kernel's `Modifiers` already carries `meta`; `KeyLabelStyle::MacGlyphs`
  already renders ⌘ labels. Real Cmd shortcuts are a translation table away.

## The shape

### Step 1 — extract the compositor (kernel, the load-bearing step)

New `iridium-editor/src/render/compositor.rs` (feature `render`): a
`FrameCompositor` owning the renderers and caches that `wasm.rs` holds today,
with one entry point — compose one frame from `&Editor` + pixel metrics onto a
`wgpu::TextureView`. `wasm.rs` becomes a thin caller (canvas + JS events +
compositor); the desktop shell becomes another. This is a refactor of the
largest render path in the tree and is **the majority of the risk**: it must
land with the web demo pixel-identical (the existing demo is the oracle).
Nothing else starts until this is green.

### Step 2 — the shell crate (`apps/iridium-desktop`)

- **Window + loop**: winit `EventLoop` + `Window`, event-driven exactly like
  the terminal face — render on input/resize/redraw-request, never a busy
  loop; an idle editor costs nothing. `NativeSurface` mirrors `WebSurface`:
  `Backends::METAL`(/PRIMARY), `PresentMode::Fifo` default with the
  latency-lean modes (`Mailbox`/`Immediate`) selectable for measurement.
  Scale-factor handled from day one (Retina is 2x; logical vs physical pixel
  confusion is the classic first bug).
- **Input**: winit → kernel `KeyEvent`/`Modifiers` translation table. ⌘ maps
  to `meta`, and the default keymap gains the mac layer so ⌘C/⌘V/⌘Z/⌘K work
  natively — the thing no terminal emulator could give us without protocol
  negotiation. Mouse deliberately deferred (same reasoning as the TUI:
  a wrong caret is worse than none) unless Tom rules otherwise — the pixel
  metrics DO exist here, so it is a scope choice, not a capability gap.
- **Host layer**: port of `apps/iridium/src/app` minus the cell painting —
  file open/save (the same atomic-save code path), prompts, message line,
  dirty tracking. The kernel-first command dispatch pattern from the palette
  work carries over unchanged.
- **Overlays are the honest stretch.** The palette, search, prompts and the
  undo-tree panel are painted UI. In the TUI they are cell rows; on the GPU
  they are glyphon text runs + quads. The kernel data (`search_text`,
  `history_snapshot`, key hints) is face-neutral and ready; the painting is
  new. V1 scope: prompts + search + palette (the daily drivers), undo-tree
  panel following. This is where "a week" becomes "honestly, nearer two."
- **Clipboard**: system clipboard via `arboard` (already proven in the
  estate — meridian ships it). First face with real cross-app copy/paste.
- **Bundle**: hand-written `Info.plist` + `.app` layout + ad-hoc codesign in
  a small script — no cargo-bundle dependency needed for a single target.

### Step 3 — prove the claim that justifies the whole track

The reason this exists is latency, so it gets measured, not asserted:
keydown-timestamp → present-timestamp instrumentation behind a debug flag,
plus a frame-time criterion bench on a 10k-line file. The numbers go in this
doc when they exist. If the shell cannot beat the webview meaningfully, that
is a finding worth having before more is built on it.

## What v1 deliberately does not contain

Mouse (scope choice, see above), soft wrap (blocked on the recorded design),
multiple windows/tabs (one file per window, like the TUI), preferences UI
(`--theme` flag parity only), LSP (kernel gap, not shell gap).

## Decisions for Tom

- **D-A**: winit dependency — accept? (Nothing in the estate uses it; it is
  the standard Rust windowing layer and the alternative is hand-rolled
  objc2 AppKit, weeks not days, for no latency gain.)
- **D-B**: mouse in v1 — the desktop face is the first where hit-testing is
  honest. In or out?
- **D-C**: v1 overlay set — prompts+search+palette then history panel, or
  hold v1 until all four?
- **D-D**: bundle identity (app name, icon) — pure taste, needed only at
  packaging.
