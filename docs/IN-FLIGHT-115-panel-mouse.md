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
