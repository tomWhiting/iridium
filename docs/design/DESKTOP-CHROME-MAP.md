# Desktop Chrome Map — real rounded panels for the overlay chrome

Produced 4 Aug 2026 from a full read of the desktop overlay path, the
kernel's quad renderer, and the web demo's React overlays, after the
controlling verdict on the first painted chrome: the box-drawing glyph
borders read as "the terminal editor in a window," and the border
strokes leave visible gaps at code line-spacing. The ruling this map
serves: panel chrome becomes **real shapes** — true rounded-rectangle
panels, real padding, a shadow — so the face reads as a desktop app.
Rounded corners are mandatory and must be genuine arcs (Tom's standing
UI rule: rounded, never sharp).

Citation legend: `O:` = `apps/iridium-desktop/src/overlay.rs` (720
lines), `A:` = `apps/iridium-desktop/src/app.rs`, `Q:` =
`crates/iridium-editor/src/render/quad.rs`, `T:` =
`crates/iridium-editor/src/render/text.rs`, `C:` =
`crates/iridium-editor/src/render/compositor.rs`, `S:` =
`apps/iridium-desktop/src/search.rs`, `P:` =
`apps/iridium-desktop/src/command_palette.rs`, `H:` =
`apps/iridium-desktop/src/history_overlay.rs`, `Pr:` =
`apps/iridium-desktop/src/prompt.rs`, `L:` =
`apps/iridium-desktop/src/line.rs`, `M:` =
`apps/iridium-desktop/src/mouse.rs`, `Th:` =
`crates/iridium-editor/src/theme/colors.rs`, `mod:` =
`crates/iridium-editor/src/render/mod.rs`, `W:` =
`examples/web/src/CommandPalette.tsx`, `U:` =
`examples/web/src/UndoTreePanel.tsx`, `gl:` = vendored
`~/.cargo/registry/src/…/glyphon-0.10.0/src/`. Font metrics were read
directly from the embedded face's TTF tables (the `JetBrains Mono`
bytes at A:110, the same file the web demo serves).

Prior art: `docs/design/RETAINED-SHAPING-MAP.md` (a **concurrent**
change in `crates/iridium-editor/src/render/**`; §2.6 and §4 here state
how this change stays out of its way), `docs/design/COMPOSITOR-EXTRACTION-MAP.md`,
`docs/DESKTOP-SHELL-PLAN.md` (D-C: the full overlay set ships in v1).

## The two load-bearing facts, up front

1. **The gap defect is arithmetic, not rendering noise.** The border
   glyphs ink exactly the font's design cell (1.32 em); cosmic-text
   spaces baselines at the renderer's 1.4 em line height (T:20,
   T:175-177). Every vertical join in the border therefore falls short
   by 0.08 em — ~2.2 physical px per row at the face's working size —
   and through each gap the *document* shows, because the panel's
   background quad is deliberately inset behind the border column
   (O:506-514). No tuning of the glyph approach closes this without
   giving up code line-spacing. Receipts in §1.4.

2. **The seam for a pure reskin already exists.** Panel *content* is
   composed windowlessly by the builders (S:255, P:211, H:126) into
   `PanelContent` — rows of coloured runs plus a caret (O:183-193) —
   and only `OverlayPainter` turns that into pixels (O:38-41). The
   chrome change is confined to the painter's geometry and quads plus
   one new kernel render module; the builders, their tests, and every
   key path are untouched (§4).

## 1. The current chrome, inventoried

### 1.1 The pass structure

After `FrameCompositor::compose` draws the document, a **second render
pass** runs on the same texture view with `LoadOp::Load` — the composed
frame is kept, not cleared (O:14-25, the load op at O:399). Order
within the pass: all quads first (backgrounds, selected rows, carets),
then all text (O:409-410). The painter owns its own `TextRenderer` —
its own font system, atlas and prepared-area slot, separate from the
compositor's, because glyphon's `prepare` uploads one frame's areas per
renderer (O:16-21, T:433-452). The strip and every open panel are
shaped into individual buffers, prepared together as one frame's text
areas (O:353-386), and the face's `render_frame` closure presents both
passes as one frame (A:1107-1136). The desktop drives it from `redraw`:
strip content from the prompt/message (A:1054-1067), panels bottom-most
first so the palette paints on top (A:1159-1186; order: search,
history, palette).

Primitives: the kernel's `QuadRenderer` and `TextRenderer`, publicly
exported precisely so a face paints overlays with renderers the kernel
already trusts (O:8-12, mod:66-68).

### 1.2 The strip

The prompt strip is one line high plus padding, pinned to the bottom,
full width (O:43-46). Its geometry: height = `line_height + 2 × 6.0`
(`STRIP_PAD`, O:67, O:429), horizontal text padding 10 px (`TEXT_PAD`,
O:64), caret a 2 px quad (`CARET_WIDTH`, O:70, O:451-461). It is
drawn as: one full-width background quad in `theme.editor.gutter`
(O:444-450), the caret quad, then a single shaped text buffer clipped
at the padding (O:354-369). Field scrolling keeps the caret in the last
cell (`scroll_for`, O:626-637).

A finding worth its own line: **in the dark preset the strip's
background quad is a no-op** — `theme.editor.gutter` is
`Color::new(0,0,0,0)` (Th:140), and the quad pipeline alpha-blends
(Q:210), so a fully transparent quad contributes nothing and the
strip's text sits directly on document text. The redesign gives the
strip the same honest opaque background derivation the panels already
use (`panel_background`, O:593-613, whose opacity both presets are
tested for at O:711-719).

### 1.3 The panels

A panel is horizontally centred, capped at `PANEL_MAX_COLUMNS = 64`
exterior columns (O:74), refused below 20 columns or 4 rows
(O:76-82, O:296), anchored top (palette, undo tree) or bottom above
the strip (search) (O:169-179, O:500-504; the strip's height is
reserved at O:337-341). `panel_fit` converts the window to grid units
and hands builders `content_columns = exterior − 4` — border plus one
pad space each side — and `max_interior_rows` (O:290-303). Height is
`(interior_rows + 2) × line_height`: two whole rows are spent on the
border lines (O:280-282, O:492-495).

What is glyph and what is quad today:

| Element | Mechanism | Where |
|---|---|---|
| Border `╭ ─ ╮ │ ╰ ╯` | **text** — spans in `theme.editor.foreground`, shaped into the panel's buffer with the content | O:538, O:557-586 |
| Panel background | one sharp quad, inset **half a cell on every side** so its corners hide behind the arc glyphs | O:506-514 |
| Selected row | translucent quad in `theme.editor.selection` behind the row's text | O:515-525 |
| Field caret | 2 px quad in `theme.editor.cursor`, placed at `(caret.column + 2) × char_width` — the `+2` is the border-plus-pad columns | O:526-536 |
| Match highlight | **per-grapheme-cluster rich text** — matched clusters recoloured to the theme's current-match colour at full alpha, never split mid-cluster | P:406-435, P:440-443 |
| Row content | coloured runs on the char grid, budgeted by `LineBuilder` | L:16-24, O:120-158 |

Fonts and sizing: everything is `JetBrains Mono` embedded (A:110), at
`BASE_FONT_SIZE = 14.0` logical × the window's scale factor (A:102,
A:183-184), reloaded on scale change because `load_font` is the only
char-width remeasuring path (A:1027-1044, T:189-225). The grid is
`char_width × line_height` with `line_height = font_size × 1.4`
(T:17-20, T:175-177) — the compositor's own grid, deliberately shared.

### 1.4 The gap defect, measured

Read from the embedded font's tables (unitsPerEm 1000):

- hhea ascent 1020, descent −300, lineGap 0 → the font's design cell
  is **1.320 em** (OS/2 typo and win metrics agree exactly).
- U+2502 `│` inks y ∈ [−300, 1020] — **exactly the full 1.32 em
  cell**, designed to abut vertically under terminal packing.
- U+256D `╭` inks y ∈ [−300, 420] — its stem runs to the cell bottom,
  meeting the next row's `│` top only if baselines are 1.32 em apart.
- U+2500 `─` inks x ∈ [−20, 620] against a 600-unit advance — the
  horizontals *overlap* their neighbours, which is why the top and
  bottom border lines look fine and only the verticals break.

The overlay shapes panel text at `Metrics::relative(font_size, 1.4)`
(T:204, T:229-231, buffer creation at O:540-545 via T:248-253):
baselines land 1.4 em apart, glyph ink spans 1.32 em, so **every
vertical join gaps by 0.08 em** — 2.24 physical px at the working size
(28 px font on a 2× display), at every row boundary of both border
columns and at all four corner-to-stem joins. And because the
background quad is inset half a cell precisely to stay behind the arc
glyphs (O:506-507), the border columns sit *outside* the quad: each
gap opens onto the document under the panel, not onto panel
background. A terminal never sees this because a terminal cell **is**
the font cell — the TUI's `FloatingBox`
(`crates/iridium-tui/src/frame/panel.rs:42`) packs rows at 1.32 em by
construction. The defect is structural to drawing terminal furniture
at code line-spacing; the fix is to stop drawing chrome with glyphs.

## 2. The rounded-corner capability, priced

### 2.1 What `QuadRenderer` actually is

One pipeline, one shader, sharp rectangles only. Vertex format:
`QuadVertex { position: [f32; 2], color: [f32; 4] }`, 24-byte stride,
per-vertex step mode (Q:23-40). Uniforms: viewport size alone, bound to
the vertex stage (Q:44-49, Q:164-176). Shader: WGSL embedded as a
string constant — vertex converts pixel coords to clip space, fragment
returns the interpolated colour flat (Q:81-115). Pipeline:
`TriangleList`, alpha blending, no depth (Q:196-223). Six CPU-side
vertices per quad into a pre-allocated buffer of `MAX_QUADS = 1024`
(Q:118, Q:281-313). Consumers: the compositor owns **two** instances
(background and cursor layers, C:50, C:182-186) and the overlay
painter owns a third (O:213). It is exported to every face from
mod:66.

The web face inherits whatever this module can do in the same sense it
inherits everything kernel-side: its GPU canvas drives the identical
`FrameCompositor` (`crates/iridium-bindings/src/wasm.rs:1592`), and the
render module's exports are available to any face (mod:28-37, 60-68).
But note the honest asymmetry: **the web demo's overlays are React
DOM** (W, U) — they already have real rounded corners for free and use
no quads. Inheritance of a rounded-quad capability matters for the
kernel's future (GPU-side chrome on any face, rounded minimap/scrollbar
furniture), not for the web palette as it exists.

### 2.2 Path (a) — SDF rounded-rect, the standard approach

A fragment-shader signed-distance field: for pixel `p` relative to the
rect centre with half-size `b` and radius `r`,
`d = length(max(abs(p) − b + r, 0)) − r`; coverage
`alpha = 1 − smoothstep(−0.5, 0.5, d)` gives a genuinely circular,
antialiased arc at any scale. What it needs beyond today's shader:
per-quad rect geometry and radius must reach the fragment stage, which
the current 24-byte per-vertex format cannot carry.

Two ways to get it, priced:

- **Complicate the existing pipeline**: widen `QuadVertex` with
  `rect: [f32; 4]` + `radius: f32` + `blur: f32` (24 → 48 bytes,
  duplicated across all 6 vertices), branch the fragment shader.
  Cost: every existing sharp quad — the compositor's whole quad
  volume, gutter bands, selections, carets (C:182-239) — pays the
  fatter vertex and the branch for nothing; and it **edits `quad.rs`,
  a file the concurrent retained-shaping work's compose path depends
  on** (C:50). Priced and declined.
- **A separate sibling pipeline** (recommended): new module
  `crates/iridium-editor/src/render/rounded.rs` mirroring `quad.rs`'s
  structure (ctor pattern Q:148-244, viewport uniform Q:44-49,
  pre-allocated CPU buffer Q:118-138). **Instanced**, not per-vertex:
  one instance = `{ rect: [f32; 4], color: [f32; 4], radius: f32,
  blur: f32 }` (40-byte stride, `VertexStepMode::Instance`); the
  vertex shader expands a 4-vertex strip from `@builtin(vertex_index)`
  with a `3 × blur` margin so shadows have room to fall off; the
  fragment evaluates the SDF. ~300 lines including CPU-side tests,
  exported next to `Quad`/`QuadRenderer` (mod:66). Zero edits to any
  existing render file except two lines in `mod.rs`.

Draw-call shape in the overlay pass: quads (sharp) → rounded quads →
text. One extra pipeline switch, ≤ ~10 instances a frame. The overlay
pass's per-frame budget is a handful of quads plus one small shaped
buffer; this does not move it.

### 2.3 Path (b) — arc approximation with N small quads per corner

No shader change: slice each corner into 1-px-tall sharp quads whose
widths trace the arc. At the recommended 8 logical px radius on a 2×
display that is 16 slices × 4 corners = 64 quads per panel — fine
against `MAX_QUADS` (Q:118) — but the slices have **no antialiasing**:
the quad fragment shader writes flat coverage (Q:111-114), so the arc
is a staircase of hard edges. That is visibly *worse* than the glyph
arcs being replaced, which at least came through glyphon's antialiased
rasterizer. Softening the staircase means per-slice alpha ramps
computed on the CPU — re-implementing the SDF, badly, per frame. And
it is desktop-face-only by construction: the arithmetic would live in
`overlay.rs` and teach the kernel nothing. Declined on quality, not
cost.

### 2.4 Path (c) — what the estate already has

Inventoried honestly:

- **glyphon custom glyphs**: `TextArea.custom_glyphs` (gl
  custom_glyph.rs:7-30; the overlay currently passes `&[]`, T:632) can
  draw arbitrary CPU-rasterized alpha masks through the text atlas via
  a rasterization callback. It could, in principle, rasterize
  rounded-rect masks. Declined: it is designed for icon-sized glyphs —
  a panel-sized mask is an enormous atlas entry re-rasterized on every
  panel resize, churning the same atlas the overlay's text lives in
  (trimmed every frame, O:414).
- **wgpu 28** ships no shape or vector utility; **glyphon 0.10** is
  text plus the above; the workspace graph has no lyon, vello, or any
  other tessellation/vector crate (root `Cargo.toml` workspace
  dependency list, lines 22-90). Nothing to borrow.

### 2.5 The shadow, priced with the corner

A soft drop shadow is a **blurred rounded rect** — the same SDF with a
wider falloff: `alpha = shadow_alpha × (1 − smoothstep(−blur, blur,
d))` (a good-enough Gaussian stand-in at these blur sizes). With path
(a) the shadow is *one more instance* of the same pipeline: drawn
first, slightly offset and enlarged, `blur > 0`; the panel quad then
draws crisp (`blur = 0`) on top. Marginal cost of shadow once SDF
exists: ~zero.

The alternative — layered translucent sharp quads (8-12 nested
outlines stepping alpha down) — bands visibly on dark backgrounds,
multiplies quad count by the layer count, and with path (b) corners
each layer would itself need arc slices: the two costs multiply. The
shadow question and the corner question have the same answer; deciding
them together is the point of R1/R2.

### Recommendation

**Path (a), as the separate instanced pipeline, with the shadow from
the same shader.** It is the only path that produces genuine arcs
(the mandate), it prices the shadow at ~zero marginal, it leaves
`quad.rs` and the compositor untouched while retained shaping is
mid-flight in the same tree, and it lands the capability kernel-side
where every face can reach it (mod:66's existing pattern).

## 3. The design language

The polish reference is the web demo's palette (W) and undo-tree panel
(U) — the two share one vocabulary. Extracted values, and their GPU
adaptation:

| Element | Web reference | GPU face target |
|---|---|---|
| Corner radius | 8 px (W:233, U:204); 4 px on key-hint chips (W:306) | **8 logical px × scale factor** on panels; 4 logical on the selected-row quad (§ below) |
| Panel background | opaque `#252525` over a dimmed page (W:231) | keep `panel_background(theme)` — current-line band composited opaque over the editor background, gutter fallback (O:593-613); already tested opaque in both presets (O:711-719) |
| Border | 1 px `#444` hairline (W:232) | **1 physical px hairline**: outer rounded quad in a border colour, inner background quad inset 1 px with radius − 1. No `EditorColors` field exists for it (Th:76-125); derive `foreground` at ~0.18 alpha composited over the panel background (the O:616-624 helper). Theme deserialization is forgiving (Th:70-73), so a dedicated `panel_border` colour is a safe *later* addition — not this change |
| Shadow | `0 16px 48px rgba(0,0,0,0.55)` (W:234, U:205) | one SDF instance: offset (0, 4 logical px), blur 16 logical px, black at 0.4 alpha; values are Tom's to tune at the rebuild (§5) |
| Padding | input `0.75rem 1rem`, rows `0.4rem 1rem` (W:238, 255) | panel interior padding **12 logical px horizontal, 8 logical vertical**; content stays on the char grid inside it (§ below) |
| Selected row | full-width `#3a4a63` band (W:258-260) | keep the translucent `theme.editor.selection` quad (O:515-525) — but drawn through the rounded pipeline at radius 4 so *no sharp corner exists anywhere in the chrome*, honouring the standing rule the old `panel_text` test asserted with glyphs (O:679-686) |
| Match highlight | bold + `#8ab4f8` (W:298-300) | keep the per-cluster recolour in `search_match_current` at full alpha (P:406-443, Th:143) — unchanged |
| Backdrop | `rgba(0,0,0,0.45)` full-screen behind the modal palette (W:220) | one full-screen translucent quad **behind top-anchored panels only** (palette, undo tree), lighter — 0.25 — because the editor text is the product; never behind the search panel (it must not dim the matches it just highlighted, A:940-943's viewport reasoning) or the strip. Ruled in R5 |
| Top anchor | `paddingTop: 12vh` (W:219) | replace `PANEL_TOP = 1` row (O:80-82, O:502) with **2 × line_height** — reads as floating, stays near the top for long lists |

The web hex values are **not** imported: overlay colours continue to
come from the theme system exactly where they come from today —
`panel_background(theme)` (O:593-613), `theme.editor.selection`
(O:521), `theme.editor.cursor` carets (O:533), `theme.editor.gutter`
strip → replaced by `panel_background` (§1.2 defect),
`theme.editor.foreground`/`line_number`/`search_match_current` text
(O:464-467, P:348-350, S:295). What is imported is the *shape
language*: opaque panel a step above the editor surface, hairline,
real radius, one soft shadow, real padding.

**Geometry, concretely** (all logical values × the window scale
factor; the painter needs the scale factor plumbed in — today only
`app.rs` knows it, A:183, A:1035):

- `panel_width = content_columns × char_width + 2 × PAD_X`,
  `panel_height = interior_rows × line_height + 2 × PAD_Y`, with
  `PAD_X = 12`, `PAD_Y = 8` logical. The two border *rows* and four
  border *columns* the glyph chrome spent (O:492-495, O:300) are
  returned to content: `panel_fit` derives `content_columns` from
  `(min(width, PANEL_MAX_COLUMNS × char_width) − 2 × PAD_X) /
  char_width` — the 64-column cap (O:74) and the 20-column floor
  (O:78) and `PANEL_MAX_VISIBLE_ROWS = 12` (O:86) all stay, so panel
  content is the same width class the terminal face draws.
- Content origin `(x + PAD_X, y + PAD_Y)`; the caret quad's `+2`
  column offset (O:529) becomes `PAD_X`; the selected-row quad spans
  `x + 4 → panel_width − 8` logical inset so its rounded corners never
  overhang the panel's.
- Radius clamped to `min(panel_width, panel_height) / 2` — the
  two-row search panel stays honest.
- The **strip** stays a docked, edge-to-edge bar: full width, no free
  corners to round (the mandate concerns corners that exist), opaque
  `panel_background`, a 1-physical-px hairline along its top edge, and
  its existing paddings (`TEXT_PAD`, `STRIP_PAD`) bumped to the shared
  12/8 logical scale. `strip_height` keeps its meaning for the
  search-panel reserve (O:337-341) and caret-visibility inset
  (A:950-954).
- `panel_height(interior_rows)` (O:280-282) changes formula
  (`rows × line_height + 2 × PAD_Y`) but keeps its contract; its one
  caller is the caret-visibility inset above.

## 4. What stays untouched — and the seam that guarantees it

**Untouched entirely:**

- The builders and their semantics: search rows, degradation order and
  caret math (S:255-338); palette rows, match display and key hints
  (P:211-267, P:340-403); undo-tree rows (H:126-200); the
  `LineBuilder` budget arithmetic (L:16+). They emit `PanelContent`
  against a `PanelFit` and never see pixels.
- The overlay state machines and key handling: prompt modality
  (Pr:1-27), palette/search/history outcome types, the app's dispatch.
  The TUI test suites remain the spec for these semantics; nothing
  here touches semantics.
- Focus discipline and panel stacking order (A:1159-1186).
- `PanelContent`/`PanelRow`/`Span`/`PanelCaret`/`PanelFit` types
  (O:100-204) — `PanelFit` keeps its two fields; only its *derivation*
  changes (§3).
- Mouse: panels are not mouse-interactive — hit-testing covers the
  document viewport only (M:203) — so no hit geometry can drift.

**The seam:** builders → `PanelContent` (data) → `OverlayPainter`
paints (O:38-41, A:1164-1185). It already exists; the one place
geometry and painting are mixed is inside `shape_panel` itself
(O:477-554), which computes placement while pushing quads. The minimal
split this change should make: extract a pure `PanelGeometry` value
(exterior rect, content origin, radius, per-row rects, caret rect)
computed by a free function of `(window w/h, char_width, line_height,
scale, anchor, rows, content_columns, reserve_bottom)` — CPU-testable
with no GPU (§5) — and have `shape_panel` consume it. `panel_text`
(O:557-586) and `truncate_chars` (O:588-591) are deleted with the
glyph borders; row padding/truncation moves into the per-row span
assembly since the right border no longer needs a landing column.

**Interaction with retained shaping** (the concurrent change): the
overlay pass runs after compose on the same view in the same frame
(O:22-25, A:1107-1134) and shares nothing with the compositor but
device/queue/format — it owns its own `TextRenderer` and atlas
(O:16-21, O:208-214). This change's only kernel touches are one *new*
file and two export lines in `mod.rs`; `quad.rs`, `text.rs` and
`compositor.rs` are not edited, so the only merge point with the
retained-shaping track is trivial (`mod.rs`). Behavioural interplay is
positive: overlay keystrokes currently recompose the whole document
beneath the panel (RETAINED-SHAPING-MAP §1); once its stage 1 lands
those frames become compose cache hits, and the overlay pass — a
handful of quads, ≤ ~10 rounded instances, one small shaped buffer —
is what a panel keystroke costs.

## 5. Proof plan

**Stays green (the reskin's regression fence):** every content-level
test — search panel rows/caret/degradation (S:595 onward, asserting
via `PanelRow::text`), palette rows and match highlighting (P's test
module), undo-tree rows (H:273 onward), prompt semantics (Pr),
`scroll_for` (O:648-663), `panel_background` opacity in both presets
(O:711-719), and the TUI suites, which never touch this face.

**Dies with the glyphs, replaced:** the `panel_text` tests (O:666-708),
including `no square corner may appear anywhere` (O:682-686). Their
*statement* survives translated: the new geometry tests assert
`radius > 0` for every drawable panel in both presets — the same rule,
now about arcs instead of characters.

**New tests (all CPU-side, no GPU):**

1. `PanelGeometry` (§4): placement and centring per anchor;
   `reserve_bottom` stacking (search above strip); padding arithmetic;
   radius clamp `≤ min(w, h) / 2`; caret and selected-row rects strictly
   inside the content area; the fit round-trip — content composed
   against a `PanelFit` always produces geometry that fits the window
   that produced the fit, at 1× and 2× scale.
2. The rounded module's CPU half: instance packing (stride/layout),
   the blur margin expansion, degenerate rects (w or h ≤ 2r), the
   `MAX_*` truncation mirroring Q:281.
3. The strip: opaque background in both presets (the §1.2 defect,
   pinned the way O:711-719 pins the panels).

**Judged by eye, honestly:** the shader's arcs, the shadow's weight,
the exact radius/padding/backdrop values. No CPU test proves a shadow
looks right. The change ends with a bundled rebuild handed to Tom —
the feel gate — with R3/R5's numbers explicitly his to move; the
implementation runs the full suite (`cargo test`, clippy pedantic per
CLAUDE.md) before that handoff. This map itself was produced read-only
against a live build in the same tree; no build was run for it.

## 6. Open rulings for the controlling seat

- **R1 — Corner mechanism.** SDF rounded-rect in a **new instanced
  sibling pipeline** `render/rounded.rs` (recommended: genuine
  antialiased arcs, kernel-side so every face can reach it, zero edits
  to `quad.rs`/`compositor.rs` while retained shaping is in flight) vs
  widening the existing quad pipeline (every sharp quad pays; edits a
  shared file) vs per-corner quad slices (aliased staircase, worse than
  the glyphs it replaces, desktop-only). Recommend the new module.
- **R2 — Shadow.** Yes, via the same SDF shader's blur parameter — one
  extra instance per panel, offset (0, 4) logical, blur 16 logical,
  black at 0.4 alpha (recommended; ~zero marginal cost once R1 lands)
  vs layered translucent quads (bands, multiplies against the corner
  cost) vs no shadow (the web reference and "reads as a desktop app"
  both say shadow). Recommend yes-via-SDF.
- **R3 — Radius and padding.** 8 logical px radius (the web
  reference's exact value, W:233), 12/8 logical px interior padding,
  top anchor at 2 × line_height, radius clamped to half the panel's
  short side. Recommend these as the starting numbers, explicitly
  Tom's to tune at the rebuild.
- **R4 — Border.** Hairline (1 physical px, two nested SDF quads,
  colour derived from `foreground` at ~0.18 over the panel background)
  vs none-with-shadow-only vs a new `panel_border` theme colour.
  Recommend the derived hairline now — it carries the light preset,
  where a shadow alone separates poorly — and note the additive theme
  field is safe later (Th:70-73's forgiving deserialization).
- **R5 — Dimmed backdrop.** One full-screen quad at 0.25 black behind
  **top-anchored panels only** (palette, undo tree), never behind the
  search panel or strip (recommended: matches the web reference's
  statement at lower strength, one quad, trivially removable at the
  feel gate) vs none. Recommend yes at 0.25.
- **R6 — The glyph-border code.** Delete `panel_text`,
  `truncate_chars` and their tests (O:557-591, O:666-708) vs keep
  behind a TUI-parity flag. **Recommend delete**: the terminal face
  draws its own chrome with its own `FloatingBox`
  (`crates/iridium-tui/src/frame/panel.rs:42`) at the packing that
  makes glyph borders correct; a desktop flag would preserve a
  rendering this map just proved geometrically broken at code
  line-spacing. Anything worth resurrecting is in git.
- **R7 — Scale-factor plumbing.** The painter needs the window scale
  for logical→physical chrome values; today only `app.rs` has it
  (A:183, A:1033-1044). An explicit `OverlayPainter::set_scale` called
  from `open`/`rescaled` (recommended — mirrors `set_font`'s existing
  call discipline) vs deriving scale from `font_size /
  BASE_FONT_SIZE` (couples chrome to a constant the face could change).
  Recommend the explicit setter.

**Implementation file-touch list** (the whole change):
`crates/iridium-editor/src/render/rounded.rs` (new),
`crates/iridium-editor/src/render/mod.rs` (two export lines),
`apps/iridium-desktop/src/overlay.rs` (the reskin),
`apps/iridium-desktop/src/app.rs` (scale plumbing; nothing else).

## RULINGS — controlling seat, 4 Aug 2026

Binding on the implementation. Context for all of them: Tom ruled (22:02Z
3 Aug) that the native shell stays and the target is visual parity with
the web demo's chrome — "make it look the same." The web demo is the
spec; where a ruling below conflicts with what the web demo actually
renders, the web demo wins and the deviation is reported, not silently
chosen.

- **R1**: new instanced SDF module `render/rounded.rs`, sibling to
  `quad.rs` — accepted. `quad.rs` stays untouched (the retained-shaping
  track depends on it); every face inherits the capability.
- **R2**: shadow via the same SDF blur — accepted with the agent's
  starting values (offset 0/4 logical, blur 16, alpha 0.4), tuned
  against the web demo's actual shadow if it differs.
- **R3**: radius 8 px, padding 12/8 px, top anchor 2 line-heights — as
  STARTING values only; the web demo's extracted values override where
  they differ, and Tom tunes by eye at the bundled rebuild.
- **R4**: derived 1-px hairline border, no theme schema change —
  accepted.
- **R5**: ADJUSTED — backdrop treatment MATCHES THE WEB DEMO exactly.
  If the React palette renders no backdrop dim, the native one gets
  none; if it dims, copy its value. The implementer verifies from
  CommandPalette.tsx and reports what the web demo actually does.
- **R6**: the desktop glyph-border code is DELETED. The TUI's
  `FloatingBox` is its own correct chrome at terminal packing and is
  not touched.
- **R7**: explicit `set_scale` plumbing — accepted.
- **Addendum**: the transparent strip-background finding (dark preset,
  `theme.editor.gutter` alpha 0 makes the strip's backing quad a no-op)
  is IN SCOPE for the reskin — the strip gets an honest background.
