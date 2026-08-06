# Session state — 6 Aug 2026

## 🛑 THE PLAN FILE IS STALE — DO NOT QUOTE IT AS FACT

`~/.claude/plans/immutable-stargazing-moler.md` describes work that **has since
landed**. I read it as current and told Tom, as a present-tense defect, that
the syntax side owned two parsers and that the editor's tree went stale after
every edit. **Both were fixed long ago.** Corrected to him on 6 Aug.

What is actually true, verified in the code:

- **One parser, one retained tree** — `iridium_syntax::SyntaxTree`, with
  `edit`, `reparse` and `changed_ranges`.
- `Highlighter::spans_in(&self, tree, source)` **borrows** it. There is no
  `folding.rs`; folds run off the same tree.
- `EditorState::syntax` is a `SyntaxState` with `note_edit` / `sync` /
  `SyntaxDelta`. Typing does not parse. If an edit ever misses `note_edit`,
  the revisions disagree and it parses whole rather than returning a wrong
  tree.
- The whole `ast.*` verb set exists — `expand`/`shrink` with a stack that
  survives cursor merging, sibling and child walks, node-boundary motions,
  text objects, multi-cursor from structure — with `navigate.rs` holding the
  pure `Node → Node` walks.
- The TypeScript `onHostCommand` / `onPendingKeySequence` bug is **also
  fixed**; both are assigned in the constructor.

**The rule this cost:** a planning document records what was true when it was
written. Check the code before repeating any claim from one, especially a
claim that something is broken.


## ✅ WHERE THINGS ARE — READ THIS FIRST

**The tab strip is drawn and it works**, and **the compositor now reserves
space beside the document as well as above it** — the kernel half of the
sidebar, plus the X-axis audit the gutter bug demanded before it.

Everything below is committed, pushed, and green on all six gates.

| commit | what |
|---|---|
| `fd42466` | the window's size reaches every tab (kernel bug) |
| `f75b6a9` | Desktop A — `DesktopApp` holds a `Workspace<DesktopDocument>` |
| `19527a9` | Desktop B — the five workspace commands, additive open |
| `8be2849` | one top inset the painter and the hit test both measure from |
| `68ffca4` | the gutter measures from the top inset too (kernel bug) |
| `80caf9b` | Desktop C — the tab strip is drawn, and clicking it works |
| `710cf7f` | **the compositor reserves space beside the document** |
| `a6d28d6` | the fuzzy matcher leaves the command palette for `crate::fuzzy` |
| `cd033b6` | **`compositor.rs` split into thirteen files, none over 450** |
| `4c68943` | **`app.rs` split into nineteen files, none over 462** |

---

## 🧱 THE COMPOSITOR SPLIT — landed 6 Aug, `cd033b6`

`compositor.rs` was 2,289 lines against this project's own 500-line bar. It
is now `render/compositor/` — thirteen files, largest 450, `mod.rs`
declarations only.

**The seam is *when the code runs*, not what it touches.** That choice is
the whole point: both real bugs of the strip and inset work were in this
file and both were the same shape — a per-frame painter and a
between-frames query each computing one number, agreeing by coincidence.
Neither was visible while both copies sat two thousand lines apart.

| file | lines | what |
|---|---|---|
| `state.rs` | 290 | the fields, documented once, and `new` |
| `target.rs` / `highlight.rs` | 33 / 91 | the two seams a face plugs into |
| `metrics.rs` / `shape.rs` | 61 / 138 | per-frame value types; the cache key |
| `frame.rs` | 450 | `compose`, the uniform sync, the render pass |
| `shaping.rs` | 328 | content extraction, fills, rebuild, wrap readback |
| `gutter_column.rs` | 187 | gutter width ×2, its text, its change bars |
| `quads.rs` | 258 | line backgrounds, selection, carets |
| `placement.rs` | 311 | between-frames queries + the readback accessors |
| `insets.rs` / `settings.rs` | 80 / 168 | what a face pushes in between frames |

**`gutter_column.rs` is the exercise paying off.** `gutter_width` (what a
between-frames query measures from) and `frame_gutter_width` (what the
painter measures from) now sit eight lines apart. They *differ on purpose* —
only the painter consults custom gutter text — and that difference is now
reviewable in one screen instead of inferable across 900 lines.
`placement.rs` does the same for `content_left_edge_past_gutter`, the one
place the inset and the padding are summed.

**How it was proved to be a pure refactor**, because "the tests pass" is not
enough for a move this large:

1. Every non-comment, non-import line diffed against the original,
   order-independent (`grep -vE '^\s*(//|$|use |mod |pub use )' | sort`).
   The *only* differences: the `pub(super)` prefixes siblings need, seven
   `impl FrameCompositor` headers and their braces, and two rustfmt reflows.
   **No statement added, dropped or altered.**
2. The forty public item names extracted from both and diffed — identical.
   This is the check that catches a `pub` quietly becoming private, which
   compiles fine inside the crate and breaks a face.
3. All six gates: 2,088 tests, zero failures — including the eight
   left-inset and seven top-inset pixel readbacks, and the whole
   retained-shaping suite, which is what actually pins the cache logic that
   moved between files.

**Fields are `pub(super)`, not private.** `FrameCompositor` is one unit of
state whose *methods* split by phase; it is not a type with a boundary
through its middle. Anyone tempted to "tighten" this should note that
narrowing the fields means re-introducing accessors that exist only to let
the painter read what it already owns.

---

## 🧱 THE APP SPLIT — landed 6 Aug, `4c68943`

`apps/iridium-desktop/src/app.rs` was 3,270 lines. It is now
`apps/iridium-desktop/src/app/` — **nineteen files, largest 462**.

**The seam is *which input a method answers*.** That is how a defect in this
file presents itself: "the right-click menu does the wrong thing", "the wheel
scrolls past the end", "the title says the wrong file". Each of those
sentences now names one file.

| file | lines | what |
|---|---|---|
| `mod.rs` | 103 | declarations and the map of where the pieces live |
| `state.rs` | 258 | `DesktopApp`, `DesktopDocument`, `Flow`, the small readers |
| `startup.rs` | 247 | `Options`, `StartupError`, the `Shell`, `new` |
| `handler.rs` | 135 | the winit seam — routing only, no decisions |
| `keyboard.rs` | 272 | a key press and the modal ladder it descends |
| `pointer.rs` | 270 | motion, press, release, wheel, hit test |
| `menu.rs` | 130 | the context menu: open, steer, spend a click |
| `host_commands.rs` | 96 | the verbs the kernel hands back for the face to run |
| `tabs.rs` | 173 | opening, closing, walking tabs; the strip's content |
| `files.rs` | 187 | save, save-as, drop, open, quit |
| `clipboard.rs` | 99 | the system clipboard, and what to say when it refuses |
| `viewport.rs` | 223 | scroll, its clamp, and the window geometry both need |
| `paint.rs` | 177 | composing and presenting one frame |
| `title.rs` | 40 | what the window is called |
| `tests/` | 1,141 | `support` · `saving` 168 · `panels` 462 · `tabs` 402 |

`app` is a **public** module (unlike `compositor`), so the nine-item public
surface is what `run.rs` and `lib.rs` already used. It is unchanged.

**Proved pure the same two ways as the compositor**, plus a third specific to
moving tests:

1. Order-independent non-comment, non-import line diff. Every difference is a
   `pub(super)` prefix with an exact unprefixed counterpart, one of eleven
   added `impl DesktopApp {` headers with its brace, a wrapped-import
   continuation line, or the `FONT` path going one directory deeper.
2. Public item names, old against new: identical, nine for nine.
3. **Every comment line in the old `mod tests` block, diffed against the new
   files.** The line-level diff strips comments, so it cannot see a lost doc
   comment — and three *had* been dropped by an off-by-one in the extracted
   ranges. This check found them. **Do this on any split that moves tests.**

## 🧭 THE LEFT INSET — WHAT LANDED AND WHY

`FrameCompositor::set_left_inset(width)` is the horizontal twin of
`set_top_inset`. The face passes the width of chrome it draws down the left;
the gutter's background, its numbers, the change bars, the content column and
both directions of hit-testing all follow.

**Asymmetric on purpose.** `set_top_inset` takes *chrome height plus*
`DOCUMENT_TOP_PADDING`, because the document's vertical breathing room is on
the same side as the chrome. `set_left_inset` takes the chrome width **alone**
— the horizontal breathing room lives on the *far* side of the gutter, so it
is not the face's to account for.

### The audit — every X-axis measure in `compositor.rs`

**Nine were anchored to `content_offset_x`** and follow for free: the content
text origin, `blame_left`, the line-background quads (x and width), the
selection quads, the cursor quads, and both main hit-test paths.

**Six measured from the *window's* left edge and did not.** All six now
measure from the reserved edge:

1. the gutter background quad (`Quad::new(0.0, …)`)
2. the line numbers' origin (a hardcoded `8.0`, now `GUTTER_TEXT_PADDING`)
3. their clip bound's `left: 0`
4. their clip bound's `right: gutter_width` (measured from zero)
5. the change indicator bars (`2.0`/`3.0`, now named constants)
6. **the content text area's `left: 0`** — the exact analogue of the `top: 0`
   that put every line number one row out from the line it named

### Proxies collapsed

- `content_left_edge_past_gutter(gutter_width)` is now **the one place** the
  inset and the padding are added. The painter and the between-frames queries
  can now differ only in the *gutter width* — which they measure deliberately
  differently (`frame_gutter_width` consults custom gutter text,
  `gutter_width` does not), and which says so where they do.
- `content_left_edge(line_count)` is the public form, and answers before a
  frame has been composed — which `content_offset_x()` cannot.
- `wasm.rs` was rebuilding the sum by hand as "gutter width plus a hardcoded
  ten", and returning a hardcoded ten from `getTextOffsetY`. Both now ask the
  compositor.
- **`mouse.rs::cell_pixel` was checked and deliberately left alone.** It
  synthesizes a coordinate on the handler's idealized grid, never a screen
  one, so the inset is correctly none of its business.

### The live bug this found

**The inline blame ghost text was clipped at the window top, not at the
band.** Blame sits on the caret's line and the caret can be scrolled up
behind reserved chrome, so it would have been drawn *inside* the tab strip.
Latent only because blame is populated solely from the web face, which draws
no chrome above the document — it would have started showing the day the
TypeScript tab strip (#45) landed. Test proven red first: *"the blame ghost
text drew at (298, 34), inside the 44-pixel band."*

---

## ⚠️ TWO TESTING TRAPS WORTH KEEPING — both hit today

**1. A translation test cannot see an absolute error.**
`the_content_column_is_the_same_pixels_a_sidebars_width_across` compares the
inset frame against the plain one. It cannot see the content's clip bound at
all: **both frames clip relative to their own column**, so a uniformly wrong
bound shaves both alike and the translation survives. Verified — a
twelve-pixel over-tight bound passes it untouched. Two wrongs agreeing, which
is the whole subject of the file it lives in.

**2. Quads are not clipped by a text area's bounds.**
The absolute test that replaced it nearly repeated the mistake. Scanning the
full frame height it found the **caret** sitting exactly at the content
column's left edge and reported that as the text — a caret is geometry, and
`TextBounds` does not touch geometry. A frame whose text had been clipped away
entirely still had ink precisely where the test looked. It reads **one line's
rows, clear of the caret** now, and catches the twelve pixels.

The same fact is why `the_blame_ghost_text_stays_out_of_the_reserved_band`
compares two frames rather than asserting the band is empty: the band is *not*
empty — the caret and the line-background quads are drawn into it whenever the
caret's line is scrolled behind the chrome. **The desktop face gets away with
this only because the overlay paints the strip opaquely afterwards.** That is
a real observation, recorded as **#51**, not fixed.

---

## ⏭ WHAT I PUT TO TOM AND HE HAS NOT YET ANSWERED (6 Aug, ~07:20)

He said: *"happy to do what you got to do… I want everything built on as
stable a foundation as we can possibly get it built."* Since the syntax
foundation turned out to be done already, I offered him three, **measured
rather than remembered**:

1. **Split the oversized files.** ~~`app.rs` 3,270~~ *(done, `4c68943`)* ·
   `wasm.rs` 3,154 · `core.rs` 2,807 · ~~`compositor.rs` 2,289~~ *(done,
   `cd033b6`)* · `overlay.rs` 1,768, against
   CLAUDE.md's **500**. The argument that makes this foundation work rather
   than tidying: *both* real bugs found this session were in
   `compositor.rs`, and both were the same shape — two places computing one
   number, agreeing by coincidence. A 2,300-line file is what stops anyone
   seeing both copies at once. Recommended order: `compositor.rs` (proven
   history of hiding exactly this), then `app.rs`, then `wasm.rs`.
2. **Fix #51** — the quads drawn inside reserved chrome.
3. **Back to the Oil popover.**

**Absent a reply, start (1) on `compositor.rs`.** It matches his stated
priority, it is low-risk because the tests already pin the behaviour, and it
is wanted whichever of the three he picks. Candidate seams, from reading it:
`compose()`; the retained-shaping gate (`ShapeKey`, `RetainedShape`,
`rebuild_retained`); the four quad builders; the placement queries and the
insets; the setters and accessors.

## ▶️ THEN: the Oil popover — TOM CHANGED DIRECTION, 6 Aug

**The drawn sidebar is no longer the next piece.** Tom asked for an
**oil.nvim**-style file navigator instead, as a command-palette-style popover,
fused with fuzzy filtering and regex. Confirmed explicitly — he named
oil.nvim.

**This means `set_left_inset` has no caller yet.** It is correct, tested and
still what any future drawn sidebar needs, and the audit that came with it
caught a live bug — but do not expect to use it soon. Said plainly to Tom.

### What already exists (verified, not assumed)

- **`crates/iridium-tree`** — full face-agnostic tree behaviour: expansion
  state, virtualised row projection, keyboard navigation, selection that
  survives collapsing a parent. A face supplies a `TreeSource` and renders
  `Row`s. **Nothing else needs writing for the tree half.**
- **`crate::fuzzy`** — the matcher, extracted from the command palette this
  session (`a6d28d6`) so files and commands rank through the same code.
  `/` and `\` already count as word starts, which is what a path needs.
- **regex** — a workspace dependency, wired into `search::find` with
  `validate_regex` and `escape_regex` already public.
- The command palette exists in kernel, bindings, desktop **and TUI**.

### The design — as corrected by Tom, 6 Aug

**I proposed read-only-while-filtered and Tom overruled it**, correctly: bulk
actions on a filtered set of names are exactly what he wants. The resolution:

1. **Editable always, filtered or not.** The buffer applies changes by diffing
   its current text against **what it showed when it loaded**. With a filter
   active that "before" state is *the filtered set*, so anything the filter
   hid was never in the comparison and cannot be touched. The ambiguity I
   worried about is removed by scoping the diff, not by disabling editing.
2. **Rows must carry stable ids, not be matched by name.** THE trap. Diffing
   by filename makes a rename read as delete-plus-create — which for a large
   file destroys the contents and rewrites them instead of moving it. A
   rename must resolve to a rename.
3. **Changing the filter while dirty is a decision point** — the one case
   where the scoping argument breaks, since the "before" set shifts under the
   pending edit. Treat the filter as part of the buffer's identity: changing
   it prompts apply-or-discard, as switching files would.
4. **A confirmation listing the actual operations** before anything touches
   disk (`4 renames, 2 deletes, 1 create`, by name). This is what makes bulk
   editing feel safe, and it makes the filtered case self-evidently correct —
   nothing hidden appears in the list.
5. **Filtering keeps the hierarchy** — hide non-matching rows but keep the
   directories leading to matches, still indented. Not a flat ranked list.
   The flat alternative was offered as a genuine fork; no ruling yet.
6. **A sigil, not a mode key**: plain text is fuzzy, a leading `/` is regex.

### Build order proposed

Filesystem `TreeSource` → the popover showing it → fuzzy filtering → regex →
**the editable-buffer half last**, since it is the part that touches the disk.

### Deliberately not decided

- Whether Enter on a directory descends in the popover or opens an Oil buffer
  in a tab.
- Whether this replaces `⌘O` or sits beside it.

Both wait until something is on screen to react to.

## ▶️ WHAT IS LEFT ON THE STRIP (small, none of it blocking)

- **The close control's alpha is 0.40**, nearly invisible on an inactive tab.
  Deliberate; Tom's to rule on. **Asked 6 Aug.**
- **The wheel over the strip scrolls the document.** VS Code scrolls the
  strip. Not a defect; a choice nobody has made. **Asked 6 Aug.**
- No drag-to-reorder, no middle-click-to-close, no hover state.
- No context menu on the strip — a menu of *tab* verbs is a separate design.

---

## ⚠️ THINGS I GOT WRONG — all told to Tom

1. **I rebuilt a binary a live session was running from.** `pgrep -x
   iridium-desktop`, **before** the build, not after.
2. **Never restore a file with `mv` from a backup.** `cp x backup` then `mv
   backup x` gives the file the *backup's* mtime, older than whatever cargo
   built in between — cargo then sees "unchanged" and reuses a stale build.
   Use `cp backup x`, or `touch` after.
3. **I committed a test without its fix.** The fix shared a file with other
   work, and `git add -i` is unavailable, so the first commit would have been
   red standing alone. Amended into one honest commit. **Stage the fix and its
   test together, or not at all.**

## 📦 INSTALLING

Use **`apps/iridium-desktop/bundle/install.sh`**, not a hand-rolled sequence.
It already refuses while any `iridium-desktop` runs (`pgrep -x`, never `-f`),
rebuilds through `bundle.sh`, copies with `ditto` so the signature survives,
swaps only once the new copy is fully written, and verifies **what landed**
rather than what was staged.

## Standing rules (unchanged, non-negotiable)

- **Never** `git stash`, `git checkout --`, `git restore`, `git reset`,
  `git clean`, `git worktree` in this checkout. Agent briefs must name each
  banned command individually.
- **Never** ports 3000, 3030, 8000, 8080. **VITE BAN** — run-once builds
  only, `frame` serves, never `npm run dev`.
- A measured figure leaves this seat **only if the command producing it is in
  the same message's tool output**. **Never pipe or compound a command whose
  exit status you intend to believe** — redirect to a file, then `echo
  exit=$?` as its own statement.
- `pgrep -x`, never `-f`. Never kill pids 99844 or 31161. The no-swap rule
  rides Tom's **live session as a process**, not a pid number.
- git is wired to difftastic — `git diff --no-ext-diff`.
- No `unwrap()` / `expect()` / `panic!` / `todo!()` / `unimplemented!()` /
  `dbg!()` outside `#[cfg(test)]`. All public items documented. **Red test
  first for every bug fix**, proven to fail against the unfixed code.
- Files under 500 lines; `mod.rs` carries declarations only.
- ⚠️ **Never justify a design by citing Tom's examples.** He told me on 6 Aug
  to drop JSON/JSONL from design talk entirely — he mentioned that work once,
  I kept quoting it back as rationale, and it read as hyper-specialising the
  editor around one case. Describe what a feature does *structurally*, never
  who it is for or what format prompted it. Second instance of this
  correction; the first was framing syntax navigation around expand/shrink
  because he had named it. Treat any urge to write "perfect for your X work"
  as the tell.
- ⚠️ **Anything meant for Tom leaves through the Meridian `send` tool, or it
  did not happen.** Tom `dm:c9255b2a-5731-4d17-8124-e3bfa2224186`. This seat
  is "Doug".
- `du -sk` on the lane tree at lane open and lane close.

## The six gates

Run `cargo fmt --all` first. Then, each unpiped, each exit status checked on
its own:

```
cargo test --workspace --all-features
cargo test -p iridium-editor --no-default-features
cargo test -p iridium-editor --no-default-features --features syntax
cargo check -p iridium-bindings --no-default-features --features web --target wasm32-unknown-unknown
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo fmt --all --check
```

The screenshot harness, when a frame needs judging by eye:

```
IRIDIUM_CHROME_SHOT_DIR=<dir> cargo test -p iridium-desktop \
    --test chrome_screenshots -- --ignored
```

**The lesson that keeps earning its keep:** *the proxy law is not satisfied by
testing the queries. Render it and look at it* — and then check that what you
looked at was the thing you meant.

---

## Open questions and known gaps

- **Awaiting Tom**: should a new file open into the current group or always at
  the top level? Defaulting to the current group — one line to change
  (`open_file` in `app.rs`, the `parent` binding).
- **Awaiting Tom (asked 6 Aug)**: the inactive close control's alpha, and
  whether the wheel over the strip should scroll the strip.
- **#51** the caret and line-background *quads* are drawn inside a reserved
  band when the caret's line scrolls behind chrome. `TextBounds` does not clip
  geometry. Masked today only because the desktop overlay paints the strip
  opaquely afterwards.
- **Oversized files still to split**, worst first: `wasm.rs` (3,154),
  `core.rs` (2,807), `overlay.rs` (1,768, wants its colour derivations in a
  `chrome.rs`) and `workspace/model.rs` (578, wants attach/detach/subtree in
  a `tree.rs`). `compositor.rs` (`cd033b6`) and `app.rs` (`4c68943`) are
  done. **Those two are the worked examples to copy**: pick the seam by the
  axis defects appear along, keep fields `pub(super)`, and prove purity with
  the diffs described above rather than trusting the suite alone.
- **#39** kernel has no `clear_language` — an unknown-extension file inherits
  the previous one's highlighting. Only bites `save_as` now.
- **#42/#43/#44/#45** the whole web-face half of tabs, untouched.
- **#40** five `f64 as usize` casts on JS line numbers in `wasm.rs`.
- **#35** HiDPI font scale not re-applied across displays.
- **#31** light/dark theme switch.
