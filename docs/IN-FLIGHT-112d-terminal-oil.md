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

2c. **Hoist the explorer — first slice landed 12 Aug 2026.** `plan`, `order`,
   `apply` and the two test files are now `iridium-panel::explorer`, behind a
   `pub(crate) use iridium_panel::explorer::{apply, plan};` in the desktop's
   `file_tree/mod.rs`. **Not one import was rewritten in the nine modules that
   stayed** — they still read `super::plan::…`, which is the proof that the
   hoist moved code and not meaning.

   **Census, both directions:** desktop lib **553 → 510** (−43), panel crate
   **4 → 47** (+43). 30 of those are `plan_tests` and 13 `apply_tests`, which
   is the whole of what moved. Sum 557 either way; `test/workspace` runs both.
   Ten gates green.

   ⚠️ **`buffer` cannot follow yet, and the reason is a rule rather than a
   preference.** `buffer.rs` is an `impl FileExplorer` block, and an inherent
   impl may only be written in the crate that defines the type. It moves when
   `FileExplorer` moves, not before — so the doc's earlier "start with the ten
   that import nothing" was one file optimistic: `buffer` imports nothing from
   the desktop *crate* but is welded to a desktop type.

   `iridium-panel` gained `iridium-file` as a **dev-dependency** only. `apply`
   writes to a real disk and its tests build real directories; a fixture that
   faked one would be testing something else.

   **`filter` followed in the same tick**, and it is the one that cost a
   decision. It reaches `iridium_explorer::{FileTree, NodeId}`, so
   `iridium-panel` now depends on `iridium-explorer` — which is native by
   nature, a browser having no directory to read. ⚠️ **That narrows the panel
   crate to native targets, deliberately.** Both faces that depend on it are
   native; the web face composes its panels in TypeScript and never links it;
   `iridium-bindings` does not depend on `iridium-panel` at all, so the wasm
   gate is untouched. Recorded in the manifest so the day that changes, it
   changes loudly. Census flat — `filter` carries no tests of its own — which
   is the right proof that nothing was lost on the way.

   **`prompt::Entry` followed, and it is a prerequisite rather than a slice.**
   Five of the nine remaining modules import it, so nothing coupled could move
   ahead of it. It is now `iridium-panel::entry`, re-exported from the
   desktop's `prompt` so `crate::prompt::Entry` still names it. The seven verbs
   the palette, the search bar and the explorer already called directly went
   from `pub(crate)` to `pub`; `edit` — the key-named form the prompt drives it
   with — went from private to `pub` for the same reason.

   ⚠️ **This one is *not* census-preserving, deliberately.** Every existing
   test of the field went through `Prompt`, which stayed behind — so the move
   as-such would have put 180 lines of grapheme-cluster caret arithmetic into
   the shared crate with **no test in that crate able to fail on it**.
   `cargo test -p iridium-panel` would have passed with `Entry` broken. Eight
   direct tests were written to close that: the control-character invariant on
   both ways in, cluster-whole backspace and delete, motion reporting whether
   it moved, the column counted in `char`s rather than clusters or bytes,
   insertion at the caret, and `edit` naming the same verbs the panels call by
   hand. Panel crate **47 → 55**; desktop **510**, unchanged, because nothing
   left it. Proven discriminating: removing the control-character guard fails
   exactly two of the eight and nothing else in the workspace.

   **`rows` and `confirm` moved too, and they were not supposed to be
   movable.** The map put both in the coupled nine. They are not: neither is an
   `impl FileExplorer` — they are free functions over `EditedRow`s and
   `PanelRow`s, and every type they touch had already crossed. Their whole
   foreign surface was `crate::overlay::{PanelRow, Span}` and `crate::line`,
   which are the desktop's *re-exports of this crate*, so the move was three
   import lines. `compose.rs` still reads `super::rows::…` and
   `super::confirm::…` and did not change at all.

   ⚠️ **They stop at `PanelRow`s, and that is the seam holding.** Neither
   builds a `PanelContent`, because that carries an anchor and an anchor is a
   statement about a window. Each face wraps the rows in its own placement.

   Census: desktop **508 → 495** (−13), panel **57 → 70** (+13) — the whole of
   `confirm_tests`. Sum 565 either way.

   ⭐ **What this means for the remaining set: it is seven, not nine** —
   `panel`, `buffer`, `keys`, `edit_keys`, `mode`, `session`, `compose`, plus
   `buffer_tests`, `edit_keys_tests`, `mode_tests` and the `tests/` directory.
   ⚠️ The commit that landed this slice says "six" in its message; the
   directory has seven files in it and the directory is right. Counted from
   `ls`, not from memory, which is how the wrong number got written in the
   first place.

   Only `compose` is genuinely hard: it is the one that must stop returning a
   `PanelContent` and start returning rows plus a caret, with the desktop
   adding the anchor. The other six move together with `FileExplorer`, because
   every one of them is either an `impl` on it or holds one.

   Still to come in 2c: the coupled set — `panel`, `buffer`, `keys`,
   `edit_keys`, `mode`, `session`, `confirm`, `rows`, `compose` — plus
   `project`'s two root functions. `PanelAnchor`, `PanelContent` and the
   painters stay behind. ⭐ **`panel` goes first of those, not last**: every
   one of the nine is either an `impl FileExplorer` or imports the type, so
   nothing else in the set can move ahead of it.

   **`project::chosen_root` — ruled and moved.** R1b named it as the single
   member of the explorer's foreign surface the other three panels do not
   share, and `panel.rs` calls it. ⚠️ **Correcting a note I wrote an hour
   earlier in this same file: `chosen_root` reads nothing from the
   environment.** It is `ExplorerRoot { crawl: !is_filesystem_root(path), path }`
   and nothing else. The environment-reading function is its neighbour,
   `explorer_root`, which is a different question wearing the same word.

   ⭐ **Two questions, one word.** *"Somebody walked into this folder — may a
   search read past it?"* is a property of the explorer, answered identically
   in every face. *"Nobody said where to open; where should it be?"* consults
   the active tab, the process's working directory and the home directory —
   and the two faces need not answer it alike. So the **choice** moved to
   `iridium-panel::explorer::root` with its two tests, and the **guess** stayed
   in `project.rs`. `is_filesystem_root` is `pub` from the shared side because
   the guess needs the same predicate, and two spellings of "has no parent"
   would be two places for it to stop being true.

   Census: desktop **510 → 508**, panel **55 → 57**. Sum 565 either way.

   *Revert cost: the two `pub use` lines in `project.rs`.*

   **The scope 2c was written against, unchanged:** the whole explorer — the
   tree, filter, keys, oil buffer, edit keys, plan, confirm, apply — plus
   `project`'s two root functions. `PanelAnchor`, `PanelContent` and the
   painters stay in the desktop; `overlay` re-exports the rest. Desktop
   behaviour must not change: the census is the proof, and it has held once.

   ### ✅ 2c is DONE — `cb2a9208`, `e82f0ef7`, `1d582a65` (13 Aug 2026)

   Landed in three commits, and the split is deliberate: only the middle one
   had to be atomic.

   **`cb2a9208` — `scroll_for` first, alone.** The last thing `compose` reached
   for that could cross without the type. Moved with its three tests, because a
   `const fn` hoisted into a crate where nothing can fail on it is the same
   hazard `Entry` had one slice earlier. Census: panel **70 → 73**, desktop
   **495 → 492**.

   **`e82f0ef7` — the atomic move.** `panel`, `buffer`, `keys`, `edit_keys`,
   `mode`, `session`, `compose`, plus `buffer_tests`, `edit_keys_tests`,
   `mode_tests` and the whole `tests/` directory. One commit because the orphan
   rule makes it one.

   Census: panel **73 → 215**, desktop **492 → 350**. **±142, exactly
   balanced.** No test written, deleted, renamed or skipped — every one that
   ran in the desktop crate runs in the panel crate, against the same real
   directories and the same real reader thread.

   ⭐ **The `compose` problem, priced against the prediction.** This file said
   `compose` was "genuinely hard: it must stop returning a `PanelContent` and
   start returning rows plus a caret, with the desktop adding the anchor."
   That was right about the shape and **wrong about the difficulty**. The
   translation is one struct literal, in a new free function
   `file_tree::content`, and the reason it is that small is that `hovered` was
   *already* set by the face after composition and `content_columns` was
   already just `fit.content_columns` passed through. The anchor was the only
   genuinely face-owned field. The new type is `iridium_panel::PanelBody`.

   Worth keeping: the thinness is the finding. It says the R2 seam was drawn in
   the right place rather than merely somewhere, and that is a claim the hoist
   could only settle by being carried out.

   **A free function, not a method, and not by choice.** The orphan rule
   forbids `impl FileExplorer` in the desktop crate now. Read that as a
   feature: it is structurally impossible for this face to grow explorer
   behaviour the terminal face would never see. `PanelAnchor` does not exist
   over there.

   **`1d582a65` — ⚠️ what the census could not prove.** The anchor is the one
   thing that did *not* move, and it changed shape: `panel_contents` used to
   compose and then overwrite `content.anchor`; it now chooses first and passes
   it in. **Nothing in the repository could tell the difference.** A sidebar
   silently drawing as a floating popover — wrong placement, wrong height,
   document indented past nothing — passed all 351 tests.

   Measured both ways: `explorer_anchor` sabotaged to always return `Top` gives
   **1 failed, 351 passed**, and the one failure is the test added there;
   reverted, **352 passed**. `explorer_anchor` is extracted as a free function
   for the same reason `apply_hover` beside it already was — `panel_contents`
   returns an empty vector without a `Shell`, so a decision made inside it is
   unreachable from a headless test.

   ⭐ **The law this one earns: a census proves nothing about the code that
   did not move.** It balances departures against arrivals, which is exactly
   the right instrument for a hoist and exactly the wrong one for the seam the
   hoist leaves behind. The seam is new code by definition, and new code wants
   a test that fails without it.

   *Revert cost: three commits, all mechanical; `git revert` in reverse order.*

   **Sizes after, against the 800 target / 1,000 hard limit:** the largest file
   in `iridium-panel` is `edit_keys_tests.rs` at 787, then `plan_tests.rs` 768
   and `panel.rs` 573. `apps/iridium-desktop/src/file_tree/mod.rs` is 67 lines
   and holds nothing but the wrapper and the two re-exports.
3. ✅ **The TUI paints it — DONE, `ecb44392` + `bb9d1a13` (13 Aug 2026).**
   `Ctrl+Alt+E` opens a file tree in the terminal. New
   `iridium_tui::frame::file_explorer`: a poll, a paint, and the translation of
   one enum. Neither the command id nor the chord is written in that face —
   both were inherited from the kernel's default keymap.

   ⭐ **Steps 3 and 4 collapsed into one, and that is the hoist paying out.**
   The plan said step 3 would use the *queried* ceiling and step 4 would fix it
   to the browsed one. There was nothing to fix: the TUI asks for
   `PanelFit::popover`, and R3's ruling travels **with the shared crate**. A
   twelve written into the terminal painter would have been that face quietly
   disagreeing with the other one about how tall a file tree should be. The
   remaining part of old step 4 — full *screen* height — was never a fit, it is
   the left band, and that is step 5.

   ### ⚠️ The units seam, ruled — characters versus cells

   `PanelFit::content_columns` counts `char`s, because the GPU face draws where
   every character is one character-width and a shared crate has no business
   knowing what a terminal thinks a column is. A terminal's columns are
   **display cells**, and a CJK name is two per character.

   **RULED: the builder budgets in characters, the face spends in cells and
   clips.** Identical for every ASCII path. Where they diverge, `paint_text`
   advances by display width and refuses to write past the area, so a row too
   wide in cells is cut at the border rather than overrunning it — the contract
   `PanelRow` already stated. Teaching the shared crate about display width
   would apply a *terminal's* width model to the GPU face, where it is false.
   Cost: a directory of wide-character names loses a character or two off its
   longest rows, right-aligned hint first. Nothing corrupts, nothing panics.

   ### ⚠️ A test that could only pass, caught by measuring

   The first guard for that seam was "no row overruns the box". Sabotaging the
   advance to `chars().count()` — **the exact defect it claimed to guard** —
   passed all six tests, because `paint_text` clips to the content area and so
   a mis-advanced run damages content and never the border.

   Replaced with one that reads the cells back: `日本` then `END`, and `END`
   must begin at column four. Sabotaged it fails with `日END`; reverted, 313
   pass. The weak test is kept for what it does cover and relabelled to stop
   claiming more.

   ### Two rulings the wiring forced

   - **Where the terminal opens it.** The shared `chosen_root` answers "may a
     search read past a directory somebody walked into". The *guess* stays
     per-face, and the two faces genuinely guess differently: the GPU face must
     distrust its working directory (Finder hands a bundled app `/`), while a
     terminal's is where a person `cd`-ed to — the most reliable statement of
     intent either face gets. This **vindicates** the 2b/2c split rather than
     straining it.
   - **`Deed` loses `Copy` to carry a path.** ⚠️ Open is a *destructive* verb
     on a one-buffer face — the GPU face answers the same
     `ExplorerOutcome::Open` with a new tab and loses nothing. Asked about like
     a reload. The `Copy`-preserving alternative was a `pending_path` field
     meaningful only while one prompt is open: two states that must agree, and
     a stale one opens the wrong file over unsaved work.

   ### ⚠️ One outcome the terminal cannot honour, and it says so

   `ExplorerOutcome::ToggleSidebar` becomes `ExplorerAction::Report` with a
   sentence for the statusline, not a silent no-op. A key that does nothing is
   indistinguishable from a key that is broken. **The test asserting this must
   be deleted, not edited, when step 5 lands.**

   Censuses: iridium-tui **307 → 313**, apps/iridium **86 → 92**.

   *Revert cost: two commits; the panel module is self-contained and the
   wiring is additive apart from `Deed`.*
5. ✅ **The left band** — R4. **LANDED 13 Aug 2026.** `FrameLayout` gives up
   columns; gutter, `TextArea` and `Viewport` follow; zero is written, not
   skipped.

   **The geometry.** `Chrome::sidebar_columns` in, `FrameLayout::sidebar_columns`
   out. The band comes off the screen *first* and everything on the X axis is
   measured against what is left — sizing the gutter against the full width and
   subtracting afterwards would let a wide gutter and a wide band together claim
   more columns than the screen has. One field and not two (a width and an
   origin that must agree): the band starts at the left edge, so its width *is*
   the gutter's origin, and `FrameLayout::gutter_area()` derives it.

   ⚠️ **The assumed column zero was real.** `gutter::paint` wrote its line
   number at absolute `0` and `blank_row` filled `0..width`. Both now take a
   `GutterArea { origin, width }`. Measured, not assumed: forcing that origin
   back to zero fails `the_gutter_paints_after_the_band_and_not_underneath_it`
   and nothing else.

   **The furniture.** A new `SidebarBox` beside `FloatingBox` in `frame/panel.rs`.
   ⭐ *A floating box is furniture with an outside — four borders, because it
   sits on something. A band has no outside on three of its four edges, so it is
   drawn with one vertical rule on its right.* That also disposes of the corner
   question this face would otherwise inherit: a panel flush at column zero has
   no free corners to round. It stops above the statusline, which describes the
   *document* and keeps the full width.

   Width: 32 columns, never more than half the screen, refused below `MIN_WIDTH`
   — a file tree that leaves the document narrower than itself has inverted
   which one is the point.

   **The seam.** `ExplorerOutcome::ToggleSidebar` now flips an `Anchor` inside
   `FileExplorerPanel` and reports `Handled`; the host learns about it from a
   different answer to `sidebar_columns(columns, rows)`. ⚠️ That is asked **every
   frame** and its answer written **even when zero** — the band is refused on a
   screen too small for one, so the same panel answers 32 then 0 across a resize
   with no key pressed. A sidebar the screen cannot fit falls back to a popover
   *for that frame* without forgetting it was chosen.

   ⚠️ **The kernel's viewport is not the frame's geometry.** `Chrome` reaches
   the geometry every frame, so what is drawn follows the band at once; the
   kernel's viewport is only written by `sync_viewport`. `App::resync_after_band_change`
   calls it when — and only when — the width actually moved, because
   `sync_viewport` reaches `Editor::state_mut`, which discards the keyboard
   handler's transient state.

   ⚠️ **The interaction R4 did not name: the search panel.** The explorer is
   checked before search in `handle_key`, so a search panel cannot be opened
   while the tree is up — but it can already *be* up when the tree opens, and a
   band running to the bottom of the screen paints over its left thirty columns.
   Ruled: **the band spans the document's rows, not the screen's.** The panel
   already takes its rows *from the document*, so it is a peer of the band and
   not something the band sits beside; and the statusline describes the document
   and keeps the full width.

   The number is `frame::document_rows(rows, search_open)` — factored out of
   `layout` and made public rather than recomputed in the host, because a second
   copy of that arithmetic in `apps/iridium` would be wrong the first time the
   search panel changed height. The host passes the same value to
   `sidebar_columns` and to `paint`, which is what keeps the columns given up and
   the columns drawn in from disagreeing.

   The old `the_sidebar_request_is_reported_rather_than_swallowed` test was
   **deleted**, as it was written to be. Five replaced it, plus four on the
   geometry. Census: iridium-tui **313 → 321**.

   **Three sabotages, each measured rather than claimed:**

   | sabotage | what failed |
   |---|---|
   | `GutterArea::origin` forced to `0` | `the_gutter_paints_after_the_band_and_not_underneath_it`, alone |
   | `Placement::close`'s leftover-row loop dropped | `every_row_the_band_reserved_is_painted_even_past_the_last_file` + the flush-edge test |
   | `SidebarBox::fitted(columns, rows)` instead of `band_rows` | `a_band_stops_at_the_rows_it_was_given…` + the flush-edge test |
6. **`iridium <dir>`** — R5. The CLI learns what a directory is, the session
   takes it as its project, and the tree is up when the editor is.

---

## Where this leaves #112

Parts A, B and C are landed and installed. Part D is the last of Tom's
sentence, and **steps 1, 2a, 2b, 2c, 3 and 4 are landed** — the file explorer
is face-independent, whole, in `iridium-panel`, and `Ctrl+Alt+E` puts it on
screen in the terminal. What is left is **step 5** (the left band in
`FrameLayout`, so the panel can be a sidebar rather than a popover) and
**step 6** (`iridium <dir>`).

⛔ **Not installed.** `bundle/install.sh` refuses while `iridium-desktop` runs,
by design, and pid 69873 is still up. Nothing here is on Tom's machine yet.
