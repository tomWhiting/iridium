# #112c — the file explorer as a sidebar

Tom, 12 Aug 2026: the oil surface should also work "as a sidebar", and in the
terminal face it should be "quite big", especially "when you open up the
terminal editor on… a directory".

Parts A (taller) and B (hidden files) are landed. This is part C.

---

## The ground, verified rather than assumed

⭐ **The hard half is already built and has never been called.**

| seam | where | state |
| --- | --- | --- |
| `FrameCompositor::set_left_inset(pixels)` | `crates/iridium-editor/src/render/compositor/insets.rs:56` | **exists, unused.** Its own doc names this exact case: *"The face passes the width of whatever chrome it draws there — a sidebar, a file tree — and every horizontal measure follows: the gutter's background and its numbers, the change bars, the content column, and both directions of hit-testing."* |
| the pattern to mirror | `apps/iridium-desktop/src/app/viewport.rs:170`, `sync_top_inset` | `set_top_inset(DOCUMENT_TOP_PADDING + strip)`, re-called on open, resize and scale change |
| where every panel is composed | `apps/iridium-desktop/src/app/paint.rs:185` | `panels.push(explorer.content(theme, fit))` — **the fit is already a parameter** |
| the one fit they share | `overlay.rs:705`, `fit_for` | centred popover: width capped at `PANEL_MAX_COLUMNS`, height `1 - TOP_ANCHOR_FRACTION` of the window |

`grep -rn "set_left_inset" apps/` returns **nothing**. Task #50 audited the X
axis and landed the mechanism; nobody has yet drawn anything in the band it
reserves. So this is a wiring job on prepared ground, not new geometry.

---

## Rulings

Taken here, with reasoning, rather than put to Tom — the revert cost of each is
noted. Only R2 and R3 are taste.

**R1 — a sidebar is a *placement* the same panel takes, not a second panel.**
One `FileExplorer`, one `content()`, two placements. Two panels would be two
implementations of the oil buffer, its keys, its filter and its edit mode —
and the divergence would show up as "renaming works in the popover and not in
the sidebar". `PanelContent` is already placement-independent; this ruling is
also the cheaper build. *Revert cost: n/a, nothing else is defensible.*

**R2 — the sidebar pushes the document, it does not float over it.**
Via `set_left_inset`. That is what the inset exists for and what its
documentation says. A sidebar that overlays hides the code you opened it to
navigate. *Revert cost: one call site.*

**R3 — fixed width in part C; dragging is its own piece.**
A drag handle needs pointer capture, a minimum, a persisted value and a
hit-test band of its own — every one of which is additive on top of a working
fixed-width sidebar, and none of which is needed to answer "is this the right
shape". *Revert cost: none; it is a later addition, not a reversal.*

**R4 — persistence is deliberately out of part C.**
A sidebar that forgets it was open is one nobody keeps open, so this is owed —
but it belongs with the config work (#59/#102/#105), not with the geometry.
Named here so it is not silently dropped.

---

## ⚠️ The trap, and it is the interesting part

**`EXPLORER_MAX_VISIBLE_ROWS = 30` is a property of the *popover*, not of the
explorer.** `overlay.rs`'s own comment already says so — the constant exists to
keep the panel "a popover with a window around it on a large display instead of
silently becoming a full-height column", and notes that the full-height column
"is the **sidebar**, which is a placement rather than a bigger number".

So the ceiling must move with the placement. A sidebar that inherits 30 is a
sidebar that stops drawing two thirds of the way down a tall window and leaves
dead space below it — which is the *same defect* part A just fixed, reintroduced
one layer up.

⭐ **This is why R1 does not make the ceiling a shared constant.** The row
ceiling and the fit are the two things that differ between the placements; the
composition, the keys and the buffer are the things that must not. Getting that
line in the right place is the whole design.

Concretely: `fit_for` produces the popover fit today. The sidebar needs a
second constructor — full window height less padding, width a fixed column
count — and `paint.rs:185` passes *that* fit to the explorer while every other
panel keeps the popover one. `content()` already takes the fit, so nothing in
`compose.rs` changes.

---

## Build order

1. `PanelFit` gains a sidebar constructor beside `fit_for`; the row ceiling
   becomes a field of the fit rather than a module constant read directly by
   `compose.rs`. *(This is the step the trap above is about — do it first, and
   the rest is wiring.)*
2. `paint.rs` passes the sidebar fit to the explorer when the placement is
   sidebar, the popover fit otherwise.
3. `sync_left_inset` in `app/viewport.rs`, mirroring `sync_top_inset`: called
   on open, resize and scale change, and on the placement toggle.
4. The painter draws the explorer's `PanelContent` in the reserved band —
   anchored to the window's left edge and full height, rather than centred.
5. Hit-testing: the band belongs to the panel, so a click in it must not reach
   the document. `set_left_inset`'s doc says both directions of hit-testing
   already follow the inset — **verify that by test rather than by reading it**,
   because it has never had a caller.
6. A command and a chord to switch placement.

Steps 1–2 are pure composition and testable without a window, which is where
the discrimination lives: a test that the sidebar fit fills a tall window and
the popover fit does not.

---

## What this does not cover

The **terminal face**, which is the other half of Tom's sentence. He named
opening the terminal editor *on a directory* as the case that should shape that
design rather than follow it — so it gets its own map, after this one, rather
than being assumed to be the same panel with a different painter.
