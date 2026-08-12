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
- The **hover band has never been looked at**. It is arithmetic-tested (opaque,
  distinct from the panel background and from the selection, in both presets)
  and has no screenshot. Worth one in `chrome_screenshots.rs` next tick.
- The **popover** explorer's click path is covered only by the sidebar tests
  reaching the same `click_row`; a popover-specific test (click opens a file
  *and* closes the popover) is not written.
