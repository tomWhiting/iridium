# Iridium — The Triple Face

**Written 2026-07-30.** Design note on making Iridium a genuine
"sing, dance, and act" editor: one engine with a terminal face, a desktop
face, and a web face. Extends `PLAN.md` §3 (target architecture) and
Phases 2/4/5 with one structural correction and one new seam.

This note is grounded in a file-level audit of the current tree, not in
aspiration. Every claim below is checked against code.

---

## 1. The good news: the kernel is already almost GPU-free

The fear that "the editor is welded to wgpu" is unfounded. Measured across
`crates/iridium-editor/src/`:

| Module | Files touching `wgpu`/`glyphon`/`cosmic_text` |
|---|---|
| `document/` | 0 of 4 |
| `editor/` | 0 of 4 |
| `history/` | 0 of 3 |
| `input/` | 0 of 16 |
| `search/` | 0 of 3 |
| `span_index/` | 0 of 2 |
| `theme/` | 0 of 4 |
| `view/` | 0 of 2 |
| `render/` | **5 of 14** |

`lib.rs` names wgpu exactly once — in a doc comment. **wgpu being a
non-optional dependency of `iridium-editor` is a `Cargo.toml` artifact,
not a real coupling.**

Inside `render/`, only four files carry GPU types:

| File | Lines | Nature |
|---|---|---|
| `pipeline.rs` | 902 | GPU |
| `text.rs` | 678 | GPU |
| `web.rs` | 346 | GPU |
| `quad.rs` | 327 | GPU |
| `highlight.rs` | 1013 | **pure layout** |
| `gutter.rs` | 920 | **pure layout** |
| `viewport.rs` | 695 | **pure layout** |
| `simple_highlight.rs` | 494 | **pure layout** |
| `cursor.rs` | 454 | **pure layout** |

**≈2,250 lines of actual GPU code against ≈3,575 lines of pure layout
logic that every face needs.**

The entire kernel→render coupling is three references to *pure* types:

- `render::Viewport` — `editor/core.rs:14`, `input/mouse.rs:16`
- `render::{DEFAULT_MINIMAP_WIDTH, MinimapPosition}` — `editor/config.rs:5`
- `render::{MinimapRenderer, MinimapDimensions}` — `input/mouse.rs:691+`

None of those touch the GPU. The split is a move-files-and-fix-imports
job, not a rewrite.

---

## 2. Two more things that already work in our favour

**Input is already platform-neutral.** The only non-`crate`/`std` imports
across all 16 files of `input/` are `iridium_syntax::Language`, `serde`,
and `web_time::Instant` (a shim that compiles native *and* wasm). The
contract is `KeyCode::Char(char)` plus
`Modifiers { shift, ctrl, alt, meta, alt_graph }` — no winit type, no DOM
type, no crossterm type. A terminal face needs a key translator of roughly
200 lines, not an input subsystem.

**Viewport math is metrics-parameterised, not pixel-hardcoded.**
`Viewport::new(width, height, line_height)` (`render/viewport.rs:87`)
derives everything — `document_line_at_y`, `screen_y_for_line`,
`visible_line_range`, and all the fold-aware traversal — from
`line_height`. Construct it with `line_height = 1.0` and width/height in
cells and the whole thing becomes correct cell math, folds included, with
no new code.

That is the single biggest reason the terminal face is tractable rather
than a second implementation.

---

## 3. The one correction to PLAN.md §3

`PLAN.md` §3 currently assigns "the compositor + glyphon/quad/**gutter/
minimap primitives**" to `iridium-render`, the GPU crate.

**That placement would force the terminal face to either duplicate gutter,
highlight, and cursor layout, or depend on a GPU crate to get at them.**
Both outcomes are how multi-face editors drift into three subtly different
editors wearing a trench coat.

Corrected placement — the test is *"does it name a GPU type?"*, not
*"does it sound like rendering?"*:

```
iridium-core/      document, history, input, search, span_index, theme,
                   config, fold state
                   + viewport.rs          (moved out of render/)
                   + highlight.rs         (moved out of render/)
                   + gutter.rs            (moved out of render/)
                   + cursor.rs            (moved out of render/)
                   + simple_highlight.rs  (moved out of render/)
                   + the paint model (§4)
                   NO wgpu. NO web. Compiles anywhere.

iridium-render/    pipeline.rs, text.rs, quad.rs, web.rs.
                   Rasterises a paint list onto a wgpu surface.
                   Shared verbatim by desktop and web.

iridium-tui/       Rasterises the SAME paint list into a cell grid.
                   crostterm/termwiz, kitty keyboard protocol,
                   synchronized output, OSC 52 clipboard,
                   unicode-width cell math, damage tracking.

iridium-desktop/   winit shell over iridium-render.
iridium-web/       wasm-bindgen translator over iridium-render.
```

The CI gate `cargo check -p iridium-core --no-default-features` already
exists in spirit as the kernel gate; after the split it becomes
load-bearing, because wgpu will genuinely not be in the dependency graph.

---

## 4. The new seam: a backend-neutral paint model

This is the piece that makes "triple threat" a property of the
architecture rather than a promise maintained by discipline.

**The kernel does not draw. It produces a `Frame`: a resolved, immutable
description of what should appear, in logical text coordinates.** Each
face rasterises that description in its own idiom.

```
EditorState + Viewport + Theme + spans
        │
        ▼
   layout (pure, in core)
        │
        ▼
      Frame  ─── styled runs, cursor rects, gutter cells,
        │        selection ranges, fold markers
        │        all in (line, column-range, style)
        │
   ┌────┴───────────────────┐
   ▼                        ▼
iridium-render          iridium-tui
(quads + glyphs)        (cells + SGR)
   │                        │
   ▼                        ▼
desktop / web           terminal
```

**Coordinates are logical, not pixel.** A run is
`(line, col_start..col_end, style)`. The GPU faces resolve columns to
pixels through their font metrics; the terminal resolves them to cells
through `unicode-width`. Pixel coordinates in the shared model would make
the terminal impossible; cell coordinates would cost the GPU faces their
subpixel quality. Logical text coordinates cost neither — and code is
overwhelmingly monospace anyway, which is why this is the right lowest
common denominator for *this* editor and would be the wrong one for a word
processor.

**What this buys, concretely:**

- One layout implementation, three rasterisers. A gutter change lands in
  all three faces at once, or it lands in none.
- The `Frame` is a pure value, so layout becomes snapshot-testable without
  a GPU, a browser, or a terminal. Today none of the layout logic can be
  tested headlessly.
- The terminal gets damage tracking for free: diff consecutive `Frame`s
  and repaint only changed cells.
- Frame (the app framework) already ratified this shape — its **ENGINE-1**
  law states the editor is "ONE engine with three faces", and its
  authoring core is deliberately written with no DOM type in any signature
  so it can be reused by non-browser faces. A neutral paint model is what
  lets Frame honour that.

---

## 5. Per-face cost, honestly

**Terminal** — the largest piece of *new* code, but far smaller than it
looks: a cell rasteriser for `Frame`, a crossterm→`KeyCode` translator
(~200 lines), truecolor/256-colour quantisation of the existing theme, and
damage tracking. It reuses document, history, input, search, folds,
viewport, gutter, highlight, and cursor logic unchanged.

Blocked on **D1 (keymap model)** — an nvim-tone terminal face is a
statement about modality, and that decision shapes the input layer for all
three faces. This remains the highest-leverage unanswered question in the
repo, and it gates the face Tom actually lives in.

**Desktop** — the *cheapest* face. `iridium-render` already exists and
already drives the web face; desktop is a winit window, a wgpu surface,
native clipboard, and real IME events fed to a state machine that is
already sound. Little new logic, mostly shell.

**Web** — already works. After the split, `iridium-bindings` is gutted to
an event translator. Its two outstanding items are independent of this
work: the production build ships no `.wasm` (dev-only vite aliasing masks
it), and IME needs the hidden-editable-element pattern because a canvas
fires no composition events.

---

## 6. Sequencing

Nothing here changes the phase order in `PLAN.md`; it sharpens Phase 2.

1. **Phase 2a — hoist the compositor** out of `wasm.rs` (3,659 lines, the
   worst file in the repo) into `iridium-render`. Fixes the biggest module
   and is a prerequisite for the desktop face.
2. **Phase 2b — split the crates** along the §3 boundary, moving the five
   pure layout files into core. Turn on the no-GPU CI gate.
3. **Phase 2c — introduce the `Frame` paint model**, and port
   `iridium-render` onto it. One face on the new seam proves it before a
   second face depends on it.
4. **D1**, then **Phase 4 (terminal)** — now a rasteriser plus a keymap,
   not an editor.
5. **Phase 5 (desktop)** — mostly shell over the render crate.

Do 2a before 2c. Building the paint model while the compositor is still
buried in `wasm.rs` means designing the seam against the wrong shape.

---

## 7. Open questions for Tom

- **D1 (keymap model)** — unchanged from `PLAN.md` §7, and now clearly the
  gate on the face you care most about. Recommendation stands: keymap
  layer, non-modal default, modal as a keymap.
- **Paint-model granularity.** Per-run styling is proposed. Per-glyph would
  permit ligature and variable-font control on GPU faces at the cost of a
  fatter shared model and a terminal that discards most of it.
  Recommendation: per-run.
- **Does the terminal face get the minimap?** It can be drawn in cells, but
  Rams would probably say no. Interacts with **D2**.
