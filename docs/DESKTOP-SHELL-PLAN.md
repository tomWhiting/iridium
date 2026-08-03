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

**MEASURED — 3 Aug 2026, M-series macOS, Metal, release profile, every
figure from the controlling seat's own hand.**

Instrumentation: `IRIDIUM_LATENCY=1 iridium-desktop <file>` logs every
keydown→present sample to stderr and a min/p50/p95/max summary on clean
exit (`apps/iridium-desktop/src/latency.rs` documents the exact policy —
clock starts at winit event receipt before translation, stops after queue
submit + surface present; earliest unpresented keydown wins a shared
frame). Bench: `cargo bench -p iridium-editor --bench compose_frame` —
headless by construction (no surface, offscreen Bgra8Unorm 1512×982),
built-in fallback highlighting, each iteration waits for GPU completion.

Live keydown→present (injected keystrokes, real window, release binary):

| Document | Samples | min | p50 | p95 | max |
|---|---|---|---|---|---|
| Small file (near-empty viewport) | 6 | 1.16ms | ~2.7ms | — | 7.75ms |
| 10k-line file (full dense viewport) | 43 | 40.36ms | 41.17ms | 42.21ms | 44.54ms |

Headless compose bench, 10k-line file (criterion means [low, high]):
steady_state **35.2ms** [33.6, 36.9]; after_mid_file_edit **34.4ms**
[33.0, 35.9]; after_scroll_change **30.9ms** [30.0, 31.7].

**The verdict, honestly:** the shell meets the sub-8ms budget only while
the viewport is sparse. A full screen of dense code costs ~41ms
keydown→present — ~5× over budget, ~24fps — and the bench places
~35ms of that inside `FrameCompositor::compose` on the CPU. Diagnosis
(reproduced, not assumed): the cost is per-*viewport*, not per-document —
a 100-line file with a full viewport benches the same ~31ms; skipping the
GPU wait changes nothing; disabling the keyword highlighter changes
nothing. The compose path rebuilds and reshapes the entire visible
cosmic-text buffer every frame (`create_buffer` → `set_rich_text` →
`shape_until_scroll`, `Family::Monospace` resolved against a native
fontdb holding every system font). The optimization target is therefore
**retained shaping** — cache the shaped buffer across frames and reshape
only what changed (edit, scroll, resize, font) — a kernel compositor
change, now first on the follow-up ledger. The instrumentation and bench
stay in the tree so the fix is measured against these same numbers. Note
the webview comparison that motivated the track is unaffected as an
*architecture* argument (its compositor hop adds latency on top of
whatever the frame costs), but the honest finding is that today the frame
itself is the bottleneck, and it is ours to fix.

**RETAINED SHAPING LANDED — 4 Aug 2026, commits 4fc0950 (stage 1 + 2a)
and 5846f62 (R7), design in docs/design/RETAINED-SHAPING-MAP.md. Bench
from the controlling seat's hand, same machine WITH a live desktop
session running (noisier than the 3 Aug quiet-box numbers — compare
shapes, not absolutes across days):**

| Case | 3 Aug (rebuild-every-frame) | 4 Aug (retained) |
|---|---|---|
| steady_state | 35.2ms | **3.68ms** [3.58, 3.78] (hit) |
| first_frame (new; cold ≈ old steady_state) | — | 42.5ms [40.6, 44.7] |
| after_mid_file_edit | 34.4ms | **7.54ms** [7.27, 7.82] |
| after_small_scroll (new; sub-line, hit) | — | 6.28ms [5.80, 6.80] |
| after_scroll_change (full miss; stage 2b) | 30.9ms | 8.0ms [5.4, 12.6] (wide — noisy box) |

The edit frame — the keystroke path, the track's headline — is inside
the 8ms budget on a 10k-line dense viewport even on a busy machine. Two
honest caveats, both verified by the implementing agent and recorded in
its artifacts (scratchpad retained-bench-agent.txt,
baseline-bench-recheck.txt): (a) the 3 Aug absolutes do not reproduce —
the unchanged pre-change tree benched steady_state ~19.8ms on 4 Aug, so
cross-day absolute comparisons carry environment drift; same-day
quiet-box figures were 19.8ms full rebuild → 1.56ms hit / 1.86ms edit.
(b) A real pre-existing defect was found and fixed in the same change:
`FrameCompositor::new` never seeded the glyph viewport uniform, so
HEADLESS consumers (the bench, the proof tests) were culling all glyph
draw in `prepare` — the 3 Aug bench numbers never included glyph
drawing; faces were unaffected (their startup resize seeded it). The
post-change live keydown→present re-measurement (3 Aug baseline: p50
41.17ms) is DEFERRED to the next bundle refresh — keystroke injection
races the user's live session for focus and lost honestly, twice.
Stage 2b (scroll window rotation) remains open, gated on these numbers
per R4.

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
