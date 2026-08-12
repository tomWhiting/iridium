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

**R5 — a sidebar is not modal, and this was missing from the map.**
Found on contact with `app/keyboard.rs:36`, not by reading: `press` routes
every key to the explorer whenever `self.explorer.is_some()`. A *popover* that
holds key focus until `Escape` is right — it is a thing you open, use and
dismiss. A **sidebar that did the same would make the document unreachable
while it is on screen**, which is the opposite of what a sidebar is for: Tom's
whole ask is a tree that stays open *beside* the code.

So the placement changes the routing as well as the geometry:

| placement | keys | `Escape` |
| --- | --- | --- |
| popover | always the panel's | closes it |
| sidebar, focused | the panel's | **unfocuses, leaves it drawn** |
| sidebar, unfocused | the document's | the document's |

⭐ **Focus is derived, not stored twice.** A popover is focused by definition,
so the routing reads `popover || sidebar_focused` and never consults the flag
for a popover — which is what makes "an unfocused popover", the one state that
would black-hole every key, unrepresentable in effect rather than merely
unlikely.

⚠️ The placement itself lives on `DesktopApp`, **not** on `FileExplorer`, for
the reason already written beside `DesktopApp::project`: closing the explorer
*drops* it, so anything stored on the panel is forgotten on every close. A
sidebar that reverted to a popover each time it was reopened is a sidebar
nobody would use twice. *Revert cost: the field and one branch.*

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

1. ✅ **LANDED `5ae51ba4`.** `PanelFit` gains `max_browse_rows` and two named
   constructors, `popover` and `sidebar`; `sidebar_fit_for` is the counterpart
   of `fit_for`. *(This is the step the trap above is about — do it first, and
   the rest is wiring.)*

   The ceiling is carried on the **fit** rather than on a placement enum the
   explorer reads, and the reason is not tidiness: a panel that took its width
   from one placement and its ceiling from another would be too tall for the
   box it is drawn in and nothing downstream could tell. On the fit the two
   cannot be paired wrongly — including on the narrow-window fallback, where
   the popover fit and the popover ceiling arrive together or not at all.

   Measured on 3024×1964 at 2×: **47** interior rows / **46** browsed, against
   the popover's 30. The mutation that inherits the popover rule fails the
   discriminating test with `left: 30, right: 46`.
2. ✅ `paint.rs` passes the sidebar fit to the explorer when the placement is
   sidebar, the popover fit otherwise. `DesktopApp::explorer_sidebar_fit` is
   the single answer to "is there a sidebar on screen", so the band that is
   *reserved* and the panel that is *drawn* cannot disagree.
3. ✅ `sync_left_inset` in `app/viewport.rs`, mirroring `sync_top_inset`:
   window open, resize, scale change, the explorer opening or closing, and the
   placement toggle. **Zero is a value it writes**, not a case it skips — a
   sidebar that closed while the inset stayed set would leave the document
   indented past an empty band, with every horizontal measure agreeing.
4. ✅ `PanelAnchor::Left { top, interior_rows }` — the one anchor whose height
   is its *band's* rather than its content's, because the reserve is what
   pushes the text and a short panel over a full-height reserve would leave
   the document indented past nothing.
5. ✅ Hit-testing, and it needed no new code. `crates/iridium-editor/tests/
   left_inset.rs` — written for #50, run under `test/workspace` ever since —
   already proves the round trip (`a_click_where_a_column_is_drawn_resolves_to
   _that_column_under_an_inset`), that the inset actually moves the column
   rather than being stored and ignored, and that nothing the document draws
   reaches into the band. **Verified by running it, not by reading it.** What
   had never existed was a *face* that called `set_left_inset`; the mechanism
   was proven all along. On the panel's side, `dismiss_modal_panel` was the
   real work — see step 6.
6. ✅ `explorer.togglePlacement` on `Ctrl+Alt+B` / `⌘B`, **and the focus
   routing of R5**.

   Two defects came out of this step, both found by running rather than
   reading, and both recorded because they are the same shape:

   ⚠️ **`ExplorerOutcome::Closed` was two intents in one value.** The panel
   returned it for `Escape` *and* for its own `⌘⌥E` close chord. The moment
   `Escape` stopped closing a sidebar, `⌘⌥E` stopped closing one too — the
   toggle chord became a silent no-op on the placement that most needed it.
   Split into `Dismissed` (give the document back) and `Closed` (the panel
   goes away), which is what they always meant.

   ⚠️ **`dismiss_modal_panel` would have swallowed every document click**
   while a sidebar was up, and closed the sidebar on a click outside it. A
   sidebar is not one of the modal three; a click in the text has to reach the
   text and take the keys back.

   ⚠️ **The panel consumes every key it is handed**, so `⌘B` never reached the
   host command until the panel named the chord itself — the same reason it
   already names its own close chord.

**LANDED `5ae51ba4` (step 1) and `b947f481` (steps 2–6), 12 Aug 2026.**
Ten gates green. What remains of #112 is the terminal face — see below.

---

## What this does not cover

The **terminal face**, which is the other half of Tom's sentence. He named
opening the terminal editor *on a directory* as the case that should shape that
design rather than follow it — so it gets its own map, after this one, rather
than being assumed to be the same panel with a different painter.
