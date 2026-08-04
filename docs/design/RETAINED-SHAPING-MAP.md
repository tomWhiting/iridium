# Retained Shaping Map — killing the per-frame reshape in `FrameCompositor::compose`

Produced 4 Aug 2026 by a full read of the compose path and the vendored
cosmic-text sources, for the follow-up ledger entry created by step 3 of
`docs/DESKTOP-SHELL-PLAN.md` (the measured finding at
`docs/DESKTOP-SHELL-PLAN.md:107-136`: live keydown→present p50 **41.17ms**
on a 10k-line dense viewport against a sub-8ms budget; headless compose
bench steady_state **35.2ms**, after_mid_file_edit **34.4ms**,
after_scroll_change **30.9ms**; diagnosis reproduced, not assumed — the
cost is per-viewport, sits in `compose` on the CPU, and is the rebuild +
reshape of the entire visible cosmic-text buffer every frame).

Citation legend: `C:` = `crates/iridium-editor/src/render/compositor.rs`
(1653 lines), `T:` = `crates/iridium-editor/src/render/text.rs`, `W:` =
`crates/iridium-bindings/src/wasm.rs` (3016 lines), `A:` =
`apps/iridium-desktop/src/app.rs`, `H:` =
`apps/iridium-desktop/src/highlight.rs`, `B:` =
`crates/iridium-editor/benches/compose_frame.rs`, `F:` =
`crates/iridium-editor/src/editor/fold_state/mod.rs`, `D:` =
`crates/iridium-editor/src/document/buffer.rs`, `ct15:`/`ct16:` = vendored
`~/.cargo/registry/src/…/cosmic-text-0.15.0/src/` and `…-0.16.0/src/`.
Prior art: `docs/design/COMPOSITOR-EXTRACTION-MAP.md` (the extraction this
change builds on), `docs/SOFT-WRAP-DESIGN.md` (the planned kernel wrap
model this change must not preempt — see §4).

## The two load-bearing facts, up front

1. **A keystroke is an edit.** Stage 1 (whole-buffer retention) collapses
   only the frames where *nothing* changed — caret blink, overlay repaint,
   mouse-move redraws, sub-line scroll. Typing bumps the document revision
   every frame, so stage 1 alone leaves the headline number — 41ms
   keydown→present while typing on a dense viewport — essentially where it
   is (the bench's after_mid_file_edit 34.4ms is that path). **Fixing
   typing requires stage 2.** This is stated here so nobody ships stage 1
   and declares victory.

2. **cosmic-text permits stage 2, but not through the API the compositor
   uses today.** `Buffer::set_rich_text` unconditionally resets every
   line's shape and layout caches — retention behind it is impossible. But
   `BufferLine::set_text` diffs and resets only lines that actually
   changed, `Buffer.lines` is a public `Vec<BufferLine>`, and each
   `BufferLine` privately caches its own `ShapeLine` and `Vec<LayoutLine>`.
   The per-line shaping cache we need **already exists inside the
   library**; the work is routing around `set_rich_text`'s scorched-earth
   reset, not building a cache. Receipts in §3.2.

Also found while verifying, and worth flagging because the task said
"cosmic-text 0.16": **the render path's `Buffer` is cosmic-text 0.15, not
0.16.** The compositor imports `glyphon::Buffer` (C:41), glyphon 0.10 pins
`cosmic-text = "0.15"` (glyphon-0.10.0 Cargo.toml:53-54, Cargo.lock:1000),
and the workspace's direct `cosmic-text = "0.16.0"` dependency
(root Cargo.toml:32, optional in iridium-editor's `render` feature) has
**zero** `cosmic_text::` references anywhere in `crates/iridium-editor/src`
— it is dormant. Every API fact below was verified in **both** vendored
versions; the two are identical in every method this design touches
(citations give both). The dormant dep is its own small cleanup (§6, R7).

## 1. The per-frame cost inventory

What a steady-state frame is, per face: on the web face, the rAF loop is a
500ms blink ticker calling `forceRender` (`COMPOSITOR-EXTRACTION-MAP.md`
§7), plus ~30 `forceRender` sites on the input paths — an idle focused
editor pays full compose twice a second. On the desktop face there is no
animation loop and no idle blink (A:16-23); its steady-state frames are
overlay repaints (palette/search/prompt keystrokes recompose the editor
under the overlay — A:1082-1136, `compose` at A:1108 runs before
`overlay.paint` at A:1122) and mouse-effect redraws.

The compose pipeline, top to bottom. "Steady input?" = does this step's
input change on a steady-state frame (blink / overlay / mouse-move):

| Step | Where | Cost class | Steady input? | Cached across frames today? |
|---|---|---|---|---|
| Viewport uniform sync | C:395, 663-673 | 3 GPU writes, gated | no | **yes** — `cached_viewport_{width,height}` C:204-207 |
| Viewport range math | C:401-413 | trivial | no | recomputed (cheap) |
| `build_visible_content` | C:415-416, 679-762 | **O(document lines)** — the doc→visual pre/post fill walks all 10k lines (C:705-712, 752-759) plus the viewport string build | no | **no** — rebuilt every frame |
| `frame_gutter_width` | C:424, 771-787 | trivial | no | recomputed |
| `create_buffer` | C:429 → T:248-253 | new `Buffer` + `set_size` | no | **no** — fresh buffer every frame |
| `fill_content_buffer` → `resolve` | C:431-437, 792-829; desktop H:179-225, web W:2794-2845 | span query + run `Vec` per frame | no | spans indexed per generation (H:106-141) but **resolved per frame** |
| `set_rich_text` | C:813-822 → T:276-297 → ct15 buffer.rs:738+, ct16:754-886 | collect attrs `Vec`, then **reset every line's shape+layout** (§3.2) | no | **no** |
| `shape_buffer` | C:438 → T:303-305 → `shape_until_scroll` ct15:412, ct16:413-489 | **THE cost**: full shape + layout of the whole viewport buffer, `Family::Monospace` resolved against a full native fontdb, height `None` so every line shapes | no | **no** — this is the ~30ms |
| Wrap readback | C:440, 833-864 | walk layout runs | no | written every frame; read between frames (hit-testing C:1333-1474) |
| `build_line_numbers` + gutter buffer create/set/shape | C:443, 868-934; C:446-455 | a **second shaped buffer** per frame (viewport-height digit strings, `Shaping::Advanced`) | no | **no** |
| Cursor position walk | C:459-498 → T:319-378 | walk layout runs, read-only | no | `cached_cursor_abs_y/_doc_line` written each frame C:497-498 |
| Blink update | C:501 | trivial | **yes** (blink phase) | n/a |
| Quad passes (gutter bg, line bg, change bars, selection, carets) | C:518-533, 940-1225 | O(viewport), read buffer+map read-only | blink yes (C:1200-1201 gates caret quads); selections on drag | rebuilt every frame — correct, cheap |
| Blame buffer | C:558-589 | tiny shaped buffer, only when blame set for cursor line | no | rebuilt |
| Text areas + glyphon `prepare` | C:595-630 → T:433-452 | glyph vertex upload; atlas rasterization only for unseen glyphs (`SwashCache`/`TextAtlas` persist, T:71-89) | scroll remainder enters `TextArea.top` C:537-544 | atlas/swash caches persist; prepare itself runs every frame |
| Render pass + submit | C:653, 1232-1276 | encode + submit | new swapchain view every frame | necessarily per frame |
| `trim_cache` | C:656 → T:475-477 | atlas trim | — | per frame |

Already cached across frames, for completeness: viewport dims (C:204-207),
`cached_char_width` (C:200-201; measured once per font, T:86-88, 189-199),
the glyph atlas and swash rasterization caches inside `TextRenderer`, the
desktop face's span index per parse generation (H:49-141, gated on
`{parses, revision}` H:112-115), and the between-frames layout answers
(C:246-262) — which are, however, *rebuilt* every frame, cached only in
the sense that hit-testing reads them between frames.

The shape of the problem is now exact: on a steady-state frame **every
input from `build_visible_content` through the gutter shape is unchanged**,
and all of it — including the two full shaping passes — is recomputed to
identical values. That recomputation is the measured 35.2ms.

## 2. The full input set of the shaped content buffer

Everything that, if changed, makes a retained shaped buffer stale. Each
row cites where the input enters compose. Miss one and the fix ships a
visual staleness bug.

| # | Input | Enters at | Change signal available today |
|---|---|---|---|
| 1 | Document content | `doc.line(doc_line)` C:727-729; `line_to_byte_offset` C:413 | `Document::revision()` D:124-131 — bumps on every text mutation, never on cursor moves (test D:455) |
| 2 | Viewport line range `viewport_start..viewport_end` | derived from `scroll_y`, `line_height`, surface height C:404-408 | recompute; **key the range, not raw `scroll_y`** — the sub-line remainder is applied at `TextArea.top` (C:537, 544), so pixel scrolls within one line are cache hits |
| 3 | Content (wrap) width = surface width − gutter width − padding | C:425-428 | recompute per frame; folds in **surface width**, **`gutter_enabled`** (C:771-773), **`custom_gutter_lines`** (C:776-786), **`cached_char_width`**, and **digit rollover** (999→1000 lines widens the gutter, C:777 → gutter.rs `calculate_width`) — all reduce to one f32; key its bits |
| 4 | Font metrics (size × line-height multiplier) | baked into the buffer at creation, T:249-250; `set_font_size` C:1555-1557 | recompute from config per frame (`line_height()` T:175-177); no other setter is exposed on the compositor today — `set_line_height`/`set_font_family`/`apply_typography` exist on `TextRenderer` (T:541-578) but no face can reach them; keying metrics anyway makes that future-proof |
| 5 | Font data | `load_font` C:1543-1547 mutates the fontdb; `Family::Monospace` resolution can change | **none — add a font generation counter bumped by `load_font`** |
| 6 | Theme (foreground into text attrs C:801, 538; fallback highlighter palette) | `set_dark_theme` C:1584-1592 | **none — add a theme generation counter** |
| 7 | `syntax_enabled` | three-way branch C:802-828 (four-way since the no-language ruling: a resolve-`None` splits on `HighlightSource::language_active` into the keyword bridge — language set — or plain foreground — no language) | the bool itself; key member. Its companion `language_active` (the face's answer) is a key member too, so a language set or unset at runtime invalidates without any generation moving |
| 8 | Highlight spans (the resolver's answer) | `highlights.resolve` C:812; desktop spans gated on `{parses, revision}` H:79-115; web spans replaced wholesale at W:1687 (`setTreeSitterHighlights`), legacy at W:1691, cleared at W:1737, and viewport-scoped in the resolver itself W:2799-2816 | **none at the seam — the `HighlightSource` trait must carry a generation** (§3.1). The resolver's own viewport dependence is subsumed by row 2. Desktop subtlety: `HighlightCache::refresh` clears `entry` **without** bumping `rebuilds` when the language is removed (H:108-110), so `rebuilds` alone is not a sufficient generation — presence/language must fold in |
| 9 | `syntax_theme` capture-name map | C:809; mutated through the **raw `&mut` accessor** C:1626-1628, used by the web face at W:1709-1726 | **none — the raw accessor defeats change tracking** (§6, R3) |
| 10 | Fold state (hidden lines skipped C:716-718; fold placeholder text appended C:731-744) | `fold_state` parameter | **none — `FoldState` has no generation counter** (F:62-72; every mutator ends in `rebuild_line_mapping`, F:270, 280, 317, 335, 356, 422, none bumps anything observable). Folding does **not** touch document revision — this is the sharpest staleness trap in the whole set: fold a region, revision unchanged, retained buffer shows the unfolded text |
| 11 | Wrap/shaping constants: `Wrap::WordOrGlyph` (cosmic default, never set), tab width (default, never set), `Shaping::Advanced` (T:267, 293) | fixed today | not key members; becomes one the day `docs/SOFT-WRAP-DESIGN.md` lands `Wrap::None` — note left for that change |

Inputs that are **not** buffer inputs and deliberately stay out of the
key (they feed quads or separate buffers that stage 1 keeps rebuilding
every frame, so they cannot go stale): cursor/selection positions
(C:459-498, 1065-1225 — read the buffer, never shape it), blink phase
(C:501, 1200), `line_backgrounds`/`gutter_changes` (C:940-1054),
`blame_data` (C:558-589 — a tiny per-frame buffer; retaining it buys
nothing). `custom_gutter_lines` is the one presentation input that IS a
buffer input twice over: it changes the gutter *width* (row 3, main
buffer) and the gutter *text* (the retained gutter buffer, §3.1).

## 3. The design, staged

### 3.1 Stage 1 — whole-buffer retention

`FrameCompositor` grows one field:

```rust
/// The shaped viewport buffer from the last frame, with the exact inputs
/// it was shaped under. `None` before the first frame.
retained: Option<RetainedShape>,

struct RetainedShape {
    key: ShapeKey,
    buffer: Buffer,          // main content, shaped
    gutter: Option<Buffer>,  // line numbers, shaped (None when gutter off)
}

/// Every input from §2, as a comparable value. f32 inputs are keyed by
/// bit pattern — bit-equality is the honest cache question ("did the
/// input change"), not approximate equality.
#[derive(PartialEq, Eq, Clone)]
struct ShapeKey {
    document_revision: u64,      // §2 row 1
    viewport_start: usize,       // §2 row 2
    viewport_end: usize,
    content_width: u32,          // f32::to_bits — subsumes §2 row 3 entirely
    font_size: u32,              // to_bits, §2 row 4
    line_height_factor: u32,     // to_bits
    font_generation: u64,        // §2 row 5 — new counter, bumped in load_font
    theme_generation: u64,       // §2 row 6 — new counter, bumped in set_dark_theme
    syntax_enabled: bool,        // §2 row 7
    language_active: bool,       // §2 row 7 companion — HighlightSource::language_active
    highlight_generation: u64,   // §2 row 8 — face-supplied, see below
    syntax_theme_generation: u64,// §2 row 9 — bumped when the map is mutably borrowed
    fold_generation: u64,        // §2 row 10 — new counter on FoldState
    gutter_text_generation: u64, // custom_gutter_lines identity + gutter_enabled,
                                 // bumped in set_custom_gutter_lines/set_gutter_enabled
}
```

The gutter buffer rides the same key: `cpu_line_numbers` is a pure
function of the visible lines, the wrap counts (both derived from the
main buffer) and the custom gutter text (C:868-934), so main-key hit +
`gutter_text_generation` unchanged ⇒ the gutter buffer is current, no
comparison needed. Its shaping is the same cost class as the content's
(a full viewport of `Shaping::Advanced` digit strings, C:446-455) —
leaving it per-frame would forfeit a real slice of the win.

**The highlight generation seam.** `compose` may only skip `resolve`
when the answer is known unchanged, and only the face knows that. Extend
the trait (C:135-139):

```rust
pub trait HighlightSource {
    fn resolve<'a>(&mut self, context: &HighlightContext<'a>) -> Option<Vec<(&'a str, Color)>>;
    /// Moves whenever `resolve`'s answer could differ for an identical
    /// context. A face that mutates its spans without moving this shows
    /// stale colors — that is the contract, stated in the docs.
    fn generation(&self) -> u64;
}
```

Desktop: derive from `HighlightCache` — but not `rebuilds` alone; the
language-removed path clears spans without bumping it (H:108-110). An
honest desktop generation is `(rebuilds, entry.is_some(), language)`
hashed or carried as a small struct the cache exposes. Web: one `u64` on
`WebEditor`, bumped at every span/state mutation site — W:1687
(`setTreeSitterHighlights`), W:1691 (`use_ts_highlights = true`), W:1737
(clear), and the legacy `ts_highlights` setter — carried into
`WebHighlightSource` (W:2777-2792). The bench's `NoHighlights` returns a
constant (B:85-91).

**What compose's control flow becomes:**

1. Sync uniforms (unchanged, C:395).
2. Compute the viewport range, gutter width, content width — cheap,
   always (C:401-428 minus the buffer).
3. Build `ShapeKey`. On **miss**: run today's path exactly —
   `build_visible_content`, create/fill/shape both buffers, wrap
   readback, line numbers — and store the result as `retained`. On
   **hit**: skip all of it; `cpu_visible_content`, `cpu_visible_doc_lines`,
   `cpu_doc_to_visual`, the visual line map, `cached_total_visual_lines`
   and `cpu_line_numbers` are all deterministic functions of the same key
   and already hold last frame's (= this frame's) values.
4. Always, hit or miss: cursor walk (reads the buffer, C:459-498), blink
   update, all quad passes, blame buffer, text areas, `prepare`, render
   pass, `trim_cache`. These are what a frame legitimately owes.

Steady-state cost after stage 1: cursor walk + quads + prepare + pass.
The sparse-viewport live number (~2.7ms p50, DESKTOP-SHELL-PLAN:111)
bounds everything but dense-viewport `prepare`; if prepare on ~90 dense
lines turns out to carry real weight, that is a measured follow-on (§4,
glyphon row) — stage 1 deliberately does not touch it.

Two implementation notes, so the estimate is honest:

- **Borrow shape.** Today `buffer` is a local passed `&mut` into
  `&mut self` methods (C:429-443). Retained, it lives in `self`; the
  clean mechanical fix is `let mut retained = self.retained.take()` at
  the top of the rebuild arm and put-back after, keeping every helper's
  signature. No API change leaks.
- **`build_visible_content`'s O(document) walk** (C:705-712, 752-759)
  is skipped on hits along with everything else — on misses it stays;
  it is tens of microseconds of pushes, not part of the 30ms, and
  narrowing it is not this change.

### 3.2 Stage 2 — assessed honestly: needed, and the API permits it

**Why it is needed.** Stage 1's miss path is today's full path. A
keystroke moves `document_revision` (and, with a language set, the parse
generation → `highlight_generation`), so **every typing frame is a
miss**: after_mid_file_edit stays ~34.4ms, after_scroll_change stays
~30.9ms, and the live typing number on a dense viewport stays ~41ms —
5× over budget. The entire point of the desktop track is typing latency
(DESKTOP-SHELL-PLAN:89). Stage 1 alone does not deliver the track's
promise; it delivers blink, overlay, mouse and sub-line-scroll frames.

**What cosmic-text actually permits** (verified in the vendored source of
both versions; the live one is 0.15 via glyphon — see the preamble):

- `Buffer::set_rich_text` calls `BufferLine::reset_new` on **every**
  line it writes (ct16 buffer.rs:807, 857, 869; ct15:791, 841, 853), and
  `reset_new` unconditionally marks the shape and layout caches unused
  (ct16 buffer_line.rs:48-63, identical in ct15). It also resets the
  buffer's internal scroll (ct16:883, ct15:716). **Retention behind
  `set_rich_text` is impossible; "retained Buffer + call `set_rich_text`
  again" reshapes everything.**
- `BufferLine::set_text(text, ending, attrs_list) -> bool` **diffs**
  text, line ending and attrs list, and resets shape+layout **only when
  they differ** (ct16 buffer_line.rs:74-91; ct15:73-90, identical).
  `set_attrs_list` diffs likewise (ct16:126-134).
- `Buffer.lines` is `pub Vec<BufferLine>` (ct16 buffer.rs:209; ct15:212),
  and glyphon re-exports `BufferLine`, `AttrsList` and `cosmic_text`
  itself (glyphon lib.rs:22-30), so `LineEnding` is reachable. A
  `BufferLine` carries its caches by value (`shape_opt`, `layout_opt`,
  ct16 buffer_line.rs:17-18) — **moving one within the Vec preserves its
  shaping**.
- `shape_until_scroll` → `line_layout` → `BufferLine::layout` consults
  the per-line cache first (ct16 buffer_line.rs:249, buffer.rs:534-549);
  with warm lines it is a walk, not a reshape.
- Width-only changes go through `set_size` → `relayout`, which resets
  **layout only** and reuses each line's `ShapeLine` (ct16
  buffer.rs:289-311, 641-676; ct15 identical) — resize re-wraps without
  re-shaping, a free structural win the moment the buffer is retained.

**The middle path is therefore the real stage 2** — not a new cache, but
feeding the retained buffer through the diffing API:

- **2a — per-line diffing (fixes typing).** On a content/highlight miss
  with the viewport range unchanged: build the visible content and
  resolve runs as today; split the flat run list at line boundaries into
  one `AttrsList` per line (defaults = `Attrs::new().family(Family::Monospace)`,
  spans added only where they differ from defaults — exactly what
  `set_rich_text` produces, ct16 buffer.rs:826, so output is identical
  by construction; every line gets `LineEnding::default()` = `Lf`, ct16
  line_ending.rs:4-8, matching ct16 buffer.rs:787); then call
  `lines[i].set_text(...)` per visible line and truncate/extend the Vec.
  A one-character keystroke reshapes **one line** (plus any lines whose
  highlight runs genuinely changed — an unterminated string recoloring
  fifty lines reshapes fifty lines, which is correct, not a bug). The
  per-line compare cost is a string compare plus an `AttrsList` build
  and compare over ~90 lines — micro-, not milliseconds.
- **2b — window rotation (fixes scroll).** Buffer line *i* is doc line
  `viewport_start + i`, so a one-line scroll shifts every line's text
  and naive diffing misses all 90. Because `BufferLine`s move with their
  caches, the fix is to treat `buffer.lines` as a rotating window keyed
  by doc line: on a range shift, drain departed lines, `rotate`, and
  `set_text` only the entrants. A one-line scroll then reshapes ~1 line;
  the bench's full-screen hop (B:271-281) still reshapes about half the
  window — proportional, honest. Fold-state changes invalidate the whole
  window (the doc-line ↔ buffer-line correspondence itself moved);
  that is the `fold_generation` doing its job.

**Pricing.** 2a is the smaller half: the run-splitting walk plus the
per-line set loop, a few dozen lines in `fill_content_buffer`'s
replacement, and it converts the 34.4ms edit frame into approximately a
steady-state frame plus one line's shape — this is the piece that moves
the 41ms headline. 2b adds the window bookkeeping (rotation, doc-line
keys, fold interaction) — the fiddliest code in the change, worth doing
only if scroll-frame numbers still matter after 2a is measured
(scrolling at ~30ms is 33fps jank, visible but not the track's headline;
typing at 41ms is the finding). Recommended sequencing in §6 R4.

**Alternative priced and declined — cosmic-text's `shape-run-cache`
feature** (ct16 Cargo.toml:51, shape.rs:58-71, 414-471; same in 0.15):
a FontSystem-level cache of shape runs keyed by (attrs, text) that would
cheapen full rebuilds without compositor changes. Declined because (a)
the flag would have to be set on **glyphon's** cosmic-text 0.15 — the
workspace's direct 0.16 dep is a different crate to Cargo and does not
unify features, so we would be adding a second direct dependency on a
version we otherwise don't use; (b) layout (wrap) still rebuilds for
every line every frame; (c) it is a constant-factor mitigation on the
wrong path — steady-state frames should cost zero shaping, not cheaper
shaping.

## 4. Interaction risks

| Risk | Basis | Resolution |
|---|---|---|
| **Highlight span lifetimes** — resolve returns `Vec<(&'a str, Color)>` borrowed from `cpu_visible_content` (C:138, 812; H:179-183) | `set_rich_text`/`set_text` **copy** the text into the buffer's own per-line `String`s (ct16 buffer.rs:764-877, buffer_line.rs:80-85) — the retained `Buffer` owns its text; no borrow survives compose | Unchanged on miss frames. On hit frames `resolve` is **never called** — which is exactly why the `generation()` contract exists: a face mutating spans silently shows stale color. Stage 2a splits the same borrowed runs per line inside the same scope — no lifetime change |
| **Cursor/selection/caret quads** | Read the buffer read-only (`cursor_position_in_buffer` T:319-378, quad passes C:1065-1225) and rebuild every frame; blink gate C:1200-1201 | Already independent of shaping. Caret motion, drags and blink work against a retained buffer with zero design change |
| **Wrap readback + hit-testing coherence** | `cached_visual_line_map` / `cpu_visible_doc_lines` / `cached_map_viewport_start` / `cached_content_offset_x` are written only in the compose rebuild path (C:843-864) and read between frames by `pixel_to_position` / `position_to_pixel` / `max_scroll_y` (C:1287-1474) | They are derived from exactly the `ShapeKey` inputs, so key-hit ⇒ they already hold the correct values; key-miss ⇒ rebuilt with the buffer. Coherence is the cache invariant itself. No site outside compose mutates them (verified by read of all writes) |
| **glyphon `prepare`** | Takes fresh `TextArea` borrows of `&Buffer` per call (T:433-452, C:595-630); nothing requires a newly created `Buffer` | Retained buffer is handed in unchanged. Stage 1 keeps `prepare` and `trim_cache` per frame: `trim` evicts atlas entries not referenced by recent prepares, so skipping `prepare` while trimming risks evicting live glyphs — that coupling is not verified in glyphon internals and does not belong in this change |
| **Buffer-internal scroll** | `set_rich_text` resets it (ct16:883); the compositor never sets it and virtualizes externally (C:404-413, 537) | Per-line `set_text` never touches it; the invariant "buffer scroll is default" holds in every path. Assert it in debug if paranoid |
| **The web face** | Drives the identical `compose` (W:1592-1604); its blink ticker is a 500ms `forceRender` | Nothing native-only anywhere in the design — retention lives in `FrameCompositor`, kernel-side, `web_time::Instant` already in use (C:42). The web face owes exactly one thing: the generation bumps at W:1687, 1691, 1709-1726, 1737. The blink ticker becomes the single biggest beneficiary — an idle web editor stops paying 35ms twice a second |
| **Two theme sources on desktop** | The desktop resolver colors from `editor.state().theme.syntax` (A:1104); the compositor's own theme colors everything else (C:195, 1584) | Today the desktop sets theme at startup only (`--theme` flag parity, DESKTOP-SHELL-PLAN:141-142) and never calls `set_dark_theme`. If runtime theme switching lands, the resolver side must move `highlight_generation` — record that in the trait's contract docs now |
| **`IRIDIUM_LATENCY` / the bench measure the same thing** | The live ruler spans winit receipt → present (latency.rs module docs, A:1144-1151) — untouched. The bench times whole `compose` + GPU wait (B:184-204) — untouched | Same rulers, before and after. One honest caveat: after stage 1, the bench's `steady_state` case measures the **hit** path where it used to measure the rebuild — same scenario, new meaning; record both numbers in the plan doc, and see §5 on a cold case |
| **`docs/SOFT-WRAP-DESIGN.md`** | Plans to delete the wrap readback and set `Wrap::None` | Retention is behind the compositor's existing accessor interface (C:28-36), so the wrap model swap remains possible; but stage 2b's doc-line-keyed window is exactly the shape a kernel `RowIndex` wants to feed. Do not build 2b in a way that hard-codes cosmic-text's run readback deeper than it already is |

## 5. The proof plan

**What pins correctness today.** The web pixel oracle
(`COMPOSITOR-EXTRACTION-MAP.md`, OUTCOME block — byte-identical canvas
protocol, re-runnable); the desktop highlight cache's generation tests
(H:322-347, the `rebuilds`-observable pattern); unit tests in the render
modules (cursor/gutter/highlight/units/text) — but **`compose` itself has
no direct test in-tree** (`crates/iridium-editor/tests/` holds only
`host_extension_surface.rs`). The compose bench is a ruler, not an
oracle.

**New tests the change needs:**

1. **The rebuild observable.** A `shape_rebuilds() -> u64` counter on
   `FrameCompositor` (the exact `HighlightCache::rebuilds` pattern,
   H:53-56, 149-151), bumped once per miss-path rebuild; stage 2 adds a
   `lines_reshaped() -> u64`. These make every cache claim testable
   without reading pixels.
2. **The staleness matrix** (headless GPU harness lifted from the bench,
   B:108-157; a missing adapter fails loudly, B:78-81, never silently
   passes): for each §2 row, mutate the input, compose, assert **miss**
   and assert the observable output actually changed where visible —
   edit, scroll ≥1 line, resize, `load_font`, `set_font_size`, theme
   flip, `set_syntax_enabled`, highlight-generation bump,
   **fold toggle** (the revision-silent trap, §2 row 10),
   `set_custom_gutter_lines`, gutter toggle, and the 999→1000 digit
   rollover. And the hit side: identical compose, sub-line scroll, blink
   frame, presentation-input mutation (`line_backgrounds` etc.) each
   assert **hit**.
3. **Pixel identity, hot vs cold.** The bench's offscreen texture
   already has `COPY_SRC` (B:146): copy the target to a mapped buffer
   and compare bytes. Determinism requirements: the face font loaded
   from bytes (B:65), fixed metrics, and blink pinned by calling
   `reset_blink()` immediately before each compared compose (blink is
   the one time-dependent input, C:501). Assert: (a) frame 2 (hit)
   byte-identical to frame 1 (cold); (b) for each staleness-matrix
   mutation, the warm-after-invalidation frame byte-identical to a
   fresh compositor composing the same state cold. This is the
   pixel-identity argument: the cache may only change *when* work
   happens, never *what* is produced.
4. **Stage 2 precision.** One-char edit ⇒ `lines_reshaped == 1` (fallback
   highlighter; with spans, == the count of recolored lines); one-line
   scroll under 2b ⇒ entrants only; fold toggle ⇒ full window.
5. **Face wiring.** Web generation bumps covered by bindings-side review
   plus the existing TS harness; desktop generation covered by extending
   H's tests with the language-removal case (the `entry = None` path
   H:108-110 must move the exposed generation).

**Expected bench deltas** (`cargo bench -p iridium-editor --bench
compose_frame`, same 10k-line document):

| Case | Today | After stage 1 | After stage 2a | After 2b |
|---|---|---|---|---|
| `steady_state` (becomes warm/hit) | 35.2ms | **collapses** — target: quads+prepare+pass, expect low single digits | same | same |
| `after_mid_file_edit` | 34.4ms | ~unchanged (**stated consequence: typing not fixed**) | **collapses** to ≈ hit + 1 line | same |
| `after_scroll_change` (full-screen hop) | 30.9ms | ~unchanged | ~unchanged | ≈ half-window reshape |
| `after_small_scroll` (new, one-line) | — | add it: miss, ~30ms | ~30ms | **collapses** |
| `first_frame` / cold (new) | ≡ steady_state today | add it: preserves the historical ~35ms meaning so the before/after comparison stays honest | — | — |

Live ruler acceptance: 10k-line dense-viewport typing p50 sub-8ms after
stage 2a, measured by the same `IRIDIUM_LATENCY` protocol and recorded
next to the 3 Aug numbers in `DESKTOP-SHELL-PLAN.md`.

## 6. Open rulings for the controlling seat

- **R1 — Highlight generation seam shape.** Trait method
  `HighlightSource::generation()` (recommended: the face owns the
  counter, `compose`'s signature is untouched, the bench's null source
  returns a constant) vs. an extra `compose` parameter. Recommend the
  trait method.
- **R2 — Fold invalidation.** Add a `generation: u64` to `FoldState`
  bumped by every mutator (recommended — O(1), kernel change of a dozen
  lines, and the counter is independently useful) vs. hashing
  `folded_lines` per frame in the key. Recommend the counter.
- **R3 — The raw `&mut` presentation accessors.** `syntax_theme_mut`
  (C:1626-1628) defeats change tracking. Recommend: bump
  `syntax_theme_generation` inside the accessor on every mutable borrow
  (over-invalidates only when the host actually calls it, which
  coincides with real changes); leave `line_backgrounds_mut` /
  `gutter_changes_mut` / `blame_data_mut` untracked — they feed
  per-frame quads/blame and cannot go stale.
- **R4 — Stage sequencing.** Recommend: land stage 1 + the counters +
  the test harness, measure, then 2a in the same track (typing is the
  finding; stage 1 alone does not answer it), then decide 2b on the
  measured scroll numbers. Each stage gates on the staleness matrix and
  pixel identity before the next.
- **R5 — Bench vocabulary.** `steady_state` silently changes meaning
  (rebuild → hit). Recommend keeping the name for the scenario, adding
  `first_frame` (cold) to preserve the historical number's meaning, and
  adding `after_small_scroll` so 2b has a case that shows its actual
  win. The plan doc records old and new side by side.
- **R6 — How stage 2a writes lines.** Through `buffer.lines[i].set_text`
  directly from the compositor (recommended — the diffing lives in
  cosmic-text where it is tested) vs. a `TextRenderer` wrapper method
  preserving the current "all buffer mutation goes through
  `TextRenderer`" discipline (T:262-305). Recommend the wrapper method
  with the same semantics — one line of indirection keeps the
  `FontSystem` borrow discipline in one file.
- **R7 — The dormant direct `cosmic-text 0.16` dependency.** Unused in
  `iridium-editor/src` (zero references) while the live Buffer is
  glyphon's 0.15 — a version-drift trap for exactly the kind of API
  verification this map did. Not this change's business to remove;
  recommend a separate one-commit cleanup, noted here so it does not
  ambush the implementer with two `Buffer` types.

## RULINGS — controlling seat, 4 Aug 2026

All seven as recommended, binding on the implementation:

- **R1**: `HighlightSource::generation()` trait method. The face owns the
  counter; the contract (documented on the trait): the value MUST change
  whenever a subsequent `resolve` could return different runs for the
  same content — including span-clearing paths, per the desktop
  `highlight.rs:108-110` finding.
- **R2**: `FoldState` gains a `generation: u64` bumped by every mutator.
- **R3**: `syntax_theme_mut` bumps its generation on every mutable
  borrow; `line_backgrounds_mut`/`gutter_changes_mut`/`blame_data_mut`
  stay untracked (per-frame consumers, cannot go stale).
- **R4**: sequencing is stage 1 + counters + proof harness → measure →
  stage 2a → measure. **2b is NOT in this track** — it is decided on 2a's
  measured scroll numbers, by a later ruling.
- **R5**: `steady_state` keeps its name (scenario-stable); `first_frame`
  (cold) and `after_small_scroll` cases added; the plan doc records old
  and new side by side.
- **R6**: stage 2a writes through a `TextRenderer` wrapper method —
  buffer mutation discipline stays in one file.
- **R7**: the dormant direct cosmic-text 0.16 dependency is removed as
  its own separate commit at the end of the track, not mixed into the
  retention change.
