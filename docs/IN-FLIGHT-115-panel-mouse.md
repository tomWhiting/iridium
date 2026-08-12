# #115 — the mouse does nothing in any panel but the context menu

Tom, 12 Aug 2026, after the sidebar landed:

> "I'm just noticing in the desktop I can't select anything or navigate really
> any of the menus — so the sidebar, or any of those things — with the mouse.
> Either clicking or scrolling."

Reproduced by reading the two entry points, and it is exactly as he describes.

---

## The ground, measured

⭐ **The hit test already exists and is already correct.** This is a wiring job
on prepared ground, the same shape #112c step 5 turned out to be.

| seam | where | state |
| --- | --- | --- |
| `PanelGeometry::row_at(x, y, rows) -> Option<usize>` | `apps/iridium-desktop/src/overlay/geometry.rs` | **exists, tested** (`overlay/tests.rs:226-240` covers the row, the padding above, the padding below and off the left edge). |
| the one caller | `apps/iridium-desktop/src/app/menu.rs:72` and `:102` | the **context menu** — hover highlight and click-to-run. So one panel out of four is mouse-driven, and it proves the mechanism works. |
| every other panel | — | ⛔ nothing calls `row_at`. The palette, the undo tree and the file explorer are keyboard-only. |

### Why a click does nothing rather than falling through

`app/pointer.rs`, `dismiss_modal_panel`: when the pointer is on a panel it
returns `true` — which means *handled* — and the press is **swallowed**. That
was #29's fix and it is right as far as it goes: a click on a panel must not
reach the document underneath. What was never built is the other half — a click
on a panel reaching the **panel**.

⚠️ **This is the same shape as the three defects #112c found.** `true` from
`dismiss_modal_panel` means "the document must not see this", and it was read as
"nothing else needs to see it either". One value standing for two sentences,
complete-looking because nothing had asked it the second question yet.

### Why the wheel does nothing

`app/pointer.rs::wheel` converts the delta and calls `self.scroll_by(delta_y)`
unconditionally — the document's scroll, with no test for what is under the
pointer. So scrolling over a full-height sidebar scrolls the *document* behind
it.

---

## What has to be built

1. **A pointer→panel resolution that names which panel**, not just *whether* one
   is under the pointer. `pointer_is_on_a_panel` answers a boolean today;
   the click and the wheel both need the identity and the geometry.
   ⚠️ It must agree with what was **painted**, not with what is open — the
   explorer's fit differs between placements, and `paint.rs` is where the two
   are already reconciled.
2. **Click on a row** → the panel's own selection verb. For the explorer this is
   what `Enter` does: a file opens, a folder toggles. Reusing the key path
   rather than adding a second one is the R1 argument at method scale.
3. **Wheel over a panel** → that panel's list scrolls, and the document does
   not. The explorer's `scroll` field and `follow_selection` already exist;
   what is missing is a way to move the window without moving the selection.
4. **Hover highlight** for the other three, since the context menu has it and a
   panel that highlights nothing under the pointer reads as dead.

## Ordering against #112d

⚠️ **This lands before the explorer hoist (#112d step 2c).** Tom is using the
sidebar now and cannot click in it; the hoist is invisible to him. Doing the
mouse first also means the click path moves into `iridium-panel` **already
built**, rather than being written in the desktop and then ported — which is the
duplication R1 exists to prevent.

His words on the hoist, same message, which settle R1 as ratified rather than
merely ruled: *"absolutely definitely all need to share the same core, so
definitely don't want any duplicates."*

---

## What landed — 12 Aug 2026, `e18a63a4` (pushed)

Ten gates green, read from `ci.sh`'s own `✅ all 10 gates passed`.

### The four pieces, all built

| # | Piece | Where |
| --- | --- | --- |
| 1 | pointer → **which** panel | `overlay/frame.rs` — `PaintedFrame`/`PaintedPanel`/`PanelHit`, `hit()` reads **topmost first** |
| 2 | click on a row → the panel's own verb | `app/panel_mouse.rs` + `click_row` on explorer, palette, history, search |
| 3 | wheel over a panel → that panel's list | `panel_wheel`, converted against the panel's **painted** row pitch |
| 4 | hover highlight | `PanelContent.hovered` + `hover_color` at `HOVER_STRENGTH` = 0.4 of the theme's selection |

### The load-bearing move

⭐ **The painted record left the GPU painter for the app.** `OverlayPainter`
cannot be built without a device, so every question a pointer asks of the
screen could previously only be answered by looking at one. `paint()` now fills
a `&mut PaintedFrame` — an out-parameter rather than a return value, because the
record must survive the error path — and the app adopts it **only on `Ok`**,
since a frame that failed leaves the *previous* one on screen and that is the
one a press must still resolve against.

The record carries the **drawn row count** per panel. `row_at` cannot tell the
last row from the padding without it, and a caller supplying its own count would
answer for the panel as it is *now* — a row off every time a directory listing
lands between the frame and the click.

### ⭐ Law 9: a window follows the selection when the selection *moves*, not when a frame happens

`follow_selection` re-centred on the selection every composition, in three
places (explorer, palette, and the kernel's `TreeViewSelection`). Correct for as
long as the keyboard was the only thing that could move either — and the moment
the wheel moves the window and leaves the selection alone, the very next frame
undoes it. The list springs back under the pointer.

Each now keeps `followed` and compares. **The same shape as the scroll bounce:
a consumer feeding its own answer back in, presenting as motion with no input.**
Three separate implementations rather than one hoisted type, deliberately: the
kernel's keys on a *node id* and cannot reach `iridium-panel` without a cycle,
so they are not the same type wearing two names.

### Ordering ruling that held

`panel_press` runs **before** `dismiss_modal_panel` in `pointer_pressed`. That
made the dismissal step unconditional — every path through it now answers "the
user pointed somewhere else" — and let the two `pointer_is_on_a_panel` guards go.

### Discrimination

7 of 11 new app tests failed against the unfixed code before any fix was
written. ⚠️ **The wheel test uses `PixelDelta`, not `LineDelta`, and that is
what makes it able to fail**: a line delta is scaled by the compositor's line
height, and a windowless test session has no compositor — so it would have
resolved to zero and "the document did not move" would have been true whatever
the routing did. Proven discriminating by disabling the panel branch and
watching it fail. `wheel` also stopped early-returning without a shell; it now
takes `map_or(0.0, …)` for the line height, which is the honest answer to "how
many rows is that" before a font has measured and leaves pixel deltas working.

### Still owed on #115

- ⛔ **Not installed.** `bundle/install.sh` (with **bash**) refuses while
  `iridium-desktop` runs, by design. Needs Tom to quit it. Then the `strings`
  receipt: `panel_mouse` is not a symbol, so grep the binary for
  `explorer.toggleSidebar` = 2 as the freshness control plus something new —
  suggest counting the hover band's absence/presence is not greppable, so use
  `git rev-parse` against the installed build's recorded sha instead.
- The **hover band has been looked at.** `chrome-hover.png` — the palette with
  a row hovered *and* a different row selected, so the two bands are in one
  frame. **Verdict: it works.** The band is clearly visible against the panel
  background and reads plainly as weaker than the selection rather than as a
  second one; its ends carry the same arc as the selection band. The hovered
  row is found in the composed content rather than written in as a literal,
  because an index past the end or on a separator is silently skipped by the
  painter — a hard-coded row would have produced a frame with no band on it and
  nothing would have said so.
- The **popover click path is covered.** Two tests, stated as a pair:
  `a_press_on_a_popover_row_opens_the_file_and_takes_the_popover_away` and
  `a_press_on_a_sidebar_row_leaves_the_sidebar_standing`. One press verb, one
  outcome, two placements — and the only thing that reads the placement is
  `leave_explorer`. Proven discriminating: making `leave_explorer` treat a
  popover like a sidebar fails the popover test and leaves the sidebar test
  green.

### The harness was 316 lines over the bar before this tick, and it got worse

`chrome_screenshots.rs` stood at 1,232 against a 1,000-line hard limit, and the
hover shot took it to 1,316. Split along the seam between *how a frame is made*
and *what a frame is*, following the layout `iridium-editor/tests/retained_shaping`
already uses — a `tests/<name>/main.rs` target root with siblings:

| file | lines | holds |
| --- | --- | --- |
| `chrome_screenshots/main.rs` | 756 | the headless device, the compositor, the readback, the PNG encoder and its oracle |
| `chrome_screenshots/shots.rs` | 489 | one function per frame, plus `run()` |
| `chrome_screenshots/encoder.rs` | 120 | the two encoder tests that need no adapter |

⭐ **The split needed no visibility changes to the infrastructure**, because a
child module can see its parent's private items. Only `run` had to be named
outward, and clippy required `pub` rather than `pub(crate)` inside a private
module.

`MAX_RUN_BYTES` re-measured with the hover frame in the set: **9,340,285 bytes
over 25 frames**, `chrome-hover.png` itself 355,292. Ceiling raised 9,500,000 →
10,200,000, still far under the ~11.3 MB `Adaptive` would produce over the same
25. The doc's older 8,734,042 figure is now explicitly labelled as belonging to
the set that preceded the hover frame — a measured number keeps the set it was
measured on.

### Receipts, 12 Aug 2026

- `bash scripts/ci.sh` → exit 0, `✅ all 10 gates passed`.
- Desktop lib tests: **553 passed, 0 failed**.
- The GPU harness run end to end after the split: `test result: ok. 1 passed`,
  25 frames written.
