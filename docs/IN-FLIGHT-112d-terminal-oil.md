# #112d — the oil surface in the terminal face

Tom, 12 Aug 2026, the sentence this whole task came from: the oil surface
should also work "as a sidebar", and in the terminal face it should be "quite
big", especially **"when you open up the terminal editor on… a directory"**.

Parts A (taller), B (hidden files) and C (the desktop sidebar) are landed. This
is part D, and it is the half he named as the case that should *shape* the
design rather than follow it.

---

## The ground, verified rather than assumed

⚠️ **This corrects the handoff.** `docs/IN-FLIGHT-doug-handoff.md` said
"nothing from #112c transfers — the terminal face has no pixel grid, no
`PanelFit`, and no `set_left_inset`". That was written from memory and is wrong
in both directions. Measured today:

| claim in the handoff | what is actually there |
| --- | --- |
| "no panel model" | `crates/iridium-tui/src/frame/panel.rs` — `FloatingBox::fitted(columns, rows)`, rounded `╭ ╮ ╰ ╯` corners, `MAX_WIDTH = 64`, `MIN_WIDTH = 20`, `MAX_VISIBLE_ROWS = 12`, and a documented refusal to draw on a screen too small for an honest panel. The palette and the undo tree already share it. |
| "no reserve mechanism" | `frame/geometry.rs` already states the rule this task needs, on the other axis: *"The panel's rows are taken **from** the document's rather than laid over them, and the kernel's viewport is sized to what is left."* That is #112c's R2, already ruled and already built here — for the search panel, vertically. |
| — | `FrameLayout { gutter_width, text: TextArea, text_rows, viewport, … }` is computed **without drawing**, so what a frame *would* look like is already a query. `TextArea { origin, width, scroll }` — and `scroll` is a **horizontal** scroll in cells, so this face pans long lines instead of wrapping them. |
| "the terminal face is where the oil surface needs to be made bigger" | ⛔ **There is no file explorer in the terminal face at all.** `grep -rln "explorer\|FileExplorer" crates/iridium-tui/src/` returns nothing. |

⭐ **So #112d is a build, not an adjustment.** The task is not "make the
terminal's oil surface taller"; it is "the terminal face has never had one".

And the case Tom named is missing at the front door too: `apps/iridium/src/cli.rs`
takes exactly one path and has no notion of a directory — no `is_dir`, no
project. `iridium <dir>` in a terminal today tries to open the directory as a
file.

### What the desktop's explorer is made of, and how much of it is desktop

`apps/iridium-desktop/src/file_tree/` is **7,453 lines** across 19 modules:
the tree, the filter and its crawl, the browse keys, the oil buffer, its edit
keys, the plan, the confirmation, and `apply`. Its entire dependency on the
desktop is composition:

```text
crate::overlay::{PanelRow, Span, PanelFit, PanelAnchor, PanelCaret, PanelContent, scroll_for}
crate::line::{LineBuilder, skip_chars, highlighted_spans, match_color}
crate::prompt::Entry
crate::project::chosen_root
```

Everything else is `iridium_explorer::{FileTree, NodeId, NodeInfo, EntryKind,
ListingError}` and `iridium_editor::theme`. **Both faces already share the
kernel's `Theme`** — the TUI resolves it once per frame into cell styles in
`frame/palette.rs`, and the desktop's `Span` carries a `theme::Color` directly.

---

## Rulings

Taken here with reasoning and a revert cost, rather than put to Tom.

### R1 — the oil buffer is **hoisted**, never re-implemented

⭐ **This is #112c's R1 one level up, and the argument is the same one.** There
it was "two panels would be two implementations of the oil buffer, its keys, its
filter and its edit mode — and the divergence would show up as *renaming works
in the popover and not in the sidebar*". Here the divergence would read
*renaming works in the desktop and not in the terminal*, which is worse: the two
faces are shipped separately and nothing compiles both compositions at once.

So `file_tree/` moves to a face-independent crate — working name
**`iridium-oil`** — depending only on `iridium-explorer` and
`iridium-editor::theme`. What stays behind in each face is the **painting** and
the **placement**, which is all that was ever face-specific.

*Revert cost: n/a — nothing else is defensible for a surface with an
apply-to-the-filesystem step in it.*

### R1b — ⚠️ the shared vocabulary is **every panel's**, not the explorer's

Measured per file, which is what changed this ruling. Of the 19 modules in
`file_tree/`, **ten import nothing from the desktop at all** — `apply`,
`buffer`, `filter`, `order`, `plan` and their test modules. The whole foreign
surface of the other nine is this, and it is the complete list:

```text
overlay::{Span, PanelRow, PanelCaret, PanelFit, PanelContent, PanelAnchor, scroll_for}
line::{LineBuilder, skip_chars, highlighted_spans, match_color}
prompt::Entry
project::chosen_root
```

⭐ **Every one of those except `chosen_root` is used by the palette, the undo
tree and the search panel too.** They are not the explorer's dependencies; they
are the desktop's *panel-composition vocabulary*, and the explorer is merely the
first panel that has to leave the building with them.

So the crate is **`iridium-panel`**, not `iridium-oil`, with two module trees:
`row` — the vocabulary above — and `explorer` — the oil surface built on it. One
crate rather than two, because the vocabulary has exactly one consumer group and
a second `Cargo.toml` would buy nothing but a second place to bump a version.
The desktop's `overlay` re-exports the vocabulary, so its other three panels do
not churn at all.

*Revert cost: the re-export line.*

### R2 — the seam is **styled rows**, not a trait the faces implement

The crate emits what it already emits: rows of `(text, theme::Color)` spans plus
an optional caret, against a **fit** the caller passes in. That is the desktop's
`PanelRow`/`Span`/`PanelCaret` with the `crate::overlay` prefix removed, and the
TUI already owns the one conversion it needs.

⚠️ **`PanelAnchor` and `PanelFit` do not move.** An anchor is pixels and a
window; the TUI's counterpart is a `FloatingBox` in cells. What crosses is a
*fit* — content width, interior rows, browse ceiling — which #112c already
reduced to three integers for exactly this reason. The generalisation was
accidental and it holds.

*Revert cost: the conversion function in each face.*

### R3 — ⚠️ twelve is wrong here for the same reason thirty was wrong there

`panel::MAX_VISIBLE_ROWS = 12` is a **queried** panel's ceiling. It is right for
the command palette and the undo tree, which you open, type at, and dismiss.
A browsed file tree gets the screen — that is the whole of part A, and the law
it produced was written before this face was looked at:

> **Twelve is right for a panel that is *queried* and wrong for one that is
> *browsed*.**

⭐ The mechanism from #112c carries directly: the browse ceiling rides on the
*fit*, so the screen that composes the rows never learns which kind of panel it
is in. A terminal fit constructed by the browsed path and a floating fit
constructed by the queried path cannot be paired wrongly.

*Revert cost: one constructor.*

### R4 — the sidebar takes columns **from** the document, it does not float over them

The face has already ruled this on the vertical axis and written down why:
floating the search panel would make the kernel scroll matches underneath it.
A file tree floating over the text has the same defect one axis across — it
hides the code you opened it to navigate, and the kernel's `Viewport` would
describe a width the document does not have.

Concretely: `FrameLayout` gains a left band, the gutter's origin moves right by
its width, `TextArea::{origin, width}` follow, and `viewport` is sized to what
is left. ⚠️ **Zero is a value it writes, not a case it skips** — the same rule
`sync_left_inset` carries in the desktop, for the same reason.

*Revert cost: one field on `FrameLayout` and the three measures that read it.*

### R5 — `iridium <dir>` roots the explorer there and shows it

This is the case Tom named, so it shapes the rest rather than being bolted on:
the terminal editor opened on a directory comes up **with the tree already
there**, not with an empty buffer and a chord to discover. The desktop answered
the same question in #101 and the answer is already factored —
`project::chosen_root` and `project::explorer_root` are pure functions over
`(active file, cwd, home)`, and #57 proved the split was what made the `/`-root
defect testable at all.

⚠️ **Those two functions live in `apps/iridium-desktop/src/project.rs` and must
move with the oil crate**, or the terminal face will grow a second answer to
"where is this session" that is free to disagree the moment either moves.

*Revert cost: the CLI branch.*

### R6 — ⛔ not in part D, named so it is not silently dropped

- **Mouse.** `geometry.rs` says hit testing "will need" the text area. Wiring a
  click in the tree is additive on a working keyboard-driven panel.
- **Persistence.** Still #59/#102/#105's, exactly as R4 of #112c left it.
- **#113's modality.** The terminal face's modal-by-default question is its own
  task and must not be decided as a side effect of this one.

---

## The trap

⚠️ **The hoist is where this task can go wrong, and the failure is silent.**
`file_tree/` is 7,453 lines with 5 test modules of its own
(`apply_tests`, `buffer_tests`, `confirm_tests`, `edit_keys_tests`,
`mode_tests`, `plan_tests`) plus `file_tree/tests/`. A hoist that moves the code
and leaves the tests behind — or moves them and lets `cargo test` stop running
them because nothing declares the new crate — would produce a green run over
code nobody is checking any more.

**The control step, before any of it:** count the file-tree tests that run
today, and require the same count after. A number taken *before* the move is
the only oracle that survives the move.

---

## Build order

Each step is checkable on its own, and the first two are strictly refactor.

1. ✅ **The census — DONE, 12 Aug 2026.** `cargo test -p iridium-desktop --lib
   file_tree`: **198 passed, 0 failed**, made of

   | module | tests |
   | --- | --- |
   | `file_tree::tests` | 65 |
   | `plan_tests` | 30 |
   | `buffer_tests` | 29 |
   | `edit_keys_tests` | 29 |
   | `mode_tests` | 19 |
   | `apply_tests` | 13 |
   | `confirm_tests` | 13 |

   ⭐ **The per-module breakdown, not just the total, and that is the point.**
   A total can be held level by a suite that stopped running and another that
   grew; the seven numbers cannot. **198 in, 198 out, and each row unchanged.**
2a. ✅ **The cut in `overlay.rs` — DONE `e9ae4245`.** It was 2,324 lines against
   this project's 1,000-line hard limit *and* the file the hoist has to cut
   through: the vocabulary was in the same file as the wgpu render pass, which
   can never leave this face. Split by what a defect **is** —
   `metrics` 236, `content` 246, `geometry` 360, `paint` 785, `tests` 664,
   `mod` 97 — every file now under the 800 target. **`content.rs` is the piece
   that moves**, and it has no wgpu, no window and no device in it, which is
   why the cut is there rather than at a convenient line number.

   ⚠️ **A slice-one hoist of "the modules that import nothing" is not
   available, and that was measured.** `buffer.rs` imports
   `super::panel::FileExplorer`, and `panel.rs` is the central type every other
   module hangs off. Only `plan`, `apply`, `order` and `filter` are genuinely
   free-standing — 43 of the 198 tests — so the explorer moves **as one piece**
   or not at all. Better to know that before starting than halfway through.

2b. ✅ **The vocabulary — DONE.** `crates/iridium-panel` exists and holds
   `row` (`Span`, `PanelRow`, `PanelCaret`, `PanelFit`, and the two
   visible-row ceilings) and `line` (`LineBuilder`, `highlighted_spans`,
   `match_color`, `skip_chars`). It depends on `iridium-editor` with default
   features **off** and on `unicode-segmentation`, and on nothing else — no
   renderer, no device, no window, which is the property that lets the terminal
   face depend on it.

   `PanelAnchor`, `PanelContent` and `StripContent` stayed behind, because an
   anchor is a statement about a window. The desktop's `overlay` re-exports the
   moved names and `crate::line` is now an eight-line re-export, so all eight
   consumers still read exactly as they did.

   **The census, both directions:** file-tree 198 across all seven modules,
   unchanged. Desktop lib **529 → 525**, and the new crate has **4** — `line.rs`
   carried a test module and it moved with its code. 525 + 4 = 529, and the
   `test/workspace` gate runs both. Ten gates green.

   ⚠️ **What the census means from here on.** When the explorer follows, the
   desktop's count *will* drop by roughly 198 and the panel crate's will rise by
   the same. A seat reading only the desktop number would see that as loss. The
   invariant is the **sum across the workspace**, and the per-module rows are
   what say the sum was preserved for the right reason.

2c. **Hoist the explorer** — `explorer`
   (the tree, filter, keys, oil buffer, edit keys, plan, confirm, apply) plus
   `project`'s two root functions. `PanelAnchor`, `PanelContent` and the
   painters stay in the desktop; `overlay` re-exports the rest. Desktop
   behaviour must not change: the census is the proof.

   ⭐ **Start with the ten modules that import nothing** — `apply`, `buffer`,
   `filter`, `order`, `plan` and their tests, 85 of the 198 — because they move
   without a single import rewrite, and a hoist proven on them is a hoist whose
   *mechanics* are no longer in question when the coupled nine follow.
3. **The TUI paints it** into a `FloatingBox`, at the *queried* ceiling, reached
   by the same command id the desktop uses. The smallest slice that puts a file
   tree in a terminal.
4. **The browsed fit** — R3. Full screen height, and the ceiling rides on the
   fit rather than on the screen that composes the rows.
5. **The left band** — R4. `FrameLayout` gives up columns; gutter, `TextArea`
   and `Viewport` follow; zero is written, not skipped.
6. **`iridium <dir>`** — R5. The CLI learns what a directory is, the session
   takes it as its project, and the tree is up when the editor is.

---

## Where this leaves #112

Parts A, B and C are landed and installed. Part D is the last of Tom's
sentence, and step 1 of it is the census — which is the next thing to run.
