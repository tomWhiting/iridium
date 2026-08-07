# #35 — HiDPI: font scale never re-applied when a window moves between displays

**Ground verified 8 Aug against the tree at `5af7aad`. Two of the three things
this item could have meant are already correct; the third is real and has a
latent kernel defect sitting behind it.**

---

## What the backlog item says, and what it turns out to be

The title is the whole description — there was never a write-up. So the first
job was to find out which face it names. Both were read rather than assumed.

| face | re-applies the scale? | verdict |
| --- | --- | --- |
| desktop (winit) | **yes** | ✅ already correct, and correct for a good reason |
| web (browser) | **no** — there is no path that can | ⛔ **this is #35** |

### The desktop face is correct — and it is worth saying why

`app/handler.rs:72` routes `WindowEvent::ScaleFactorChanged` to
`app/viewport.rs:203` `rescaled`, which re-does **exactly** the set that
`app/startup.rs:135-154` derives from the scale factor, in the same order:

```
compositor.set_font_size(BASE * scale)
compositor.load_font(FONT)
overlay.set_font(size, FONT)
overlay.set_scale(scale)
```

startup's scale-derived set and `rescaled`'s are the same four calls. Nothing is
set once at startup and left. winit emits `ScaleFactorChanged` when a window
moves to a monitor with a different factor, and the matching `Resized` follows
separately and is handled separately.

⭐ **The `load_font` call in `rescaled` is load-bearing, and its doc comment is
right about why** — I doubted it mid-investigation and the code says otherwise.
See the next section: `set_font_size` alone does not remeasure the character
width, and `load_font` is the only path that does. Remove that one line as a
"redundant reload" and the desktop face acquires the exact bug this item names.

---

## The kernel defect behind it

`FrameCompositor` keeps **its own** `cached_char_width`
(`render/compositor/state.rs:79`, initialised at `:250` to `14.0 * 0.6`,
"Default until a font is loaded"). It is the number every positional answer
reads:

- `compositor/placement.rs:81` — x from column
- `compositor/placement.rs:151` — column from x (the mouse path)
- `compositor/placement.rs:247` — the public `char_width()`
- `compositor/gutter_column.rs:32,54` — gutter width
- `compositor/frame.rs:83` — content width

**Exactly one thing writes it: `load_font` (`compositor/settings.rs:22`).**

`set_font_size` (`settings.rs:41`) forwards to the text renderer and returns.
The text renderer *does* invalidate its own lazy cache (`text.rs:788`) — but the
compositor's copy is a separate field and nothing tells it.

So after a bare `set_font_size(2×)`:

- the line height is right — it is computed as `font_size × multiplier`, not
  cached;
- the character width is the **old** size's measurement.

Every column-to-x and x-to-column answer is then off by the ratio, and the error
**grows with the column**: click near the end of a long line and the caret lands
somewhere else entirely. The gutter is sized wrong by the same factor.

### Why no test catches it today

`tests/retained_shaping/layout.rs:123`
`a_font_size_change_misses_and_recomposes_identically` exercises exactly this
call and passes.

⭐ **Stated as the proxy it is:** "warm-after-`set_font_size` is byte-identical
to cold-after-`set_font_size`" stands in for *"a font size change is applied
correctly"*. **Where they diverge:** both sides of that comparison load the font
at 14 and then set 16, so **both** carry the same stale character width. The
proxy agrees with its target on every input where the staleness is shared — and
the whole failure is that it is shared. A pixel-identity oracle cannot see a
defect that is identical on both comparands.

The new test uses a different oracle: a compositor that loaded its font *at* the
new size, which is correct by construction.

### Why it is latent rather than live

No face calls `set_font_size` after a font is loaded without a `load_font`
immediately behind it — the desktop does both, and the web sets the size during
construction and loads its font afterwards. So the defect is unreachable today
**only because no face can re-apply the scale at all**, which is #35. Fixing
#35 without this walks straight into it.

---

## The web face, precisely

`packages/@iridium/core/src/controller/index.ts` gets the pixel-ratio *reading*
right and the *applying* wrong.

The getter at `:455` is fresh, and its doc comment at `:449-453` already says
the reason out loud:

> Read rather than cached because it changes when the window moves between
> displays.

Four readers go through it and all four are correct: canvas sizing at creation
(`:553`), canvas sizing in the `ResizeObserver` (`:691`), mouse coordinates
(`:721`), viewport height (`:1196`).

**The fifth consumer is not a reader.** The font size is computed once, inside
`createWebEditor` (`crates/iridium-bindings/src/wasm.rs:373-389`), from the
ratio passed at construction, and baked into the compositor. There is **no
export that can change it afterwards** — `wasm.rs` exposes `resize`, `loadFont`
and nothing else in this area.

So the getter's comment is true and the bug survives it: the ratio is re-read
everywhere it is read, and the one place it is *not* re-read is the one place it
was never a read.

### Two failure modes, not one

1. **The `ResizeObserver` fires.** Canvas backing store is resized to the new
   ratio, the font is not. Text renders at the old physical size against a
   canvas that now has a different number of device pixels per CSS pixel — so
   the text is visibly the wrong size, and the caret is wrong with it.
2. **The `ResizeObserver` does not fire at all.** Dragging a window from a 2×
   display to a 1× display usually leaves the CSS box the same size, so the
   observer has nothing to report. Then even the canvas is stale: it keeps a
   backing store sized for 2× while the browser presents it at 1×, and the whole
   frame is scaled by the compositor. **There is no resize event to hang the fix
   on.**

Failure mode 2 is why the fix cannot simply be "also re-apply the font in the
resize handler". The browser has no `devicePixelRatiochange` event; the
supported mechanism is `matchMedia("(resolution: Xdppx)")`, which fires when the
current ratio stops matching and must be **re-armed at the new ratio** each
time.

---

## The change

Three parts, smallest first.

**1. Kernel — `FrameCompositor::set_font_size` remeasures.**
On an accepted size, refresh `cached_char_width` from the text renderer.
`TextRenderer::measure_char_width` already falls back to `font_size * 0.6` when
it cannot shape (`text.rs:291`), so this is honest before a font is loaded as
well as after — which is what the current doc comment's "nothing honest to
measure" was protecting, and it turns out to be protected one layer down.

Not bumping `font_generation`: `ShapeKey` already carries `font_size` in its own
right (`compositor/shape.rs:30`), so the retained-shaping cache is *not* stale
on this path. Checked rather than assumed — this was a suspected second defect
and it is not one.

**2. Bindings — `WebEditor::setPixelRatio(ratio)`.**
Sanitised through the existing `display_scale::sanitize_pixel_ratio`, so the
re-apply path and the construction path cannot disagree about what a usable
ratio is. The 14.0 base and the multiplication move into `display_scale` as a
named function with host tests, because at present the constant is written twice
— `wasm.rs:373` and `startup.rs:41` — and the duplication is only safe while
nobody changes one.

**3. TypeScript — detect the change and call it.**
A re-arming `matchMedia` watcher, plus a ratio check inside the existing
`ResizeObserver` so the two paths converge on the same call. Both routed through
one private method so there is exactly one place that decides what happens when
the ratio moves.

---

## Progress

- [x] Ground verified: desktop correct, web broken, kernel defect named
- [ ] Red test for the kernel defect, proven red
- [ ] Kernel fix
- [ ] `display_scale::scaled_font_size` + host tests
- [ ] `setPixelRatio` export
- [ ] TypeScript watcher
- [ ] Eight-gate battery + the ninth (wasm clippy)
