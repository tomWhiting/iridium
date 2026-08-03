# Compositor Extraction Map — `wasm.rs::render_frame` → `render/compositor.rs`

Produced 3 Aug 2026 by a full read of the render path, for step 1 of
`docs/DESKTOP-SHELL-PLAN.md`. `W:` = `crates/iridium-bindings/src/wasm.rs`
(4015 lines). `E:` = `crates/iridium-editor/src/render/`.

Prior art already in tree: `docs/DESKTOP-SHELL-PLAN.md:46-55` (Step 1 spec),
`docs/TRIPLE-FACE.md:223-235`, `docs/SOFT-WRAP-DESIGN.md` (kernel wrap design
— not yet implemented).

## Rulings for this step (conservative by default — a refactor changes nothing observable)

The five open questions at the bottom are ruled as follows for step 1; every
ruling favors the smallest diff that keeps the web demo pixel-identical.
Revisiting any of them is a later step, not this one.

1. **Highlight ownership**: the compositor takes pre-resolved spans from the
   face (option a). `WebSpanIndex` stays in the bindings crate. Moving it
   kernel-side is real work with its own oracle; not this step.
2. **Surface abstraction**: concrete parameters
   (`&TextureView, &Device, &Queue, width, height`), no trait — a generic on
   the compositor would fight `wasm-bindgen` for nothing.
3. **`scroll_y` ownership**: stays on the face. Compositor takes it as input.
4. **`pixel_ratio`**: preserved as-is on `WebEditor` (written, never read) —
   observable behavior unchanged.
5. **Frame timing**: the timer and slow-frame `log()` stay in `wasm.rs`,
   wrapped around the `compose` call — the compositor stays free of any
   logging seam, and the observable log output is unchanged.

Also ruled: the compositor entry point is named **`compose`**, killing the
`render_frame`-means-two-things ambiguity the map flags in §7.

## OUTCOME — landed 3 Aug 2026, commit 230f5ed

The extraction executed to this map. All five gates green (workspace
all-features 1619/0, GPU-free kernel, wasm32 check, clippy zero-new, fmt);
wasm.rs 4,015 → 3,016 lines. Deviations from the rulings, each forced and
none pixel-observable:

- **Ruling 1 amended in mechanism, not in substance**: the highlight seam is
  a `HighlightSource` trait rather than a pre-built span-list parameter —
  the spans borrow the visible-content string the compositor builds
  mid-compose, so a pre-built argument is unrepresentable without a
  two-phase API or per-span allocation. The face still owns tree-sitter
  resolution (`WebHighlightSource` in wasm.rs, bodies verbatim); the
  decision tree is identical.
- Surface params grouped in a concrete `FrameTarget` struct (Ruling 2 held;
  grouping avoids a `too_many_arguments` lint without `#[allow]`).
- Casts route through new `units.rs` helpers proven bit-identical to `as`
  by sweep tests (the moved code carried a banned `#[allow]`); sole
  domain-impossible difference: `dimension_to_bound` saturates where
  `as i32` would wrap, at surface sizes ≥ 2³¹ px.
- 11 `cpu_*` buffers moved, not 9 — this map's own field list enumerates 11.
- `ensure_cursor_visible` stayed on the face (Ruling 3: it mutates
  `scroll_y`); only `cursor_anchor_y` resolution moved.
- Frame acquisition now precedes CPU compose (compose receives the
  `TextureView`); on the prepare-failure path an acquired frame drops
  un-presented. Error-path strings render through `IridiumError`.

**Pixel oracle: PASS (3 Aug, headless chromium, apple/metal-3 WebGPU).**
Protocol: pre-extraction bundle served statically on 14571, two screenshots
(noise floor: byte-identical PNGs, 0 pixels), then one wasm-pack + one
`vite build` rebuild at HEAD, two more screenshots. Result: **every
GPU-composited canvas pixel byte-identical** across the extraction (canvas
device rows 194–1544; the sole diff region, 19,960 device pixels at
y 114–173, is two HTML toolbar buttons — "Commands" and "History" — from
app-shell commits newer than the 30 Jul dist, above the canvas entirely).
Console on the rebuilt side shows "Creating FrameCompositor" — the new path
demonstrably live. Artifacts in the session scratchpad under
`pixel-oracle/`.

**Found by the oracle — a real pipeline defect, unfixed:** `vite build`
emits the wasm-bindgen glue verbatim as a chunk but never emits
`iridium_bindings_bg.wasm` as an asset, so a freshly built
`examples/web/dist` 404s on its own wasm and renders nothing. The served
demo only ever worked because something copied the wasm in by hand; after
every `vite build` today, `pkg/iridium_bindings_bg.wasm` must be copied to
`dist/assets/` manually. Worth a build-pipeline fix (vite asset include or
a copy step in the `build` script) as its own small change.

---

## 1. The render path, top to bottom

| Range | What | Verdict |
|---|---|---|
| W:1865-1873 `render()` | `needs_redraw` gate → `render_frame` | STAYS (thin) |
| W:1876-1879 `force_render()` | unconditional | STAYS (thin) |
| W:1883-1888 | fn head; imports `glyphon::{TextArea,TextBounds}`, `web_time::Instant`, `wgpu::*` | EXTRACT |
| W:1891, 2729-2735 | frame timer + slow-frame `log()` | SHARED (kernel has unused `view::FrameTimer`; `log()` is JS) |
| W:1893-1914 | viewport-uniform dirty check → 3× `update_viewport` | EXTRACT |
| W:1916-1931 | viewport line range (scroll_y/line_height, overscan=10), `viewport_start_byte` | EXTRACT |
| W:1933-2003 | build `cpu_visible_content`, `cpu_visible_doc_lines`, `cpu_doc_to_visual`; fold-hidden skip; folded-line " … }" suffix | EXTRACT (pure: Document + FoldState → String) |
| W:2005-2023 | line_height/char_width/padding=10; gutter width (custom vs `GutterRenderer::calculate_width`); `content_offset_x` | EXTRACT |
| W:2025-2027 | `content_width = surface.width() - offset`; `create_buffer(Some(w))` ← **this is where wrapping happens** | EXTRACT (width value is SHARED input) |
| W:2029-2086 | 4-way highlight branch: WebSpanIndex → `Viewport::query_byte_range` → `build_rich_spans_viewport`; legacy `ts_highlights`; `SimpleHighlighter`; plain. Then `shape_buffer` | **SEAM** — see §2 |
| W:2088-2102 | build `cached_visual_line_map` from `buffer.layout_runs()` | SHARED (built in render, read outside) |
| W:2104-2111 | `cached_total_visual_lines = visual_line + extra_wrap_lines` | SHARED |
| W:2113-2179 | line-number string with wrap-aware blank continuation rows (custom vs auto, `GutterRenderer::digit_columns`) | EXTRACT |
| W:2181-2191 | gutter buffer create/set/shape | EXTRACT |
| W:2193-2233 | cursor doc→visual, `virtual_scroll_offset`, `cursor_position_in_buffer`, `cached_cursor_abs_y`/`cached_cursor_doc_line` | SHARED (cache), EXTRACT (math) |
| W:2235-2236 | `cursor_renderer.update(Instant::now())` blink | EXTRACT |
| W:2238-2250 | gutter background quad | EXTRACT |
| W:2252-2310 | line-background (diff) quads, wrap-aware via visual line map + fallback | EXTRACT |
| W:2312-2366 | gutter change bars (3px @ x=2) | EXTRACT |
| W:2368-2493 | selection quads — per-cursor, per-doc-line, per-wrap-segment intersection; `newline_extra = char_width*0.5`; fallback at 2470-2491 | EXTRACT (largest single block, 125 lines) |
| W:2495-2528 | cursor quads (2px) for **every** caret, shared blink | EXTRACT |
| W:2530-2555 | main `TextArea` (glyphon), `adjusted_scroll_y` | EXTRACT |
| W:2557-2592 | blame buffer + `blame_left` | EXTRACT |
| W:2594-2643 | text_areas Vec (main / gutter / blame) | EXTRACT |
| W:2645-2647 | `text_renderer.prepare(device,queue,areas)` — `.map_err(JsValue::from_str)` | EXTRACT body, STAYS error map |
| W:2649-2668 | clear color; `cpu_background_quads` batch, order gutter→line-bg→change-bars→selection | EXTRACT |
| W:2670-2723 | `surface.render_frame(closure)`: encoder, render pass, bg quads → text → cursor quads, submit | EXTRACT body / **surface is the seam** |
| W:2726 | `trim_cache()` | EXTRACT |

**Renderers held:** `text_renderer: TextRenderer` (W:166), `background_quad_renderer: QuadRenderer` (168), `cursor_quad_renderer: QuadRenderer` (170), `cursor_renderer: CursorRenderer` (171), `gutter_renderer: GutterRenderer` (172), `highlighter: SimpleHighlighter` (173). **No minimap renderer exists in wasm.rs at all** — `MinimapRenderer` is unwired (only `show_minimap` config bool at `crates/iridium-bindings/src/editor.rs:845`). The compositor should not pretend to own one yet; flag as new work, not extraction.

## 2. Web-specific vs face-neutral — where the seam is

**Face-neutral already (EXTRACT wholesale):** everything W:1916-2668 except the four `self.surface.{width,height,device,queue}()` reads. Those appear at W:1895-1896, 1922, 2026, 2035, 2247, 2256-2257, 2315, 2373, 2545-2546, 2611, 2632-2633, 2646, 2679. All are plain `u32`/`&Device`/`&Queue` — nothing web-typed crosses.

**Genuinely web-typed (STAYS):**
- `Result<(), JsValue>` and the three `.map_err(|e| JsValue::from_str(...))` (W:1883, 2647, 2723)
- `log()` (W:327, 2731) — `web_sys` console binding
- `WebSurface` construction (W:355) and `HtmlCanvasElement` (W:10, 341)
- All `JsValue`/`js_sys::Reflect` ingest for spans/themes/blame/backgrounds (W:2773-2830, 1321-1471, 2831+)

**The seam is `WebSurface`.** `E:web.rs:43-296` — `WebSurface` already exposes exactly the face-neutral shape: `device()/device_arc()/queue()/queue_arc()/format()/width()/height()/resize()/render_frame(FnOnce(&TextureView,&Device,&Queue))`. Nothing in it leaks `web_sys` past construction (`from_canvas`, W:355). **Recommendation (adopted, see Rulings):** `FrameCompositor::compose()` takes `(&wgpu::TextureView, &Device, &Queue, u32 width, u32 height)` and returns `Result<(), IridiumError>`. `wasm.rs` keeps the `WebSurface` and the `JsValue` mapping; `apps/iridium-desktop` supplies a `NativeSurface` with the same accessors. That makes the closure at W:2681-2722 move into the compositor unchanged.

**Ambiguity ruled (see Rulings §1):** the highlight branch (W:2029-2085) reads `self.span_index: WebSpanIndex` (`crates/iridium-bindings/src/web_span_index.rs`) and `self.ts_highlights: Vec<JsHighlightSpan>` — both are *bindings-crate* types, not web types. `build_rich_spans_viewport` (W:1711-1779), `build_rich_spans_from_ts` (W:1786-1838) and `color_for_highlight_type` (W:1849-1860) are pure. Ruled: the compositor takes a pre-built span list and the face owns highlighting — smaller diff, pixel-identity trivial. Kernel-side span indexing is a later step.

## 3. Scroll/layout coupling — non-render consumers of render-built caches

`cached_visual_line_map` + `cpu_visible_doc_lines` + `cached_map_viewport_start` + `cached_content_offset_x` are **written only in `render_frame` (W:2091-2093, 2100)** and read by:

| Site | Consumer | Reads |
|---|---|---|
| W:1649-1661 `max_scroll_y()` (`getMaxScrollY`) | scroll clamp | `cached_total_visual_lines`, fallback `FoldState::visible_line_count` |
| W:1632-1636 `set_scroll_y` / 1639-1642 `scroll_by` | via `max_scroll_y` | transitive |
| W:1669-1699 `ensure_cursor_visible()` | scroll-to-caret | `cached_cursor_abs_y`, `cached_cursor_doc_line` |
| W:3708-3765 `pixel_to_position()` (`pixelToPosition`) | **hit-testing** | `cached_map_viewport_start` (3714), `cached_visual_line_map` (3726-3730), `cpu_visible_doc_lines` (3734), `cached_content_offset_x` (3742), `cached_char_width` (3710) |
| W:3775-3852 `position_to_pixel()` (`positionToPixel`) | inverse hit-test | same set (3788-3830) |
| W:3653-3657 `getLineHeight`, 3659-3662 `getCharWidth`, 3665-3673 `current_gutter_width`, 3676-3679 `getTextOffsetX`, 3682-3685 `getTextOffsetY` | JS layout queries | `text_renderer`, `cached_char_width`, `gutter_renderer` |

**Conclusion: the compositor must OWN these caches and EXPOSE them read-only.** `pixelToPosition` is called on every mousemove from `packages/@iridium/core/src/controller/index.ts:686` and `crates/iridium-bindings/ts/controller/index.ts` — it is on the input hot path, not the render path, and it demands the map be queryable between frames. Compositor API: `visual_line_map() -> &[(usize,usize)]`, `map_viewport_start() -> usize`, `content_offset_x() -> f32`, `total_visual_lines() -> usize`, `cursor_abs_y() -> (f32, usize)`, `char_width() -> f32`, `line_height() -> f32`. Then W:1649-1699, 3653-3685, 3708-3852 become one-line delegations and stay in `wasm.rs` as wasm-bindgen exports.

**Also note:** the fallback paths (W:1654-1657, 3751-3763, 3841-3851) fire before the first frame. Pixel-identity requires preserving them exactly, including the `.max(0.0) as usize` / `+ 0.5` rounding.

## 4. Renderer APIs — reuse assessment

- **`E:text.rs` `TextRenderer`** (`new(device,queue,format)`:112; `with_config`:132). Per-frame: `create_buffer(Option<f32>)`:248, `set_text`:262, `set_rich_text`:276, `shape_buffer`:303, `cursor_position_in_buffer(&Buffer,line,col,char_width)`:319, `visual_lines_per_logical_line(&Buffer)`:385, `update_viewport(queue,w,h)`:416, `prepare(device,queue,areas)`:433, `render(&mut RenderPass)`:463, `trim_cache`:475. Metrics: `line_height()`:175 (= font_size × ratio), `char_width()`:189 (**`&mut self`**, lazily measures "MM" and caches). Takes no `Editor`, no theme — **already face-neutral, use unchanged.**
- **`E:quad.rs` `QuadRenderer`**: `new(device,format)`:148, `update_viewport(queue,w,h)`:247, `render(pass,queue,&[Quad])`:268. `Quad::new(x,y,w,h,ThemeColor)`:69. **Face-neutral, unchanged.**
- **`E:cursor.rs` `CursorRenderer`**: `new(CursorConfig)`:139, `update(Instant)->BlinkState`:151, `is_visible()`:180, `reset_blink()`:188, `set_focus`:196, `set_style`:210. Also has `compute_cursors(...)`:249 / `compute_primary_cursor`:288 returning `Vec<CursorRect>` — **wasm.rs does NOT use them** (they are wrap-unaware; W:2495-2528 hand-rolls quads via `cursor_position_in_buffer`). Extract the hand-rolled version verbatim for pixel-identity; do **not** swap to `compute_cursors`. `reset_blink` is called from 8 non-render sites (W:608, 623, 637, 764, 1080, 1136, 1144, 1191, 3514, 3624) — the compositor must expose a `reset_blink()` passthrough.
- **`E:gutter.rs` `GutterRenderer`**: `new()`:181, `calculate_width(total_lines,char_width)`:239, `digit_columns(total)`:219 (associated fn). wasm.rs uses **only those three**; the richer `compute_background`:274 / `compute_line_numbers`:301 / `compute_fold_indicators`:397 / `hit_test_fold_indicator`:538 are unused by the web face. Face-neutral, unchanged.
- **`E:minimap/`** `MinimapRenderer` — pure geometry, **completely unwired**. Out of scope for a pixel-identical extraction.
- **`E:highlight.rs`** `SelectionRenderer::compute_selections`:215, `CurrentLineRenderer`:252, `SearchHighlightRenderer`:536 — all pure, all unused by wasm.rs. The W:2368-2493 selection block duplicates their intent but is wrap-aware where they are not. **Flag:** tempting to unify; doing so in this step will break pixel-identity. Defer.

## 5. Word wrap

**Wrapping today is entirely inside cosmic-text**, triggered by one line: `create_buffer(Some(content_width))` at W:2027 (`E:text.rs:248-256` → `buffer.set_size(font_system, width, None)`). There is no wrap code in the kernel, no `layout` module, no `WrapIndex`. Everything downstream — `cached_visual_line_map`, `visual_lines_per_logical_line`, `extra_wrap_lines`, the gutter blank-continuation rows, wrap-segment selection intersection — is a *readback* of glyphon's layout runs.

Consequence: **the compositor must own wrapping for now**, because wrapping is inseparable from `TextRenderer`+`Buffer`, and hit-testing depends on the readback. `docs/SOFT-WRAP-DESIGN.md` explicitly plans to delete this ("lets the GPU faces set `Wrap::None` and delete the entire ad-hoc wrap model currently living in `wasm.rs` — `cached_visual_line_map`, `visual_lines_per_logical_line`, `extra_wrap_lines`"). **Design the compositor's cache accessors as an interface, not as exposed fields**, so the kernel `layout::RowIndex` can replace the implementation later without touching `wasm.rs`. Do not attempt both changes at once.

## 6. Metrics / DPI

- `line_height` = `font_size × config.line_height` (`E:text.rs:175`) — font-derived, not canvas-derived.
- `char_width` = measured from the loaded font by shaping "MM" (`E:text.rs:189-224`), cached; copied into `WebEditor::cached_char_width` at W:484 in `load_font` (W:480-490). **Font-derived.** SHARED (hit-testing reads it at W:3710, 3777, 3667).
- `pixel_ratio: f32` (W:177) — **canvas-derived**, passed in from `window.devicePixelRatio` by the TS controller. Used **exactly once**, at W:375: `font_size = 14.0 * pixel_ratio`. It is stored on the struct (W:419) and **never read again** — flag as dead state, or as the field a future `set_pixel_ratio` needs. STAYS on the face (the compositor receives an already-scaled font size).
- Surface `width()`/`height()` are **physical pixels** — the TS resize observer does `canvas.width = width * pixelRatio` then `editor.resize(...)` (`packages/@iridium/core/src/controller/index.ts:660-670`; same in `crates/iridium-bindings/ts/controller/index.ts:329-338`). Canvas-derived → compositor input parameter, not compositor state.
- Mouse coords are also pre-multiplied by DPR in TS (`.../controller/index.ts:687-690`) before `pixelToPosition` — so the whole pixel space is physical throughout. No conversion inside Rust. Keep it that way.

## 7. Callers and cadence

- `render()` W:1865 — gated on `needs_redraw`. Only external caller found: `examples/web/src/App.tsx:30`.
- `forceRender()` W:1876 — **the real driver.** ~30 call sites in the TS controllers (`packages/@iridium/core/src/controller/index.ts` lines 589, 666, 678, 715, 723, 752, 820, 886, 1052, 1104, 1143 …; `crates/iridium-bindings/ts/controller/index.ts` lines 280, 337, 349, 376, 384, 398, 484, 504, 685, 714, 904, 929, 942, 949, 957, 964, 970, 976, 988, 994).
- **Cadence:** (a) **per keystroke / per mouse event / per scroll**, synchronously; (b) **per resize** via `ResizeObserver`; (c) a rAF loop that calls `forceRender()` only when `currentTime - lastBlinkTime >= 500ms` — i.e. the rAF loop is a **500ms blink ticker**, not a 60/120Hz redraw loop (`.../controller/index.ts:673-684` and `crates/iridium-bindings/ts/controller/index.ts:344-355`); (d) a debounced rAF for highlight updates (`packages/.../index.ts:501-504`).
- `self.surface.render_frame(closure)` at W:2681 is `WebSurface::render_frame` (`E:web.rs:275-296`) — a different function from `WebEditor::render_frame`. Same name, two meanings; the compositor entry point is named `compose` to kill the ambiguity (ruled above).

## Extraction checklist (ordering)

1. Introduce `FrameCompositor` in `E:compositor.rs` behind `feature = "render"`, owning: `TextRenderer`, 2× `QuadRenderer`, `CursorRenderer`, `GutterRenderer`, `SimpleHighlighter`, `Theme`, all 9 `cpu_*` buffers (W:210-231, 269, 273, 279), all 6 `cached_*` layout fields (W:238-252), `cached_viewport_{width,height}` (W:202-203), `cached_char_width` (W:192), `viewport_config` (W:200), `gutter_enabled` (W:181), `syntax_enabled` (W:179), and the presentation maps `line_backgrounds`/`gutter_changes`/`custom_gutter_lines`/`blame_data`/`syntax_theme` (W:259-279).
2. Entry point `compose(&mut self, &Editor, &FoldState, f32 scroll_y, view, device, queue, width, height) -> Result<(), IridiumError>` = W:1893-2726 moved verbatim, `self.surface.X()` → parameters, `JsValue` → `IridiumError`.
3. Add read-only accessors for §3's cache consumers; rewrite W:1649-1699, 3653-3685, 3708-3852 as delegations.
4. `WebEditor` keeps: `editor`, `surface`, `scroll_y`, `needs_redraw`, `pixel_ratio`, `fold_state`, `fold_syntax`, `span_index`, `ts_highlights`, `use_ts_highlights`, `read_only`, `keyboard_handler`, `palette_mru`, `pending_*` — plus `compositor: FrameCompositor`.
5. Oracle: the existing web demo, pixel-compared before/after. `web-test/` and `evidence/` exist in-tree — check whether either already has a screenshot harness before building one.
